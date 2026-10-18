# coc-6

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/coc-6`  
> テスト: `extensions/coc-6/tests/coc6_test.rs`

## 責務

Call of Cthulhu 6 版の判定を `Generast` の判定木に変換する。`CC` / `1D100` などの接頭辞、`<=target`、`【技能】` / `[技能]` を解析し、成功段階を付ける。

## 公開 API / 外部接続点

- `parse_coc6_string(input, speaker_id) -> Result<Generast, Coc6Error>`
- `judge_coc6(roll, target) -> (degree, level_name)`
- `parse_bracket_subject`、`parse_suffix`
- `try_combine_diceroll_and_suffix(roll_ast, suffix) -> Option<(Generast, String)>`
- `derive_seed(speaker_id, text)`
- `process_ast(ast, speaker_id)`
- `self_introduction_info()`
- 共通 ABI（[../ARCHITECTURE.md](../ARCHITECTURE.md)）

## 文法

```text
(CC | 1D100 | D100) [<= target] [【skill】 | [skill]]
```

例: `1D100<=70【目星】`、`CC<=50`、`CC【聞き耳】`。

判定:

| 条件 | degree | level_name |
| --- | --- | --- |
| `roll <= 5` かつ成功 | 2 | 決定的成功 |
| `roll <= floor(target / 5)` | 2 | スペシャル |
| `roll <= target` | 1 | 成功 |
| `96 <= roll <= 100` かつ失敗 | -2 | 致命的失敗 |
| それ以外の失敗 | -1 | 失敗 |

## 内部フロー

1. 入力を trim し、接頭辞を解析する。
2. 接頭辞があり、`<=target` が取れた場合だけ判定を作る。
3. `derive_seed(speaker_id, text)` で seed を作り、`1D100` の `DiceExpr` を `SeededEntropy` で評価する。
4. `judge_coc6` で degree / level_name を決める。
5. `diceroll_tin(seed, full_expr)` を作り、`JudgementData` の `roll` に入れる。
6. `judgement_tin` を `Element` として返す。後続文字列があれば `Nested` にまとめる。
7. 接頭辞がない場合は元の文字列を返す。

`process_ast` は `String` を直接解析するほか、`Nested` 内で `Element(diceroll)` の直後に `String` が続く場合、`try_combine_diceroll_and_suffix` で judgement に合成する。これは alhalte-dsl が先にダイスを `diceroll` にしたケースを拾うため。

## 既知の差分・制約

- target は `<=` / `<` / `==` / `=` を受け付けるが、判定自体は「出目 <= target」で行う。
- 技能名は `【】` と `[]` の両方を受け付ける。
- ダイスは送信時に一度評価して degree を確定し、同時に `diceroll` Tin を残す。表示時に再度評価しても同じ seed なので結果は一致する。
- 1D100 以外のダイス式と組み合わせる場合は、alhalte-dsl の `diceroll` と後続文字列の合成経路を使う。
