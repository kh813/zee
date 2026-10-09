# zee User Manual / ユーザーマニュアル

**zee** is a lightweight, modern GUI and TUI text editor built for plain text, Markdown, code, and configuration files.  
It provides a native hardware-accelerated desktop application (`Zee.app` / `zee.exe` / `zeeg`) alongside a high-performance terminal interface (`zee` / `zee-cli`) running across macOS, Linux, and Windows — including seamlessly over SSH.

**zee** は、プレーンテキスト、Markdown、ソースコード、設定ファイル編集のための軽量・モダンなGUI & TUIテキストエディタです。  
macOS、Linux、Windows上で動作し、ハードウェアアクセラレーションによるデスクトップGUI（`Zee.app` / `zee.exe` / `zeeg`）と、ターミナルやSSH経由で軽快に動くTUI（`zee` / `zee-cli`）の両方をフルサポートしています。

---

## Table of Contents / 目次

1. [Installation / インストール](#1-installation--インストール)
2. [Basic Usage & CLI Modes / 基本的な使い方と起動モード](#2-basic-usage--cli-modes--基本的な使い方と起動モード)
3. [Keyboard Shortcuts / キーボードショートカット](#3-keyboard-shortcuts--キーボードショートカット)
4. [Configuration / 設定 (`config.toml`)](#4-configuration--設定-configtoml)
5. [Custom Themes & Syntaxes / テーマとシンタックスのカスタマイズ](#5-custom-themes--syntaxes--テーマとシンタックスのカスタマイズ)
6. [Google Drive Integration / Google ドライブ連携](#6-google-drive-integration--google-ドライブ連携)
7. [Internationalization (i18n) / 多言語設定](#7-internationalization-i18n--多言語設定)
8. [SSH & Remote Usage / SSHリモート接続での使用](#8-ssh--remote-usage--sshリモート接続での使用)
9. [Troubleshooting / トラブルシューティング](#9-troubleshooting--トラブルシューティング)

---

## 1. Installation / インストール

### Pre-built Binaries / ビルド済みバイナリ（推奨）

Download the pre-compiled archive for your OS from the [GitHub Releases page](https://github.com/kh813/zee/releases).  
[GitHub Releases ページ](https://github.com/kh813/zee/releases) からお使いのOSに合ったアーカイブをダウンロードしてください。

| Platform / プラットフォーム | Package / パッケージ | Contents / 収録バイナリ | Notes / 備考 |
| :--- | :--- | :--- | :--- |
| **macOS Apple Silicon** | `zeeg-v0.x.x-macos-arm64.zip` | `Zee.app` | Drag & drop to `/Applications` (M1/M2/M3/M4対応) |
| **Windows x86-64** | `zee-v0.x.x-windows-x64.zip` | `zee.exe` | Standalone native GUI executable (Windows 64bit GUI) |
| **Linux x86-64 (CLI)** | `zee-cli-v0.x.x-linux-x64.zip` | `zee`, `zee-cli` | Ultra-fast, zero-dependency terminal CLI binaries (サーバー・SSH向け) |
| **Linux Desktop (Dual)** | `zee-desktop-v0.x.x-linux-x64.zip` | `zee`, `zee-cli`, `zeeg` | Desktop bundle with .desktop file & icon (GUI/CLI兼用デスクトップ向け) |

### Linux: Dual Binary vs Pure CLI / Linuxの兼用版とCLI専用版について
- **`zee` (Dual GUI / CLI Binary / 兼用版)**:
  - Running `zee [file]` in a terminal launches the **Terminal CLI (TUI)**.
  - Running `zee --gui [file]` or `zee -g [file]` launches the **GPU Desktop GUI**.
  - Clicking `zee.desktop` launches the **GUI** automatically.
  - *ターミナルから通常実行するとCLI、`--gui` または `-g` を付けるとGUIが立ち上がります。*
- **`zee-cli` (Pure Zero-Dependency CLI / 超軽量CLI専用版)**:
  - Completely stripped of all graphical and display server dependencies (X11, Wayland, Vulkan).
  - Guaranteed to work on headless Linux servers, Docker containers, and minimal distributions without library errors.
  - *GUIライブラリ（Vulkan/Wayland等）を一切リンクしないゼロ依存バイナリ。ヘッドレスサーバーやコンテナ環境でも確実に動作します。*

### macOS Gatekeeper Note / macOS初回起動時の注意
On macOS, right-click `Zee.app` in Finder, select **Open**, and click **Open** in the dialog prompt.  
Alternatively, clear the quarantine attribute via Terminal:
```bash
xattr -cr /Applications/Zee.app
```

---

## 2. Basic Usage & CLI Modes / 基本的な使い方と起動モード

```bash
# Open an empty buffer / 空のバッファで起動
zee

# Open a specific file / ファイルを開く
zee myfile.txt

# Open multiple files in tabs / 複数ファイルをタブで開く
zee file1.txt file2.txt

# Launch hardware-accelerated GUI on Linux / LinuxでGUI版を起動
zee --gui myfile.txt
zee -g myfile.txt

# Launch zero-dependency CLI explicitly / 純粋CLI版で起動
zee-cli myfile.txt
```

### Command-Line Arguments & Direct Line Jump / コマンドライン引数と行ジャンプ

| Invocation / コマンド | Result / 動作 |
| :--- | :--- |
| `zee` | Opens with an empty `[No Name]` buffer / 空の新規バッファを開く |
| `zee file.txt` | Opens `file.txt` (creates a named memory buffer if not found) / ファイルを開く |
| `zee file.txt:42` | Opens `file.txt` and jumps cursor directly to line 42 / 42行目に直接ジャンプ |
| `zee +42 file.txt` | Alternative syntax to jump directly to line 42 / 42行目に直接ジャンプ（Vim互換構文） |
| `zee file.txt:42:15` | Jumps to line 42, column 15 / 42行目15列目にジャンプ |
| `zee --gui file.txt` (`-g`) | Forces hardware-accelerated desktop GUI launch / GUI版として強制起動 |
| `zee a.txt b.txt` | Opens both files in separate tabs / 複数ファイルを別タブで開く |

### Advanced File Operations / 高度なファイル操作

- **Reload File (`⌘Shift+R` / `Ctrl+Shift+R`)**:
  Reloads the active buffer from disk. Prompts for confirmation if you have unsaved edits.  
  *ディスクからファイルを再読み込みします。未保存の変更がある場合は確認ダイアログが表示されます。*
- **Open Recent / 最近使ったファイル**:
  Access recent files via `File > Open Recent`. History is saved in `~/.config/zee/recent_files.json`.  
  *`File > Open Recent` から最近開いたファイル履歴にアクセスできます。*
- **New from Template / テンプレートから新規作成**:
  Create new files pre-populated with standard skeletons via `File > New from Template`.  
  Built-in templates include Rust (bin/lib), Python, Go, HTML5, Markdown, and Shell.  
  Custom templates can be placed in `~/.config/zee/templates/`.  
  *標準テンプレート（Rust, Python, Go, HTML5, Markdown, Shell）から雛形ファイルを即座に作成できます。*

### Sidebar: File Tree & Multi-Language Outline / サイドバー

Toggle the sidebar with `View > [x] Sidebar` or `Ctrl+B` / `⌘B`:  
*`View > [x] Sidebar` または `Ctrl+B` / `⌘B` でサイドバーの表示/非表示を切り替えます:*

- **Files Tab / ファイルツリー**:
  - Interactive file explorer with directory expansion and smooth scrolling.
  - **Context Menu / コンテキストメニュー**: Right-click on any file/folder (or press `m` / `F10` in TUI) to access:
    - `📄 New File`: Create a new file / 新規ファイル作成
    - `📁 New Folder`: Create a new folder / 新規フォルダ作成
    - `✏️ Rename`: Rename item / 名前変更
    - `🗑️ Delete`: Delete item / 削除（確認付き）
    - `↻ Refresh`: Rescan file tree / ツリー再読み込み
    - `👁 Show/Hide Hidden Files`: Toggle dotfiles (`.gitignore`, `.env`, etc.) / 隠しファイル表示切り替え
  - **Show Hidden Files**: Click `👁` in header, toggle via context menu, or press `h` in TUI. State is saved to `config.toml`.  
    *ヘッダーの `👁` ボタン、右クリック、またはTUIで `h` キーを押して隠しファイルを切り替えます。*
- **Outline Tab / アウトライン**:
  - Code symbol navigator extracting headings, functions, classes, and structs:
    - **Markdown**: Heading hierarchy (`#`, `##`, etc.) / 見出しツリー
    - **Rust**: Modules, structs, enums, impls, and functions (`fn`) / 構造体・関数
    - **Python**: Classes and functions/methods (`def`) / クラス・メソッド
    - **Go**: Structs, interfaces, and functions (`func`) / 型・関数
    - **JSON**: Key hierarchy and nested structures / キー階層
    - **HTML / CSS**: Heading tags, titles, media rules, and selectors / 見出しやセレクタ

### Mouse Block (Rectangular) Selection / マウス矩形選択

In the desktop GUI, hold the `Alt` key (`Option` on macOS) while dragging the mouse:  
*デスクトップGUIで `Alt` キー（macOSは `Option`）を押しながらマウスドラッグ:*
- Initiates **Visual Block** rectangular selection across multiple lines and columns. (複数行・列の矩形選択を開始)
- Copy (`⌘C` / `Ctrl+C`), Cut (`⌘X` / `Ctrl+X`), and Paste (`⌘V` / `Ctrl+V`). (ブロックコピー/カット/ペースト)
- Typing replaces all selected lines in the column simultaneously. (文字入力で全選択行を一括同時編集)

---

## 3. Keyboard Shortcuts / キーボードショートカット

### File Menu / ファイル操作

| Action / 操作 | Shortcut (macOS) | Shortcut (Win / Linux) |
| :--- | :--- | :--- |
| New Tab / 新規タブ | `⌘T` | `Ctrl+T` |
| New Window / 新規ウィンドウ (GUI) | `⌘N` | `Ctrl+N` |
| Open File / ファイルを開く | `⌘O` | `Ctrl+O` |
| Open Recent / 最近開いたファイル | `File > Open Recent` | `File > Open Recent` |
| Reload File / ファイル再読み込み | `⌘Shift+R` | `Ctrl+Shift+R` |
| Save / 保存 | `⌘S` | `Ctrl+S` |
| Save As / 名前を付けて保存 | `⌘Shift+S` | `Ctrl+Shift+S` |
| Close Tab / タブを閉じる | `⌘W` | `Ctrl+W` |
| Quit / 終了 | `⌘Q` | `Ctrl+Q` |

### Edit Menu / 編集操作

| Action / 操作 | Shortcut (macOS) | Shortcut (Win / Linux) |
| :--- | :--- | :--- |
| Undo / 元に戻す | `⌘Z` | `Ctrl+Z` |
| Redo / やり直し | `⌘Shift+Z` / `⌘Y` | `Ctrl+Y` |
| Cut / 切り取り | `⌘X` | `Ctrl+X` |
| Copy / コピー | `⌘C` | `Ctrl+C` |
| Paste / 貼り付け | `⌘V` | `Ctrl+V` |
| Select All / すべて選択 | `⌘A` | `Ctrl+A` |
| Find / 検索 | `⌘F` | `Ctrl+F` |
| Find & Replace / 置換 | `⌘R` / `⌘Shift+F` | `Ctrl+R` / `Ctrl+Shift+F` |

### View & Window Navigation / 表示・ナビゲーション

| Action / 操作 | Shortcut (macOS) | Shortcut (Win / Linux) |
| :--- | :--- | :--- |
| Toggle Sidebar / サイドバー切り替え | `⌘B` | `Ctrl+B` |
| Toggle Vi Mode / Viモード切り替え | `⌘E` / `F4` | `Ctrl+E` / `F4` |
| Preferences / 設定ダイアログ | `⌘,` | `Ctrl+,` |
| Go to Line / 指定行へジャンプ | `⌘G` | `Ctrl+G` |
| Next Tab / 次のタブ | `Ctrl+Tab` | `Ctrl+Tab` |
| Previous Tab / 前のタブ | `Ctrl+Shift+Tab` | `Ctrl+Shift+Tab` |
| Zoom In / 拡大 (GUI) | `⌘=` / `⌘+` | `Ctrl+=` / `Ctrl++` |
| Zoom Out / 縮小 (GUI) | `⌘-` | `Ctrl+-` |
| Reset Zoom / 拡大リセット (GUI) | `⌘0` | `Ctrl+0` |
| Refresh File Tree / ツリー再読み込み | `⌘⌥R` / `F5` | `Ctrl+Alt+R` / `F5` |

### Vi / Vim Mode Commands / Viモードコマンド

Toggle with `Ctrl+E` / `⌘E` / `F4`. A Vim-standard 2-row bottom bar appears for Ex commands and mode indicators.  
*`Ctrl+E` / `⌘E` / `F4` で切り替えます。下部にVim標準のステータスおよびExコマンドラインが表示されます。*

#### Ex Commands (`:`) / コマンドライン
- `:w` / `:w [path]`: Save buffer / 保存
- `:q` / `:q!`: Quit active tab / タブを閉じる（`!`で未保存強制破棄）
- `:wq` / `:x`: Save and quit / 保存して閉じる
- `:qa` / `:qa!`: Quit all tabs / 全タブを閉じる
- `:wqa`: Save all modified tabs and quit / 全保存して終了
- `:e [path]`: Open or reload file / ファイルを開くまたは再読み込み
- `:<line>` (e.g. `:42`): Jump to line / 指定行へジャンプ
- `:noh`: Clear search highlights / 検索ハイライト消去
- `:bn` / `:bp`: Next / previous tab / 次・前のタブ
- `:[range]s/pattern/replacement/[flags]`: Regex pattern substitution / 置換（例: `:%s/foo/bar/g`）
- `:set nu` / `:set nonu`: Toggle line numbers / 行番号の表示・非表示

#### Normal Mode Motions / 移動コマンド
- `h` / `j` / `k` / `l`: Left, down, up, right / 左・下・上・右
- `w` / `b` / `e`: Word motions / 単語送り・戻し・末尾
- `0` / `^` / `$`: Line start, first non-blank, line end / 行頭・非空白行頭・行末
- `gg` / `G`: File start / end (or `[count]G` jump) / ファイル先頭・末尾・指定行
- `Ctrl+D` / `Ctrl+U`: Half-page scroll down / up / 半画面スクロール
- `f<char>` / `F<char>`: Find char inline / 行内文字検索
- `i` / `I` / `a` / `A` / `o` / `O`: Enter Insert mode / 挿入モードへ移行
- `x` / `dd` / `D`: Delete char, line, to line end / 文字削除・行削除
- `u`: Undo / 元に戻す
- **IME Transparency**: Japanese IME keystrokes are automatically normalized in Normal mode (e.g. `っ` triggers `dd`, `い` triggers `i`).  
  *日本語IMEがONでも自動で英数キーに正規化されるため、モード切り替えに失敗しません。*

---

## 4. Configuration / 設定 (`config.toml`)

Configuration is saved in `~/.config/zee/config.toml` (or `%APPDATA%\zee\config.toml` on Windows).  
*設定ファイルは `~/.config/zee/config.toml`（Windowsは `%APPDATA%\zee\config.toml`）に保存され、GUI版とCLI版で同期されます。*

```toml
# UI language ("en" for English, "ja" for 日本語)
language = "en"

# Active theme / 配色テーマ
# ("tokyo-night", "catppuccin-latte", "catppuccin-mocha", "dracula", "nord", "solarized-dark", etc.)
theme = "tokyo-night"

# Display line numbers in gutter / 行番号表示
line_numbers = true

# Wrap long lines in editor / 右端での折り返し
word_wrap = true

# Enable Vi/Vim keybindings by default / Viモードの有効化
vi_mode = false

# Tab stop width / タブ幅 (1-16)
tab_size = 4

# Expand tabs to spaces / タブをスペースに展開
expand_tab = false

# Automatically trim whitespace and ensure trailing newline on save / 保存時自動整形
format_on_save = false

# Open sidebar by default / 起動時にサイドバーを開く
sidebar = false

# Sidebar dock position ("right" or "left") / サイドバー位置
sidebar_position = "right"

# Font size and line height for GUI editor / GUIフォントサイズ
font_size = 13.0
line_height = 20.0
```

---

## 5. Custom Themes & Syntaxes / テーマとシンタックスのカスタマイズ

- **Custom Themes / カスタムテーマ**:  
  Place `.toml` theme files in `~/.config/zee/themes/`. Themes appear instantly in the Settings dropdown.  
  *`~/.config/zee/themes/` に `.toml` テーマを配置すると、設定画面に自動反映されます。*
- **Custom Syntax Highlighting / カスタムシンタックス**:  
  Place `.toml` syntax definition files in `~/.config/zee/syntaxes/`. Supports regex patterns, keyword lists, and comments.  
  *`~/.config/zee/syntaxes/` に定義ファイルを配置することで、独自言語のハイライトを追加できます。*

---

## 6. Google Drive Integration / Google ドライブ連携

Zee allows you to browse, edit, and save Google Drive files seamlessly:  
*Google ドライブ上のファイルを直接開いて編集・保存できます:*

1. Open `File > Open from Google Drive…` (`Google ドライブから開く…`).
2. **API Configuration / 認証設定**:
   - Click `Load from JSON file 📂` (`JSONファイルから読み込む 📂`) to directly import your Google Cloud Console OAuth credentials JSON.
   - Alternatively, copy and paste Client ID and Secret directly using `Ctrl+V` / `⌘V` or the `Paste 📋` button.
3. Complete authentication in your default web browser.
4. Browse files, search in folders, and open documents directly into Zee tabs.

---

## 7. Internationalization (i18n) / 多言語設定

Zee natively supports **English** and **Japanese (日本語)** across all menus, dialogs, status messages, and context menus.  
Switch languages anytime via `Preferences` (`⌘,` / `Ctrl+,`) or by setting `language = "ja"` in `config.toml`.  
*すべてのメニュー、ダイアログ、通知メッセージが英語と日本語に対応しています。設定ダイアログからいつでも切り替え可能です。*

---

## 8. SSH & Remote Usage / SSHリモート接続での使用

When editing files over remote SSH terminal connections, add the following line to your remote `~/.bashrc` or `~/.zshrc` to prevent `Ctrl+S` from freezing terminal transmission:  
*SSH接続先で `Ctrl+S` による端末フリーズを防ぐため、リモートのシェル設定に以下を追加してください:*

```bash
stty -ixon
```

Zee fully supports the **OSC 52** clipboard protocol: text copied inside a remote TUI terminal session synchronizes directly to your local workstation's clipboard.  
*OSC 52 クリップボード転送に対応しており、SSHリモート先でコピーしたテキストが手元のマシンのクリップボードにそのまま同期されます。*

---

## 9. Troubleshooting / トラブルシューティング

- **Linux GUI fails to launch on headless servers / ヘッドレス環境でGUIが起動しない**:  
  Ensure you are using `zee-cli` (or running `zee` in a terminal). The GPU-accelerated GUI requires an active X11 or Wayland display server.  
  *GUI機能はX11またはWaylandが必要です。サーバー環境では依存関係ゼロの `zee-cli` をご利用ください。*
- **Terminal characters misplaced / ターミナル表示のズレ**:  
  Ensure your terminal supports UTF-8 and 24-bit TrueColor (`export COLORTERM=truecolor`).  
  *端末エミュレータがTrueColorとUTF-8に対応していることを確認してください。*
- **Reset Configuration / 設定のリセット**:  
  Delete `~/.config/zee/config.toml` to restore all default factory settings.  
  *`~/.config/zee/config.toml` を削除すると初期設定に戻ります。*
