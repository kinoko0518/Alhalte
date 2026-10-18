# generast-rs

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/generast-rs`（package 名 `generast`）  
> テスト: `crates/generast-rs/tests/{ast_test,dice_test,label_test}.rs`

## 責務

チャットで用いる汎用構文木 `Generast`、そのラベル定義、決定論的なダイス構文木と評価器、ゲームシステム固有の判定木 `JudgementData` を提供する。UI や通信には依存しない。

## 公開 API / 外部接続点

外部接続点はない。主な公開 API は以下。

### AST

```rust
pub struct Nested {
    pub meta: Tin,
    pub contents: Vec<Generast>,
}

pub enum Generast {
    Element(Tin),
    String(String),
    Nested(Nested),
}
```

- `Generast::string` / `element` / `nested`
- 装飾コンストラクタ: `bold`、`italic`、`h1`〜`h4`、`hidden`、`boten`、`grounded`、`text_color`、`pill`、`ruby`、`indent`、`hanging`、`kerning`
- 要素コンストラクタ: `character_id`、`fluct`、`assign`、`syntax_error`
- `to_tin` / `from_tin`、`encode_generast` / `decode_generast`
- 拡張機能入出力: `encode_ext_io(generast, speaker_id)` / `decode_ext_io(tin)`。payload は postcard の `(Generast, u64)`。

### ダイス

- `DiceExpr`、`Dice`、`DiceValue`、`RollRecord`、`SortMode`、`FoldOp`
- `Entropy` trait、`SeededEntropy`、`Integer = u64`
- `DiceExpr::eval` / `eval_with_trace` / `eval_with_limit` / `eval_with_trace_and_limit`
- `DiceExpr::dice_until`（停止不能な述語を構築時に拒否）、`validate_dice_until`
- `DEFAULT_MAX_TAKES = 4096`

### 判定・状態

- `JudgementData { degree: i8, level_name, subject, target, roll }`
- `DicerollData { seed, expr }`、`FluctData`、`AssignData`
- `Pred`（比較、AND/OR/NOT、`Always` / `Never`、`LengthLessThan`、`LastRoll`）
- `Rational`（任意精度有理数、0 除算は評価エラー）

### ラベル

`LABEL_ALHALTE_GENERAST` と、23 個の標準ラベル（下記）および `*_tin` / `extract_*` ヘルパー。

## データ形式

### ラベル一覧

| 識別子 | 内容 |
| --- | --- |
| `alhalte/generast` | 汎用構文木のルート。 |
| `alhalte/generast/style/bold` | 太字。 |
| `alhalte/generast/style/italic` | 斜体。 |
| `alhalte/generast/style/h1`〜`h4` | 見出し 1〜4。 |
| `alhalte/generast/style/hidden` | スポイラー。 |
| `alhalte/generast/style/text-color` | 文字色。payload は色文字列。 |
| `alhalte/generast/style/pill` | ピル。payload は背景色文字列。 |
| `alhalte/generast/style/ruby` | ルビ。payload はルビ文字列。 |
| `alhalte/generast/style/boten` | 傍点。 |
| `alhalte/generast/style/indent` | 字下げ。payload は u32。 |
| `alhalte/generast/style/hanging` | ぶら下げ。payload は `(u32, u32)`。 |
| `alhalte/generast/style/grounded` | 地付き。 |
| `alhalte/generast/style/kerning` | カーニング。payload は u32。 |
| `alhalte/generast/style/h-in-v` | 縦中横。 |
| `alhalte/generast/diceroll` | ダイス構文木。payload は `DicerollData`。 |
| `alhalte/generast/judgement` | 判定木。payload は `JudgementData`。 |
| `alhalte/generast/character-id` | キャラクターへのメンション。payload は u64。 |
| `alhalte/generast/fluct` | 能力値変動。payload は `FluctData`。 |
| `alhalte/generast/assign` | 能力値代入。payload は `AssignData`。 |
| `alhalte/generast/syntax-error` | 構文エラー。payload はエラー文字列。 |

`Nested` の `meta` は上記の装飾ラベル、`Element` は要素ラベルを運ぶ。中身を持たない装飾の `contents` は空でよい。ルート以外は暗号化しないことが推奨される。

### ダイス構文木

`DiceExpr` は以下のカテゴリを持つ。

| カテゴリ | ノード |
| --- | --- |
| リテラル | `Integer`、`Rational`、`Bool`、`String`、`Dice`、`List` |
| 数値・比較 | `ItoR`、`Floor`、`Ceil`、`Round`、`Add`、`Sub`、`Mul`、`Div`、`Eq`、`Ne`、`Lt`、`Le`、`Gt`、`Ge` |
| 論理 | `Not`、`And`、`If` |
| ダイス | `MakeDice`、`Take`、`DiceUntil`、`RollN`、`RerollDice`、`UpperDice`、`D66` |
| リスト | `Filter`、`Fold`、`Flatten`、`SortAsc`、`Slice`、`Length`、`Sum`、`KeepHighest`、`KeepLowest`、`DropHighest`、`DropLowest` |
| 意味付け | `CountSuccess`、`Tally`、`Choose`、`Lookup`、`Repeat` |

`Pred` は `Eq` / `Ne` / `Lt` / `Le` / `Gt` / `Ge` / `And` / `Or` / `Not` / `Always` / `Never` / `LengthLessThan` / `LastRoll` を持つ。

## 内部フロー

### ダイス評価

1. `SeededEntropy::from_seed(seed)` で ChaCha20 RNG を初期化する。
2. `Entropy::take(faces)` は `next_u64` を棄却サンプリングし、`1..=faces` を返す。
3. `DiceExpr` を深さ優先・左から右へ評価する。
4. `Take` の総数を数え、既定 4096 回を超えたらエラーにする。
5. `eval_with_trace` は `RollRecord` の列を返す。

同じ seed と同じ式からは常に同じ結果になる。`DiceUntil` は全面で恒真になる述語を構築時に拒否し、無限ループを防ぐ。

### 拡張機能 I/O

`encode_ext_io` は `(Generast, speaker_id)` を postcard で Tin の payload にし、ラベルを `alhalte/generast` にする。拡張機能はこの Tin を受け取り、同じ形式で返す。`decode_ext_io` はラベルを検証してから復元する。

## 既知の差分・制約

- ディレクトリ名は `generast-rs`、package 名は `generast`。
- `Generast::Element` は任意の Tin を運べるため、未知ラベルも構文木として保持できる。解釈は利用側（saignant、拡張機能）に委ねる。
- ダイスの `Integer` は `u64`。負の値は `Rational` で扱う。
- `DiceExpr::eval` は暗号論的な乱数ではなく、seed 由来の決定論的乱数を使う。
