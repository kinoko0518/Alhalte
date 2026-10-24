# tin-rs

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/tin-rs`（package 名 `tin`）  
> テスト: `crates/tin-rs/tests/spec_tests.rs`

## 責務

Alhalte の全モジュールで共有される、ラベル付き・自己記述的なバイナリコンテナ `tin` を提供する。平文データ構造である `TinContent` と、暗号化・圧縮メタデータ（Opener）や `TinId` を内包する自己完結型コンテナ `Tin` を定義し、暗号化、圧縮、`TinId` の導出、およびラベル（識別子文字列の XXH64 ハッシュ）の計算を一箇所に集約する。ネットワーク通信やファイル I/O は持たない。

## 公開 API / 外部接続点

外部プロセスやネットワークへの接続点はない。他 crate から利用される主な公開 API は以下のとおり。

### 型定義

| 型 | 内容 |
| --- | --- |
| `TinContent` | 平文の中身を表す構造体。`label: Label`（`u64`）と `payload: Vec<u8>` を保持する（旧 `Tin { label, payload }` から改名）。 |
| `Tin` | エンコード済みバイト列（またはコンテンツ）、`TinId`、および Opener 情報（`Compression` と暗号化方式・鍵/Nonce 情報）を保持する自己完結型コンテナ。呼び出し側が暗黙の `Opener` を別途引き回す必要がない。 |
| `TinId([u8; 32])` | 256bit の Tin 識別子を表す Newtype。 |
| `ConvergentKey([u8; 32])` | 収束暗号（`0001`）で導出・使用される 256bit 鍵の Newtype。 |
| `RoomKey([u8; 32])` | 部屋鍵暗号（`0010`）で使用される 256bit 鍵の Newtype。 |
| `Opener` | 1 バイトのコンテナヘッダ。`Encryption` と `Compression` を持ち、`From<Opener> for u8` および `TryFrom<u8> for Opener` により 1 バイト表現と相互変換する。 |
| `Encryption` | 暗号化モード。`Plaintext` (`0000`)、`Convergent` (`0001`)、`RoomKey([u8; 12])` (`0010`、96bit Nonce を保持)。 |
| `Compression` | 圧縮モード。`None` (`0000`)、`Zstd` (`0001`)、`Lz4` (`0010`)、`Brotli` (`0011`)。 |
| `CompressedTinContent` | 圧縮済みバイト列 `bin: Vec<u8>` と `compression: Compression` を保持する中間表現。 |
| `Label` | フォーマット識別子の XXH64 ハッシュ値（`u64`）。 |

### `TinContent` の構築・型変換

従来のプリミティブ変換メソッド群（旧 `from_*` / `as_*` / `to_*`）は廃止し、Rust 標準の `From<T> for TinContent` および `TryFrom<TinContent> for T`（`TryInto`）トレイト実装に統一している。

- **基本コンストラクタ**:
  - `TinContent::new(label, payload)` / `TinContent::with_ident(ident, payload)` / `TinContent::need_not_know(payload)`
- **`From` / `TryFrom`（`TryInto`）によるプリミティブ・識別子変換**:
  - 論理値: `bool`（`primitive/bool`）
  - 符号なし整数: `u8`, `u16`, `u32`, `u64`, `u128`（`primitive/u8` 〜 `primitive/u128`、リトルエンディアン）
  - 符号あり整数: `i8`, `i16`, `i32`, `i64`, `i128`（`primitive/i8` 〜 `primitive/i128`、リトルエンディアン）
  - 浮動小数点数: `f16`（`half::f16`）, `f32`, `f64`（`primitive/f16`, `primitive/f32`, `primitive/f64`、IEEE 754 リトルエンディアン）
  - 文字列: `String`, `&str`（`primitive/string`、UTF-8）
  - 識別子: `TinId`（`alhalte/tin-id`、32 バイト）
- **UNIX 時刻**:
  - `alhalte/unix-time` ラベルを用いた `u64` 秒の変換ヘルパー
- **Postcard 直列化**:
  - `TinContent::from_postcard(ident, val)` / `TinContent::from_postcard_label(label, val)` / `TinContent::to_postcard()`（およびラベル固定型向けの `Tinnable` トレイト）
- **平文バイト列変換**:
  - `TinContent::to_bytes(&self) -> Vec<u8>`: `label(8B, u64 LE) || payload` の非圧縮平文バイト列へ変換する（旧 `to_plaintext`）。
  - `TinContent::from_bytes(bytes: &[u8]) -> Result<Self, TinError>`: 平文バイト列から `TinContent` を復元する（旧 `from_plaintext`）。

### `Tin` の構築・ワイヤ表現・暗号化

- `Tin` は内部に `tin_id: TinId`、圧縮方式 `compression: Compression`、暗号化状態（平文 / `ConvergentKey` を伴う収束暗号 / `nonce: [u8; 12]` と `RoomKey` を伴う部屋鍵暗号）、およびコンテンツバイト列を保持する。
- `TinContent` から圧縮（`compress` → `CompressedTinContent`）と暗号化を経て `Tin` を構築し、ワイヤバイト列への直列化およびワイヤバイト列からの `Tin::decode` を行う。
- パディング済み生バイト列向けの低レベル関数（`alhalte-canner` 等で利用）:
  - `encode_raw_plaintext`、`encode_raw_convergent`、`encode_raw_room_key`、`encode_raw_plaintext_bytes`、`decode_raw_plaintext_bytes`
- 圧縮・展開: `compress` / `CompressedTinContent::decompress`
- 暗号プリミティブ: `encrypt_aes_256_gcm_siv` / `decrypt_aes_256_gcm_siv`、`derive_convergent_id_and_key`、`verify_convergent`、`ZERO_NONCE`

### ラベル定義

`label_hash(ident)` は識別子文字列の XXH64 ハッシュ（seed 0）を計算する。`LABEL_NEED_NOT_KNOW = 0`、およびプリミティブ型・標準型の定数 `IDENT_*` / `LABEL_*` を公開する。

## データ形式

```text
opener    = encryption(4bit) ‖ compression(4bit)
tin       = opener(1B) ‖ [nonce(12B) ※RoomKey のみ] ‖ body
plaintext = label(8B, u64 LE) ‖ payload   (= TinContent::to_bytes())
body      = encrypt(compress(plaintext))
```

### 暗号方式（Opener 上位 4bit）

| ビット列 | バリアント | 方式 |
| --- | --- | --- |
| `0000` | `Encryption::Plaintext` | 平文（暗号化なし）。 |
| `0001` | `Encryption::Convergent` | `plaintext` の BLAKE3 XOF 512bit 出力の後半 256bit を `ConvergentKey` とし、Zero Nonce（`[0u8; 12]`）の AES-256-GCM-SIV で暗号化する収束暗号。前半 256bit が `TinId` となる。 |
| `0010` | `Encryption::RoomKey([u8; 12])` | 256bit の `RoomKey` とランダムな 96bit Nonce による AES-256-GCM-SIV 暗号。Nonce（12B）はワイヤ上で opener の直後に平文として配置される。 |

### 圧縮方式（Opener 下位 4bit）

| ビット列 | バリアント | 方式と固定パラメータ |
| --- | --- | --- |
| `0000` | `Compression::None` | 無圧縮。 |
| `0001` | `Compression::Zstd` | Zstandard（level 19）。 |
| `0010` | `Compression::Lz4` | LZ4 block 形式（非圧縮サイズを `u32` LE で先頭に付与）。 |
| `0011` | `Compression::Brotli` | Brotli（quality 11 / lgwin 22）。 |

### `TinId` の導出規則

- **収束暗号（`0001`）**: 圧縮・暗号化前の `plaintext`（`label || payload`）に対する BLAKE3 XOF 512bit の前半 256bit（32B）。
- **平文（`0000`）・部屋鍵暗号（`0010`）**: ワイヤ上に直列化された `tin` 全体に対する BLAKE3-256 ハッシュ（32B）。

### 標準ラベル一覧

| 識別子 | 内容 |
| --- | --- |
| `0` | `need not know`（フォーマット指定なし） |
| `primitive/string` | UTF-8 文字列 |
| `primitive/u8` … `primitive/u128` | 符号なし整数（リトルエンディアン） |
| `primitive/i8` … `primitive/i128` | 符号あり整数（リトルエンディアン） |
| `primitive/f16` / `f32` / `f64` | IEEE 754 浮動小数点数（リトルエンディアン） |
| `primitive/bool` | 1 バイト論理値（`1` = true、`0` = false、それ以外はエラー） |
| `alhalte/unix-time` | `u64` LE の UNIX 時刻（秒） |
| `alhalte/tin-id` | 256bit `TinId`（32 バイト） |

複数のフィールドを持つ構造化データは postcard（serde）で直列化して `payload` に格納する。

## 内部フロー

### エンコード（`TinContent` → `Tin` → ワイヤバイト列）

1. `TinContent::to_bytes()` により `label(8B, u64 LE) ‖ payload` の平文バイト列を生成する。
2. 指定された `Compression` で圧縮し、`CompressedTinContent` を得る。
3. 暗号方式に応じて暗号化し、Opener 情報と `TinId` を備えた `Tin` を構築する。
   - **平文**: 圧縮済みバイト列をそのまま本体とし、ワイヤ全体の BLAKE3-256 を `TinId` とする。
   - **収束暗号**: 平文から `TinId` と `ConvergentKey` を導出し、圧縮済みバイト列を Zero Nonce で暗号化する。
   - **部屋鍵暗号**: 渡された `RoomKey` と 12 バイト Nonce で暗号化し、Opener の直後に Nonce を配置したワイヤ全体の BLAKE3-256 を `TinId` とする。

### デコード（ワイヤバイト列 → `Tin` / `TinContent`）

1. 先頭 1 バイトから `Opener::try_from(u8)` で暗号方式と圧縮方式を解析し、未定義のビットパターンを拒否する。
2. 暗号方式が `RoomKey` の場合は続く 12 バイトを Nonce として切り出し、残りを暗号文本体とする。
3. 対応する鍵（`ConvergentKey` または `RoomKey`）で復号し、`Compression` に従って展開する。
4. 収束暗号の場合は、展開後の平文から `TinId` と `ConvergentKey` を再導出し、与えられた鍵および期待される `TinId` と一致することを検証する（`verify_convergent`）。
5. `TinContent::from_bytes` で先頭 8 バイトの `label` と残りの `payload` に分離し、さらに必要に応じて `TryInto` や `to_postcard` で目的の Rust 型へ変換する。

## 不変条件・エラー

- `#![forbid(unsafe_code)]` を適用し、ライブラリ内で `unwrap` / `expect` / `panic!` を使用しない。
- 未定義の Opener ビット列は `TinError::UnsupportedEncryption` / `UnsupportedCompression` を返す。
- 復号鍵が必要な暗号方式で鍵が与えられない場合は `MissingKey` を返す。
- 収束暗号における鍵の不一致は `KeyMismatch`、`TinId` の不一致は `TinIdMismatch` を返す。
- `TryFrom<TinContent>` によるプリミティブ変換時、ラベルが異なる場合は `LabelMismatch`、ペイロード長が不正な場合は `InvalidPayloadLength`、`bool` の値が `0` / `1` 以外の場合は `InvalidBool` を返す。

## 既知の差分・制約

- Issue #272 により、従来の `Tin { label, payload }` は `TinContent` に改名され、`Tin` は Opener（圧縮・暗号・鍵情報）と `TinId` を内包するコンテナ型へと再設計された。また、プリミティブ変換は `From` / `TryFrom` トレイト実装に置き換えられた。
- `encode_raw_*` 系の関数は、PADMÉ パディング済みの生平文バイト列を直接処理するために `alhalte-canner` から利用される。
- ラベルハッシュは実行時（`label_hash`）とコンパイル時定数（`const_xxh64`）の双方で計算できる。各モジュール固有のラベルはそれぞれの crate 内で定義する。
