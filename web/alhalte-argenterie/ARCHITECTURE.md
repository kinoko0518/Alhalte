# alhalte-argenterie

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `web/alhalte-argenterie`（package `@alhalte/argenterie`）  
> テスト: `web/alhalte-argenterie/tests/*.test.ts`

## 責務

公式 cutlery。Svelte 5 と Noph UI で部屋 UI を提供する。部屋への入室、scene / character / log の表示と操作、拡張機能の管理と実行、Saignant WASM によるログ描画、接続状態とオフライン通知を担う。画面は Material 3 Expressive に準拠する。

## 公開 API / 外部接続点

### HTTP

| 接続先 | 内容 |
| --- | --- |
| `maitre-d` `/api/rooms/{room_id}/permissions` | 部屋権限の取得・設定・削除（`utils/session.ts`）。 |
| `maitre-d` `/api/accounts/search` | account 検索。 |
| `maitre-d` `/api/account/{account_id}` | account 取得・更新。アカウントメニューの表示名・アイコン取得に使う（`utils/account.ts`）。 |
| `maitre-d` `/auth/google/url` | Google 認可 URL の取得（`utils/account.ts`）。 |
| `maitre-d` `/auth/google/callback` | 認可コードを session に交換（POST、`utils/account.ts`）。 |
| `maitre-d` `/api/room/{room_id}/export/zip?type=full|logs` | ZIP ダウンロード（`CommonHeader.svelte`）。 |
| `maitre-d` `/api/cellar/store`、`/api/cellar/store/{tin_id}`、`/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。アセット・拡張 wasm の Tin を XHR で送受信する（`utils/cellarTransfer.ts`）。進捗は `upload.onprogress`、失敗は型付き `CellarTransferError`。 |
| Vite dev proxy | `/api`、`/room` を `http://127.0.0.1:8080`、`/ws` を `ws://127.0.0.1:8080` へ転送。 |

### WebSocket

`utils/roomSocket.ts` の `RoomSocket` が `/ws/room/{roomId}?account_id=...&session_id=...` へ接続する。`ClientFrame` を JSON で送り、`ServerFrame` を JSON で受ける。heartbeat ping には自動で `HeartbeatResponse` を返す。未接続時の送信はキューに積み、再接続後に flush する。

- **Tin エンコード**: banquette が `Tin` を期待する frame（`AppendLog.entry`、character の `data`）は、`jsonToTin` / `logRecordToTin` / `characterToTinMap` で Tin JSON `{ label, payload }` に変換してから送る。label は pantry の `labelHash` で用途別定数（`LABEL_LOG_RECORD`、`LABEL_JSON`、`LABEL_BANQUETTE_LOG_CHANGE_BGM` など）を使う。受信側は `tinToJson` / `tinToLogRecord` / `tinMapToCharacter` で戻す。
- **Promise API と ack**: mutate 用の `request*` と query 用の `query*` が Promise を返す。既存の void API（`appendLog`、`updateState`、`createCharacter` など）は残している。
  - `AppendLog` は payload に `clientOpId` を埋め、同じ id を持つ `LogEventAppended` だけで resolve する（他クライアントの echo では resolve しない）。
  - `UpdateState` / `DeleteState` は `scene_id` + `key`（更新は値も比較）で照合する。
  - Character CRUD は `CharacterCreated` / `CharacterUpdated` / `CharacterDeleted` を FIFO で照合する（UI は同時操作を直列化する前提）。
  - `CreateScene` / `DeleteScene` / `MoveCharacter` は新設の `SceneCreated` / `SceneDeleted` / `CharacterMoved` で resolve する。
  - `RewindLog` は `LogEventRewound`、`ChangeBgm` は `LogEventAppended` で resolve する。
  - `Error` は対応する `*Failed` code と `PermissionDenied` で reject する。
  - 応答が無い場合は `DEFAULT_OPERATION_TIMEOUT_MS = 8000` で `OperationTimeoutError`（`code: 'timeout'`）にし、pending を必ず破棄する。サーバー由来の失敗は `ServerOperationError`（`code` 付き）で返す。
- 未処理だった `StateSnapshot` / `LogBiblioSnapshot` / `LogChunkSnapshot` / `CharacterInfo` / `LogEventRewound` / `SceneCreated` / `SceneDeleted` / `CharacterMoved` も dispatch し、`onServerFrame` と個別 callback の両方へ流す。

