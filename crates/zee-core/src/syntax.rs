use serde::{Deserialize, Serialize};
use regex::Regex;
use std::ops::Range;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Keyword,
    TypeName,
    Function,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Constant,
    Attribute,
    Error,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    InlineCode,
    Bold,
    Link,
    ListMarker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxDefinition {
    pub meta: SyntaxMeta,
    #[serde(rename = "rule", default)]
    pub rules: Vec<SyntaxRuleDef>,
}

impl SyntaxDefinition {
    pub fn builtins() -> Vec<Self> {
        let files = [
            include_str!("../../../assets/syntax/c.toml"),
            include_str!("../../../assets/syntax/cpp.toml"),
            include_str!("../../../assets/syntax/css.toml"),
            include_str!("../../../assets/syntax/go.toml"),
            include_str!("../../../assets/syntax/html.toml"),
            include_str!("../../../assets/syntax/javascript.toml"),
            include_str!("../../../assets/syntax/json.toml"),
            include_str!("../../../assets/syntax/markdown.toml"),
            include_str!("../../../assets/syntax/plain-text.toml"),
            include_str!("../../../assets/syntax/python.toml"),
            include_str!("../../../assets/syntax/rust.toml"),
            include_str!("../../../assets/syntax/shell.toml"),
            include_str!("../../../assets/syntax/sql.toml"),
            include_str!("../../../assets/syntax/swift.toml"),
            include_str!("../../../assets/syntax/toml.toml"),
            include_str!("../../../assets/syntax/typescript.toml"),
            include_str!("../../../assets/syntax/xml.toml"),
            include_str!("../../../assets/syntax/yaml.toml"),
        ];

        files
            .iter()
            .map(|s| toml::from_str(s).expect("failed to parse builtin syntax definition"))
            .collect()
    }

