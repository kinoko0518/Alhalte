# aozora

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/aozora`  
> テスト: `extensions/aozora/tests/aozora_test.rs`

## 責務

青空文庫書式のメッセージを `Generast` に変換する。`aozora-rs-core` の tokenizer / scopenizer / retokenizer を使い、ルビ、傍点、見出し、字下げ、地付き、縦中横、太字、斜体を扱う。

## 公開 API / 外部接続点

- `parse_aozora(text: &str) -> Result<Generast, AozoraError>`
- `process_ast(ast, _speaker_id) -> Generast`
- `self_introduction_info()`
- 共通 ABI（[../ARCHITECTURE.md](../ARCHITECTURE.md)）

## 変換表

| 青空文庫の要素 | 出力 |
| --- | --- |
| 太字 | `bold` |
| 斜体 | `italic` |
| ルビ | `ruby` |
| 傍点 | `boten` |
| A/B/C 見出し | `h1` / `h2` / `h3` |
| 字下げ | `indent(n)` |
| ぶら下げ | `hanging(first, rest)` |
| 地付き | `grounded` |
| カーニング | `kerning(n)` |
| 縦中横 | `h-in-v` |
| 改行 | `String("\n")` |
| 挿絵 | `String("［＃挿絵（path）入る］")` |

## 内部フロー

1. 空文字なら空の `String` を返す。
2. `Input::new(text)` → `tokenize` → `scopenize` → `retokenize` の順で解析する。
3. 解析に失敗した場合、または page が空の場合は、元の文字列をそのまま `String` として返す。
4. `Retokenized::DecoBegin` / `DecoEnd` をスタックで対応させ、`convert_deco_to_generast` で装飾ノードを作る。
5. 閉じられていないフレームは末尾で閉じる。
6. ルートの子が 1 個ならそのまま、複数なら `Nested`、空なら空 `String` を返す。

`process_ast` は `String` を `parse_aozora` に通し、`Nested` の子を再帰処理する。

## 既知の差分・制約

- `aozora-rs-core` は git 依存（`github.com/kinoko0518/aozora-rs`）。外部ネットワークに依存するテストは書かない。
- 未知の `Deco` は `need not know` の `Nested` にフォールバックする。情報を落とさず、装飾だけを失う。
- 挿絵は画像として埋め込まず、注記文字列として残す。
