# zee User Manual

**zee** is a lightweight, modern GUI and TUI text editor for plain text, Markdown, and config files.  
It provides a native hardware-accelerated desktop application (`Zee.app` / `zeeg`) as well as a terminal interface (`zee`) running on macOS, Linux, and Windows — including seamlessly over SSH.

---

## Table of Contents

1. [Installation](#1-installation)
2. [Basic Usage](#2-basic-usage)
3. [Keyboard Shortcuts](#3-keyboard-shortcuts)
4. [Configuration](#4-configuration)
5. [Theme File Format](#5-theme-file-format)
6. [Syntax Definition File Format](#6-syntax-definition-file-format)
7. [Internationalization (i18n)](#7-internationalization-i18n)
8. [SSH Usage Notes](#8-ssh-usage-notes)
9. [Troubleshooting](#9-troubleshooting)
10. [Roadmap & Future Plans](#10-roadmap--future-plans)

---

## 1. Installation

### Pre-built binaries (recommended)

Download the appropriate binary from the [releases page] and place it in your `PATH`.

| Platform | Binary | Notes |
| :--- | :--- | :--- |
| **macOS Apple Silicon (GUI)** | `zeeg-v0.1.x-macos-arm64.zip` | macOS Native App Bundle (`Zee.app`). |
| **Windows x86-64 (GUI)** | `zeeg-v0.1.x-windows-x64.zip` | Windows 64-bit Native GUI (`zeeg.exe`). |
| **Linux x86-64 (CLI)** | `zee-v0.1.x-linux-x64.zip` | Linux 64-bit Terminal CLI / TUI (`zee`). |

> **Note on Extended Builds**: Extended support binaries (Linux GUI `zeeg`, macOS CLI `zee`, Windows ARM64 `zeeg.exe`, Linux ARM64) are available on-demand via GitHub Actions Extended Support Releases or buildable directly from source.

**macOS / Linux quick install:**
```bash
# Example: macOS Apple Silicon TUI
curl -L https://github.com/kh813/zee/releases/latest/download/zee-macos-arm64.tar.gz -o zee.tar.gz
tar -xzf zee.tar.gz
chmod +x zee
sudo mv zee /usr/local/bin/zee
```

> **macOS Gatekeeper**: On first run, macOS may block the binary. Right-click → Open to allow, or run:
> ```bash
> xattr -d com.apple.quarantine /usr/local/bin/zee
> ```

### Build from source

**Requirements**: Rust toolchain (`rustup`), C compiler / build-essential

#### macOS & Linux (Make)
```bash
git clone https://github.com/kh813/zee.git
cd zee

# Build both TUI and GUI for current OS
make

# Or build individual components:
make gui      # Build GUI (zeeg and Zee.app on macOS)
make tui      # Build TUI (zee)

# Install binaries to ~/.local/bin (and ~/Applications on macOS)
make install

# Outputs land in dist/
ls dist/
```

#### Windows (PowerShell - No Make Needed)
```powershell
# Using make.ps1:
.\make.ps1         # Build Windows GUI (dist/zee.exe)
.\make.ps1 tui     # Build Windows TUI (dist/zee-cli.exe)
.\make.ps1 test    # Run tests (cargo test --workspace)
.\make.ps1 package # Package into dist/zee-windows-x64.zip
.\make.ps1 clean   # Clean build artifacts

# Or directly with Cargo:
cargo build --release -p zeeg   # Windows GUI: target/release/zeeg.exe
cargo build --release -p zee-tui   # Terminal TUI: target/release/zee.exe
```

**Available make targets:**

| Target | Description |
| :--- | :--- |
| `make` / `make local` | Build both TUI (`zee`) and GUI (`zeeg` / `Zee.app`) for the local host OS into `dist/` |
| `make tui` / `make cli` | Build TUI binary (`zee`) into `dist/` |
| `make gui` | Build GUI binary (and `Zee.app` bundle on macOS) into `dist/` |
| `make test` | Run workspace unit & integration tests (`cargo test --workspace`) |
| `make check` | Run fast workspace type-checks (`cargo check --workspace --all-targets`) |
| `make install` | Install binaries to `~/.local/bin` (and `Zee.app` to `~/Applications` on macOS) |
| `make package` | Create distribution archive (`.tar.gz` / `.zip`) in `dist/` |
| `make clean` | Remove `dist/` and clean cargo cache |
| `make help` | List all make targets and usage |

> **GitHub Actions CI/CD**:
> - **CI (`.github/workflows/ci.yml`)**: Automatically triggers on PRs and pushes to `main` across macOS, Ubuntu Linux, and Windows to verify checks, tests, and builds.
> - **Releases (`.github/workflows/release.yml`)**: Automatically cross-compiles release binaries (TUI & GUI) for macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), Linux x64 (`x86_64-unknown-linux-gnu`), and Windows x64 (`x86_64-pc-windows-msvc`), generates `SHA256SUMS.txt`, and publishes them to GitHub Releases whenever a version tag (e.g. `v0.1.0`) is pushed.

---

## 2. Basic Usage

```bash
zee                      # Open with empty buffer
zee myfile.txt           # Open a file (creates empty named buffer if it does not exist)
zee file1.txt file2.txt  # Open multiple files in tabs
```

### Command-Line Behavior

| Invocation | Result |
| :--- | :--- |
| `zee` | Opens with a single empty `[No Name]` buffer |
| `zee myfile.txt` | Opens `myfile.txt`; if it does not exist, creates an empty buffer pre-named `myfile.txt` (not written to disk until you save) |
| `zee myfile.txt:42` | Opens `myfile.txt` and jumps cursor directly to line 42 |
| `zee +42 myfile.txt` | Alternative syntax to open `myfile.txt` and jump directly to line 42 |
| `zee a.txt b.txt` | Opens both files in separate tabs; first tab is active |
| `zee /some/dir` | Shows an error dialog (`"..." is a directory`) and opens an empty buffer |

### Advanced File Operations

- **Reload File (`⌘Shift+R` / `Ctrl+Shift+R`)**:
  Reloads the active file from disk. If there are unsaved local modifications, a warning dialog prompts you to confirm discarding changes.
- **Open Recent**:
  Access recently opened files via `File > Open Recent`. File history is preserved in `~/.config/zee/recent_files.json`. Choose `Clear History` to purge the list.
- **New from Template**:
  Create new files instantly pre-populated with standard skeletons via `File > New from Template`.
  - Built-in templates: Rust Binary, Rust Library, Python Script, Go Application, HTML5 Webpage, Markdown Document, Shell Script.
  - Automatically expands `{filename}`, `{date}`, `{year}`, `{author}`, and sets initial cursor position at `{cursor}`.
  - Custom templates: Place `.tmpl` or text files in `~/.config/zee/templates/` (or choose `File > New from Template > Open Templates Directory...`).

### Sidebar: File Tree & Multi-Language Outline

Toggle the sidebar with `View > [x] Sidebar` or `Ctrl+B` / `⌘B`:
- **Files Tab**:
  Interactive file tree explorer for the workspace directory. Supports folder expansion, click-to-open, and smooth vertical/horizontal scrolling.
- **Outline Tab**:
  Fast code symbol navigator. Automatically recognizes and extracts code structures:
  - **Markdown**: Heading hierarchy (`#`, `##`, etc.).
  - **Rust**: Modules (`mod`), structs, enums, impl blocks, and functions (`fn`).
  - **Python**: Classes (`class`) and functions/methods (`def`).
  - **Go**: Structs, interfaces (`type`), functions, and methods (`func`).
  - **JSON**: Key hierarchy and nested objects/arrays.
  - **HTML**: `<title>` and heading tags (`<h1>`–`<h6>`).
  - **CSS**: `@media` rules and top-level selectors.
  - *Extensible*: Additional language parsers or overrides can be loaded via WebAssembly plugins.

### Mouse Block (Rectangular) Selection

In the desktop GUI, hold the `Alt` key (Option on macOS) while dragging the mouse:
- Initiates **Visual Block** rectangular selection across multiple lines and columns.
- Supports multi-line block Copy (`⌘C` / `Ctrl+C`), Cut (`⌘X` / `Ctrl+X`), and Paste (`⌘V` / `Ctrl+V`).
- Typing characters replaces the rectangular block across all selected lines simultaneously.
- Pressing `Backspace` or `Delete` deletes the rectangular block.

### UI Layout

```
┌─────────────────────────────────────────────┐
│ File   Edit   View   Help                   │  ← Menu Bar
├─────────────────────────────────────────────┤
│ [+] main.rs × │ README.md ×                │  ← Tab Bar
├─────────────────────────────────────────────┤
│ Find:    [___________________________] ...  │  ← Find/Replace Panel (optional)
├─────────────────────────────────────────────┤
│  1 │ fn main() {                            │
│  2 │     println!("Hello");                 │  ← Editor Area
│  3 │ }                                      │
├─────────────────────────────────────────────┤
│ [+] main.rs        Ln 2, Col 5  UTF-8  LF  Rust │  ← Status Bar
└─────────────────────────────────────────────┘
```

- **Tab Bar**: `[+]` prefix means unsaved changes. Click tabs to switch; middle-click to close.
- **Status Bar**: shows cursor position, encoding, line ending, and active syntax.
- **Find/Replace Panel**: appears when `Ctrl+F` or `Ctrl+H` is pressed; pushes the editor area down (never overlaps text).

---

## 3. Keyboard Shortcuts

### Menu Bar

| Action | Shortcut |
| :--- | :--- |
| Open File menu | `Alt+F` |
| Open Edit menu | `Alt+E` |
| Open View menu | `Alt+V` |
| Open Help menu | `Alt+H` |
| Move between menus (when open) | `←` / `→` |
| Move within dropdown | `↑` / `↓` |
| Activate item | `Enter` |
| Close menu | `Esc` |

### File

| Action | Shortcut |
| :--- | :--- |
| New Tab | `Ctrl+T` (`⌘T` on macOS) |
| New from Template… | `File > New from Template` |
| New Window (GUI) | `Ctrl+N` (`⌘N` on macOS) |
| Open… | `Ctrl+O` (`⌘O` on macOS) |
| Open Recent | `File > Open Recent` |
| Reload File | `Ctrl+Shift+R` (`⌘Shift+R` on macOS) |
| Save | `Ctrl+S` (`⌘S` on macOS) |
| Save As… | `Ctrl+Shift+S` (`⌘Shift+S` on macOS) |
| Close Tab | `Ctrl+W` (`⌘W` on macOS) |
| Exit / Quit | `Ctrl+Q` (`⌘Q` on macOS) |

> **Note**: On macOS, use the Command key (`⌘`) instead of `Ctrl` for all shortcuts (e.g., `⌘T` for New Tab, `⌘N` for New Window, `⌘S` for Save).

### Edit

| Action | Shortcut |
| :--- | :--- |
| Undo | `Ctrl+Z` (`⌘Z` on macOS) |
| Redo | `Ctrl+Y` (`⌘Shift+Z` / `⌘Y` on macOS) |
| Cut | `Ctrl+X` (`⌘X` on macOS) |
| Copy | `Ctrl+C` (`⌘C` on macOS) |
| Paste | `Ctrl+V` (`⌘V` on macOS) |
| Select All | `Ctrl+A` (`⌘A` on macOS) |
| Find | `Ctrl+F` (`⌘F` on macOS) |
| Find & Replace | `Ctrl+R` / `Ctrl+Shift+F` (`⌘R` / `⌘Shift+F` on macOS) |

> **Paste behavior**: Line endings in pasted text are automatically normalized to match the current buffer's line ending setting (LF, CRLF, or CR).

### Navigation & View

| Action | Shortcut |
| :--- | :--- |
| Help / About | `Ctrl+H` (`⌘H` on macOS) |
| Toggle Vi Mode | `Ctrl+I` (`⌘I` on macOS) |
| Preferences / Settings | `Ctrl+,` (`⌘,` on macOS) |
| Zoom In | `Ctrl+=` / `Ctrl++` (`⌘=` on macOS) |
| Zoom Out | `Ctrl+-` (`⌘-` on macOS) |
| Reset Zoom | `Ctrl+0` (`⌘0` on macOS) |
| Go to Line… | `Ctrl+G` |
| New tab | `Ctrl+T` |
| Next tab | `Ctrl+Tab` |
| Previous tab | `Ctrl+Shift+Tab` |
| Toggle Line Numbers | `View > [x] Line Numbers` |
| Toggle Word Wrap | `View > [x] Word Wrap` |

### Vi / Vim Mode Commands (Normal, Visual, Visual Line & Visual Block Modes)

Toggle Vi Mode with `Ctrl+I` (`⌘I` on macOS) or via `View > [x] Vi Mode`.

#### Normal Mode
- **Motions**: `h` / `j` / `k` / `l` (left/down/up/right), `w` / `b` / `e` (word forward/backward/end), `0` / `^` (line start/first non-blank), `$` (line end), `gg` / `G` (document start/end).
- **Mode Switching**: 
  - `i` (insert at cursor), `I` (insert at line start), `a` (append after cursor), `A` (append at line end), `o` (open newline below), `O` (open newline above).
  - `v` (Visual character mode), `V` (Visual Line mode), `Ctrl+V` / `⌘V` (Visual Block / rectangular selection mode).
  - `Esc` (clear selection/pending operator).
- **Operators & Deletion**:
  - `x` (delete character), `r<char>` (replace character).
  - `dw` (delete word), `de` (delete to word end), `db` (delete to word start), `d$` / `D` (delete to line end), `d0` / `d^` (delete to line start), `dd` (delete line).
- **Change**:
  - `cw` / `ce` / `cb` / `c$` / `c0` / `c^` (change motion to Insert mode), `C` (change to line end), `cc` / `S` (change whole line), `s` (substitute character).
- **Yank & Put (Clipboard)**:
  - `yw` / `ye` / `yb` / `y$` / `y0` / `y^` (yank motion), `yy` / `Y` (yank whole line).
  - `p` (paste after cursor / below line), `P` (paste before cursor / above line).
- **Other**: `u` (undo), `J` (join next line).

#### Visual & Visual Line Modes (`v` / `V`)
- **Navigation & Selection**: `h` / `j` / `k` / `l`, `w` / `b` / `e`, `0`, `$` to expand/contract selection.
- **Operations on Selection**: `y` (yank), `d` / `x` (cut), `c` / `s` (cut and enter Insert mode), `p` (replace selection with clipboard), `Esc` (return to Normal mode).

#### Visual Block Mode (Rectangular Selection / `Ctrl+V` or `⌘V`)
- **Navigation**: `h` / `j` / `k` / `l`, `0`, `$` to select a rectangular column area across multiple lines.
- **Block Operations**:
  - `d` / `x` (delete rectangular column block).
  - `y` (yank rectangular block to clipboard).
  - `c` / `s` (delete rectangular block and enter Insert mode).
  - `I` (insert text before the rectangular block on all selected lines).
  - `A` (append text after the rectangular block on all selected lines).
  - `p` (replace rectangular block).
  - `Esc` (exit to Normal mode).

### Find/Replace Panel

| Action | Shortcut |
| :--- | :--- |
| Next match (downward) | `Enter` / `F3` |
| Previous match (upward) | `Shift+Enter` / `Shift+F3` |
| Close panel | `Esc` |
| Cycle focus (inputs ↔ toggles) | `Tab` / `Shift+Tab` |

### Mouse

| Action | Behavior |
| :--- | :--- |
| Click | Move cursor |
| Click + drag | Select text |
| Alt (Option) + click + drag | Rectangular block selection (GUI) |
| Double-click | Select word |
| Triple-click | Select line |
| Shift + click | Extend selection |
| Scroll wheel | Scroll vertically |
| Shift + scroll | Scroll horizontally (word wrap off only) |
| Click line number | Select entire line |
| Middle-click tab | Close tab |

---

## 4. Configuration

All configuration lives in `~/.config/zee/config.toml`.

- If the file **exists**, **zee** reads it at startup and applies any keys it finds over the built-in defaults.
- If the file **does not exist**, **zee** runs on built-in defaults — the file is not created automatically on startup.
- Unknown keys are silently ignored. Missing keys fall back to their default values.

**Runtime config writes**: When you change a persistent setting at runtime (e.g., toggling Line Numbers or switching Theme via the View menu), **zee** writes that change back to `~/.config/zee/config.toml` automatically. If the file does not exist yet, it is created at that point with only the changed key(s).

To start customizing manually, copy the template shipped with **zee** and edit it:

```bash
cp /path/to/zee/assets/config.toml.default ~/.config/zee/config.toml
```

### All Configuration Keys

```toml
# ~/.config/zee/config.toml

# UI language (see Section 7 for available locales)
language = "en"

# Active theme (must match a filename in ~/.config/zee/themes/ without .toml)
# Built-in themes: "tokyo-night", "light", "solarized-dark", "solarized-light",
#                  "catppuccin-mocha", "catppuccin-latte"
theme = "tokyo-night"

# Show line numbers in the left gutter (can be toggled at runtime via View menu)
line_numbers = true

# Enable vi keybindings (Normal / Insert / Visual modes)
vi_mode = false

# Wrap long lines in the editor area (default: true)
word_wrap = true

# Tab stop width in display cells (1–16)
# Controls how wide a tab character (\t) appears on screen.
tab_size = 4

# Expand tab key press to spaces on input.
# true  = pressing the Tab key inserts spaces (tab_size spaces wide).
# false = pressing the Tab key inserts a literal \t character.
# Note: this affects what is inserted when you press Tab. The actual characters
# already in the file are always preserved as-is on disk regardless of this setting.
expand_tab = false

# Auto-cleanup on save (can also be toggled directly via File menu)
# Automatically trim trailing spaces and tabs from lines when saving.
trim_trailing_whitespace = true
# Ensure file ends with a trailing newline character (\n) when saving.
ensure_final_newline = true

# GUI Font & Spacing Settings (zeeg only; ignored by zee-tui)
# font_family = "Menlo"      # Editor monospace font family (null = system monospace)
font_size = 14.0             # Editor font size in pixels (default: 14.0)
line_height = 22.0           # Editor line height in pixels (default: 22.0)
# ui_font_family = ".AppleSystemUIFont" # UI font family
ui_font_size = 13.0          # UI font size in pixels (default: 13.0)
```

> **Note**: All config files are loaded at startup only. Changes to `config.toml` take effect after restarting `zee`, except for settings changed via the View menu which are applied immediately.

### Directory Structure

```
~/.config/zee/
├── config.toml               ← main config
├── themes/
│   ├── tokyo-night.toml      ← built-in
│   ├── light.toml            ← built-in
│   ├── solarized-dark.toml   ← built-in
│   ├── solarized-light.toml  ← built-in
│   ├── catppuccin-mocha.toml ← built-in
│   ├── catppuccin-latte.toml ← built-in
│   └── my-theme.toml         ← your custom theme
├── syntax/
│   ├── plain-text.toml  ← built-in
│   ├── markdown.toml    ← built-in
│   ├── rust.toml        ← built-in
│   ├── toml.toml        ← built-in
│   ├── python.toml      ← built-in
│   ├── go.toml          ← built-in
│   ├── swift.toml       ← built-in
│   ├── javascript.toml  ← built-in
│   ├── html.toml        ← built-in
│   ├── css.toml         ← built-in
│   ├── xml.toml         ← built-in
│   └── my-lang.toml     ← your custom syntax definition
└── locales/
    └── fr.toml          ← your custom locale (optional)
```

---

## 5. Theme File Format

Theme files live in `~/.config/zee/themes/*.toml`. Built-in and custom user themes are automatically loaded and selectable from the **View > Theme** menu and `config.toml` in both **GUI** and **CLI/TUI** modes.

Colors can be specified in multiple standard formats:
- **CSS Hex**: `"#rgb"`, `"#rrggbb"`, or `"#rrggbbaa"` (e.g. `"#1a1b26"`, `"#fff"`, `"#1a1b2680"`)
- **CSS RGB / RGBA**: `"rgb(26, 27, 38)"`, `"rgba(26, 27, 38, 0.5)"`
- **ANSI**: `"ansi(1)"` (0–255 color index) or `"ansi(red)"` / `"ansi(bright_blue)"`
- **Named CSS Colors**: `"black"`, `"white"`, `"red"`, `"green"`, `"blue"`, `"yellow"`, `"magenta"`, `"cyan"`, `"gray"`

### Full Schema

```toml
# ~/.config/zee/themes/my-theme.toml

[meta]
name        = "My Theme"          # Display name shown in View > Theme menu
author      = "Your Name"         # Optional
version     = "1.0"               # Optional

[editor]
background  = "#1a1b26"           # Editor area background
foreground  = "#c0caf5"           # Default text color
cursor      = "#c0caf5"           # Cursor color (block / beam)
selection   = "#283457"           # Selected text background
line_number = "#3b4261"           # Gutter line number color
current_line = "#1e2030"          # Background of the line the cursor is on (optional)

[ui]
menu_bar_bg         = "#16161e"   # Menu bar background
menu_bar_fg         = "#c0caf5"   # Menu bar text
menu_item_active_bg = "#7aa2f7"   # Highlighted menu item background
menu_item_active_fg = "#1a1b26"   # Highlighted menu item text
tab_bar_bg          = "#16161e"   # Tab bar background
tab_active_bg       = "#1a1b26"   # Active tab background
tab_active_fg       = "#c0caf5"   # Active tab text
tab_inactive_bg     = "#16161e"   # Inactive tab background
tab_inactive_fg     = "#565f89"   # Inactive tab text
status_bar_bg       = "#16161e"   # Status bar background
status_bar_fg       = "#c0caf5"   # Status bar text
panel_bg            = "#16161e"   # Find/Replace panel background
panel_fg            = "#c0caf5"   # Find/Replace panel text
panel_error_fg      = "#f7768e"   # Find input text color when no matches found
dialog_bg           = "#1e2030"   # Dialog background
dialog_border       = "#7aa2f7"   # Dialog border color
button_active_bg    = "#7aa2f7"   # Active/focused button background
button_active_fg    = "#1a1b26"   # Active/focused button text

[syntax]
# Token type colors. All keys are optional; omitted keys fall back to editor.foreground.
keyword     = "#bb9af7"
type_name   = "#2ac3de"
function    = "#7aa2f7"
string      = "#9ece6a"
number      = "#ff9e64"
comment     = "#565f89"
operator    = "#89ddff"
punctuation = "#c0caf5"
constant    = "#ff9e64"
attribute   = "#bb9af7"
error       = "#f7768e"
```

### Minimal Theme (only required keys)

```toml
[meta]
name = "Minimal Dark"

[editor]
background  = "#1c1c1c"
foreground  = "#d4d4d4"
cursor      = "#d4d4d4"
selection   = "#264f78"
line_number = "#5a5a5a"

[ui]
status_bar_bg = "#007acc"
status_bar_fg = "#ffffff"
```

All other keys default to reasonable fallback values derived from `editor.foreground` and `editor.background`.

---

## 6. Syntax Definition File Format

Syntax definition files live in `~/.config/zee/syntax/*.toml`.  
They map **file extensions** to a set of **regex-based token rules**.

### Built-in Languages (18 Languages)

**zee** includes built-in syntax definitions for:
- **Programming & Scripting**: Rust, Python, JavaScript, TypeScript, Go, C, C++, Shell Script (Bash/Zsh/sh), SQL
- **Markup & Data**: HTML, CSS, Markdown, JSON, YAML, TOML
- **Config & DevOps**: Dockerfile, Makefile, Diff / Patch

### Full Schema

```toml
# ~/.config/zee/syntax/python.toml

[meta]
name        = "Python"
extensions  = ["py", "pyw"]

# Rules are evaluated in order; the first match wins.
# The regex engine is Rust's `regex` crate (RE2 syntax — no backtracking).

[[rule]]
token = "comment"
pattern = "#.*"

[[rule]]
token = "string"
start   = '"""'
end     = '"""'

[[rule]]
token = "string"
start   = "'''"
end     = "'''"

[[rule]]
token = "string"
pattern = '"(?:[^"\\]|\\.)*"'

[[rule]]
token = "string"
pattern = "'(?:[^'\\]|\\.)*'"

[[rule]]
token = "keyword"
pattern = "def|class|if|elif|else|for|while|return|import|from|as|with|in|not|and|or|is|None|True|False|lambda|yield|pass|break|continue|try|except|finally|raise|del|global|nonlocal|assert"
word_boundary = true

[[rule]]
token = "type_name"
pattern = '\b[A-Z][a-zA-Z0-9_]*\b'

[[rule]]
token = "number"
pattern = '\b(?:0x[0-9a-fA-F]+|0o[0-7]+|0b[01]+|\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)\b'

[[rule]]
token = "function"
pattern = '(?<=def )[a-zA-Z_][a-zA-Z0-9_]*'

[[rule]]
token = "operator"
pattern = '[+\-*/%=<>!&|^~]+'

[[rule]]
token = "attribute"
pattern = '@[a-zA-Z_][a-zA-Z0-9_.]*'
```

### Token Types Reference

| Token | Suggested use |
| :--- | :--- |
| `keyword` | Reserved words (`fn`, `if`, `class`, …) |
| `type_name` | Type names, structs, classes |
| `function` | Function and method names |
| `string` | String and character literals |
| `number` | Numeric literals |
| `comment` | Line comments and block comments |
| `operator` | Operators and arrows |
| `punctuation` | Brackets, commas, semicolons |
| `constant` | Constants, enum variants, `true`/`false` |
| `attribute` | Decorators, annotations, attributes |
| `error` | Syntax error markers |

### Rule Types Reference

| Key | Behavior |
| :--- | :--- |
| `pattern` | Single regex match; highlights the entire match |
| `start` + `end` | Region match; highlights from `start` to `end` (inclusive), spanning multiple lines |
| `word_boundary = true` | Wraps `pattern` in `\b...\b` automatically (default: `false`) |
| `escape = "..."` | Region rules only. A single character that escapes the next character inside a region, preventing `end` from matching. Example: `escape = "\\"` means `\"` does not close a `"..."` string. |

### Rule Matching Priority

Rules are evaluated in file order — **first matching rule wins**.

1. More specific rules should come before general ones
2. Region rules (`start`/`end`) take precedence over `pattern` rules at the same position
3. Regions do not nest by default

> **Tip**: Place comment and string region rules near the top of your rule list.

> **Performance note**: Rules are compiled once at startup using the `regex` crate and applied in parallel with `rayon`. Prefer anchored patterns; avoid heavy backtracking.

---

## 7. Internationalization (i18n)

**zee** loads UI strings from a locale file at startup.

### Selecting a Language

```toml
# ~/.config/zee/config.toml
language = "ja"   # Japanese
```

### Built-in Locales

| Code | Language |
| :--- | :--- |
| `en` | English (default) |
| `ja` | Japanese |

### Custom Locale File Format

Create `~/.config/zee/locales/<code>.toml`. Any key omitted falls back to the `en` built-in.

```toml
# ~/.config/zee/locales/en.toml
# Complete English built-in locale — use as reference for translations.

[meta]
language    = "en"
name        = "English"
author      = "zee contributors"

[menu]
file        = "File"
edit        = "Edit"
view        = "View"
help        = "Help"

[menu.file]
new         = "New"
open        = "Open…"
save        = "Save"
save_as     = "Save As…"
close       = "Close"
exit        = "Exit"

[menu.edit]
undo        = "Undo"
redo        = "Redo"
cut         = "Cut"
copy        = "Copy"
paste       = "Paste"
find        = "Find…"
replace     = "Replace…"
select_all  = "Select All"

[menu.view]
go_to_line   = "Go to Line…"
line_numbers = "Line Numbers"
word_wrap    = "Word Wrap"
vi_mode      = "Vi Mode"
encoding     = "Encoding"
line_ending  = "Line Ending"
theme        = "Theme"
syntax       = "Syntax"

[panel]
find            = "Find:"
replace         = "Replace:"
prev            = "< Prev"
next            = "> Next"
replace_one     = "Replace"
replace_all     = "Replace All"
close           = "Close"
match_case      = "Match Case"
whole_word      = "Whole Word"
use_regex       = "Use Regex"

[status]
no_name               = "[No Name]"
no_matches            = "No matches"
search_wrapped_top    = "Search wrapped to top"
search_wrapped_bottom = "Search wrapped to bottom"
matches               = "{current} of {total} matches"
replaced_count        = "{n} replacement(s) made"
terminal_too_small    = "Terminal too small ({cols}x{rows}). Please resize."
cursor                = "Ln {line}, Col {col}"
selection             = "{n} chars"

[error]
cannot_open_dir       = "Cannot open directory: {path}"
failed_to_open        = "Failed to open {path}: {error}"

[dialog]
ok                      = "OK"
cancel                  = "Cancel"
yes                     = "Yes"
no                      = "No"
save                    = "Save"
dont_save               = "Don't Save"
discard_reopen          = "Discard & Reopen"
discard_reopen_prompt   = "Discard unsaved changes and reopen?"
reopen_file             = "Reopen File"
open_file               = "Open File…"
save_as                 = "Save As…"
go_to_line              = "Go to Line"
about                   = "About"
show_hidden             = "Show Hidden"
detect_encoding         = "Detect Encoding"
overwrite_prompt        = "File already exists. Overwrite?"
unsaved_changes_title   = "Unsaved Changes"
unsaved_changes         = "Unsaved changes in \"{filename}\"."

[dialog.file_browser]
name                    = "Name"
size                    = "Size"
modified                = "Modified"
filename                = "File name"

[about]
version     = "Version"
license     = "License"
```

---

## 8. SSH Usage Notes

### Clipboard over SSH (OSC 52)

**zee** supports clipboard sharing over SSH via the **OSC 52** escape sequence.  
When you copy text (`Ctrl+C`), **zee** sends an OSC 52 sequence that instructs your **local** terminal emulator to place the text in your local clipboard. This happens alongside a regular platform clipboard write — both are always attempted.

**Supported terminal emulators**:
- iTerm2 (macOS) — enable in Preferences → General → Applications in terminal may access clipboard
- WezTerm — enabled by default
- Windows Terminal — enabled by default

If your terminal does not support OSC 52, `Ctrl+C` still copies to the remote clipboard (accessible within the same SSH session).

### Ctrl+S Freezing

Some shell configurations interpret `Ctrl+S` as `XOFF` (pause output), causing the terminal to appear frozen. **zee** disables this via raw mode, but as a precaution add this to your `~/.bashrc` or `~/.zshrc`:

```bash
stty -ixon
```

### Performance

**zee** uses diff-based rendering (only changed screen cells are redrawn), minimizing bytes sent over the network. It performs well on connections with up to several hundred milliseconds of latency.

---

## 9. Troubleshooting

### Terminal appears frozen after Ctrl+S
Press `Ctrl+Q` to unfreeze (XON), then add `stty -ixon` to your shell profile. See [Section 8](#8-ssh-usage-notes).

### Japanese/Chinese text displays with wrong column alignment
Ensure your terminal emulator uses a **monospace font with CJK support** (e.g., Noto Mono, Sarasa Mono, HackGen). **zee** uses Unicode display-cell widths (CJK = 2 cells); the terminal font must agree.

### Terminal too small message
Resize your terminal to at least **40 columns × 24 rows**. **zee** resumes automatically.

### Theme or syntax not appearing in menu
Check that the `.toml` file is in the correct directory and restart **zee**. Config files are loaded at startup only.

### Mouse not working over SSH
Ensure your SSH client passes through mouse escape sequences. In PuTTY, enable "xterm-style mouse reporting" in Terminal → Features settings.

### Undo clears the unsaved-changes indicator
This is expected behavior. When you undo all changes since the last save, the file is back to its saved state and `[+]` is removed from the tab and status bar.

---

## 10. Roadmap & Future Plans

The following features and improvements are planned for upcoming releases:

### 1. Keybinding Customization System
- Configuration-based keybindings in `~/.config/zee/keybindings.toml` (or inside `config.toml`).
- Remappable shortcuts for all core actions, menu items, and plugin commands.
- Support for multi-stroke key sequences and mode-specific bindings (GUI, TUI, Vi mode).

### 2. Everyday Line Editing Operations
In coordination with the keybinding customization system, convenient everyday line operations will be added:
- **Duplicate Line / Selection**: Quick duplication of the current line or selection (e.g. `Ctrl+Shift+D` / `Cmd+Shift+D`).
- **Move Line Up / Down**: Move current line or block of lines up or down (e.g. `Alt+Up` / `Alt+Down`).
- **Delete Line**: Instantly delete the current line without leaving an empty line (e.g. `Ctrl+Shift+K` / `Cmd+Shift+K`).
- **Toggle Line Comment**: Language-aware single line / block commenting (e.g. `Ctrl+/` / `Cmd+/`).
- **Join Lines**: Join current line with the line below (`Ctrl+J` or Vi `J`).

### 3. Native GUI Multi-Tab Drag & Drop
- Rearrange editor tabs via native mouse drag and drop.
- Detach tabs into separate windows.
