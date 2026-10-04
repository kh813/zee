  # zee Todo List

## Phase 23: Upcoming Roadmap & Features (Planned)

### UI & Sidebar Refinements
- [ ] **Configurable Sidebar Position (Left / Right)**:
  - [ ] Support placing the sidebar on either the left or right side of the editor
  - [ ] Default to **right** side (natural for macOS users, while configurable to left for Windows Explorer familiarity)
  - [ ] Add `sidebar_position = "right"` (options: `"left"`, `"right"`) to `config.toml` and Preferences settings
  - [ ] Update border styling (`border_l_1` when right, `border_r_1` when left) and coordinate calculations in `editor_view`
- [ ] **Sidebar Vertical Split & File Properties**:
  - [ ] Allow splitting the sidebar vertically into two panes (top/bottom)
  - [ ] Top pane: Files tree / Outline tree
  - [ ] Bottom pane: Active file properties (file name, full path, file size, last modified timestamp, encoding, line ending, permissions)
- [ ] **Enhanced Code Outline**:
  - [ ] In addition to Markdown headings (already implemented), display function/class/method outline tree for source code files (via WASM plugins or built-in grammar parsers) with click-to-jump

### Config & Ecosystem
- [x] **Configuration Persistence (`~/.config/zee/config.toml`)**:
  - [x] Auto-load and write back runtime settings to `~/.config/zee/config.toml` (macOS/Linux) and `%APPDATA%\zee\config.toml` (Windows)
- [ ] **Config & Plugins Export/Import**:
  - [ ] Add menu actions to export configuration and installed plugins into an archive/file
  - [ ] Add menu actions to import configuration and plugins

### Plugin System & Management
- [x] **Plugin Directory (`~/.config/zee/plugins/`)**:
  - [x] Core `PluginManager` already scans and loads WASM plugins from `~/.config/zee/plugins/`
- [ ] **Plugin Management UI**:
  - [ ] Create a dedicated modal dialog/management screen to list, enable, disable, and view details of installed plugins
- [ ] **Plugins Menu in Menu Bar**:
  - [ ] Add top-level "Plugins" menu
  - [ ] Dynamically populate commands provided by active plugins and execute them on the active buffer / selection

### Editing & Selection Enhancements
- [ ] **Rectangular / Column Selection (矩形選択)**:
  - [ ] Support vertical column / box selection across multiple lines (`Alt + mouse drag` or visual block mode)
  - [ ] Support column editing, multi-cursor insertion/deletion, and block cut/copy/paste (evaluate core implementation vs WASM plugin extensibility)

## Phase 22: GUI Font & Spacing and Theme Customization
- [x] **GUI Font & Typography Customization (`zee-gui`)**:
  - [x] Add `font_family`, `font_size`, and `line_height` to `Config` for editor code area
  - [x] Add `ui_font_family` and `ui_font_size` for UI chrome (tabs, status bar, dialogs)
  - [x] Wire dynamic font size and line height into editor text measurement, rendering, scroll calculations, and mouse hit-testing
  - [x] Keep CLI (`zee-tui`) untouched (terminal font is controlzee by terminal emulator)
- [x] **Theme Customization & Human-Readable CSS Colors (TUI & GUI)**:
  - [x] Expand theme color parser in `zee-core` to support CSS Hex (`#rgb`, `#rrggbb`, `#rrggbbaa`), CSS `rgb(...)`, `rgba(...)`, `ansi(...)`, and named CSS colors
  - [x] Implement dynamic user theme discovery from `~/.config/zee/themes/*.toml` in `Theme::load_all()` and `Theme::find_by_name()`
  - [x] Populate theme menus and live switching dynamically in both TUI (`zee-tui`) and GUI (`zee-gui`)
- [x] **Documentation & Configuration Templates**:
  - [x] Create `assets/config.toml.default` and update root `config.toml.default`
  - [x] Update `app_specs.md`, `MANUAL.md`, `README.md`, and `DEVLOG.md`

## Phase 21: GUI Modernization & UX Polish
- [x] **Typography & Styling Foundation**:
  - [x] Separate UI proportional font (`.AppleSystemUIFont` / sans-serif) from editor monospace font
  - [x] Implement consistent spacing, rounded border utilities, and hover color mappings
- [x] **Modern TabBar**:
  - [x] Rounded tab pills with clean active/inactive separation
  - [x] Unsaved modification indicator with styzee dirty dot
  - [x] Close button (`×`) with hover highlight and padding
  - [x] Smooth tab switching and horizontal scroll handling
  - [x] Add New Tab (`+`) button
