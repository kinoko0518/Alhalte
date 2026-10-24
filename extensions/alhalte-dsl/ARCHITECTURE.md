# alhalte-dsl

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/alhalte-dsl`  
> テスト: `extensions/alhalte-dsl/tests/dsl_test.rs`

## 責務

Alhalte 標準のドメイン固有言語（DSL）を解析し、`Generast` の要素ノードへ変換する。BCDice 系のコマンドよりも自由度の高い汎用ダイス式、キャラクター能力値の増減・代入、キャラクターへのメンション、およびダイス構文エラーの検出を担う。パーサは `winnow` で実装されている。

## 公開 API / 外部接続点

- `parse_dsl(input: &str, speaker_id: u64) -> Result<Generast, DslError>`
- `parse_dice_command(&mut &str) -> ModalResult<DiceExpr>`
- `derive_seed(speaker_id, text) -> [u8; 32]`
- `process_ast(ast, speaker_id) -> Generast`
- `self_introduction_info() -> (id, name, icon_svg, description)`（ABI v2）
- 共通 WASM ABI: `alhalte_abi_version`、`alloc`、`dealloc`、`process_generast`、`self_introduction`（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## 文法と変換規則

`parse_dsl` は入力文字列に対して上から順にマッチングを試みる。各要素ノードは対応するラベルの `TinContent` を内包する `Generast::Element` として構築される（#272）。

| 優先順 | 記法パターン | 出力される `Generast` / ラベル |
| --- | --- | --- |
| 1 | `{ID}.{status}+{delta}` / `{ID}.{status}-{delta}` | `Element`（`alhalte/generast/fluct`） |
| 2 | `{ID}.{status}={value}` | `Element`（`alhalte/generast/assign`） |
| 3 | 単独の `@123` または `{123}` | `Element`（`alhalte/generast/character-id`） |
| 4 | ダイス式 | `Element`（`alhalte/generast/diceroll`） |
| 5 | `@123` を含む混在テキスト | `Nested`（`String` と `character-id` の列） |
| 6 | 上記以外の通常テキスト | `String`（無変換） |

ダイス式パーサ（`parse_dice_command`）は、四則演算、`NDM`、`D66`、`choice[...]`、反復 `xN ...`、キープ・ドロップ（`KH` / `KL` / `DH` / `DL`）、比較演算、および目標値指定をサポートする。入力文字列全体がダイス式として余りなく消費された場合にのみ `diceroll` ノードを生成し、途中に解釈できない余剰トークンが残った場合は構文エラー判定へ回す。

乱数シード `derive_seed` は `BLAKE3(speaker_id.to_le_bytes() || text)` で計算する。同一の発言者 ID と同一の入力テキストからは常に同じシードが導出され、ダイス結果の決定論的な再現性が保証される。

## 内部フロー

1. 入力文字列の前後の空白をトリムする。
2. 能力値の増減（`fluct`）、能力値の代入（`assign`）、および単独メンション（`character-id`）との完全一致を順に試す。
3. ダイス式らしい先頭パターンを持つ場合、`parse_dice_command` で `DiceExpr` の構築を試みる。
4. 入力全体がダイス式として消費された場合は `diceroll_tin(seed, expr)` を `Element` として返す。
5. パース途中で失敗し、かつダイスコマンドの書き損じ（構文エラー）と判定できる場合は `Generast::syntax_error` を返す。
6. 文中に `@123` 形式のメンションを含む場合は、通常文字列と `character-id` 要素に分割して `Nested` ノードにまとめる。
7. いずれにも該当しない場合は、元の文字列をそのまま `Generast::String` として返す。

`process_ast` は構文木を再帰的に走査し、`String` ノードに `parse_dsl` を適用し、`Nested` ノードの子要素を再帰処理し、既存の `Element` ノードは変更せずにそのまま返す。

## 既知の差分・制約

- ダイス式は送信時（拡張機能内）には評価せず、`seed` と `DiceExpr` を格納した `diceroll` の `TinContent` として構文木に埋め込む。実際の出目計算は描画時に `alhalte-saignant` が `SeededEntropy` を用いて行う（ただし後続の判定系拡張機能が成功度を判定するために先行評価する場合は同じ `seed` を共有する）。
- 構文エラー判定にはヒューリスティックが含まれる。通常のチャット文を誤ってエラーにしないよう、ダイスコマンドの接頭辞や構造を持ちながら完結していない文字列のみを `syntax_error` とする。
- 能力値の増減・代入ノードは対象キャラクター ID とステータス名を構文木として表現するものであり、実際のキャラクター状態への適用・表示はクライアント（cutlery）および `saignant` 側が担う。
