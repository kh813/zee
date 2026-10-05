use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub extension: String,
    pub content: String,
    pub is_user: bool,
}

impl Template {
    pub fn new(id: impl Into<String>, name: impl Into<String>, extension: impl Into<String>, content: impl Into<String>, is_user: bool) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            extension: extension.into(),
            content: content.into(),
            is_user,
        }
    }

    /// Expands macros like {filename}, {date}, {year}, {author}, and {cursor}
    /// Returns the expanded text and the character offset of {cursor} (if present).
    pub fn expand(&self, filename: Option<&str>) -> (String, usize) {
        let fn_str = filename.unwrap_or("Untitled");
        let (year, month, day) = current_date();
        let date_str = format!("{:04}-{:02}-{:02}", year, month, day);
        let year_str = format!("{:04}", year);
        let author_str = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "User".to_string());

        let mut expanded = self.content
            .replace("{filename}", fn_str)
            .replace("{date}", &date_str)
            .replace("{year}", &year_str)
            .replace("{author}", &author_str);

        let cursor_marker = "{cursor}";
        let cursor_offset = if let Some(byte_idx) = expanded.find(cursor_marker) {
            // Count characters up to byte_idx
            let char_offset = expanded[..byte_idx].chars().count();
            expanded.replace_range(byte_idx..byte_idx + cursor_marker.len(), "");
            char_offset
        } else {
            expanded.chars().count()
        };

        (expanded, cursor_offset)
    }

    /// Loads all built-in and user-defined templates
    pub fn load_all() -> Vec<Template> {
        let mut list = Self::builtins();
        if let Some(user_dir) = Self::templates_dir() {
            if user_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(user_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                                let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                                let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                                list.push(Template::new(
                                    format!("user_{}", stem),
                                    format!("{} (User)", file_name),
                                    ext,
                                    content,
                                    true,
                                ));
                            }
                        }
                    }
                }
            }
        }
        list
    }

    pub fn templates_dir() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/zee/templates"));
        #[cfg(target_os = "windows")]
        let base = std::env::var_os("APPDATA").map(|h| PathBuf::from(h).join("zee/templates"));
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(|h| PathBuf::from(h).join("zee/templates"))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/zee/templates")));
        base
    }

    pub fn builtins() -> Vec<Template> {
        vec![
            Template::new(
                "rust_bin",
                "Rust Binary (main.rs)",
                ".rs",
                "fn main() {\n    {cursor}println!(\"Hello, world!\");\n}\n",
                false,
            ),
            Template::new(
                "rust_lib",
                "Rust Library (lib.rs)",
                ".rs",
                "pub fn add(left: u64, right: u64) -> u64 {\n    left + right\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn it_works() {\n        let result = add(2, 2);\n        assert_eq!(result, 4);\n    }\n}\n",
                false,
            ),
            Template::new(
                "python",
                "Python Script",
                ".py",
                "#!/usr/bin/env python3\n\"\"\"\n{filename} - Created on {date} by {author}\n\"\"\"\n\ndef main():\n    {cursor}pass\n\nif __name__ == \"__main__\":\n    main()\n",
                false,
            ),
            Template::new(
                "go",
                "Go Program (main.go)",
                ".go",
                "package main\n\nimport \"fmt\"\n\nfunc main() {\n    {cursor}fmt.Println(\"Hello, World!\")\n}\n",
                false,
            ),
            Template::new(
                "html5",
                "HTML5 Document",
                ".html",
                "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n    <meta charset=\"UTF-8\">\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n    <title>{filename}</title>\n</head>\n<body>\n    {cursor}\n</body>\n</html>\n",
                false,
            ),
            Template::new(
                "markdown",
                "Markdown Document",
                ".md",
                "# {filename}\n\nAuthor: {author}\nDate: {date}\n\n## Overview\n\n{cursor}\n",
                false,
            ),
            Template::new(
                "shell",
                "Shell Script (bash)",
                ".sh",
                "#!/usr/bin/env bash\nset -euo pipefail\n\n{cursor}\n",
                false,
            ),
        ]
    }
}

/// Helper function to compute (year, month, day) from system clock without external crate dependencies.
fn current_date() -> (i32, u32, u32) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let days = (now / 86400) as i64;
    // Algorithm based on civil calendar computation from day count since 1970-01-01
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as i32, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_date() {
        let (y, m, d) = current_date();
        assert!(y >= 2024);
        assert!((1..=12).contains(&m));
        assert!((1..=31).contains(&d));
    }

    #[test]
    fn test_template_expansion() {
        let tpl = Template::new(
            "test",
            "Test Template",
            ".rs",
            "// File: {filename}\n// Date: {date}\nfn hello() {\n    {cursor}println!(\"hi\");\n}\n",
            false,
        );

        let (expanded, cursor) = tpl.expand(Some("hello.rs"));
        assert!(expanded.contains("// File: hello.rs"));
        assert!(!expanded.contains("{cursor}"));
        assert!(cursor > 0);
        let expected_line = expanded.lines().nth(3).unwrap();
        assert_eq!(expected_line, "    println!(\"hi\");");
    }

    #[test]
    fn test_builtins_list() {
        let builtins = Template::builtins();
        assert!(builtins.iter().any(|t| t.id == "rust_bin"));
        assert!(builtins.iter().any(|t| t.id == "python"));
        assert!(builtins.iter().any(|t| t.id == "html5"));
    }
}
