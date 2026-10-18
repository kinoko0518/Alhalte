# alhalte-dsl

> 上位文書: [../ARCHITECTURE.md](../ARCHITECTURE.md) / [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `extensions/alhalte-dsl`  
> テスト: `extensions/alhalte-dsl/tests/dsl_test.rs`

## 責務

Alhalte 内で動く DSL を `Generast` に変換する。BCDice 系より表現の制約が緩いダイス記法、キャラクターの能力値変動・代入、メンション、構文エラーを扱う。winnow で解析する。

## 公開 API / 外部接続点

- `parse_dsl(input: &str, speaker_id: u64) -> Result<Generast, DslError>`
- `parse_dice_command(&mut &str) -> ModalResult<DiceExpr>`
- `derive_seed(speaker_id, text) -> [u8; 32]`
- `process_ast(ast, speaker_id) -> Generast`
- `self_introduction_info() -> (name, icon_svg, description)`
- ABI: `alhalte_abi_version`、`alloc`、`dealloc`、`process_generast`、`self_introduction`

## 文法と変換

`parse_dsl` は上から順に試す。

| 優先 | 記法 | 出力 |
| --- | --- | --- |
| 1 | `{ID}.{status}+{delta}` / `{ID}.{status}-{delta}` | `alhalte/generast/fluct` |
| 2 | `{ID}.{status}={value}` | `alhalte/generast/assign` |
| 3 | `@123` / `{123}` | `alhalte/generast/character-id` |
| 4 | ダイス表現 | `alhalte/generast/diceroll` |
| 5 | `@123` を含む混在テキスト | `Nested` + `String` + `character-id` |
| 6 | その他 | `String` のまま |

ダイス表現は四則演算、`NDM`、`D66`、`choice[...]`、`xN ...`、`KH` / `KL` / `DH` / `DL`、比較、目標値を扱う。ダイス式全体が消費された場合だけ `diceroll` を作り、余りがある場合は構文エラー判定へ回す。

`derive_seed` は `BLAKE3(speaker_id.to_le_bytes() || text)`。同じ発言者・同じテキストからは同じ seed になり、ダイス結果が再現する。

## 内部フロー

1. 入力前後の空白を除く。
2. 能力値変動、代入、単独メンションを完全一致で試す。
3. ダイス表現らしい文字列なら `parse_dice_command` で `DiceExpr` を作る。
4. ダイス式が完全に消費されたら `diceroll_tin(seed, expr)` を返す。
5. 失敗し、構文エラーと判断できるなら `syntax_error` を返す。
6. メンションを含む混在テキストなら、文字列とメンションを `Nested` にまとめる。
7. それ以外は元の文字列を `String` として返す。

`process_ast` は `String` を `parse_dsl` に通し、`Nested` の子を再帰処理し、`Element` はそのまま返す。

## 既知の差分・制約

- ダイスは送信時に評価せず、seed と式を `diceroll` Tin に入れる。実際の出目は表示時に Saignant が `SeededEntropy` で評価する。
- 構文エラー判定はヒューリスティックを含む。ダイスらしいが解析できない文字列だけをエラーにし、通常文はそのまま残す。
- 能力値変動・代入の対象はキャラクター ID と status 名で、値の反映は cutlery / 将来の状態管理に委ねる。
