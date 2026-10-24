# alhalte-pantry

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `web/alhalte-pantry`（パッケージ名 `@alhalte/pantry`）  
> テスト: `web/alhalte-pantry/tests/{pantry,codec,crypto,compression,opener,adapters}.test.ts`

## 責務

ブラウザ側で動作するクライアント状態・ストレージ管理層であり、UI には依存しない。上位の cutlery（`alhalte-argenterie` など）に対して以下の 3 つの責務を担う（Issue #302）。

1. **State 管理**: シーンごとにスコープされた Key-Value State（ルートシーン `/` の拡張機能設定や部屋アセット一覧、各シーンの名前・BGM・背景画像・視点・駒配置など）を保持し、サーバー（`alhalte-banquette`）との同期および変更通知を担う。
2. **Log 管理**: シーンごとにスコープされたログ（`biblio`・`frozen chunk`・`active chunk`・`head`）を保持し、ログの追記・編集・削除・巻き戻しイベントの処理と変更通知、送信失敗時の保留キュー管理、および起動・オンライン復帰時の同期と再送を担う。
3. **Tin 管理**: `alhalte-cellar` との通信を `get` / `put` として抽象化し、メディア（画像・音声）の再エンコード、暗号化・復号、圧縮・展開、PADMÉ パディング処理、ローカルキャッシュ、およびアップロード状態（`"putted"` / `"put-failed"`）の追跡とオンライン復帰時の再送を担う。

## 公開 API / 外部接続点

### 外部接続

| 接続先 | 内容 |
| --- | --- |
| `alhalte-cellar`（または `maitre-d` の `/api/cellar/*` プロキシ） | Tin の取得（`GET /tin/{tinId}`、本体および `X-Alhalte-Wrapped-Key` ヘッダの取得）と格納（`POST /store` による事前確認および `PUT /store/{tinId}` による本体アップロード）。 |
| `alhalte-banquette`（WebSocket 連携） | シーン単位の State（`StateSnapshot` / `StateUpdated` / `StateDeleted`）および Log（`LogBiblioSnapshot` / `LogChunkSnapshot` / `LogEventAppended` / `LogEventEdited` / `LogEventDeleted` / `LogEventRewound`）の同期・要求送出。 |

部屋鍵（32 バイト、`64 hex`）は `PantryOptions.roomKey`、`setRoomKey`、`setRoomKeyProvider`、または `window.location.hash`（URL フラグメント）から解決する。

### `Pantry`

- **初期化・鍵管理**:
  - `new Pantry(options?)`
  - `setRoomKey(key)`、`setRoomKeyProvider(provider)`、`getRoomKey()`
- **Tin 管理**:
  - `get(tinId, options?)` / `getTin(tinId, options?)`: ローカルキャッシュにあれば即座に返し、なければ in-flight 重複排除のうえで `cellar` から取得・復号・展開してキャッシュへ格納する。
  - `put(input, options?)` / `putTin(tin)`: メディア変換・エンコード・ローカル保存と `cellar` への格納を行う。すでに `"putted"` 状態であれば no-op とし、失敗時は `"put-failed"` として記録する。
  - `hasTin(tinId)`、`deleteTin(tinId)`、`clearCache()`、`getTinStatus(tinId)`（`"putted"` | `"put-failed"` | `"cached"`）
  - `retryFailedTins()`: `"put-failed"` 状態の Tin をオンライン復帰時に再送する。
  - `inFlightCount`
- **State 管理**:
  - シーン単位（ルートシーン `/` を含む）の State 取得・更新・削除 API と、変更を購読するサブスクリプション API。
  - `StateSnapshot` / `StateUpdated` / `StateDeleted` を取り込み、購読者へ変更を即時通知する。
- **Log 管理**:
  - シーン単位の `biblio`（`frozen chunk` の `TinId` 配列）、`active chunk`、`head` の取得・更新 API と、変更を購読するサブスクリプション API。
  - 追記・編集・削除・巻き戻し要求の送出とイベント適用、および通信失敗時の保留キュー（pending queue）取得・オンライン復帰時の同期と再送 API。

