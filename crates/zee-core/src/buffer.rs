use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::ops::Range;
use anyhow::Result;
use ropey::Rope;
use crate::{Encoding, LineEnding};
use encoding_rs::*;

#[derive(Debug, Clone)]
pub struct EditDelta {
    pub char_range: Range<usize>,
    pub old_text: String,
    pub new_text: String,
    pub line_range: Range<usize>,
}

pub struct Editor {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub encoding: Encoding,
    pub line_ending: LineEnding,
    pub read_only: bool,
    pub vi_mode: crate::ViMode,
    
    pub undo_stack: Vec<EditDelta>,
    pub redo_stack: Vec<EditDelta>,
    pub saved_undo_len: usize,
    pub modified_since_save: bool,

    pub cursor: usize, // char index
    pub selection_anchor: Option<usize>,
    pub selection: Option<Range<usize>>, // char index range
    pub scroll_row: usize,
    pub scroll_vrow: usize, // Visual row offset within the logical line
    pub scroll_col: usize,

    pub find_results: Vec<crate::search::Match>,
    pub current_match_idx: Option<usize>,
    pub search_status: Option<String>,

    pub syntax_highlighter: Option<crate::syntax::SyntaxHighlighter>,
    pub line_states: Vec<crate::syntax::LineState>,
    pub line_tokens: Vec<Option<Vec<crate::syntax::TokenSpan>>>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            encoding: Encoding::Utf8,
            line_ending: LineEnding::Lf,
            read_only: false,
            vi_mode: crate::ViMode::Insert,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            saved_undo_len: 0,
            modified_since_save: false,
            cursor: 0,
            selection_anchor: None,
            selection: None,
            scroll_row: 0,
            scroll_vrow: 0,
            scroll_col: 0,
            find_results: Vec::new(),
            current_match_idx: None,
            search_status: None,
            syntax_highlighter: None,
            line_states: vec![crate::syntax::LineState::Normal],
            line_tokens: vec![None],
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let (encoding, content) = Self::decode_bytes(&bytes);

        let line_ending = if content.contains("\r\n") {
            LineEnding::Crlf
        } else if content.contains('\r') {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        };

        let line_count = content.lines().count().max(1);
        let mut editor = Self {
            rope: Rope::from_str(&content),
            path: Some(path.clone()),
            encoding,
            line_ending,
            read_only: false,
            vi_mode: crate::ViMode::Insert,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            saved_undo_len: 0,
            modified_since_save: false,
            cursor: 0,
            selection_anchor: None,
            selection: None,
            scroll_row: 0,
            scroll_vrow: 0,
            scroll_col: 0,
            find_results: Vec::new(),
            current_match_idx: None,
            search_status: None,
            syntax_highlighter: None,
            line_states: vec![crate::syntax::LineState::Normal; line_count],
            line_tokens: vec![None; line_count],
        };

        let first_line = if editor.rope.len_lines() > 0 {
            Some(editor.rope.line(0).to_string())
        } else {
            None
        };
        if let Some(highlighter) = Self::detect_syntax_with_content(&path, first_line.as_deref()) {
            editor.update_syntax(Some(highlighter));
        }

