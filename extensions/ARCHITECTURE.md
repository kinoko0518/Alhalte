# 拡張機能

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md)  
> 実装: `extensions/*`  
> テスト: `extensions/*/tests/*.rs`

## 責務

チャットメッセージの送信時およびプレビュー時に、汎用構文木 `Generast` を変換する WASM モジュール群。有効化された拡張機能を優先度（`priority`）の昇順に呼び出し、直前の拡張機能の出力を次の入力とする左畳み込み（パイプライン処理）によって最終的な構文木を構築する。公式拡張機能の構文解析はすべて `winnow`（青空文庫記法のみ `aozora-rs-core` を併用）で実装されている。

## 公開 ABI / 外部接続点

各拡張機能は `wasm32-unknown-unknown` をターゲットとし、WASI や JavaScript ホスト関数には一切依存しない純粋な関数モジュールとして動作する。エクスポートされるシンボルは以下のとおり。

| エクスポート | シグネチャ | 内容 |
| --- | --- | --- |
| `alhalte_abi_version` | `() -> i32` | ABI バージョンを返す。現行バージョンは `2`（ホスト側は `1` と `2` の双方を受理する）。 |
| `alloc` | `(len: i32) -> i32` | ホストが入力データを書き込むための線形メモリ領域を確保する。 |
| `dealloc` | `(ptr: i32, len: i32)` | `alloc` または返り値用に確保されたメモリ領域を解放する。 |
| `process_generast` | `(ptr: i32, len: i32) -> i64` | 入力構文木を変換し、出力バッファの位置と長さを返す。 |
| `self_introduction` | `() -> i64` | 任意エクスポート。ABI v2 では `(id, name, icon_svg, description)`、ABI v1 では `(name, icon_svg, description)` を postcard 直列化したバイト列として返す。 |
| `memory` | `WebAssembly.Memory` | ホストとゲスト間で共有する線形メモリ。 |

`i64` の返り値は上位 32bit にポインタ、下位 32bit にバイト長を格納した `(ptr << 32) | (len & 0xffff_ffff)` 形式である。ホストは線形メモリから出力バイト列を読み取った後、必ず `dealloc(ptr, len)` を呼び出して領域を解放する。

## 入出力データ形式

`process_generast` の入出力は `alhalte/generast` ラベルを持つ `TinContent`（またはワイヤ形式の `Tin`）である（#272）。

```text
TinContent.label   = alhalte/generast
TinContent.payload = postcard((Generast, speaker_character_id: u64))
```

入力バイト列はワイヤ形式（Opener 付き `Tin`）と非圧縮平文形式（`TinContent::to_bytes()` すなわち `label(8B, u64 LE) || payload`）の双方を受け付ける。実装はまずワイヤ形式としての `Tin::decode` を試み、失敗した場合は `TinContent::from_bytes`（旧 `Tin::from_plaintext`）として解釈する。出力は入力と同じ形式（ワイヤ形式ならワイヤ形式、平文形式なら平文形式）で返す。

構文エラーが検出された場合は、処理全体を異常終了させるのではなく `Generast::syntax_error(message)` ノードを含む構文木を正常応答として返す。ホスト側ではこれを `alhalte-saignant` に渡し、エラー表示用 HTML として描画する。

## 内部フロー

1. ホスト（`alhalte-argenterie`）は、現在の部屋（`pantry` で同期される `extension::<id>` state および manifest）で有効化されている拡張機能を `priority` の昇順に並べる（#301 / #302）。
2. 入力テキストを `Generast::String` とし、発言者のキャラクター ID と組にした `(Generast, speaker_id)` の `TinContent`（`alhalte/generast` ラベル）を初期入力とする。
3. 各拡張機能の `process_generast` を順番に呼び出し、得られた出力をそのまま次の拡張機能の入力へ渡す。
4. すべての拡張機能を通過した最終的な構文木を `alhalte-saignant` に渡して HTML を描画、または `banquette` へ送信する。

各拡張機能内部の `process_ast` は、`Generast::String` ノードに対して自身のパーサを適用し、`Generast::Nested` ノードの子要素を再帰的に処理し、未知の `Generast::Element` ノードはそのまま通過させるのを基本パターンとする。なお、判定系の拡張機能（`coc-6` / `coc-7` / `emoklore`）は、前段の `alhalte-dsl` が生成した `Element(diceroll)` と直後の `String` ノードを走査・結合して `Element(judgement)` へ昇格させる処理も行う。

## 配布と有効化・同期

- `Restaurant.toml` の `[[extensions]]` に `id`、WASM ファイルの `path`、および初期 `enabled` を記述する。表示名・アイコン・説明文は WASM 自身の `self_introduction` を唯一の正とし、設定ファイルには `name` を記述しない（#274）。
- `alhalte-bistrot stage-extensions --output-dir <dir>`（または `alhalte-cafe` の起動時 staging）が `<id>.wasm` を配置し、`manifest.json` を生成する。`manifest.json` は `id`、`url`、`enabled`、`priority`（`Restaurant.toml` での定義順、0-based）を持つ。
- `alhalte-argenterie` は `.wasm` をロードし、ABI バージョン（`1` または `2`）と必須エクスポートを検証してから実行パイプラインに登録する。
- ABI v2 の `self_introduction` が返す `id` が manifest（または登録対象）の `id` と一致しない WASM は不正とみなして実行せず、`wasmMissing: true`（fail-closed）として扱う。
- 部屋内での拡張機能の ON/OFF 切り替えやカスタム拡張機能の追加は、ルートシーン `/` の `extension::<id>` state として記録され、`@alhalte/pantry` を通じて即座に全参加者のクライアント UI および実行パイプラインへリアクティブに同期・反映される（#301 / #302）。

## 既知の差分・制約

- 初期設計文書では関数名を `process-generast(Generast) -> Generast` と表記していたが、WASM 線形メモリ境界を越える実装上のシグネチャは `process_generast(ptr, len) -> i64` である。
- `wasm32` 環境では `getrandom` に `UNSUPPORTED` を返すカスタムスタブを登録している。したがって拡張機能内部で非決定論的な乱数生成を行うことはできず、ダイス等の乱数はすべて `speaker_id` と入力文字列から決定論的に導出した `seed` を用いる。
- 拡張機能は優先度順に直列実行されるため、適用順序が最終的な構文木に影響する（例: `alhalte-dsl` でダイス式を `diceroll` 要素に変換した後、`coc-6` 等で判定木 `judgement` に合成する）。

## 公式拡張機能一覧

| 拡張機能 | 文書 |
| --- | --- |
| alhalte-dsl | [alhalte-dsl/ARCHITECTURE.md](alhalte-dsl/ARCHITECTURE.md) |
| aozora | [aozora/ARCHITECTURE.md](aozora/ARCHITECTURE.md) |
| markdown | [markdown/ARCHITECTURE.md](markdown/ARCHITECTURE.md) |
| coc-6 | [coc-6/ARCHITECTURE.md](coc-6/ARCHITECTURE.md) |
| coc-7 | [coc-7/ARCHITECTURE.md](coc-7/ARCHITECTURE.md) |
| emoklore | [emoklore/ARCHITECTURE.md](emoklore/ARCHITECTURE.md) |