`GetTinOptions` は `roomKey`、`wrappedKey`、`forceFetch`、`stripPadme` を受け取る。

### コーデック・暗号・メディア変換

- **メディア再エンコード**:
  - 画像は取り込み時に長辺最大 1920px の WebP へ変換する。
  - 音声は取り込み時に最大 128kbps の Opus へ変換する。
- **Tin エンコード / デコード**:
  - `decodeTin(wireBytes, options?) -> DecryptedTin`
  - `encodeTin(tin, encryption, compression, options?) -> EncodedTinResult`
  - `parseOpener` / `serializeOpener` / `hasWireNonce`
  - `decompress` / `compressLz4BlockSizePrefixed`
- **暗号・ハッシュ・パディング**:
  - `parseRoomKey`、`normalizeTinId`、`deriveConvergentIdAndKey`、`computeBlake3Hash`、`encryptAes256GcmSiv`、`decryptAes256GcmSiv`
  - `hasIso7816Padding`、`removeIso7816Padding`、`applyIso7816Padding`

### キャッシュ・ストレージアダプタ

`PantryCacheAdapter` は `get` / `set` / `has` / `delete` / `clear` を備える。既定では実行環境に応じて以下の順で選択する。

1. ブラウザで `caches`（Cache API）が利用可能なら `CacheStoragePantryCacheAdapter`
2. ブラウザで `indexedDB` が利用可能なら `IndexedDbPantryCacheAdapter`
3. それ以外の環境では `MemoryPantryCacheAdapter`

## データ形式

### `DecryptedTin`

```ts
interface DecryptedTin {
  tinId: string;      // 64文字の16進文字列（32バイト）
  label: bigint;      // u64 リトルエンディアン
  payload: Uint8Array;
  opener: Opener;     // 元の暗号化方式・圧縮方式
}
```

`CacheStoragePantryCacheAdapter` は `opener(1B) || label(8B LE) || payload` を `Response` として保存する。`IndexedDbPantryCacheAdapter` は 64bit 整数の精度を保つため `label` を 10 進文字列として保存する。

### シーン State / Log 構造

- **Scene State**: `sceneId -> Map<key, DecryptedTin | JsonValue>`。ルートシーン `/` には部屋全体で共有される設定（`extension::<id>`、`extension::__seeded`、`asset::<id>` など）を保持し、各シーンにはシーン固有の状態（`std::name`、`std::bgm*`、`bgm::*`、`std::b-img*`、`view::transform`、`piece::<id>::*` など）を保持する。
- **Scene Log**:
  - `biblio`: 確定・凍結済みチャンク（`frozen chunk`）の `TinId` をチャンク番号順に保持する配列。
  - `active chunk`: 現在追記中のチャンクのログエントリ列。
  - `head`: 現在の書き込み位置（チャンク番号およびチャンク内インデックス）。
  - `pendingQueue`: 送信失敗により保留されているログ操作リクエストの FIFO キュー。

## 内部フロー

### 1. State 管理（Issue #301 / #302）

1. シーンごとに独立した State マップを保持する。ルートシーン `/` の State は入室時から常時購読・保持対象とし、管理者が行った拡張機能の ON/OFF 切り替え（`extension::<id>`）や部屋アセットの追加・削除（`asset::<id>`）を部屋内の全クライアントへ即座に伝播させる。
2. サーバーから `StateSnapshot` を受信した際は、対象シーンの State を権威あるスナップショットとして冪等に置換し、差分を購読者へ通知する。
3. `StateUpdated` / `StateDeleted` を受信した際は、対象シーンの該当キーを更新または削除し、登録されている変更リスナー（UI コンポーネントや拡張機能パイプラインなど）へ即座に通知する。
4. クライアントからの更新・削除要求はサーバーの ack（`StateUpdated` / `StateDeleted`）をもって確定させる。

### 2. Log 管理（Issue #302）

