# tin-rs

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/tin-rs`（package 名 `tin`）  
> テスト: `crates/tin-rs/tests/spec_tests.rs`

## 責務

Alhalte の全モジュールが共有する、ラベル付き・自己記述的なバイナリコンテナ `tin` を提供する。暗号化、圧縮、tin id の導出、ラベル（識別子の XXH64）を一箇所に閉じ込める。ネットワークやファイル I/O は持たない。

## 公開 API / 外部接続点

外部接続点はない。他 crate から利用される公開 API は以下。

### 型

| API | 内容 |
| --- | --- |
| `Tin` | `label: u64` と `payload: Vec<u8>`。 |
| `Opener` | 1 バイトの opener。`Encryption` と `Compression` を持つ。 |
| `Encryption` | `Plaintext` (`0000`)、`Convergent` (`0001`)、`RoomKey` (`0010`)。 |
| `Compression` | `None` (`0000`)、`Zstd` (`0001`)、`Lz4` (`0010`)、`Brotli` (`0011`)。 |
| `Label` / `TinId` | `u64` / `[u8; 32]`。 |

### `Tin` の構築・解釈

- `Tin::new(label, payload)` / `with_ident(ident, payload)` / `need_not_know(payload)`
- `from_string` / `as_string`
- `from_u8` … `from_u128` / `as_u8` … `as_u128`
- `from_i8` … `from_i128` / `as_i8` … `as_i128`
- `from_f16` / `as_f16`、`from_f32` / `as_f32`、`from_f64` / `as_f64`
- `from_bool` / `as_bool`、`from_unix_time` / `as_unix_time`
- `from_tin_id` / `as_tin_id`
- `from_postcard` / `from_postcard_label` / `to_postcard`
- `to_plaintext` / `from_plaintext`

### ワイヤー表現

- `Tin::encode_plaintext(compression)` → `(wire_bytes, tin_id)`
- `Tin::encode_convergent(compression)` → `(wire_bytes, tin_id, convergent_key)`
- `Tin::encode_room_key(compression, room_key, nonce)` → `(wire_bytes, tin_id)`
- `Tin::decode(wire_bytes, key)` → `(tin, opener, tin_id)`
- 生バイト列向け: `encode_raw_plaintext`、`encode_raw_convergent`、`encode_raw_room_key`、`encode_raw_plaintext_bytes`、`decode_raw_plaintext_bytes`
- 圧縮: `compress` / `decompress`
- 暗号: `encrypt_aes_256_gcm_siv` / `decrypt_aes_256_gcm_siv`、`derive_convergent_id_and_key`、`verify_convergent`、`ZERO_NONCE`

### ラベル

`label_hash(ident)` は XXH64（seed 0）。`LABEL_NEED_NOT_KNOW = 0`。プリミティブ型と標準ラベルの定数、`IDENT_*` / `LABEL_*` を公開する。

## データ形式

```text
opener    = encryption(4bit) ‖ compression(4bit)
tin       = opener(1B) ‖ [nonce(12B) ※RoomKey のみ] ‖ body
plaintext = label(8B, u64 LE) ‖ payload
body      = encrypt(compress(plaintext))
```

### 暗号

| ビット列 | 方式 |
| --- | --- |
| `0000` | 平文。 |
| `0001` | `plaintext` の BLAKE3 XOF 512bit の後半 256bit を鍵とする、Zero Nonce の AES-256-GCM-SIV（収束暗号）。前半 256bit が tin id。 |
| `0010` | 部屋の鍵 256bit とランダム 96bit Nonce による AES-256-GCM-SIV。Nonce は opener の直後に平文で置く。 |

### 圧縮

| ビット列 | 方式と固定パラメータ |
| --- | --- |
| `0000` | 無圧縮。 |
| `0001` | Zstandard level 19。 |
| `0010` | LZ4 block 形式、非圧縮サイズを u32 LE で前置。 |
| `0011` | Brotli quality 11 / lgwin 22。 |

### tin id

- `0001`: `plaintext` の BLAKE3 XOF 512bit の前半 256bit。
- `0000` / `0010`: 直列化した tin 全体の BLAKE3-256。

### ラベル

| 識別子 | 内容 |
| --- | --- |
| `0` | need not know |
| `primitive/string` | UTF-8 |
| `primitive/u8` … `primitive/u128` | 符号なし整数、LE |
| `primitive/i8` … `primitive/i128` | 符号あり整数、LE |
| `primitive/f16` / `f32` / `f64` | IEEE 754、LE |
| `primitive/bool` | `1` = true、`0` = false。それ以外はエラー。 |
| `alhalte/unix-time` | u64 LE の UNIX 秒 |
| `alhalte/tin-id` | 256bit tin id |

複数フィールドを持つ payload は postcard（serde）で直列化する。

## 内部フロー

### エンコード

1. `Tin` を `label(8B LE) ‖ payload` の平文へ変換する。
2. 指定された圧縮方式で圧縮する。
3. 暗号方式に応じて暗号化する。
   - 平文: そのまま。
   - 収束: 平文から tin id と鍵を導出し、Zero Nonce で暗号化。
   - 部屋鍵: 渡された鍵と Nonce で暗号化し、Nonce を wire に前置。
4. opener を先頭に付ける。
5. tin id を上記規則で計算する。

### デコード

1. opener を読み、未割り当ての暗号・圧縮を拒否する。
2. 暗号方式に応じて Nonce と body を切り出す。
3. 復号・展開する。
4. 収束暗号では復号後の平文から tin id と鍵を再計算し、渡された鍵と一致することを検証する。
5. `label(8B LE) ‖ payload` を `Tin` に戻す。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。
- 未割り当ての opener は `TinError::UnsupportedEncryption` / `UnsupportedCompression`。
- 復号鍵が必要な方式で鍵がない場合は `MissingKey`。
- 収束暗号の鍵不一致は `KeyMismatch`、tin id 不一致は `TinIdMismatch`。
- プリミティブの payload 長不一致は `InvalidPayloadLength`、bool の不正値は `InvalidBool`。
- ライブラリ内で `unwrap` / `expect` / `panic!` を使わない。

## 既知の差分・制約

- 仕様上の `tin id` は 256bit で、実装でも `[u8; 32]`。
- `encode_raw_*` は PADMÉ パディング済みの生平文を受け取るため、`alhalte-canner` から利用される。
- ラベルの識別子は文字列から実行時・定数の両方で計算できる。追加ラベルは各モジュールで `label_hash` または `const_xxh64` を使って定義する。
