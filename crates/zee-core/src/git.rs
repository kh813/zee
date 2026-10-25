use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DEFAULT_GITIGNORE_CONTENT: &str = r#"# ==============================================================================
# OS Generated Files (macOS, Windows, Linux)
# ==============================================================================
# macOS
.DS_Store
.DS_Store?
._*
.Spotlight-V100
.Trashes
.AppleDouble
.LSOverride

# Windows
Thumbs.db
Thumbs.db:encryptable
ehthumbs.db
ehthumbs_vista.db
desktop.ini
$RECYCLE.BIN/

# Linux
*~
.fuse_hidden*
.directory

# ==============================================================================
# Editor & Temporary Files
# ==============================================================================
*.swp
*.swo
*.tmp
*.bak
.zee/

# ==============================================================================
# Build & Cache Artifacts
# ==============================================================================
target/
dist/
node_modules/
"#;

fn base_git_command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.arg("-C").arg(root);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn is_git_installed() -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("--version");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn is_git_repo(root: &Path) -> bool {
    if root.join(".git").exists() {
        return true;
    }
    let mut cmd = base_git_command(root);
    cmd.args(["rev-parse", "--is-inside-work-tree"]);
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn ensure_git_repo(root: &Path) -> Result<()> {
    if !is_git_repo(root) {
        let mut cmd = base_git_command(root);
        cmd.arg("init");
        let output = cmd.output().context("Failed to execute git init")?;
        if !output.status.success() {
            anyhow::bail!("git init failed: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    let gitignore_path = root.join(".gitignore");
    if !gitignore_path.exists() {
        let _ = std::fs::write(&gitignore_path, DEFAULT_GITIGNORE_CONTENT);
    }

    Ok(())
}

pub fn relative_path<'a>(root: &'a Path, target: &'a Path) -> PathBuf {
    target.strip_prefix(root).unwrap_or(target).to_path_buf()
}

pub fn is_path_ignored(root: &Path, target: &Path) -> bool {
    let rel = relative_path(root, target);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let file_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // 1. Direct .gitignore check
    let gitignore_path = root.join(".gitignore");
    if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let pattern = trimmed.trim_start_matches('/');
            if pattern == rel_str || pattern == file_name {
                return true;
            }
            if let Some(stripped) = pattern.strip_suffix('/') {
                if stripped == rel_str || stripped == file_name {
                    return true;
                }
            }
        }
    }

    // 2. git check-ignore
    if is_git_repo(root) {
        let mut cmd = base_git_command(root);
        cmd.args(["check-ignore", &rel_str]);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                return true;
            }
        }
    }

    false
}

pub fn add_to_gitignore(root: &Path, target: &Path) -> Result<()> {
    let rel = relative_path(root, target);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let clean_str = if rel_str.starts_with("./") {
        rel_str[2..].to_string()
    } else {
        rel_str
    };

    let gitignore_path = root.join(".gitignore");
    let existing = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path).unwrap_or_default()
    } else {
        DEFAULT_GITIGNORE_CONTENT.to_string()
    };

    let mut found = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == clean_str || trimmed == format!("/{}", clean_str) {
            found = true;
            break;
        }
    }

    if !found {
        let mut updated = existing;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&clean_str);
        updated.push('\n');
        std::fs::write(&gitignore_path, updated).context("Failed to write .gitignore")?;
    }

    if is_git_repo(root) {
        let mut cmd = base_git_command(root);
        cmd.args(["rm", "--cached", "--quiet", "--", &clean_str]);
        let _ = cmd.output();
    }

    Ok(())
}

pub fn remove_from_gitignore(root: &Path, target: &Path) -> Result<()> {
    let rel = relative_path(root, target);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let clean_str = if rel_str.starts_with("./") {
        rel_str[2..].to_string()
    } else {
        rel_str
    };

    let gitignore_path = root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(());
    }

    let existing = std::fs::read_to_string(&gitignore_path).context("Failed to read .gitignore")?;
    let mut lines = Vec::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == clean_str || trimmed == format!("/{}", clean_str) {
            continue;
        }
        lines.push(line);
    }

    let mut updated = lines.join("\n");
    if !updated.is_empty() {
        updated.push('\n');
    }
    std::fs::write(&gitignore_path, updated).context("Failed to update .gitignore")?;

    Ok(())
}

