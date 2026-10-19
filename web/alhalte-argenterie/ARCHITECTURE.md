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
| `maitre-d` `POST /api/rooms` | 認証必須。`{ name, room_id? }` で部屋を登録し、作成者を `gm` にする（`utils/home.ts` の `createRoom`）。`room_id` はクライアント生成の 16 hex を送ってもよいが、返却値が正。 |
| `maitre-d` `GET /api/rooms/{room_id}` | 入室 preflight。部屋名・caller の `role`・`done` を返す（`utils/home.ts` の `fetchRoomInfo`）。403 / 404 は事前に判別する。 |
| `maitre-d` `/api/room/{room_id}/permissions` | 部屋権限の取得・設定・削除（`utils/session.ts`）。`room_id` は 16 hex。 |
| `maitre-d` `/api/accounts/search` | account 検索。 |
| `maitre-d` `/api/account/{account_id}` | account 取得・更新。アカウントメニューの表示名・アイコン取得に使う（`utils/account.ts`）。 |
| `maitre-d` `/auth/google/url` | Google 認可 URL の取得（`utils/account.ts`）。 |
| `maitre-d` `/auth/google/callback` | 認可コードを session に交換（POST、`utils/account.ts`）。 |
| `maitre-d` `/api/room/{room_id}/export/zip?type=full|logs` | ZIP ダウンロード（`CommonHeader.svelte`）。 |
| `maitre-d` `/api/cellar/store`、`/api/cellar/store/{tin_id}`、`/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。アセット・拡張 wasm の Tin を XHR で送受信する（`utils/cellarTransfer.ts`）。store check は `200 OK` + body の `result`（`upload_required` / `deduplicated` / `already_owned`）で判定し、`upload_required` のときだけ本体を PUT する。進捗は `upload.onprogress`、失敗は型付き `CellarTransferError`。 |
| Vite dev proxy | `/api`、`/auth`、`/room` を `http://127.0.0.1:8080`、`/ws` を `ws://127.0.0.1:8080` へ転送。`/auth` は Google サインインの URL 取得と callback に必要。 |

### WebSocket

`utils/roomSocket.ts` の `RoomSocket` が `/ws/room/{roomId}?account_id=...&session_id=...` へ接続する。`ClientFrame` を JSON で送り、`ServerFrame` を JSON で受ける。heartbeat ping には自動で `HeartbeatResponse` を返す。未接続時の送信はキューに積み、再接続後に flush する。

- **URL 構築**: `buildRoomWebSocketUrl` は招待ページ URL（`https://host/rooms/<id>#<key>`）からは **origin だけ** を使い、`ws(s)://host/ws/room/<id>` を組み立てる。`/rooms/<id>` などのパスを WebSocket URL に引き継がない。明示的な `ws(s)://.../ws/room/...` だけをそのまま使い、それ以外の `ws(s)` URL も origin から作り直す。
- **room ID 検証**: `utils/home.ts` の `parseRoomUrl` は 16 hex（または移行用 decimal）だけを受理し、`room-1` のような任意文字列は `null` で拒否する。`generateRoomUrl` は 16 hex を作る。
- **close 通知と再接続**: `RoomSocket` は `onclose(event)` を `onClose` callback と `getLastCloseEvent()` で公開する。`1008` / `4403` / `4404` は再試行不能として自動再接続を止め、`App` はこれらを `isForbidden`（`GateScreen`）に反映する。入室前の HTTP 403 / 404 は `GET /api/rooms/{id}` の preflight で判別する。

