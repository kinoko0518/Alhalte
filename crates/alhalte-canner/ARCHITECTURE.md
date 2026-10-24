# alhalte-canner

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-canner`  
> テスト: `crates/alhalte-canner/tests/{encryption_test,image_test,sound_test,padding_test}.rs`

## 責務

画像・音声アセットの決定論的な再エンコードと、平文のエントロピーに応じた暗号化（PADMÉ パディングを伴う低エントロピー暗号化、および収束暗号＋鍵ラップによる高エントロピー暗号化）を提供する。収束暗号による重複排除（deduplication）を成立させるため、同一の入力データと同一の `canner` バージョンからは、実行環境に依存せずビット単位で同一の出力を生成することを目標とする。ネットワーク I/O は持たない。

## 公開 API / 外部接続点

外部接続点はない。主な公開 API は以下のとおり。

### 画像の再エンコード

- `encode_image(input: &[u8]) -> Result<Vec<u8>>`
- `MAX_IMAGE_SIDE = 1920`
- `WEBP_LOSSY_QUALITY = 80.0`

入力画像をデコードし、長辺が 1920px を超える場合に限りアスペクト比を維持して縮小した上で、lossy WebP（quality 80）へ再エンコードする。環境間の浮動小数点誤差を排除するため、リサイズは 16.16 固定小数点演算によるバイリニア補間で行う。

### 音声の再エンコード

- `encode_sound(input: &[u8]) -> Result<Vec<u8>>`
- `TARGET_SAMPLE_RATE = 48_000`
- `TARGET_BITRATE_BPS = 128_000`
- `OPUS_FRAME_SIZE = 960`

入力音声をデコードし、固定小数点の線形補間で 48kHz にリサンプリングした上で、128kbps の Opus（Ogg コンテナ）へ再エンコードする。チャンネル数はモノラルなら 1ch、ステレオなら 2ch とし、3ch 以上の入力は 2ch へダウンミックスする。

### 暗号化・復号

`tin-rs` の `TinContent`（平文の `label` + `payload`）、`RoomKey`、`ConvergentKey`、`TinId`、`Compression` を用いて、以下の 2 系統の暗号化を提供する（#272）。

- **低エントロピー暗号化（RoomKey 直接暗号化 + PADMÉ パディング）**:
  - `encrypt_low_entropy(content, compression, room_key, nonce) -> (wire_bytes, tin_id)`（または自己完結型 `Tin`）
  - `encrypt_low_entropy_random(content, compression, room_key)`
  - `decrypt_low_entropy(wire_bytes, room_key) -> TinContent`
- **高エントロピー暗号化（収束暗号 + RoomKey による鍵ラップ）**:
  - `encrypt_high_entropy(content, compression, room_key, wrap_nonce) -> HighEntropyOutput`
  - `encrypt_high_entropy_random(content, compression, room_key)`
  - `decrypt_high_entropy(wire_bytes, wrapped_key, room_key) -> TinContent`
  - `HighEntropyOutput { tin_id, wrapped_key, tin_bytes }`

### パディング

- `padme_padded_len(len)`、`pad_iso7816_4_padme(plaintext)`、`unpad_iso7816_4_padme(padded)`

## 内部フロー

### 低エントロピー暗号化（Opener 暗号ビット `0010`）

短いテキストなど、サイズや内容の推測を防ぎたい低エントロピーなデータに用いる。

1. `TinContent::to_bytes()` で `label(8B, u64 LE) || payload` の平文バイト列を得る。
2. 末尾に `0x80` を付加し、PADMÉ で定まる目標長まで `0x00` で埋める（ISO/IEC 7816-4 パディング）。
3. 指定された圧縮方式で圧縮し、`RoomKey` と 96bit Nonce を用いて AES-256-GCM-SIV で暗号化する。
4. Opener（1B）、Nonce（12B）、暗号文を連結してワイヤ表現とし、全体の BLAKE3-256 を `TinId` とする。

復号時は `RoomKey` で復号・展開した後、末尾の `0x00*` と区切りバイト `0x80` を除去し、`TinContent::from_bytes` で元の `TinContent` を復元する。

### 高エントロピー暗号化（Opener 暗号ビット `0001`）

画像・音声などサイズが大きく重複排除を行いたいアセットに用いる。

1. `TinContent::to_bytes()` の平文バイト列を BLAKE3 XOF 512bit でハッシュ化し、前半 256bit を `TinId`、後半 256bit を `ConvergentKey` とする。
2. 平文を圧縮し、`ConvergentKey` と Zero Nonce（`[0u8; 12]`）を用いて AES-256-GCM-SIV で収束暗号化する。
3. 導出した `ConvergentKey`（32B）を `RoomKey` とランダムな 96bit Nonce で AES-256-GCM-SIV 暗号化し、`nonce(12B) || ciphertext` を `wrapped_key`（`X-Alhalte-Wrapped-Key` 相当）とする。
4. `(tin_id, wrapped_key, tin_bytes)` を返す。

復号時は、まず `wrapped_key` を `RoomKey` で復号して `ConvergentKey` を取り出し、その `ConvergentKey` を用いて本体を復号・検証（`verify_convergent`）し、`TinContent::from_bytes` で復元する。

### PADMÉ パディング長計算

`padme_padded_len` は入力長 $L$（ISO/IEC 7816-4 の先頭 `0x80` 1 バイトを加えた平文長 $+ 1$ を下限とする）に対し、$E = \lfloor \log_2(L) \rfloor$、$S = \lfloor \log_2(E) \rfloor + 1$ とおき、$2^{E-S}$ の倍数へ切り上げることで情報漏洩を $O(\log \log L)$ ビット以下に抑える。

## 既知の差分・制約

- 初期構想ではクライアント側 WASM モジュールとして配布する想定だったが、現在は Rust ネイティブライブラリとして実装されており、ブラウザ側におけるアセット変換（WebP 最大 1920px / Opus 128kbps）や暗号化・再送を含む Tin 管理は `@alhalte/pantry`（`web/alhalte-pantry`）が担う（#302）。また、クライアント〜`cellar` 間で RoomKey 暗号化 Tin と `X-Alhalte-Wrapped-Key` をやり取りする際は、暗号化方式と復号鍵の対応を一致させて `invalid polyval tag` を防ぐ（#298）。
- Issue #272 に伴い、平文の `{ label, payload }` 表現は `TinContent`（および `to_bytes` / `from_bytes`）、鍵は `RoomKey` / `ConvergentKey` Newtype で扱う。
- 画像処理には `image` + `oxideav-webp`、音声処理には `symphonia` + `opus-pure` を使用した純 Rust 実装であり、OS やブラウザ固有のコーデックに依存しない。
- `encrypt_low_entropy` / `encrypt_high_entropy` はテスト容易性のため呼び出し側から Nonce を注入できる設計になっており、乱数 Nonce を用いる場合は `*_random` 関数を使用する。
