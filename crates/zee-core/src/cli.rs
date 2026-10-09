use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTarget {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

/// Parse command-line file arguments into `FileTarget` structures.
///
/// Supports:
/// - Plain paths: `file.rs`
/// - Colon line numbers: `file.rs:42`
/// - Colon line and column: `file.rs:42:15`
/// - Plus line syntax: `+42 file.rs` or `file.rs +42`
pub fn parse_file_targets(args: &[String]) -> Vec<FileTarget> {
    let mut targets: Vec<FileTarget> = Vec::new();
    let mut pending_line = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Skip command-line options/flags like --gui, -g, -v, --help, etc.
        if arg.starts_with('-') && arg != "-" {
            i += 1;
            continue;
        }

        // Format: +<line> (e.g. +42)
        if arg.starts_with('+') && arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit()) {
            if let Ok(line) = arg[1..].parse::<usize>() {
                if let Some(last) = targets.last_mut() {
                    if last.line.is_none() {
                        last.line = Some(line);
                        i += 1;
                        continue;
                    }
                }
                pending_line = Some(line);
                i += 1;
                continue;
            }
        }

        // If path exists on disk as-is, keep it intact (could contain unusual chars)
        let path_obj = PathBuf::from(arg);
        if path_obj.exists() {
            targets.push(FileTarget {
                path: path_obj,
                line: pending_line.take(),
                col: None,
            });
            i += 1;
            continue;
        }

        // Try parsing path:line:col or path:line
        let parts: Vec<&str> = arg.rsplitn(3, ':').collect();
        if parts.len() >= 2 {
            if parts.len() == 3 {
                if let (Ok(line), Ok(col)) = (parts[1].parse::<usize>(), parts[0].parse::<usize>()) {
                    let path = PathBuf::from(parts[2]);
                    targets.push(FileTarget {
                        path,
                        line: Some(line),
                        col: Some(col),
                    });
                    pending_line = None;
                    i += 1;
                    continue;
                }
            }
            if let Ok(line) = parts[0].parse::<usize>() {
                let path = PathBuf::from(parts[1]);
                targets.push(FileTarget {
                    path,
                    line: Some(line),
                    col: None,
                });
                pending_line = None;
                i += 1;
                continue;
            }
        }

        targets.push(FileTarget {
            path: path_obj,
            line: pending_line.take(),
            col: None,
        });
        i += 1;
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_targets() {
        let args = vec!["src/main.rs".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(targets[0].line, None);
        assert_eq!(targets[0].col, None);

        let args = vec!["src/main.rs:42".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(targets[0].line, Some(42));
        assert_eq!(targets[0].col, None);

        let args = vec!["src/main.rs:42:15".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(targets[0].line, Some(42));
        assert_eq!(targets[0].col, Some(15));

        let args = vec!["+100".to_string(), "foo.rs".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("foo.rs"));
        assert_eq!(targets[0].line, Some(100));

        let args = vec!["bar.rs".to_string(), "+200".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("bar.rs"));
        assert_eq!(targets[0].line, Some(200));

        let args = vec!["--gui".to_string(), "main.rs:10".to_string(), "-g".to_string()];
        let targets = parse_file_targets(&args);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, PathBuf::from("main.rs"));
        assert_eq!(targets[0].line, Some(10));
    }
}