1. シーンごとに `biblio`（`frozen chunk` の `TinId` リスト）、`active chunk`、および `head` を管理する。
2. ログ追記時、`active chunk` にエントリを追加して `head` を進める。`active chunk` がチャンク上限に達した場合は、当該チャンクを `frozen chunk` として確定させてその `TinId` を `biblio` の末尾へ追加し、新しい空の `active chunk` を生成して `head` のチャンク内位置を `0` にリセットする。
3. サーバーから届く追記（`LogEventAppended`）・編集（`LogEventEdited`）・削除（`LogEventDeleted`）・巻き戻し（`LogEventRewound`）イベントを現在保持している `active chunk` / `frozen chunk` および `head` に適用し、購読者へ変更を通知する。`RewindLog` により過去のチャンクや位置へ巻き戻された場合は、切り詰められたチャンクとエントリを破棄して `head` を更新する。
4. クライアントからログ操作（追記・編集・削除・巻き戻し）を要求した際、オフラインやタイムアウト等でリクエストが失敗した場合は、当該リクエストを保留キュー（pending queue）へ追加し、呼び出し元へエラーと現在の保留キュー一覧を返す。
5. 起動時およびオンライン復帰時には、まずサーバーから最新の `biblio`・`active chunk`・`head` を取得してローカルのログ状態を同期し、その後に保留キュー内の未送信リクエストを FIFO 順で再送（flush）する。

### 3. Tin 管理（`get` / `put` と再エンコード、Issue #302）

#### `put`（取り込み・エンコード・アップロード）

1. 入力が画像または音声メディアの場合、Tin 化の前処理として画像を長辺最大 1920px の WebP に、音声を最大 128kbps の Opus に再エンコードする。
2. 平文（`label(8B LE) || payload`）を構築して圧縮・暗号化し、`tinId`（および収束暗号化時は `wrappedKey`）を算出する。
3. ローカルキャッシュへ保存し、該当 `tinId` の状態を確認する。すでに `"putted"` 状態であればサーバー通信を行わず no-op で完了する。
4. 未アップロードの場合は `cellar` の `/store` へ事前確認を行い、`upload_required` のときのみ `/store/{tinId}` へ本体を `PUT` する。成功（`deduplicated` / `already_owned` を含む）した時点で状態を `"putted"` に遷移させる。
5. オフラインや通信エラーでアップロードに失敗した場合は状態を `"put-failed"` として記録する。オンライン復帰時には `"put-failed"` の Tin を列挙して自動的に再送する。

#### `get`（取得・復号・展開）

1. `tinId` を 64 文字の 16 進文字列（32 バイト）へ正規化する。
2. `forceFetch` が指定されていなければローカルキャッシュを検索し、ヒットすれば即座に返す。
3. 同一 `tinId` に対する in-flight Promise が存在する場合はそれを共有して返す（重複リクエストの排除）。
4. `cellar` から `{ tinBytes, wrappedKey }` を取得する。
5. 部屋鍵を「呼び出し時の明示指定 → 静的設定鍵 → provider → URL フラグメント」の優先順で解決する。
6. `decodeTin` で復号・展開して整合性を検証する。
7. ローカルキャッシュへ保存し、in-flight 管理から除外して結果を返す。

### 4. `decodeTin` / `encodeTin` と鍵・Nonce・WrappedKey の整合性（Issue #298）

AES-256-GCM-SIV（`@noble/ciphers/aes.js`）は認証付き暗号であり、復号時に認証タグ（POLYVAL タグ）を検証する。暗号化時と異なる 32 バイト鍵・12 バイト Nonce・暗号文（または改ざん・破損したバイト列）を渡すと、`invalid polyval tag` エラーを送出して復号を拒否する。そのため、暗号化方式ごとに以下の鍵・Nonce 規約を厳密に守る必要がある。

