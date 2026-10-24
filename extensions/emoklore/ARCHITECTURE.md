# emoklore

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/emoklore`  
> テスト: `extensions/emoklore/tests/emoklore_test.rs`

## 責務

エモクロア TRPG の技能判定コマンドを解析し、`Generast` の判定木（`alhalte/generast/judgement` ラベルを持つ `TinContent` 要素）へ変換する。`{X}DM<=Y` / `XDM<=Y` / `DM<=Y` 形式の複数 D10 判定および `〈技能〉` などの技能名表記を解釈し、成功度の合計から判定段階を算出する。

## 公開 API / 外部接続点

- `parse_emoklore_string(input, speaker_id) -> Result<Generast, EmokloreError>`
- `judge_emoklore(rolls, target) -> (degree, level_name)`
- `parse_emoklore_prefix`、`parse_suffix`、`parse_angle_bracket_subject`
- `try_combine_diceroll_and_suffix(roll_ast, suffix) -> Option<(Generast, String)>`
- `derive_seed`、`process_ast`、`self_introduction_info`
- 共通 WASM ABI（詳細は [../ARCHITECTURE.md](../ARCHITECTURE.md) を参照）

## 文法と判定規則

```text
({X} | X)? DM [<= target] [〈skill〉 | 《skill》 | <skill> | 【skill】 | [skill]]
```

入力例: `{2}DM<=5〈調査〉`、`2DM<=6`、`DM<=4〈観察眼〉`

各 10 面ダイス（D10）の出目に対する成功度配点:

| ダイスの出目 | 成功度変化 |
| --- | --- |
| `1` | `+2`（クリティカル） |
| `2..=target` | `+1`（成功） |
| `10` | `-1`（ファンブル） |
| 上記以外（`target < roll <= 9`） | `0`（失敗） |

全ダイスの成功度合計から決定される判定段階（`judge_emoklore`）:

| 合計成功度（`degree`） | `level_name` |
| --- | --- |
| `<= -1` | `ファンブル` |
| `0` | `失敗` |
| `1` | `シングル` |
| `2` | `ダブル` |
| `3` | `トリプル` |
| `4..=9` | `ミラクル` |
| `>= 10` | `カタストロフ` |

## 内部フロー

1. 接頭辞 `{X}DM` / `XDM` / `DM` を解析し、振るダイスの個数 `X`（省略時は `1`）を取得する。
2. 目標値 `<=target` と技能名表記を解析する。目標値が指定されていない場合は判定木への変換を行わない。
3. `derive_seed(speaker_id, text)` で決定論的シードを生成し、`SeededEntropy` を用いて `X` 個の D10 を振る。
4. 出目の配列と目標値を `judge_emoklore` に渡し、合計成功度 `degree` と判定名 `level_name` を得る。
5. `diceroll_tin(seed, RollN(D10, X))` と `judgement_tin`（いずれも `TinContent`）を組み立て、`Generast::Element` として返す（#272）。
6. 判定式の後ろに余剰テキストが続く場合は `Nested` ノードで連結して返す。

`process_ast` は単独の `String` ノードの解析に加え、`Nested` ノード内で前段の `alhalte-dsl` が生成した `Element(diceroll)` と直後の `String` を合成して判定木を作る経路（`try_combine_diceroll_and_suffix`）にも対応する。

## 既知の差分・制約

- 技能名の括弧表記は `〈〉`、`《》`、`<>`、`【】`、`[]` の 5 種類すべてを受理する。
- ダイス個数の指定は波括弧付きの `{X}DM` と数値直接指定の `XDM` の双方に対応し、数値省略時（`DM`）は `1DM` として扱う。
- 各ダイスの成功度は内部で `i32` として合算した後に `JudgementData.degree` の型である `i8` へキャストする。
