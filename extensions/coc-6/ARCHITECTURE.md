# coc-6

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/coc-6`  
> テスト: `extensions/coc-6/tests/coc6_test.rs`

## 責務

クトゥルフ神話 TRPG（第 6 版 / Call of Cthulhu 6th Edition）の技能判定コマンドを解析し、`Generast` の判定木（`alhalte/generast/judgement` ラベルを持つ `TinContent` 要素）へ変換する。`CC` や `1D100` などの接頭辞、目標値 `<=target`、および技能名 `【技能】` / `[技能]` を解釈し、出目と目標値から成功段階（決定的成功・スペシャル・成功・失敗・致命的失敗）を算出する。

## 公開 API / 外部接続点

- `parse_coc6_string(input, speaker_id) -> Result<Generast, Coc6Error>`
- `judge_coc6(roll, target) -> (degree, level_name)`
- `parse_bracket_subject`、`parse_suffix`
- `try_combine_diceroll_and_suffix(roll_ast, suffix) -> Option<(Generast, String)>`
- `derive_seed(speaker_id, text) -> [u8; 32]`
- `process_ast(ast, speaker_id) -> Generast`
- `self_introduction_info() -> (id, name, icon_svg, description)`（ABI v2）
- 共通 WASM ABI（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## 文法と判定規則

```text
(CC | 1D100 | D100) [<= target] [【skill】 | [skill]]
```

入力例: `1D100<=70【目星】`、`CC<=50`、`1D100<=65 [図書館]`

成功段階（`judge_coc6`）の判定表:

| 条件 | `degree` | `level_name` |
| --- | --- | --- |
| `roll <= 5` かつ `roll <= target`（成功） | `2` | `決定的成功` |
| `roll <= floor(target / 5)` | `2` | `スペシャル` |
| `roll <= target` | `1` | `成功` |
| `96 <= roll <= 100` かつ `roll > target`（失敗） | `-2` | `致命的失敗` |
| 上記以外の失敗（`roll > target`） | `-1` | `失敗` |

## 内部フロー

1. 入力文字列をトリムし、接頭辞（`CC` / `1D100` / `D100`）と目標値指定（`<=target`）を解析する。
2. 接頭辞と目標値の双方が揃っている場合にのみ判定処理を行う（目標値がない単独の `CC` などは変換しない）。
3. `derive_seed(speaker_id, text)` で決定論的シードを導出し、`1D100` の `DiceExpr` を `SeededEntropy` で評価して出目を得る。
4. 出目と目標値を `judge_coc6` に渡し、`degree` と `level_name` を決定する。
5. ダイスロール結果を表す `diceroll_tin(seed, full_expr)`（`TinContent`）を生成して `JudgementData.roll` に格納し、`judgement_tin` を `Generast::Element` として構築する（#272）。
6. 技能名の後ろに余剰テキストがある場合は `Nested` ノードで連結して返す。

また `process_ast` は、単独の `String` ノードを解析するだけでなく、`Nested` ノード内で `Element(diceroll)` の直後に `String`（技能名や比較式など）が並んでいるパターンを検出し、`try_combine_diceroll_and_suffix` によって 1 つの `Element(judgement)` ノードへと合成する。これにより、前段の `alhalte-dsl` が先にダイス部分を `diceroll` 要素へ変換した場合でも正しく CoC 6 版の判定木が構築される。

## 既知の差分・制約

- 比較演算子の表記としては `<=` / `<` / `==` / `=` を受理するが、CoC 6 版の判定ロジック自体は常に「出目 $\le$ 目標値」として計算する。
- 技能名の括弧は全角隅付き括弧 `【】` と半角角括弧 `[]` の両方に対応する。
- ダイスは判定段階（`degree`）を確定させるため拡張機能内で一度評価されるが、同時に同じ `seed` を持つ `diceroll` の `TinContent` を `JudgementData` 内に保持する。描画時に `alhalte-saignant` が再評価しても同じ `seed` から完全に同一の出目が再現される。
