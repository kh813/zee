# Zee プラグイン開発ガイド (Plugin Development Guide)

本ドキュメントは、Zee エディタ向けの拡張プラグイン（WebAssembly / WASM）を開発するための公式ガイドです。

Zee のプラグインシステムは、安全で高速な **WebAssembly (WASM)** サンドボックス上で動作します。Rust などの言語で記述し、`.wasm` にコンパイルするだけで、テキスト変換やコードアウトライン解析などの独自機能を Zee に追加できます。

---

## 目次
1. [プラグインの概要と仕組み](#1-プラグインの概要と仕組み)
2. [プラグインの配置場所](#2-プラグインの配置場所)
3. [クイックスタート（最小プロジェクトの作成）](#3-クイックスタート最小プロジェクトの作成)
4. [マニフェスト (`plugin.toml`) の書き方](#4-マニフェスト-plugintoml-の書き方)
5. [Rust による実装（ボイラープレート）](#5-rust-による実装ボイラープレート)
6. [機能の実装例](#6-機能の実装例)
   - [A. テキスト変換コマンドの実装](#a-テキスト変換コマンドの実装)
   - [B. アウトラインプロバイダの実装](#b-アウトラインプロバイダの実装)
7. [ビルドとインストール](#7-ビルドとインストール)
8. [動作確認とデバッグ](#8-動作確認とデバッグ)
9. [Component Model (WIT) への対応について](#9-component-model-wit-への対応について)

---

## 1. プラグインの概要と仕組み

Zee のプラグインは以下の特徴を持っています：

- **完全なサンドボックス**: WASM ランタイム内で隔離されて実行されるため、クラッシュしてもエディタ本体を巻き込みません。
- **エディタとの連携**:
  - **テキスト変換**: 選択範囲（またはファイル全体）を受け取り、加工して瞬時にバッファへ反映（Undo/Redo にも完全対応）。
  - **メニュー統合**: エディタのメニューバー「Plugins」にコマンドが自動表示され、ショートカットやクリックで実行可能。
  - **アウトライン表示**: Markdown の見出しやプログラミング言語の関数ツリーを抽出し、サイドパネルの「Outline」タブに階層ツリーとして描画（クリックで対象行へジャンプ）。

---

## 2. プラグインの配置場所

Zee は起動時、および設定画面から以下のディレクトリを自動スキャンしてプラグインを読み込みます。

- **macOS / Linux**: `~/.config/zee/plugins/`
- **Windows**: `%APPDATA%\zee\plugins\`

### 配置形式
プラグインは次のいずれかの形式で配置します：

1. **ディレクトリ形式（推奨）**:
   ```
   ~/.config/zee/plugins/my-plugin/
   ├── plugin.toml  # プラグインのメタデータ定義
   └── plugin.wasm  # ビルドされた WASM バイナリ
   ```
2. **単体ファイル形式**:
   ```
   ~/.config/zee/plugins/my-plugin.wasm
   ```

> [!TIP]
> Zee GUI のメニュー `Help`（または macOS のアプリメニュー `Zee`） -> `Manage Plugins...` を開き、画面下の **「Open Plugins Folder」** をクリックすると、このフォルダを Finder やエクスプローラーで直接開くことができます。

---

## 3. クイックスタート（最小プロジェクトの作成）

Rust を使用して新しいプラグインを作成する手順です。

### 3.1 前提条件
Rust ツールチェーンと WASM ターゲットがインストールされている必要があります：

```bash
rustup target add wasm32-unknown-unknown
```

### 3.2 プロジェクトの初期化
```bash
cargo new --lib my-zee-plugin
cd my-zee-plugin
```

### 3.3 `Cargo.toml` の設定
`cdylib` 形式でビルドするように設定し、JSON 処理用の `serde` を追加します：

```toml
[package]
name = "my-zee-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

---

## 4. マニフェスト (`plugin.toml`) の書き方

プロジェクトルートに `plugin.toml` を作成します。ここでプラグインの名前、提供するコマンド、対象言語を宣言します。

```toml
id = "my-plugin"
name = "My Custom Utilities"
version = "0.1.0"
description = "Text manipulation utilities for Zee"
languages = ["txt", "md", "rust", "json"]

[capabilities]
# サイドパネルのアウトライン解析を提供するかどうか
outline_provider = false

# メニューバー「Plugins」に表示されるコマンド一覧
commands = [
    "reverse_text",
    "wrap_quotes"
]
```

### 各項目の説明
- `id`: プラグインの一意の ID（小文字アルファベット、数字、ハイフン）。
- `name`: メニューやプラグインマネージャーに表示される名称。
- `version`: バージョン番号。
- `description`: 簡単な説明文。
- `languages`: プラグインが対象とする拡張子一覧（アウトラインプロバイダ等の自動適用判定に使用）。
- `capabilities.commands`: プラグインが提供するコマンド名の一覧。ここに書いた文字列がそのままエディタの「Plugins」メニューに表示されます。
- `capabilities.outline_provider`: アウトライン抽出関数 `zee_parse_outline` を実装する場合に `true` にします。

---

## 5. Rust による実装（ボイラープレート）

Zee と WASM 間で文字列を安全に受け渡しするための基本ボイラープレートです。
`src/lib.rs` の先頭に以下のメモリ管理コードを含めます。

```rust
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ホスト (Zee) <-> プラグイン間のメモリ管理 ABI
// ---------------------------------------------------------------------------

/// ホストがゲストメモリを確保するためのアロケータ
#[no_mangle]
pub extern "C" fn zee_alloc(len: i32) -> *mut u8 {
    if len <= 0 {
        return std::ptr::null_mut();
    }
    let mut buf = std::mem::ManuallyDrop::new(Vec::<u8>::with_capacity(len as usize));
    buf.as_mut_ptr()
}

/// ホストが不要になったゲストメモリを解放するためのデアロケータ
/// # Safety
/// ptr は zee_alloc または pack_result で生成された有効なポインタである必要があります。
#[no_mangle]
pub unsafe extern "C" fn zee_dealloc(ptr: *mut u8, len: i32) {
    if !ptr.is_null() && len > 0 {
        drop(Vec::from_raw_parts(ptr, 0, len as usize));
    }
}

/// 結果バッファをリークさせ、ポインタと長さを 64bit 値にパックしてホストへ返却
fn pack_result(bytes: Vec<u8>) -> u64 {
    if bytes.is_empty() {
        return 0;
    }
    let boxed = std::mem::ManuallyDrop::new(bytes.into_boxed_slice());
    let len = boxed.len() as u64;
    let ptr = boxed.as_ptr() as u64;
    (ptr << 32) | (len & 0xFFFF_FFFF)
}

/// ホストから渡されたポインタと長さを &str として参照
unsafe fn read_str<'a>(ptr: *const u8, len: i32) -> Option<&'a str> {
    if ptr.is_null() || len <= 0 {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len as usize)).ok()
}
```

---

## 6. 機能の実装例

### A. テキスト変換コマンドの実装

エディタ上のテキストを変換するコマンドです。
エクスポート関数 `zee_transform_text` を定義します：

```rust
/// # Safety
/// cmd_ptr, text_ptr は有効な UTF-8 バイト列を指している必要があります。
#[no_mangle]
pub unsafe extern "C" fn zee_transform_text(
    cmd_ptr: *const u8,
    cmd_len: i32,
    text_ptr: *const u8,
    text_len: i32,
) -> u64 {
    match (read_str(cmd_ptr, cmd_len), read_str(text_ptr, text_len)) {
        (Some(cmd), Some(text)) => {
            let result = transform(cmd, text);
            pack_result(result.into_bytes())
        }
        _ => 0,
    }
}

/// 実際の変換ロジック
fn transform(cmd: &str, input: &str) -> String {
    match cmd {
        "reverse_text" => input.chars().rev().collect(),
        "wrap_quotes" => format!("\"{}\"", input),
        _ => input.to_string(), // 未知のコマンドはそのまま返す
    }
}
```

- ユーザーがテキストを選択している場合は**その選択範囲**が、選択していない場合は**ファイル全体**が `input` に渡されます。
- 返した文字列がそのままエディタに置換挿入されます。

---

### B. アウトラインプロバイダの実装

サイドバーの「Outline」タブにシンボル構造を表示する機能です。
`plugin.toml` で `outline_provider = true` と指定し、`zee_parse_outline` をエクスポートします。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineNode {
    pub title: String,            // 表示名 (例: "# 見出し", "fn my_function")
    pub level: usize,             // 階層深度 (1 が最上位)
    pub line: usize,              // クリック時のジャンプ先行番号 (0-indexed)
    pub children: Vec<OutlineNode>, // 子ノード
    pub is_expanded: bool,        // 初期表示でツリーを展開するか
}

/// # Safety
/// ptr は有効なファイル内容 UTF-8 文字列を指している必要があります。
#[no_mangle]
pub unsafe extern "C" fn zee_parse_outline(ptr: *const u8, len: i32) -> u64 {
    let Some(text) = read_str(ptr, len) else {
        return 0;
    };
    
    let nodes = parse_my_outline(text);
    match serde_json::to_vec(&nodes) {
        Ok(json_bytes) => pack_result(json_bytes),
        Err(_) => 0,
    }
}

/// 例: 行頭が "fn " で始まる行を抽出するシンプルなパーサー
fn parse_my_outline(content: &str) -> Vec<OutlineNode> {
    let mut nodes = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ") {
            let fn_name = trimmed.trim_start_matches("fn ").split('(').next().unwrap_or(trimmed);
            nodes.push(OutlineNode {
                title: format!("fn {}", fn_name.trim()),
                level: 1,
                line: line_idx,
                children: Vec::new(),
                is_expanded: true,
            });
        }
    }
    nodes
}
```

---

## 7. ビルドとインストール

### 7.1 WebAssembly にコンパイル
```bash
cargo build --target wasm32-unknown-unknown --release
```

ビルド成果物は `target/wasm32-unknown-unknown/release/my_zee_plugin.wasm` に出力されます。

### 7.2 プラグインディレクトリへ配置
```bash
# プラグインディレクトリを作成
mkdir -p ~/.config/zee/plugins/my-plugin

# WASM バイナリとマニフェストを配置
cp target/wasm32-unknown-unknown/release/my_zee_plugin.wasm ~/.config/zee/plugins/my-plugin/plugin.wasm
cp plugin.toml ~/.config/zee/plugins/my-plugin/plugin.toml
```

> [!NOTE]
> ファイル名は `plugin.wasm` または `<ディレクトリ名>.wasm` とすることで正しく認識されます。

---

## 8. 動作確認とデバッグ

1. **Zee を起動（または再起動）**:
   - 上部メニューバーに **「Plugins」** が追加され、`plugin.toml` に定義したコマンド一覧（例: `My Custom Utilities: reverse_text`）が表示されます。
2. **実行テスト**:
   - テキストを選択してメニューのコマンドをクリックすると、即座に文字列が変換されます。
   - `Cmd+Z` / `Ctrl+Z` で直前の変換を取り消せることを確認してください。
3. **プラグインマネージャーで確認**:
   - `Help` -> `Manage Plugins...` から、プラグインが正常にロードされているか、コマンドや権限が有効になっているかを確認できます。

### ユニットテストの推奨
Rust の標準テストを活用して、`transform` 関数や `parse_my_outline` 関数をホスト上でそのままテストできます：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse() {
        assert_eq!(transform("reverse_text", "hello"), "olleh");
    }
}
```

```bash
cargo test
```

---

## 9. Component Model (WIT) への対応について

Zee は標準の WASM に加え、**Bytecode Alliance Wasmtime Component Model / WIT (`zee:plugin@0.2.0`)** に対応しています。

Component Model では WIT (WebAssembly Interface Types) ファイルに基づいて型安全なバインディングが自動生成され、ポインタ操作や手動メモリ管理が不要になります：

- **Fuel & Epoch 制限**: 無限ループするプラグインでも、エディタ本体がハングせず 50ms 程度で安全に強制中断されます。
- **StoreLimits**: プラグインごとのメモリ消費上限が厳格に保護されます。

詳細なアーキテクチャや WIT 定義については、[docs/wasm_plugin_system_design.md](file:///Users/hiroshi/dev/zee/docs/wasm_plugin_system_design.md) をご参照ください。
