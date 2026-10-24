use std::ops::Range;
use regex::RegexBuilder;
use crate::buffer::Editor;

#[derive(Debug, Clone, Default)]
pub struct SearchFlags {
    pub match_case: bool,
    pub whole_word: bool,
    pub use_regex: bool,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub pattern: String,
    pub flags: SearchFlags,
}

#[derive(Debug, Clone)]
pub struct Match {
    pub char_range: Range<usize>,
}

impl Editor {
    pub fn search(&self, query: &SearchQuery) -> Vec<Match> {
        if query.pattern.is_empty() {
            return Vec::new();
        }

        let pattern = if query.flags.use_regex {
            query.pattern.clone()
        } else {
            regex::escape(&query.pattern)
        };

        let pattern = if query.flags.whole_word {
            format!(r"\b{}\b", pattern)
        } else {
            pattern
        };

        let mut builder = RegexBuilder::new(&pattern);
        builder.case_insensitive(!query.flags.match_case);
        builder.multi_line(true);

        let re = match builder.build() {
            Ok(re) => re,
            Err(_) => return Vec::new(), // Invalid regex
        };

        let mut matches = Vec::new();
        let text = self.rope.to_string(); // TODO: Optimize this to avoid full string conversion

        for m in re.find_iter(&text) {
            let start_byte = m.start();
            let end_byte = m.end();
            
            let start_char = self.rope.byte_to_char(start_byte);
            let end_char = self.rope.byte_to_char(end_byte);
            
            matches.push(Match {
                char_range: start_char..end_char,
            });
        }

        matches
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSearchMatch {
    pub path: std::path::PathBuf,
    pub rel_path: String,
    pub line_number: usize, // 1-indexed
    pub col: usize,         // 0-indexed char offset
    pub line_text: String,
}

pub fn search_project<P: AsRef<std::path::Path>>(
    root: P,
    query: &SearchQuery,
    max_results: usize,
) -> Vec<ProjectSearchMatch> {
    if query.pattern.is_empty() {
        return Vec::new();
    }

    let root = root.as_ref();
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let pattern = if query.flags.use_regex {
        query.pattern.clone()
    } else {
        regex::escape(&query.pattern)
    };

    let pattern = if query.flags.whole_word {
        format!(r"\b{}\b", pattern)
    } else {
        pattern
    };

    let mut builder = RegexBuilder::new(&pattern);
    builder.case_insensitive(!query.flags.match_case);
    builder.multi_line(false);

    let re = match builder.build() {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let file_name = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir() {
                !matches!(
                    file_name.as_ref(),
                    ".git" | "target" | "node_modules" | "dist" | "build" | ".idea" | ".vscode" | ".zed"
                )
            } else {
                true
            }
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        // Skip binary extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "ico" | "pdf" | "zip" | "tar" | "gz" | "wasm" | "exe" | "dylib" | "so" | "dll" | "bin"
            ) {
                continue;
            }
        }

        let rel_path = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());

        if let Ok(content) = std::fs::read_to_string(path) {
            for (line_idx, line) in content.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    let col = line[..m.start()].chars().count();
                    results.push(ProjectSearchMatch {
                        path: path.to_path_buf(),
                        rel_path: rel_path.clone(),
                        line_number: line_idx + 1,
                        col,
                        line_text: line.to_string(),
                    });

                    if results.len() >= max_results {
                        return results;
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_project_basic() {
        let temp = std::env::temp_dir().join("zee_test_proj_search");
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        std::fs::write(temp.join("a.rs"), "fn hello_world() {}\n").unwrap();
        std::fs::write(temp.join("b.txt"), "hello there\nsecond line\n").unwrap();

        let query = SearchQuery {
            pattern: "hello".to_string(),
            flags: SearchFlags::default(),
        };

        let results = search_project(&temp, &query, 100);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.line_text.contains("hello_world")));
        assert!(results.iter().any(|r| r.line_text.contains("hello there")));

        let _ = std::fs::remove_dir_all(&temp);
    }
}
