# zee (ZEpto Editor)

[![CI](https://github.com/kh813/zee/actions/workflows/ci.yml/badge.svg)](https://github.com/kh813/zee/actions/workflows/ci.yml)
[![Release](https://github.com/kh813/zee/actions/workflows/release.yml/badge.svg)](https://github.com/kh813/zee/releases)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/OS-macOS%20%7C%20Windows%20%7C%20Linux-brightgreen.svg)](https://github.com/kh813/zee/releases)

**zee** is a lightweight, modern, and lightning-fast text editor built in Rust.

---

### 💻 Supported Operating Systems (サポートOS)

| Operating System (OS) | GUI Edition (デスクトップ版) | CLI / TUI Edition (ターミナル版) | Architectures | Status |
| :--- | :--- | :--- | :--- | :--- |
| **macOS** | **Zee.app** (Metal GPU-accelerated) | **zee** (Terminal CLI) | Apple Silicon (M1/M2/M3/M4) / Intel | ✅ Supported |
| **Windows** | **zee.exe** (DirectX GPU-accelerated) | **zee.exe** (Console mode) | 64-bit (x86-64) / ARM64 | ✅ Supported |
| **Linux** | **zeeg** (Vulkan GPU-accelerated) | **zee** (Ultra-fast CLI / TUI) | x86-64 / ARM64 (aarch64) | ✅ Supported |

---

### 🌐 Multi-OS & Dual GUI / CLI Architecture

**zee** is engineered from the ground up as a **multi-OS text editor supporting both native GUI desktop applications and terminal CLI (TUI)**.

Its core design philosophy is to provide the **exact same user experience across all platforms and environments**:
- **GUI for macOS & Windows**: Native GPU-accelerated desktop experience (`Zee.app` on macOS, `zee.exe` on Windows).
- **CLI / TUI for Linux & Remote Workflows**: Ultra-fast, lightweight terminal experience (`zee`) specifically optimized for Linux environments and remote SSH workflows.
- **Unified User Experience**: Whether launching the GUI app on your Mac/Windows workstation or running the CLI over SSH on a headless Linux server, you get the **exact same menus, keyboard shortcuts, dialogs, visual themes, and intuitive editing feel**—eliminating the need to switch muscle memory between environments.

> **日本語**:  
> **zee** は、**マルチOS（macOS / Windows / Linux）**に対応し、**GUI（デスクトップアプリ）とCLI（ターミナルTUI）の双方をフルサポート**したテキストエディタです。  
> 「WindowsやMacでのリッチなGUI版」と「主にLinux環境やSSH接続先で活躍するCLI版」の間で、ショートカットキー、メニュー構成、ダイアログ、操作感に至るまで**全く同じユーザー体験**を提供することを目指して設計されています。

---

## Highlights & Features

- **Dual GUI & TUI Experience**: Native hardware-accelerated desktop GUI (`zeeg` on macOS, Windows, and Linux) and terminal TUI (`zee`) sharing 100% feature and shortcut parity.
- **Sidebar (File Tree & Multi-Language Outline)**:
  - Collapsible sidebar with directory tree browser and code symbol outline tree.
  - Full file management: create new files/folders, rename, and delete directly via right-click context menu in GUI and TUI (`m` key), keeping the file tree interface clean.
  - Dotfiles & hidden file visibility toggle (`👁` button, right-click menu, or `h` key in TUI) with state persisted to `config.toml`.
  - Instant file tree refresh (`F5` / `⌘⌥R` / `↻`) syncing external changes made in Finder/Explorer while preserving expanded folder states.
  - Multi-language outline navigation: **Markdown**, **Rust**, **Python**, **Go**, **JSON**, **YAML**, **Shell (bash/sh)**, **HTML**, and **CSS** supported out of the box, with plugin extension support.
  - Smooth vertical and horizontal scrolling across deep folder trees and complex symbol structures.
  - Position configurable on either the **left** or **right** side of the editor.
- **Starter File Templates**:
  - Quickly scaffold new projects via `File > New from Template` (Rust bin/lib, Python, Go, HTML5, Markdown, Shell).
  - Built-in macro interpolation (`{filename}`, `{date}`, `{year}`, `{author}`, `{cursor}`).
  - User-extensible template directory support (`~/.config/zee/templates/`).
- **Mouse Rectangular & Column Selection**:
  - Hold `Alt` (`Option` on macOS) + drag to select rectangular code blocks.
  - Multi-line block copy, cut, paste, deletion, and simultaneous column editing.
- **Productivity & Workflow Refinements**:
  - Direct CLI line jumps: `zee file.rs:42` or `zee +42 file.rs`.
  - Format & auto-cleanup on save: automatically trims trailing whitespace and ensures a single trailing newline (can be toggled on/off via the `Edit` menu).
  - In-app `Reload File` with unsaved change safety prompts (`⌘Shift+R` / `Ctrl+Shift+R`).
  - Persistent `Open Recent` file history.
- **Extensible WASM Plugin System**:
  - Sandboxed WebAssembly plugin runtime supporting text manipulation commands and custom outline providers.
  - Dedicated in-app **Plugin Manager** to install, inspect, and manage plugins.
  - [Plugin Development Guide](docs/plugin_development_guide.md) available for creating custom extensions.
- **In-App Self-Update with Session Restoration**:
  - One-click update check (`Help` → `Check for Updates...`) that downloads and swaps binaries in place.
  - Automatically restores open tabs, cursor positions, scroll offsets, and sidebar state after relaunch.
- **Backup & Portability**:
  - Built-in export and import of all user configuration and installed plugins into a portable archive (`.zip` / `.tar.gz`).
- **Modern Aesthetics & Built-in Themes**:
  - Beautiful built-in themes: Tokyo Night, Catppuccin Latte, Dracula, Monokai, Nord, Gruvbox, Solarized, and more.
- **Interactive Find & Replace**:
  - Real-time search panel with match count, regex, case sensitivity, and whole-word matching.
- **Multi-Tab Workspace**:
  - Open, switch, close, and navigate multiple files effortlessly.
- **Internationalization (i18n)**:
  - English and Japanese (日本語) native UI translation.
- **Fast Syntax Highlighting**:
  - Highlighting for Rust, Markdown, Python, Go, JSON, TOML, YAML, HTML, CSS, JavaScript/TypeScript, Shell, and more.
- **Optional Vi Mode**:
  - Modal editing (`Normal`, `Insert`, `Visual`, `VisualBlock`) for Vi/Vim power users.
  - Dedicated 2-row bottom bar with Ex command line (`:w [path]`, `:q[!]`, `:wq`, `:e[!]`, `:<line>`, `:noh`, `:bn`, `:bp`, `:set nu/wrap`).
  - Full Japanese IME transparency with automatic key normalization (`っ` for `dd`, `い` for `i`, full-width symbols/colons).
  - Context-aware cursor shapes: Block `█` in Normal mode, vertical bar `|` in Insert mode (Latin), and amber underscore `_` with `[あ]` status badge when CJK IME is active.
  - Tracking progress towards full POSIX.1-2017 vi compliance via [POSIX Vi Compliance Checklist](docs/POSIX_VI_COMPLIANCE.md).
- **Remote SSH & OSC 52 Clipboard**:

  - Seamless system clipboard synchronization locally and over SSH terminal connections.

---

## Installation

### Download Pre-built Binaries

Pre-compiled standalone packages and installers are available on the [Releases page](https://github.com/kh813/zee/releases).

| Platform | Type | File Name | Notes |
| :--- | :--- | :--- | :--- |
| **macOS (Apple Silicon)** | Native GUI App | `zeeg-macos-arm64.zip` | Extract and drag `Zee.app` to `/Applications` |
| **Windows (x86-64)** | Native GUI App | `zee-windows-x64.zip` | Standalone `zee.exe` Windows 64-bit desktop GUI |
| **Linux (x86-64)** | Terminal CLI / TUI | `zee-linux-x64.zip` | Standalone `zee` Linux 64-bit terminal binary |

> **Note on Extended Support Builds**: Binaries for Linux GUI (`zeeg`), macOS Terminal CLI (`zee`), Windows ARM64 (`zee.exe`), and Linux ARM64 are available on-demand via GitHub Actions Extended Support Releases or buildable directly from source.

#### macOS Gatekeeper Note
On macOS, downloaded applications outside the App Store may show a warning: *"Zee.app cannot be opened because the developer cannot be verified"*.
To resolve this:
- **Option 1**: Right-click (or Control-click) `Zee.app` in Finder, select **Open**, and click **Open** in the prompt.
- **Option 2**: Run the following command in Terminal:
  ```bash
  xattr -cr /Applications/Zee.app
  ```

---

### Build from Source

**Prerequisites**: Rust toolchain (1.80+), standard C build tools.

#### macOS & Linux (Make)
```bash
git clone https://github.com/kh813/zee.git
cd zee

# Build both GUI and TUI binaries into dist/
make

# Or build individually:
make gui      # Desktop GUI: dist/zeeg (and dist/Zee.app on macOS)
make tui      # Terminal TUI: dist/zee

# Install to system (~/.local/bin and ~/Applications on macOS)
make install
```

#### Windows (PowerShell)
```powershell
git clone https://github.com/kh813/zee.git
cd zee

.\make.ps1         # Builds Windows GUI into dist/zee.exe
.\make.ps1 tui     # Builds Windows TUI into dist/zee-cli.exe
.\make.ps1 test    # Runs workspace test suite
```

---

## Usage

### Desktop GUI
- **macOS**: Launch `Zee.app` from Applications or Spotlight.
- **Windows**: Run `zee.exe [FILE...]` or launch via application launcher / Start menu.
- **Linux**: Run `zeeg [FILE...]` or launch via application launcher.
- **Direct Line Jump**: `zee src/main.rs:42` (or `zeeg src/main.rs:42`) or `+42 src/main.rs`.

### Terminal TUI / Remote (SSH)
- Run `zee [FILE...]` in any terminal emulator.
- **Direct Line Jump**: `zee src/main.rs:42` or `zee +42 src/main.rs`.

For a complete reference of keyboard shortcuts, menus, and configuration, see the [User Manual](MANUAL.md).

---

## Configuration

Settings are saved in `~/.config/zee/config.toml` (or `%APPDATA%\zee\config.toml` on Windows) and automatically synced between GUI and TUI:

```toml
language = "en"               # "en" or "ja"
theme = "tokyo-night"         # "tokyo-night", "catppuccin-latte", "dracula", "nord", etc.
line_numbers = true
word_wrap = true
vi_mode = false
tab_size = 4
expand_tab = false
sidebar = false               # Open sidebar by default
sidebar_position = "right"    # "right" or "left"
font_size = 13.0              # GUI editor font size
line_height = 20.0
```

---

## Plugin Development

Want to extend **zee** with custom text manipulation utilities or outline providers?  
Check out our comprehensive [Plugin Development Guide](docs/plugin_development_guide.md) to build WebAssembly plugins using Rust.

---

## Remote Usage (SSH)

When running **zee** over SSH, prevent `Ctrl+S` from pausing terminal transmission by adding this line to your `~/.bashrc` or `~/.zshrc`:

```bash
stty -ixon
```

---

## License

- **Source Code**: Licensed under the [Apache License, Version 2.0](LICENSE).
- **Logos & Brand Assets**: Copyright © 2026 kh813. All rights reserved. (Not covered by Apache 2.0 without explicit permission; derivative works and forks must replace the branding and icons).
- **Third-Party Acknowledgements**: Built with the [GPUI](https://github.com/zed-industries/zed) framework (Apache-2.0).
