# alhalte-argenterie

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `web/alhalte-argenterie`（package `@alhalte/argenterie`）  
> テスト: `web/alhalte-argenterie/tests/*.test.ts`

## 責務

公式 cutlery。Svelte 5 と Noph UI で部屋 UI を提供する。部屋への入室、scene / character / log の表示と操作、拡張機能の管理と実行、Saignant WASM によるログ描画、オフライン時の操作キュー通知を担う。画面は Material 3 Expressive に準拠する。グローバルな接続状態バナーは持たず、通信失敗は操作したタブ / エディタ内で告知する（Issue #261）。

## 公開 API / 外部接続点

### HTTP

| 接続先 | 内容 |
| --- | --- |
| `maitre-d` `POST /api/rooms` | 認証必須。`{ name, room_id? }` で部屋を登録し、作成者を `admin` にする（`utils/home.ts` の `createRoom`）。`room_id` はクライアント生成の 16 hex を送ってもよいが、返却値が正。 |
| `maitre-d` `GET /api/rooms/{room_id}` | 入室 preflight。部屋名と呼び出し元の `role`（`admin` / `participant` / `viewer` / `denied`）を返す（`utils/home.ts` の `fetchRoomInfo`）。`denied` は 403 で返るため、403 は `GateScreen`、404 は not-found として事前に判別する。旧 `done` フラグは #200 で廃止され、返ってきても UI は使わない。 |
| `maitre-d` `/api/accounts/search` | account 検索。 |
| `maitre-d` `/api/account/{account_id}` | account 取得・更新。アカウントメニューの表示名・アイコン取得に使う（`utils/account.ts`）。 |
| `maitre-d` `GET /auth/login?return_to=...&prompt=...` | Google サインインの入口。絶対 URL の `return_to`（fragment を除く）へ遷移する（`utils/account.ts`）。state と `redirect_uri` は maitre-d が持つ。 |
| `maitre-d` `GET /auth/google/callback` | Google からの固定 callback。ブラウザが直接踏む。maitre-d が `session_id` Cookie を設定して `return_to` へ 302 する。アプリは関与しない。 |
| `maitre-d` `GET /api/session` | OAuth 後に Cookie から `account`（`AccountResponse`）と `session_id` を取得する（`utils/account.ts`）。 |
| `maitre-d` `/api/room/{room_id}/export/zip?type=full|logs` | ZIP ダウンロード（`CommonHeader.svelte`）。 |
| `maitre-d` `/api/cellar/store`、`/api/cellar/store/{tin_id}`、`/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。アセット・拡張 wasm の Tin を XHR で送受信する（`utils/cellarTransfer.ts`）。store check は `200 OK` + body の `result`（`upload_required` / `deduplicated` / `already_owned`）で判定し、`upload_required` のときだけ本体を PUT する。handshake body は `{ tin_id, size, wrapped_key }` のみで、`owner` / `session_id` は送らない（maitre-d が session から付与する。送ると 400）。進捗は `upload.onprogress`、失敗は型付き `CellarTransferError`。 |
| Vite dev proxy | `/api`、`/auth`、`/room` を `http://127.0.0.1:8080`、`/ws` を `ws://127.0.0.1:8080` へ転送。`/auth` はサインイン開始（`/auth/login`）と Google callback の両方に必要で、maitre-d の `--public-url` がこの dev origin を指すことで Cookie が UI と同一 origin に付く。 |

### WebSocket

`utils/roomSocket.ts` の `RoomSocket` が `/ws/room/{roomId}?account_id=...&session_id=...` へ接続する。`ClientFrame` を JSON で送り、`ServerFrame` を JSON で受ける。heartbeat ping には自動で `HeartbeatResponse` を返す。未接続時の送信はキューに積み、再接続後に flush する。

