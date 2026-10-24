# coc-7

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/coc-7`  
> テスト: `extensions/coc-7/tests/coc7_test.rs`

## 責務

新クトゥルフ神話 TRPG（第 7 版 / Call of Cthulhu 7th Edition）の技能判定コマンドを解析し、`Generast` の判定木（`alhalte/generast/judgement` ラベルを持つ `TinContent` 要素）へ変換する。ボーナスダイス・ペナルティダイス（±1〜2 個）および第 7 版の成功段階（クリティカル / イクストリーム成功 / ハード成功 / レギュラー成功 / 失敗 / ファンブル）を処理する。

## 公開 API / 外部接続点

- `parse_coc7_string(input, speaker_id) -> Result<Generast, Coc7Error>`
- `judge_coc7(roll, target) -> (degree, level_name)`
- `roll_coc7_dice(bonus, entropy) -> Result<u64, Coc7Error>`
- `parse_bracket_subject`、`parse_suffix`、`parse_prefix`
- `try_combine_diceroll_and_suffix(roll_ast, suffix) -> Option<(Generast, String)>`
- `derive_seed`、`process_ast`、`self_introduction_info`
- 共通 WASM ABI（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## 文法と判定規則

サポートする接頭辞一覧:

| 接頭辞 | 意味 |
| --- | --- |
| `CCB2` / `CC+2` | ボーナスダイス 2 個 |
| `CCB1` / `CCB` / `CC+1` | ボーナスダイス 1 個 |
| `CCP2` / `CC-2` | ペナルティダイス 2 個 |
| `CCP1` / `CCP` / `CC-1` | ペナルティダイス 1 個 |
| `1D100` / `D100` / `CC` | 通常ロール（加減なし） |

```text
prefix [<= target] [【skill】 | [skill]]
```

入力例: `CCB<=70【目星】`、`CCP2<=50`、`CC<=60【図書館】`

成功段階（`judge_coc7`）の判定表:

| 条件 | `degree` | `level_name` |
| --- | --- | --- |
| `roll == 1` | `4` | `クリティカル` |
| `roll == 100`、または `target < 50` かつ `96 <= roll <= 100` | `-2` | `ファンブル` |
| `roll <= floor(target / 5)` | `3` | `イクストリーム成功` |
| `roll <= floor(target / 2)` | `2` | `ハード成功` |
| `roll <= target` | `1` | `レギュラー成功` |
| 上記以外（`roll > target`） | `-1` | `失敗` |

## 内部フロー

1. 接頭辞を解析し、ボーナス・ペナルティダイス数 `bonus`（`+2`, `+1`, `0`, `-1`, `-2`）を取得する。
2. 目標値 `<=target` が指定されている場合にのみ判定処理を行う。
3. `derive_seed(speaker_id, text)` で決定論的シードを生成する。
4. `bonus == 0` の場合は通常の `1D100` を 1 回評価する。
5. `bonus != 0` の場合は `roll_coc7_dice` により、まず 1 の位（10 面ダイス）を 1 回振り、続けて 10 の位を `1 + |bonus|` 個振る。ボーナス時は最小値、ペナルティ時は最大値の 10 の位を採用して合成する（10 の位・1 の位ともに `0` の場合は `100` として扱う）。
6. 確定した出目と目標値を `judge_coc7` に渡し、`degree` と `level_name` を算出する。
7. `diceroll_tin` と `judgement_tin`（いずれも `TinContent`）を組み立て、`Generast::Element` として返す（#272）。

`process_ast` は `coc-6` と同様、単独の `String` 解析に加え、`Nested` 内で前段の `alhalte-dsl` が生成した `Element(diceroll)` と後続 `String` を合成する経路（`try_combine_diceroll_and_suffix`）も備える。

## 既知の差分・制約

- ボーナス・ペナルティダイス付きのロールでは、`roll_coc7_dice` が `Entropy::take` を直接呼び出して 1 の位と 10 の位を順番に振るため、汎用 `DiceExpr::RollN` とは乱数ストリームの消費順が異なる。ただし同一の `seed` と同一の `bonus` 指定からは常に同じ出目が決定論的に再現される。
- 前段の `diceroll` 要素と後続文字列を合成する経路では、元の `diceroll` の式が比較式（`Le` など）を含み目標値を抽出できる場合にのみ判定木への昇格を行う。