- **Tin エンコード**: banquette が `Tin` を期待する frame（`AppendLog.entry`、character の `data`）は、`jsonToTin` / `logRecordToTin` / `characterToTinMap` で Tin JSON `{ label, payload }` に変換してから送る。label は pantry の `labelHash` で用途別定数（`LABEL_LOG_RECORD`、`LABEL_JSON`、`LABEL_BANQUETTE_LOG_CHANGE_BGM` など）を使う。受信側は `tinToJson` / `tinToLogRecord` / `tinMapToCharacter` で戻す。`CharacterList` / `CharacterInfo` も `tinMapToCharacter` で全属性（statuses / parameters / chatPalette / icon / color / piecePosition など）を decode する。
- **Promise API と ack**: mutate 用の `request*` と query 用の `query*` が Promise を返す。既存の void API（`appendLog`、`updateState`、`createCharacter` など）は残している。
  - `AppendLog` は payload に `clientOpId` を埋め、同じ id を持つ `LogEventAppended` だけで resolve する（他クライアントの echo では resolve しない）。
  - `UpdateState` / `DeleteState` は `scene_id` + `key`（更新は値も比較）で照合する。
  - Character CRUD は `CharacterCreated` / `CharacterUpdated` / `CharacterDeleted` を FIFO で照合する（UI は同時操作を直列化する前提）。
  - `CreateScene` / `DeleteScene` / `MoveCharacter` は新設の `SceneCreated` / `SceneDeleted` / `CharacterMoved` で resolve する。
  - `RewindLog` は `LogEventRewound`、`ChangeBgm` は `LogEventAppended` で resolve する。
  - `Error` は対応する `*Failed` code と `PermissionDenied` で reject する。
  - 応答が無い場合は `DEFAULT_OPERATION_TIMEOUT_MS = 8000` で `OperationTimeoutError`（`code: 'timeout'`）にし、pending を必ず破棄する。サーバー由来の失敗は `ServerOperationError`（`code` 付き）で返す。
- 未処理だった `StateSnapshot` / `LogBiblioSnapshot` / `LogChunkSnapshot` / `CharacterInfo` / `LogEventRewound` / `SceneCreated` / `SceneDeleted` / `CharacterMoved` も dispatch し、`onServerFrame` と個別 callback の両方へ流す。
- **SceneList / SceneInfo**: `SceneList.scenes` は `SceneSummary`（`id`、`name`、`present_character_count`、`present_characters`、`active_client_count`、`is_loaded`）の列で、旧サーバーの bare id 列も受理する。`present_characters` は scene 在籍 character id の昇順リストで、`App` はこれで `scene.members` を再構築する（reload / reconnect 後も復元）。`RoomSocket` は id 列を `getScenes()`、summary を `getSceneSummaries()` に保持し、`onSceneInfo` で create / rename / membership の broadcast を通知する。
- **StateSnapshot hydration**: `EnterScene` の成功応答として banquette が `StateSnapshot` を返す（access 可能な場合）。`App` は `onStateSnapshot`（real socket）と subscribe（custom / test socket）の両経路で受け、`std::name` / `std::bgm<N>` / `bgm::<slot>::volume` / `std::b-img*` / `view::transform` / `piece::<id>::*` を冪等に適用する。再接続・scene 切替でも同じ snapshot が再適用される。

### WASM

- `utils/saignant.ts`: `/wasm/alhalte_saignant.wasm` を読み、`render_generast_abi_v2`（存在すれば）または `render_generast_abi` と `get_full_css_abi` を呼ぶ。現在の `characters` から `{ "<id>": "<name>" }` を作って v2 に渡し、mention / `fluct` / `assign` の名前を解決する。v2 をエクスポートしない wasm では v1 にフォールバックし、名前は渡さない。render cache key は characters signature を含み、`CharacterList` / `CharacterInfo` 更新時は `LogTab` が cache と `renderedContentMap` を破棄する。Vitest では `node:fs` で実ファイルを読む。
- `wasm/runner.ts`: 拡張機能 wasm の `alhalte_abi_version`、`alloc`、`dealloc`、`process_generast`、任意の `self_introduction` を呼ぶ。`ExtensionPipeline` は有効な拡張機能を priority 昇順に適用する。
- 開発時のビルド / 配置は `nix run .#saignant-wasm`（または `nix develop -c ./scripts/build-saignant-wasm.sh`）。`public/wasm/alhalte_saignant.wasm` は commit 対象で、Rust の `abi::ABI_VERSION` と一致することを `crates/alhalte-saignant/tests/artifact_test.rs` が検証する。

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
- `LogRecord`（`content`、`speaker`、`speakerId`、`timestamp`、`chunkId`、任意の `chunkIndex` / `position` / `contentTin` / `kind` / `event` / `cached`）
- `ExtensionItem`（`wasmBytes`、`priority`、`enabled`、`cellarTinId`）

