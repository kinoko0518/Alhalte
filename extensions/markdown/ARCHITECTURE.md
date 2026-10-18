# markdown

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/markdown`  
> テスト: `extensions/markdown/tests/markdown_test.rs`

## 責務

送信されたメッセージを Markdown 風の記法として解釈し、`Generast` に変換する。winnow でインライン記法を解析する。

## 公開 API / 外部接続点

- `parse_markdown(input: &str) -> Result<Generast, MarkdownError>`
- `parse_inlines(input: &str) -> Vec<Generast>`
- `process_ast(ast, _speaker_id) -> Generast`
- `self_introduction_info()`
- 共通 ABI（[../ARCHITECTURE.md](../ARCHITECTURE.md)）

## 文法

| 記法 | 出力 |
| --- | --- |
| `# ` / `## ` / `### ` / `#### ` | `h1` / `h2` / `h3` / `h4` |
| `**text**` | `bold` |
| `*text*` | `italic` |
| `\|\|text\|\|` | `hidden`（スポイラー） |
| `[text]{color:#rrggbb}` | `text-color` |
| `[text]{pill:#rrggbb}` | `pill` |

行頭の見出しは行単位で処理する。それ以外の行はインライン解析する。複数のブロックがある場合は、`need not know` ラベルの `Nested` にまとめる。

## 内部フロー

1. 入力を `\n` で行に分ける。
2. 各行を `parse_line` に通す。見出しなら対応する `Generast::h*`、それ以外は `parse_inlines`。
3. `parse_inlines` は色 / ピル、スポイラー、太字、斜体の順に試し、失敗したら 1 文字ずつ plain text に積む。
4. 行間に `\n` の `String` を挟む。
5. ブロックが 1 個ならそのまま、複数なら `Nested` にまとめる。

`process_ast` は `String` を `parse_markdown` に通し、`Nested` の子を再帰処理する。

## 既知の差分・制約

- 自己紹介文には打ち消し線 `~~` が記載されているが、パーサは実装していない。
- 見出しは行頭のみ。インラインの `#` は見出しにならない。
- 色・ピルの値は文字列として保持し、CSS としての妥当性は Saignant の `sanitize_css_color` に委ねる。
- 複数行をまとめる `Nested` の `meta` は `need not know`（label 0）で、装飾ではない。
