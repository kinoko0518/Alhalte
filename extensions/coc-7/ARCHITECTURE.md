# coc-7

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/coc-7`  
> テスト: `extensions/coc-7/tests/coc7_test.rs`

## 責務

Call of Cthulhu 7 版の判定を `Generast` の判定木に変換する。ボーナス・ペナルティダイス、レギュラー / ハード / イクストリーム / クリティカル / ファンブルを扱う。

## 公開 API / 外部接続点

- `parse_coc7_string(input, speaker_id) -> Result<Generast, Coc7Error>`
- `judge_coc7(roll, target) -> (degree, level_name)`
- `roll_coc7_dice(bonus, entropy) -> Result<u64, Coc7Error>`
- `parse_bracket_subject`、`parse_suffix`、`parse_prefix`
- `try_combine_diceroll_and_suffix`
- `derive_seed`、`process_ast`、`self_introduction_info`
- 共通 ABI（[../ARCHITECTURE.md](../ARCHITECTURE.md)）

## 文法

接頭辞:

| 接頭辞 | 意味 |
| --- | --- |
| `CCB2` / `CC+2` | ボーナス 2 |
| `CCB1` / `CCB` / `CC+1` | ボーナス 1 |
| `CCP2` / `CC-2` | ペナルティ 2 |
| `CCP1` / `CCP` / `CC-1` | ペナルティ 1 |
| `1D100` / `D100` / `CC` | 通常 |

```text
prefix [<= target] [【skill】 | [skill]]
```

例: `CCB<=70【目星】`、`CCP2<=50`、`CC<=60【図書館】`。

判定:

| 条件 | degree | level_name |
| --- | --- | --- |
| `roll == 1` | 4 | クリティカル |
| `roll == 100`、または `target < 50` で `96..=100` | -2 | ファンブル |
| `roll <= floor(target / 5)` | 3 | イクストリーム成功 |
| `roll <= floor(target / 2)` | 2 | ハード成功 |
| `roll <= target` | 1 | レギュラー成功 |
| それ以外 | -1 | 失敗 |

## 内部フロー

1. 接頭辞から bonus（正） / penalty（負） / 0 を得る。
2. `<=target` が取れた場合だけ判定を作る。
3. `derive_seed` で seed を作る。
4. bonus が 0 なら 1D100 を 1 回振る。
5. bonus / penalty がある場合は、1 の位を 10 面ダイスで決め、10 の位を `1 + |bonus|` 個振る。bonus は最小値、penalty は最大値を採用する。`00` は 100 として扱う。
6. `judge_coc7` で degree / level_name を決める。
7. `diceroll_tin` と `judgement_tin` を組み立てて返す。

`process_ast` は coc-6 と同じく、既存の `diceroll` + 後続文字列の合成にも対応する。

## 既知の差分・制約

- bonus / penalty のダイスは `roll_coc7_dice` 内で `Entropy::take` を直接使う。通常の `DiceExpr` の `RollN` とは乱数の消費順が異なる。
- 合成経路では、既存 `diceroll` の式が `Le` でない場合、target を式から取れないため判定を作らない。
- 1 の位と 10 の位を別々に振るため、`SeededEntropy` の消費順が固定されている。同じ seed と同じ bonus からは同じ出目になる。
