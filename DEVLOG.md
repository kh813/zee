# zee Devlog

## 2026-10-06

### Vi Command Mode Japanese IME Inline Input & Robust Test Suite (GUI & CLI / TUI)
- **Inline Japanese IME Support in Vi Command Line (`: %s/...`)**:
  - Fixed an issue where Japanese text could not be typed inline in the vi command line (Ex commands like `:%s/日本語/Japanese/g`) and conversion candidate windows popped up outside the window on macOS and Windows.
  - Enabled `accepts_text_input` in `EditorView::EntityInputHandler` during vi command mode (`workspace.vi_cmd.is_some()`), allowing macOS Cocoa and Windows IMM32/TSF to recognize active text input.
  - Updated `replace_and_mark_text_in_range` and `replace_text_in_range` to route preedit text into `workspace.vi_cmd_preedit` and committed characters directly into `workspace.vi_cmd`.
  - Added bottom status bar command row bounds calculation in `EditorView::bounds_for_range` so IME candidate popup windows anchor precisely beneath the command line cursor.
  - Rendered inline preedit composition with underline highlight in `StatusBar` vi command bar.
  - Added hardware cursor positioning in `zee-tui` when `is_vi_cmd_mode` is active so terminal IMEs position conversion popups at the command line.
  - Fixed Backspace handling for multi-byte UTF-8 Japanese characters in both GUI and TUI vi command line.
- **Comprehensive Vi Mode & Non-Vi Mode Editor Test Suite**:
  - Added unit and integration tests covering Japanese substitution (`:%s/日本語/Japanese/g`) and full-width IME syntax tokens (`：％ｓ／日本語／Japanese／ｇ`).
  - Added test coverage for `vi_cmd` and `vi_cmd_preedit` state transitions, multi-byte backspace, and terminal hardware cursor positioning.
  - Added extensive tests for standard text editing (typing, navigation, deletion, clipboard, selection, menu toggles, and multi-tab cycling) with vi mode disabled to ensure zero regressions across all platforms.

## 2026-09-14

### GUI Font/Spacing Customization, Syntax Highlighting & Theme System Enhancement (GUI & CLI)
- **Natural Browser-style New Tab / New Window Shortcuts & Window Cascading**:
  - Implemented cascading offset (28px diagonal step) for new windows so overlapping windows are easily recognizable when opening multiple windows.
  - Updated keyboard shortcuts to browser-standard conventions: `Cmd+T` / `Ctrl+T` for **New Tab**, and `Cmd+N` / `Ctrl+N` for **New Window** (in GUI).
  - Implemented window close on `Cmd+W` / `Ctrl+W` when no tabs remain open in the workspace (`workspace.editors.is_empty()`), closing the window immediately without confirmation dialogs.
  - Implemented global action handlers for `NewTab` (Cmd+T), `NewWindow` (Cmd+N), and `New` so pressing shortcuts when no windows are open reliably opens a new window with a new tab.
  - Fixed an issue where `Cmd+T` failed to open a new tab due to empty scratch buffer replacement logic overwriting the existing buffer instead of adding a new tab. Added dedicated `Workspace::new_tab()`.
  - Added unit tests (`test_new_tab_always_adds_new_tab`, `test_add_editor_untitled_always_adds_tab`, `test_close_active_editor_until_empty`) ensuring `new_tab()` and tab management remain fully robust.
  - Updated File menu items (`New Tab` / `新規タブ`, `New Window` / `新規ウィンドウ`), empty workspace keyboard hints (`⌘T: New Tab`, `⌘N: New Window`, `⌘O: Open File`), TUI shortcuts (`Ctrl+T` / `Ctrl+N`), and documentation (`MANUAL.md`, `app_specs.md`).
- **macOS App Bundle Launch & Reopen Fix**:
  - Removed explicit `NSPrincipalClass` from `Info.plist` to allow GPUI's runtime `GPUIApplication` subclass to properly bind and handle window lifecycle events.
  - Implemented `app.on_reopen` in `led-gui` to automatically spawn a new window when `open dist/led.app` or dock activation occurs with no open windows.
- **Automatic Syntax Highlighting on File Open & Drag-and-Drop**:
  - Automatically detect language syntax definitions (Markdown, Rust, Python, JavaScript, CSS, HTML, Go, TOML, Swift, XML) on file open in `Editor::from_file` and on Save As (`save_as`).
  - Pre-tokenizes lines on load so syntax colors (headings, bold, inline/fenced code blocks, links in Markdown, keywords, types, strings, comments in code) are immediately rendered in both GUI and TUI without manual syntax selection.
  - Fixed a string slice panic in `EditorView::render_line_content` and `render_wrapped_line` by clamping token byte ranges to stripped line lengths and ensuring safe UTF-8 character boundary alignment.
