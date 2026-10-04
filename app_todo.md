  # zee Todo List

## Phase 23: Workspace, Backup, Sidebar & Plugin Manager (Completed in v0.1.5)

### UI & Sidebar Refinements
- [x] **Configurable Sidebar Position (Left / Right)**:
  - [x] Support placing the sidebar on either the left or right side of the editor
  - [x] Default to **right** side (natural for macOS users, while configurable to left for Windows Explorer familiarity)
  - [x] Add `sidebar_position = "right"` (options: `"left"`, `"right"`) to `config.toml` and Preferences settings
  - [x] Update border styling (`border_l_1` when right, `border_r_1` when left) and coordinate calculations in `editor_view`
- [x] **Open Folder & Automatic Sidebar File Tree**:
  - [x] Add "Open Folder..." (`Cmd+Shift+O` / `Ctrl+Shift+O`) to File menu
  - [x] When a directory is opened or passed as argument, automatically open sidebar and show file tree in Files tab
- [x] **Sidebar Vertical Split & File Properties**:
  - [x] Allow splitting the sidebar vertically into two panes (top/bottom)
  - [x] Top pane: Files tree / Outline tree
  - [x] Bottom pane: Active file properties (file name, size, line count, character count, encoding, line ending format) with collapsible toggle
- [ ] **Enhanced Code Outline**:
  - [ ] In addition to Markdown headings (already implemented), display function/class/method outline tree for source code files (via WASM plugins or built-in grammar parsers) with click-to-jump

### Config & Ecosystem
- [x] **Configuration Persistence (`~/.config/zee/config.toml`)**:
  - [x] Auto-load and write back runtime settings to `~/.config/zee/config.toml` (macOS/Linux) and `%APPDATA%\zee\config.toml` (Windows)
- [x] **Config & Plugins Export/Import**:
  - [x] Add menu actions to export configuration and installed plugins into an archive (`.zip`, `.tar.gz`, `.toml`)
  - [x] Add menu actions to import configuration and plugins with directory traversal protection and immediate reload
  - [x] Add Backup & Restore management section to Preferences dialog

### Plugin System & Management
- [x] **Plugin Directory (`~/.config/zee/plugins/`)**:
  - [x] Core `PluginManager` scans and loads WASM plugins from `~/.config/zee/plugins/`
- [x] **Plugin Management UI**:
  - [x] Dedicated modal dialog (`DialogType::PluginManager`) to list installed plugins, show capabilities (Outline, Commands), install new `.wasm` files via file picker, open plugins folder, and uninstall
- [x] **Plugins Menu in Menu Bar**:
  - [x] Top-level "Plugins" menu dynamically listing active plugins and their commands
  - [x] Execution of plugin transforms on active buffer or selection with undo/redo support

### Editing & Selection Enhancements
- [ ] **Rectangular / Column Selection (矩形選択 - 今後予定)**:
  - [ ] Core `ViMode::VisualBlock` foundation is already implemented in `zee-core` (`Ctrl+v`, `delete_visual_block`, `insert_visual_block`, `y`/`d`/`c`/`I`/`A`)
  - [ ] Add standard GUI mouse rectangular selection via `Alt (Option) + mouse drag` (accessible even when Vi mode is OFF)
  - [ ] Implement rectangular block paste (`Block Paste`): pasting rectangular multi-line text into a column without inserting standard line breaks
  - [ ] Support real-time multi-cursor typing across selected lines during visual block insertion

---

## Phase 24: 次世代 WASM プラグインシステム アーキテクチャ計画 (Component Model / WIT 移行)

現在実装済みの wasmi ベースの軽量スタック型 WASM 実装を発展させ、Wasmtime Component Model / WIT に基づく本格的・安全・高機能な拡張機能エコシステムへ刷新する計画。

### 1. 全体アーキテクチャ方針 (Host-Guest Contract)
- **ホストは薄く、プラグインは粗粒度 API で通信**:
  - WASM境界の越境オーバーヘッド・メモリコピーを最小化するため、1文字・1行ごとの細かい呼出は排除し、バッチまたはトランザクション単位の API を採用。
- **イベント駆動型フック (Guest Exports)**:
  - `on-startup`, `on-buffer-opened`, `on-buffer-changed`, `on-command`, `on-save` 等のフックをプラグインが export し、ホストが必要な契機でのみ非同期呼出。
- **トランザクション単位のバッファ変更 (`apply-edits(list<edit>)`)**:
  - 差分編集リストを一括適用することで、エディタの Undo/Redo ツリーとの整合性を担保し高速処理を実現。
- **プラグイン隔離とステートフル性**:
  - プラグイン内部で状態を保持可能とし、エディタ内部の生ポインタや内部データ構造は直接見せず、抽象ハンドル (`resource`) で隠蔽。

### 2. WIT (WebAssembly Interfaces) インターフェース設計
- **インターフェースのバージョニング (`zee:plugin@0.2.0`)**:
  - メジャーバージョンによる破壊的変更の検知、下位互換性および複数バージョンの world サポート。