### ローカル保存

- `localStorage`: account ID、session ID、訪問済み部屋、最近の部屋、アセット（`alhalte_pantry_assets`）、部屋ログのキャッシュ（`alhalte:room-logs:<roomId>`）。部屋ログは `kind` / `event` / `contentTin` を含めて保存し、復元時は cache-origin として扱う。オフライン表示専用で、サーバーの biblio / chunk snapshot が届いた scene はキャッシュではなくサーバーを正とする。
- IndexedDB `alhalte-argenterie-outbox`（store `outbox`）: 送信できなかった message / character（拡張用に state / asset も型として許容）を `{ id, roomId, kind, createdAt, payload }` で保存する。`createdAt` から `OUTBOX_TTL_MS = 30 日` を超えたエントリは読み出し時に prune する。IndexedDB が無い環境ではメモリ実装へフォールバックする。
- Cookie: `session_id`（`SameSite=Lax`）。
- `sessionStorage`: OAuth の `state`、認可後の戻り先 URL（クエリと部屋鍵フラグメントを含む）、OAuth の `redirect_uri`。サインイン中だけ保持し、完了時に削除する。
- URL フラグメント: 部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を作る。

## 内部フロー

### 入室

1. `main.ts` が `App` を mount する。管理者は `isAdmin` ではなくサーバーの `role` から決める。
2. `App.svelte` は `resolveRoomFromLocation` で URL から部屋を解決する。
3. 入室前に `GET /api/rooms/{room_id}` を preflight する。成功時は返却された `name` を `currentRoom.name` に、`role` を `userRole` に反映してから `RoomSocket` を作る。403 / 401 は `GateScreen`、404 は not-found 画面、ネットワーク / 5xx は 2 秒間隔・最大 5 回の再試行後に再試行 UI を出す。403 / 404 は再試行しない。
4. `RoomSocket` を作り、account / session をクエリに付けて接続する。preflight が `ok` になるまで WebSocket は開かない。`initialRoom` prop はテスト / プレビュー用の明示 override で、preflight と `userRole` の上書きをスキップする。
5. `ServerFrame` の `SceneList`（`SceneSummary` の列）、`SceneInfo`、`CharacterList`、`CharacterInfo`、`StateSnapshot`、`StateUpdated`、`StateDeleted`、`LogEventAppended`、`SceneCreated` / `SceneDeleted` / `CharacterMoved` / `CharacterDeleted` などを Svelte の state に反映する。`SceneList` / `StateSnapshot` がシーン名・BGM・背景・view・駒の権威で、`SceneList` / `SceneInfo` の `present_characters` が `scene.members` の権威、`CharacterInfo` / `CharacterDeleted` / `CharacterMoved` が character 一覧の権威。`SceneInfo` は create / rename / membership を他クライアントへ即時反映する。
6. ログタブは入室時と scene 切り替え時に最新 chunk を読む（後述）。WebSocket の `Error` frame や close code（`1008` / `4403` / `4404`）も `isForbidden` のフォールバックとして `GateScreen` を出す。

`isAdmin` は `userRole === 'gm'`（viewer は常に false）で導出し、`isAdmin` prop はテスト / プレビュー用の明示 override に限定する。`VisitedRoom.isAdmin` は localStorage 互換のために保存するだけで、権限判定には使わない。

### 部屋作成

1. ホーム画面の「部屋の新規作成」は、表示用に 16 hex の `room_id` と 32 byte の部屋鍵をクライアントで生成する。
2. 「入室する」で `POST /api/rooms` に `{ name, room_id }` を送る。account / session が無い場合は送信せず、ダイアログ内にサインイン導線を出す。
3. 成功時だけ、返却された `room_id` とクライアント生成の部屋鍵から招待 URL を作り、最近の部屋へ保存して入室する。`role` はサーバーが返した値を localStorage 互換の `isAdmin` に反映するだけ。
4. 401 はサインイン導線、403 / 通信失敗 / その他の HTTP 失敗はダイアログ内のエラーとして表示し、ローカル部屋の作成や入室は行わない。ダイアログは開いたまま再試行できる。

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

