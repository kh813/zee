# Zee 次世代 WASM プラグインシステム 設計書 & 進捗記録

本ドキュメントは、Zee エディタにおける WebAssembly プラグインシステムを **Wasmtime Component Model / WIT (WebAssembly Interface Types)** ベースへ刷新するための詳細設計書および進捗記録です。

---

## 1. 背景と刷新の目的

従来のスタック型・手動メモリ管理（ポインタ渡し + JSONシリアライズ）による WASM プラグイン方式には以下の課題がありました：
1. **メモリ安全性と型の制約**: 生ポインタ（`ptr`, `len`）とパック値（`i64`）による独自規約に依存しており、境界をまたぐ型安全性が弱い。
2. **粗粒度トランザクションの欠如**: 1文字・1行ごとの細かい呼出は低速であり、Undo/Redo 履歴との整合性が崩れやすい。
3. **暴走・無限ループへの脆弱性**: プラグインが無限ループや過大なメモリ割り当てを起こした場合にホスト側（UI）がハングするリスク。
4. **スレッド分離の不足**: UI スレッドで直接 WASM を実行すると描画フレームレートが低下する。

これらを解決するため、Bytecode Alliance 標準の **Component Model** と **WIT** を採用し、安全・高速・高拡張性なプラグインシステムを構築します。

---

## 2. システムアーキテクチャ

```
+-------------------------------------------------------------------------+
|                               Zee Host                                  |
|                                                                         |
|  +-------------------+       Message Passing      +------------------+  |
|  |  UI / GPUI Thread | <========================> |  Plugin Worker   |  |
|  |  (Non-blocking)   |   (Request / Transaction)  |  (Background OS) |  |
|  +-------------------+                            +------------------+  |
|                                                             |           |
|                                         +-------------------+-------+   |
|                                         | Wasmtime Component Engine |   |
|                                         | - Fuel limit (命令数)     |   |
|                                         | - Epoch interruption (時間)|   |
|                                         | - StoreLimits (メモリ上限) |   |
|                                         +---------------------------+   |
|                                                       |                 |
|                                          Host Imports (WIT Bindgen)     |
|                                                       |                 |
+-------------------------------------------------------|-----------------+
                                                        |
                                            Component Model Boundary
                                                        |
+-------------------------------------------------------v-----------------+
|                       Guest WASM Plugin (Component)                     |
|                                                                         |
|  Imports:  zee:plugin/buffer (text-range, apply-edits, line-count, ...) |
|  Exports:  on-command, get-outline, on-buffer-changed                   |
|  State:    Plugin-internal state (sandboxed)                            |
+-------------------------------------------------------------------------+
```

---

## 3. WIT 仕様策定 (`zee:plugin@0.2.0`)

### 3.1 バッファ編集インターフェース (`buffer`)
- **座標規約**: 文字列位置はすべて **UTF-8 バイトオフセット** (`u32`) を標準とし、明文化。
- **トランザクション編集 (`apply-edits`)**: 単一の編集リスト `list<edit>` を一度に適用。

```wit
package zee:plugin@0.2.0;

interface buffer {
  record edit {
    start-byte: u32,
    end-byte: u32,
    new-text: string,
  }

  record buffer-info {
    line-count: u32,
    byte-length: u32,
    path: option<string>,
  }

  /// 現在のアクティブバッファ情報を取得
  get-info: func() -> buffer-info;

  /// 指定したバイト範囲の文字列を取得 (コピーを最小化)
  get-text-range: func(start-byte: u32, end-byte: u32) -> result<string, string>;

  /// バッファ全体を取得 (小規模ファイルまたは全体の解析用)
  get-all-text: func() -> string;

  /// バッファに対する一括編集トランザクションを適用
  apply-edits: func(edits: list<edit>) -> result<_, string>;
}

interface outline {
  record outline-node {
    title: string,
    level: u32,
    line: u32,
  }
}

world plugin {
  import buffer;

  /// コマンド実行フック (例: "format_json", "sort_lines", "to_uppercase")
  export on-command: func(name: string) -> result<string, string>;

  /// アウトライン構造解析フック
  export get-outline: func() -> result<list<outline::outline-node>, string>;

  /// 初期化 / ライフサイクルフック
  export on-init: func() -> result<_, string>;
}
```

---

## 4. ホスト側ランタイム設計

1. **Engine の集約**:
   - `wasmtime::Engine` はアプリ全体で単一の `Arc<Engine>` を共有。
   - `Config::new()` で以下を設定：
     - `wasm_component_model(true)`
     - `consume_fuel(true)`: 命令数ベースのカウント
     - `epoch_interruption(true)`: バックグラウンドの tick スレッドによるタイムアウト制御