- **`resource` 型によるハンドル管理**:
  - `resource buffer`, `resource window` 等のハンドル型でホストがライフサイクルを管理（不正IDアクセスやUse-After-Free防止）。
- **Result 型による厳格なエラー伝搬**:
  - `result<T, error-type>` で安全に返し、プラグインのパニックによる WASM トラップを遮断。
- **機能ごとの `interface` 分離**:
  - `buffer`, `outline`, `fs`, `http`, `ui` を分離し、宣言権限に基づく最小限の import のみ許可。
- **インデックス規約の明確化**:
  - 文字列位置情報（UTF-8 バイトオフセット / Unicode スカラー値 / 行・列 UTF-16 コードユニット）の定義を WIT 仕様書で明文化。

```wit
package zee:plugin@0.2.0;

interface buffer {
  resource buffer {
    text-range: func(start-byte: u32, end-byte: u32) -> result<string, string>;
    apply-edits: func(edits: list<edit>) -> result<_, string>;
    line-count: func() -> u32;
    get-path: func() -> option<string>;
  }
  record edit {
    start-byte: u32,
    end-byte: u32,
    new-text: string,
  }
}

interface outline {
  record outline-node {
    title: string,
    level: u32,
    line: u32,
  }
  export get-outline: func(buf: borrow<buffer::buffer>) -> result<list<outline-node>, string>;
}

world plugin {
  import buffer;
  export on-command: func(name: string, buf: borrow<buffer::buffer>) -> result<_, string>;
}
```

### 3. ホスト側（Wasmtime + Component Model）実装・暴走対策
- **Wasmtime エンジン集約と Store 分離**:
  - アプリ全体で共有する `Arc<wasmtime::Engine>` を 1 つ保持。プラグインごとに独立した `Store<T>` を生成。
- **暴走・無限ループ防止 (Fuel / Epoch Interruption)**:
  - `config.consume_fuel(true)` による命令数制限。
  - `config.epoch_interruption(true)` によるタイムアウト監視スレッドの導入。
- **メモリ上限の厳格化 (`StoreLimits`)**:
  - プラグインごとのメモリ消費上限（例: 64MB / 128MB）を設定し OOM を防止。
- **UI スレッド分離 (Background Worker)**:
  - プラグインの重い計算や I/O は UI スレッドから完全に切り離し、専用のバックグラウンドワーカーで非同期実行。
- **クラッシュ隔離 (Trap Isolation)**:
  - プラグインがトラップ（クラッシュ）してもエディタ本体は継続稼働し、該当プラグインのみを無効化してユーザーに通知。
- **起動高速化 & コンパイルキャッシュ**:
  - `Component::serialize` / `deserialize` による事前コンパイル済みバイナリのキャッシュ。
  - プラグイン manifest に基づく遅延ロード（特定言語やコマンド実行時のみインスタンス化）。

### 4. セキュリティ & ケイパビリティモデル
- **Manifest による権限宣言 (`plugin.toml`)**:
  - 例: `permissions = ["fs:read:workspace", "net:api.github.com"]`。
- **デフォルト権限ゼロ (Principle of Least Privilege)**:
  - 宣言されていないホスト機能は Linker に登録せず、ロード自体を拒否。
- **WASI preopen によるファイルシステム隔離**:
  - カレントワークスペース外へのアクセスを WASI レベルで遮断。
- **ホスト仲介型ネットワーク通信**:
  - 生のソケット開放は行わず、ホスト提供の HTTP API を通じてリクエストを検証・プロキシ。

### 5. プラグイン作者向け開発体験 (DX) & ツールチェーン
- **公式 SDK の提供 (`zee-plugin-sdk`)**:
  - Rust 向けに薄いラッパー crate（`zee_sdk::Buffer`, `zee_sdk::register_command!`）を用意し、WIT ボイラープレートを隠蔽。
- **多言語対応テンプレート**:
  - Rust, TinyGo, JavaScript/TypeScript (ComponentizeJS) 用のスターターテンプレートリポジトリ。
- **開発者向けコマンド**:
  - `zee --dev-plugin ./target/wasm32-wasip1/debug/my_plugin.wasm` によるホットリロード実行モード。
  - プラグイン専用の標準出力・ログビューアダイアログ。
- **モックホスト (`zee-plugin-mock`)**:
  - エディタ本体を起動せずに `cargo test` だけでプラグインロジックをテスト可能なモック環境。

### 6. 段階的実装ロードマップ
- **Step 1**: WIT で最小限の `zee:plugin@0.2.0` world（バッファ読み書き＋コマンド 1 個）を定義。
- **Step 2**: ホスト側で `wasmtime` (Component Model, fuel, StoreLimits, epoch, スレッド分離) の最小基盤を実装。
- **Step 3**: 既存の `zee-plugin-text` を新しい WIT / Component Model 形式に移行し、互換性とパフォーマンスを検証。
- **Step 4**: Manifest 権限モデルおよび WASI preopen によるセキュリティサンドボックスを適用。
- **Step 5**: SDK（`zee-plugin-sdk`）とテンプレートを整備し、コミュニティへ公開。

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