- **GUI Preferences / Settings Dialog (`Cmd+,`)**:
  - Implemented interactive floating modal settings card in `led-gui`.
  - Supports live switching and instant preview of Themes (built-in and custom), Font Family presets (`System Default`, `Menlo`, `SF Mono`, `Fira Code`, `JetBrains Mono`, `Courier New`), Font Size stepper, Line Height stepper, UI Font Size stepper, Tab Width (`2`, `4`, `8`), and toggles for `Line Numbers`, `Word Wrap`, and `Spaces/Tabs`.
  - Added "Reset to Defaults" button to restore stock editor preferences.
  - Changes instantly persist to `~/.config/led/config.toml`.
- **Zoom In / Zoom Out / Reset Zoom (`Cmd+=`, `Cmd+-`, `Cmd+0`)**:
  - Added native menu items and keyboard shortcuts to quickly adjust editor text size and line spacing.
- **Unsaved Changes Dialog Fix on Cmd+Q**:
  - Fixed standard ASCII keystroke dispatch in `EditorView::handle_key_down` so direct character typing marks buffers as modified and records undo history properly.
  - Ensures Cmd+Q (`Quit`) and window close actions consistently prompt the Unsaved Changes confirmation dialog.
- **Theme System & Human-Readable CSS Colors**:
  - Expanded `Color` in `led-core` to parse CSS hex (`#rgb`, `#rrggbb`, `#rrggbbaa`), `rgb(...)`, `rgba(...)`, `ansi(...)` (numeric or named colors), and standard named colors (`black`, `white`, etc.).
  - Added dynamic theme discovery via `Theme::load_all()` and `Theme::find_by_name()` from `~/.config/led/themes/*.toml`.
  - Dynamically populated native menus in `led-gui` and TUI theme submenus in `led-tui`.
- **TabBar Close (`×`) & Empty State Support**:
  - Fixed an issue where the last remaining unmodified tab could not be closed. Closing the last tab now properly removes it from `TabBar` while keeping the window open.
  - Implemented an elegant empty workspace view in `EditorView` and `StatusBar` with keyboard shortcut hints (`⌘N: New Tab`, `⌘O: Open File`).
  - Added clean empty buffer replacement logic: when opening a new file while the workspace contains an unmodified/untitled scratch buffer, the unmodified tab is automatically replaced by the opened file instead of leaving an unused empty tab.
- **Local Build & GitHub Actions CI/CD Workflows**:
  - Unified `Makefile` to automatically detect host OS (macOS, Linux, Windows) with targets for `make` (build all for host OS), `make tui`, `make gui` (bundles `led.app` on macOS), `make test`, `make check`, `make install`, `make package`, and `make clean`.
  - Added `.github/workflows/ci.yml` for multi-OS CI testing and checks on PRs and pushes to `main`.
  - Added `.github/workflows/release.yml` for multi-platform binary compilation (macOS ARM64/Intel, Linux x64, Windows x64), checksum generation, and automated GitHub Release publishing on version tags (`v*`).
- **Config & Documentation**:
  - Created `assets/config.toml.default` and refreshed root `config.toml.default` documenting all keys.
  - Updated `app_specs.md`, `MANUAL.md`, and `app_todo.md`.

## 2026-05-14

### GUI Modernization & Visual Polish (GUI)
- **Typography & Font Separation**: Separated UI proportional font (`.AppleSystemUIFont` on macOS, sans-serif on others) from editor monospace font (`Menlo` / `monospace`). Tab labels, status bar indicators, search toolbar, and modal dialogs now render with crisp native system UI typography.
- **Modern TabBar**: Redesigned tabs with rounded pill shapes, subtle hover highlights, clean active tab borders, uncommitted dirty dot indicator (`●`), hover-responsive close button (`×`), and a New Tab (`+`) button.
- **Modern StatusBar**: Segmented pill layout with hover feedback, vi mode badge pill, and interactive click handlers (clicking Line/Col opens Go to Line).
- **Find/Replace Toolbar**: Modernized input boxes with focus rings, placeholder text, search match counters (`3 of 12`), and compact pill toggle buttons for Match Case (`Aa`), Whole Word (`\b`), and Regex (`.*`).
- **Modal Dialog Cards**: Updated About, Go to Line, and Unsaved Changes dialogs to modern floating card modals with rounded corners (`rounded-xl`), elevated drop shadows, and primary/secondary button states.
- **Editor Overlay Scrollbar**: Added smooth semi-transparent overlay scrollbar with hover states for viewport navigation.