### WASM

- `utils/saignant.ts`: `/wasm/alhalte_saignant.wasm` を読み、`render_generast_abi` と `get_full_css_abi` を呼ぶ。Vitest では `node:fs` で実ファイルを読む。
- `wasm/runner.ts`: 拡張機能 wasm の `alhalte_abi_version`、`alloc`、`dealloc`、`process_generast`、任意の `self_introduction` を呼ぶ。`ExtensionPipeline` は有効な拡張機能を priority 昇順に適用する。

### 主な画面・コンポーネント

| 領域 | コンポーネント |
| --- | --- |
| アプリ全体 | `App.svelte` |
| ホーム | `Home/HomeScreen.svelte`、`utils/home.ts`、`utils/recentRooms.ts` |
| フィールド | `Field/FieldPanel.svelte`、`Field/FieldPiece.svelte`、`lib/components/BoardCanvas.svelte` |
| コントロール | `Control/ControlPanel.svelte`、`CommonHeader.svelte`、`CommonFooter.svelte` |
| タブ | `LogTab`、`SceneGraphTab`、`SceneManagementTab`、`CharacterManagementTab`、`ExtensionManagementTab` |
| モーダル | `AssetPickerModal`、`CharacterEditModal`、`RoomPermissionModal`、`GenericImagePicker`、`IconImagePicker` |
| アカウント | `Control/AccountMenu.svelte`、`utils/account.ts` |
| 門前払い | `Gate/GateScreen.svelte` |
| 接続表示 | `OfflineIndicator.svelte`、`utils/connection.ts` |

横長画面ではフィールド左・コントロール右、縦長画面ではフィールド全体＋ボトムシート。コントロールパネルはサイドシート / ボトムシートの 2 形態と開閉状態を持つ。

## データ形式・状態

### 主要な型

- `VisitedRoom` / `RecentRoom`
- `Character`（`statuses`、`parameters`、`chatPalette`、`hideStatus`、`isNarrative`、`placedAsPiece`、`piecePosition`）
- `Scene`（`juices`、`viewTransform`、`members`）
- `LogRecord`（`content`、`speaker`、`speakerId`、`timestamp`、`chunkId`）
- `ExtensionItem`（`wasmBytes`、`priority`、`enabled`、`cellarTinId`）

### ローカル保存

- `localStorage`: account ID、session ID、訪問済み部屋、最近の部屋、アセット（`alhalte_pantry_assets`）、部屋ログのキャッシュ。
- IndexedDB `alhalte-argenterie-outbox`（store `outbox`）: 送信できなかった message / character（拡張用に state / asset も型として許容）を `{ id, roomId, kind, createdAt, payload }` で保存する。`createdAt` から `OUTBOX_TTL_MS = 30 日` を超えたエントリは読み出し時に prune する。IndexedDB が無い環境ではメモリ実装へフォールバックする。
- Cookie: `session_id`（`SameSite=Lax`）。
- `sessionStorage`: OAuth の `state`、認可後の戻り先 URL（クエリと部屋鍵フラグメントを含む）、OAuth の `redirect_uri`。サインイン中だけ保持し、完了時に削除する。
- URL フラグメント: 部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を作る。

## 内部フロー

### 入室

1. `main.ts` が `App` を mount する。開発エントリは `isAdmin: true` を渡す。
2. `App.svelte` は `resolveRoomFromLocation` で URL から部屋を解決する。
3. `RoomSocket` を作り、account / session をクエリに付けて接続する。
4. `ServerFrame` の `SceneList`、`CharacterList`、`StateUpdated`、`StateDeleted`、`LogEventAppended`、`SceneCreated` / `SceneDeleted` / `CharacterMoved` などを Svelte の state に反映する。
5. ログタブは入室時と scene 切り替え時に最新 chunk を読む（後述）。403 や「全体公開されていない部屋です」を受けたら `isForbidden` を立て、`GateScreen` を出す。

### 門前払い画面

`GateScreen` はフィールドやコントロールパネルの代わりに全画面で表示する。

