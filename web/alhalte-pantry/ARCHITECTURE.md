# alhalte-pantry

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `web/alhalte-pantry`（package `@alhalte/pantry`）  
> テスト: `web/alhalte-pantry/tests/{pantry,codec,crypto,compression,opener,adapters}.test.ts`

## 責務

ブラウザ側で tin を取得・復号・展開し、アセット ID（tin id）から一意に取り出せる形でキャッシュする。cellar からのダウンロード、部屋鍵の解決、PADMÉ の除去、同時リクエストの重複排除を担う。UI には依存しない。

## 公開 API / 外部接続点

### 外部接続

| 接続先 | 内容 |
| --- | --- |
| `alhalte-cellar` | `GET {baseUrl}/tin/{tinId}`。`HttpCellarClient` が本体と `X-Alhalte-Wrapped-Key` を読む。 |

部屋鍵は `PantryOptions.roomKey`、`setRoomKey`、`setRoomKeyProvider`、または `window.location.hash` から解決する。

### `Pantry`

- `new Pantry(options?)`
- `getTin(tinId, options?)`: キャッシュ → in-flight 重複排除 → cellar 取得 → 復号 → キャッシュ。
- `putTin(tin)`、`hasTin(tinId)`、`deleteTin(tinId)`、`clearCache()`
- `setRoomKey(key)`、`setRoomKeyProvider(provider)`、`getRoomKey()`
- `inFlightCount`

`GetTinOptions` は `roomKey`、`wrappedKey`、`forceFetch`、`stripPadme`。

### コーデック

- `decodeTin(wireBytes, options?) -> DecryptedTin`
- `encodeTin(tin, encryption, compression, options?) -> EncodedTinResult`（テスト・クライアント側 canner 補助）
- `parseOpener` / `serializeOpener` / `hasWireNonce`
- `decompress` / `compressLz4BlockSizePrefixed`
- `parseRoomKey`、`normalizeTinId`、`deriveConvergentIdAndKey`、`computeBlake3Hash`、`encryptAes256GcmSiv`、`decryptAes256GcmSiv`
- `hasIso7816Padding`、`removeIso7816Padding`、`applyIso7816Padding`

### キャッシュアダプタ

`PantryCacheAdapter` は `get` / `set` / `has` / `delete` / `clear` を持つ。既定は環境に応じて以下を選ぶ。

- ブラウザで `caches` があれば `CacheStoragePantryCacheAdapter`
- ブラウザで `indexedDB` があれば `IndexedDbPantryCacheAdapter`
- それ以外は `MemoryPantryCacheAdapter`

## データ形式

`DecryptedTin`:

```ts
interface DecryptedTin {
  tinId: string;      // 64 hex
  label: bigint;      // u64 LE
  payload: Uint8Array;
  opener: Opener;     // 元の暗号・圧縮
}
```

CacheStorage アダプタは `opener(1B) || label(8B LE) || payload` を `Response` として保存する。IndexedDB は `label` を 10 進文字列にして 64bit 精度を保つ。

## 内部フロー

### `getTin`

1. tin id を hex / 32 バイトへ正規化する。
2. `forceFetch` でなければキャッシュを引く。
3. 同じ tin id の in-flight Promise があればそれを返す。
4. cellar から `{ tinBytes, wrappedKey }` を取得する。
5. 部屋鍵を、リクエスト指定 → 静的鍵 → provider → URL フラグメントの順で解決する。
6. `decodeTin` で復号・展開する。
7. キャッシュへ保存し、in-flight から削除する。

### `decodeTin`

1. opener を読み、暗号・圧縮を検証する。
2. `Plaintext`: 本体を展開し、wire 全体の BLAKE3-256 を tin id にする。
3. `RoomKey`: 12 バイト Nonce と本体を分離し、部屋鍵で復号する。PADMÉ があれば除去してから展開する。tin id は wire 全体の BLAKE3-256。
4. `Convergent`: wrapped key を部屋鍵で開いて収束鍵を得る。Zero Nonce で本体を復号し、展開する。平文から BLAKE3 XOF 512bit を再計算し、tin id と鍵を検証する。
5. `expectedTinId` があれば照合する。
6. 平文の先頭 8 バイトを label、残りを payload として返す。

### エンコード

`encodeTin` はテスト用・補助用。平文を作り、圧縮し、暗号方式に応じて wire を組み立てる。`RoomKey` は Nonce を前置し、`Convergent` は収束鍵を部屋鍵で wrap して `wrappedKey` を返す。

## 既知の差分・制約

- 仕様では canner の wasm が再エンコードと暗号化を担うが、現状のブラウザ側暗号・展開はこのパッケージの TS 実装。
- `HttpCellarClient` は owner / session を送らない。cellar の download は session 検証を行わないため現状は動くが、認可を強める場合はヘッダ追加が必要。
- `encodeTin` の Brotli は `node:zlib` に依存する。ブラウザ単体では使えない。
- CacheStorage のキーは `https://alhalte.internal/tin/{tinId}` という内部 URL。実際のネットワークへは出ない。
- `removeIso7816Padding` は末尾の `0x80` を探す。平文末尾が偶然 `0x80` で終わる場合は誤検出し得るため、`stripPadme` は RoomKey の低エントロピー tin に限定する。