## 2026-05-11

### GUI Stability & UX (GUI)
- **GUI Crash Fix (SIGABRT)**: Resolved a crash caused by recursive action dispatch and re-entrant window updates in the `Quit` and `Exit` handlers.
- **Centralized Quit Logic**: Refactored the multi-window quit coordination to the application level in `app.rs`. `WindowView` no longer listens for `Quit` or `Exit` actions directly; instead, the global handler coordinates checking each window for modified buffers and prompting the user sequentially.
- **Focus Restoration**: Fixed a bug where focus was not properly restored to the editor after closing a dialog. Now explicitly focusing the editor's `FocusHandle` in all dialog close paths.
- **Window Centering & Theme Detection**: Implemented automatic window centering on the primary display and added OS light/dark mode detection for the default theme.

## 2026-05-10

### GUI Rendering & Documentation (GUI)
- **GUI Visibility**: Applied robust rendering fixes to `EditorView`. Text chunks now use `h_full` and `items_center` within a fixed-height flex-row to ensure vertical visibility. Explicitly set `font_family("Menlo")` for consistent monospace rendering.
- **Horizontal Scrolling**: Refactored `render_line` to use an absolute positioned wrapper for the content area, fixing horizontal scroll offset application.
- **Documentation**: Updated `app_specs.md` with detailed GUI rendering implementation requirements and refreshed `app_todo.md`.
    - **GUI**: Updated `bounds_for_range` and `render_line` to use visual column calculations (via `unicode-width`) instead of raw character indices. This ensures the IME candidate window appears at the correct cursor position for CJK text.
    - **TUI**: Added hardware cursor movement to the logical cursor position after each render pass, allowing terminal emulators to properly position the IME candidate window.

### GUI Bug Fixes & UX Improvements
- **GUI Re-entrancy Crash Fix**: Fixed a `panic_already_borrowed` crash caused by synchronous file dialogs blocking the GPUI event loop. Replaced `rfd` synchronous calls with `AsyncFileDialog` and `cx.spawn` in both `app.rs` and `window_view.rs`.
- **Japanese Inline IME Support**: Implemented `marked_text_range` in `EditorView` and refined composition handling to support native inline input on macOS.
- **Color Consistency**: Unified `led_color_to_gpui` across all widgets to ensure alpha is explicitly set to 1.0, avoiding accidental transparency issues. Replaced hardcoded colors in `FindPanel` with theme-aware colors.
- **Standard Shortcuts**: Added missing macOS shortcuts (`Cmd+C`, `Cmd+V`, `Cmd+X`, `Cmd+Shift+Z`) to ensure full platform parity and resolve non-functional shortcut reports.
- **App Lifecycle & Menu State**: Moved action handlers for `New`, `Open`, `About`, and `Quit` to the application level. This ensures the menu remains enabled even when all windows are closed, and allows reopening the app via the menu.
- **Improved Dialogs**: Updated `WindowView` to ensure `About` dialog can be triggered globally and improved background dimming for modal dialogs.

## 2026-05-08
...
### Completed Phase 13: led-gui — gpui Setup & Window Skeleton
- Pinned `gpui` to commit `6766514`.
- Implemented `led-gui` entry point and main application loop.
- Set up platform-conditional menu system: native NSMenu for macOS, placeholder in-window menu bar for others.
- Created `WindowView` root component that composes child views based on platform.
- Implemented stubs for `EditorView`, `TabBar`, `StatusBar`, `FindPanel`, and `MenuBar`.
- Resolved macOS build issues by implementing an `xcrun` shim that redirects Metal tool calls to the correct toolchain path.
- Updated `Makefile` to integrate shims and ensure stable builds for both TUI and GUI targets.

### Completed Phase 12: i18n & Final Polish
- Implemented full i18n framework in `led-core`.
- Added built-in Japanese (`ja`) locale and support for external TOML locales.
- Localized all UI elements in `led-tui`, including menus, dialogs, and status messages.
- Added `README.md` with installation and SSH usage instructions.
- Verified build and localization across all targets.

### Completed Phase 11: Syntax Highlighting & Themes
- Implemented regex-based syntax highlighting engine in `led-core` with `rayon` parallelization.
- Added 11 built-in syntax definitions (Rust, Python, Go, etc.) and 6 built-in themes (Tokyo Night, Solarized, etc.).
- Wired theme and syntax selection in `led-tui`, with live application and persistence.
- Verified highlighting performance and correctness for all supported languages.

