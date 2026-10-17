# Alhalte
セルフホストVTTのためのモジュール群。ccfoliaなどとは異なり、PLやGMの運用による仮想的な概念ではなく、第一級概念としてシーンが存在する。用語としては全体の構成としてのrestaurant、配布される構成としてのfranchise、client matter総体としてのcutleryなど。

## tin-rs
Alhalteの中で型を自己主張するバイナリデータ、tinを扱うためのlibrary crate。

データは暗号化方式と圧縮方式をそれぞれ4bitで表すopener、暗号化された任意長のバイナリデータcontentsを持ち、contentsの先頭64bitにはそのバイナリデータが従う形式の識別子を64bit長のxxHash（XXH64、seed 0）にかけたもの、labelを持つことが期待される。

識別子はmimetypeを基本とするが、vendor/(category/)typeの形式で独自の識別子を持つことも許可される。

### バイト列表現
openerは1バイトで、上位4bitが暗号表現、下位4bitが圧縮表現を表す。tin全体のバイト列は以下の通り。

```
tin       = opener(1B) ‖ [nonce(12B) ※暗号表現が0010の場合のみ] ‖ body
plaintext = label(8B, u64 LE) ‖ payload
body      = encrypt(compress(plaintext))
```

つまり、labelは平文の先頭に置かれ、圧縮、暗号化の順に適用される。圧縮のパラメータは実装間の恒等性のため固定する（Zstandard: level 19、LZ4: block形式・サイズ前置、Brotli: quality 11 / lgwin 22）。

### tin id
tin idは256bit。暗号表現が0001の場合はplaintextのBLAKE3 XOF出力512bitの前半256bitとし、収束暗号による重複排除を可能にする。それ以外の場合は、直列化したtin全体のBLAKE3-256とする。
### opener
#### 暗号表現

暗号は排他的。0010の場合、openerの直後に96bitのNonceが続く。0001の場合は暗号化後の恒等性のためZero Nonceが使われているため、必要ない。

| ビット列 | それが表す表現                                                     |
| ---- | ----------------------------------------------------------- |
| 0000 | 平文                                                          |
| 0001 | plaintextのBLAKE3 XOF出力512bitの後半256bitを鍵とする、Zero NonceのAES-256-GCM-SIV暗号（収束暗号）。前半256bitはtin idとなる。 |
| 0010 | URLフラグメントに含まれる256bitの部屋の鍵と、ランダムな96bitのNonceを用いたAES-256-GCM-SIV暗号。 |
| 以降   | 未割り当て                                                       |

#### 圧縮表現
| ビット列 | それが表す圧縮方式 |
| ---- | --------- |
| 0000 | 無圧縮       |
| 0001 | Zstandard |
| 0010 | LZ4       |
| 0011 | Brotli    |
| 以降   | 未割り当て     |

### label
ここでは特別な表現、およびプリミティブ型のみを記述する。

| 識別子                 | それが表すもの                                    |
| ------------------- | ------------------------------------------ |
| （ハッシュが0）            | need not know                              |
| `primitive/string`  | UTF-8の文字列型                                 |
| `primitive/u*`      | リトルエンディアンの符号なし整数表現。`*`には8から128までの2のべき乗が入る。 |
| `primitive/i*`      | リトルエンディアンの符号あり整数表現。`*`には8から128までの2のべき乗が入る。 |
| `primitive/f*`      | IEEE 754が定める浮動小数表現。`*`には16から64までの2のべき乗が入る。 |
| `primitive/bool`    | 正論理の真理値表現。                                 |
| `alhalte/unix-time` | 64bitの符号なしUNIX時間。                          |
| `alhalte/tin-id`    | 256bitのtinを一意に指すID。                        |