1. `handleSendMessage` は送信前に extension pipeline を実行し、入力 `(Generast::String, speaker_id)` の ext-io Tin から得た結果を `LogRecord.contentTin` に保存する。pipeline を実行できない場合は `content` のみ保存し、`pipeline-warning` を表示する。この時点では `ChunkLogManager` に追加しない。`isSending` を立てて `requestAppendLog` を await する（`CommonFooter` は送信ボタンを波線 `CircularProgress` に差し替え、フッターを `aria-busy` で暗転し、二重送信を防ぐ）。
2. 成功時はサーバーの `LogEventAppended` echo を待つ。`RoomSocket` は echo を `ChunkLogManager` に追加してから購読者へ通知するため、`App` 側でローカル追加しない（二重追加しない）。`clientOpId` による照合で他クライアントの echo とは区別する。
3. タイムアウト / 通信エラー時は `kind: 'message'` として IndexedDB outbox に積む。ログタブは現在 scene のキューのうち、まだ描画されていない `LogRecord` を取得済みログの下に表示し、その上に波線 `OfflineQueueNotice kind="message"`（「サーバーと通信できませんでした」「メッセージは次回の接続時に送信されます」、件数付き）を置く（Issue #148）。フッター上部の重複通知は出さない。入力欄へは復元しない。再接続で FIFO flush し、各メッセージは echo 後にのみ描画する。flush 中に echo とキューが併存しても `LogRecord.id` で dedupe する。オフライン時は `RoomSocket` のメモリキューを使わず outbox へ直接積む（二重送信を避ける）。
4. ログタブは `ChunkLogManager` の変更を購読し、未描画の record を処理する。`contentTin`（送信時に保存した extension 適用後の `alhalte/generast` Tin）があれば extension pipeline を再実行せず Saignant に渡す。旧レコードだけ `buildGenerastFromText` で有効な拡張機能を priority 昇順に適用する。
5. `renderGenerast` が Saignant WASM で HTML にし、`{@html}` で表示する。現在の `characters` を名前マップとして渡すため、本文中の mention / `fluct` / `assign` もキャラクター名で描画される（Issue #163）。WASM 未初期化時は HTML エスケープしたテキストを返す。
6. `change-bgm` / `change-bg-img` / `character-came` / `character-gone` の Tin は label で postcard payload を decode し、`kind: 'system'` の控えめな M3 行として時刻付きで表示する。postcard bytes を UTF-8 のチャット本文として decode しない。ラベル不明は従来どおりテキスト fallback にする。

### chunk の読み込み（最新・過去）

- `LogTab` は表示時と scene 切り替え時に `queryLogBiblio(sceneId)` → biblio の head から最新 `chunk_index` を得る → `queryLogChunk(sceneId, chunkIndex)` の順で要求する。scene ごとに in-flight / loaded を管理し、二重ロードしない。
- biblio は `ChunkLogManager.applyServerBiblio` に渡す。`chunks[i]` を chunk index `i` とみなし、`readLogBiblioChunkIds` が作る stable な chunk id（`log-chunk:<sceneId>:<index>`）を `registerSceneChunks` で登録する。active chunk は `head.chunkIndex` の位置になる。
- 読み込み中はメッセージ一覧上に contained `LoadingIndicator` を出し、「ログはありません」は出さない。応答後は `ChunkLogManager` に snapshot を取り込み、`id` / `(chunkIndex, position)` で dedupe する。
- 上端スクロール（`handleScrollEdge('top')`）は「読み込み済みの最小 chunk index の 1 つ前」を 1 回だけ `ChunkLogManager` の fetch handler 経由で `queryLogChunk` する。fetch handler は `tinToLogRecord` で entry を `LogRecord` に変換する。同一 chunk の並行要求は in-flight Promise を共有し、最古 chunk まで到達したら `null` を返して追加要求しない（無限ループしない）。
- 過去 chunk の挿入時はサーバーの `(chunkIndex, position)` 順で並べ、読み手のスクロール位置を維持する（最下部へジャンプしない）。
- タイムアウト / 通信エラー時は `OfflineQueueNotice kind="generic"`（「接続が回復したらログを再読み込みします」）を出し、再接続時に再試行する。Saignant 描画は維持する。

