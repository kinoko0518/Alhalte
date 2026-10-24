# markdown

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/markdown`  
> テスト: `extensions/markdown/tests/markdown_test.rs`

## 責務

入力メッセージを Markdown 風の装飾記法として解析し、`Generast` の装飾ノードへ変換する。行頭の見出し記法およびインライン装飾記法を `winnow` でパースする。

## 公開 API / 外部接続点

- `parse_markdown(input: &str) -> Result<Generast, MarkdownError>`
- `parse_inlines(input: &str) -> Vec<Generast>`
- `process_ast(ast, _speaker_id) -> Generast`
- `self_introduction_info() -> (id, name, icon_svg, description)`（ABI v2）
- 共通 WASM ABI（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## サポートする文法

各装飾は対応するラベルの `TinContent` を `meta` に持つ `Generast::Nested` ノードへ変換される（#272）。

| 記法 | 出力される `Generast` ノード |
| --- | --- |
| 行頭の `# ` / `## ` / `### ` / `#### ` | `h1` / `h2` / `h3` / `h4` |
| `**text**` | `bold`（太字） |
| `*text*` | `italic`（斜体） |
| `\|\|text\|\|` | `hidden`（スポイラー） |
| `[text]{color:#rrggbb}` | `text-color`（文字色） |
| `[text]{pill:#rrggbb}` | `pill`（背景色付きピル） |

見出し記法は行頭にある場合のみ行単位で解釈し、見出し内部のテキストおよび見出し以外の通常行に対してはインライン解析（`parse_inlines`）を適用する。複数行や複数要素に分かれる場合は、`need not know`（ラベル `0`）の `Nested` ノードで束ねる。

## 内部フロー

1. 入力文字列を改行 `\n` ごとに行分割する。
2. 各行を `parse_line` に通す。行頭が `# `〜`#### ` に一致すれば対応する `Generast::h1`〜`h4` を構築し、それ以外の行は `parse_inlines` で解析する。
3. `parse_inlines` は文字色・ピル（`[...]{...}`）、スポイラー（`||...||`）、太字（`**...**`）、斜体（`*...*`）の優先順でマッチングを試み、いずれにも一致しない箇所は 1 文字ずつプレーンテキストバッファに蓄積して `Generast::String` にまとめる。
4. 複数行が存在する場合は、行と行の間に `Generast::String("\n")` を挿入して改行を保持する。
5. 生成されたノードが 1 つだけならそのノードを直接返し、複数ある場合は `need not know` ラベルの `Nested` にまとめて返す。

`process_ast` は `Generast::String` に `parse_markdown` を適用し、`Generast::Nested` の子要素を再帰的に処理する。

## 既知の差分・制約

- `self_introduction` の説明文には打ち消し線 `~~` への言及が含まれるが、現在のパーサには `~~` は実装されていない。
- 見出し記法は行頭でのみ有効であり、文中の `#` は通常の文字として扱われる。
- `text-color` および `pill` の色指定文字列はパーサ内ではそのまま `TinContent`（`String::into()`）として保持し、CSS 値としての安全性検証（サニタイズ）は描画時の `alhalte-saignant`（`sanitize_css_color`）に委ねる。
- 複数要素を束ねるルート `Nested` の `meta` は `need not know`（ラベル `0`）であり、視覚的な装飾を持たないグループ化ノードとして機能する。