### 構造化contents
プリミティブ型以外で複数のフィールドを持つlabelのpayloadは、別段の定めがない限り[postcard](https://github.com/jamesmunns/postcard)（serde）で直列化する。フィールドの順序は、各labelの定義に書かれた順とする。

## generast-rs
チャットの中で用いられる汎用構文木を扱うためのlibrary crate。型定義は、擬似コード的に以下の通り。

```rust
struct Nested {
	meta: Tin,
	contents: Vec<Generast>,
}

enum Generast {
	Element(Tin),
	String(String),
	Nested(Nested),
}
```

### label定義
bold、italic等の中身の記述がないもののcontentsは0バイトで、最上位のalhalte/generast以外は暗号化しないことが推奨される。

| 識別子                               | それが表すもの                                             |
| --------------------------------- | --------------------------------------------------- |
| alhalte/generast                  | 汎用構文木。                                              |
| alhalte/generast/style/bold       | Nested用太字定義。                                        |
| alhalte/generast/style/italic     | Nested用斜体定義。                                        |
| alhalte/generast/style/h1         | Nested用heading 1定義。                                 |
| alhalte/generast/style/h2         | Nested用heading 2定義。                                 |
| alhalte/generast/style/h3         | Nested用heading 3定義。                                 |
| alhalte/generast/style/h4         | Nested用heading 4定義。                                 |
| alhalte/generast/style/hidden     | Nested用spoiler定義。                                   |
| alhalte/generast/style/text-color | Nested用文字色定義。                                       |
| alhalte/generast/style/pill       | Nested用ピル定義。カスタムの背景色を設定してピルを作る。                       |
| alhalte/generast/style/ruby       | Nested用ルビ定義。ルビ文字列をcontentsにかける。                     |
| alhalte/generast/style/boten      | Nested用傍点定義。                                        |
| alhalte/generast/style/indent     | Nested用字下げ定義。u32で何文字字下げするかを指定する。                    |
| alhalte/generast/style/hanging    | Nested用のN字下げ、折り返してM字下げ。u32を2つ持つ。                    |
| alhalte/generast/style/grounded   | Nested用の地付き定義。                                      |
| alhalte/generast/style/kerning    | Nested用N字詰め（N字で自動改行する）定義。                           |
| alhalte/generast/style/h-in-v     | Nested用縦中横定義。                                       |
| alhalte/generast/diceroll         | ダイス構文木。                                             |
| alhalte/generast/judgement        | 判定構文木。                                              |
| alhalte/generast/character-id     | キャラクターに対するメンション表現。                                  |
| alhalte/generast/fluct            | キャラクターのステータス変動{CharacterID}.{N}+{M}の表現。Nは文字列、Mはi32。 |
| alhalte/generast/assign           | キャラクターのステータス代入{CharacterID}.{N}={M}の表現。Nは文字列、Mはi32。 |
| alhalte/generast/syntax-error     | 構文エラーを指摘する型。                                        |
### ダイス構文木

ひとつの演算系のための抽象構文木表現。純粋なサイコロ操作の抽象表現としての形式を保ち、それをどう解釈するかはcutlery上の拡張機能としてのゲームルーム拡張機能に委ねられる。`alhalte/generast/diceroll`は実際には送信者側が入力したエントロピーを持つ。

構文木は`generast-rs`内のネイティブな代数的データ型として表現され、再帰降下的に評価される。構文木の構築・変換段階では乱数状態を持たない純粋なデータ構造としての参照透過性を保ち、評価時にcutleryから可変状態としての&mut Entropyを注入して木を深さ優先・左から右へと走査することで、決定論的な出目と最終結果を算出する。

`Entropy`は`fn take(&mut self, faces: Integer) -> Result<Integer, EntropyError>`（1..=facesを返す）を持つtraitとして定義する。物理演算ダイスなどの外部拡張は、このtraitを実装して出目を供給する。標準実装`SeededEntropy`は、`alhalte/generast/diceroll`に格納された256bitのseedで初期化したChaCha20（rand_chacha互換）のストリームから、u64 LEを取り出して棄却サンプリングで偏りなく出目を得る。そのため、seedとダイス構文木が同じであれば、どの実装でも同じ結果を再現できる。`alhalte/generast/diceroll`のcontentsは、`seed: [u8; 32]`、`expr: ダイス構文木`の順でpostcardにより直列化する。

ゲームシステム固有の成功判定、クリティカル・ファンブル処理、専用コマンドなどのドメイン依存ロジックは、ダイス構文木から判定構文木への変換として定義される。

#### ダイス構文木の型と演算系

##### 基本型

| 型 | 定義 |
| :--- | :--- |
| `Integer` | 符号なし整数。ダイスの面数、個数、リストの要素番号や長さを表す。 |
| `Rational` | 任意精度の有理数。整数および分数を包含し、四則演算および修正値計算のすべてを担う。0除算は評価エラーとする。 |
| `Bool` | 正論理の真理値表現。目標値との比較結果や振り足しの継続条件を表す。 |
| `String` | UTF-8の文字列型。選択項目やダイス表の結果テキストを表す。 |
| `Dice` | `Integer`型の面数Mを持つダイス定義。 |
| `List(T)` | 任意の型`T`の順序付きリスト。 |
| `Pred(T)` | `T`型の値に対する比較・判定条件を表す述語式ノード。 |

##### プリミティヴ演算ノード

評価器が直接解釈する最小限の基本演算ノード群。乱数状態`Entropy`を消費する操作を`Take`と`DiceUntil`の2つに局所化し、それ以外をすべて決定論的な純粋演算として定義する。ダイスに対する不正行為への厳密さよりも、物理演算ダイスなどの外部拡張の間口を広げることを優先し、`Entropy`はcutlery sideで生成する。

| 演算                         | 型シグネチャ（概念）                                              | 処理                                                                                                         |
| :------------------------- | :------------------------------------------------------ | :--------------------------------------------------------------------------------------------------------- |
| `ItoR`                     | `Integer -> Rational`                                   | 整数から有理数へのキャスト。`Integer`は`Rational`の部分集合であるため常に可能。                                                          |
| `Floor`                    | `Rational -> Integer`                                   | 有理数の小数点以下を切り捨てて整数へ射影する。`(1+3)D(2+2)`のように`Rational`上で計算した結果を`Dice`の面数や個数（`Integer`）へ戻す役割も担う。                |
| `Add`, `Sub`, `Mul`, `Div` | `Rational × Rational -> Rational`                       | 有理数の四則演算。                                                                                                  |
| `Eq`, `Lt`                 | `Rational × Rational -> Bool`                           | 有理数の等価判定（`==`, `=`）および小なり判定（`<`）。                                                                          |
| `Not`                      | `Bool -> Bool`                                          | 真理値の論理否定。                                                                                                  |
| `And`                      | `Bool × Bool -> Bool`                                   | 真理値の論理積。                                                                                                   |
| `If`                       | `Bool × T × T -> T`                                     | 真理値に応じて第2引数または第3引数を評価して返す条件分岐。                                                                             |
| `MakeDice`                 | `Integer -> Dice`                                       | 指定した面数Mを持つ`Dice`を生成する。                                                                                     |
| `Take`                     | `Dice -[Entropy]-> Integer`                             | 評価器の`Entropy`を消費し、`Dice`から1回の出目Nを返す最小の乱数プリミティヴ。                                                            |
| `DiceUntil`                | `Dice × Pred(List(Integer)) -[Entropy]-> List(Integer)` | これまでの出目リストに対する述語式`Pred`が真を返す間`Take`を繰り返し、蓄積された出目のリストを返す。無限ループ防止のため、対象`Dice`のすべての面で恒真となる停止不能な述語は構築時の検査でエラーとして棄却する。さらに、1回の評価全体での`Take`の総数が上限（既定値 4096）を超えた場合も評価エラーとする。 |
| `Filter`                   | `List(T) × Pred(T) -> List(T)`                          | リストの中から述語式`Pred`が真を返す要素のみを抽出し、新しいリストとして返却する。                                                               |
| `Fold`                     | `List(T) × T' × Expr -> T'`                             | リストの要素を初期値`T'`から左畳み込みで集約する。                                                                                |
| `Flatten`                  | `List(List(T)) -> List(T)`                              | ネストしたリストを1段階平坦化する。                                                                                         |
| `SortAsc`                  | `List(Rational) -> List(Rational)`                      | 有理数のリストを昇順に並び替えた新しいリストを返却する。                                                                               |
| `Slice`                    | `List(T) × Integer × Integer -> List(T)`                | リストの指定した開始位置（0始まり）から指定個数の要素を切り出し、新しいリストとして返却する。                                                            |
| `Length`                   | `List(T) -> Integer`                                    | リストの要素数を返却する。                                                                                              |

##### セマンティック演算

プリミティヴ演算の組み合わせ、あるいは評価器の組み込みメソッドとして提供される演算群。ダイス表現の意味論で、少なくともBCDiceで可能な表現を内包する。

| 演算                     | 型シグネチャ（概念）                                                        | 合成定義・処理                                                                                    |
| :--------------------- | :---------------------------------------------------------------- | :----------------------------------------------------------------------------------------- |
| `Ceil`                 | `Rational -> Integer`                                             | 有理数の切り上げ。`Floor`と四則演算・条件分岐の組み合わせとして評価される。                                                  |
| `Round`                | `Rational -> Integer`                                             | 有理数の四捨五入。`Floor(Add(x, 1/2))` として評価される。                                                    |
| `Ne`, `Le`, `Gt`, `Ge` | `Rational × Rational -> Bool`                                     | 比較演算子群。`Eq`、`Lt`、`Not`、`And` の組み合わせとして評価される。                                               |
| `Sum`                  | `List(Rational) -> Rational`                                      | 有理数の総和。初期値`0`と`Add`を用いた畳み込み。                                                               |
| `KeepHighest`          | `List(Rational) × Integer -> List(Rational)`                      | 有理数のリストから大きい順にk個選び返却する。`SortAsc`と末尾からの`Slice`（開始位置 L - k、長さ k）。                            |
| `KeepLowest`           | `List(Rational) × Integer -> List(Rational)`                      | 有理数のリストから小さい順にk個選び返却する。`SortAsc`と先頭からの`Slice`（開始位置 0、長さ k）。                                |
| `DropHighest`          | `List(Rational) × Integer -> List(Rational)`                      | 有理数のリストから大きい順にk個捨て、残りを返却する。`SortAsc`と先頭からの`Slice`（開始位置 0、長さ L - k）。                        |
| `DropLowest`           | `List(Rational) × Integer -> List(Rational)`                      | 有理数のリストから小さい順にk個捨て、残りを返却する。`SortAsc`と末尾からの`Slice`（開始位置 k、長さ L - k）。                        |
| `RollN`                | `Dice × Integer -[Entropy]-> List(Integer)`                       | 指定した個数Nだけ`Dice`を振る。「蓄積リストの`Length`がN未満である間」を条件として`DiceUntil`を実行する。                         |
| `CountSuccess`         | `List(Rational) × Pred(Rational) -> Integer`                      | 条件を満たす出目の個数（成功数）を算出する。`Length` と `Filter` の合成。                                             |
| `RerollDice`           | `List(Dice) × Pred(Integer) -[Entropy]-> List(Integer)`           | 個数振り足しダイス。各`Dice`に対して「直前の出目が振り足し条件を満たす間」を条件とする`DiceUntil`を順次実行し、全出目を`Flatten`で結合する。        |
| `UpperDice`            | `Dice × Integer × Pred(Integer) -[Entropy]-> List(List(Integer))` | 上方無限ロール。単一ダイスに対する`DiceUntil`（直前の出目が閾値以上の間継続）をnグループ分順次反復し、グループごとの出目リストを返却する。                |
| `D66`                  | `SortMode -[Entropy]-> Integer`                                   | D66ロール。6面ダイス2個の`RollN`で得た2要素リストに整列指定（そのまま、昇順、降順）を適用し、十の位と一の位として ` d1 * 10 + d2 ` を算出する。    |
| `Tally`                | `Dice × Integer -[Entropy]-> List(Integer × Integer)`             | 集計ダイス。`RollN`で得た出目リストに対し、各面 1..=M と等しい要素の個数を数え上げる。`TY`では個数0の要素を除外する。                       |
| `Choose`               | `List(String) × Integer -[Entropy]-> List(String)`                | チョイスコマンド。残存リストの`Length`を面数とする`MakeDice`と`Take`でインデックスを決定し、該当要素の抽出と残存リストの更新を指定個数分繰り返す非復元抽出。 |
| `Lookup`               | `List(Integer × String) × Integer -> String`                      | オリジナル表およびシステム固有表の参照。出目と一致するキーを持つペアを抽出して文字列を返す。                                             |
| `Repeat`               | `Integer × Expr -[Entropy]-> List(T)`                             | 繰り返しコマンド。任意の部分構文木`Expr`を指定回数だけ反復評価し、各回の結果のリストを返す。                                          |
### 判定構文木
各ゲームシステム固有のダイスボット拡張は、alhalte/generast/dicerollを一つのトークンとして見、前後の独自コマンドを構文解析して判定構文木を組み立てたり、その他のGenerastを組み立てる方式でルールを拡張する。

システム固有の成否・スペシャル・ファンブル等の意味付けを行った判定構文木へと変換する射として実装される。判定構文木の型定義は以下の通り。

```rust
pub struct JudgementData {
	/// 成功度
	///
	/// 負：失敗段階　0：中立　正: 成功段階
	pub degree: i8,
	/// 人間可読なシステム固有の成功判定名
	pub level_name: String,
	/// 何を判定しているか
	pub subject: Option<String>,
	/// 目標値
	pub target: Option<Rational>,	
	/// ダイス構文木
	pub roll: Generast,
}
```

## alhalte-banquette
セッションの部屋ごとに生成されるプロセス。どう情報を交換するかのみを関心とし、どんな情報を交換するかはclientに委ねられる。部屋全体の状態は仮想的なシーンである`/`を用いて同期される。`/`はdoneフラグを持ち、これが立っていれば部屋内の情報は全公開される。

sceneから誰も居なくなったら永続化を行い、そのシーンのために確保したメモリ領域を開放する。また、4分ごとに永続化とheartbeatを行い、誰も応答しないことを3回繰り返したらプロセスをexitする。

部屋の状態としてはscene-idからsceneを引くscene-biblioとcharacter-idからcharacterを引くcharacter-biblioを持つ。scene-idはUTF-8文字列で、ルートの仮想シーンは`/`である。character-idは64bitの符号なし整数で、banquetteが重複しないように払い出す。

characterの所有権を持つのはcharacter-biblio。characterはStringとTinの連想配列であり、`primitive/string`型の`std::name`、`alhalte/maitre-d/account-id`型の`std::owner_account`を持つことが推奨される。持っていなかった場合の実装はcutlery matter。

部屋の管理者は、`/`のstateにある`std::admins`（account-idのリストをpostcardで直列化したもの）で表す。部屋を作成したアカウントが最初の管理者となる。管理者だけが`/`のstate、および他人が所有するcharacterを変更できる。

**label定義**

| 識別子               | それが表すもの                            |
| ----------------- | ---------------------------------- |
| alhalte/banquette | alhalte-banquetteの状態空間をバイナリに固めたもの。 |

### scene
それぞれ独立したlogとstateを持つ独立した空間。シーンのid自体は公開情報だが、該当シーン内に所有するキャラクターが居ないアカウント（管理者を除く）によるシーンに対するログや盤面、シーン名の問い合わせに対して応答は行われない。

**label定義**
シーンのstateをBTreeMap連想配列statesと、シーンのchunkのtin idを順番に並べ、headがどのchunkの何番目を指しているかをu32で指定したもの、biblioを持つ。

| 識別子                     | それが表すもの                |
| ----------------------- | ---------------------- |
| alhalte/banquette/scene | sceneの状態空間をバイナリに固めたもの。 |

#### log
シーンごとに1 << N件のlogを1塊にしたchunkを持ち、headを用いて書き込み、読み込み先を指定する。Nはbanquetteの起動引数で指定し、既定値は8（256件）とする。headはbiblio内のchunkの番号（u32）と、chunk内の位置（u32）の組である。暗号化はcutlery matterであり、banquetteは暗号化されているか平文かを知る必要がない。

logは`Option<Tin>`で表現され、headを進めてデータを書き込むappend、headより前の特定のchunkをSome(T)で上書きするedit、headより前の特定のChunkをNoneで上書きするdelete、headを巻き戻すrewindによって操作され、それぞれの操作はシーン内のclientへ通知される。これは中に平文のTinが入った`HashMap<String, Tin>`を用いてduck typing的に扱う運用をしている。

これらのイベントはbanquetteによって仲介されて各clientに到達する。cutleryはこれらのイベントを正しく処理できることが期待される。NoneはTombstone扱いで、head以降の番地は不定値として無効扱い。

chunkが満杯になった場合、chunkを1つのtinに固め、cellarへと送信する。

**label定義**

| 識別子                                  | それが表すもの         |
| ------------------------------------ | --------------- |
| alhalte/banquette/log/chunk          | 1チャンク分のLog      |
| alhalte/banquette/log/biblio         | チャンク同士の接続の定義    |
| alhalte/banquette/log/change-bg-img  | シーンの背景画像の変更     |
| alhalte/banquette/log/change-bgm     | シーンのBGMの変更      |
| alhalte/banquette/log/character-came | シーンへのキャラクターの入室  |
| alhalte/banquette/log/character-gone | シーンからのキャラクターの退場 |

#### state
高頻度に更新される、スクリーンパネルなどのロギングにあまり意味がない状態を保存するための仕組み。WebRTCのDataChannelを用いて交換され、暗号化は行われない。banquetteはDataChannel確立のためのsignalingをWebSocket上で中継する。DataChannelを確立できないclientに対しては、WebSocket経由でstateを中継する。

StringからTinを引くBTreeMapを用いた連想配列としてシーンごとに更新を受け付け、シーン内に存在するclientに更新を通知する。tinのサイズ1つにつき4KiBまでに制限される。

`std::bgm`、`std::b-img`などのKeyで、BGMや背景画像など、素直な実装では最新状態の取得にコストがかかる状態の配信の仕組みとしても使われている。1以上の`::`を用いて名前空間を分離することが期待される。`std::*`は予約される。

## alhalte-maitre-d
認証をGoogleのOAuthに丸投げし、SQLiteを用いてインスタンス上の64bitのアカウントIDとインスタンス固有のソルトで暗号化されたGoogleアカウントのsub、ハンドルネーム、アイコンのtin idの対応表を作るほか、64bitのセッションIDを発行する。毎日午前4時（インスタンスのローカル時刻）に、前回の午前4時以降一度もheartbeatがなかったセッションを全件削除することでセッション切れとなる。Googleアカウントから継承するのはsubのみ。OAuthのclient IDとsecretはbistrotから引数として渡される。

通信の玄関口として機能し、WebSocket接続のヘッダ（`X-Alhalte-Room`、ブラウザからはクエリパラメータ`room`）に含まれる部屋IDを元に通信をdemultiplexし、UNIXソケット経由でbanquetteに通信内容を流し込む。部屋IDは64bitで、URLのパスに含まれる。部屋の鍵はURLフラグメントにのみ含まれ、サーバーには送信されない。まだbanquetteが起動されていない部屋であれば通信をキューし、banquetteを起動する。

さらに、セッションIDがあるアカウントIDに対して有効化の問い合わせも受け付ける。開発用に、debugビルドしたものでは暗号化されたsubが入るべき場所に0が入ったデバッグアカウントを作成することができる。

| 識別子                         | それが表すもの   |
| --------------------------- | --------- |
| alhalte/maitre-d/account-id | アカウントのID。 |
| alhalte/maitre-d/session-id | セッションのID。 |

## alhalte-cellar
ファイルを実体として、tin id、tinのパス、所有者、およびnullableなwrapped keyの直積である行を参照としてDBに保存する。

所有者、所有者の有効なセッションID、tin id、tinのsizeの直積を含むstoreリクエストを受け取り、所有者がそのデータを保存する権利があることを確認。所有者がtinを送信する必要がある場合は210を返答、送信の必要がない場合は211を、すでに所有しているtinであった場合212を返答する。

このときの送信の必要がない場合はすでにそのtinが保存されている場合であり、その場合は同じファイル実体を指す別の行としてDBに行が追加される。

送信の必要がないことをclientに告知することは帯域とclientが不要なことを知ることのトレードオフであるため、引数の--anti-oracleが明示的にtrueに設定にされている場合、210を返却し、受け取ったtinを`/dev/null`へ捨てる。

また、同様にunregisterも行なうことができ、定期的にどこからも参照されていない実体を削除する。

## alhalte-pantry
tin-idを受け取り、すでにpantryに存在するtinであればpantryから返却、未ダウンロードのtinであればalhalte-cellarにダウンロードリクエストを送信、URLフラグメントを使って復号し、ブラウザのキャッシュにアセットIDから一意に取り出し可能な形で保存する。

## alhalte-canner
クライアントサイドで動く純粋関数的なwasm。アセットの再エンコードと暗号化を行なう。アセットの再エンコードの実行は容量削減のためのoptionであって必須ではないが、インスタンス運用者のため、またユーザがアップロード制限を回避するために推奨される。実装する関数は以下の通り。

- encode-sound
- encode-image
- encrypt-high-entropy
- encrypt-low-entropy

encrypt-low-entropyは、発言のテキストデータなどの重複排除を行なう必要がない低エントロピーなデータを暗号化するための関数で、暗号表現0010のtinを生成する。まずplaintextの末尾に`0x80`を付け、PADMÉの長さになるまで`0x00`で埋める（ISO/IEC 7816-4方式。復号時に末尾から取り除く）。次に、ランダム生成したNonceと部屋の鍵で暗号化する。Nonceはopenerの直後に平文で書き込み、そのあとに暗号化したバイナリデータを書き込む。

encode-imageは容量削減のため、画像を最長辺最大1920pxに縮小してwebp（lossy、quality 80）に再エンコードする。encode-soundは音声を48kHz、128kbpsのopus（Ogg）に再エンコードする。

収束暗号による重複排除を成立させるため、再エンコードは**デバイス非依存の恒等性**を持たなければならない。つまり、同じ入力と同じcannerのバージョンからは、どの端末でもビット単位で同一の出力が得られることを保証する。このため、ブラウザ組み込みのエンコーダ（Canvas、WebCodecs）は使わない。デコード、リサンプリング、縮小、エンコードのすべてをcannerのwasm内で行い、libwebpとlibopus（固定小数点ビルド）をバージョン固定で静的にリンクし、単一スレッドで実行する。また、SIMD拡張のうちrelaxed-simdなど非決定的な命令は使わない。cannerのwasm自体もtinとして配布し、同じrestaurant内のclientが同じバイナリを使うようにする。

encrypt-high-entropyは、plaintextをBLAKE3 XOFで512bitハッシュ化し、前半256bitをtin id、後半256bitを収束暗号鍵として暗号表現0001のtinを生成する。さらに、収束暗号鍵をサーバーには未知なURLフラグメント中の部屋の鍵で暗号化し（ランダムNonceのAES-256-GCM-SIV、`nonce ‖ ciphertext`）、tin id、暗号化された暗号鍵（wrapped key）、tinの直積を返却する。

## alhalte-kitchen
部屋を固めた.zip、.tar.gzを受け取って内部でグラフ表現を構築、特定のキャラの始点ノードから終点ノードまでの道筋、あるいはシーンIDのリストを受け取ってログを直列化、EPUB、HTML5などの形式にトランスパイルする。

## alhalte-saignant
logを解釈し、適切なclassを割り当ててHTML構造に変換するモジュール。Generastの解釈も含む。cssはカテゴリごとに分割され、さらに構造的な正しさを保証するものと見た目の華美さを保証するものに分割される。スタイルはMaterial 3 Expressive準拠。

## alhalte-argenterie
ユーザにUIを提供するSvelte製公式cutlery。UI Componentには[Noph UI](https://noph.dev/)を用いる。使い勝手はccfoliaをベースとしている。画面レイアウトとしては、横長画面では左側にスクリーンパネルや駒などを置くフィールドパネル、右側のコントロールパネルは複数のステートが存在するため後述。縦長画面では全体にフィールド、その上にボトムシートでコントロールパネルという配置になる。スタイルはMaterial 3 Expressive準拠。

ここでは、左上から順に、同じ段、またはその子構成要素を（）で囲い、！でクリック時に表示、［］で縦に並んだ要素、...で同様のフォーマットの繰り返しを表現する。

### フィールド画面
フィールドマップはマウスホイールの回転、左クリックしながらのマウスの移動、タッチ移動、ピンチでの拡大縮小が可能である。また、フィールドパネルの上のレイヤ、画面に左端には以下のUI要素を備える。もちろんのこと、それぞれのシーンでフィールド画面は完全に独立している。

```
［
	（部屋名）
	※キャラクターはイニシアチブ順に並んでいる
	（右下にイニシアチブ表記のあるキャラクターアイコン　［
		（ラベル名　最大値/現在地　※裏にプログレスバー的に最大値分の現在値を表示）
		...※2つで1行
	］）
］
```

### ホーム画面
ccfoliaと異なり、URLの全体をサーバーが保持できないため、自身が管理者である部屋のリストの表示は行わず、localStorageに保存された、訪れたことのある部屋のリストをカードとして表示し、右下に部屋の新規作成ボタンを設けるに留める。

部屋の作成時にはオーバーレイで以下のメニューが出現する。

```
［
	（文字列”部屋の作成”、中央寄せ）
	（単一行入力フィールド、ルーム名）
	（
		ルーム招待URL※クライアントサイドでランダム生成したURLフラグメント付き、編集不能
		”リンクをコピー”ボタン
	）
	（中央寄せ警告文”このリンクを紛失した場合、部屋へのアクセスは永久に失われます！”）
	（”入室する”ボタン）
］
```

### 部屋内

**コントロールパネル共通フッタ―**
入力フィールドにおいて、チャットパレットの各行と部分一致していればそれを入力フィールドの直上に、下側が繋がり、入力フィールドと同じ背景色で、区切り線を挟んで部分一致しているもののリスト、上側は角丸で、他のUI要素を覆いかぶせる形でサジェストする。サジェストはタップすれば部分一致している部分をその全体に置換、外をタップすれば消える。

```
［
	（
		ログタブへの切り替えボタン　シーングラフタブへの切り替えボタン
		シーン設定タブへの切り替えボタン　シーン管理タブへの切り替えボタン
		キャラクター管理タブへの切り替えボタン
		拡張機能管理タブへの切り替えボタン
	）
	（キャラクター選択ボタン（アイコン　キャラクター名）　送信ボタン）
	（入力フィールド）
］
```

**ログタブ**
ログはチャンク単位で保持し、スクロールが片端に達してからpantryへの要求、ログの解釈を行う。ログは`alhalte/generast`形式の"content"、`alhalte/generast/character-id`形式の"speaker"、`alhalte/unix-time`形式の"timestamp"を持つことが期待される。それぞれのフォールバックは（空）、"Anonymous"、1970年1月1日0時0分0秒。

```
［
	（左寄せしまうボタン, 中央寄せシーン名, 右寄せ歯車アイコン！［
		ZIPログ出力　［
			管理者専用
			シーン名変更　シーン削除　シーンメンバー管理
		］
	］）
	［
		（アイコン　［
			（名前　タイムスタンプ）
			（コンテンツ）
		］）
		...
	］
	（以下コントロールパネル共通フッター）
］
```

**シーングラフタブ**
VSCodeのGit Graphのような形式で、アクセスできるシーンのログから生成したキャラクターの移動をグラフビューとして図示する。この中のコミットのように図示され、横にシーン名が添えられた点をクリックまたはタッチすることで該当シーンに移動し、ログタブに遷移する。

無相関なシーン同士は別々のブランチのように描画される。自身がアクセス権のないシーンへの移動はそれぞれ点"？"への点線で図示される。この点は未知のシーン1つごとに1つ。時系列が早いものは常に上、遅いものは常に下。同じシーンに戻ってきた場合、点は複製される。

**シーン設定タブ**
```
［
	（中央寄せ単一行入力欄としてのシーン名　兼　スプリットボタン［
		ほかのシーン名
		...※タップで設定中のシーンを切り替え
	］）
	（ヘディング”Juices”）
	（カード”背景画像”※クリック、またはタップでファイル選択）
	（説明”画像は一方を指定すればもう一方を自動で計算します。もう一方も入力した場合、端を切り落として揃えます。”）
	（数値入力欄”横幅”　数値入力欄”縦幅”）
	
	（（ピル”BGM1”※クリック時、またはタップでファイル選択）　音量スライダー）
	（（ピル”BGM2”※クリック時、またはタップでファイル選択）　音量スライダー）
	（（ピル”BGM3”※クリック時、またはタップでファイル選択）　音量スライダー）
	
	（ヘディング”管理”）
	［
		ボタン”メンバーを移動”
		ボタン”シーンを新規作成”
		ボタン”シーンを削除”※赤ボタン、全画面確認ダイアログあり
	］
］
```

**シーンを新規作成**
オーバーレイとして出現する。

```
［
	中央寄せ文字列”シーンを新規作成”
	単一行入力欄”シーン名”
	トグル”Juicesを継承”
	右寄せボタン”新規作成”
］
```

**メンバーを移動**
オーバーレイとして出現する。存在しないシーン名が割り振られていた場合、Juicesを継承して新規作成する。

```
［
	中央寄せ文字列”メンバーを移動”
	［
		（
			キャラクターアイコン　キャラクター名
			右向き矢印　単一行入力欄　兼　スプリットボタン！［
				シーン名
				...
			］
		）
		...
	］
］
```

**キャラクタータブ**

```
［
	（キャラクターアイコン　［
		キャラクター名　説明※120文字以降は……として省略
	］　削除ボタン※全画面警告あり　駒として配置ボタン）
	...
］
```

各キャラクターをクリックすると、以下のパネルが開く。

```
［
	（中央寄せ文字列”キャラクター編集”）
	（アイコン※クリックでファイル選択　単一行入力欄”名前”　整数入力欄”イニシアチブ”）
	（複数行入力欄”説明”）
	（単一行入力欄”参照”）
	［
		ステータス
		（単一行入力欄”ラベル”　整数入力欄”現在値”　整数入力欄”最大値”）
		...
	］
	［
		パラメータ
		（単一行入力欄”ラベル”　整数入力欄”値”）
		...
	］
	（複数行入力欄”チャットパレット”）
	（トグル”ステータスを秘匿”）
］
```

**拡張機能タブ**
拡張機能をwasmとして読み込み、cellarに保管し、部屋内で共有する。拡張機能の追加や優先度の変更、削除、無効化ができるのは部屋の管理者のみ。部屋の管理者以外にはグレーアウトして見え、触っても動かない。

```
［
	（有効化チェックボックス　拡張機能名　右寄せ削除ボタン）
	...
］※並べ替え可能リスト
```
## alhalte-bistrot
公式のrestaurant。引数で渡されたBistrot.tomlを読み、各モジュールに渡す引数を生成する純粋関数としての性質を持つほか、各モジュールを適切に起動するためのsystemd設定ファイルを生成することもできる。秘密情報はropsで暗号化することを前提とする。

# 拡張機能
Alhalteの拡張機能は、基本的にはログを送信するとき、導入している拡張機能が優先度順に呼び出され、String(送信したログ)のGenerastと発言者の直積をスタートとして前のGenerast構造に対する写像を返すことを繰り返し、最終的な構文木を送信することで実装される。拡張機能は`process-generast(Generast) -> Generast`を実装するwasmとして配布される。

## 拡張機能ABI
ブラウザで追加のツールなしに実行できるよう、拡張機能はWASIに依存しないcore wasmモジュールとし、以下をexportする。

| export | シグネチャ | 説明 |
| --- | --- | --- |
| `memory` | memory | 線形メモリ。 |
| `alhalte_abi_version` | `() -> i32` | ABIのバージョン。現行は`1`。 |
| `alloc` | `(len: i32) -> i32` | ホストが入力を書き込む領域を確保する。 |
| `dealloc` | `(ptr: i32, len: i32)` | 領域を解放する。 |
| `process_generast` | `(ptr: i32, len: i32) -> i64` | 入力として、`alhalte/generast`をlabelに持つ平文tin（`Generast`と発言者の`character-id`をpostcardで直列化したもの）を受け取り、出力の`(ptr << 32) \| len`を返す。出力は同じ形式の平文tinとする。エラーの場合は`alhalte/generast/syntax-error`を含むGenerastを返す。 |

## 公式拡張機能
いずれもwinnowを用いた実装。
### alhalte-dsl
BCDice系だがより表現の制約が緩いダイス表現記法や、キャラクターのパラメータ変動記法などのAlhalte内で動いているDSLをGenerastとして出力する。

### aozora
github.com/kinoko0518/aozora-rs を用いてLogで送信されたメッセージを青空文庫書式として解釈し、Generastとして出力する。

### markdown
送信されたメッセージをMarkdownとして解釈し、Generastとして出力する。

### coc-6
ダイス構文木のあとの【……】技能表記からCoC6版用の判定構文木を生成する処理などを行う。

### coc-7
ダイス構文木のあとの【……】技能表記からCoC7版用の判定構文木を生成する処理などを行う。

### emoklore
{X}DM表記の処理やダイス構文木のあとの〈……〉技能表記からエモクロア版用の判定構文木を生成する処理を行う。