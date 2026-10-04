# Tree-sitter 多言語コードアウトライン設計仕様書

## 1. 概要と目的

`zee` のサイドパネルにおける「Outline（アウトライン）」機能は、現在 Markdown の見出し階層（`#` 〜 `######`）に対応しています。本計画では、プログラミング言語（Python, Go, Rust）および構造化・マークアップ言語（JSON, CSS, HTML）において、関数・クラス・メソッド・構造体・セレクタ等のシンボル階層を抽出し、サイドバーにツリー表示してクリックで該当行へ即座に移動できる機能を導入します。

---

## 2. パーサー選定方針：Tree-sitter の採用

独自パーサーの実装や正規表現による抽出ではなく、業界標準の **`tree-sitter`** を採用します。

### 採用理由
1. **高い耐障害性 (Error-Tolerant)**:
   - ユーザーがタイピング中の書きかけのコードや構文エラーが存在するコードでも、パニックせず構文木（CST）を壊さずに維持・解析可能。
2. **増分構文解析 (Incremental Parsing)**:
   - 前回の構文木と編集差分のみを入力することで、数千行〜数万行のファイルでも数ミリ秒〜マイクロ秒単位で再解析が完了し、エディタの入力応答性を阻害しない。
3. **統一された AST / クエリ API**:
   - 言語ごとに異なるパーサー crate を個別に扱うのではなく、Tree-sitter Query（S式記法）によって宣言的にシンボルを抽出できる。
4. **外部依存・サーバー不要**:
   - 言語サーバー（LSP）のような外部プロセスのインストールが不要で、オフラインかつ 0 秒起動で完結する。

---

## 3. 採用予定 Crate

| 対象言語 | Crate 名 | 抽出対象シンボル例 |
| :--- | :--- | :--- |
| **コア** | `tree-sitter` | パーサーエンジン、Query マッチャー、構文木管理 |
| **Rust** | `tree-sitter-rust` | `fn`, `struct`, `enum`, `trait`, `impl`, `const`, `type` |
| **Python** | `tree-sitter-python` | `def` (関数・メソッド), `class`, `async def` |
| **Go** | `tree-sitter-go` | `func` (関数・メソッド), `type ... struct`, `type ... interface` |
| **JSON** | `tree-sitter-json` | トップレベルオブジェクトキー、ネスト配列・オブジェクト |
| **CSS** | `tree-sitter-css` | クラス/IDセレクタ (`.class`, `#id`), `@media`, `@keyframes` |
| **HTML** | `tree-sitter-html` | `header`, `nav`, `main`, `section`, `article`, `div#id`, `h1`-`h6` |

---

## 4. データ構造と UI マッピング

### コアデータ構造 (`zee-core::outline`)
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Property,
    Heading(u8),
    Custom(&'static str),
}

#[derive(Debug, Clone)]
pub struct OutlineNode {
    pub title: String,
    pub kind: SymbolKind,
    pub level: usize,
    pub line: usize,
    pub col: usize,
    pub is_expanded: bool,
    pub children: Vec<OutlineNode>,
}
```

### UI 表示バッジ
- **Rust**: `fn`, `str`, `enum`, `trt`, `impl`
- **Python**: `def`, `cls`
- **Go**: `fn`, `typ`
- **JSON**: `{}`
- **CSS**: `css`
- **HTML**: `<>`
- **Markdown**: `H1` 〜 `H6`

---

## 5. パフォーマンスとスレッドモデル

1. **バックグラウンド解析**:
   - ファイル保存時、またはタイピング休止時（デバウンス: 250ms〜500ms）にバックグラウンドスレッドで構文解析を実行。
   - UI スレッドを一切ブロックせず、解析完了時にメインスレッドへ `OutlineUpdated` メッセージを送信。
2. **増分構文解析**:
   - `tree_sitter::Parser::parse_with(&mut self, ..., old_tree)` により、編集箇所のみを差分解析。

---

## 6. WASM プラグインおよび LSP との連携

1. **WASM プラグイン (`zee:plugin/outline`)**:
   - プラグインが独自言語や DSL のアウトラインを提供する場合、組み込み Tree-sitter よりプラグインの `get-outline` を優先可能。
2. **LSP (Language Server Protocol) との将来統合**:
   - ユーザーが LSP を有効化している場合、`textDocument/documentSymbol` の結果で Tree-sitter のアウトラインを高精度に補正可能（型推論に基づく階層化など）。