- 中央大文字: 「アカウントに入室権限がありません」
- 説明: 「部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？」
- 主操作: 「サインイン・アカウント切り替え」。サインイン済みならローカル session を破棄してから `prompt=select_account` 付きの Google 認可 URL を開く。
- 右上: `AccountMenu`。サインイン・アカウント切り替え・サインアウトを行える。
- 副操作: 「ホームに戻る」（`isForbidden` を解除して `/` へ戻る）。

### アカウントUI / Googleサインイン

1. `AccountMenu` は account ID と session ID の両方が保存されている場合だけサインイン済みとみなす。
2. サインイン済みなら `GET /api/account/{account_id}` で表示名とアイコン Tin ID を取得し、32 byte の Tin ID から `/api/cellar/tin/<hex>` を組み立ててアバターにする。取得中・取得失敗時はイニシャルまたは `account_circle` を表示する。
3. アバター押下で M3 Menu を開き、「別のアカウントでログイン」「サインアウト」を出す。サインアウトは localStorage / Cookie / OAuth 用 sessionStorage を消す。部屋鍵フラグメントは保持する。
4. 未サインインなら Filled tonal の「サインイン」を出す。
5. サインインは `GET /auth/google/url` で認可 URL を取得し、現在のページ（クエリ・フラグメントを除く URL）へ戻るようリダイレクトする。Google へ遷移する前に、戻り先 URL（部屋鍵を含む）・`state`・`redirect_uri` を sessionStorage に保存する。
6. アプリ起動時（`main.ts`）に URL の `code` / `state` を検出し、`POST /auth/google/callback` で session に交換する。成功後は account / session を保存し、sessionStorage の戻り先 URL を `history.replaceState` で復元してから `App` を mount する。
7. 配置はホーム画面の AppBar trailing、フィールド画面の右上、門前払い画面の右上。ホーム画面のテーマ設定 UI は削除し、テーマ選択はコントロールパネル共通ヘッダーの gear menu に集約する。

### 発言とログ表示

1. `handleSendMessage` は `LogRecord` を作るが、この時点では `ChunkLogManager` に追加しない。`isSending` を立てて `requestAppendLog` を await する（`CommonFooter` は送信ボタンを波線 `CircularProgress` に差し替え、フッターを `aria-busy` で暗転し、二重送信を防ぐ）。
2. 成功時はサーバーの `LogEventAppended` echo を待つ。`RoomSocket` は echo を `ChunkLogManager` に追加してから購読者へ通知するため、`App` 側でローカル追加しない（二重追加しない）。`clientOpId` による照合で他クライアントの echo とは区別する。
3. タイムアウト / 通信エラー時は `kind: 'message'` として IndexedDB outbox に積み、フッター上部の波線 `OfflineQueueNotice`（`kind="message"`）に件数を出す。入力欄へは復元しない。再接続で FIFO flush し、各メッセージは echo 後にのみ描画する。オフライン時は `RoomSocket` のメモリキューを使わず outbox へ直接積む（二重送信を避ける）。
4. ログタブは `ChunkLogManager` の変更を購読し、未描画の record を処理する。`buildGenerastFromText` が有効な拡張機能を priority 昇順に適用し、`Generast` Tin を作る。
5. `renderGenerast` が Saignant WASM で HTML にし、`{@html}` で表示する。WASM 未初期化時は HTML エスケープしたテキストを返す。

### 最新 chunk の読み込み

- `LogTab` は表示時と scene 切り替え時に `queryLogBiblio(sceneId)` → biblio の head から最新 `chunk_index` を得る → `queryLogChunk(sceneId, chunkIndex)` の順で要求する。scene ごとに in-flight / loaded を管理し、二重ロードしない。
- 読み込み中はメッセージ一覧上に contained `LoadingIndicator` を出し、「ログはありません」は出さない。応答後は `ChunkLogManager` に snapshot を取り込み、`id` / `(chunkIndex, position)` で dedupe する。
- タイムアウト / 通信エラー時は `OfflineQueueNotice kind="generic"`（「接続が回復したらログを再読み込みします」）を出し、再接続時に再試行する。既存の遅延読み込み（スクロール端）と Saignant 描画は維持する。

### BGM

