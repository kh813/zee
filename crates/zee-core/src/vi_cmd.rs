//! Ex command parser and data types for vi/vim mode.

use crate::buffer::normalize_vi_char;

/// Represents a parsed Ex command (e.g. `:w`, `:q!`, `:42`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ExCommand {
    /// `:w[!] [path]` - Write buffer to file.
    Write { path: Option<String>, force: bool },
    /// `:q[!]` - Quit current window / tab.
    Quit { force: bool },
    /// `:wq[!] [path]` or `:x[!] [path]` - Write and quit.
    WriteQuit { path: Option<String>, force: bool },
    /// `:qa[!]` - Quit all tabs.
    QuitAll { force: bool },
    /// `:wqa[!]` - Write and quit all tabs.
    WriteQuitAll { force: bool },
    /// `:e[!] [path]` - Edit / reload file.
    Edit { path: Option<String>, force: bool },
    /// `:<number>` - Jump to line number (1-based).
    GoToLine(usize),
    /// `:noh` or `:nohlsearch` - Clear search highlight.
    NoHighlight,
    /// `:bn` or `:bnext` - Next buffer/tab.
    BufferNext,
    /// `:bp` or `:bprev` - Previous buffer/tab.
    BufferPrev,
    /// `:set [option]` - Set option (e.g., `nu`, `nonu`, `wrap`, `nowrap`).
    Set { option: String, value: Option<String> },
    /// Empty command (e.g. `:` alone).
    Empty,
    /// Unknown or unsupported command string.
    Unknown(String),
}

