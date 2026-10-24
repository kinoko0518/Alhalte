# aozora

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/aozora`  
> テスト: `extensions/aozora/tests/aozora_test.rs`

## 責務

青空文庫記法で記述されたテキストを解析し、`Generast` の組版・装飾ノードへ変換する。`aozora-rs-core` の tokenizer / scopenizer / retokenizer を利用し、ルビ、傍点、見出し、字下げ、ぶら下げ、地付き、縦中横、太字、斜体などを表現する。

## 公開 API / 外部接続点

- `parse_aozora(text: &str) -> Result<Generast, AozoraError>`
- `process_ast(ast, _speaker_id) -> Generast`
- `self_introduction_info() -> (id, name, icon_svg, description)`（ABI v2）
- 共通 WASM ABI（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## 記法の変換対応表

各装飾は対応するラベルとパラメータを持つ `TinContent` を `meta` に据えた `Generast::Nested` ノードへと変換される（#272）。

| 青空文庫の要素 | 出力される `Generast` ノード |
| --- | --- |
| 太字 | `bold` |
| 斜体 | `italic` |
| ルビ | `ruby` |
| 傍点 | `boten` |
| 大見出し / 中見出し / 小見出し（A/B/C 見出し） | `h1` / `h2` / `h3` |
| 字下げ | `indent(n)` |
| ぶら下げ | `hanging(first, rest)` |
| 地付き | `grounded` |
| カーニング | `kerning(n)` |
| 縦中横 | `h-in-v` |
| 改行 | `String("\n")` |
| 挿絵注記 | `String("［＃挿絵（path）入る］")` |

## 内部フロー

1. 入力が空文字列の場合は、空の `Generast::String` を返す。
2. `Input::new(text)` を作成し、`tokenize` → `scopenize` → `retokenize` のパイプラインで青空文庫トークン列へ変換する。
3. 解析に失敗した場合、または生成されたページが空の場合は、入力文字列をそのまま `Generast::String` として返す（フェイルセーフ）。
4. `Retokenized::DecoBegin` と `DecoEnd` をスタックで対応付け、`convert_deco_to_generast` で対応する装飾 `Nested` ノードを構築する。
5. 入力末尾で閉じられていない装飾フレームが残っている場合は、末尾で自動的に閉じて木構造を完結させる。
6. ルート要素が 1 つだけならそのノードを直接返し、複数ある場合は `need not know`（ラベル `0`）の `Nested` にまとめ、空なら空の `String` を返す。

`process_ast` は `Generast::String` を `parse_aozora` に通し、`Generast::Nested` の子要素を再帰的に処理する。

## 既知の差分・制約

- `aozora-rs-core` は Git リポジトリ依存（`github.com/kinoko0518/aozora-rs`）として導入されている。テスト実行時には外部ネットワーク通信を行わない。
- `Generast` 側に対応するスタイルラベルが存在しない未知の `Deco` バリアントに遭遇した場合は、テキスト情報を失わないよう `need not know`（ラベル `0`）の `Nested` ノードにフォールバックし、中身の文字列をそのまま保持する。
- 挿絵記法は外部画像要素としては展開せず、`［＃挿絵（...）入る］` の注記文字列として残す。