- BGM は state とログの両方に書く。state キーは `std::bgm`（slot 1、旧来の互換）、`std::bgm2` / `std::bgm3`（slot 2/3）、音量は `bgm::<slot>::volume`。値は 64 hex の Tin ID（停止・未設定は null / 削除）。
- `SceneSettingsTab` は再生/停止・選択・音量変更を async 化し、`requestUpdateState(sceneId, key, tinIdOrNull)` と `requestChangeBgm(sceneId, slot, tinIdOrNull)` の両方が成功したときだけローカルの `bgmSlots` / `scene.juices` を確定する。送信中は対象 slot の「BGM選択」アイコンを波線 `CircularProgress` にし、二重実行を防ぐ。
- 失敗時は変更前へ戻し、slot 付近にエラーを出す。キューには積まない（最終状態のみが意味を持つため、再接続時の自動再送をしない）。
- `App` は `StateUpdated` / `StateDeleted` の BGM キーを `bgmSlots` と旧 `bgm1..3` へ反映する。ルート scene `/` の state 更新は管理者のみで、`PermissionDenied` はそのまま表示する。

### scene の管理操作

- 「シーンを新規作成」「ログを削除」「シーンを削除」「メンバー移動」は `requestCreateScene` / `requestRewindLog(sceneId, 0, 0)` / `requestDeleteScene` / `requestMoveCharacter` で送り、成功 ack 後にだけローカル state を更新する（楽観更新しない）。
- ログ削除は `RewindLog(0, 0)` で全ログを切り詰め、`LogEventRewound` 後にローカルの chunk をクリアする。
- 送信中は押したアイコンを波線 `CircularProgress` にし、当該操作を二重実行できない。失敗時は管理領域の下に中央寄せで「サーバーと通信できませんでした」を出す。
- 接続中は ack 後にだけローカル state を更新し、失敗時は管理領域の下に中央寄せで「サーバーと通信できませんでした」を出す。`roomSocket` が無い / 未接続の場合は従来のローカル専用動作（一覧・現在 scene・chunk・members をローカルで更新するだけで、サーバーへは送らず、キューにも積まず、エラー表示もしない）。`SceneList` broadcast は scene 一覧の権威として反映し、`SceneDeleted` は第二の防護として該当 scene を除く。

### キャラクター同期

- キャラクタータブは表示時に `queryListCharacters` を発行し、問い合わせ中は contained `LoadingIndicator` を出す。タイムアウト時は `OfflineQueueNotice kind="character"` を出し、キャッシュ済み一覧は保持する。
- 保存は ack 後にだけ反映する。作成は `CharacterCreated` のサーバー採番 ID を使う。保存中は編集パネルを `aria-busy` + 非活性にし、保存アイコンを波線 `CircularProgress` にする。成功時のみ閉じる。
- タイムアウト時は `kind: 'character'` として outbox に積み、再接続 flush で送信して成功後に一覧へ反映する。`PermissionDenied` はキューせずパネルを開いたままエラー表示する。削除も ack 後に行う。

### 拡張機能

- メタデータは `/` scene の state key `extension::<id>` に `{ id, name, description, icon, priority, enabled, tinId }` の JSON Tin で保存する。wasm 本体は cellar に RoomKey 暗号化で置き、`tinId` だけを state に持つ（4KiB 制限を守る）。
- タブ表示時に `queryState('/')` を発行し、contained `LoadingIndicator` を出す。マージ規則は、サーバーが列挙した id についてはサーバーのメタデータを正とし、サーバーに無い公式既定 id はローカルの wasm ごと保持し、サーバーに無いカスタム id は削除する。同じ id でサーバーの `tinId` が変わった場合だけ wasm を取り直し、それ以外はローカルの `wasmBytes` を保持する。手元に wasm が無い場合は `tinId` から cellar で取得する。
- toggle / delete / reorder / upload は楽観更新せず、`requestUpdateState` / `requestDeleteState` の成功後にだけローカルへ反映する。送信中は該当行のアイコンを波線 `CircularProgress` にする。
- タイムアウト / エラー時はタブ上部に「サーバーと通信できませんでした」を出し、管理者でもタブ全体を暗転・`aria-disabled`・`pointer-events: none` で操作不可にする。再接続で `queryState('/')` を再試行する。
- `.wasm` は検証後に `encodeRoomTin` + `uploadTin` で cellar へ保存し、進捗を表示して完了後に state を更新する。

### アセット