- [x] **Modern StatusBar**:
  - [x] Segmented pill design with proportional typography and badge styling
  - [x] Interactive click handlers: click line/col for Go to Line dialog
  - [x] Clean visual hierarchy between file info and status indicators
- [x] **Modern Find/Replace Panel**:
  - [x] Rounded input fields with placeholder text and focus rings
  - [x] Pill toggle buttons for Match Case (`Aa`), Whole Word (`\b`), Regex (`.*`)
  - [x] Search count badge (`3 of 12`) and sleek navigation action buttons
- [x] **Modern Modal Dialogs**:
  - [x] Rounded modal card UI with drop shadow (`rounded-xl`, `shadow-2xl`)
  - [x] Primary / Secondary button styling with hover states
  - [x] Clean Go to Line, About, and Unsaved Changes dialog layouts
- [x] **Scrollbar & Editor Visual Polish**:
  - [x] Smooth overlay scrollbar with hover feedback
  - [x] Selection highlight, gutter border, and line padding refinements

## Phase 18: Project Management & Navigation
- [ ] **File Explorer (Side Bar)**:
  - [ ] Implement directory scanning in `zee-core`
  - [ ] Create `FileTreeView` in GUI / `SideBar` in TUI
  - [ ] Add `Alt+1` shortcut to toggle side bar focus
- [ ] **Fuzzy Finder (`Ctrl+P`)**:
  - [ ] Implement fast file indexing and fuzzy matching logic
  - [ ] Create searchable list dialog for both TUI and GUI
- [ ] **Recently Opened Files**:
  - [ ] Persist file history in `config.toml` or separate state file
  - [ ] Add `File > Recent Files` submenu
- [ ] **Project-wide Search (`Ctrl+Shift+F`)**:
  - [ ] Basic "grep" functionality across the current workspace directory

## Phase 19: Advanced Editing & Visuals
- [ ] **Bracket Matching**:
  - [ ] Highlight corresponding `()`, `[]`, `{}` pairs
- [ ] **Auto-indentation**:
  - [ ] Maintain indentation level on newline
  - [ ] Language-specific indentation rules (e.g., after `{`)
- [ ] **Comment Toggle (`Ctrl+/`)**:
  - [ ] Support single-line and block comment toggling based on syntax
- [ ] **Soft Tabs / Tab Conversion**:
  - [ ] Option to use spaces instead of tabs
  - [ ] "Convert Tabs to Spaces" action

## Phase 20: Settings & Ecosystem
- [ ] **Settings Dialog (UI-based Configuration)**:
  - [ ] Visual editor for `config.toml` settings
  - [ ] Live preview for theme and font changes
- [ ] **Keybinding Customization**:
  - [ ] Allow users to override default shortcuts in `config.toml`
- [x] **CI/CD & Compilation (GitHub Actions)**:
  - [x] Multi-platform compilation workflow (`release.yml`) for TUI (`zee`) and GUI (`zee-gui`) across macOS, Linux, and Windows
  - [x] Artifact upload and release packaging for tagged releases
- [ ] **Packaging & Distribution**:
  - [ ] Homebrew (macOS), NSIS (Windows), and .deb/.rpm (Linux) packages
  - [ ] Documentation for installation via `cargo install`

---

## Phase 17: Bugfixes & Polish (TUI & GUI)

### TUI Fixes
- [x] Fix editor visibility (cursor and text) in default themes
- [x] Fix highlight visibility in File Open/Save dialogs
- [x] Fix `Esc` key regression for closing dialogs
- [x] Fix Unsaved Changes dialog:
  - [x] Implement `Tab` navigation
  - [x] Ensure it appears when closing any modified buffer or quitting with any modified buffer
- [x] Fix Menu "Exit" action behavior
- [x] Improve TUI "Terminal Default" theme to match terminal color scheme (ANSI 16 colors)
- [x] Improve Japanese inline input (implemented via hardware cursor positioning)

### GUI Fixes
- [x] Fix editor visibility parity with TUI
- [x] Fix default window position (center on screen)
- [x] Fix automatic theme selection (OS light/dark mode)
- [x] Fix color visibility:
  - [x] Unify `zee_color_to_gpui` across all widgets using `gpui::rgb`.
  - [x] Ensure `EditorView` uses consistent color mapping for text and background.