1. **Opener の検証**: 先頭 1 バイトの Opener から暗号化方式と圧縮方式を読み取り、未知のビットパターンは拒否する。
2. **`Plaintext`（平文、`0111`）**: Nonce や鍵を持たない。本体を展開し、wire 全体の BLAKE3-256 ハッシュを `tinId` とする。
3. **`RoomKey`（部屋鍵暗号化、`0000`）**:
   - wire 形式は `opener(1B) || nonce(12B) || ciphertext`。
   - 暗号化時は、その部屋の 32 バイト `roomKey` とランダムに生成した 12 バイト `nonce` で平文（必要に応じて ISO/IEC 7816-4 形式の PADMÉ パディングを付与した圧縮済みデータ）を暗号化する。`tinId` は暗号化後の wire 全体の BLAKE3-256 ハッシュとなる。
   - 復号時は、wire から先頭 12 バイトの `nonce` と後続の `ciphertext` を分離し、**暗号化時と同一の `roomKey`** で復号する。復号後に PADMÉ パディングがあれば除去してから展開する。
   - **注意（部屋鍵の不一致防止）**: `RoomKey` 暗号化された Tin は暗号化時の `roomKey` に強く束縛される。別の部屋で作成したアセットの `tinId` を別部屋の `roomKey` で復号しようとした場合や、URL フラグメントの消失・取り違えにより異なる `roomKey` が渡された場合は `invalid polyval tag` となる。部屋を跨いで再利用するアセット（個人スコープの BGM・画像など）は、対象部屋の `roomKey` で再エンコード・再ラップするか、収束暗号化（`Convergent`）または復号済みキャッシュの再利用によって鍵の整合性を保証しなければならない。
4. **`Convergent`（収束暗号化、`0001`）**:
   - 画像・音声などの高エントロピーな大容量メディアで重複排除を行うための方式。
   - `opener(1B) || label(8B LE) || plaintext` を入力として BLAKE3 XOF（512bit）を計算し、前半 32 バイトを `tinId`、後半 32 バイトを収束鍵 `convergentKey` とする（`deriveConvergentIdAndKey`）。
   - 本体の暗号化には `convergentKey` と 12 バイトのゼロ埋め Nonce（`ZERO_NONCE`）を用いるため、wire 本体には Nonce を含めない（`opener(1B) || ciphertext`）。
   - `convergentKey`（32B）は、その部屋の `roomKey` とランダムな 12 バイト Nonce で AES-256-GCM-SIV によりラップし、60 バイトの `wrappedKey`（`wrapNonce(12B) || wrappedCiphertext(48B)`）として `cellar` の `/store` へ登録・`X-Alhalte-Wrapped-Key` ヘッダから取得する。
   - 復号時は、まず `wrappedKey` を当該部屋の `roomKey` でアンラップして `convergentKey` を取り出し、次に `convergentKey` と `ZERO_NONCE` で本体を復号・展開する。展開後の平文から BLAKE3 XOF（512bit）を再計算し、`tinId` と `convergentKey` が一致することを検証する。
5. `expectedTinId` が指定されている場合は算出した `tinId` と照合し、不一致なら拒否する。
6. 展開後の平文の先頭 8 バイト（リトルエンディアン `u64`）を `label`、残りを `payload` として返す。

## 既知の差分・制約

- 全体アーキテクチャ上は `alhalte-canner` の WASM 実装と責務が重なる部分があるが、ブラウザ側のキャッシュ・状態管理・暗号・圧縮展開はこの `@alhalte/pantry` パッケージの TypeScript 実装が担う。
- 単体の `HttpCellarClient` は `owner` / `session_id` を付与しない。認証が必要な経路では `maitre-d` の `/api/cellar/*` プロキシ（セッション Cookie から `owner` / `session_id` を補完）を経由する。
- `encodeTin` の Brotli 圧縮は `node:zlib` に依存するため、ブラウザ環境単体では LZ4 または無圧縮を使用する（展開はブラウザでも全方式対応）。
- `CacheStoragePantryCacheAdapter` は内部擬似 URL `https://alhalte.internal/tin/{tinId}` をキャッシュキーとして使用する（実際のネットワークリクエストは発生しない）。
- `removeIso7816Padding` は末尾の `0x80` デリミタを探索してパディングを除去する。任意のバイナリペイロードが偶然 `0x80` で終わる場合の誤除去を防ぐため、`stripPadme` の適用は `RoomKey` 暗号化された低エントロピー Tin に限定する。