- `utils/assets.ts` は画像・音声を `FileReader` で data URL にし、localStorage と in-memory の Pantry キャッシュアダプタへ保存する。`AssetPickerModal` は personal / room、image / audio で絞り込み、NONE カードと削除を提供する。
- アップロードは `encodeRoomTin`（RoomKey 暗号化 + LZ4）→ `uploadTin` で cellar へ送る。進捗は実 XHR の `upload.onprogress` のみを使い、カード内に波線 `LinearProgress` を出す。成功時は `asset.id` に `tinId` を付け、room scope はさらに `/` の state key `asset::<id>` にメタデータ（id, name, mediaType, mimeType, size, tinId, createdAt, ownerAccountId）を `requestUpdateState` で保存する。失敗時はカード内に「アップロードに失敗しました」と再試行ボタンを出す。
- personal のメタデータは localStorage のみに置き、`tinId` を他者へ公開しない。本体は cellar に置く。
- モーダルを開くたびに `syncRoomAssets` が `queryState('/')` で `asset::*` を取得してローカルへマージし、欠けている tin は `downloadTin` で取得する。同期失敗時は静かに既存キャッシュを表示する。アセットは 30 日 TTL の対象外（永続）。
- `App` は `AssetPickerModal` に `roomSocket` と解決済みの部屋鍵を渡す。

### 権限 UI

`utils/session.ts` が account 検索・部屋権限 CRUD を呼ぶ。`RoomPermissionModal` は M3 Search、Divider、Connected Button Group で viewer / PL / GM を切り替える。viewer の場合はフッターの送信・ダイス・scene 変更などを無効化する。入室拒否時は `GateScreen` を出し、右上の `AccountMenu` からサインイン・アカウント切り替え・サインアウトを行える。

## 既知の差分・制約

- 送信時の `extensionPipeline.executePipeline` は結果を使わない。実質的な拡張機能適用はログ表示時（`LogTab.updateRenderedHtml`）に行われる。
- アセットは cellar へ実アップロードするが、`personal` のメタデータは localStorage のみで、他クライアントとは共有されない。アセット本体は RoomKey で暗号化して cellar に置くが、maitre-d proxy と cellar は `Bytes` でリクエストを全量メモリにバッファする（64 MiB 上限）。大きなファイルほどメモリを消費する。
- アウトボックス flush は at-least-once。ack を受け取れずにタイムアウトした操作は再送されるため、サーバー側で適用済みの場合は重複する可能性がある（冪等キーは持たない）。
- 拡張機能タブは開いた時点の `queryState('/')` を正とし、開いている間の `StateUpdated` broadcast は反映しない（再接続・再表示で取り直す）。
- 公式既定拡張の削除はサーバーに `extension::<id>` の削除を送るが、サーバーが列挙しない公式既定 id は常にローカルの wasm ごと保持されるため再表示される。削除の永続性は保証されない。
- タイムアウトは 8 秒で、権限問い合わせのようにサーバーが黙って破棄する操作では唯一の失敗シグナルになる。`PermissionDenied` が返らない経路はタイムアウトとして扱う。
- Saignant WASM は character resolver を受け取らないため、mention は `Anonymous` になる。
- `main.ts` の `isAdmin: true` は開発用エントリの既定値。実運用では account / 権限から決定する必要がある。
- 部屋鍵は URL フラグメントに置く。サーバーへ送らない設計だが、ブラウザ履歴や共有 URL には残る。
- ホーム画面のテーマ設定 UI は Issue #116 で削除した。現在のテーマ選択はコントロールパネル共通ヘッダーの gear menu にあるパレットのみで、light / dark / system の切替 UI は持たない。
- OAuth の `redirect_uri` は現在のパス（クエリ・フラグメントを除く）を使う。Google Cloud Console 側でこの URL を登録する必要がある。`state` は sessionStorage に保存した値と照合し、一致しない交換は行わない。
- `AccountMenu` は account ごとに `GET /api/account/{id}` を呼ぶ。プロフィール取得に失敗してもサインイン状態は維持し、イニシャル表示にフォールバックする。
- 現行 main では WebSocket の HTTP 403 status をブラウザから直接読めないため、`isForbidden` は権限一覧 API の結果と `Error` frame の文字列から立てている。Issue #114 の権限モデル移行後は、banquette / maitre-d の正式な拒否通知に接続する必要がある。このため、`room_permissions` に行があり `done` が true の部屋でも、権限一覧に自分がいないだけで門前払い画面になる可能性がある。