- [x] **Verify native GUI rendering (no invisible text)**
- [x] Implement Japanese inline input support (IME) in `EditorView`:
  - [x] Fix `marked_text_range` to return the composition range.
  - [x] Ensure `replace_and_mark_text_in_range` correctly manages composition state.
  - [x] Improve `bounds_for_range` for accurate candidate window placement.
- [x] Fix native macOS/Windows shortcuts:
  - [x] Verify `cmd-q`, `cmd-o`, `ctrl-q`, `ctrl-o` etc. are correctly bound and handzee.
  - [x] Ensure `EditorView` doesn't intercept system/action shortcuts in `on_key_down`.
- [x] Fix app-level menu state when no windows are open:
  - [x] Ensure `New`, `Open`, and `Quit` actions remain enabzee in the global menu.
  - [x] Verify `app.on_action` handlers are correctly registered.
- [x] Use OS native dialogs for Open/Save (integrate `rfd` crate) - Already partially done in code, ensure consistency.

### TUI Fixes
- [x] Fix CJK character spacing:
  - [x] Update `Renderer` to handle multi-width characters.
  - [x] Update `App` and `Dialog` to set correct character widths.
- [x] Improve IME candidate window placement by moving hardware cursor to logical cursor position.

- [x] Achieve menu parity with TUI (full Encoding and Line Ending support)
- [x] Fix dialog positioning and window bounding
- [x] **Implement file drag-and-drop support**:
  - [x] Register drag-and-drop event handler in `app.rs` or `window_view.rs`
  - [x] Implement path extraction from drop events
  - [x] Add logic to check for existing tabs before opening new ones
  - [x] Ensure the last dropped file becomes the active tab
  - [x] Verify handling of multiple files dropped at once

### Common / Others
- [x] Integrate application icons:
  - [x] Ensure icon files are in place (`.icns` for Mac, `.ico` for Windows)
  - [x] Set icons for macOS `.app` bundle and Windows `.exe`
- [x] Final performance and UI polish

### ✅ Phase 17 Completion Log

- **Completed**: 2026-05-13
- **Commit**: `(pending)`
- **Implementer**: AI session & Gemini CLI
- **Files created**:
  - `crates/zee-gui/build.rs` — Added Windows resource compilation.
  - `crates/zee-gui/resources/` — Organized icon assets.
- **Files modified**:
  - `crates/zee-gui/Cargo.toml` — Added `winres` build dependency.
  - `crates/zee-tui/src/app.rs` — Fixed hardware cursor visibility and placement.
  - `crates/zee-gui/src/widgets/editor_view.rs` — Robust rendering and IME fixes.
- **Key decisions made**:
  - Embedded Windows icon via `winres` for native `.exe` appearance.
  - Fixed TUI IME positioning by moving the hardware cursor to the logical cursor.
  - Explicitly set monospace fonts and unified color mapping in GUI to ensure visibility.
- **Known issues / deferred work**:
  - File drag-and-drop support remains limited in current GPUI version.

### ✅ Phase 16 Completion Log

- **Completed**: 2026-05-09
- **Commit**: `d88e60f9c2ef97733417b870ebb15587c966af9b` (and subsequent bugfixes)
- **Implementer**: AI session & Gemini CLI
- **Files created**: None
- **Files modified**:
  - `crates/zee-gui/src/app.rs` — Fixed compilation, added dynamic theme selection and parameterized actions.
  - `crates/zee-gui/src/window_view.rs` — Fixed dialog overlay, workspace notifications, and encoding/line ending handlers.
  - `crates/zee-gui/src/widgets/editor_view.rs` — Improved text visibility, font inheritance, and model observation.
  - `crates/zee-gui/src/widgets/dialog.rs` — Full implementation of modal dialogs and file browser.
- **Key decisions made**:
  - Implemented a parameterized `SetTheme` action in GPUI to support dynamic theme selection.
  - Switched to `observe` for the `Workspace` model to ensure correct UI re-renders on state change.
  - Used absolute positioning for the dialog overlay to keep it centered and within window boundaries.
- **Known issues / deferred work**:
  - File drag-and-drop was removed due to API compatibility issues with the pinned GPUI version.
  - Scroll performance for large file lists in dialogs could be improved.
- **Bugfixes addressed**:
  - Grayed-out Encoding, Line Ending, and Theme menus are now functional.
  - Visibility in default theme improved (fixed font/size inheritance).
  - Dialogs no longer exceed window boundaries.
  - Fixed a critical compilation error in `Workspace::new` call.