    pub fn load_all() -> Vec<Self> {
        let mut syntaxes = Self::builtins();

        if let Some(syntax_dir) = crate::config::Config::syntax_dir() {
            if syntax_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(syntax_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Ok(mut custom_syntax) = toml::from_str::<SyntaxDefinition>(&content) {
                                    if custom_syntax.meta.name.is_empty() {
                                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                            custom_syntax.meta.name = stem.to_string();
                                        }
                                    }
                                    // If custom syntax has same name as a built-in, override it
                                    if let Some(pos) = syntaxes.iter().position(|s| s.meta.name.eq_ignore_ascii_case(&custom_syntax.meta.name)) {
                                        syntaxes[pos] = custom_syntax;
                                    } else {
                                        syntaxes.push(custom_syntax);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        syntaxes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentSyntax {
    Line(String),
    Block(String, String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxMeta {
    pub name: String,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub line_comment: Option<String>,
    #[serde(default)]
    pub block_comment: Option<Vec<String>>,
}

impl SyntaxMeta {
    pub fn comment_syntax(&self) -> Option<CommentSyntax> {
        if let Some(ref l) = self.line_comment {
            return Some(CommentSyntax::Line(l.clone()));
        }
        if let Some(ref b) = self.block_comment {
            if b.len() == 2 {
                return Some(CommentSyntax::Block(b[0].clone(), b[1].clone()));
            }
        }
        let name_lower = self.name.to_lowercase();
        match name_lower.as_str() {
            "python" | "shell" | "sh" | "bash" | "zsh" | "yaml" | "toml" | "ruby" | "perl" => {
                Some(CommentSyntax::Line("#".to_string()))
            }
            "rust" | "c" | "cpp" | "c++" | "go" | "javascript" | "typescript" | "swift" | "java" | "csharp" => {
                Some(CommentSyntax::Line("//".to_string()))
            }
            "sql" | "lua" | "haskell" => {
                Some(CommentSyntax::Line("--".to_string()))
            }
            "ini" => {
                Some(CommentSyntax::Line(";".to_string()))
            }
            "css" => {
                Some(CommentSyntax::Block("/*".to_string(), "*/".to_string()))
            }
            "html" | "xml" | "markdown" => {
                Some(CommentSyntax::Block("<!--".to_string(), "-->".to_string()))
            }
            _ => {
                if self.extensions.iter().any(|e| matches!(e.to_lowercase().as_str(), "py" | "sh" | "bash" | "zsh" | "yml" | "yaml" | "toml" | "rb" | "pl")) {
                    Some(CommentSyntax::Line("#".to_string()))
                } else if self.extensions.iter().any(|e| matches!(e.to_lowercase().as_str(), "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "go" | "js" | "ts" | "jsx" | "tsx" | "swift" | "java")) {
                    Some(CommentSyntax::Line("//".to_string()))
                } else if self.extensions.iter().any(|e| matches!(e.to_lowercase().as_str(), "sql" | "lua")) {
                    Some(CommentSyntax::Line("--".to_string()))
                } else if self.extensions.iter().any(|e| matches!(e.to_lowercase().as_str(), "html" | "xml" | "md" | "markdown")) {
                    Some(CommentSyntax::Block("<!--".to_string(), "-->".to_string()))
                } else if self.extensions.iter().any(|e| matches!(e.to_lowercase().as_str(), "css")) {
                    Some(CommentSyntax::Block("/*".to_string(), "*/".to_string()))
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyntaxRuleDef {
    pub token: TokenType,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub word_boundary: bool,
    #[serde(default)]
    pub escape: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    Normal,
    InsideRegion { rule_index: usize },
}

#[derive(Debug, Clone)]
pub struct TokenSpan {
    pub byte_range: Range<usize>,
    pub token: TokenType,
}

pub struct CompiledRule {
    pub token: TokenType,
    pub start_re: Regex,
    pub end_re: Option<Regex>,
    pub escape: Option<char>,
}

pub struct SyntaxHighlighter {
    pub def: SyntaxDefinition,
    pub rules: Vec<CompiledRule>,
}

impl SyntaxHighlighter {
    pub fn new(def: SyntaxDefinition) -> anyhow::Result<Self> {
        let mut rules = Vec::new();
        for rule in &def.rules {
            if let Some(ref pattern) = rule.pattern {
                let pattern = if rule.word_boundary {
                    format!(r"\b{}\b", pattern)
                } else {
                    pattern.clone()
                };
                rules.push(CompiledRule {
                    token: rule.token,
                    start_re: Regex::new(&pattern)?,
                    end_re: None,
                    escape: None,
                });
            } else if let (Some(start), Some(end)) = (&rule.start, &rule.end) {
                rules.push(CompiledRule {
                    token: rule.token,
                    start_re: Regex::new(start)?,
                    end_re: Some(Regex::new(end)?),
                    escape: rule.escape.as_ref().and_then(|s| s.chars().next()),
                });
            }
        }
        Ok(Self { def, rules })
    }

    /// Linear pre-pass to resolve multi-line region boundaries.
    /// Returns the state at the START of each line.
    pub fn compute_line_states(&self, lines: &[String], initial_state: LineState) -> Vec<LineState> {
        let mut states = Vec::with_capacity(lines.len());
        let mut current_state = initial_state;

        for line in lines {
            states.push(current_state);
            current_state = self.next_line_state(line, current_state);
        }

        states
    }

    pub fn next_line_state(&self, line: &str, mut state: LineState) -> LineState {
        let mut offset = 0;
        while offset < line.len() {
            match state {
                LineState::Normal => {
                    let mut best_match: Option<(usize, usize, usize)> = None; // (rule_index, abs_start, abs_end)
                    for (i, rule) in self.rules.iter().enumerate() {
                        if let Some(caps) = rule.start_re.captures_at(line, offset) {
                            let m = caps.get(0).unwrap();
                            if best_match.is_none() || m.start() < best_match.unwrap().1 {
                                best_match = Some((i, m.start(), m.end()));
                            }
                        }
                    }

                    if let Some((i, _start, end)) = best_match {
                        let rule = &self.rules[i];
                        if rule.end_re.is_some() {
                            state = LineState::InsideRegion { rule_index: i };
                        }
                        if end > offset {
                            offset = end;
                        } else if let Some(ch) = line[offset..].chars().next() {
                            offset += ch.len_utf8();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                LineState::InsideRegion { rule_index } => {
                    let rule = &self.rules[rule_index];
                    let end_re = rule.end_re.as_ref().unwrap();
                    
                    let mut search_offset = offset;
                    let mut found_end = false;
                    while let Some(m) = end_re.find_at(line, search_offset) {
                        let end_pos = m.start();
                        if let Some(esc) = rule.escape {
                            if end_pos > 0 && line.as_bytes()[end_pos - 1] as char == esc {
                                // Check if the escape is escaped
                                let mut esc_count = 0;
                                for c in line[..end_pos].chars().rev() {
                                    if c == esc { esc_count += 1; } else { break; }
                                }
                                if esc_count % 2 != 0 {
                                    // Escaped
                                    search_offset = if m.end() > search_offset {
                                        m.end()
                                    } else if let Some(ch) = line[search_offset..].chars().next() {
                                        search_offset + ch.len_utf8()
                                    } else {
                                        break;
                                    };
                                    continue;
                                }
                            }
                        }
                        offset = if m.end() > offset {
                            m.end()
                        } else if let Some(ch) = line[offset..].chars().next() {
                            offset + ch.len_utf8()
                        } else {
                            line.len()
                        };
                        state = LineState::Normal;
                        found_end = true;
                        break;
                    }
                    if !found_end {
                        return state;
                    }
                }
            }
        }
        state
    }

    pub fn highlight_line(&self, line: &str, mut state: LineState) -> Vec<TokenSpan> {
        let mut spans = Vec::new();
        let mut offset = 0;

        while offset < line.len() {
            match state {
                LineState::Normal => {
                    let mut best_match: Option<(usize, usize, usize, usize, usize)> = None; // (rule_index, abs_m_start, abs_m_end, abs_s_start, abs_s_end)
                    for (i, rule) in self.rules.iter().enumerate() {
                        if let Some(caps) = rule.start_re.captures_at(line, offset) {
                            let m = caps.get(0).unwrap();
                            if best_match.is_none() || m.start() < best_match.unwrap().1 {
                                let (s_start, s_end) = if let Some(g1) = caps.get(1) {
                                    (g1.start(), g1.end())
                                } else {
                                    (m.start(), m.end())
                                };
                                best_match = Some((i, m.start(), m.end(), s_start, s_end));
                            }
                        }
                    }

                    if let Some((i, _m_start, m_end, s_start, s_end)) = best_match {
                        let rule = &self.rules[i];
                        if rule.end_re.is_some() {
                            state = LineState::InsideRegion { rule_index: i };
                        }
                        spans.push(TokenSpan {
                            byte_range: s_start..s_end,
                            token: rule.token,
                        });
                        offset = if m_end > offset {
                            m_end
                        } else if let Some(ch) = line[offset..].chars().next() {
                            offset + ch.len_utf8()
                        } else {
                            break;
                        };
                    } else {
                        break;
                    }
                }
                LineState::InsideRegion { rule_index } => {
                    let rule = &self.rules[rule_index];
                    let end_re = rule.end_re.as_ref().unwrap();
                    
                    let mut search_offset = offset;
                    let mut found_end = false;
                    while let Some(m) = end_re.find_at(line, search_offset) {
                        let end_pos = m.start();
                        if let Some(esc) = rule.escape {
                            if end_pos > 0 && line.as_bytes()[end_pos - 1] as char == esc {
                                let mut esc_count = 0;
                                for c in line[..end_pos].chars().rev() {
                                    if c == esc { esc_count += 1; } else { break; }
                                }
                                if esc_count % 2 != 0 {
                                    search_offset = if m.end() > search_offset {
                                        m.end()
                                    } else if let Some(ch) = line[search_offset..].chars().next() {
                                        search_offset + ch.len_utf8()
                                    } else {
                                        break;
                                    };
                                    continue;
                                }
                            }
                        }
                        let abs_end = m.end();
                        spans.push(TokenSpan {
                            byte_range: offset..abs_end,
                            token: rule.token,
                        });
                        offset = if abs_end > offset {
                            abs_end
                        } else if let Some(ch) = line[offset..].chars().next() {
                            offset + ch.len_utf8()
                        } else {
                            line.len()
                        };
                        state = LineState::Normal;
                        found_end = true;
                        break;
                    }
                    if !found_end {
                        spans.push(TokenSpan {
                            byte_range: offset..line.len(),
                            token: rule.token,
                        });
                        offset = line.len();
                    }
                }
            }
        }
        spans
    }

    pub fn highlight_lines_parallel(&self, lines: &[String], line_states: &[LineState]) -> Vec<Vec<TokenSpan>> {
        lines.par_iter().zip(line_states.par_iter())
            .map(|(line, state)| self.highlight_line(line, *state))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtin_syntaxes_compile() {
        let builtins = SyntaxDefinition::builtins();
        assert!(builtins.iter().any(|b| b.meta.name == "JSON"));
        for def in builtins {
            let name = def.meta.name.clone();
            if let Err(e) = SyntaxHighlighter::new(def) {
                panic!("Failed to compile syntax {}: {}", name, e);
            }
        }
    }

    #[test]
    fn test_json_syntax_highlighting() {
        let builtins = SyntaxDefinition::builtins();
        let json_def = builtins.into_iter().find(|b| b.meta.name == "JSON").unwrap();
        let highlighter = SyntaxHighlighter::new(json_def).unwrap();

        let line = r#"{"name": "zee", "count": 42, "valid": true}"#;
        let spans = highlighter.highlight_line(line, LineState::Normal);
        assert!(!spans.is_empty());
        // Verify key, string, number, constant, punctuation are captured
        let has_type = spans.iter().any(|s| s.token == TokenType::TypeName);
        let has_string = spans.iter().any(|s| s.token == TokenType::String);
        let has_number = spans.iter().any(|s| s.token == TokenType::Number);
        let has_const = spans.iter().any(|s| s.token == TokenType::Constant);
        let has_punct = spans.iter().any(|s| s.token == TokenType::Punctuation);
        assert!(has_type);
        assert!(has_string);
        assert!(has_number);
        assert!(has_const);
        assert!(has_punct);
    }

    #[test]
    fn test_shell_syntax_highlighting() {
        let builtins = SyntaxDefinition::builtins();
        let shell_def = builtins.into_iter().find(|b| b.meta.name == "Shell").unwrap();
        let highlighter = SyntaxHighlighter::new(shell_def).unwrap();

        let line = r#"if [ $ENV = "production" ]; then # check env"#;
        let spans = highlighter.highlight_line(line, LineState::Normal);
        assert!(!spans.is_empty());
        assert!(spans.iter().any(|s| s.token == TokenType::Keyword));
        assert!(spans.iter().any(|s| s.token == TokenType::Attribute)); // $ENV
        assert!(spans.iter().any(|s| s.token == TokenType::String)); // "production"
        assert!(spans.iter().any(|s| s.token == TokenType::Comment)); // # check env
    }

    #[test]
    fn test_markdown_heading_and_fenced_code_block_edge_cases() {
        let builtins = SyntaxDefinition::builtins();
        let md_def = builtins.into_iter().find(|b| b.meta.name == "Markdown").unwrap();
        let highlighter = SyntaxHighlighter::new(md_def).unwrap();

        // 1. Inline triple backticks in prose must NOT hijack subsequent # Heading1 lines
        let lines = vec![
            "コードブロックは ``` で囲みます。\n".to_string(),
            "# 見出し1\n".to_string(),
            "```rust\n".to_string(),
            "# not a heading inside code fence\n".to_string(),
            "```\n".to_string(),
            "# 見出し1 復帰\n".to_string(),
        ];
        let states = highlighter.compute_line_states(&lines, LineState::Normal);
        assert_eq!(states[1], LineState::Normal);
        let h1_spans = highlighter.highlight_line(&lines[1], states[1]);
        assert_eq!(h1_spans.first().map(|s| s.token), Some(TokenType::Heading1));

        // Inside fenced code block -> Keyword, after closing fence -> back to Normal / Heading1
        let inside_spans = highlighter.highlight_line(&lines[3], states[3]);
        assert_eq!(inside_spans.first().map(|s| s.token), Some(TokenType::Keyword));
        assert_eq!(states[5], LineState::Normal);
        let h1_after_spans = highlighter.highlight_line(&lines[5], states[5]);
        assert_eq!(h1_after_spans.first().map(|s| s.token), Some(TokenType::Heading1));

        // 2. Start-of-line anchor (^) must NOT match mid-line after another token
        let inline_hash = "`code`#not-a-heading\n";
        let spans = highlighter.highlight_line(inline_hash, LineState::Normal);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].token, TokenType::InlineCode);
        assert_eq!(spans[0].byte_range, 0..6);

        // 3. Inline quadruple backticks (```` ````) on a single line must NOT enter a multi-line fence
        let quad_inline_lines = vec![
            "```` ```` この文字列があると壊れる\n".to_string(),
            "# 見出し1\n".to_string(),
            "```` ``` ```` inline triple backticks\n".to_string(),
            "## 見出し2\n".to_string(),
            "```inline``` single line triple backticks\n".to_string(),
            "# 見出し1 まだ正常\n".to_string(),
        ];
        let quad_states = highlighter.compute_line_states(&quad_inline_lines, LineState::Normal);
        assert_eq!(quad_states[1], LineState::Normal);
        assert_eq!(quad_states[3], LineState::Normal);
        assert_eq!(quad_states[5], LineState::Normal);
        assert_eq!(
            highlighter.highlight_line(&quad_inline_lines[1], quad_states[1]).first().map(|s| s.token),
            Some(TokenType::Heading1)
        );
        assert_eq!(
            highlighter.highlight_line(&quad_inline_lines[3], quad_states[3]).first().map(|s| s.token),
            Some(TokenType::Heading2)
        );
        assert_eq!(
            highlighter.highlight_line(&quad_inline_lines[5], quad_states[5]).first().map(|s| s.token),
            Some(TokenType::Heading1)
        );

        // 4. Nested 4-backtick fenced code block containing a 3-backtick code block
        let nested_fence_lines = vec![
            "````markdown\n".to_string(),
            "```rust\n".to_string(),
            "fn main() {}\n".to_string(),
            "```\n".to_string(),
            "````\n".to_string(),
            "# 見出し1 ネスト後\n".to_string(),
        ];
        let nested_states = highlighter.compute_line_states(&nested_fence_lines, LineState::Normal);
        // Lines 1..=4 must stay inside the 4-backtick fence (inner ``` must NOT close it prematurely)
        assert_ne!(nested_states[1], LineState::Normal);
        assert_ne!(nested_states[2], LineState::Normal);
        assert_ne!(nested_states[3], LineState::Normal);
        assert_ne!(nested_states[4], LineState::Normal);
        // Line 5 (after closing ````) must return to Normal and highlight as Heading1
        assert_eq!(nested_states[5], LineState::Normal);
        assert_eq!(
            highlighter.highlight_line(&nested_fence_lines[5], nested_states[5]).first().map(|s| s.token),
            Some(TokenType::Heading1)
        );
    }

    #[test]
    fn test_python_multiline_docstrings_and_escaped_strings() {
        let builtins = SyntaxDefinition::builtins();
        let py_def = builtins.into_iter().find(|b| b.meta.name == "Python").unwrap();
        let hl = SyntaxHighlighter::new(py_def).unwrap();

        let lines = vec![
            "\"\"\"\n".to_string(),
            "docstring line 1\n".to_string(),
            "\"\"\"\n".to_string(),
            "'''\n".to_string(),
            "single-quote docstring\n".to_string(),
            "'''\n".to_string(),
            "x = \"hello \\\"world\\\" # not a comment\"\n".to_string(),
            "def ok():\n".to_string(),
        ];
        let states = hl.compute_line_states(&lines, LineState::Normal);
        assert_ne!(states[1], LineState::Normal, "Inside \"\"\" docstring must be InsideRegion");
        assert_eq!(states[3], LineState::Normal, "After closing \"\"\" must return to Normal");
        assert_ne!(states[4], LineState::Normal, "Inside ''' docstring must be InsideRegion");
        assert_eq!(states[6], LineState::Normal, "After closing ''' must return to Normal");

        // Escaped quotes inside string must keep entire literal as String (no bogus Comment on '#')
        let str_spans = hl.highlight_line(&lines[6], states[6]);
        assert!(str_spans.iter().any(|s| s.token == TokenType::String && s.byte_range == (4..37)));
        assert!(!str_spans.iter().any(|s| s.token == TokenType::Comment));

        // Following function definition must highlight cleanly
        let fn_spans = hl.highlight_line(&lines[7], states[7]);
        assert_eq!(fn_spans[0].token, TokenType::Keyword);
        assert_eq!(fn_spans[1].token, TokenType::Function);
    }

    #[test]
    fn test_go_multiline_raw_strings_and_escapes() {
        let builtins = SyntaxDefinition::builtins();
        let go_def = builtins.into_iter().find(|b| b.meta.name == "Go").unwrap();
        let hl = SyntaxHighlighter::new(go_def).unwrap();

        let lines = vec![
            "s := `\n".to_string(),
            "/* not a block comment */\n".to_string(),
            "`\n".to_string(),
            "msg := \"hello \\\"/*\\\" world\"\n".to_string(),
            "func main() {}\n".to_string(),
        ];
        let states = hl.compute_line_states(&lines, LineState::Normal);
        assert_ne!(states[1], LineState::Normal);
        let inside = hl.highlight_line(&lines[1], states[1]);
        assert_eq!(inside.first().map(|s| s.token), Some(TokenType::String));
        assert_eq!(states[3], LineState::Normal);

        // Escaped quote before /* must NOT start a block comment
        assert_eq!(states[4], LineState::Normal);
        let main_spans = hl.highlight_line(&lines[4], states[4]);
        assert_eq!(main_spans[0].token, TokenType::Keyword);
    }

    #[test]
    fn test_rust_raw_strings_and_escaped_quotes() {
        let builtins = SyntaxDefinition::builtins();
        let rs_def = builtins.into_iter().find(|b| b.meta.name == "Rust").unwrap();
        let hl = SyntaxHighlighter::new(rs_def).unwrap();

        let lines = vec![
            "let a = r##\"contains \"# and /* not comment */\"##;\n".to_string(),
            "let b = r#\"\n".to_string(),
            "multiline \"raw\" /* still string */\n".to_string(),
            "\"#;\n".to_string(),
            "let c = \"escaped \\\"/*\\\" quote\";\n".to_string(),
            "fn main() {}\n".to_string(),
        ];
        let states = hl.compute_line_states(&lines, LineState::Normal);
        assert_eq!(states[1], LineState::Normal, "Single-line r##\"...\"## must close on same line");
        assert_ne!(states[2], LineState::Normal, "Multiline r#\"...\"# must stay InsideRegion");
        let inside = hl.highlight_line(&lines[2], states[2]);
        assert_eq!(inside.first().map(|s| s.token), Some(TokenType::String));
        assert_eq!(states[4], LineState::Normal, "After closing \"# must return to Normal");
        assert_eq!(states[5], LineState::Normal, "Escaped quote before /* must not open comment");
        let fn_spans = hl.highlight_line(&lines[5], states[5]);
        assert_eq!(fn_spans[0].token, TokenType::Keyword);
    }

    #[test]
    fn test_js_ts_multiline_template_literals_with_escape() {
        let builtins = SyntaxDefinition::builtins();
        for lang in ["JavaScript", "TypeScript"] {
            let def = builtins.iter().find(|b| b.meta.name == lang).unwrap().clone();
            let hl = SyntaxHighlighter::new(def).unwrap();

            let lines = vec![
                "const s = `line 1 \\`still template\\`\n".to_string(),
                "/* not a comment */\n".to_string(),
                "`;\n".to_string(),
                "const x = \"a \\\"/*\\\" b\";\n".to_string(),
                "function ok() {}\n".to_string(),
            ];
            let states = hl.compute_line_states(&lines, LineState::Normal);
            assert_ne!(states[1], LineState::Normal, "{lang}: escaped backtick must not close template literal");
            assert_eq!(hl.highlight_line(&lines[1], states[1]).first().map(|s| s.token), Some(TokenType::String));
            assert_eq!(states[3], LineState::Normal, "{lang}: closing backtick must restore Normal");
            assert_eq!(states[4], LineState::Normal, "{lang}: escaped quote before /* must not open comment");
            assert_eq!(hl.highlight_line(&lines[4], states[4])[0].token, TokenType::Keyword);
        }
    }

    #[test]
    fn test_c_cpp_swift_escaped_strings_and_multiline_literals() {
        let builtins = SyntaxDefinition::builtins();

        for lang in ["C", "C++"] {
            let def = builtins.iter().find(|b| b.meta.name == lang).unwrap().clone();
            let hl = SyntaxHighlighter::new(def).unwrap();
            let lines = vec![
                "const char *s = \"escaped \\\"/*\\\" quote\";\n".to_string(),
                "int main() { return 0; }\n".to_string(),
            ];
            let states = hl.compute_line_states(&lines, LineState::Normal);
            assert_eq!(states[1], LineState::Normal, "{lang}: escaped quote before /* must not leak block comment");
            assert_eq!(hl.highlight_line(&lines[1], states[1])[0].token, TokenType::Keyword);
        }

        let swift_def = builtins.iter().find(|b| b.meta.name == "Swift").unwrap().clone();
        let swift_hl = SyntaxHighlighter::new(swift_def).unwrap();
        let swift_lines = vec![
            "let text = \"\"\"\n".to_string(),
            "/* not a comment */\n".to_string(),
            "\"\"\"\n".to_string(),
            "let msg = \"escaped \\\"/*\\\" quote\"\n".to_string(),
            "func run() {}\n".to_string(),
        ];
        let swift_states = swift_hl.compute_line_states(&swift_lines, LineState::Normal);
        assert_ne!(swift_states[1], LineState::Normal, "Swift multiline \"\"\" must stay InsideRegion");
        assert_eq!(swift_hl.highlight_line(&swift_lines[1], swift_states[1])[0].token, TokenType::String);
        assert_eq!(swift_states[3], LineState::Normal);
        assert_eq!(swift_states[4], LineState::Normal);
        assert_eq!(swift_hl.highlight_line(&swift_lines[4], swift_states[4])[0].token, TokenType::Keyword);
    }
}