### ログキャッシュと scene graph 復元

- `ChunkLogManager` の cache-origin record は `cached: true` を持ち、`alhalte:room-logs:<roomId>` から復元したものだけが該当する。オフラインで cache-origin record を表示している間は LogTab に「キャッシュ表示中」を出す。
- `LogBiblioSnapshot` / `LogChunkSnapshot` はサーバー正。biblio が head を持つ scene では cache-origin record を破棄し、chunk snapshot は該当 chunk の entries を置換する（append しない）。head が無い biblio は cache とサーバー由来 chunk を空にし、サーバーで削除されたログをキャッシュから復活させない。live echo だけは空 biblio 応答との race を避けるため保持する。
- `SceneGraphTab` の `movements` は、ライブの `CharacterMoved` と `character-came` / `character-gone` system record から再構築した履歴を `reconcileMovements` で統合する。came の scene を到着先、直前の gone の scene を出発元として `fromSceneId` を復元し、timestamp が無い system event は `(chunkIndex, position)` の順で並べる。同じ character / route の live move は log record が読み込まれた時点で二重計上しない。再入室・再接続後も、読み込み済み（キャッシュ含む）の came/gone から graph が復元される。

### BGM

- BGM は state とログの両方に書く。track は `std::bgm`（slot 1、旧来の互換）/ `std::bgm<N>`（任意の slot N >= 2）に `{ tinId, mimeType, name }` の JSON Tin で保存する。旧形式の裸の 64 hex 文字列も受理し、MIME 不明時は `audio/mpeg` を使う。再生状態は `bgm::<slot>::playing`（boolean）、音量は `bgm::<slot>::volume`。
- 停止は `bgm::<slot>::playing=false` だけを書き、選択中の track は残す（再開できる）。track の削除は「NONE」を選んだときだけで、`std::bgm<N>` を削除する。
- slot は 1〜3 固定ではなく任意番号を扱う。`StateSnapshot` / `StateUpdated` に未知の slot 番号が現れたら `bgmSlots` に追加して表示する。`SceneSettingsTab` の「スロット追加」も最大 slot + 1 を使う。
- `SceneSettingsTab` は再生/停止・選択・音量変更を async 化し、再生/停止は `bgm::<slot>::playing`、選択は `std::bgm<N>`、停止/選択変更は `requestChangeBgm` の ack が揃ったときだけローカルの `bgmSlots` / `scene.juices` を確定する。送信中は対象 slot の「BGM選択」アイコンを波線 `CircularProgress` にし、二重実行を防ぐ。
- 再生は `utils/audio.ts` の `AudioManager` が担う。slot ごとに `HTMLAudioElement` を 1 つだけ保持し、`downloadTin(tinId, roomKey)` で復号した payload から object URL を作る。`StateSnapshot` / `SceneInfo` / `bgm::<slot>::playing` 更新 / scene 入場で再生を調整し、停止・track 変更・scene 切替・unmount では pause して object URL を revoke する。同じ snapshot の再適用は冪等で、再生や download を重複させない。
- 音源取得 / 復号に失敗した slot は row 付近にエラーを表示し、`playing` を確定しない（state を書かない）。`play()` が autoplay policy で拒否された場合は「クリックして有効化」を出し、クリックで同じ要素を再生し直す。
- 失敗時は変更前へ戻し、slot 付近にエラーを出す。キューには積まない（最終状態のみが意味を持つため、再接続時の自動再送をしない）。
- `App` は `StateUpdated` / `StateDeleted` の BGM キー（track / playing / volume）を `bgmSlots` と旧 `bgm1..3` へ反映し、現在 scene の音声だけを再生する。state 更新の可否はサーバーの権限判定に従い、`PermissionDenied` はそのまま表示する。viewer も他クライアントが開始した BGM は再生する（viewer の操作制限は #158 / #159 のまま）。

