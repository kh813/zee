# zee (ZEpto Editor)

[![CI](https://github.com/kh813/zee/actions/workflows/ci.yml/badge.svg)](https://github.com/kh813/zee/actions/workflows/ci.yml)
[![Release](https://github.com/kh813/zee/actions/workflows/release.yml/badge.svg)](https://github.com/kh813/zee/releases)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/OS-macOS%20%7C%20Windows%20%7C%20Linux-brightgreen.svg)](https://github.com/kh813/zee/releases)

**zee** is a lightweight, modern, and lightning-fast text editor built in Rust.  
**zee** は、Rust で構築された軽量・現代的で超高速なテキストエディタです。

---

## 💻 Supported Operating Systems / サポートOS

| Operating System / OS | GUI Edition / デスクトップ版 | CLI & TUI Edition / ターミナル版 | Architectures / アーキテクチャ | Status / 状態 |
| :--- | :--- | :--- | :--- | :--- |
| **macOS** | **Zee.app** (Metal GPU) | **zee** / **zee-cli** | Apple Silicon (M1/M2/M3/M4) & Intel | ✅ Fully Supported |
| **Windows** | **zee.exe** (DirectX GPU) | **zee.exe** (Console mode) | 64-bit (x86-64) & ARM64 | ✅ Fully Supported |
| **Linux** | **zee** (`--gui`, Vulkan GPU) | **zee** (Default) / **zee-cli** (Zero-dep) | x86-64 & ARM64 (aarch64) | ✅ Fully Supported |

---

## 🌐 Multi-OS & Dual GUI / CLI Architecture / マルチOS・GUI/CLI両対応

**zee** is engineered from the ground up to deliver a unified, seamless editing experience across **macOS, Windows, and Linux**, fully supporting both native desktop GUIs and terminal CLIs (TUI).

**zee** は、**macOS / Windows / Linux** のマルチOSに対応し、**GUI（デスクトップアプリ）とCLI（ターミナルTUI）の双方をフルサポート**したテキストエディタです。

### 🎯 Unified Experience / 一貫したユーザー体験
Whether launching the GUI app on a Mac/Windows workstation or editing over SSH on a headless Linux server, you get the **exact same menus, keyboard shortcuts, dialogs, themes, and intuitive editing feel**—eliminating the need to switch muscle memory between environments.

「WindowsやMacでのリッチなGUI版」と「主にLinux環境やSSHリモート接続で活躍するCLI版」の間で、ショートカット、メニュー構成、ダイアログ、操作感に至るまで**全く同じユーザー体験**を提供することを目指して設計されています。

### 🐧 Linux: Smart Dual Binary & Zero-Dependency CLI / Linux向けデュアルバイナリ設計
- **`zee` (Dual GUI / CLI Binary / 兼用バイナリ)**:
  - Running `zee [file]` in a terminal launches the **Terminal CLI (TUI)** immediately.
  - Running `zee --gui` or `zee -g` launches the **Hardware-Accelerated GUI**.
  - Clicking the application icon from desktop app launchers (.desktop) automatically launches the **GUI**.
  - *ターミナルから通常実行するとCLI版、`zee --gui`（または `zee -g`）で実行するとGUI版が起動します。*
- **`zee-cli` (Pure Zero-Dependency CLI / 超軽量CLI専用バイナリ)**:
  - Stripped of all GPU and graphical library dependencies (Wayland/X11/Vulkan).
  - Guaranteed to run instantly on minimal Linux servers, headless cloud VPS instances, and Docker containers without any shared library errors.
  - *GUIライブラリ（Vulkan/Wayland等）を一切リンクしない超軽量バイナリ。グラフィック環境のない最小構成LinuxサーバーやDockerでも確実に動作します。*

---

## ✨ Highlights & Features / 主な機能と特徴

- **Dual GUI & TUI Parity / GUIとCLIの完全同期**:
  - Native hardware-accelerated desktop GUI (`Zee.app` / `zee.exe` / `zee --gui`) and terminal TUI (`zee` / `zee-cli`) share 100% feature and shortcut parity.
  - *GUI版とターミナルCLI版でショートカットや操作体系が100%一致。*
- **Sidebar: File Tree & Outline / サイドバー（ファイルツリー & アウトライン）**:
  - Collapsible file explorer and code symbol outline navigation.
  - Full file management: create, rename, and delete files/folders directly via context menu (or `m` key in TUI).
  - Dotfiles & hidden file toggle (`👁` button or `h` key).
  - Multi-language symbol outline: Markdown, Rust, Python, Go, JSON, YAML, Shell, HTML, CSS.
  - *フォルダツリーと関数/見出しのアウトライン表示。ファイルの新規作成・名前変更・削除や隠しファイル切り替えに対応。*
- **Starter File Templates / 新規作成テンプレート**:
  - Scaffold new files via `File > New from Template` (Rust, Python, Go, HTML5, Markdown, Shell).
  - Macro interpolation (`{filename}`, `{date}`, `{year}`, `{author}`, `{cursor}`).
  - *テンプレートから一瞬で雛形ファイルを作成。*
- **Mouse Rectangular / Column Selection / 矩形選択 & 列編集**:
  - Hold `Alt` (`Option` on macOS) + drag to select rectangular code blocks for simultaneous column editing.
  - *Alt/Optionキーを押しながらドラッグで矩形選択・一括編集が可能。*
- **Direct CLI Line Jumps / コマンドラインからの行指定起動**:
  - Open files directly at specific line numbers: `zee file.rs:42` or `zee +42 file.rs`.
  - *ファイル名に行番号を添えて即座に対象行を開く。*
- **Extensible WASM Plugin System / WebAssembly プラグイン拡張**:
  - Sandboxed WASM plugin runtime with an in-app Plugin Manager.
  - *WASMによる安全で高速なプラグイン拡張と内蔵プラグインマネージャー。*
- **Optional Vi Mode / Viモード (Vim互換)**:
  - Modal editing (`Normal`, `Insert`, `Visual`, `VisualBlock`) with Ex command line (`:w`, `:q`, `:wq`, `:%s`).
  - Full Japanese IME transparency: automatically normalizes full-width keys (`っ` for `dd`, `い` for `i`).
  - *モーダル編集対応。日本語IMEがONのままでも入力が壊れない自動正規化を完備。*
- **Google Drive Integration / Google ドライブ連携**:
  - Browse, open, edit, and save Google Drive files seamlessly with OAuth 2.0 integration and credentials JSON import.
  - *Googleドライブ上のファイルを直接開いて同期・保存可能。認証JSONのインポートに対応。*
- **In-App Self-Update / セルフアップデート**:
  - One-click update check (`Help` → `Check for Updates...`) with automatic open tabs and session restoration.
  - *ワンクリックで更新チェック・自動適用し、作業中のタブやカーソル位置を自動復元。*

---

## 📦 Installation & Download / インストールとダウンロード

Pre-compiled standalone packages and installers are available on the [Releases page](https://github.com/kh813/zee/releases).  
ビルド済みバイナリは [GitHub Releases](https://github.com/kh813/zee/releases) からダウンロードできます。

| Package / パッケージ | Platform / OS | Edition / 内容 | Description / 説明 |
| :--- | :--- | :--- | :--- |
| `zee-macos-arm64.zip` | macOS (Apple Silicon) | Native GUI (`Zee.app`) | Drag & drop to `/Applications` (Finder / Spotlight対応) |
| `zee-windows-x64.zip` | Windows (x86-64) | Native GUI (`zee.exe`) | Standalone desktop executable (デスクトップアプリ単体) |
| `zee-linux-x64.zip` | Linux (x86-64) | Pure CLI (`zee` & `zee-cli`) | Zero-dependency terminal binaries (サーバー・SSH向け軽量版) |
| `zee-desktop-linux-x64.zip` | Linux (x86-64) | Desktop (`zee` dual, `zee-cli`) | Full desktop bundle with .desktop file & icon (デスクトップ向け兼用版) |

### macOS Gatekeeper Note / macOSでの初回起動時の注意
On macOS, downloaded apps outside the App Store may show a warning: *"Zee.app cannot be opened because the developer cannot be verified"*.  
macOSで「開発元を確認できないため開けません」と表示される場合は、以下のいずれかで許可してください:
1. Right-click `Zee.app` in Finder → Select **Open** → Click **Open** in the prompt. (Finderで右クリックして「開く」を選択)
2. Run in Terminal:  
   ```bash
   xattr -cr /Applications/Zee.app
   ```

---

## 🚀 Basic Usage / 基本的な使い方

### 1. Launching GUI / GUI版の起動
- **macOS**: Launch `Zee.app` from Applications or Spotlight.
- **Windows**: Launch `zee.exe` or open from Start menu.
- **Linux (Dual Binary)**:
  ```bash
  # Launch hardware-accelerated GUI (Vulkan GPU)
  zee --gui file.txt
  # or short flag:
  zee -g file.txt
  ```

### 2. Launching Terminal CLI / CLI（ターミナル）版の起動
```bash
# Open file in terminal (TUI)
zee file.txt

# Or explicitly using zero-dependency CLI binary on Linux:
zee-cli file.txt

# Open directly at specific line/column:
zee src/main.rs:42
zee +100 src/main.rs
```

---

## 🛠️ Build from Source / ソースからのビルド

**Prerequisites / 必要環境**: Rust toolchain (1.80+), C build tools (`make` / `gcc` / `clang`).

### macOS & Linux (Make)
```bash
git clone https://github.com/kh813/zee.git
cd zee

# Build both GUI and CLI binaries into dist/ (現在のOS向けにすべてビルド)
make

# Or build individually / 個別ビルド:
make cli      # Pure CLI binaries: dist/zee-cli and dist/zee
make gui      # GUI: dist/Zee.app (macOS) or dist/zee (Linux dual binary)

# Install to ~/.local/bin and ~/Applications (macOS)
make install
```

### Windows (PowerShell)
```powershell
git clone https://github.com/kh813/zee.git
cd zee

.\make.ps1         # Builds Windows GUI into dist/zee.exe
.\make.ps1 tui     # Builds Windows TUI into dist/zee-cli.exe
.\make.ps1 test    # Runs workspace tests
```

---

## ⚙️ Configuration / 設定

Settings are saved in `~/.config/zee/config.toml` (or `%APPDATA%\zee\config.toml` on Windows) and automatically shared between GUI and TUI:  
設定ファイルは GUI版 と CLI版 で自動的に共有されます:

```toml
language = "en"               # "en" or "ja" (言語: 英語または日本語)
theme = "tokyo-night"         # "tokyo-night", "catppuccin-latte", "dracula", "nord", etc.
line_numbers = true           # 行番号表示
word_wrap = true              # 右端折り返し
vi_mode = false               # Vi/Vim モード
tab_size = 4                  # タブ幅
expand_tab = false            # タブをスペースに展開
sidebar = false               # 起動時にサイドバーを開く
sidebar_position = "right"    # サイドバー位置 ("right" または "left")
font_size = 13.0              # GUI フォントサイズ
line_height = 20.0            # GUI 行の高さ
```

---

## 📖 User Manual & Documentation / ユーザーマニュアル

For a complete reference of keyboard shortcuts, Vi commands, menus, and configuration options, please refer to the [User Manual](MANUAL.md).  
キーボードショートカット一覧、Viコマンド、詳細な設定については [User Manual (MANUAL.md)](MANUAL.md) をご覧ください。

---

## 📄 License / ライセンス

- **Source Code / ソースコード**: Licensed under the [Apache License, Version 2.0](LICENSE).
- **Logos & Brand Assets / ロゴ・ブランド資産**: Copyright © 2026 kh813. All rights reserved. (Not covered by Apache 2.0 without explicit permission).
- **Third-Party Acknowledgements**: Built with the [GPUI](https://github.com/zed-industries/zed) framework (Apache-2.0).