pub fn git_add(root: &Path, target: Option<&Path>) -> Result<()> {
    ensure_git_repo(root)?;

    let mut cmd = base_git_command(root);
    if let Some(t) = target {
        let rel = relative_path(root, t);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        cmd.args(["add", "--", &rel_str]);
    } else {
        cmd.args(["add", "-A"]);
    }

    let output = cmd.output().context("Failed to run git add")?;
    if !output.status.success() {
        anyhow::bail!("git add failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}

pub fn git_snapshot(root: &Path, custom_msg: Option<&str>) -> Result<String> {
    ensure_git_repo(root)?;

    // 1. Stage all files
    let mut add_cmd = base_git_command(root);
    add_cmd.args(["add", "-A"]);
    let _ = add_cmd.output();

    // 2. Check status --porcelain
    let mut status_cmd = base_git_command(root);
    status_cmd.args(["status", "--porcelain"]);
    let status_out = status_cmd.output().context("Failed to check git status")?;
    let status_str = String::from_utf8_lossy(&status_out.stdout);

    if status_str.trim().is_empty() {
        return Ok("変更はありません（ワーキングツリーは最新のスナップショットと一致しています）".to_string());
    }

    // 3. Generate commit message
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let msg = match custom_msg {
        Some(m) if !m.trim().is_empty() => format!("Snapshot: {} ({})", m.trim(), now),
        _ => {
            let folder_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("Project");
            format!("Snapshot: [{}] {}", folder_name, now)
        }
    };

    // 4. Commit
    let mut commit_cmd = base_git_command(root);
    commit_cmd.args(["commit", "-m", &msg]);
    let output = commit_cmd.output().context("Failed to run git commit")?;
    if !output.status.success() {
        anyhow::bail!("git commit failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let out_str = String::from_utf8_lossy(&output.stdout);
    let first_line = out_str.lines().next().unwrap_or("").trim();
    Ok(format!("📸 スナップショットを作成しました:\n{}\n{}", msg, first_line))
}

pub fn git_status(root: &Path) -> Result<String> {
    ensure_git_repo(root)?;

    let mut cmd = base_git_command(root);
    cmd.args(["status", "-s", "-b"]);
    let output = cmd.output().context("Failed to run git status")?;
    let short_str = String::from_utf8_lossy(&output.stdout);

    let mut branch_cmd = base_git_command(root);
    branch_cmd.args(["branch", "--show-current"]);
    let b_out = branch_cmd.output().ok();
    let current_branch = b_out
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "main".to_string());

    if short_str.lines().count() <= 1 {
        Ok(format!("ブランチ: {}\n\nクリーン（未コミットの変更はありません）", current_branch))
    } else {
        Ok(format!("ブランチ: {}\n\n変更されたファイル:\n{}", current_branch, short_str.trim()))
    }
}

pub fn git_history(root: &Path, max_count: usize) -> Result<String> {
    ensure_git_repo(root)?;

    let mut cmd = base_git_command(root);
    cmd.args(["log", "--oneline", "-n", &max_count.to_string()]);
    let output = cmd.output().context("Failed to run git log")?;
    let hist = String::from_utf8_lossy(&output.stdout);

    if hist.trim().is_empty() {
        Ok("まだスナップショット（コミット履歴）がありません。".to_string())
    } else {
        Ok(hist.trim().to_string())
    }
}

pub fn git_restore_latest(root: &Path, target: Option<&Path>) -> Result<String> {
    ensure_git_repo(root)?;

    let mut cmd = base_git_command(root);
    if let Some(t) = target {
        let rel = relative_path(root, t);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        cmd.args(["checkout", "HEAD", "--", &rel_str]);
    } else {
        cmd.args(["checkout", "HEAD", "--", "."]);
    }

    let output = cmd.output().context("Failed to run git checkout HEAD")?;
    if !output.status.success() {
        anyhow::bail!("git restore failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok("最新のスナップショット（HEAD）に復元しました。".to_string())
}

pub fn get_current_branch(root: &Path) -> String {
    let mut cmd = base_git_command(root);
    cmd.args(["branch", "--show-current"]);
    if let Ok(output) = cmd.output() {
        let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !b.is_empty() {
            return b;
        }
    }
    "main".to_string()
}

pub fn get_main_branch(root: &Path) -> String {
    let mut cmd = base_git_command(root);
    cmd.args(["rev-parse", "--verify", "refs/heads/main"]);
    if let Ok(output) = cmd.output() {
        if output.status.success() {
            return "main".to_string();
        }
    }
    let mut cmd2 = base_git_command(root);
    cmd2.args(["rev-parse", "--verify", "refs/heads/master"]);
    if let Ok(output) = cmd2.output() {
        if output.status.success() {
            return "master".to_string();
        }
    }
    "main".to_string()
}

pub fn git_new_draft(root: &Path, name: Option<&str>) -> Result<String> {
    ensure_git_repo(root)?;

    // Auto-save any current changes
    let _ = git_snapshot(root, Some("Pre-draft auto snapshot"));

    let timestamp = chrono::Local::now().format("%m%d-%H%M").to_string();
    let branch_name = match name {
        Some(n) if !n.trim().is_empty() => {
            let clean = n.trim().replace(' ', "-");
            if clean.starts_with("draft/") {
                clean
            } else {
                format!("draft/{}", clean)
            }
        }
        _ => format!("draft/{}", timestamp),
    };

    let mut cmd = base_git_command(root);
    cmd.args(["checkout", "-b", &branch_name]);
    let output = cmd.output().context("Failed to checkout new draft branch")?;
    if !output.status.success() {
        anyhow::bail!("ドラフトブランチ作成に失敗しました: {}", String::from_utf8_lossy(&output.stderr));
    }

    let mut cfg_cmd = base_git_command(root);
    cfg_cmd.args(["config", "--local", "git-easy.last-draft", &branch_name]);
    let _ = cfg_cmd.output();

    Ok(format!("🌱 ドラフトブランチ '{}' を作成し、切り替えました。", branch_name))
}

pub fn git_merge_draft(root: &Path) -> Result<String> {
    ensure_git_repo(root)?;

    let current = get_current_branch(root);
    let main_branch = get_main_branch(root);

    if !current.starts_with("draft/") {
        anyhow::bail!("現在はドラフトブランチ上ではありません（現在: {}）\nドラフトブランチ（draft/...）に切り替えてからマージを実行してください。", current);
    }

    // Auto-save any changes in draft
    let _ = git_snapshot(root, Some("Pre-merge auto snapshot"));

    // Switch to main
    let mut switch_cmd = base_git_command(root);
    switch_cmd.args(["checkout", &main_branch]);
    let s_out = switch_cmd.output().context("Failed to switch to main branch")?;
    if !s_out.status.success() {
        anyhow::bail!("{} への切り替えに失敗しました: {}", main_branch, String::from_utf8_lossy(&s_out.stderr));
    }

    // Merge draft
    let merge_msg = format!("Apply draft '{}' into {}", current, main_branch);
    let mut merge_cmd = base_git_command(root);
    merge_cmd.args(["merge", "--no-ff", &current, "-m", &merge_msg]);
    let m_out = merge_cmd.output().context("Failed to merge draft")?;
    if !m_out.status.success() {
        anyhow::bail!("マージに失敗しました: {}", String::from_utf8_lossy(&m_out.stderr));
    }

    Ok(format!("🔀 ドラフト '{}' を {} にマージしました。", current, main_branch))
}

pub fn git_switch_draft(root: &Path) -> Result<String> {
    ensure_git_repo(root)?;

    let current = get_current_branch(root);
    let main_branch = get_main_branch(root);

    // Auto-save
    let _ = git_snapshot(root, Some("Pre-switch auto snapshot"));

    if current.starts_with("draft/") {
        // Save as last-draft and switch back to main
        let mut cfg_cmd = base_git_command(root);
        cfg_cmd.args(["config", "--local", "git-easy.last-draft", &current]);
        let _ = cfg_cmd.output();

        let mut cmd = base_git_command(root);
        cmd.args(["checkout", &main_branch]);
        let out = cmd.output().context("Failed to switch to main branch")?;
        if !out.status.success() {
            anyhow::bail!("{} への切り替えに失敗しました: {}", main_branch, String::from_utf8_lossy(&out.stderr));
        }
        Ok(format!("🔄 メインブランチ '{}' に切り替えました。", main_branch))
    } else {
        // Find last-draft or most recent draft
        let mut cfg_cmd = base_git_command(root);
        cfg_cmd.args(["config", "--local", "git-easy.last-draft"]);
        let last = cfg_cmd.output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());

        let target_branch = match last {
            Some(b) => b,
            None => {
                let mut list_cmd = base_git_command(root);
                list_cmd.args(["branch", "--list", "draft/*", "--sort=-committerdate"]);
                let list_out = list_cmd.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
                let first = list_out.lines().map(|l| l.trim().trim_start_matches('*').trim().to_string()).find(|s| !s.is_empty());
                match first {
                    Some(b) => b,
                    None => {
                        // No draft exists, create a new one
                        return git_new_draft(root, None);
                    }
                }
            }
        };

        let mut cmd = base_git_command(root);
        cmd.args(["checkout", &target_branch]);
        let out = cmd.output().context("Failed to checkout draft branch")?;
        if !out.status.success() {
            anyhow::bail!("'{}' への切り替えに失敗しました: {}", target_branch, String::from_utf8_lossy(&out.stderr));
        }
        Ok(format!("🔄 ドラフトブランチ '{}' に切り替えました。", target_branch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_ignore_management() {
        let temp_dir = std::env::temp_dir().join(format!("zee_test_git_mod_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let file = temp_dir.join("test_note.md");
        std::fs::write(&file, "Hello").unwrap();

        assert!(!is_path_ignored(&temp_dir, &file));

        add_to_gitignore(&temp_dir, &file).unwrap();
        assert!(is_path_ignored(&temp_dir, &file));

        remove_from_gitignore(&temp_dir, &file).unwrap();
        assert!(!is_path_ignored(&temp_dir, &file));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