### scene の管理操作

- 「シーンを新規作成」「ログを削除」「シーンを削除」「メンバー移動」は `requestCreateScene` / `requestRewindLog(sceneId, 0, 0)` / `requestDeleteScene` / `requestMoveCharacter` で送り、成功 ack 後にだけローカル state を更新する（楽観更新しない）。
- シーン作成時の表示名は `CreateScene.name` として送り、banquette が `std::name` へ原子的に書く。リネームは `requestUpdateState(sceneId, 'std::name', name)` の `StateUpdated` ack 後にだけローカル名を確定する。サーバーは create / rename で `SceneInfo` を room へ broadcast し、他クライアントは再読み込みなしで名前を更新する。
- ログ削除は `RewindLog(0, 0)` で全ログを切り詰め、`LogEventRewound` 後にローカルの chunk をクリアする。
- 送信中は押したアイコンを波線 `CircularProgress` にし、当該操作を二重実行できない。失敗時は管理領域の下に中央寄せで「サーバーと通信できませんでした」を出す。
- 接続中は ack 後にだけローカル state を更新する。**未接続時は scene 作成 / 削除 / ログ削除 / メンバー移動を成功扱いにせず**、ローカル一覧を変更せずに同じエラーを表示する（ローカル専用フォールバックは持たない）。`SceneList` broadcast は scene 一覧の権威として反映し、`SceneDeleted` は第二の防護として該当 scene を除く。

### キャラクター同期

- キャラクタータブは表示時に `queryListCharacters` を発行し、問い合わせ中は contained `LoadingIndicator` を出す。タイムアウト時は `OfflineQueueNotice kind="character"` を出し、キャッシュ済み一覧は保持する。
- `CharacterList` / `CharacterInfo` は `tinMapToCharacter` で全属性を decode する。character tab と field が同じ statuses / parameters / chatPalette / icon / color / piecePosition を表示する。
- `CreateCharacter` / `UpdateCharacter` 成功時、banquette は room 全体へ `CharacterInfo { character_id, data }` を broadcast する。`App` は `CharacterInfo` を id で upsert する（ack と broadcast の重複は冪等）。`DeleteCharacter` の `CharacterDeleted`、`MoveCharacter` の `CharacterMoved` も同様に全クライアントへ届き、`App` は一覧・`scene.members` を更新する。
- `scene.members` は `SceneInfo` / `SceneList` の `present_characters` を正として再構築する。reload / reconnect 後もサーバーが永続化した在籍情報から復元され、`character::<id>::scene` のような write-only mirror は使わない。
- 保存は ack 後にだけ反映する。作成は `CharacterCreated` のサーバー採番 ID を使う。保存中は編集パネルを `aria-busy` + 非活性にし、保存アイコンを波線 `CircularProgress` にする。成功時のみ閉じる。
- タイムアウト時は `kind: 'character'` として outbox に積み、再接続 flush で送信して成功後に一覧へ反映する。`PermissionDenied` はキューせずパネルを開いたままエラー表示する。削除も ack 後に行う。

### 駒の同期

- 駒の配置・位置は scene state の `piece::<id>::placed` / `piece::<id>::pos` が正。`App` は `StateSnapshot` / `StateUpdated` / `StateDeleted` を適用し、`characters` の `placedAsPiece` / `piecePosition` に反映する。他クライアントの駒も同じ state から描画される。
- 配置 toggle と drag 後の位置確定は `requestUpdateState` の ack 後にだけローカルへ確定する。drag 中はフィールド描画を即時追従させ、同じ駒の連続 drag は coalesce して最後の位置だけを送る（fire-and-forget しない）。
- 未接続時は配置 / 位置変更を成功扱いにせず、`piece-action-error`（「ルームに接続していないため、駒を操作できません」）を表示して drag 前の位置へ戻す。ack が失敗した場合も同様に戻す。
- character 削除時は banquette が `piece::<id>::*` を best-effort で削除し、`StateDeleted` を broadcast する。これにより他クライアントに操作可能な幽霊駒が残らない。

