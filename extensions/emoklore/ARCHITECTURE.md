# emoklore

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/emoklore`  
> テスト: `extensions/emoklore/tests/emoklore_test.rs`

## 責務

エモクロア TRPG の判定を `Generast` の判定木に変換する。`{X}DM<=Y` / `XDM<=Y` / `DM<=Y` の形式と、`〈技能〉` などの技能表記を扱う。

## 公開 API / 外部接続点

- `parse_emoklore_string(input, speaker_id) -> Result<Generast, EmokloreError>`
- `judge_emoklore(rolls, target) -> (degree, level_name)`
- `parse_emoklore_prefix`、`parse_suffix`、`parse_angle_bracket_subject`
- `try_combine_diceroll_and_suffix`
- `derive_seed`、`process_ast`、`self_introduction_info`
- 共通 ABI（[../ARCHITECTURE.md](../ARCHITECTURE.md)）

## 文法

```text
({X} | X)? DM [<= target] [〈skill〉 | 《skill》 | <skill> | 【skill】 | [skill]]
```

例: `{2}DM<=5〈調査〉`、`2DM<=6`、`DM<=4`。

判定は各ダイスの成功度の合計:

| 出目 | 成功度 |
| --- | --- |
| 1 | +2（クリティカル） |
| 2..=target | +1（成功） |
| 10 | -1（ファンブル） |
| それ以外 | 0 |

合計から level_name を決める。

| 成功度 | level_name |
| --- | --- |
| <= -1 | ファンブル |
| 0 | 失敗 |
| 1 | シングル |
| 2 | ダブル |
| 3 | トリプル |
| 4..=9 | ミラクル |
| >= 10 | カタストロフ |

## 内部フロー

1. `{X}DM` / `XDM` / `DM` を解析し、ダイス個数 `X` を得る（省略時 1）。
2. `<=target` と技能表記を解析する。target がない場合は判定を作らない。
3. `derive_seed` で seed を作り、`X` 個の D10 を `SeededEntropy` で振る。
4. 出目列を `judge_emoklore` に渡し、degree / level_name を得る。
5. `diceroll_tin(seed, RollN(D10, X))` と `judgement_tin` を組み立てて返す。
6. 後続文字列があれば `Nested` にまとめる。

`process_ast` は `String` を直接解析するほか、`Nested` 内の `Element(diceroll)` + `String` の合成にも対応する。

## 既知の差分・制約

- 技能表記は `〈〉`、`《》`、`<>`、`【】`、`[]` の 5 種類を受け付ける。
- ダイス個数は `{X}` でも `X` でもよい。`DM` の前に数字がない場合は 1 個。
- 成功度は `i32` で集計してから `i8` にキャストする。極端に多いダイス個数では飽和しない点に注意。