2. **Store の分離とリソース制限**:
   - 各プラグイン実行時、独立した `Store<T>` を生成。
   - `StoreLimits` によりメモリ割り当てサイズ上限（デフォルト 64MB）を設定。
   - 実行前に `store.set_fuel(10_000_000)` などを注入し、枯渇時は即座にトラップして中断。
3. **バックグラウンド実行基盤**:
   - プラグイン呼び出しを非同期スレッド（`smol::unblock` またはワーカースレッド）で実行。
   - UI スレッドの応答性を完全に維持。
4. **クラッシュ・トラップ隔離**:
   - プラグイン内でパニックまたは fuel/epoch 切れが発生した場合、`anyhow::Error` でトラップを捕捉し、エディタ本体は正常動作を維持。該当プラグインの状態をリセットまたは警告通知。

---

## 5. 実装進捗トラッカー (Progress Tracker)

| ステップ | タスク内容 | 担当領域 | 状態 | 備考 |
|---|---|---|---|---|
| **Step 1** | **WIT 仕様の策定と配置** | `crates/zee-core/wit/` | `[COMPLETED]` | `zee:plugin@0.2.0` パッケージ定義、buffer/outline/plugin world |
| **Step 2.1** | **Wasmtime 依存追加 & Component Model 設定** | `crates/zee-core/Cargo.toml` | `[COMPLETED]` | `wasmtime` (component-model) 統合 |
| **Step 2.2** | **ホスト側 WIT Bindgen & バインディング実装** | `crates/zee-core/src/component_plugin.rs` | `[COMPLETED]` | Host trait実装, Fuel, Epoch, StoreLimits |
| **Step 2.3** | **非同期/バックグラウンド実行ランナー** | `crates/zee-core/src/component_plugin.rs` | `[COMPLETED]` | ワーカースレッド & タイムアウト管理 |
| **Step 3.1** | **テスト用 Component プラグイン作成** | `crates/zee-core/tests/` | `[COMPLETED]` | WIT 準拠のテストバイナリ / モック実装 |
| **Step 3.2** | **単体テスト & 暴走対策テスト** | `crates/zee-core/src/component_plugin.rs` | `[COMPLETED]` | Fuel枯渇検知、Epochタイムアウト、メモリ制限、編集適用テスト |
| **Step 3.3** | **既存 PluginManager への統合** | `crates/zee-core/src/plugin.rs` | `[COMPLETED]` | Component Model とクラシック WASM の透過的サポート |

---

## 6. 検証記録 (Verification Log)

- **2026-10-04 (Step 1 - 3 実装 & テスト完了)**:
  - **Step 1 (WIT仕様策定)**: `crates/zee-core/wit/plugin.wit` にて `zee:plugin@0.2.0`（`buffer`, `outline`, `plugin` world）を策定。
  - **Step 2 (ホストランタイム基盤)**:
    - Wasmtime Component Model, `consume_fuel(true)`, `epoch_interruption(true)` (50ms 周期バックグラウンドティッカー), `StoreLimits` (メモリ上限) を実装。
    - `execute_command_async` による非同期バックグラウンド実行（UI スレッドブロッキング回避）を実装。
    - `buffer` インターフェース（`get-info`, `get-text-range`, `get-all-text`, `apply-edits`）の実装と UTF-8 境界厳格バリデーション。
  - **Step 3 (検証 & テスト実施)**:
    - `test_store_limits_memory_allocation`: 64KB メモリ上限超過時のメモリ拡張拒否を検証（PASS）。
    - `test_host_state_buffer_operations`: UTF-8 マルチバイトスライス、範囲外境界、編集トランザクションを検証（PASS）。
    - `test_component_engine_initialization`: エンジン設定と Epoch 割り込み有効化を検証（PASS）。
    - `test_wat_component_compilation`: Component Model バイナリのコンパイルを検証（PASS）。
    - `test_async_command_execution`: 別スレッドでの非同期コマンド実行とメッセージパッシング応答を検証（PASS）。
    - `test_component_command_execution`: WIT `on-command` 呼び出しと結果文字列の返却を検証（PASS）。
    - `test_component_plugin_init_and_fuel_interruption`: 無限ループプラグインが Fuel 枯渇（命令数制限）により安全に中断されることを検証（PASS）。
    - `test_component_plugin_epoch_timeout`: 無限ループプラグインが Epoch タイムアウト（時間制限）により 50ms 程度で即座に強制中断されることを検証（PASS）。
  - **ワークスペース全テスト**: 85 件すべてのユニットテスト・結合テストが成功（`cargo test --workspace` PASS, `cargo clippy --all-targets` 0 warnings）。