### 拡張機能

- メタデータは `/` scene の state key `extension::<id>` に `{ id, name, description, icon, priority, enabled, tinId }` の JSON Tin で保存する。wasm 本体は cellar に RoomKey 暗号化で置き、`tinId` だけを state に持つ（4KiB 制限を守る）。
- **一度だけの seed とサーバー正**: 部屋 root state の `extension::__seeded` を seed 済みマーカーとする。マーカーが無い部屋では、拡張機能タブを開いた GM がサーバーに無い公式既定拡張のメタデータを `requestUpdateState` で書き、最後にマーカーを書く（途中で失敗するとマーカーが無いため、再接続時に残りを seed し直す）。既存 room もマーカーが無ければ移行として一度だけ seed し、すでにサーバーにあるメタデータは上書きしない。GM 以外は seed せず、公式既定を読み取り専用で表示する。
- マーカーがある場合、サーバー state が正。公式既定 id でもサーバーに無ければ削除済みとして扱い、再入室・再接続で再追加しない。マーカーが無い legacy room に限り、サーバーに無い公式既定 id をローカルの wasm ごと保持する。サーバーに無いカスタム id は削除する。同じ id でサーバーの `tinId` が変わった場合だけ wasm を取り直し、それ以外はローカルの `wasmBytes` を保持する。手元に wasm が無い場合は `tinId` から cellar で取得する。
- **wasm 欠落**: `loadDefaultExtensions` は manifest にある wasm の取得・instantiate に失敗した拡張を `wasmMissing: true` で返す。`applyRemoteExtensions` は `tinId` の download に失敗した拡張も `wasmMissing: true` にする。タブは該当行に「wasm がありません」を出し、有効化 Switch を無効にする（並べ替え・削除は可能）。`ExtensionPipeline.executePipeline` は wasm の無い有効拡張を黙って skip せず、`getLastWarnings()` / `onWarning` に警告を残す。送信時 pipeline の警告はフッター上部の `pipeline-warning` に表示し、原文（ext-io Tin）はそのまま送信する。
- タブ表示時に `queryState('/')` を発行し、contained `LoadingIndicator` を出す。
- toggle / delete / reorder / upload は楽観更新せず、`requestUpdateState` / `requestDeleteState` の成功後にだけローカルへ反映する。送信中は該当行のアイコンを波線 `CircularProgress` にする。
- タイムアウト / エラー時はタブ上部に「サーバーと通信できませんでした」を出し、一覧はスクロール・閲覧できる状態を保つ。暗転・`pointer-events: none` をタブ全体に掛けず、toggle / delete / reorder / upload などの mutation 操作だけを `disabled` / `aria-disabled` にする（Issue #149）。再接続で `queryState('/')` を再試行する。
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

`utils/session.ts` が account 検索・部屋権限 CRUD を呼ぶ。`RoomPermissionModal` は M3 Search、Divider、Connected Button Group で viewer / PL / GM を切り替える。ロール変更・追加・削除は API の成功を待ってからローカル一覧へ反映し、失敗時は元の一覧へ戻してエラーバナーを表示する（楽観的に確定しない）。`CommonHeader` の「部屋の全てをPLに公開」は `RoomSocket.requestSetDone` の `DoneUpdated` ack 後にのみダイアログを閉じ、`SetDoneFailed` / `PermissionDenied` / timeout ではエラーを表示して開いたままにする。`App` は `isViewer` を `SceneSettingsTab`（シーン作成・削除・メンバー移動）、`SceneGraphTab`（シーン削除）、`CharacterManagementTab`（作成・編集・削除・駒配置）、`ExtensionManagementTab`（追加・並べ替え・削除・toggle）へ渡し、viewer では該当操作を無効化する。viewer の場合はフッターの送信・ダイス・scene 変更も無効化する。入室拒否時は `GateScreen` を出し、右上の `AccountMenu` からサインイン・アカウント切り替え・サインアウトを行える。

## 既知の差分・制約