- **URL 構築**: `buildRoomWebSocketUrl` は招待ページ URL（`https://host/rooms/<id>#<key>`）からは **origin だけ** を使い、`ws(s)://host/ws/room/<id>` を組み立てる。`/rooms/<id>` などのパスを WebSocket URL に引き継がない。明示的な `ws(s)://.../ws/room/...` だけをそのまま使い、それ以外の `ws(s)` URL も origin から作り直す。
- **room ID 検証**: `utils/home.ts` の `parseRoomUrl` は 16 hex（または移行用 decimal）だけを受理し、`room-1` のような任意文字列は `null` で拒否する。`generateRoomUrl` は 16 hex を作る。
- **close 通知と再接続**: `RoomSocket` は `onclose(event)` を `onClose` callback と `getLastCloseEvent()` で公開する。`1008` / `4403` / `4404` は再試行不能として自動再接続を止め、`App` はこれらを `denied`（`GateScreen`）に反映する。入室前の HTTP 403（`denied`） / 404 は `GET /api/rooms/{id}` の preflight で判別する。
- **権限 frame (Issue #201)**: 権限表は HTTP ではなく WS で扱う。`ClientFrame::QueryPermissions`（`PermissionsSnapshot` が返る）、`SetMemberPermission { account_id, permission }` と `SetDefaultPermission { permission }`（成功で `PermissionsUpdated`、失敗で `SetMemberPermissionFailed` / `SetDefaultPermissionFailed` の `Error`）を `requestQueryPermissions` / `requestSetMemberPermission` / `requestSetDefaultPermission` で送る。level は banquette の Serde 表記（`Admin` / `Participant` / `Viewer` / `Denied`）で送受信し、UI の role（小文字）とは `roleToPermissionLevel` / `permissionLevelToRole` で変換する。ack は要求元にしか届かず、部屋全体へは broadcast されない。

- **Tin エンコード**: banquette が `Tin` を期待する frame（`AppendLog.entry`、character の `data`）は、`jsonToTin` / `logRecordToTin` / `characterToTinMap` で Tin JSON `{ label, payload }` に変換してから送る。label は pantry の `labelHash` で用途別定数（`LABEL_LOG_RECORD`、`LABEL_JSON` など）を使う。受信側は `tinToJson` / `tinToLogRecord` / `tinMapToCharacter` で戻す。`CharacterList` / `CharacterInfo` も `tinMapToCharacter` で全属性（statuses / parameters / chatPalette / icon / color / piecePosition など）を decode する。
- **Promise API と ack**: mutate 用の `request*` と query 用の `query*` が Promise を返す。既存の void API（`appendLog`、`updateState`、`createCharacter` など）は残している。
  - `AppendLog` は payload に `clientOpId` を埋め、同じ id を持つ `LogEventAppended` だけで resolve する（他クライアントの echo では resolve しない）。
  - `UpdateState` / `DeleteState` は `scene_id` + `key`（更新は値も比較）で照合する。
  - Character CRUD は `CharacterCreated` / `CharacterUpdated` / `CharacterDeleted` を FIFO で照合する（UI は同時操作を直列化する前提）。
  - `CreateScene` / `DeleteScene` / `MoveCharacter` は新設の `SceneCreated` / `SceneDeleted` / `CharacterMoved` で resolve する。
  - `RewindLog` は `LogEventRewound` で resolve する。
  - `Error` は対応する `*Failed` code と `PermissionDenied` で reject する。`SpeakerNotOwned`（話者の所有権違反、Issue #271）は再試行不能として扱い、outbox に積まない。
  - 応答が無い場合は `DEFAULT_OPERATION_TIMEOUT_MS = 8000` で `OperationTimeoutError`（`code: 'timeout'`）にし、pending を必ず破棄する。サーバー由来の失敗は `ServerOperationError`（`code` 付き）で返す。
- 未処理だった `StateSnapshot` / `LogBiblioSnapshot` / `LogChunkSnapshot` / `CharacterInfo` / `LogEventRewound` / `SceneCreated` / `SceneDeleted` / `CharacterMoved` も dispatch し、`onServerFrame` と個別 callback の両方へ流す。
- **SceneList / SceneInfo**: `SceneList.scenes` は `SceneSummary`（`id`、`name`、`present_character_count`、`present_characters`、`active_client_count`、`is_loaded`）の列で、旧サーバーの bare id 列も受理する。`present_characters` は scene 在籍 character id の昇順リストで、`App` はこれで `scene.members` を再構築する（reload / reconnect 後も復元）。`RoomSocket` は id 列を `getScenes()`、summary を `getSceneSummaries()` に保持し、`onSceneInfo` で create / rename / membership の broadcast を通知する。
- **StateSnapshot hydration**: `EnterScene` の成功応答として banquette が `StateSnapshot` を返す（access 可能な場合）。`App` は `onStateSnapshot`（real socket）と subscribe（custom / test socket）の両経路で受け、`std::name` / `std::bgm<N>` / `bgm::<slot>::playing` / `bgm::<slot>::volume` / `bgm::<slot>::started_at` / `bgm::<slot>::paused_at` / `bgm::<slot>::removed` / `std::b-img*` / `view::transform` / `piece::<id>::*` を冪等に適用する。BGM は removed → track → timestamps → playing / volume の順に適用する。再接続・scene 切替でも同じ snapshot が再適用される。

### WASM

- `utils/saignant.ts`: `/wasm/alhalte_saignant.wasm` を読み、`render_generast_abi_v2`（存在すれば）または `render_generast_abi` と `get_full_css_abi` を呼ぶ。現在の `characters` から `{ "<id>": "<name>" }` を作って v2 に渡し、mention / `fluct` / `assign` の名前を解決する。v2 をエクスポートしない wasm では v1 にフォールバックし、名前は渡さない。render cache key は characters signature を含み、`CharacterList` / `CharacterInfo` 更新時は `LogTab` が cache と `renderedContentMap` を破棄する。Vitest では `node:fs` で実ファイルを読む。
- `wasm/runner.ts`: 拡張機能 wasm の `alhalte_abi_version`（1 または 2）、`alloc`、`dealloc`、`process_generast`、任意の `self_introduction` を呼ぶ。ABI 1 は tuple3（id なし）、ABI 2 は tuple4 `(id, name, icon_svg, description)` を decode する。ABI 2 の id が manifest id と一致しない wasm は `wasmMissing: true` にする。`ExtensionPipeline` は有効な拡張機能を priority 昇順に適用する。
- 開発時のビルド / 配置は `nix run .#saignant-wasm`（または `nix develop -c ./scripts/build-saignant-wasm.sh`）。`public/wasm/alhalte_saignant.wasm` は commit 対象で、Rust の `abi::ABI_VERSION` と一致することを `crates/alhalte-saignant/tests/artifact_test.rs` が検証する。

### 主な画面・コンポーネント

| 領域 | コンポーネント |
| --- | --- |
| アプリ全体 | `App.svelte` |
| ホーム | `Home/HomeScreen.svelte`、`utils/home.ts`、`utils/recentRooms.ts` |
| フィールド | `Field/FieldPanel.svelte`、`Field/FieldPiece.svelte`、`lib/components/BoardCanvas.svelte` |
| コントロール | `Control/ControlPanel.svelte`、`CommonHeader.svelte`、`CommonFooter.svelte`、`SceneSwitcher.svelte` |
| タブ | `LogTab`、`SceneGraphTab`（グラフ / リスト）、`SceneManagementTab`、`CharacterManagementTab`、`ExtensionManagementTab` |
| モーダル | `AssetPickerModal`、`CharacterEditModal`、`SceneMoveModal`、`RoomPermissionModal`、`GenericImagePicker`、`IconImagePicker` |
| アカウント | `Control/AccountMenu.svelte`、`utils/account.ts` |
| 門前払い | `Gate/GateScreen.svelte` |
| 接続状態 | `utils/connection.ts`（グローバルバナーは持たない） |

横長画面ではフィールド左・コントロール右、縦長画面ではフィールド全体＋ボトムシート。コントロールパネルはサイドシート / ボトムシートの 2 形態と開閉状態を持つ。縦長のボトムシートはドラッグハンドルと半開状態を持たず、開くと常に全開（ビューポートから noph-ui の上部マージンだけ引いた高さ）になる（Issue #262）。

## データ形式・状態

### 主要な型

- `VisitedRoom` / `RecentRoom`
- `Character`（`statuses`、`parameters`、`chatPalette`、`hideStatus`、`isNarrative`、`placedAsPiece`、`piecePosition`）
- `Scene`（`juices`、`viewTransform`、`members`）
- `LogRecord`（`content`、`speaker`、`speakerId`、`timestamp`、`chunkId`、任意の `chunkIndex` / `position` / `contentTin` / `kind` / `event` / `cached`）
- `ExtensionItem`（`wasmBytes`、`priority`、`enabled`、`cellarTinId`）

### ローカル保存

- `localStorage`: account ID、session ID、訪問済み部屋、最近の部屋、アセット（`alhalte_pantry_assets`）、部屋ログのキャッシュ（`alhalte:room-logs:<roomId>`）、テーマ（`alhalte:theme`）、共通フッターのダイスピッカー表示設定（`alhalte:dice-picker-visible`、`'true'` / `'false'`、既定は表示。読み書きの失敗・不正値は表示として扱う）。部屋ログは `kind` / `event` / `contentTin` を含めて保存し、復元時は cache-origin として扱う。オフライン表示専用で、サーバーの biblio / chunk snapshot が届いた scene はキャッシュではなくサーバーを正とする。
- IndexedDB `alhalte-argenterie-outbox`（store `outbox`）: 送信できなかった message / character（拡張用に state / asset も型として許容）を `{ id, roomId, kind, createdAt, payload }` で保存する。`createdAt` から `OUTBOX_TTL_MS = 30 日` を超えたエントリは読み出し時に prune する。IndexedDB が無い環境ではメモリ実装へフォールバックする。
- Cookie: `session_id`（`SameSite=Lax`）。
- `sessionStorage`: サインイン前の完全な戻り先 URL（クエリと部屋鍵フラグメントを含む）。サインイン中だけ保持し、`/api/session` 完了時に削除する。OAuth の `state` と `redirect_uri` は maitre-d が保持するためアプリは持たない。
- URL フラグメント: 部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を作る。

## 内部フロー

### 入室

1. `main.ts` が `App` を mount する。管理者は `isAdmin` ではなくサーバーの `role` から決める。
2. `App.svelte` は `resolveRoomFromLocation` で URL から部屋を解決する。
3. 入室前に `GET /api/rooms/{room_id}` を preflight する。成功時は返却された `name` を `currentRoom.name` に、`role`（4 段階）を `userRole` に反映してから `RoomSocket` を作る。403（`denied`） / 401 は `GateScreen`、404 は not-found 画面、ネットワーク / 5xx は 2 秒間隔・最大 5 回の再試行後に再試行 UI を出す。403 / 404 は再試行しない。
4. `RoomSocket` を作り、account / session をクエリに付けて接続する。preflight が `ok` になるまで WebSocket は開かない。`initialRoom` prop はテスト / プレビュー用の明示 override で、preflight と `userRole` の上書きをスキップする。
5. `ServerFrame` の `SceneList`（`SceneSummary` の列）、`SceneInfo`、`CharacterList`、`CharacterInfo`、`StateSnapshot`、`StateUpdated`、`StateDeleted`、`LogEventAppended`、`SceneCreated` / `SceneDeleted` / `CharacterMoved` / `CharacterDeleted` などを Svelte の state に反映する。`SceneList` / `StateSnapshot` がシーン名・BGM・背景・view・駒の権威で、`SceneList` / `SceneInfo` の `present_characters` が `scene.members` の権威、`CharacterInfo` / `CharacterDeleted` / `CharacterMoved` が character 一覧の権威。`SceneInfo` は create / rename / membership を他クライアントへ即時反映する。
6. ログタブは入室時と scene 切り替え時に最新 chunk を読む（後述）。WebSocket の `Error` frame や close code（`1008` / `4403` / `4404`）も `denied`（`isForbidden`）のフォールバックとして `GateScreen` を出す。

`RoomRole` は `admin` / `participant` / `viewer` / `denied` の 4 段階で、`utils/session.ts` の `roomRoleCapabilities` が `isAdmin` / `isParticipant` / `isViewer` / `isDenied` と `canSend` / `canManageScenes` / `canManageCharacters` / `canManageExtensions` / `canManageRoom` を導出する。`isAdmin` は `effectiveRole === "admin"`、`isViewer` は `viewer` のみで、participant を viewer 扱いしない。`isAdmin` prop はテスト / プレビュー用の明示 override に限定し、role が未確定（preflight 前）のときだけ participant 相当として扱う。`VisitedRoom.isAdmin` は localStorage 互換のために保存するだけで、権限判定には使わない。

### 部屋作成

1. ホーム画面の「部屋の新規作成」は、表示用に 16 hex の `room_id` と 32 byte の部屋鍵をクライアントで生成する。
2. 「入室する」で `POST /api/rooms` に `{ name, room_id }` を送る。account / session が無い場合は送信せず、ダイアログ内にサインイン導線を出す。
3. 成功時だけ、返却された `room_id` とクライアント生成の部屋鍵から招待 URL を作り、最近の部屋へ保存して入室する。`role` はサーバーが返した値を localStorage 互換の `isAdmin` に反映するだけ。
4. 401 はサインイン導線、403 / 通信失敗 / その他の HTTP 失敗はダイアログ内のエラーとして表示し、ローカル部屋の作成や入室は行わない。ダイアログは開いたまま再試行できる。

### ローカルログイン画面

`maitre-d --local-login` のとき、`GET /auth/login` は SPA の `/login?return_to=...` へ 302 する。`main.ts` は `pathname === '/login'` を検出すると `LoginScreen` を mount し、`completeGoogleSignIn()` は呼ばない（未ログインの 401 で sessionStorage の戻り先と部屋鍵 fragment が消えるのを防ぐ）。

- 画面は noph-ui の `Card` / `TextField`（account 名、`type="password"`）/ `Button`（filled）で構成し、自作の input / button は使わない。
- `POST /api/auth/local/login` に `{ account_name, password }` を JSON で送る。成功時は `return_to`（same-origin のみ。cross-origin は `/`）へ遷移する。
- 401 は「アカウント名またはパスワードが違います」、400 はサーバーの入力エラー、404 は「ローカルログインが無効」、その他は汎用エラーをフォーム内に `aria-live` で出す。
- 遷移後、アプリ mount の `completeGoogleSignIn()` が `/api/session` を Cookie 付きで呼び、account / session と部屋鍵 fragment を復元する。処理名は Google 固有だが、実装は任意の session 復帰に使える（Issue #294）。
- ホーム画面のデバッグ警告と `GET /api/server-info` / `fetchServerInfo` は削除した。

### 本番 SPA ビルド

`vite.config.ts` はライブラリ用で `index.html` を出さない。ポータブル版・`nix run .`・`maitre-d --web-root` 向けに `vite.config.web.ts` と `pnpm run build:app` を追加し、`dist-app/`（`index.html`、assets、`public/` の wasm）を生成する。`maitre-d` は `ServeDir` + `index.html` fallback で `/login` や `/rooms/<id>` を配信する。

### 門前払い画面

`GateScreen` はフィールドやコントロールパネルの代わりに全画面で表示する。4 段階 role では `denied` がこれに当たる（preflight の 403、role が `denied` の応答、WebSocket の `1008` / `4403` / `4404`）。

- 中央大文字: 「アカウントに入室権限がありません」
- 説明: 「部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？」
- 主操作: 「サインイン・アカウント切り替え」。サインイン済みならローカル session を破棄してから `prompt=select_account` 付きの Google 認可 URL を開く。
- 右上: `AccountMenu`。サインイン・アカウント切り替え・サインアウトを行える。
- 副操作: 「ホームに戻る」（`isForbidden` を解除して `/` へ戻る）。

### アカウントUI / Googleサインイン

1. `AccountMenu` は account ID と session ID の両方が保存されている場合だけサインイン済みとみなす。
2. サインイン済みなら `GET /api/account/{account_id}` で表示名とアイコン Tin ID を取得し、32 byte の Tin ID から `/api/cellar/tin/<hex>` を組み立ててアバターにする。取得中・取得失敗時はイニシャルまたは `account_circle` を表示する。
3. アバター押下で M3 Menu を開き、「別のアカウントでログイン」「サインアウト」を出す。サインアウトは account / session（`alhalte:account-id` / `alhalte:session-id` と `session_id` Cookie）と OAuth 用 sessionStorage を消す。端末嗜好（`alhalte:theme` / `alhalte:dice-picker-visible`）は残す。部屋鍵フラグメントは保持する。
4. 未サインインなら Filled tonal の「サインイン」を出す。
5. サインインは現在の完全な URL（クエリ・部屋鍵フラグメントを含む）を sessionStorage に保存し、fragment を除いた絶対 URL を `return_to` にして `GET /auth/login` へ遷移する。`prompt=select_account` はアカウント切り替え時に付ける。`return_to` の origin 検証・state 発行・Google への `redirect_uri` はすべて maitre-d の責務で、Google に argenterie の origin / port は渡らない。
6. callback は maitre-d が処理し、`session_id` Cookie を設定して同じ `return_to` へ 302 する。アプリ起動時（`main.ts`）に sessionStorage の戻り先 URL があれば `GET /api/session` を Cookie 付きで呼び、account / session を保存して `history.replaceState` で元の URL（fragment 含む）を復元する。401 なら未サインインのまま戻り先の保存を消す。
7. 配置はホーム画面の AppBar trailing、フィールド画面の右上、門前払い画面の右上。ホーム画面のテーマ設定 UI は削除し、テーマ選択はコントロールパネル共通ヘッダーの gear menu に集約する。フィールドの `.field-container` は盤面 pan 用に pointer capture を取るが、`field-account-layer` / `field-overlay-layer` とボタン・入力要素からの pointerdown は pan の対象外にし、オーバーレイ上の操作（アバターの Menu など）を奪わない（Issue #268）。

### 発言とログ表示

1. `handleSendMessage` は送信前に extension pipeline を実行し、入力 `(Generast::String, speaker_id)` の ext-io Tin から得た結果を `LogRecord.contentTin` に保存する。pipeline を実行できない場合は `content` のみ保存し、`pipeline-warning` を表示する。この時点では `ChunkLogManager` に追加しない。`isSending` を立てて `requestAppendLog` を await する（`CommonFooter` は送信ボタンを波線 `CircularProgress` に差し替え、フッターを `aria-busy` で暗転し、二重送信を防ぐ）。
2. 成功時はサーバーの `LogEventAppended` echo を待つ。`RoomSocket` は echo を `ChunkLogManager` に追加してから購読者へ通知するため、`App` 側でローカル追加しない（二重追加しない）。`clientOpId` による照合で他クライアントの echo とは区別する。
3. タイムアウト / 通信エラー時は `kind: 'message'` として IndexedDB outbox に積む。ログタブは現在 scene のキューのうち、まだ描画されていない `LogRecord` を取得済みログの下に表示し、その上に波線 `OfflineQueueNotice kind="message"`（「サーバーと通信できませんでした」「メッセージは次回の接続時に送信されます」、件数付き）を置く（Issue #148）。フッター上部の重複通知は出さない。入力欄へは復元しない。再接続で FIFO flush し、各メッセージは echo 後にのみ描画する。flush 中に echo とキューが併存しても `LogRecord.id` で dedupe する。オフライン時は `RoomSocket` のメモリキューを使わず outbox へ直接積む（二重送信を避ける）。
4. ログタブは `ChunkLogManager` の変更を購読し、未描画の record を処理する。`contentTin`（送信時に保存した extension 適用後の `alhalte/generast` Tin）があれば extension pipeline を再実行せず Saignant に渡す。旧レコードだけ `buildGenerastFromText` で有効な拡張機能を priority 昇順に適用する。
5. `renderGenerast` が Saignant WASM で HTML にし、`{@html}` で表示する。現在の `characters` を名前マップとして渡すため、本文中の mention / `fluct` / `assign` もキャラクター名で描画される（Issue #163）。WASM 未初期化時は HTML エスケープしたテキストを返す。
6. `character-came` / `character-gone` の Tin は label で postcard payload を decode し、`kind: 'system'` の控えめな M3 行として時刻付きで表示する。postcard bytes を UTF-8 のチャット本文として decode しない。ラベル不明は従来どおりテキスト fallback にする。
7. **話者の所有権（Issue #271）**: `speakerId` を持たない record はナレーションとして誰でも送れる。`speakerId` を持つ record は、実在する character で、かつ `std::owner_account` が送信者と一致する場合だけ送れる（admin は全 character を代演できる）。最終認可は banquette が `AppendLog` / `EditLog` の entry を検査して行い、違反は `SpeakerNotOwned` になる。UI はフッターのピッカーに `speakableCharacters`（admin=全件 / participant=保有分 / viewer・denied=空）だけを出し、既定話者もその先頭（無ければナレーション）にする。`SpeakerNotOwned` は再試行せず、キュー済みの record は一度だけナレーションへ変換し、それでも拒否されたら破棄する。`LogTab` は `speakerId` がある record でキャラクターの現在名を優先し、未知 id は中立ラベルにする。

### chunk の読み込み（最新・過去）

- `LogTab` は表示時と scene 切り替え時に `queryLogBiblio(sceneId)` → biblio の head から最新 `chunk_index` を得る → `queryLogChunk(sceneId, chunkIndex)` の順で要求する。scene ごとに in-flight / loaded を管理し、二重ロードしない。
- biblio は `ChunkLogManager.applyServerBiblio` に渡す。`chunks[i]` を chunk index `i` とみなし、`readLogBiblioChunkIds` が作る stable な chunk id（`log-chunk:<sceneId>:<index>`）を `registerSceneChunks` で登録する。active chunk は `head.chunkIndex` の位置になる。
- 読み込み中はメッセージ一覧上に contained `LoadingIndicator` を出し、「ログはありません」は出さない。応答後は `ChunkLogManager` に snapshot を取り込み、`id` / `(chunkIndex, position)` で dedupe する。
- 上端スクロール（`handleScrollEdge('top')`）は「読み込み済みの最小 chunk index の 1 つ前」を 1 回だけ `ChunkLogManager` の fetch handler 経由で `queryLogChunk` する。fetch handler は `tinToLogRecord` で entry を `LogRecord` に変換する。同一 chunk の並行要求は in-flight Promise を共有し、最古 chunk まで到達したら `null` を返して追加要求しない（無限ループしない）。
- 過去 chunk の挿入時はサーバーの `(chunkIndex, position)` 順で並べ、読み手のスクロール位置を維持する（最下部へジャンプしない）。
- タイムアウト / 通信エラー時は `OfflineQueueNotice kind="generic"`（「接続が回復したらログを再読み込みします」）を出し、再接続時に再試行する。Saignant 描画は維持する。

### ログキャッシュと scene graph 復元・絞り込み

- `ChunkLogManager` の cache-origin record は `cached: true` を持ち、`alhalte:room-logs:<roomId>` から復元したものだけが該当する。オフラインで cache-origin record を表示している間は LogTab に「キャッシュ表示中」を出す。
- `LogBiblioSnapshot` / `LogChunkSnapshot` はサーバー正。biblio が head を持つ scene では cache-origin record を破棄し、chunk snapshot は該当 chunk の entries を置換する（append しない）。head が無い biblio は cache とサーバー由来 chunk を空にし、サーバーで削除されたログをキャッシュから復活させない。live echo だけは空 biblio 応答との race を避けるため保持する。
- `SceneGraphTab` の `movements` は、ライブの `CharacterMoved` と `character-came` / `character-gone` system record から再構築した履歴を `reconcileMovements` で統合する。came の scene を到着先、直前の gone の scene を出発元として `fromSceneId` を復元し、timestamp が無い system event は `(chunkIndex, position)` の順で並べる。同じ character / route の live move は log record が読み込まれた時点で二重計上しない。再入室・再接続後も、読み込み済み（キャッシュ含む）の came/gone から graph が復元される。
- **scene graph のアカウント絞り込み（Issue #263）**: participant の graph は、保存済み account ID と `std::owner_account` から保有 character を引き、その移動だけを `V_known` の算出に使う（`filterOwnedMovements` → `collectKnownSceneIds`）。`V_known` は保有 character が参加した scene に、`/`・現在の scene・保有 character が現在在籍する scene を加えた集合で、毎回計算する（キャッシュしない）。graph 自体には全 character の移動を渡すため、既知 scene 間の遷移や他 character が見つけた frontier は失わない。`V_known` 外への移動は、scene ID / 名前を捨てた独立した「？」ノードへの点線 edge にし、同じ実 scene への複数 edge も共有しない。admin / viewer は `can_access_scene` が全 scene を許可するため、従来どおり全 graph を表示する。この絞り込みは UI の可視性だけを変えるもので、認可は banquette の `can_access_scene` / `can_send_to_scene` が最終判断する。
- **グラフ / リスト（Issue #273）**: `SceneGraphTab` の先頭に `SegmentedButton`（グラフ / リスト、既定グラフ）を置く。リストは `allScenes` を `Divider` 区切りで並べ、行クリックは graph ノードと同じ `onNavigateToScene`（`App.handleSelectScene` → `enterScene`）を通す。現在 scene の行は `aria-current` と check アイコンで示す。削除は `capabilities.isAdmin && id !== "/"` の行だけに右寄せのゴミ箱を出し、graph ノードと同じルート hoist の確認ダイアログ（無ければタブ内フォールバック）を経由する。`scenes` prop が無い standalone 利用では graph ノード（`isUnknown` を除き dedupe）から一覧を導出する。リストは participant / viewer にも全 scene 名を表示する（可視性は #263 の graph と同じ扱いで、書き込みは banquette が拒否する）。

### メニュー配置

noph-ui の `Menu` は `popover` であり、HTML の UA stylesheet が `position: fixed; inset: 0` を与える。`utils/menuPosition.ts` はアンカーの viewport rect から位置を計算し、`position: fixed !important` / `margin: 0 !important` / `position-area: none !important` / `position-try-fallbacks: none !important` / `align-self: unset !important` / `justify-self: unset !important` で UA の自動配置を打ち消す。

- `inset` は shorthand であり、使わない辺を `removeProperty()` で消すと author 宣言が失われ、UA の `0` がその辺に適用される。`position: fixed` + `fit-content` の過剰拘束は LTR で `left` / `top` を優先するため、メニューが x=0 / y=0 に潰れる（Issue #241）。
- そのため **毎回 4 辺すべてを longhand で設定する**: 使う側は `<px> !important`、使わない側は `auto !important`。文字列パス（`getMenuPositionStyle` が返す `style` 属性）と DOM パス（`applyMenuPosition` の `$effect`）は同じ宣言リストを使う。
- resize / scroll で再適用しても 4 辺の pin を維持し、`max-height` は利用可能領域から計算する。

### BGM / 背景画像

BGM と背景画像は Argenterie の都合であり、banquette はこれらのキー・ラベル・イベントを知らない（Issue #186）。予約は Argenterie 側で行う。

- BGM は state のみに書く。専用の log イベントは持たない。track は `std::bgm`（slot 1、旧来の互換）/ `std::bgm<N>`（任意の slot N >= 2）に `{ tinId, mimeType, name }` の JSON Tin で保存する。旧形式の裸の 64 hex 文字列も受理し、MIME 不明時は `audio/mpeg` を使う。
- **行 = track のある slot**（Issue #249）。`createDefaultJuices` は `bgmSlots: []` を作り、`getNormalizedBgmSlots` は track を持つ slot だけを返す（`bgmSlots` が無い legacy scene は `bgm1..3` の set 済み slot のみ、`bgm::<slot>::removed` の tombstone は常に除外）。「スロット追加」は `max(既存行 ∪ tombstone) + 1` の slot で AssetPicker を開き、選択の ack 後にだけ行を作る。同じ slot 番号は再利用しない。
- **追加フローの失敗表示（Issue #275）**: 行が無い slot のエラーは `{#each bgmSlots}` の外、BGM 節ヘッダ直下の `bgm-add-error`（`role="status"`）に 1 つだけ出す。prepare の失敗（部屋鍵なし / 取得・復号失敗）、tinId の無い音源、ack の拒否（`PermissionDenied` を含む）がここに現れる。読み取り専用の viewer / denied は「スロット追加」自体を disabled にし、`handleAddBgmSlot` / `handleBgmSelect` でも早期 return する。行がある slot は従来どおり `bgm-error-<slot>` に行内表示する。
- 再生状態は `bgm::<slot>::playing`（boolean）、音量は `bgm::<slot>::volume`、再生位置は `bgm::<slot>::started_at` / `bgm::<slot>::paused_at`（整数 UNIX ms、Issue #250）。duration は state に持たない。
  - 再生中: `offset = ((now - started_at) mod D + D) mod D`
  - 一時停止中: `offset = ((min(now, paused_at) - started_at) mod D + D) mod D`
  - 再開: `started_at = now - (paused_at - started_at)`、`paused_at` を削除、`playing=true`。新規 track / ⏪ は `started_at = now`、`paused_at` を削除する。
  - `D` は各クライアントの `HTMLAudioElement.duration`。不明な間は生の非負 offset を `currentTime` に入れ、ブラウザの clamp / loop に任せる。時刻は state を書いたクライアントの壁時計であり、クライアント間の NTP 同期を前提とする。
- 一時停止は `bgm::<slot>::playing=false` と `paused_at` を書き、選択中の track と復号済みの object URL を保持する（再開で `downloadTin` をやり直さない）。track の削除は NONE と 🗑️ の両方で、`std::bgm<N>` / `volume` / `playing` / `started_at` / `paused_at` を削除し、`bgm::<slot>::removed=true` の tombstone を残す（後から届く stale な track broadcast / snapshot で行が復活しないようにする）。track 変更・削除・scene 切替・unmount では object URL を revoke する。
- `SceneSettingsTab` は再生 / 一時停止 / 再開 / ⏪ / 選択 / 音量 / 削除を async 化し、track `std::bgm<N>` → `started_at` → `paused_at` 削除 → `playing` の順（一時停止は `paused_at` → `playing=false`）に ack を待って書く。ack が揃ったときだけローカルの `bgmSlots` / `scene.juices` を確定する。送信中は対象 slot の各コントロールを波線 `CircularProgress` にし、二重実行を防ぐ。
- UI は slot ごとの Connected button group（⏪ `replay` / ▶️・⏸️ `play_arrow`・`pause` / 🎵 `music_note`）と音量 Slider、🗑️ の削除ボタン。viewer / denied は全コントロールを disabled にし、最終認可はサーバー。
- 再生は `utils/audio.ts` の `AudioManager` が担う。slot ごとに `HTMLAudioElement` を 1 つだけ保持し、`loop=true`、`downloadTin(tinId, roomKey)` で復号した payload から object URL を作る。`StateSnapshot` / `StateUpdated` / scene 入場で `started_at` / `paused_at` / `playing` を反映し、再生中は transport identity（`tinId` / `started_at` / `paused_at`）の変化または 1 秒超のドリフトでだけシークする。同じ snapshot の再適用は冪等で、再生や download を重複させない。
- autoplay 制限はユーザーに通知しない（Issue #248）。最初の `pointerdown`（capture, once）で、`playing` 意図があり準備済みの slot を導出済み offset から再開する。準備前の操作で消費した場合は次の操作で再試行する。音源取得 / 復号に失敗した slot は row 付近にエラーを表示し、`playing` を確定しない（state を書かない）。
- 失敗時は変更前へ戻し、slot 付近にエラーを出す。キューには積まない（最終状態のみが意味を持つため、再接続時の自動再送をしない）。
- `App` は `StateUpdated` / `StateDeleted` の BGM キー（track / playing / volume / started_at / paused_at / removed）を `bgmSlots` と旧 `bgm1..3` へ反映し、現在 scene の音声だけを再生する。`StateSnapshot` は removed → track → timestamps → playing / volume の順に適用する（辞書順に依存しない）。state 更新の可否はサーバーの権限判定に従い、`PermissionDenied` はそのまま表示する。viewer も他クライアントが開始した BGM は再生する（viewer の操作制限は #158 / #159 のまま）。

### scene の管理操作

- 「シーンを新規作成」「ログを削除」「シーンを削除」「メンバー移動」は `requestCreateScene` / `requestRewindLog(sceneId, 0, 0)` / `requestDeleteScene` / `requestMoveCharacter` で送り、成功 ack 後にだけローカル state を更新する（楽観更新しない）。
- **シーン検索と新規作成（Issue #269）**: コントロールパネル共通ヘッダーと「キャラクターを移動」モーダルは同じ `SceneSwitcher` を使う。Enter（モーダルでは「移動」ボタンも同じ `resolve()`）は、入力と完全一致する scene → 先頭の候補 scene → 管理者なら新規作成、の順に解決する。新規作成の id は `generateSceneId()` が作り、`CreateScene { name }` の ack 後にだけローカル一覧へ追加する。ヘッダーの作成は ack 後に `handleSelectScene(id)` を通して `enterScene` まで行う。空結果の案内は管理者では「シーンは新規作成されます」、非管理者では従来の「該当するシーンはありません」で、非管理者には作成導線を出さない（サーバーも `authorize_admin` で拒否する）。
- **キャラクター移動モーダル（Issue #269）**: キャラクター一覧の行ボタンは旧「別シーンに配置」Menu を置き換え、ルート hoist の「キャラクターを移動」モーダルを開く。既存 scene なら `MoveCharacter` だけ、新規名なら `CreateScene` ack → `MoveCharacter` ack の順で行い、`currentSceneId` は変えない（管理者が curation 中の scene から飛ばされない、Issue #228 の不変条件）。create だけ成功して move が失敗した場合は空の scene がサーバーに残る（自動ロールバックはしない）。
- シーン作成時の表示名は `CreateScene.name` として送り、banquette が `std::name` へ原子的に書く。リネームは `requestUpdateState(sceneId, 'std::name', name)` の `StateUpdated` ack 後にだけローカル名を確定する。サーバーは create / rename で `SceneInfo` を room へ broadcast し、他クライアントは再読み込みなしで名前を更新する。
- ログ削除は `RewindLog(0, 0)` で全ログを切り詰め、`LogEventRewound` 後にローカルの chunk をクリアする。
- 送信中は押したアイコンを波線 `CircularProgress` にし、当該操作を二重実行できない。失敗時は管理領域の下に中央寄せで「サーバーと通信できませんでした」を出す。
- 接続中は ack 後にだけローカル state を更新する。**未接続時は scene 作成 / 削除 / ログ削除 / メンバー移動を成功扱いにせず**、ローカル一覧を変更せずに同じエラーを表示する（ローカル専用フォールバックは持たない）。`SceneList` broadcast は scene 一覧の権威として反映し、`SceneDeleted` は第二の防護として該当 scene を除く。

### キャラクター同期

- キャラクタータブは表示時に `queryListCharacters` を発行し、問い合わせ中は contained `LoadingIndicator` を出す。タイムアウト時は `OfflineQueueNotice kind="character"` を出し、キャッシュ済み一覧は保持する。
- `CharacterList` / `CharacterInfo` は `tinMapToCharacter` で全属性を decode する。character tab と field が同じ statuses / parameters / chatPalette / icon / color / piecePosition を表示する。
- `CreateCharacter` / `UpdateCharacter` 成功時、banquette は room 全体へ `CharacterInfo { character_id, data }` を broadcast する。`App` は `CharacterInfo` を id で upsert する（ack と broadcast の重複は冪等）。`DeleteCharacter` の `CharacterDeleted`、`MoveCharacter` の `CharacterMoved` も同様に全クライアントへ届き、`App` は一覧・`scene.members` を更新する。
- `scene.members` は `SceneInfo` / `SceneList` の `present_characters` を正として再構築する。reload / reconnect 後もサーバーが永続化した在籍情報から復元され、`character::<id>::scene` のような write-only mirror は使わない。
- 保存は ack 後にだけ反映する。作成は `CharacterCreated` のサーバー採番 ID を使う。保存中は編集パネルを `aria-busy` + 非活性にし、保存アイコンを波線 `CircularProgress` にする。成功時のみ閉じる。編集の入口は所有者（`isCharacterOwnedBy`）または管理者だけに許す（Issue #267 / #228。サーバーの CRUD も owner または admin）。
- タイムアウト時は `kind: 'character'` として outbox に積み、再接続 flush で送信して成功後に一覧へ反映する。`PermissionDenied` はキューせずパネルを開いたままエラー表示する。削除も ack 後に行う。

### 駒の同期

- 駒の配置・位置は scene state の `piece::<id>::placed` / `piece::<id>::pos` が正。`App` は `StateSnapshot` / `StateUpdated` / `StateDeleted` を適用し、`characters` の `placedAsPiece` / `piecePosition` に反映する。他クライアントの駒も同じ state から描画される。
- 配置 toggle と drag 後の位置確定は `requestUpdateState` の ack 後にだけローカルへ確定する。drag 中はフィールド描画を即時追従させ、同じ駒の連続 drag は coalesce して最後の位置だけを送る（fire-and-forget しない）。
- 未接続時は配置 / 位置変更を成功扱いにせず、`piece-action-error`（「ルームに接続していないため、駒を操作できません」）を表示して drag 前の位置へ戻す。ack が失敗した場合も同様に戻す。
- character 削除時は banquette が `piece::<id>::*` を best-effort で削除し、`StateDeleted` を broadcast する。これにより他クライアントに操作可能な幽霊駒が残らない。

### 拡張機能

- メタデータは `/` scene の state key `extension::<id>` に `{ id, name, description, icon, priority, enabled, tinId }` の JSON Tin で保存する。wasm 本体は cellar に RoomKey 暗号化で置き、`tinId` だけを state に持つ（4KiB 制限を守る）。
- **ABI 2 と自己紹介 id（Issue #274）**: 拡張機能の表示メタデータ（id / name / icon / description）の正は wasm の `self_introduction`。ABI 2 は postcard の `(id, name, icon_svg, description)` を返し、`id` は公式拡張では package 名（= manifest id）と一致する。ホストは ABI 1（`(name, icon_svg, description)`、id なし）と ABI 2 を受け入れ、それ以外を拒否する。ABI 2 の自己紹介 id が manifest id と一致しない wasm は `wasmMissing: true` として実行しない（fail-closed）。`Restaurant.toml` は `id` / wasm path / `enabled` だけを持ち、`name` は持たない。オフライン / テスト用の `DEFAULT_OFFICIAL_EXTENSIONS` はフォールバックとして残すが、staged wasm の自己紹介と一致することをテストで固定する。
- **一度だけの seed とサーバー正**: 部屋 root state の `extension::__seeded` を seed 済みマーカーとする。マーカーが無い部屋では、拡張機能タブを開いた GM がサーバーに無い公式既定拡張のメタデータを `requestUpdateState` で書き、最後にマーカーを書く（途中で失敗するとマーカーが無いため、再接続時に残りを seed し直す）。既存 room もマーカーが無ければ移行として一度だけ seed し、すでにサーバーにあるメタデータは上書きしない。GM 以外は seed せず、公式既定を読み取り専用で表示する。
- マーカーがある場合、サーバー state が正。公式既定 id でもサーバーに無ければ削除済みとして扱い、再入室・再接続で再追加しない。マーカーが無い legacy room に限り、サーバーに無い公式既定 id をローカルの wasm ごと保持する。サーバーに無いカスタム id は削除する。同じ id でサーバーの `tinId` が変わった場合だけ wasm を取り直し、それ以外はローカルの `wasmBytes` を保持する。手元に wasm が無い場合は `tinId` から cellar で取得する。
- **wasm 欠落**: `loadDefaultExtensions` は manifest にある wasm の取得・instantiate に失敗した拡張を `wasmMissing: true` で返す。`applyRemoteExtensions` は `tinId` の download に失敗した拡張も `wasmMissing: true` にする。タブは該当行に「wasm がありません」を出し、有効化 Switch を無効にする（並べ替え・削除は可能）。`ExtensionPipeline.executePipeline` は wasm の無い有効拡張を黙って skip せず、`getLastWarnings()` / `onWarning` に警告を残す。送信時 pipeline の警告はフッター上部の `pipeline-warning` に表示し、原文（ext-io Tin）はそのまま送信する。
- タブ表示時に `queryState('/')` を発行し、contained `LoadingIndicator` を出す。
- toggle / delete / reorder / upload は楽観更新せず、`requestUpdateState` / `requestDeleteState` の成功後にだけローカルへ反映する。送信中は該当行のアイコンを波線 `CircularProgress` にする。
- タイムアウト / エラー時はタブ上部に「サーバーと通信できませんでした」を出し、一覧はスクロール・閲覧できる状態を保つ。暗転・`pointer-events: none` をタブ全体に掛けず、toggle / delete / reorder / upload などの mutation 操作だけを `disabled` / `aria-disabled` にする（Issue #149）。非管理者（participant / viewer / denied）には、行の非操作コンテンツ（名前・説明・アイコン・優先度バッジ）だけを少し暗くして操作不能を視覚的に示す（Issue #244）。シェル・一覧・スクロール領域・オフライン通知・ロック表示・「wasm がありません」は full contrast を保つ。再接続で `queryState('/')` を再試行する。
- `.wasm` は検証後に `encodeRoomTin` + `uploadTin` で cellar へ保存し、進捗を表示して完了後に state を更新する。

### アセット

- `utils/assets.ts` は画像・音声を `FileReader` で data URL にし、localStorage と in-memory の Pantry キャッシュアダプタへ保存する。`AssetPickerModal` は personal / room、image / audio で絞り込み、NONE カードと削除を提供する。
- アップロードは `encodeRoomTin`（RoomKey 暗号化 + LZ4）→ `uploadTin` で cellar へ送る。進捗は実 XHR の `upload.onprogress` のみを使い、カード内に波線 `LinearProgress` を出す。成功時は `asset.id` に `tinId` を付け、room scope はさらに `/` の state key `asset::<id>` にメタデータ（id, name, mediaType, mimeType, size, tinId, createdAt, ownerAccountId）を `requestUpdateState` で保存する。失敗時はカード内に「アップロードに失敗しました」と再試行ボタンを出す。
- personal のメタデータは localStorage のみに置き、`tinId` を他者へ公開しない。本体は cellar に置く。
- シーン背景は `AssetPickerModal` の選択から `encodeRoomTin` + `uploadTin`（部屋鍵）で cellar へ上げ、scene state `std::b-img` に `{ tinId, mimeType }` の小さな JSON Tin として保存する（data URL は state に入れない）。旧形式の 64 hex 文字列も受信側で受理する。幅・高さは `std::b-img::width` / `std::b-img::height` に数値 Tin で保存する。
- 背景画像の受信側は `tinId` から `downloadTin` で復号し、`juices.bgImage` に data URL を設定する。`std::b-img` の upload / state 更新が失敗した場合はローカルを確定せず、背景カードの下に「サーバーと通信できませんでした」を出す。部屋鍵が無い場合も upload を開始しない。
- モーダルを開くたびに `syncRoomAssets` が `queryState('/')` で `asset::*` を取得してローカルへマージし、欠けている tin は `downloadTin` で取得する。同期失敗時は静かに既存キャッシュを表示する。アセットは 30 日 TTL の対象外（永続）。
- `App` は `AssetPickerModal` に `roomSocket` と解決済みの部屋鍵を渡す。`SceneSettingsTab` も同じ `roomKey` を受け取り、背景画像・寸法の state 更新に使う。部屋鍵が無い場合、`AssetPickerModal` / `SceneSettingsTab` の背景・BGM upload は開始せず、カード内に失敗を表示する。

### 権限 UI


`utils/session.ts` の `searchAccounts` だけが HTTP を使い、権限表そのものは `RoomSocket` の権限 frame で読み書きする（Issue #201）。`RoomPermissionModal` は開くたびに `requestQueryPermissions` で `PermissionsSnapshot` を取り、M3 Search、Divider、Connected Button Group（拒否 / 閲覧者 / 参加者 / 管理者）で行を描画する。一覧は account ID しか持たないため、表示名は `/api/accounts/search?q=<account_id>` で解決し、解決できない行は `#<account_id>` を出す。ロール変更・追加・既定へのリセットは `requestSetMemberPermission` の ack（`PermissionsUpdated`）が返ってから一覧を差し替える（楽観更新しない）。失敗時は一覧を変えずに `permission-error` を出す。「削除」は明示エントリを既定の権限へ戻す操作で、`SetMemberPermission(default_permission)` を送る（banquette にエントリ削除 frame は無い）。既定の権限 selector はモーダルと `CommonHeader` の gear menu にあり、`requestSetDefaultPermission` の ack 後にのみ表示が変わる。`CommonHeader` の旧「部屋の全てをPLに公開」（`SetDone` / `DoneUpdated`）は削除した。

`App` は `role` を `ControlPanel` 経由で各タブへ渡し、viewer 判定ではなく `roomRoleCapabilities` で操作を出し分ける。

| role | 送信（log / state） | character CRUD | scene 作成・削除・メンバー移動 | 拡張機能 | 権限表 / 既定の権限 |
| --- | --- | --- | --- | --- | --- |
| `admin` | 全 scene。話者は全 character（GM 代演、Issue #271） | 可（全 character） | 可 | 可 | 可 |
| `participant` | アクセス可能 scene（UI は現在の scene のみ送信可とし、最終認可はサーバー）。話者は保有 character のみ | 可（自分のみ。Issue #267） | 不可 | 不可 | 不可 |
| `viewer` | 不可（入力欄・送信・ダイスを無効化） | 不可 | 不可 | 不可 | 不可 |
| `denied` | 不可（`GateScreen` を表示） | 不可 | 不可 | 不可 | 不可 |

`PermissionsSnapshot` は `accessible_scenes` を持たないため、participant の「アクセス可能 scene」は UI では現在の scene（と banquette が writer に常時許可する `/`）だけを送信可能として扱う。`accessibleSceneIds`（全 scene）は admin / viewer の graph と書込の可否に使い、participant の graph は `sceneGraphSceneIds`（`V_known`）で絞る（Issue #263）。書込の可否とは独立している。入室拒否時は `GateScreen` を出し、右上の `AccountMenu` からサインイン・アカウント切り替え・サインアウトを行える。

## 既知の差分・制約

- `contentTin` は送信時に一度だけ生成する。pipeline を実行できない場合（実行エラー）は `content` のみを保存し、表示は従来どおり render 時 pipeline にフォールバックする。wasm の無い有効拡張は適用せず、`pipeline-warning` に表示して ext-io Tin をそのまま `contentTin` にする。
- システムイベント（BGM / 背景 / 入退場）は banquette の Tin に timestamp を持たないため、ログ上の時刻は 0（1970）になる。kitchen は前後の発言から補間する。scene graph は `(chunkIndex, position)` の順で並べ、表示時刻は 0 のままになる。
- アセットは cellar へ実アップロードするが、`personal` のメタデータは localStorage のみで、他クライアントとは共有されない。アセット本体は RoomKey で暗号化して cellar に置くが、maitre-d proxy と cellar は `Bytes` でリクエストを全量メモリにバッファする（64 MiB 上限）。大きなファイルほどメモリを消費する。
- アウトボックス flush は at-least-once。ack を受け取れずにタイムアウトした操作は再送されるため、サーバー側で適用済みの場合は重複する可能性がある（冪等キーは持たない）。
- 拡張機能タブは開いた時点の `queryState('/')` を正とし、開いている間の `StateUpdated` broadcast は反映しない（再接続・再表示で取り直す）。
- 公式既定拡張の削除の永続性は `extension::__seeded` マーカーに依存する。マーカーは GM が拡張機能タブを最初に開いたときに書かれるため、それ以前に別クライアントが削除した公式既定は seed で復活し得る（seed は「サーバーに無い公式既定を書く」ため）。サーバー正のマージも拡張機能タブを開いた時点で適用され、タブを開く前の送信 pipeline は App のローカル既定を使う。
- タイムアウトは 8 秒で、権限問い合わせのようにサーバーが黙って破棄する操作では唯一の失敗シグナルになる。`PermissionDenied` が返らない経路はタイムアウトとして扱う。
- Saignant WASM は v2 で character 名を受け取る。v2 をエクスポートしない古い wasm を読み込んだ場合だけ mention が `Anonymous` になる。committed artifact は `scripts/build-saignant-wasm.sh` で更新する。
- 開発エントリ（`main.ts`）は `isAdmin` も `userRole` も渡さない。管理者は `GET /api/rooms/{id}` の `role === "admin"` から決まる。`isAdmin` prop はテスト / プレビュー用の明示 override で、既定は false。`initialRoom` prop も同じくテスト / プレビュー用で preflight をスキップし、そのとき role 未指定なら participant 相当（送信可・管理不可）として扱う。
- 入室 preflight のネットワーク / 5xx 失敗は 2 秒間隔・最大 5 回で打ち切り、preflight エラー画面の「再試行」ボタンだけを導線にする。403 / 401 / 404 は再試行しない。401 は `GateScreen`（サインイン導線）として扱う。グローバルな接続バナーは無く、失敗は操作したタブ / エディタ内で告知する（Issue #261）。
- 部屋鍵は URL フラグメントに置く。サーバーへ送らない設計だが、ブラウザ履歴や共有 URL には残る。`App` は `roomKey` prop → `currentRoom.url` のフラグメント → `window.location.hash` の順に解決し、64 hex を検証する。鍵が無い場合、アセット・拡張 wasm の cellar upload は試行せずエラーを表示する。
- ホーム画面のテーマ設定 UI は Issue #116 で削除した。現在のテーマ選択はコントロールパネル共通ヘッダーの gear menu にあるパレットのみで、light / dark / system の切替 UI は持たない。
- Google OAuth の `redirect_uri` は maitre-d の固定 URL（`{public_url}/auth/google/callback`）だけを Google Cloud Console に登録する。argenterie の origin / port は登録しない。`return_to` は maitre-d が CORS allow-list と照合する。`/api/session` を Cookie 付きで呼ぶため、開発で Vite dev server を使う場合は `public_url` を Vite の dev origin に合わせる。cafe / ポータブル / `nix run .` では maitre-d が SPA を同一 origin で配信する。
- `AccountMenu` は account ごとに `GET /api/account/{id}` を呼ぶ。プロフィール取得に失敗してもサインイン状態は維持し、イニシャル表示にフォールバックする。
- 入室可否は `GET /api/rooms/{id}` の preflight を正とする。`isForbidden`（= `denied`）は WebSocket の `Error` frame / close code でもフォールバックとして立つが、権限表 (`PermissionsSnapshot`) からは導出しない。`PermissionsSnapshot` は要求元にしか返らず、`SetMemberPermission` / `SetDefaultPermission` の ack も broadcast されないため、他クライアントによる降格は再接続時の `QueryPermissions`（`App` は接続のたびに問い合わせる）まで見えない。participant のアクセス可能 scene は `PermissionsSnapshot` に含まれないので、UI は現在の scene だけを送信可能として扱い、最終認可は banquette に委ねる。