/// Parses an Ex command string typed in vi command mode.
///
/// Handles full-width characters (IME input) and leading colons.
pub fn parse_ex_command(input: &str) -> ExCommand {
    // Normalize any full-width Japanese characters to ASCII
    let normalized: String = input.chars().map(normalize_vi_char).collect();
    let trimmed = normalized.trim();

    // Strip leading colon(s)
    let body = trimmed.trim_start_matches(':').trim();

    if body.is_empty() {
        return ExCommand::Empty;
    }

    // Check if it's purely a line number jump (e.g. ":42", ":100")
    if let Ok(line_num) = body.parse::<usize>() {
        return ExCommand::GoToLine(line_num);
    }

    // Split into command name and arguments
    let mut parts = body.split_whitespace();
    let cmd_name = parts.next().unwrap_or("");
    let arg = parts.next().map(|s| s.to_string());

    match cmd_name {
        // --- Write / Save ---
        "w" => ExCommand::Write { path: arg, force: false },
        "w!" => ExCommand::Write { path: arg, force: true },

        // --- Quit ---
        "q" => ExCommand::Quit { force: false },
        "q!" => ExCommand::Quit { force: true },

        // --- Write & Quit ---
        "wq" | "x" => ExCommand::WriteQuit { path: arg, force: false },
        "wq!" | "x!" => ExCommand::WriteQuit { path: arg, force: true },

        // --- Quit All ---
        "qa" => ExCommand::QuitAll { force: false },
        "qa!" => ExCommand::QuitAll { force: true },

        // --- Write & Quit All ---
        "wqa" => ExCommand::WriteQuitAll { force: false },
        "wqa!" => ExCommand::WriteQuitAll { force: true },

        // --- Edit / Reload ---
        "e" | "edit" => ExCommand::Edit { path: arg, force: false },
        "e!" | "edit!" => ExCommand::Edit { path: arg, force: true },

        // --- Clear search highlight ---
        "noh" | "nohl" | "nohlsearch" => ExCommand::NoHighlight,

        // --- Buffer navigation ---
        "bn" | "bnext" => ExCommand::BufferNext,
        "bp" | "bprev" | "bprevious" => ExCommand::BufferPrev,

        // --- Settings (:set ...) ---
        "set" | "se" => {
            if let Some(opt_str) = arg {
                if let Some((k, v)) = opt_str.split_once('=') {
                    ExCommand::Set {
                        option: k.to_string(),
                        value: Some(v.to_string()),
                    }
                } else {
                    ExCommand::Set {
                        option: opt_str,
                        value: None,
                    }
                }
            } else {
                ExCommand::Set {
                    option: String::new(),
                    value: None,
                }
            }
        }

        // Unknown command
        _ => ExCommand::Unknown(body.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        assert_eq!(parse_ex_command(""), ExCommand::Empty);
        assert_eq!(parse_ex_command(":"), ExCommand::Empty);
        assert_eq!(parse_ex_command(":::"), ExCommand::Empty);
        assert_eq!(parse_ex_command("  :   "), ExCommand::Empty);
    }

    #[test]
    fn test_parse_line_number() {
        assert_eq!(parse_ex_command(":42"), ExCommand::GoToLine(42));
        assert_eq!(parse_ex_command("100"), ExCommand::GoToLine(100));
        assert_eq!(parse_ex_command("：１２３"), ExCommand::GoToLine(123));
    }

    #[test]
    fn test_parse_write_and_quit() {
        assert_eq!(parse_ex_command(":w"), ExCommand::Write { path: None, force: false });
        assert_eq!(parse_ex_command(":w!"), ExCommand::Write { path: None, force: true });
        assert_eq!(parse_ex_command(":w foo.txt"), ExCommand::Write { path: Some("foo.txt".into()), force: false });
        assert_eq!(parse_ex_command(":w! bar.txt"), ExCommand::Write { path: Some("bar.txt".into()), force: true });

        assert_eq!(parse_ex_command(":q"), ExCommand::Quit { force: false });
        assert_eq!(parse_ex_command(":q!"), ExCommand::Quit { force: true });

        assert_eq!(parse_ex_command(":wq"), ExCommand::WriteQuit { path: None, force: false });
        assert_eq!(parse_ex_command(":wq!"), ExCommand::WriteQuit { path: None, force: true });
        assert_eq!(parse_ex_command(":x"), ExCommand::WriteQuit { path: None, force: false });

        assert_eq!(parse_ex_command(":qa"), ExCommand::QuitAll { force: false });
        assert_eq!(parse_ex_command(":qa!"), ExCommand::QuitAll { force: true });
        assert_eq!(parse_ex_command(":wqa"), ExCommand::WriteQuitAll { force: false });
        assert_eq!(parse_ex_command(":wqa!"), ExCommand::WriteQuitAll { force: true });
    }

    #[test]
    fn test_parse_edit_and_buffer() {
        assert_eq!(parse_ex_command(":e"), ExCommand::Edit { path: None, force: false });
        assert_eq!(parse_ex_command(":e!"), ExCommand::Edit { path: None, force: true });
        assert_eq!(parse_ex_command(":e main.rs"), ExCommand::Edit { path: Some("main.rs".into()), force: false });

        assert_eq!(parse_ex_command(":noh"), ExCommand::NoHighlight);
        assert_eq!(parse_ex_command(":nohlsearch"), ExCommand::NoHighlight);

        assert_eq!(parse_ex_command(":bn"), ExCommand::BufferNext);
        assert_eq!(parse_ex_command(":bnext"), ExCommand::BufferNext);
        assert_eq!(parse_ex_command(":bp"), ExCommand::BufferPrev);
        assert_eq!(parse_ex_command(":bprev"), ExCommand::BufferPrev);
    }

    #[test]
    fn test_parse_set() {
        assert_eq!(parse_ex_command(":set nu"), ExCommand::Set { option: "nu".into(), value: None });
        assert_eq!(parse_ex_command(":set number"), ExCommand::Set { option: "number".into(), value: None });
        assert_eq!(parse_ex_command(":set nonu"), ExCommand::Set { option: "nonu".into(), value: None });
        assert_eq!(parse_ex_command(":set tabstop=4"), ExCommand::Set { option: "tabstop".into(), value: Some("4".into()) });
    }

    #[test]
    fn test_parse_ime_full_width() {
        assert_eq!(parse_ex_command("：ｗ"), ExCommand::Write { path: None, force: false });
        assert_eq!(parse_ex_command("：ｑ！"), ExCommand::Quit { force: true });
        assert_eq!(parse_ex_command("：ｗｑ"), ExCommand::WriteQuit { path: None, force: false });
    }

    #[test]
    fn test_parse_unknown() {
        assert_eq!(parse_ex_command(":foo bar"), ExCommand::Unknown("foo bar".into()));
    }
}
