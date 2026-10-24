# generast-rs

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/generast-rs`（package 名 `generast`）  
> テスト: `crates/generast-rs/tests/{ast_test,dice_test,label_test}.rs`

## 責務

チャット本文を表現する汎用構文木 `Generast`、その標準ラベル定義、決定論的なダイス構文木と評価器、およびゲームシステム固有の判定木 `JudgementData` を提供する。UI やネットワーク通信には依存しない純粋なデータ・評価ライブラリである。

## 公開 API / 外部接続点

外部接続点はない。主な公開 API は以下のとおり。

### 構文木（AST）

```rust
pub struct Nested {
    pub meta: TinContent,
    pub contents: Vec<Generast>,
}

pub enum Generast {
    Element(TinContent),
    String(String),
    Nested(Nested),
}
```

- 基本コンストラクタ: `Generast::string` / `element` / `nested`
- 装飾ノード（`Nested`）コンストラクタ: `bold`、`italic`、`h1`〜`h4`、`hidden`、`boten`、`grounded`、`text_color`、`pill`、`ruby`、`indent`、`hanging`、`kerning`
- 要素ノード（`Element`）コンストラクタ: `character_id`、`fluct`、`assign`、`syntax_error`
- コンテナ変換: `to_tin` / `from_tin`（`alhalte/generast` ラベルの `TinContent` との相互変換）、およびワイヤバイト列との `encode_generast` / `decode_generast`
- 拡張機能入出力ヘルパー: `encode_ext_io(generast, speaker_id)` / `decode_ext_io(tin_content)`。ペイロードは `(Generast, u64)` を postcard で直列化したバイト列。

### ダイス構文木・評価器

- 型定義: `DiceExpr`、`Dice`、`DiceValue`、`RollRecord`、`SortMode`、`FoldOp`、`Integer = u64`
- 乱数抽象化: `Entropy` トレイト、`SeededEntropy`（ChaCha20 ベース）
- 評価メソッド: `DiceExpr::eval` / `eval_with_trace` / `eval_with_limit` / `eval_with_trace_and_limit`
- 停止性検証: `DiceExpr::dice_until`（全面で恒真となり停止しない述語を構築時に拒否）、`validate_dice_until`
- 上限定数: `DEFAULT_MAX_TAKES = 4096`

### 判定・ステータス操作データ

- `JudgementData { degree: i8, level_name, subject, target, roll }`
- `DicerollData { seed, expr }`、`FluctData`、`AssignData`
- `Pred`（比較演算、論理演算 AND/OR/NOT、`Always` / `Never`、`LengthLessThan`、`LastRoll`）
- `Rational`（任意精度有理数、ゼロ除算は評価時エラーとして扱う）

### ラベル定義

ルートラベル `LABEL_ALHALTE_GENERAST` と 23 個の標準ラベル（下表）、および各ラベルに対応する `TinContent` の構築（`*_tin`）・抽出（`extract_*`）ヘルパーを公開する。その際、文字列・整数などのプリミティブ payload は `TinContent` の `From` / `TryFrom` 実装、構造化 payload は `from_postcard` / `to_postcard` を用いて変換する（#272）。

## データ形式

### 標準ラベル一覧

| 識別子 | 種別 | 内容・Payload 形式 |
| --- | --- | --- |
| `alhalte/generast` | ルート | 汎用構文木のルート（`Generast` または拡張 I/O の `(Generast, u64)`）。 |
| `alhalte/generast/style/bold` | `Nested` | 太字。 |
| `alhalte/generast/style/italic` | `Nested` | 斜体。 |
| `alhalte/generast/style/h1`〜`h4` | `Nested` | 見出し 1〜4。 |
| `alhalte/generast/style/hidden` | `Nested` | スポイラー（伏せ字）。 |
| `alhalte/generast/style/text-color` | `Nested` | 文字色。payload は色指定文字列。 |
| `alhalte/generast/style/pill` | `Nested` | ピル装飾。payload は背景色指定文字列。 |
| `alhalte/generast/style/ruby` | `Nested` | ルビ。payload はルビ文字列。 |
| `alhalte/generast/style/boten` | `Nested` | 傍点。 |
| `alhalte/generast/style/indent` | `Nested` | 字下げ。payload は `u32`。 |
| `alhalte/generast/style/hanging` | `Nested` | ぶら下げインデント。payload は `(u32, u32)`。 |
| `alhalte/generast/style/grounded` | `Nested` | 地付き（行末揃え）。 |
| `alhalte/generast/style/kerning` | `Nested` | カーニング。payload は `u32`。 |
| `alhalte/generast/style/h-in-v` | `Nested` | 縦中横。 |
| `alhalte/generast/diceroll` | `Element` | ダイスロール構文木。payload は `DicerollData`。 |
| `alhalte/generast/judgement` | `Element` | 判定結果木。payload は `JudgementData`。 |
| `alhalte/generast/character-id` | `Element` | キャラクターへのメンション。payload は `u64`。 |
| `alhalte/generast/fluct` | `Element` | キャラクター能力値の増減。payload は `FluctData`。 |
| `alhalte/generast/assign` | `Element` | キャラクター能力値への代入。payload は `AssignData`。 |
| `alhalte/generast/syntax-error` | `Element` | 構文エラー。payload はエラーメッセージ文字列。 |

`Nested.meta` には装飾ラベル（またはグループ化用の `need not know` ラベル `0`）、`Element` には要素ラベルを持つ `TinContent` を格納する。子要素を持たない装飾ノードでは `contents` を空ベクタとしてよい。また、ルートの `alhalte/generast` を包む外側の `Tin` 以外は暗号化しないことが推奨される。

### ダイス構文木

`DiceExpr` は以下のカテゴリのノードで構成される。

| カテゴリ | ノード |
| --- | --- |
| リテラル | `Integer`、`Rational`、`Bool`、`String`、`Dice`、`List` |
| 数値・比較演算 | `ItoR`、`Floor`、`Ceil`、`Round`、`Add`、`Sub`、`Mul`、`Div`、`Eq`、`Ne`、`Lt`、`Le`、`Gt`、`Ge` |
| 論理・条件分岐 | `Not`、`And`、`If` |
| ダイスロール | `MakeDice`、`Take`、`DiceUntil`、`RollN`、`RerollDice`、`UpperDice`、`D66` |
| リスト操作 | `Filter`、`Fold`、`Flatten`、`SortAsc`、`Slice`、`Length`、`Sum`、`KeepHighest`、`KeepLowest`、`DropHighest`、`DropLowest` |
| 集計・選択・反復 | `CountSuccess`、`Tally`、`Choose`、`Lookup`、`Repeat` |

条件述語 `Pred` は `Eq` / `Ne` / `Lt` / `Le` / `Gt` / `Ge` / `And` / `Or` / `Not` / `Always` / `Never` / `LengthLessThan` / `LastRoll` をサポートする。

## 内部フロー

### ダイスの決定論的評価

1. 32 バイトの `seed` から `SeededEntropy::from_seed(seed)` で ChaCha20 乱数生成器を初期化する。
2. `Entropy::take(faces)` は `next_u64` に対する棄却サンプリングによって偏りのない `1..=faces` の整数を返す。
3. `DiceExpr` を深さ優先・左から右の順序で決定論的に評価する。
4. 評価全体での `Take`（ダイスを振る回数）の総数をカウントし、上限（既定 4096 回）を超えた場合はエラーとして中断する。
5. `eval_with_trace` を呼ぶと、評価結果とともに各ダイスロールの履歴（`Vec<RollRecord>`）を返す。

同一の `seed` と同一の `DiceExpr` からは、実行環境に依存せず常に同じ出目とトレースが得られる。また `DiceUntil` はすべての出目で真となる述語を構築時に弾くことで、評価時の無限ループを防止する。

### 拡張機能 I/O のエンコード・デコード

`encode_ext_io` は `(Generast, speaker_id)` のタプルを postcard で直列化し、ラベルを `alhalte/generast` とした `TinContent`（およびその平文バイト列 `to_bytes()` またはワイヤ形式 `Tin`）を生成する。各拡張機能はこのコンテナを受け取って構文木を変換し、同じ形式で返す。`decode_ext_io` はラベルが `alhalte/generast` であることを検証した上でタプルを復元する。

## 既知の差分・制約

- ディレクトリ名は `generast-rs`、Cargo package 名は `generast`。
- Issue #272 に伴い、構文木内部の `Nested.meta` および `Generast::Element` が保持する型は、平文のラベル＋ペイロードを表す `TinContent` として整理される（旧来の文書・型エイリアスで `Tin` と表記されていた箇所に相当）。
- `Generast::Element` は任意のラベルを持つ `TinContent` を格納できるため、将来追加される未知のラベルも構文木を壊さずに保持・中継できる。実際の解釈は描画側（`alhalte-saignant`）や各拡張機能に委ねる。
- ダイス構文木における整数リテラル `Integer` は符号なしの `u64` であり、負の数値は `Rational` への変換や減算ノードで表現する。