        Ok(editor)
    }

    pub fn reload_from_disk(&mut self) -> Result<()> {
        let path = self.path.clone().ok_or_else(|| anyhow::anyhow!("Buffer has no associated file path"))?;
        let mut file = File::open(&path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let (encoding, content) = Self::decode_bytes(&bytes);

        let line_ending = if content.contains("\r\n") {
            LineEnding::Crlf
        } else if content.contains('\r') {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        };

        self.rope = Rope::from_str(&content);
        self.encoding = encoding;
        self.line_ending = line_ending;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.saved_undo_len = 0;
        self.modified_since_save = false;
        self.cursor = self.cursor.min(self.rope.len_chars());
        self.selection_anchor = None;
        self.selection = None;

        let line_count = self.line_count();
        self.line_states = vec![crate::syntax::LineState::Normal; line_count];
        self.line_tokens = vec![None; line_count];
        self.update_line_states(0, line_count);

        if self.syntax_highlighter.is_none() {
            let first_line = if self.rope.len_lines() > 0 {
                Some(self.rope.line(0).to_string())
            } else {
                None
            };
            if let Some(highlighter) = Self::detect_syntax_with_content(&path, first_line.as_deref()) {
                self.update_syntax(Some(highlighter));
            }
        }

        Ok(())
    }

    pub fn detect_syntax(path: &Path) -> Option<crate::syntax::SyntaxHighlighter> {
        Self::detect_syntax_with_content(path, None)
    }

    pub fn detect_syntax_with_content(path: &Path, content: Option<&str>) -> Option<crate::syntax::SyntaxHighlighter> {
        let syntax_defs = crate::syntax::SyntaxDefinition::builtins();

        // 1. Check file extension (e.g. .sh, .bash, .py, .rs)
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(def) = syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))) {
                if let Ok(hl) = crate::syntax::SyntaxHighlighter::new(def.clone()) {
                    return Some(hl);
                }
            }
        }

        // 2. Check filename without leading dot (e.g. .bashrc -> bashrc, .zshrc -> zshrc)
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            let clean_name = file_name.trim_start_matches('.');
            if let Some(def) = syntax_defs.iter().find(|s| s.meta.extensions.iter().any(|e| e.eq_ignore_ascii_case(clean_name))) {
                if let Ok(hl) = crate::syntax::SyntaxHighlighter::new(def.clone()) {
                    return Some(hl);
                }
            }
        }

        // 3. Check Shebang from content first line
        if let Some(first_line) = content.and_then(|c| c.lines().next()) {
            if first_line.starts_with("#!") {
                let lower = first_line.to_lowercase();
                let matched_name = if lower.contains("bash") || lower.contains("/sh") || lower.contains("zsh") {
                    Some("Shell")
                } else if lower.contains("python") {
                    Some("Python")
                } else if lower.contains("node") || lower.contains("deno") || lower.contains("bun") {
                    Some("JavaScript")
                } else {
                    None
                };

                if let Some(name) = matched_name {
                    if let Some(def) = syntax_defs.iter().find(|s| s.meta.name.eq_ignore_ascii_case(name)) {
                        if let Ok(hl) = crate::syntax::SyntaxHighlighter::new(def.clone()) {
                            return Some(hl);
                        }
                    }
                }
            }
        }

        None
    }

    pub fn update_syntax(&mut self, highlighter: Option<crate::syntax::SyntaxHighlighter>) {
        self.syntax_highlighter = highlighter;
        self.line_tokens.fill(None);
        self.update_line_states(0, self.rope.len_lines());
    }

    pub fn highlight_line(&mut self, line_idx: usize) -> Vec<crate::syntax::TokenSpan> {
        if line_idx >= self.line_count() {
            return vec![];
        }
        if let Some(tokens) = self.line_tokens.get(line_idx).and_then(|t| t.as_ref()) {
            return tokens.clone();
        }

        if let Some(ref highlighter) = self.syntax_highlighter {
            let state = self.line_states[line_idx];
            let text = self.rope.line(line_idx).to_string();
            let tokens = highlighter.highlight_line(&text, state);
            if line_idx < self.line_tokens.len() {
                self.line_tokens[line_idx] = Some(tokens.clone());
            }
            tokens
        } else {
            vec![]
        }
    }

    fn update_line_states(&mut self, from_line: usize, dirty_to_line: usize) {
        let line_count = self.rope.len_lines();
        if self.line_states.len() != line_count {
            self.line_states.resize(line_count, crate::syntax::LineState::Normal);
        }
        if self.line_tokens.len() != line_count {
            self.line_tokens.resize(line_count, None);
        }

        let mut affected_range = from_line..dirty_to_line;

        if let Some(ref highlighter) = self.syntax_highlighter {
            let mut current_state = if from_line == 0 {
                crate::syntax::LineState::Normal
            } else {
                // If we're starting from a middle line, we use the state that was at the START of that line.
                // But wait, if the line before it changed its NEXT state, we should have started from there.
                self.line_states[from_line]
            };

            for i in from_line..line_count {
                let old_state = self.line_states[i];
                self.line_states[i] = current_state;

                let text = self.rope.line(i).to_string();
                let next_state = highlighter.next_line_state(&text, current_state);
                
                if i >= dirty_to_line && i + 1 < line_count && next_state == self.line_states[i+1] && current_state == old_state {
                    // State stabilized
                    affected_range.end = i + 1;
                    break;
                }

                current_state = next_state;
                if i == line_count - 1 {
                    affected_range.end = line_count;
                }
            }

            // Parallel highlighting for the affected range using rayon
            use rayon::prelude::*;
            let highlighter_ref = highlighter;
            let lines: Vec<String> = (affected_range.start..affected_range.end)
                .map(|i| self.rope.line(i).to_string())
                .collect();
            let states: Vec<crate::syntax::LineState> = (affected_range.start..affected_range.end)
                .map(|i| self.line_states[i])
                .collect();

            let new_tokens: Vec<Vec<crate::syntax::TokenSpan>> = lines.par_iter().zip(states.par_iter())
                .map(|(line, state)| highlighter_ref.highlight_line(line, *state))
                .collect();

            for (i, tokens) in (affected_range.start..affected_range.end).zip(new_tokens) {
                if i < self.line_tokens.len() {
                    self.line_tokens[i] = Some(tokens);
                }
            }
        }
    }

    pub fn compute_line_break_segments(line_str: &str) -> Vec<usize> {
        if line_str.is_empty() {
            return vec![0];
        }
        let mut byte_to_char = vec![0; line_str.len() + 1];
        let mut char_count = 0;
        for (byte_idx, _) in line_str.char_indices() {
            byte_to_char[byte_idx] = char_count;
            char_count += 1;
        }
        byte_to_char[line_str.len()] = char_count;

        let mut breaks: Vec<usize> = unicode_linebreak::linebreaks(line_str)
            .map(|(b, _)| byte_to_char[b])
            .collect();
        if breaks.last() != Some(&char_count) {
            breaks.push(char_count);
        }
        breaks
    }

    #[allow(clippy::single_range_in_vec_init)]
    pub fn wrap_line(&self, line_idx: usize, width: usize, tab_size: usize) -> Vec<Range<usize>> {
        if width == 0 { return vec![0..self.line(line_idx).len_chars()]; }
        let line = self.line(line_idx);

        let mut len_without_newline = line.len_chars();
        while len_without_newline > 0 {
            let c = line.char(len_without_newline - 1);
            if c == '\n' || c == '\r' {
                len_without_newline -= 1;
            } else {
                break;
            }
        }

        if len_without_newline == 0 {
            return vec![0..0];
        }

        let line_str: String = line.chars().take(len_without_newline).collect();
        let breaks = Self::compute_line_break_segments(&line_str);
        let chars: Vec<char> = line_str.chars().collect();

        use unicode_width::UnicodeWidthChar;

        let mut result = Vec::new();
        let mut line_start = 0;
        let mut cur_width = 0;
        let mut seg_start = 0;

        for seg_end in breaks {
            if seg_end <= seg_start {
                continue;
            }

            // Simulate adding segment `seg_start..seg_end` to current line
            let mut simulated_width = cur_width;
            for &c in &chars[seg_start..seg_end] {
                let w = if c == '\t' {
                    tab_size - (simulated_width % tab_size)
                } else {
                    c.width().unwrap_or(0)
                };
                simulated_width += w;
            }

            if simulated_width <= width {
                cur_width = simulated_width;
            } else {
                // If the current line already has content, wrap before this segment
                if seg_start > line_start {
                    result.push(line_start..seg_start);
                    line_start = seg_start;
                    cur_width = 0;
                }

                // Check if segment fits on a fresh line
                let mut fresh_width = 0;
                for &c in &chars[seg_start..seg_end] {
                    let w = if c == '\t' {
                        tab_size - (fresh_width % tab_size)
                    } else {
                        c.width().unwrap_or(0)
                    };
                    fresh_width += w;
                }

                if fresh_width <= width {
                    cur_width = fresh_width;
                } else {
                    // Segment itself is longer than width: fallback to character-by-character wrap
                    for (offset, &c) in chars[seg_start..seg_end].iter().enumerate() {
                        let idx = seg_start + offset;
                        let w = if c == '\t' {
                            tab_size - (cur_width % tab_size)
                        } else {
                            c.width().unwrap_or(0)
                        };

                        if cur_width + w > width && idx > line_start {
                            result.push(line_start..idx);
                            line_start = idx;
                            cur_width = 0;
                            let w_recomputed = if c == '\t' {
                                tab_size - (cur_width % tab_size)
                            } else {
                                c.width().unwrap_or(0)
                            };
                            cur_width += w_recomputed;
                        } else {
                            cur_width += w;
                        }
                    }
                }
            }

            seg_start = seg_end;
        }

        if line_start < len_without_newline {
            result.push(line_start..len_without_newline);
        } else if result.is_empty() {
            result.push(0..len_without_newline);
        }

        result
    }

    pub fn get_visual_col(&self, line_idx: usize, char_offset: usize, range: &Range<usize>, tab_size: usize) -> usize {
        let line = self.rope.line(line_idx);
        let mut visual_x = 0;
        use unicode_width::UnicodeWidthChar;

        for (i, c) in line.chars().enumerate() {
            if i < range.start { continue; }
            if i >= char_offset { break; }
            if c == '\t' {
                visual_x += tab_size - (visual_x % tab_size);
            } else {
                visual_x += c.width().unwrap_or(0);
            }
        }
        visual_x
    }

    pub fn get_char_at_vcol(&self, line_idx: usize, range: Range<usize>, target_vcol: usize, tab_size: usize) -> usize {
        let line = self.rope.line(line_idx);
        let mut visual_x = 0;
        let mut char_idx = self.rope.line_to_char(line_idx) + range.start;
        use unicode_width::UnicodeWidthChar;

        for (i, c) in line.chars().enumerate() {
            if i < range.start { continue; }
            if i >= range.end { break; }
            
            let char_w = if c == '\t' {
                tab_size - (visual_x % tab_size)
            } else {
                c.width().unwrap_or(0)
            };

            if visual_x + char_w > target_vcol {
                return char_idx;
            }

            visual_x += char_w;
            char_idx += 1;
            
            if c == '\n' || c == '\r' {
                return char_idx.saturating_sub(1);
            }
        }
        char_idx.saturating_sub(if range.end > range.start && self.is_line_ending(line.char(range.end - 1)) { 1 } else { 0 })
    }

    #[allow(clippy::single_range_in_vec_init)]
    pub fn wrap_line_px(
        &self,
        line_idx: usize,
        max_width_px: f32,
        ascii_width_px: f32,
        cjk_width_px: f32,
        tab_size: usize,
    ) -> Vec<Range<usize>> {
        if max_width_px <= 0.0 || ascii_width_px <= 0.0 {
            return vec![0..self.line(line_idx).len_chars()];
        }
        let line = self.line(line_idx);

        let mut len_without_newline = line.len_chars();
        while len_without_newline > 0 {
            let c = line.char(len_without_newline - 1);
            if c == '\n' || c == '\r' {
                len_without_newline -= 1;
            } else {
                break;
            }
        }

        if len_without_newline == 0 {
            return vec![0..0];
        }

        let line_str: String = line.chars().take(len_without_newline).collect();
        let breaks = Self::compute_line_break_segments(&line_str);
        let chars: Vec<char> = line_str.chars().collect();

        use unicode_width::UnicodeWidthChar;

        let tab_width_px = tab_size as f32 * ascii_width_px;
        let char_px_fn = |c: char, cur_px: f32| -> f32 {
            if c == '\t' {
                let col_px = cur_px % tab_width_px;
                tab_width_px - col_px
            } else if c.width() == Some(2) {
                cjk_width_px
            } else if c.width() == Some(0) {
                0.0
            } else {
                ascii_width_px
            }
        };

        let mut result = Vec::new();
        let mut line_start = 0;
        let mut cur_px = 0.0;
        let mut seg_start = 0;

        for seg_end in breaks {
            if seg_end <= seg_start {
                continue;
            }

            // Simulate adding segment `seg_start..seg_end` to current line
            let mut simulated_px = cur_px;
            for &c in &chars[seg_start..seg_end] {
                simulated_px += char_px_fn(c, simulated_px);
            }

            if simulated_px <= max_width_px + 0.01 {
                cur_px = simulated_px;
            } else {
                // If the current line already has content, wrap before this segment
                if seg_start > line_start {
                    result.push(line_start..seg_start);
                    line_start = seg_start;
                    cur_px = 0.0;
                }

                // Check if segment fits on a fresh line
                let mut fresh_px = 0.0;
                for &c in &chars[seg_start..seg_end] {
                    fresh_px += char_px_fn(c, fresh_px);
                }

                if fresh_px <= max_width_px + 0.01 {
                    cur_px = fresh_px;
                } else {
                    // Segment itself is longer than max_width_px: fallback to character-by-character wrap
                    for (offset, &c) in chars[seg_start..seg_end].iter().enumerate() {
                        let idx = seg_start + offset;
                        let px = char_px_fn(c, cur_px);

                        if cur_px + px > max_width_px + 0.01 && idx > line_start {
                            result.push(line_start..idx);
                            line_start = idx;
                            cur_px = 0.0;
                            let px_recomputed = char_px_fn(c, cur_px);
                            cur_px += px_recomputed;
                        } else {
                            cur_px += px;
                        }
                    }
                }
            }

            seg_start = seg_end;
        }

        if line_start < len_without_newline {
            result.push(line_start..len_without_newline);
        } else if result.is_empty() {
            result.push(0..len_without_newline);
        }

        result
    }

    pub fn get_visual_px(
        &self,
        line_idx: usize,
        char_offset: usize,
        range: &Range<usize>,
        ascii_width_px: f32,
        cjk_width_px: f32,
        tab_size: usize,
    ) -> f32 {
        let line = self.rope.line(line_idx);
        let mut visual_x = 0.0;
        let tab_width_px = tab_size as f32 * ascii_width_px;
        use unicode_width::UnicodeWidthChar;

        for (i, c) in line.chars().enumerate() {
            if i < range.start {
                continue;
            }
            if i >= char_offset {
                break;
            }
            let char_px = if c == '\t' {
                let col_px = visual_x % tab_width_px;
                tab_width_px - col_px
            } else if c.width() == Some(2) {
                cjk_width_px
            } else if c.width() == Some(0) {
                0.0
            } else {
                ascii_width_px
            };
            visual_x += char_px;
        }
        visual_x
    }

    pub fn get_char_at_v_px(
        &self,
        line_idx: usize,
        range: Range<usize>,
        target_x: f32,
        ascii_width_px: f32,
        cjk_width_px: f32,
        tab_size: usize,
    ) -> usize {
        let line = self.rope.line(line_idx);
        let mut visual_x = 0.0;
        let mut char_idx = self.rope.line_to_char(line_idx) + range.start;
        let tab_width_px = tab_size as f32 * ascii_width_px;
        use unicode_width::UnicodeWidthChar;

        for (i, c) in line.chars().enumerate() {
            if i < range.start {
                continue;
            }
            if i >= range.end {
                break;
            }

            let char_px = if c == '\t' {
                let col_px = visual_x % tab_width_px;
                tab_width_px - col_px
            } else if c.width() == Some(2) {
                cjk_width_px
            } else if c.width() == Some(0) {
                0.0
            } else {
                ascii_width_px
            };

            if visual_x + char_px / 2.0 > target_x {
                return char_idx;
            }

            visual_x += char_px;
            char_idx += 1;

            if c == '\n' || c == '\r' {
                return char_idx.saturating_sub(1);
            }
        }
        char_idx.saturating_sub(if range.end > range.start && self.is_line_ending(line.char(range.end - 1)) { 1 } else { 0 })
    }

    pub fn ensure_cursor_visible(&mut self, visible_lines: usize, visible_cols: usize, word_wrap: bool) {
        let (cursor_line, cursor_col) = self.char_to_line_col(self.cursor);
        if cursor_line < self.scroll_row {
            self.scroll_row = cursor_line;
        } else if cursor_line >= self.scroll_row + visible_lines {
            self.scroll_row = cursor_line.saturating_sub(visible_lines.saturating_sub(1));
        }

        if !word_wrap {
            if cursor_col < self.scroll_col {
                self.scroll_col = cursor_col;
            } else if cursor_col >= self.scroll_col + visible_cols {
                self.scroll_col = cursor_col.saturating_sub(visible_cols.saturating_sub(1));
            }
        } else {
            self.scroll_col = 0;
        }
    }

    fn decode_bytes(bytes: &[u8]) -> (Encoding, String) {
        // Use encoding_rs for BOM detection
        if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
            let (content, _, _) = enc.decode(&bytes[bom_len..]);
            let encoding = if enc == UTF_8 {
                Encoding::Utf8Bom
            } else if enc == UTF_16LE {
                Encoding::Utf16Le
            } else if enc == UTF_16BE {
                Encoding::Utf16Be
            } else {
                Encoding::Utf8 // Should not happen with for_bom
            };
            return (encoding, content.into_owned());
        }

        // Try UTF-8 first
        let (res, _enc, malformed) = UTF_8.decode(bytes);
        if !malformed {
            return (Encoding::Utf8, res.into_owned());
        }

        // Japanese encodings are common, try them
        for enc in &[SHIFT_JIS, EUC_JP, ISO_2022_JP] {
            let (res, _, malformed) = enc.decode(bytes);
            if !malformed {
                let encoding = if *enc == SHIFT_JIS {
                    Encoding::ShiftJis
                } else if *enc == EUC_JP {
                    Encoding::EucJp
                } else {
                    Encoding::Iso2022Jp
                };
                return (encoding, res.into_owned());
            }
        }

        // Fallback to Latin-1 (which never fails for any byte sequence)
        let (res, _, _) = WINDOWS_1252.decode(bytes);
        (Encoding::Latin1, res.into_owned())
    }

    pub fn line(&self, idx: usize) -> ropey::RopeSlice<'_> {
        self.rope.line(idx)
    }

    pub fn is_modified(&self) -> bool {
        self.modified_since_save || self.undo_stack.len() != self.saved_undo_len
    }

    pub fn char_to_line_col(&self, pos: usize) -> (usize, usize) {
        let pos = pos.min(self.rope.len_chars());
        let line = self.rope.char_to_line(pos);
        let line_start = self.rope.line_to_char(line);
        (line, pos - line_start)
    }

    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        if line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_start = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();
        
        // Don't allow cursor after line ending unless it's the last line without one
        let mut max_col = line_len;
        if line < self.rope.len_lines() - 1 || (line_len > 0 && self.is_line_ending(self.rope.char(line_start + line_len - 1))) {
            max_col = self.get_line_max_col(line);
        }
        
        line_start + col.min(max_col)
    }

    fn is_line_ending(&self, c: char) -> bool {
        c == '\n' || c == '\r'
    }

    pub fn get_line_max_col(&self, line: usize) -> usize {
        let line_start = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();
        let mut max_col = line_len;
        if max_col > 0 {
            let last = self.rope.char(line_start + max_col - 1);
            if last == '\n' || last == '\r' {
                max_col -= 1;
                if max_col > 0 {
                    let last2 = self.rope.char(line_start + max_col - 1);
                    if last2 == '\r' {
                        max_col -= 1;
                    }
                }
            }
        }
        max_col
    }

    pub fn move_cursor_left(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        if self.cursor > 0 {
            self.cursor -= 1;
        }

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_cursor_right(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        if self.cursor < self.rope.len_chars() {
            self.cursor += 1;
        }

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_cursor_up(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        if line > 0 {
            self.cursor = self.line_col_to_char(line - 1, col);
        }

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_cursor_down(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        if line < self.rope.len_lines().saturating_sub(1) {
            self.cursor = self.line_col_to_char(line + 1, col);
        } else if line == self.rope.len_lines().saturating_sub(1) {
             // Already on last line, but might want to move to end of line if we're not there
             self.cursor = self.line_col_to_char(line, self.get_line_max_col(line));
        }

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_cursor_vup(&mut self, width: usize, tab_size: usize, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        let wraps = self.wrap_line(line, width, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_vcol = self.get_visual_col(line, col, &wraps[v_idx], tab_size);

        if v_idx > 0 {
            let target_range = &wraps[v_idx - 1];
            self.cursor = self.get_char_at_vcol(line, target_range.clone(), current_vcol, tab_size);
        } else if line > 0 {
            let prev_line = line - 1;
            let prev_wraps = self.wrap_line(prev_line, width, tab_size);
            let target_range = prev_wraps.last().unwrap();
            self.cursor = self.get_char_at_vcol(prev_line, target_range.clone(), current_vcol, tab_size);
        }

        if extend_selection {
            self.update_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_cursor_vdown(&mut self, width: usize, tab_size: usize, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        let wraps = self.wrap_line(line, width, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_vcol = self.get_visual_col(line, col, &wraps[v_idx], tab_size);

        if v_idx + 1 < wraps.len() {
            let target_range = &wraps[v_idx + 1];
            self.cursor = self.get_char_at_vcol(line, target_range.clone(), current_vcol, tab_size);
        } else if line + 1 < self.line_count() {
            let next_line = line + 1;
            let next_wraps = self.wrap_line(next_line, width, tab_size);
            let target_range = &next_wraps[0];
            self.cursor = self.get_char_at_vcol(next_line, target_range.clone(), current_vcol, tab_size);
        } else if line == self.line_count().saturating_sub(1) {
            let target_range = wraps.last().unwrap();
            self.cursor = self.line_col_to_char(line, target_range.end);
        }

        if extend_selection {
            self.update_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_cursor_vup_px(
        &mut self,
        max_width_px: f32,
        ascii_width_px: f32,
        cjk_width_px: f32,
        tab_size: usize,
        extend_selection: bool,
    ) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        let wraps = self.wrap_line_px(line, max_width_px, ascii_width_px, cjk_width_px, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_v_x = self.get_visual_px(line, col, &wraps[v_idx], ascii_width_px, cjk_width_px, tab_size);

        if v_idx > 0 {
            let target_range = &wraps[v_idx - 1];
            self.cursor = self.get_char_at_v_px(line, target_range.clone(), current_v_x, ascii_width_px, cjk_width_px, tab_size);
        } else if line > 0 {
            let prev_line = line - 1;
            let prev_wraps = self.wrap_line_px(prev_line, max_width_px, ascii_width_px, cjk_width_px, tab_size);
            let target_range = prev_wraps.last().unwrap();
            self.cursor = self.get_char_at_v_px(prev_line, target_range.clone(), current_v_x, ascii_width_px, cjk_width_px, tab_size);
        }

        if extend_selection {
            self.update_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_cursor_vdown_px(
        &mut self,
        max_width_px: f32,
        ascii_width_px: f32,
        cjk_width_px: f32,
        tab_size: usize,
        extend_selection: bool,
    ) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let (line, col) = self.char_to_line_col(self.cursor);
        let wraps = self.wrap_line_px(line, max_width_px, ascii_width_px, cjk_width_px, tab_size);
        
        let mut v_idx = 0;
        for (i, range) in wraps.iter().enumerate() {
            if col >= range.start && (col < range.end || (col == range.end && i == wraps.len() - 1)) {
                v_idx = i;
                break;
            }
        }

        let current_v_x = self.get_visual_px(line, col, &wraps[v_idx], ascii_width_px, cjk_width_px, tab_size);

        if v_idx + 1 < wraps.len() {
            let target_range = &wraps[v_idx + 1];
            self.cursor = self.get_char_at_v_px(line, target_range.clone(), current_v_x, ascii_width_px, cjk_width_px, tab_size);
        } else if line + 1 < self.line_count() {
            let next_line = line + 1;
            let next_wraps = self.wrap_line_px(next_line, max_width_px, ascii_width_px, cjk_width_px, tab_size);
            let target_range = &next_wraps[0];
            self.cursor = self.get_char_at_v_px(next_line, target_range.clone(), current_v_x, ascii_width_px, cjk_width_px, tab_size);
        } else if line == self.line_count().saturating_sub(1) {
            let target_range = wraps.last().unwrap();
            self.cursor = self.line_col_to_char(line, target_range.end);
        }

        if extend_selection {
            self.update_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_cursor_home(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let line = self.rope.char_to_line(self.cursor);
        self.cursor = self.rope.line_to_char(line);

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_cursor_end(&mut self, extend_selection: bool) {
        if extend_selection {
            self.ensure_selection();
        } else {
            self.selection = None;
        }

        let line = self.rope.char_to_line(self.cursor);
        self.cursor = self.line_col_to_char(line, self.get_line_max_col(line));

        if extend_selection {
            self.update_selection();
        }
    }

    pub fn move_word_forward(&mut self, extend_selection: bool) {
        if extend_selection { self.ensure_selection(); } else { self.selection = None; }
        let mut pos = self.cursor;
        let len = self.rope.len_chars();
        if pos >= len { return; }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let start_char = self.rope.char(pos);
        
        if is_word_char(start_char) {
            while pos < len && is_word_char(self.rope.char(pos)) { pos += 1; }
        } else if !start_char.is_whitespace() {
            while pos < len && !is_word_char(self.rope.char(pos)) && !self.rope.char(pos).is_whitespace() { pos += 1; }
        }
        while pos < len && self.rope.char(pos).is_whitespace() { pos += 1; }
        
        self.cursor = pos;
        if extend_selection { self.update_selection(); }
    }

    pub fn move_word_backward(&mut self, extend_selection: bool) {
        if extend_selection { self.ensure_selection(); } else { self.selection = None; }
        let mut pos = self.cursor;
        if pos == 0 { return; }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        
        // Skip leading whitespace
        while pos > 0 && self.rope.char(pos - 1).is_whitespace() { pos -= 1; }
        if pos == 0 { self.cursor = 0; return; }

        let start_char = self.rope.char(pos - 1);
        if is_word_char(start_char) {
            while pos > 0 && is_word_char(self.rope.char(pos - 1)) { pos -= 1; }
        } else {
            while pos > 0 && !is_word_char(self.rope.char(pos - 1)) && !self.rope.char(pos - 1).is_whitespace() { pos -= 1; }
        }
        
        self.cursor = pos;
        if extend_selection { self.update_selection(); }
    }

    pub fn move_word_end(&mut self, extend_selection: bool) {
        if extend_selection { self.ensure_selection(); } else { self.selection = None; }
        let mut pos = self.cursor;
        let len = self.rope.len_chars();
        if pos >= len.saturating_sub(1) { return; }

        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        
        // Move to next char if we're at the start of current word
        pos += 1;
        while pos < len && self.rope.char(pos).is_whitespace() { pos += 1; }
        if pos >= len { self.cursor = len.saturating_sub(1); return; }

        let start_char = self.rope.char(pos);
        if is_word_char(start_char) {
            while pos < len - 1 && is_word_char(self.rope.char(pos + 1)) { pos += 1; }
        } else {
            while pos < len - 1 && !is_word_char(self.rope.char(pos + 1)) && !self.rope.char(pos + 1).is_whitespace() { pos += 1; }
        }
        
        self.cursor = pos;
        if extend_selection { self.update_selection(); }
    }

    pub fn ensure_selection(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
        if self.selection.is_none() {
            self.selection = Some(self.cursor..self.cursor);
        }
    }

    pub fn update_selection(&mut self) {
        if let Some(anchor) = self.selection_anchor {
            match self.vi_mode {
                crate::ViMode::VisualLine => {
                    let (anchor_line, _) = self.char_to_line_col(anchor);
                    let (cursor_line, _) = self.char_to_line_col(self.cursor);
                    let start_line = anchor_line.min(cursor_line);
                    let end_line = anchor_line.max(cursor_line);
                    let start_char = self.rope.line_to_char(start_line);
                    let end_char = if end_line + 1 < self.rope.len_lines() {
                        self.rope.line_to_char(end_line + 1)
                    } else {
                        self.rope.len_chars()
                    };
                    self.selection = Some(start_char..end_char);
                }
                _ => {
                    let start = anchor.min(self.cursor);
                    let end = anchor.max(self.cursor);
                    if start == end && self.vi_mode != crate::ViMode::Visual && self.vi_mode != crate::ViMode::VisualBlock {
                        self.selection = None;
                    } else {
                        self.selection = Some(start..end);
                    }
                }
            }
        }
    }

    pub fn get_visual_block_ranges(&self) -> Vec<Range<usize>> {
        if let Some(anchor) = self.selection_anchor {
            let (anchor_line, anchor_col) = self.char_to_line_col(anchor);
            let (cursor_line, cursor_col) = self.char_to_line_col(self.cursor);

            let start_line = anchor_line.min(cursor_line);
            let end_line = anchor_line.max(cursor_line);
            let min_col = anchor_col.min(cursor_col);
            let max_col = anchor_col.max(cursor_col);

            let mut ranges = Vec::new();
            for l in start_line..=end_line {
                if l >= self.rope.len_lines() { break; }
                let line_max_col = self.get_line_max_col(l);
                let l_start_col = min_col.min(line_max_col);
                let l_end_col = (max_col + 1).min(line_max_col); // inclusive of cursor column
                if l_start_col <= l_end_col {
                    let start_char = self.line_col_to_char(l, l_start_col);
                    let end_char = self.line_col_to_char(l, l_end_col);
                    ranges.push(start_char..end_char);
                }
            }
            ranges
        } else {
            vec![]
        }
    }

    pub fn get_visual_block_text(&self) -> String {
        let ranges = self.get_visual_block_ranges();
        let mut text = String::new();
        for (i, r) in ranges.into_iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            if !r.is_empty() {
                text.push_str(&self.rope.slice(r).to_string());
            }
        }
        text
    }

    pub fn delete_visual_block(&mut self) -> Option<EditDelta> {
        let ranges = self.get_visual_block_ranges();
        if ranges.is_empty() { return None; }

        let mut deleted_text = String::new();
        // Delete from bottom to top to preserve character offsets of earlier lines
        for (i, r) in ranges.iter().rev().enumerate() {
            if i > 0 {
                deleted_text.insert(0, '\n');
            }
            if !r.is_empty() {
                let piece = self.rope.slice(r.clone()).to_string();
                deleted_text.insert_str(0, &piece);
                self.delete(r.clone());
            }
        }

        if let Some(anchor) = self.selection_anchor {
            let (anchor_line, anchor_col) = self.char_to_line_col(anchor);
            let (cursor_line, cursor_col) = self.char_to_line_col(self.cursor);
            let target_line = anchor_line.min(cursor_line);
            let target_col = anchor_col.min(cursor_col);
            self.cursor = self.line_col_to_char(target_line, target_col);
        }

        self.selection = None;
        self.selection_anchor = None;
        self.vi_mode = crate::ViMode::Normal;
        None
    }

    pub fn insert_visual_block(&mut self, text: &str, is_append: bool) {
        if let Some(anchor) = self.selection_anchor {
            let (anchor_line, anchor_col) = self.char_to_line_col(anchor);
            let (cursor_line, cursor_col) = self.char_to_line_col(self.cursor);
            let start_line = anchor_line.min(cursor_line);
            let end_line = anchor_line.max(cursor_line);
            let target_col = if is_append {
                anchor_col.max(cursor_col) + 1
            } else {
                anchor_col.min(cursor_col)
            };

            // Insert from bottom to top
            for l in (start_line..=end_line).rev() {
                if l >= self.rope.len_lines() { continue; }
                let line_max = self.get_line_max_col(l);
                let col = target_col.min(line_max);
                let char_pos = self.line_col_to_char(l, col);
                self.insert(char_pos, text);
            }

            self.cursor = self.line_col_to_char(start_line, target_col + text.chars().count());
            self.selection = None;
            self.selection_anchor = None;
            self.vi_mode = crate::ViMode::Normal;
        }
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) -> EditDelta {
        let old_line_start = self.rope.char_to_line(range.start);
        let old_text = self.rope.slice(range.clone()).to_string();
        
        self.rope.remove(range.clone());
        self.rope.insert(range.start, text);
        self.redo_stack.clear();

        let new_char_count = text.chars().count();
        let new_line_end = self.rope.char_to_line(range.start + new_char_count);
        self.update_line_states(old_line_start, new_line_end + 1);

        let delta = EditDelta {
            char_range: range.clone(),
            old_text,
            new_text: text.to_string(),
            line_range: old_line_start..new_line_end + 1,
        };

        self.push_undo(delta.clone());
        self.cursor = range.start + new_char_count;
        self.selection = None;
        self.selection_anchor = None;
        delta
    }

    pub fn transform_selection_or_buffer(&mut self, transform_fn: impl FnOnce(&str) -> String) {
        if let Some(range) = self.selection.clone() {
            if range.start < range.end {
                let text = self.rope.slice(range.clone()).to_string();
                let transformed = transform_fn(&text);
                if transformed != text {
                    self.replace(range.clone(), &transformed);
                    self.selection = Some(range.start..self.cursor);
                }
                return;
            }
        }
        // Transform entire buffer
        let text = self.rope.to_string();
        let transformed = transform_fn(&text);
        if transformed != text {
            self.replace(0..self.rope.len_chars(), &transformed);
            self.cursor = self.cursor.min(self.rope.len_chars());
        }
    }

    pub fn select_word(&mut self, pos: usize) {
        let range = self.find_word_bounds(pos);
        self.selection_anchor = Some(range.start);
        self.selection = Some(range.clone());
        self.cursor = range.end;
    }

    pub fn select_line(&mut self, line: usize) {
        if line >= self.rope.len_lines() { return; }
        let start = self.rope.line_to_char(line);
        let end = if line < self.rope.len_lines() - 1 {
            self.rope.line_to_char(line + 1)
        } else {
            self.rope.len_chars()
        };
        self.selection_anchor = Some(start);
        self.selection = Some(start..end);
        self.cursor = end;
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.selection = Some(0..self.rope.len_chars());
        self.cursor = self.rope.len_chars();
    }

    pub fn find_word_bounds(&self, pos: usize) -> Range<usize> {
        if self.rope.len_chars() == 0 { return 0..0; }
        let pos = pos.min(self.rope.len_chars().saturating_sub(1));
        let mut start = pos;
        let mut end = pos;
        
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let initial_char = self.rope.char(pos);
        let target_is_word = is_word_char(initial_char);
        
        while start > 0 {
            if is_word_char(self.rope.char(start - 1)) != target_is_word {
                break;
            }
            start -= 1;
        }
        while end < self.rope.len_chars() {
            if is_word_char(self.rope.char(end)) != target_is_word {
                break;
            }
            end += 1;
        }
        start..end
    }

    pub fn insert(&mut self, pos: usize, text: &str) -> EditDelta {
        let old_line_start = self.rope.char_to_line(pos);
        let old_text = "".to_string();
        
        self.rope.insert(pos, text);
        self.redo_stack.clear();
        
        let new_char_count = text.chars().count();
        let new_line_end = self.rope.char_to_line(pos + new_char_count);
        
        self.update_line_states(old_line_start, new_line_end + 1);

        let delta = EditDelta {
            char_range: pos..pos,
            old_text,
            new_text: text.to_string(),
            line_range: old_line_start..new_line_end + 1,
        };
        
        self.push_undo(delta.clone());
        self.cursor = pos + new_char_count;
        self.selection = None;
        self.selection_anchor = None;
        delta
    }

    pub fn delete(&mut self, range: Range<usize>) -> EditDelta {
        let old_line_start = self.rope.char_to_line(range.start);
        let old_text = self.rope.slice(range.clone()).to_string();
        
        self.rope.remove(range.clone());
        self.redo_stack.clear();
        
        let new_line_count = self.rope.len_lines();
        let new_line_end = old_line_start.min(new_line_count.saturating_sub(1));
        
        self.update_line_states(old_line_start, new_line_end + 1);

        let delta = EditDelta {
            char_range: range.clone(),
            old_text,
            new_text: "".to_string(),
            line_range: old_line_start..old_line_start + 1, // Range after delete
        };
        
        self.push_undo(delta.clone());
        self.cursor = range.start;
        self.selection = None;
        self.selection_anchor = None;
        delta
    }

    fn push_undo(&mut self, delta: EditDelta) {
        self.undo_stack.push(delta);
        if self.undo_stack.len() > 1000 {
            self.undo_stack.remove(0);
            if self.saved_undo_len > 0 {
                self.saved_undo_len -= 1;
            } else {
                // If saved_undo_len was 0 and we dropped the oldest,
                // we can't get back to saved state.
                // For now just set it to a value that will never match.
                self.saved_undo_len = usize::MAX;
            }
        }
    }

    pub fn undo(&mut self) -> Option<EditDelta> {
        if let Some(delta) = self.undo_stack.pop() {
            let inverse = self.invert_delta(&delta);
            
            let pos = delta.char_range.start;
            let old_line_start = self.rope.char_to_line(pos);

            self.rope.remove(delta.char_range.start..(delta.char_range.start + delta.new_text.chars().count()));
            self.rope.insert(delta.char_range.start, &delta.old_text);
            
            let new_line_end = self.rope.char_to_line(pos + delta.old_text.chars().count());
            self.update_line_states(old_line_start, new_line_end + 1);

            self.redo_stack.push(inverse);
            self.cursor = delta.char_range.start;
            self.selection = None;
            self.selection_anchor = None;
            Some(delta)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<EditDelta> {
        if let Some(delta) = self.redo_stack.pop() {
            let inverse = self.invert_delta(&delta);
            
            let pos = delta.char_range.start;
            let old_line_start = self.rope.char_to_line(pos);

            self.rope.remove(delta.char_range.start..(delta.char_range.start + delta.new_text.chars().count()));
            self.rope.insert(delta.char_range.start, &delta.old_text);
            
            let new_line_end = self.rope.char_to_line(pos + delta.old_text.chars().count());
            self.update_line_states(old_line_start, new_line_end + 1);

            self.undo_stack.push(inverse);
            self.cursor = delta.char_range.start + delta.old_text.chars().count();
            self.selection = None;
            self.selection_anchor = None;
            Some(delta)
        } else {
            None
        }
    }

    fn invert_delta(&self, delta: &EditDelta) -> EditDelta {
        EditDelta {
            char_range: delta.char_range.start..(delta.char_range.start + delta.new_text.chars().count()),
            old_text: delta.new_text.clone(),
            new_text: delta.old_text.clone(),
            line_range: delta.line_range.clone(),
        }
    }

    pub fn save(&mut self) -> Result<()> {
        if let Some(path) = self.path.clone() {
            self.save_as(path)
        } else {
            anyhow::bail!("No path associated with buffer")
        }
    }

    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let content = self.rope.to_string();
        let content = match self.line_ending {
            LineEnding::Lf => content.replace("\r\n", "\n").replace('\r', "\n"),
            LineEnding::Crlf => content.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n"),
            LineEnding::Cr => content.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r"),
        };

        let encoder = match self.encoding {
            Encoding::Utf8 | Encoding::Utf8Bom => UTF_8,
            Encoding::Utf16Le => UTF_16LE,
            Encoding::Utf16Be => UTF_16BE,
            Encoding::ShiftJis => SHIFT_JIS,
            Encoding::EucJp => EUC_JP,
            Encoding::Iso2022Jp => ISO_2022_JP,
            Encoding::Latin1 => WINDOWS_1252,
        };

        let mut bytes = match self.encoding {
            Encoding::Utf8Bom => vec![0xEF, 0xBB, 0xBF],
            Encoding::Utf16Le => vec![0xFF, 0xFE],
            Encoding::Utf16Be => vec![0xFE, 0xFF],
            _ => vec![],
        };

        let (encoded_bytes, _, _malformed) = encoder.encode(&content);
        bytes.extend_from_slice(&encoded_bytes);

        let mut file = File::create(path.as_ref())?;
        file.write_all(&bytes)?;
        file.flush()?;

        self.path = Some(path.as_ref().to_path_buf());
        self.modified_since_save = false;
        self.saved_undo_len = self.undo_stack.len();
        if self.syntax_highlighter.is_none() {
            if let Some(highlighter) = Self::detect_syntax(path.as_ref()) {
                self.update_syntax(Some(highlighter));
            }
        }
        Ok(())
    }

    pub fn find_matching_bracket(&self, cursor: usize) -> Option<usize> {
        let total_chars = self.rope.len_chars();
        if cursor >= total_chars {
            return None;
        }
        let line_idx = self.rope.char_to_line(cursor);
        let line_start = self.rope.line_to_char(line_idx);
        let line_end = line_start + self.rope.line(line_idx).len_chars();

        let bracket_chars = ['(', ')', '{', '}', '[', ']'];
        let mut start_pos = None;
        for pos in cursor..line_end {
            let ch = self.rope.char(pos);
            if bracket_chars.contains(&ch) {
                start_pos = Some(pos);
                break;
            }
        }
        let pos = start_pos?;
        let ch = self.rope.char(pos);
        let (target, forward) = match ch {
            '(' => (')', true),
            ')' => ('(', false),
            '{' => ('}', true),
            '}' => ('{', false),
            '[' => (']', true),
            ']' => ('[', false),
            _ => return None,
        };

        let mut depth = 0;
        if forward {
            for (i, c) in self.rope.chars_at(pos).enumerate() {
                if c == ch {
                    depth += 1;
                } else if c == target {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos + i);
                    }
                }
            }
        } else {
            let mut curr = pos;
            loop {
                let c = self.rope.char(curr);
                if c == ch {
                    depth += 1;
                } else if c == target {
                    depth -= 1;
                    if depth == 0 {
                        return Some(curr);
                    }
                }
                if curr == 0 {
                    break;
                }
                curr -= 1;
            }
        }
        None
    }

    pub fn search_word_at_cursor(&mut self, forward: bool) {
        let range = self.find_word_bounds(self.cursor);
        let word = self.rope.slice(range).to_string();
        if word.is_empty() {
            return;
        }
        let query = crate::search::SearchQuery {
            pattern: word,
            flags: crate::search::SearchFlags {
                match_case: true,
                whole_word: true,
                use_regex: false,
            },
        };
        self.find_results = self.search(&query);
        if !self.find_results.is_empty() {
            if forward {
                let idx = self.find_results.iter().position(|m| m.char_range.start > self.cursor).unwrap_or(0);
                self.current_match_idx = Some(idx);
                let m = &self.find_results[idx];
                self.cursor = m.char_range.start;
                self.selection = Some(m.char_range.clone());
            } else {
                let idx = self.find_results.iter().rposition(|m| m.char_range.start < self.cursor).unwrap_or(self.find_results.len() - 1);
                self.current_match_idx = Some(idx);
                let m = &self.find_results[idx];
                self.cursor = m.char_range.start;
                self.selection = Some(m.char_range.clone());
            }
        }
    }

    pub fn find_next_match(&mut self) {
        if self.find_results.is_empty() {
            return;
        }
        let idx = match self.current_match_idx {
            Some(i) => (i + 1) % self.find_results.len(),
            None => 0,
        };
        self.current_match_idx = Some(idx);
        let m = &self.find_results[idx];
        self.cursor = m.char_range.start;
        self.selection = Some(m.char_range.clone());
    }

    pub fn find_prev_match(&mut self) {
        if self.find_results.is_empty() {
            return;
        }
        let idx = match self.current_match_idx {
            Some(i) => if i == 0 { self.find_results.len() - 1 } else { i - 1 },
            None => 0,
        };
        self.current_match_idx = Some(idx);
        let m = &self.find_results[idx];
        self.cursor = m.char_range.start;
        self.selection = Some(m.char_range.clone());
    }

    pub fn indent_line(&mut self, line: usize, expand_tab: bool, tab_size: usize) {
        if line >= self.line_count() {
            return;
        }
        let line_start = self.rope.line_to_char(line);
        let text = if expand_tab {
            " ".repeat(tab_size)
        } else {
            "\t".to_string()
        };
        self.insert(line_start, &text);
    }

    pub fn unindent_line(&mut self, line: usize, tab_size: usize) {
        if line >= self.line_count() {
            return;
        }
        let line_start = self.rope.line_to_char(line);
        let line_slice = self.rope.line(line);
        let line_str = line_slice.to_string();
        if line_str.starts_with('\t') {
            self.delete(line_start..line_start + 1);
        } else {
            let spaces = line_str.chars().take_while(|c| *c == ' ').take(tab_size).count();
            if spaces > 0 {
                self.delete(line_start..line_start + spaces);
            }
        }
    }

    pub fn indent_range(&mut self, start_line: usize, end_line: usize, expand_tab: bool, tab_size: usize) {
        let (s, e) = if start_line <= end_line { (start_line, end_line) } else { (end_line, start_line) };
        for l in s..=e.min(self.line_count().saturating_sub(1)) {
            self.indent_line(l, expand_tab, tab_size);
        }
    }

    pub fn unindent_range(&mut self, start_line: usize, end_line: usize, tab_size: usize) {
        let (s, e) = if start_line <= end_line { (start_line, end_line) } else { (end_line, start_line) };
        for l in s..=e.min(self.line_count().saturating_sub(1)) {
            self.unindent_line(l, tab_size);
        }
    }

    pub fn toggle_case_at_cursor(&mut self) {
        if self.cursor >= self.rope.len_chars() {
            return;
        }
        let ch = self.rope.char(self.cursor);
        if ch == '\n' || ch == '\r' {
            return;
        }
        let swapped = if ch.is_uppercase() {
            ch.to_lowercase().to_string()
        } else {
            ch.to_uppercase().to_string()
        };
        self.delete(self.cursor..self.cursor + 1);
        self.insert(self.cursor, &swapped);
        let (line, col) = self.char_to_line_col(self.cursor);
        let max_col = self.get_line_max_col(line);
        if col < max_col {
            self.move_cursor_right(false);
        }
    }

    pub fn change_case_range(&mut self, range: std::ops::Range<usize>, upper: Option<bool>) {
        if range.is_empty() || range.start >= self.rope.len_chars() {
            return;
        }
        let slice = self.rope.slice(range.clone()).to_string();
        let transformed: String = match upper {
            Some(true) => slice.to_uppercase(),
            Some(false) => slice.to_lowercase(),
            None => slice.chars().map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().to_string()
                } else {
                    c.to_uppercase().to_string()
                }
            }).collect(),
        };
        self.delete(range.clone());
        self.insert(range.start, &transformed);
        self.cursor = range.start;
    }

    pub fn find_inline_char(&self, cursor: usize, ch: char, forward: bool, till: bool) -> Option<usize> {
        if cursor >= self.rope.len_chars() {
            return None;
        }
        let line = self.rope.char_to_line(cursor);
        let line_start = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();
        let line_end = line_start + line_len;

        if forward {
            if cursor + 1 >= line_end {
                return None;
            }
            for pos in (cursor + 1)..line_end {
                let c = self.rope.char(pos);
                if c == '\n' || c == '\r' {
                    break;
                }
                if c == ch {
                    if till {
                        return Some(pos.saturating_sub(1).max(cursor));
                    } else {
                        return Some(pos);
                    }
                }
            }
        } else {
            if cursor == line_start {
                return None;
            }
            let mut curr = cursor.saturating_sub(1);
            loop {
                let c = self.rope.char(curr);
                if c == ch {
                    if till {
                        return Some((curr + 1).min(cursor));
                    } else {
                        return Some(curr);
                    }
                }
                if curr == line_start {
                    break;
                }
                curr -= 1;
            }
        }
        None
    }
}

/// Normalizes a character typed in Vi normal/visual/command mode,
/// converting full-width (Zenkaku) characters, full-width symbols,
/// and Japanese IME key outputs to their ASCII equivalents.
pub fn normalize_vi_char(c: char) -> char {
    match c {
        // Full-width ASCII: ！ (U+FF01) through ～ (U+FF5E)
        '\u{FF01}'..='\u{FF5E}' => {
            char::from_u32(c as u32 - 0xFF01 + 0x21).unwrap_or(c)
        }
        // Ideographic full-width space
        '\u{3000}' => ' ',
        // Japanese IME Romaji/Hiragana key outputs
        'い' | 'イ' => 'i',
        'あ' | 'ア' => 'a',
        'お' | 'オ' => 'o',
        'う' | 'ウ' => 'u',
        // Japanese Nakaguro (slash key on Japanese layout)
        '・' => '/',
        // Wave dash / full-width tilde
        '〜' => '~',
        _ => c,
    }
}

/// Normalizes a key string typed in Vi normal/visual/command mode.
pub fn normalize_vi_key(key: &str) -> String {
    match key {
        "っ" | "ッ" => "dd".to_string(),
        "・" => "/".to_string(),
        "〜" => "~".to_string(),
        "　" => " ".to_string(),
        s if s.chars().count() == 1 => {
            let c = s.chars().next().unwrap();
            normalize_vi_char(c).to_string()
        }
        s => s.chars().map(normalize_vi_char).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_insert_undo_redo() {
        let mut editor = Editor::new();
        assert_eq!(editor.rope.to_string(), "");
        assert!(!editor.is_modified());

        editor.insert(0, "Hello");
        assert_eq!(editor.rope.to_string(), "Hello");
        assert!(editor.is_modified());

        editor.undo();
        assert_eq!(editor.rope.to_string(), "");
        assert!(!editor.is_modified());

        editor.redo();
        assert_eq!(editor.rope.to_string(), "Hello");
        assert!(editor.is_modified());
    }

    #[test]
    fn test_editor_delete_undo_redo() {
        let mut editor = Editor::new();
        editor.insert(0, "Hello World");
        editor.saved_undo_len = editor.undo_stack.len();
        editor.modified_since_save = false;
        assert!(!editor.is_modified());

        editor.delete(5..11);
        assert_eq!(editor.rope.to_string(), "Hello");
        assert!(editor.is_modified());

        editor.undo();
        assert_eq!(editor.rope.to_string(), "Hello World");
        assert!(!editor.is_modified());

        editor.redo();
        assert_eq!(editor.rope.to_string(), "Hello");
        assert!(editor.is_modified());
    }

    #[test]
    fn test_undo_limit() {
        let mut editor = Editor::new();
        for i in 0..1005 {
            editor.insert(i, "a");
        }
        assert_eq!(editor.undo_stack.len(), 1000);
        assert!(editor.is_modified());
        
        // Undo all available
        for _ in 0..1000 {
            editor.undo();
        }
        assert_eq!(editor.undo_stack.len(), 0);
        // Should still be modified because we can't get back to original empty state
        assert!(editor.is_modified());
    }

    #[test]
    fn test_wrap_line() {
        let mut editor = Editor::new();
        editor.insert(0, "abcdefghij"); // 10 chars
        let wraps = editor.wrap_line(0, 4, 4); // width 4
        assert_eq!(wraps.len(), 3);
        assert_eq!(wraps[0], 0..4); // abcd
        assert_eq!(wraps[1], 4..8); // efgh
        assert_eq!(wraps[2], 8..10); // ij

        // Test with trailing newline
        let mut editor_nl = Editor::new();
        editor_nl.insert(0, "abcdefghij\n");
        let wraps_nl = editor_nl.wrap_line(0, 4, 4);
        assert_eq!(wraps_nl.len(), 3);
        assert_eq!(wraps_nl[0], 0..4);
        assert_eq!(wraps_nl[1], 4..8);
        assert_eq!(wraps_nl[2], 8..10); // newline stripped from range

        // Test with Japanese wide characters
        let mut editor_ja = Editor::new();
        editor_ja.insert(0, "Markdown (マークダウン) とは、プレーンテキスト形式で書式付きテキストを記述する軽量マークアップ言語である。\n");
        let wraps_ja = editor_ja.wrap_line(0, 96, 4);
        assert_eq!(wraps_ja.len(), 2);
        let line_chars: Vec<char> = editor_ja.rope.line(0).chars().collect();
        let chunk0: String = line_chars[wraps_ja[0].clone()].iter().collect();
        let chunk1: String = line_chars[wraps_ja[1].clone()].iter().collect();
        assert!(chunk0.ends_with('言'));
        assert_eq!(chunk1, "語である。");
    }

    #[test]
    fn test_move_cursor_vup_and_vdown() {
        let mut editor = Editor::new();
        editor.insert(0, "line 1 is a long sentence that will wrap\nline 2");
        // width 10
        let wraps = editor.wrap_line(0, 10, 4);
        assert!(wraps.len() > 1);

        editor.cursor = 0; // line 1 visual 0
        editor.move_cursor_vdown(10, 4, false);
        let (l, c) = editor.char_to_line_col(editor.cursor);
        assert_eq!(l, 0);
        assert_eq!(c, wraps[1].start);

        editor.move_cursor_vup(10, 4, false);
        assert_eq!(editor.cursor, 0);
    }

    #[test]
    fn test_syntax_detection_markdown() {
        let temp_dir = std::env::temp_dir();
        let md_file = temp_dir.join("test_led_syntax.md");
        std::fs::write(&md_file, "# Heading\n`code`\n**bold**\n").unwrap();

        let editor = Editor::from_file(&md_file).unwrap();
        assert!(editor.syntax_highlighter.is_some());
        assert_eq!(editor.line_tokens.len(), 4);
        // First line should have Heading keyword token
        assert!(!editor.line_tokens[0].as_ref().unwrap().is_empty());
        let _ = std::fs::remove_file(&md_file);
    }

    #[test]
    fn test_syntax_detection_json() {
        let temp_dir = std::env::temp_dir();
        let json_file = temp_dir.join("test_syntax_sample.json");
        std::fs::write(&json_file, r#"{"name": "zee", "fast": true}"#).unwrap();

        let editor = Editor::from_file(&json_file).unwrap();
        assert!(editor.syntax_highlighter.is_some());
        assert_eq!(editor.syntax_highlighter.as_ref().unwrap().def.meta.name, "JSON");
        assert_eq!(editor.line_tokens.len(), 1);
        assert!(!editor.line_tokens[0].as_ref().unwrap().is_empty());
        let _ = std::fs::remove_file(&json_file);
    }

    #[test]
    fn test_syntax_detection_shell_script() {
        let temp_dir = std::env::temp_dir();

        // 1. With .sh extension
        let sh_file = temp_dir.join("deploy.sh");
        std::fs::write(&sh_file, "if [ \"$1\" = \"prod\" ]; then\n  echo \"deploying\"\nfi\n").unwrap();
        let editor = Editor::from_file(&sh_file).unwrap();
        assert!(editor.syntax_highlighter.is_some());
        assert_eq!(editor.syntax_highlighter.as_ref().unwrap().def.meta.name, "Shell");
        assert!(!editor.line_tokens[0].as_ref().unwrap().is_empty());
        let _ = std::fs::remove_file(&sh_file);

        // 2. Extensionless file with shebang
        let bin_script = temp_dir.join("my-custom-cli");
        std::fs::write(&bin_script, "#!/usr/bin/env bash\nVAR=\"test\"\necho $VAR\n").unwrap();
        let editor = Editor::from_file(&bin_script).unwrap();
        assert!(editor.syntax_highlighter.is_some());
        assert_eq!(editor.syntax_highlighter.as_ref().unwrap().def.meta.name, "Shell");
        let _ = std::fs::remove_file(&bin_script);
    }

    #[test]
    fn test_visual_col_helpers() {
        let mut editor = Editor::new();
        editor.insert(0, "a\tbc"); // a (0), \t (1-3), b (4), c (5)
        let range = 0..4; // "a\tb"
        assert_eq!(editor.get_visual_col(0, 0, &range, 4), 0);
        assert_eq!(editor.get_visual_col(0, 1, &range, 4), 1);
        assert_eq!(editor.get_visual_col(0, 2, &range, 4), 4); // after tab
        
        assert_eq!(editor.get_char_at_vcol(0, 0..4, 0, 4), 0);
        assert_eq!(editor.get_char_at_vcol(0, 0..4, 1, 4), 1);
        assert_eq!(editor.get_char_at_vcol(0, 0..4, 2, 4), 1); // during tab
        assert_eq!(editor.get_char_at_vcol(0, 0..4, 4, 4), 2); // after tab
    }

    #[test]
    fn test_visual_line_selection() {
        let mut editor = Editor::new();
        editor.insert(0, "line 1\nline 2\nline 3\n");
        editor.vi_mode = crate::ViMode::VisualLine;
        editor.cursor = 0;
        editor.ensure_selection();
        editor.cursor = 8; // on line 2
        editor.update_selection();
        assert_eq!(editor.selection, Some(0..14)); // covers line 1 and line 2
    }

    #[test]
    fn test_visual_block_selection_and_deletion() {
        let mut editor = Editor::new();
        editor.insert(0, "apple\nbanana\ncherry\n");
        editor.vi_mode = crate::ViMode::VisualBlock;
        editor.cursor = 0; // line 0, col 0
        editor.ensure_selection();
        // move to line 2, col 2 ('c')
        let pos = editor.line_col_to_char(2, 2);
        editor.cursor = pos;
        
        let ranges = editor.get_visual_block_ranges();
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], 0..3); // "app"
        assert_eq!(ranges[1], 6..9); // "ban"
        assert_eq!(ranges[2], 13..16); // "che"
        
        let block_text = editor.get_visual_block_text();
        assert_eq!(block_text, "app\nban\nche");

        editor.delete_visual_block();
        assert_eq!(editor.rope.to_string(), "le\nana\nrry\n");
    }

    #[test]
    fn test_visual_block_insert() {
        let mut editor = Editor::new();
        editor.insert(0, "one\ntwo\nthree\n");
        editor.vi_mode = crate::ViMode::VisualBlock;
        editor.cursor = 0; // line 0, col 0
        editor.ensure_selection();
        let pos = editor.line_col_to_char(2, 0);
        editor.cursor = pos; // line 2, col 0

        editor.insert_visual_block("// ", false);
        assert_eq!(editor.rope.to_string(), "// one\n// two\n// three\n");
    }

    #[test]
    fn test_ensure_cursor_visible() {
        let mut editor = Editor::new();
        // 100 lines, each with 120 characters
        let content = (0..100)
            .map(|i| format!("Line {:03}: {}", i, "x".repeat(110)))
            .collect::<Vec<_>>()
            .join("\n");
        editor.insert(0, &content);

        // Move cursor to line 50, col 90
        let pos = editor.line_col_to_char(50, 90);
        editor.cursor = pos;

        // When word_wrap is false
        editor.ensure_cursor_visible(30, 80, false);
        assert!(editor.scroll_row <= 50);
        assert!(editor.scroll_row + 30 > 50);
        assert!(editor.scroll_col <= 90);
        assert!(editor.scroll_col + 80 > 90);

        // When word_wrap is true, scroll_col is reset to 0
        editor.ensure_cursor_visible(30, 80, true);
        assert_eq!(editor.scroll_col, 0);
        assert!(editor.scroll_row <= 50);
        assert!(editor.scroll_row + 30 > 50);
    }

    #[test]
    fn test_transform_selection_or_buffer() {
        let mut editor = Editor::new();
        editor.insert(0, "hello world");
        
        // 1. Transform entire buffer
        editor.transform_selection_or_buffer(|t| t.to_uppercase());
        assert_eq!(editor.rope.to_string(), "HELLO WORLD");

        // Undo
        editor.undo();
        assert_eq!(editor.rope.to_string(), "hello world");

        // Redo
        editor.redo();
        assert_eq!(editor.rope.to_string(), "HELLO WORLD");

        // 2. Transform selection only
        editor.selection = Some(0..5); // "HELLO"
        editor.transform_selection_or_buffer(|t| t.to_lowercase());
        assert_eq!(editor.rope.to_string(), "hello WORLD");

        // Undo selection transform
        editor.undo();
        assert_eq!(editor.rope.to_string(), "HELLO WORLD");
    }

    #[test]
    fn test_cursor_line_col_multibyte_and_newlines() {
        let mut editor = Editor::new();
        // Insert Japanese and English text
        editor.insert(0, "こんにちは\nWorld 🌍!\n日本語とEnglish\n");

        // Line 0: "こんにちは\n" (5 chars + 1 newline)
        assert_eq!(editor.char_to_line_col(0), (0, 0));
        assert_eq!(editor.char_to_line_col(2), (0, 2));
        assert_eq!(editor.char_to_line_col(5), (0, 5));
        assert_eq!(editor.line_col_to_char(0, 0), 0);
        assert_eq!(editor.line_col_to_char(0, 2), 2);
        assert_eq!(editor.line_col_to_char(0, 5), 5);

        // Line 1: "World 🌍!\n" (9 chars)
        let l1_start = editor.line_col_to_char(1, 0);
        assert_eq!(l1_start, 6);
        assert_eq!(editor.char_to_line_col(6), (1, 0));
        assert_eq!(editor.char_to_line_col(12), (1, 6)); // '🌍' is char at index 6 in "World 🌍!"
        assert_eq!(editor.line_col_to_char(1, 6), 12);

        // Line 2: "日本語とEnglish\n"
        let l2_start = editor.line_col_to_char(2, 0);
        assert_eq!(editor.char_to_line_col(l2_start + 4), (2, 4));
        assert_eq!(editor.line_col_to_char(2, 4), l2_start + 4);
    }

    #[test]
    fn test_chunk_selection_and_cursor_split_logic() {
        // Test chunk splitting logic matching editor_view render_line_content
        let text = "function hello()";
        let chars: Vec<char> = text.chars().collect();
        let chunk_start_char = 10;
        let chunk_len = chars.len();

        // Cursor at column 8 in chunk ("function " -> cursor before 'h')
        let cursor_col = 9;
        let chunk_start_col = 0;
        assert!(cursor_col >= chunk_start_col && cursor_col <= chunk_start_col + chunk_len);
        let split_idx = cursor_col - chunk_start_col;
        let part1 = chars[..split_idx].iter().collect::<String>();
        let part2 = chars[split_idx..].iter().collect::<String>();
        assert_eq!(part1, "function ");
        assert_eq!(part2, "hello()");

        // Selection slicing logic
        // Selection is 12..17 ("uncti")
        let sel: std::ops::Range<usize> = 12..17;
        let sel_start = sel.start.saturating_sub(chunk_start_char);
        let sel_end = sel.end.saturating_sub(chunk_start_char);
        assert_eq!(sel_start, 2);
        assert_eq!(sel_end, 7);

        let before_sel = chars[..sel_start].iter().collect::<String>();
        let highlighted = chars[sel_start..sel_end.min(chunk_len)].iter().collect::<String>();
        let after_sel = chars[sel_end.min(chunk_len)..].iter().collect::<String>();
        assert_eq!(before_sel, "fu");
        assert_eq!(highlighted, "nctio");
        assert_eq!(after_sel, "n hello()");
    }

    #[test]
    fn test_cursor_overlay_position_and_text_stability() {
        let mut editor = Editor::new();
        let sample = "const message = 'Hello, 世界！';\t// Tab test\n";
        editor.insert(0, sample);

        let ascii_w: f32 = 8.428;
        let cjk_w: f32 = 14.0;
        let tab_size = 4;
        let line_len = editor.line(0).len_chars();
        let range = 0..line_len;

        // Verify that visual X offset increases monotonically and precisely without any split points
        let mut prev_x = 0.0;
        for col in 0..line_len {
            let vx = editor.get_visual_px(0, col, &range, ascii_w, cjk_w, tab_size);
            if col == 0 {
                assert_eq!(vx, 0.0);
            } else {
                assert!(vx > prev_x, "Cursor vx should strictly increase with column");
            }
            prev_x = vx;
        }

        // Verify wrapped line cursor row detection
        let max_w: f32 = 100.0;
        let wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, tab_size);
        let wraps_len = wraps.len();
        assert!(wraps_len > 1);

        for (v_idx, r) in wraps.iter().enumerate() {
            let is_last_vrow = v_idx == wraps_len - 1;
            for col in 0..line_len {
                let is_on_row = if is_last_vrow {
                    col >= r.start && col <= r.end
                } else {
                    col >= r.start && col < r.end
                };
                if col >= r.start && col < r.end {
                    assert!(is_on_row);
                }
            }
        }
    }

    #[test]
    fn test_mouse_hit_testing_calculation() {
        let mut editor = Editor::new();
        editor.insert(0, "Line 0: abcdef\nLine 1: 123456\nLine 2: 漢字テキスト\n");

        let line_height = 20.0_f32;
        let font_size = 14.0_f32;
        let char_width = font_size * 0.6; // 8.4
        let gutter_width = 52.0_f32;
        let sidebar_width = 220.0_f32;
        let top_offset = 36.0_f32; // tab_bar_height

        // Helper replicating mouse_pos_to_char_pos
        let mouse_to_char = |mouse_x: f32, mouse_y: f32, scroll_row: usize, scroll_col: usize| -> usize {
            let relative_y = mouse_y - top_offset;
            let line_idx = (relative_y / line_height).floor() as i32 + scroll_row as i32;
            let line_idx = line_idx.max(0).min(editor.line_count() as i32 - 1) as usize;

            let left_offset = sidebar_width + gutter_width;
            let relative_x = mouse_x - left_offset + (scroll_col as f32 * char_width);
            let col_idx = (relative_x / char_width).round() as i32;
            let col_idx = col_idx.max(0) as usize;

            editor.line_col_to_char(line_idx, col_idx)
        };

        // Click on Line 0, Col 0 (just past sidebar and gutter)
        let pos = mouse_to_char(sidebar_width + gutter_width + 1.0, top_offset + 5.0, 0, 0);
        assert_eq!(pos, 0);

        // Click on Line 1, Col 8 (mouse_y = top_offset + 25.0)
        let pos1 = mouse_to_char(sidebar_width + gutter_width + (char_width * 8.0), top_offset + 25.0, 0, 0);
        let (l, c) = editor.char_to_line_col(pos1);
        assert_eq!(l, 1);
        assert_eq!(c, 8);
    }

    #[test]
    fn test_visual_line_rendering_and_text_slicing() {
        let mut editor = Editor::new();
        let text = "Rust（ラスト）は、性能、信頼性、生産性を重視したマルチパラダイムの汎用プログラミング言語である。\n";
        editor.insert(0, text);

        let wraps = editor.wrap_line(0, 86, 4);
        assert_eq!(wraps.len(), 2);
        
        let line = editor.rope.line(0);
        let mut line_str = line.to_string();
        if line_str.ends_with('\n') {
            line_str.pop();
        }

        let char_offsets: Vec<usize> = line_str.char_indices().map(|(b, _)| b).collect();
        let total_chars = char_offsets.len();

        let slice_vrow = |range: std::ops::Range<usize>| -> String {
            let byte_start = if range.start >= total_chars { line_str.len() } else { char_offsets[range.start] };
            let byte_end = if range.end >= total_chars { line_str.len() } else { char_offsets[range.end] };
            line_str[byte_start..byte_end].to_string()
        };

        assert_eq!(slice_vrow(wraps[0].clone()), "Rust（ラスト）は、性能、信頼性、生産性を重視したマルチパラダイムの汎用プログラミング言");
        assert_eq!(slice_vrow(wraps[1].clone()), "語である。");
    }

    #[test]
    fn test_wrap_line_px_and_cursor_movement() {
        let mut editor = Editor::new();
        let text = "テキストエディタで手軽に書いた文書からHTMLを生成するために開発されたが、PowerPoint形式やLaTeX形式のファイルへ変換するソフトウェア（コンバータ) も開発されている。各コンバータの開発者によって拡張が施された各種の方言が存在する。\n";
        editor.insert(0, text);

        let ascii_w = 7.225;
        let cjk_w = 12.0;
        let max_w = 944.0;

        let wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, 4);
        assert_eq!(wraps.len(), 2);
        assert_eq!(wraps[0], 0..87);
        assert_eq!(wraps[1], 87..123);

        // Test cursor movement across pixel-wrapped visual lines
        editor.cursor = 0;
        editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, 4, false);
        let (line, col) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line, 0);
        assert_eq!(col, 87);

        editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, 4, false);
        let (line, col) = editor.char_to_line_col(editor.cursor);
        assert_eq!(line, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn test_pixel_wrapping_prevents_premature_wrap_gap() {
        let mut editor = Editor::new();
        // Line 23 from sample.md
        let text = "テキストエディタで手軽に書いた文書からHTMLを生成するために開発されたが、PowerPoint形式やLaTeX形式のファイルへ変換するソフトウェア（コンバータ) も開発されている。各コンバータの開発者によって拡張が施された各種の方言が存在する。\n";
        editor.insert(0, text);

        let ascii_w: f32 = 7.225; // macOS 12pt Menlo monospace advance
        let cjk_w: f32 = 12.0;    // macOS 12pt Hiragino Sans advance
        let max_w: f32 = 944.0;   // Available viewport editor width

        // 1. Column-based wrapping (old approach) assumed 2 columns per CJK char:
        let wrap_cols = (max_w / ascii_w).floor() as usize; // 130 columns
        let old_wraps = editor.wrap_line(0, wrap_cols, 4);
        // The old approach wrapped at char 74 because 2 cols * 7.225 = 14.45px overestimated width by ~2.45px per CJK char
        assert_eq!(old_wraps[0].end, 74);

        // 2. Pixel-based wrapping (new approach) uses exact font metric advances:
        let new_wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, 4);
        assert_eq!(new_wraps[0].start, 0);
        // New approach packs up to char 87 (13 more characters in the line!)
        assert_eq!(new_wraps[0].end, 87);

        // Calculate the actual pixel width of the first line under new vs old wrapping:
        let calc_pixel_width = |range: std::ops::Range<usize>| -> f32 {
            let line = editor.line(0);
            let mut w: f32 = 0.0;
            for c in line.chars().skip(range.start).take(range.end - range.start) {
                if unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) == 2 {
                    w += cjk_w;
                } else {
                    w += ascii_w;
                }
            }
            w
        };

        let old_rendered_w = calc_pixel_width(old_wraps[0].clone());
        let new_rendered_w = calc_pixel_width(new_wraps[0].clone());

        // Old wrap left a huge gap of > 140px (~146.7px)
        let old_gap = max_w - old_rendered_w;
        assert!(old_gap > 140.0, "Old wrapping should have left a premature wrap gap > 140px, was {}", old_gap);

        // New wrap leaves less than a single character width of gap (< 12px)
        let new_gap = max_w - new_rendered_w;
        assert!(new_gap < cjk_w, "New wrapping must fill the line within 1 character advance, gap was {}", new_gap);
        assert!(new_rendered_w <= max_w, "New wrapping must not exceed available width");
    }

    #[test]
    fn test_pure_cjk_wrapping_eliminates_gap() {
        let mut editor = Editor::new();
        // 66 Japanese characters
        let text = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんアイウエオカキクケコサシスセソタチツテト\n";
        editor.insert(0, text);

        let ascii_w: f32 = 7.225;
        let cjk_w: f32 = 12.0;
        // Available width for exactly 40 CJK characters: 40 * 12.0 = 480.0px
        let max_w: f32 = 480.0;

        let wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, 4);
        assert_eq!(wraps[0].start, 0);
        assert_eq!(wraps[0].end, 40, "Exactly 40 fullwidth characters should fit in 480px");
        assert_eq!(wraps[1].start, 40);
        assert_eq!(wraps[1].end, 66);

        // Old column wrap would only fit 480 / 7.225 = 66 cols -> 33 CJK chars (leaving 7 * 12 = 84px gap)
        let old_wrap_cols = (max_w / ascii_w).floor() as usize;
        let old_wraps = editor.wrap_line(0, old_wrap_cols, 4);
        assert_eq!(old_wraps[0].end, 33);
        assert!(wraps[0].end > old_wraps[0].end);
    }

    #[test]
    fn test_pixel_visual_coordinate_roundtrip_and_snapping() {
        let mut editor = Editor::new();
        let text = "Hello世界！\tTestテスト\n";
        editor.insert(0, text);

        let ascii_w: f32 = 8.0;
        let cjk_w: f32 = 14.0;
        let tab_size = 4;
        let max_w: f32 = 500.0;

        let wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, tab_size);
        let range = wraps[0].clone();

        // Check roundtrip for every character: get_visual_px -> get_char_at_v_px returns the char
        let total_chars = range.end - range.start;
        for col in 0..total_chars {
            let vx = editor.get_visual_px(0, col, &range, ascii_w, cjk_w, tab_size);
            // Clicking slightly to the right of the start of the character (+1.0px) snaps to that character
            let found_char = editor.get_char_at_v_px(0, range.clone(), vx + 1.0, ascii_w, cjk_w, tab_size);
            assert_eq!(found_char, col, "Roundtrip failed for col {}", col);
        }

        // Clicking beyond the end of the line snaps to the end of the line (range.end)
        let far_x = 1000.0;
        let end_char = editor.get_char_at_v_px(0, range.clone(), far_x, ascii_w, cjk_w, tab_size);
        assert_eq!(end_char, range.end);
    }

    #[test]
    fn test_pixel_cursor_vertical_navigation_mixed_text() {
        let mut editor = Editor::new();
        // Line 0 has 20 ASCII chars
        // Line 1 has 20 CJK chars
        let text = "abcdefghijklmnopqrst\nあいうえおかきくけこさしすせそたちつてと\n";
        editor.insert(0, text);

        let ascii_w: f32 = 7.225;
        let cjk_w: f32 = 12.0;
        let tab_size = 4;
        let max_w: f32 = 200.0; // Lines will wrap into multiple visual lines

        let wraps_line0 = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, tab_size);
        let wraps_line1 = editor.wrap_line_px(1, max_w, ascii_w, cjk_w, tab_size);
        assert!(!wraps_line0.is_empty());
        assert!(wraps_line1.len() >= 2);

        // Position cursor at col 10 in line 0
        editor.cursor = 10;
        let x0 = editor.get_visual_px(0, 10, &wraps_line0[0], ascii_w, cjk_w, tab_size);
        assert!((x0 - 72.25).abs() < 0.01);

        // Move down to Line 1 (CJK text)
        editor.move_cursor_vdown_px(max_w, ascii_w, cjk_w, tab_size, false);
        let (l1, c1) = editor.char_to_line_col(editor.cursor);
        assert_eq!(l1, 1);
        // At ~72.25px in CJK text (each 12px), 72.25 / 12.0 = 6.02 -> col 6
        assert_eq!(c1, 6);

        // Move back up to Line 0
        editor.move_cursor_vup_px(max_w, ascii_w, cjk_w, tab_size, false);
        let (l0_again, c0_again) = editor.char_to_line_col(editor.cursor);
        assert_eq!(l0_again, 0);
        // Col 6 * 12.0 = 72.0px -> in ASCII text (each 7.225px), 72.0 / 7.225 = 9.96 -> snaps back to col 10!
        assert_eq!(c0_again, 10);
    }

    #[test]
    fn test_mouse_drag_selection_workflow() {
        let mut editor = Editor::new();
        editor.insert(0, "Hello World, this is a test.\n");

        // 1. Mouse down at character 6 ('W')
        let down_pos = 6;
        editor.cursor = down_pos;
        editor.selection = None;
        editor.selection_anchor = Some(down_pos);

        // 2. Mouse drag to character 11 ('d')
        let drag_pos1 = 11;
        editor.ensure_selection();
        editor.cursor = drag_pos1;
        editor.update_selection();
        assert_eq!(editor.selection, Some(6..11));
        assert_eq!(editor.selection_anchor, Some(6));

        // 3. Drag back to starting point (6)
        editor.cursor = 6;
        editor.update_selection();
        assert_eq!(editor.selection, None);
        // Anchor must NOT be lost!
        assert_eq!(editor.selection_anchor, Some(6));

        // 4. Drag backward to character 0 ('H')
        editor.cursor = 0;
        editor.update_selection();
        assert_eq!(editor.selection, Some(0..6));
        assert_eq!(editor.selection_anchor, Some(6));

        // 5. Drag forward to character 17 ('t')
        editor.cursor = 17;
        editor.update_selection();
        assert_eq!(editor.selection, Some(6..17));
        assert_eq!(editor.selection_anchor, Some(6));
    }

    #[test]
    fn test_find_matching_bracket() {
        let mut editor = Editor::new();
        editor.insert(0, "fn test(a: (i32, {b: [1, 2]})) -> bool { true }\n");

        // Cursor at index 7: '(' of fn test(
        assert_eq!(editor.find_matching_bracket(7), Some(29));
        // Cursor at matching ')' at index 29
        assert_eq!(editor.find_matching_bracket(29), Some(7));

        // Cursor at '{' of {b: ...} at index 17
        assert_eq!(editor.find_matching_bracket(17), Some(27));
        assert_eq!(editor.find_matching_bracket(27), Some(17));

        // Cursor at '[' at index 21
        assert_eq!(editor.find_matching_bracket(21), Some(26));
        assert_eq!(editor.find_matching_bracket(26), Some(21));

        // Cursor before bracket on the line (e.g. index 3 't' of test) finds first bracket '(' at 7 -> jumps to 29
        assert_eq!(editor.find_matching_bracket(3), Some(29));
    }

    #[test]
    fn test_search_word_and_navigate_matches() {
        let mut editor = Editor::new();
        editor.insert(0, "apple banana apple cherry apple date\n");

        // Cursor at first 'apple'
        editor.cursor = 2;
        editor.search_word_at_cursor(true);
        assert_eq!(editor.find_results.len(), 3);
        // Jump to next apple
        assert_eq!(editor.cursor, 13);

        // Next match
        editor.find_next_match();
        assert_eq!(editor.cursor, 26);

        // Wrap to first match
        editor.find_next_match();
        assert_eq!(editor.cursor, 0);

        // Previous match wraps to last match
        editor.find_prev_match();
        assert_eq!(editor.cursor, 26);
    }

    #[test]
    fn test_indent_and_unindent() {
        let mut editor = Editor::new();
        editor.insert(0, "line1\nline2\n");

        editor.indent_line(0, true, 4);
        assert_eq!(editor.rope.to_string(), "    line1\nline2\n");

        editor.unindent_line(0, 4);
        assert_eq!(editor.rope.to_string(), "line1\nline2\n");

        editor.indent_range(0, 1, false, 4);
        assert_eq!(editor.rope.to_string(), "\tline1\n\tline2\n");

        editor.unindent_range(0, 1, 4);
        assert_eq!(editor.rope.to_string(), "line1\nline2\n");
    }

    #[test]
    fn test_case_operations() {
        let mut editor = Editor::new();
        editor.insert(0, "Hello World\n");
        editor.cursor = 0;

        editor.toggle_case_at_cursor();
        assert_eq!(editor.rope.to_string(), "hello World\n");

        editor.change_case_range(0..5, Some(true));
        assert_eq!(editor.rope.to_string(), "HELLO World\n");

        editor.change_case_range(0..5, Some(false));
        assert_eq!(editor.rope.to_string(), "hello World\n");
    }

    #[test]
    fn test_find_inline_char() {
        let mut editor = Editor::new();
        editor.insert(0, "let foo = bar(baz);\n");

        // Forward 'f' from cursor 0 -> find 'b'
        assert_eq!(editor.find_inline_char(0, 'b', true, false), Some(10));
        // Forward 't' (till) from cursor 0 -> find 'b' (pos 9)
        assert_eq!(editor.find_inline_char(0, 'b', true, true), Some(9));

        // Backward 'F' from cursor 15 -> find 'b' at 14 ('baz')
        assert_eq!(editor.find_inline_char(15, 'b', false, false), Some(14));
        // Backward 'T' (till) from cursor 15 -> find 'b' at 14 -> till is 15
        assert_eq!(editor.find_inline_char(15, 'b', false, true), Some(15));
    }

    #[test]
    fn test_normalize_vi_char_and_key() {
        // Full-width Latin letters
        assert_eq!(normalize_vi_char('ｊ'), 'j');
        assert_eq!(normalize_vi_char('ｋ'), 'k');
        assert_eq!(normalize_vi_char('ｈ'), 'h');
        assert_eq!(normalize_vi_char('ｌ'), 'l');
        assert_eq!(normalize_vi_char('ｗ'), 'w');
        assert_eq!(normalize_vi_char('ｑ'), 'q');
        assert_eq!(normalize_vi_char('Ｇ'), 'G');

        // Full-width symbols
        assert_eq!(normalize_vi_char('：'), ':');
        assert_eq!(normalize_vi_char('／'), '/');
        assert_eq!(normalize_vi_char('・'), '/');
        assert_eq!(normalize_vi_char('〜'), '~');
        assert_eq!(normalize_vi_char('～'), '~');
        assert_eq!(normalize_vi_char('！'), '!');
        assert_eq!(normalize_vi_char('＄'), '$');
        assert_eq!(normalize_vi_char('％'), '%');
        assert_eq!(normalize_vi_char('＾'), '^');
        assert_eq!(normalize_vi_char('＊'), '*');
        assert_eq!(normalize_vi_char('\u{3000}'), ' ');

        // Full-width digits
        assert_eq!(normalize_vi_char('０'), '0');
        assert_eq!(normalize_vi_char('９'), '9');

        // Japanese Romaji/Kana outputs
        assert_eq!(normalize_vi_char('い'), 'i');
        assert_eq!(normalize_vi_char('イ'), 'i');
        assert_eq!(normalize_vi_char('あ'), 'a');
        assert_eq!(normalize_vi_char('ア'), 'a');
        assert_eq!(normalize_vi_char('お'), 'o');
        assert_eq!(normalize_vi_char('オ'), 'o');
        assert_eq!(normalize_vi_char('う'), 'u');
        assert_eq!(normalize_vi_char('ウ'), 'u');

        // normalize_vi_key tests
        assert_eq!(normalize_vi_key("っ"), "dd");
        assert_eq!(normalize_vi_key("ッ"), "dd");
        assert_eq!(normalize_vi_key("ｊ"), "j");
        assert_eq!(normalize_vi_key("い"), "i");
        assert_eq!(normalize_vi_key("・"), "/");
        assert_eq!(normalize_vi_key("：ｗｑ"), ":wq");
        assert_eq!(normalize_vi_key("："), ":");
    }

    #[test]
    fn test_japanese_kinsoku_shori_punctuation_and_brackets() {
        let mut editor = Editor::new();
        // Width 12 allows exactly 6 full-width (2-col) characters per line
        // Without kinsoku shori, "これは、テストです。" at width 6 cols (3 full-width chars) would break as:
        // Line 1: "これは" (3 chars, 6 cols)
        // Line 2: "、テス" (comma at start!)
        let text = "これは、テストです。";
        editor.insert(0, text);

        let wraps = editor.wrap_line(0, 6, 4);
        let chars: Vec<char> = text.chars().collect();
        for (i, range) in wraps.iter().enumerate() {
            let line_chunk: String = chars[range.clone()].iter().collect();
            let first_char = line_chunk.chars().next().unwrap();
            assert_ne!(first_char, '、', "Visual line {} must not start with '、'", i);
            assert_ne!(first_char, '。', "Visual line {} must not start with '。'", i);
        }

        // Check bracket kinsoku
        let bracket_text = "「これは、（テスト）です。」";
        let mut editor_bracket = Editor::new();
        editor_bracket.insert(0, bracket_text);
        let b_chars: Vec<char> = bracket_text.chars().collect();
        let b_wraps = editor_bracket.wrap_line(0, 8, 4); // 4 CJK chars per line
        for (i, range) in b_wraps.iter().enumerate() {
            let line_chunk: String = b_chars[range.clone()].iter().collect();
            let first_char = line_chunk.chars().next().unwrap();
            assert_ne!(first_char, '、', "Line {} must not start with comma", i);
            assert_ne!(first_char, '。', "Line {} must not start with period", i);
            assert_ne!(first_char, '）', "Line {} must not start with closing parenthesis", i);
            assert_ne!(first_char, '」', "Line {} must not start with closing quote", i);
            if i < b_wraps.len() - 1 {
                let last_char = line_chunk.chars().last().unwrap();
                assert_ne!(last_char, '「', "Line {} must not end with opening quote when text follows", i);
                assert_ne!(last_char, '（', "Line {} must not end with opening parenthesis when text follows", i);
            }
        }
    }

    #[test]
    fn test_english_word_wrapping_and_fallback() {
        let mut editor = Editor::new();
        let text = "The quick brown fox jumps over the lazy dog.";
        editor.insert(0, text);

        // Width 12
        let wraps = editor.wrap_line(0, 12, 4);
        let chars: Vec<char> = text.chars().collect();
        for range in &wraps {
            let chunk: String = chars[range.clone()].iter().collect();
            // Verify no words are sliced in the middle
            let words: Vec<&str> = chunk.split_whitespace().collect();
            for word in words {
                assert!(
                    ["The", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog."].contains(&word),
                    "Word '{}' was split across lines in chunk '{}'", word, chunk
                );
            }
        }

        // Test emergency fallback for excessively long unbroken word
        let mut editor_long = Editor::new();
        editor_long.insert(0, "Supercalifragilisticexpialidocious");
        let wraps_long = editor_long.wrap_line(0, 10, 4);
        assert_eq!(wraps_long.len(), 4);
        assert_eq!(wraps_long[0], 0..10);
        assert_eq!(wraps_long[1], 10..20);
        assert_eq!(wraps_long[2], 20..30);
        assert_eq!(wraps_long[3], 30..34);
    }

    #[test]
    fn test_wrap_line_px_kinsoku_shori() {
        let mut editor = Editor::new();
        let text = "テキストエディタで手軽に書いた文書からHTMLを生成するために開発されたが、PowerPoint形式やLaTeX形式のファイルへ変換するソフトウェア（コンバータ）も開発されている。";
        editor.insert(0, text);

        let ascii_w = 7.225;
        let cjk_w = 12.0;
        let max_w = 400.0;

        let wraps = editor.wrap_line_px(0, max_w, ascii_w, cjk_w, 4);
        let chars: Vec<char> = text.chars().collect();
        for (i, range) in wraps.iter().enumerate() {
            let chunk: String = chars[range.clone()].iter().collect();
            let first_char = chunk.chars().next().unwrap();
            assert_ne!(first_char, '、', "Line {} in pixel wrap must not start with '、'", i);
            assert_ne!(first_char, '。', "Line {} in pixel wrap must not start with '。'", i);
            assert_ne!(first_char, '）', "Line {} in pixel wrap must not start with '）'", i);
            if i < wraps.len() - 1 {
                let last_char = chunk.chars().last().unwrap();
                assert_ne!(last_char, '（', "Line {} in pixel wrap must not end with '（'", i);
            }
        }
    }

    #[test]
    fn test_editor_reload_from_disk() {
        let temp_file = std::env::temp_dir().join("zee_test_reload.txt");
        std::fs::write(&temp_file, "Initial disk content\nLine 2").unwrap();

        let mut editor = Editor::from_file(&temp_file).unwrap();
        assert_eq!(editor.rope.to_string(), "Initial disk content\nLine 2");
        assert!(!editor.is_modified());

        // Modify buffer in editor
        editor.insert(0, "Changed: ");
        assert!(editor.is_modified());

        // Overwrite disk file from outside
        std::fs::write(&temp_file, "External update from git\nNew line").unwrap();

        // Reload
        editor.reload_from_disk().unwrap();
        assert_eq!(editor.rope.to_string(), "External update from git\nNew line");
        assert!(!editor.is_modified());

        let _ = std::fs::remove_file(temp_file);
    }
}



