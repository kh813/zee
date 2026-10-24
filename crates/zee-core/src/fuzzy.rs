use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Computes a fuzzy matching score for `pattern` against `candidate`.
/// Returns `None` if `pattern` is not a subsequence of `candidate`.
/// Returns `Some(score)` where higher scores indicate a better match.
pub fn fuzzy_match(pattern: &str, candidate: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    let p_chars: Vec<char> = pattern.to_lowercase().chars().collect();
    let c_chars: Vec<char> = candidate.to_lowercase().chars().collect();

    if p_chars.len() > c_chars.len() {
        return None;
    }

    let mut p_idx = 0;
    let mut score = 0i64;
    let mut prev_match_idx = None;

    // Check exact equality bonus
    if pattern.eq_ignore_ascii_case(candidate) {
        return Some(1000);
    }

    // Check exact suffix (e.g. filename matches) bonus
    if candidate.ends_with(pattern) {
        score += 200;
    }

    for (c_idx, &c) in c_chars.iter().enumerate() {
        if p_idx < p_chars.len() && c == p_chars[p_idx] {
            let mut match_score = 10i64;

            // Consecutive match bonus
            if let Some(prev) = prev_match_idx {
                if c_idx == prev + 1 {
                    match_score += 20;
                }
            }

            // Word boundary bonus (after /, \, _, -, .)
            if c_idx == 0 || matches!(c_chars[c_idx - 1], '/' | '\\' | '_' | '-' | '.' | ' ') {
                match_score += 30;
            }

            // Bonus if match is inside file name rather than directory
            if let Some(last_slash) = candidate.rfind(['/', '\\']) {
                if c_idx > last_slash {
                    match_score += 15;
                }
            }

            score += match_score;
            prev_match_idx = Some(c_idx);
            p_idx += 1;
        }
    }

    if p_idx == p_chars.len() {
        // Penalty for longer candidate paths so shorter/more relevant paths rank higher
        let penalty = (c_chars.len() - p_chars.len()) as i64;
        Some(score - penalty)
    } else {
        None
    }
}

/// Recursively scans `root` for files, skipping common heavy/ignored folders
/// (like .git, target, node_modules, dist, etc.) up to `max_files`.
pub fn index_workspace_files<P: AsRef<Path>>(root: P, max_files: usize) -> Vec<PathBuf> {
    let root = root.as_ref();
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let walker = WalkDir::new(root)
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
        if entry.file_type().is_file() {
            if let Ok(rel) = entry.path().strip_prefix(root) {
                files.push(rel.to_path_buf());
            } else {
                files.push(entry.path().to_path_buf());
            }
            if files.len() >= max_files {
                break;
            }
        }
    }

    files
}

/// Filters and ranks file candidates by fuzzy matching against `pattern`.
pub fn filter_and_rank_files<'a>(
    pattern: &str,
    candidates: &'a [PathBuf],
) -> Vec<(&'a PathBuf, i64)> {
    if pattern.is_empty() {
        return candidates.iter().map(|p| (p, 0i64)).collect();
    }

    let mut scored: Vec<(&'a PathBuf, i64)> = candidates
        .iter()
        .filter_map(|p| {
            let s = p.to_string_lossy();
            fuzzy_match(pattern, &s).map(|score| (p, score))
        })
        .collect();

    // Sort descending by score
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_basic() {
        assert!(fuzzy_match("bf", "buffer.rs").is_some());
        assert!(fuzzy_match("buf", "buffer.rs").is_some());
        assert!(fuzzy_match("xyz", "buffer.rs").is_none());
    }

    #[test]
    fn test_fuzzy_match_ranking() {
        let score_exact = fuzzy_match("main.rs", "main.rs").unwrap();
        let score_nested = fuzzy_match("main.rs", "src/bin/main.rs").unwrap();
        let score_partial = fuzzy_match("main.rs", "domain.rs").unwrap();

        assert!(score_exact > score_nested);
        assert!(score_nested > score_partial);
    }
}