## 2026-05-06
### Completed Phase 10: Vi Mode
- Implemented `Normal`, `Insert`, and `Visual` modes.
- Added support for mode switching via `i`, `a`, `o`, `v`, and `Esc`.
- Implemented core Vi navigation: `h`, `j`, `k`, `l`, `w`, `b`, `e`, `gg`, and `G`.
- Added editing commands: `dd` (delete line), `yy` (yank line), `p` (paste), `u` (undo), and `x` (delete char).
- Implemented basic command-line mode for `:w`, `:q`, and `:wq`.
- Integrated search (`/`) into Vi mode, opening the Find panel.
- Added a Vi mode indicator to the status bar and wired the `View > Vi Mode` toggle.
- Refactored TUI handlers to resolve borrow checker issues while calling editor actions.
- Verified build and basic functionality across all modes.

### Completed Phase 9: Find/Replace Panel
- Implemented inline Find/Replace panel with support for incremental search.
- Added support for `Match Case`, `Whole Word`, and `Use Regex` flags.
- Implemented `Next`/`Prev` navigation with wrap-around messages in the status bar.
- Added `Replace` and `Replace All` functionality, with occurrence count reporting.
- Made search results and status per-buffer, ensuring correct behavior when switching tabs.
- Integrated search highlighting into both word-wrapped and non-wrapped rendering modes.
- Wired `Edit > Find` and `Edit > Replace` menu items and keyboard shortcuts (`Ctrl+F`, `Ctrl+H`).

### Completed Phase 8: Encoding & Line Ending Support
- Improved encoding auto-detection in `led-core` to support UTF-8 (with/without BOM), UTF-16 LE/BE, Shift-JIS, EUC-JP, ISO-2022-JP, and Latin-1.
- Expanded TUI menu system to include all supported encoding and line ending options.
- Implemented `Reopen with Encoding` (disk reload) and `Convert to Encoding` (session change) actions.
- Added radio-style menu checkmarks (`✓`) for mutually exclusive options like Encoding, Line Ending, and Theme.
- Ensured the menu system and status bar stay synchronized with buffer state changes (tab switching, encoding/line ending updates).
- Verified core encoding detection logic in `led-core`.

### Completed Phase 7: Word Wrap
- Implemented logical vs. visual line splitting for display.
- Added visual line wrapping based on terminal width and `unicode-width`.
- Implemented visual navigation (`↑/↓`) when word wrap is enabled, moving by visual line rather than logical line.
- Updated the status bar to show visual column relative to the current visual line.
- Wired the `View > Word Wrap` toggle and ensured it persists to the config.
- Automatically disabled horizontal scrolling when word wrap is active.
- Verified wrapping and navigation logic with unit tests in `led-core`.

### Completed Phase 6: Buffer Management, Editing & Undo/Redo
- Implemented `Editor` core logic using `ropey` for efficient large file support.
- Implemented robust undo/redo stack with a 1000-entry limit.
- Added support for character-level text selection via mouse (drag, double/triple click, Shift+click) and keyboard.
- Implemented real-time buffer rendering in TUI, handling CJK width, tabs, and scrolling.
- Integrated selection and cursor info into the status bar.
- Verified core logic with unit tests for insertion, deletion, and undo/redo behavior.

### Completed Phase 4: Dialogs & File I/O
...

- Refined `FileBrowser` with `Detect Encoding` toggle, sorting, and relative modification times.
- Implemented robust `Buffer` loading and saving with `encoding_rs` (auto-detection and conversion).
- Enhanced `OpenDialog` and `SaveAsDialog` with inline error messages.
- Implemented `ReopenConfirmationDialog` for safe encoding changes.
- Added overwrite confirmation logic to `SaveAs` operation.
- Integrated `chrono` and `humantime` for UI time formatting.

### Completed Phase 3: Menu Bar
- Implemented top-level menu bar (`File`, `Edit`, `View`, `Help`) with localization support.
- Implemented borderless dropdown rendering for a modern TUI look.
- Added support for separators and toggle indicators.
- Implemented nested submenus with recursive rendering.
- Added mouse hit-testing for dropdowns via `dropdown_rects`.
- Implemented `Alt+F/E/V/H` keyboard shortcuts and arrow key navigation.
- Shared `Action` enum in `led-core` for cross-frontend consistency.
- Resolved borrow checker issues related to nested state access in `App`.