- `contentTin` は送信時に一度だけ生成する。pipeline を実行できない場合（実行エラー）は `content` のみを保存し、表示は従来どおり render 時 pipeline にフォールバックする。wasm の無い有効拡張は適用せず、`pipeline-warning` に表示して ext-io Tin をそのまま `contentTin` にする。
- システムイベント（BGM / 背景 / 入退場）は banquette の Tin に timestamp を持たないため、ログ上の時刻は 0（1970）になる。kitchen は前後の発言から補間する。scene graph は `(chunkIndex, position)` の順で並べ、表示時刻は 0 のままになる。
- アセットは cellar へ実アップロードするが、`personal` のメタデータは localStorage のみで、他クライアントとは共有されない。アセット本体は RoomKey で暗号化して cellar に置くが、maitre-d proxy と cellar は `Bytes` でリクエストを全量メモリにバッファする（64 MiB 上限）。大きなファイルほどメモリを消費する。
- アウトボックス flush は at-least-once。ack を受け取れずにタイムアウトした操作は再送されるため、サーバー側で適用済みの場合は重複する可能性がある（冪等キーは持たない）。
- 拡張機能タブは開いた時点の `queryState('/')` を正とし、開いている間の `StateUpdated` broadcast は反映しない（再接続・再表示で取り直す）。
- 公式既定拡張の削除の永続性は `extension::__seeded` マーカーに依存する。マーカーは GM が拡張機能タブを最初に開いたときに書かれるため、それ以前に別クライアントが削除した公式既定は seed で復活し得る（seed は「サーバーに無い公式既定を書く」ため）。サーバー正のマージも拡張機能タブを開いた時点で適用され、タブを開く前の送信 pipeline は App のローカル既定を使う。
- タイムアウトは 8 秒で、権限問い合わせのようにサーバーが黙って破棄する操作では唯一の失敗シグナルになる。`PermissionDenied` が返らない経路はタイムアウトとして扱う。
- Saignant WASM は v2 で character 名を受け取る。v2 をエクスポートしない古い wasm を読み込んだ場合だけ mention が `Anonymous` になる。committed artifact は `scripts/build-saignant-wasm.sh` で更新する。
- 開発エントリ（`main.ts`）は `isAdmin` を渡さない。管理者は `GET /api/rooms/{id}` の `role === 'gm'` から決まる。`isAdmin` prop はテスト / プレビュー用の明示 override で、既定は false。`initialRoom` prop も同じくテスト / プレビュー用で preflight をスキップする。
- 入室 preflight のネットワーク / 5xx 失敗は 2 秒間隔・最大 5 回で打ち切り、`OfflineIndicator` の再試行導線に合流する。403 / 401 / 404 は再試行しない。401 は `GateScreen`（サインイン導線）として扱う。
- 部屋鍵は URL フラグメントに置く。サーバーへ送らない設計だが、ブラウザ履歴や共有 URL には残る。`App` は `roomKey` prop → `currentRoom.url` のフラグメント → `window.location.hash` の順に解決し、64 hex を検証する。鍵が無い場合、アセット・拡張 wasm の cellar upload は試行せずエラーを表示する。
- ホーム画面のテーマ設定 UI は Issue #116 で削除した。現在のテーマ選択はコントロールパネル共通ヘッダーの gear menu にあるパレットのみで、light / dark / system の切替 UI は持たない。
- OAuth の `redirect_uri` は現在のパス（クエリ・フラグメントを除く）を使う。Google Cloud Console 側でこの URL を登録する必要がある。`state` は sessionStorage に保存した値と照合し、一致しない交換は行わない。
- `AccountMenu` は account ごとに `GET /api/account/{id}` を呼ぶ。プロフィール取得に失敗してもサインイン状態は維持し、イニシャル表示にフォールバックする。
- 入室可否は `GET /api/rooms/{id}` の preflight を正とする。`isForbidden` は WebSocket の `Error` frame / close code でもフォールバックとして立つが、権限一覧 API の結果からは導出しない（権限一覧は `RoomPermissionModal` の表示にだけ使う）。`room_permissions` に行があり `done` が true の部屋でも、preflight が `viewer` を返せば入室できる。
