# AGENT.md - AI Coding Assistant Operating Guidelines

This document outlines mandatory development rules and standards for AI coding assistants working on the `zee` repository.

---

## 1. Mandatory Bugfix Regression Testing (バグ修正時のデグレ防止テスト必須化)

Whenever an issue or bug is investigated and fixed:
1. **Evaluate Testability**: Assess whether automated test code can reproduce the issue or detect future regressions.
2. **Mandatory Test Implementation**: If the issue can be validated through tests, **writing and running corresponding regression tests is mandatory**.
3. **Workspace-Wide Verification**: Always run `cargo test --workspace` and verify that 100% of the tests pass before finishing a task or initiating a release.
4. **No Unverified Fixes**: A bugfix is not complete without passing regression tests, unless technically untestable (e.g., visual icon assets or external CI triggers).

---

## 2. Test Concurrency & Thread-Safety Isolation (テストの並列実行安全性)

Because `cargo test --workspace` executes tests across multiple threads concurrently within each crate test runner:
- **Global Mutex Locks**: Any test that modifies process-wide global state (e.g., `Config::set_custom_config_dir`, `SSH_CONNECTION` environment variables) **must** acquire the dedicated mutex guard:
  - `CONFIG_DIR_LOCK` for config directory overrides
  - `SSH_ENV_LOCK` for SSH environment variables
- **Unique Temp Directories**: Always use process-isolated temp directories (e.g., `std::env::temp_dir().join(format!("zee_test_..._{}", std::process::id()))`) and ensure cleanup after test completion.
- **Session Decoupling**: Do not allow `App::new(vec![])` or `App::with_targets(vec![])` to unexpectedly consume global files like `update_session.json` across concurrent test executions.

---

## 3. Strict Release Targets & Archive Format (通常リリース時の同梱バイナリ制限とZIP統一)

Standard releases (`.github/workflows/release.yml`, triggered by git tag `v*`) must adhere strictly to the following 3 patterns only:
1. **Linux**: CLI only (`zee-cli-v*-linux-x64.zip`)
   - Contains **only a single binary `zee`** (stripped using `strip` to optimize file size).
   - Must **not** include GUI elements or dual binaries (`zeeg`, desktop zip).
2. **macOS**: Apple Silicon GUI only (`zee-v*-macos-arm64.zip` containing `Zee.app`).
3. **Windows**: x64 GUI only (`zee-v*-windows-x64.zip` containing `zee.exe`).

### Unified Archive Format (.zip Only)
- **All release packages across all platforms MUST be `.zip` archives only.**
- **Never produce or upload `.tar.gz` packages** for standard releases. Linux CLI releases must be packaged exclusively as `.zip`.
- Automated regression test `test_release_workflow_matrix_standard_build_only` continuously verifies `.github/workflows/release.yml` and will fail if `.tar.gz` or `tar -` is present.

*Note*: Full-build / extended targets (Linux GUI `zeeg`, macOS CLI, ARM64 builds) are strictly reserved for manual trigger via `.github/workflows/extended-release.yml` and must never be published in regular tag releases.

---

## 4. UI / TUI Standards (UI・キー操作・描画の一貫性)

### Vi Mode Indicators
- Bottom command line (最下行): Displays vi mode and IME status (e.g., `-- INSERT [あ] --`). This indicator must be kept.
- Status bar line (下から2行目): Must **not** display redundant mode badges (e.g., redundant `INSERT [あ]`).

### TUI Ghosting / Afterimage Prevention
- Whenever a modal dialog (Info, About, Settings, File Open, Save) or menu dropdown closes (Esc, Enter, mouse click), `needs_clear` must be set to `true`.
- The renderer must issue SGR Reset (`\x1b[0m`) when cell background transitions to `Color::Reset` to avoid terminal BCE (Background Color Erase) artifacts.
- `Renderer::invalidate()` must be used to force differential repainting on full clear.

### Self-Update & Session Preservation
- Both CLI and GUI must automatically preserve buffer state (all open files, dirty/unsaved scratch content, cursor positions, scroll rows, active tab, sidebar tree) into `UpdateSession` before self-updating.
- Upon relaunch, the application must automatically restore all buffers and UI state.
- CLI must use in-place `exec` (Unix) or spawn (Windows) to automatically restart within the same terminal session.
