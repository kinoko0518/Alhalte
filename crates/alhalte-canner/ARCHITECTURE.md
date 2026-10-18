# alhalte-canner

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-canner`  
> テスト: `crates/alhalte-canner/tests/{encryption_test,image_test,sound_test,padding_test}.rs`

## 責務

アセットの再エンコードと暗号化を行う。収束暗号による重複排除を成立させるため、同じ入力と同じ canner のバージョンからは、端末に依存せずビット単位で同一の出力を返すことを目標とする。ネットワーク I/O は持たない。

## 公開 API / 外部接続点

外部接続点はない。主な公開 API は以下。

### 画像

- `encode_image(input: &[u8]) -> Result<Vec<u8>>`
- `MAX_IMAGE_SIDE = 1920`
- `WEBP_LOSSY_QUALITY = 80.0`

入力をデコードし、最長辺が 1920px を超える場合だけ縦横比を保って縮小し、lossy WebP（quality 80）へ再エンコードする。縮小は 16.16 固定小数点のバイリニア補間で行う。

### 音声

- `encode_sound(input: &[u8]) -> Result<Vec<u8>>`
- `TARGET_SAMPLE_RATE = 48_000`
- `TARGET_BITRATE_BPS = 128_000`
- `OPUS_FRAME_SIZE = 960`

入力をデコードし、固定小数点の線形補間で 48kHz にリサンプリングし、128kbps の Opus（Ogg コンテナ）へ再エンコードする。モノラルは 1ch、ステレオは 2ch、それ以上は 2ch へダウンミックスする。

### 暗号化

- `encrypt_low_entropy(tin, compression, room_key, nonce) -> (wire_bytes, tin_id)`
- `encrypt_low_entropy_random(tin, compression, room_key)`
- `decrypt_low_entropy(wire_bytes, room_key)`
- `encrypt_high_entropy(tin, compression, room_key, wrap_nonce) -> HighEntropyOutput`
- `encrypt_high_entropy_random(tin, compression, room_key)`
- `decrypt_high_entropy(wire_bytes, wrapped_key, room_key)`
- `HighEntropyOutput { tin_id, wrapped_key, tin_bytes }`

### パディング

- `padme_padded_len(len)`、`pad_iso7816_4_padme(plaintext)`、`unpad_iso7816_4_padme(padded)`

## 内部フロー

### 低エントロピー暗号化（`0010`）

1. `Tin` を `label(8B LE) || payload` の平文にする。
2. 末尾に `0x80` を付け、PADMÉ の目標長まで `0x00` で埋める（ISO/IEC 7816-4）。
3. 部屋鍵と Nonce で AES-256-GCM-SIV 暗号化する。
4. opener、Nonce、暗号文を wire に並べる。

復号時は展開後に末尾の `0x00*` と `0x80` を取り除く。

### 高エントロピー暗号化（`0001`）

1. 平文を BLAKE3 XOF 512bit でハッシュ化し、前半 256bit を tin id、後半 256bit を収束暗号鍵とする。
2. 平文を圧縮し、収束暗号鍵と Zero Nonce で暗号化する。
3. 収束暗号鍵を部屋鍵とランダム Nonce で暗号化し、`nonce(12B) || ciphertext` を wrapped key とする。
4. `(tin_id, wrapped_key, tin_bytes)` を返す。

復号時は wrapped key を部屋鍵で開き、得た収束暗号鍵で tin を復号する。

### PADMÉ

`padme_padded_len` は `E = floor(log2(L))`、`S = floor(log2(E)) + 1`、`2^(E-S)` の倍数へ切り上げる。パディングは平文長 + 1 を下限に計算する。

## 既知の差分・制約

- 仕様ではクライアント側 wasm の `encode-sound` / `encode-image` / `encrypt-*` として配布する想定だが、現状は Rust ライブラリで、WASM ABI は未エクスポート。ブラウザ側の暗号・展開は `web/alhalte-pantry` の TS 実装が担っている。
- 画像は `image` + `oxideav-webp`、音声は `symphonia` + `opus-pure` を使う純 Rust 実装。ブラウザ組み込みエンコーダには依存しない。
- 音声の再サンプリングは固定小数点だが、最終的な Opus ビット列の端末間一致はエンコーダ実装に依存する。テストでは決定論的な再現性を確認する。
- `encrypt_low_entropy` は呼び出し側が Nonce を渡す。ランダム Nonce が必要な場合は `*_random` を使う。
