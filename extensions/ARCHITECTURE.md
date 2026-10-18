# 拡張機能

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md)  
> 実装: `extensions/*`  
> テスト: `extensions/*/tests/*.rs`

## 責務

チャット送信時の `Generast` を変換する wasm モジュール群。有効な拡張機能を優先度順に呼び出し、前の出力を次の入力とする左畳み込みで最終的な構文木を作る。公式拡張機能はいずれも winnow で構文解析する。

## 公開 ABI / 外部接続点

拡張機能は `wasm32-unknown-unknown` を target とし、WASI や JS ホスト関数に依存しない。エクスポートは以下。

| エクスポート | 型 | 内容 |
| --- | --- | --- |
| `alhalte_abi_version` | `() -> i32` | ABI バージョン。現在 1。 |
| `alloc` | `(len: i32) -> i32` | ホスト入力用のメモリ確保。 |
| `dealloc` | `(ptr: i32, len: i32)` | 確保したメモリの解放。 |
| `process_generast` | `(ptr: i32, len: i32) -> i64` | 構文木を変換する。 |
| `self_introduction` | `() -> i64` | 任意。`(name, icon_svg, description)` を postcard で返す。 |
| `memory` | `WebAssembly.Memory` | ホストとのバッファ共有。 |

返り値の `i64` は `(ptr << 32) | (len & 0xffff_ffff)`。ホストは出力をコピーした後、`dealloc(ptr, len)` で解放する。

## 入出力

`process_generast` の入出力は Tin である。

```text
Tin.label   = alhalte/generast
Tin.payload = postcard((Generast, speaker_character_id: u64))
```

入力は wire 形式（opener 付き）でも平文 Tin（`label(8B LE) || payload`）でもよい。実装は `Tin::decode` → `Tin::from_plaintext` の順で試す。出力は入力が wire 形式なら wire 形式、平文なら平文で返す。

エラー時は `Generast::syntax_error(message)` を同じ形式で返す。ホストはこれを Saignant でエラー表示に変換する。

## 内部フロー

1. ホストが有効な拡張機能を `priority` 昇順に並べる。
2. 最初の入力を「送信テキストを `Generast::String` にしたもの + 発言者 ID」の Tin にする。
3. 各拡張機能の `process_generast` を順に呼ぶ。
4. 出力を次の入力にする。
5. 最終 Tin を Saignant に渡す。

各拡張機能の `process_ast` は、`String` をパースし、`Nested` の子を再帰処理し、`Element` はそのまま返すのが基本形。判定系（coc-6 / coc-7 / emoklore）は `Element` の diceroll と直後の `String` を組み合わせて judgement を作る。

## 配布

- `Bistrot.toml` の `[[extensions]]` に `id`、`name`、`path`、`enabled` を書く。
- `alhalte-bistrot stage-extensions --output-dir <dir>` が `<id>.wasm` をコピーし、`manifest.json` を生成する。
- manifest の `priority` は `Bistrot.toml` の定義順（0-based）。
- `alhalte-argenterie` は `.wasm` を読み、ABI バージョンと必須エクスポートを検証してからパイプラインに登録する。

## 既知の差分・制約

- 仕様上の名前は `process-generast(Generast) -> Generast` だが、実装はメモリ境界を跨ぐ `process_generast(ptr, len) -> i64`。
- `getrandom` は `wasm32` で `UNSUPPORTED` を返す stub を登録する。乱数が必要な処理は拡張機能内で完結させない。
- `self_introduction` は任意エクスポート。未実装の拡張機能ではホストが `null` を返し、名前は設定ファイルの `name` にフォールバックする。
- 公式拡張機能の自己紹介文には、実装されていない記法（例: Markdown の `~~`）が含まれる場合がある。文法の正は各拡張機能の文書とパーサ。
- 拡張機能は優先度順に直列実行する。前の拡張機能の出力が後の拡張機能の入力になるため、順序が結果を変える。

## 公式拡張機能

| 拡張機能 | 文書 |
| --- | --- |
| alhalte-dsl | [alhalte-dsl/ARCHITECTURE.md](alhalte-dsl/ARCHITECTURE.md) |
| aozora | [aozora/ARCHITECTURE.md](aozora/ARCHITECTURE.md) |
| markdown | [markdown/ARCHITECTURE.md](markdown/ARCHITECTURE.md) |
| coc-6 | [coc-6/ARCHITECTURE.md](coc-6/ARCHITECTURE.md) |
| coc-7 | [coc-7/ARCHITECTURE.md](coc-7/ARCHITECTURE.md) |
| emoklore | [emoklore/ARCHITECTURE.md](emoklore/ARCHITECTURE.md) |
