# alhalte-argenterie

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `web/alhalte-argenterie`（パッケージ名 `@alhalte/argenterie`）  
> テスト: `web/alhalte-argenterie/tests/*.test.ts`

## 責務

Alhalte の公式 Web クライアント（cutlery）。Svelte 5 と Noph UI を用いて部屋 UI を構築する。部屋への入室、シーン・キャラクター・ログの表示と操作、拡張機能の管理と実行、Saignant WASM によるログ描画、およびオフライン時の操作キュー通知を担う。

クライアント側のローカル状態・ストレージ管理（シーン単位の State 管理、シーン単位の Log 管理、`cellar` との Tin 送受信・メディア再エンコード・キャッシュ管理）は `@alhalte/pantry` に委譲し、`argenterie` は `pantry` の状態変更通知を購読して UI と拡張機能パイプラインへ反映する（Issue #301 / #302）。画面デザインは Material 3 Expressive に準拠し、グローバルな接続状態バナーは持たず、通信失敗は操作したタブやエディタ内で通知する（Issue #261）。

## 公開 API / 外部接続点

### HTTP

| 接続先 | 内容 |
| --- | --- |
| `maitre-d` `POST /api/rooms` | 認証必須。`{ name, room_id? }` で部屋を登録し、作成者を `admin` に設定する（`utils/home.ts` の `createRoom`）。クライアントで生成した 16 文字 hex の `room_id` を送信できるが、最終的な部屋 ID はサーバーの返却値を正とする。 |
| `maitre-d` `GET /api/rooms/{room_id}` | 入室前の preflight。部屋名と呼び出し元の `role`（`admin` / `participant` / `viewer` / `denied`）を返す（`utils/home.ts` の `fetchRoomInfo`）。`denied` は HTTP 403 で返るため、403 は `GateScreen`、404 は not-found 画面として WebSocket 接続前に判別する。旧 `done` フラグは Issue #200 で廃止されており、レスポンスに含まれていても UI では参照しない。 |
| `maitre-d` `/api/accounts/search` | アカウント検索。 |
| `maitre-d` `/api/account/{account_id}` | アカウント情報の取得・更新。アカウントメニューの表示名やアイコンの取得に使用する（`utils/account.ts`）。 |
| `maitre-d` `GET /auth/login?return_to=...&prompt=...` | サインインの入口。フラグメントを除いた絶対 URL を `return_to` に指定して遷移する（`utils/account.ts` の `beginSignIn`）。OAuth の `state` と `redirect_uri` は `maitre-d` 側で管理する。 |
| `maitre-d` `GET /auth/google/callback` | Google OAuth の固定コールバック。ブラウザが直接アクセスし、`maitre-d` が `session_id` Cookie を設定したうえで `return_to` へ 302 リダイレクトする（フロントエンドは直接関与しない）。 |
| `maitre-d` `GET /api/session` | サインイン完了後に Cookie から `account`（`AccountResponse`）と `session_id` を取得する（`utils/account.ts` の `completePendingSignIn`、Issue #294）。 |
| `maitre-d` `/api/room/{room_id}/export/zip?type=full\|logs` | 部屋データ・ログの ZIP エクスポート（`CommonHeader.svelte`）。 |
| `maitre-d` `/api/cellar/store`、`/api/cellar/store/{tin_id}`、`/api/cellar/tin/{tin_id}` | 認証付き `cellar` プロキシ。`pantry` の Tin 管理（`get` / `put`）および `utils/cellarTransfer.ts` を通じてアセットや拡張機能 WASM の Tin を送受信する。事前確認（store check）は `200 OK` とレスポンスボディの `result`（`upload_required` / `deduplicated` / `already_owned`）で判定し、`upload_required` のときのみ本体を `PUT` する。ハンドシェイクのボディは `{ tin_id, size, wrapped_key }` のみとし、`owner` / `session_id` は送信しない（`maitre-d` がセッションから付与するため、クライアントから送ると 400 になる）。進捗通知には `upload.onprogress` を用い、失敗時は型付きの `CellarTransferError` を返す。 |
| Vite dev proxy | 開発時に `/api`・`/auth`・`/room` を `http://127.0.0.1:8080` へ、`/ws` を `ws://127.0.0.1:8080` へ転送する。`/auth` はサインイン開始（`/auth/login`）とコールバックの双方で使われ、`maitre-d` の `--public-url` を Vite の dev オリジンに合わせることで Cookie が UI と同一オリジンに設定される。 |

### WebSocket と `pantry` 連携

`utils/roomSocket.ts` の `RoomSocket` が `/ws/room/{roomId}?account_id=...&session_id=...` へ接続し、`ClientFrame` の送信と `ServerFrame` の受信を JSON で行う。ハートビート ping には自動で `HeartbeatResponse` を返す。シーン単位の State と Log の状態管理は `pantry` が担い、`RoomSocket` で送受信される State / Log イベントと連携する（Issue #302）。

- **URL 構築**: `buildRoomWebSocketUrl` は招待ページ URL（`https://host/rooms/<id>#<key>`）から **オリジンのみ** を取り出し、`ws(s)://host/ws/room/<id>` を組み立てる（`/rooms/<id>` などのパスは引き継がない）。明示的な `ws(s)://.../ws/room/...` のみをそのまま採用し、それ以外の `ws(s)` URL もオリジンから再構築する。
- **部屋 ID 検証**: `utils/home.ts` の `parseRoomUrl` は 16 文字の 16 進文字列（または移行用の 10 進数値文字列）のみを受理し、`room-1` のような任意文字列は `null` として拒否する。`generateRoomUrl` は 16 文字の 16 進文字列を生成する。
- **切断通知と再接続**: `RoomSocket` は `onclose(event)` を `onClose` コールバックと `getLastCloseEvent()` で公開する。クローズコード `1008` / `4403` / `4404` は再試行不能とみなして自動再接続を停止し、`App` はこれらを `denied`（`GateScreen`）として反映する。入室前の HTTP 403（`denied`）および 404 は `GET /api/rooms/{id}` の preflight で判別する。
- **権限フレーム（Issue #201）**: 権限表は HTTP ではなく WebSocket で操作する。`ClientFrame::QueryPermissions`（応答は `PermissionsSnapshot`）、`SetMemberPermission { account_id, permission }`、および `SetDefaultPermission { permission }`（成功時は `PermissionsUpdated`、失敗時は `SetMemberPermissionFailed` / `SetDefaultPermissionFailed` の `Error`）を `requestQueryPermissions` / `requestSetMemberPermission` / `requestSetDefaultPermission` で送信する。権限レベルは `banquette` の Serde 表記（`Admin` / `Participant` / `Viewer` / `Denied`）で送受信し、UI 側の小文字ロール名とは `roleToPermissionLevel` / `permissionLevelToRole` で相互変換する。応答（ack）は要求元にのみ返り、部屋全体へはブロードキャストされない。
- **Tin エンコード**: `banquette` が `Tin` 形式を要求するフィールド（`AppendLog.entry` やキャラクターの `data`）は、`jsonToTin` / `logRecordToTin` / `characterToTinMap` で Tin JSON `{ label, payload }` に変換してから送信する。`label` には `pantry` の `labelHash` による用途別定数（`LABEL_LOG_RECORD`、`LABEL_JSON` など）を用いる。受信時は `tinToJson` / `tinToLogRecord` / `tinMapToCharacter` でデコードし、`CharacterList` / `CharacterInfo` でも全属性（`statuses` / `parameters` / `chatPalette` / `icon` / `color` / `piecePosition` など）を復元する。
- **Promise API と ack 照合**: 更新系の `request*` と参照系の `query*` は Promise を返す（既存の void API である `appendLog`、`updateState`、`createCharacter` なども併用可能）。
  - `AppendLog` はペイロードに `clientOpId` を埋め込み、同一 ID を持つ `LogEventAppended` のみで resolve する（他クライアントの発言エコーでは resolve しない）。
  - `UpdateState` / `DeleteState` は `scene_id` + `key`（更新時は値も比較）で照合する。
  - キャラクターの CRUD は `CharacterCreated` / `CharacterUpdated` / `CharacterDeleted` を FIFO 順で照合する（UI 側で同時操作を直列化する前提）。
  - `CreateScene` / `DeleteScene` / `MoveCharacter` は `SceneCreated` / `SceneDeleted` / `CharacterMoved` で resolve する。
  - `RewindLog` は `LogEventRewound` で resolve する。
  - `Error` フレームは対応する `*Failed` コードおよび `PermissionDenied` で reject する。`SpeakerNotOwned`（話者の所有権違反、Issue #271）は再試行不能として扱い、送信保留キューには積まない。
  - 応答がない場合は `DEFAULT_OPERATION_TIMEOUT_MS = 8000`（8 秒）で `OperationTimeoutError`（`code: 'timeout'`）として reject し、保留中の待機エントリを確実に破棄する。サーバー由来の失敗は `ServerOperationError`（`code` 付き）で返す。
- **`ServerFrame` のディスパッチ**: `StateSnapshot` / `LogBiblioSnapshot` / `LogChunkSnapshot` / `CharacterInfo` / `LogEventRewound` / `SceneCreated` / `SceneDeleted` / `CharacterMoved` を含む各フレームをディスパッチし、`pantry` の State・Log 管理および UI の個別コールバックへ流す。
- **`SceneList` / `SceneInfo`**: `SceneList.scenes` は `SceneSummary`（`id`、`name`、`present_character_count`、`present_characters`、`active_client_count`、`is_loaded`）の配列であり、旧サーバーの裸の ID 配列も受理する。`present_characters` はそのシーンに在籍するキャラクター ID の昇順リストであり、`App` はこれを正として `scene.members` を再構築する（リロードや再接続後も復元される）。`RoomSocket` は ID 列を `getScenes()`、サマリ列を `getSceneSummaries()` に保持し、`onSceneInfo` でシーンの作成・改名・在籍変更のブロードキャストを通知する。
- **State の同期と `StateSnapshot` ハイドレーション（Issue #301 / #302）**: `pantry` がルートシーン `/` および各シーンの State を保持・管理する。入室時およびシーン入場時（`EnterScene`）の `StateSnapshot`、ならびに常時届く `StateUpdated` / `StateDeleted` を `pantry` が受け取り、変更通知を通じて `App` と各タブへ即座に反映する。
  - ルートシーン `/` の `extension::<id>` / `extension::__seeded` / `asset::<id>` は、拡張機能タブを開いているか否か、また現在どのシーンを表示しているかにかかわらず入室中常時同期される。管理者が拡張機能の ON/OFF や優先度を変更すると、`pantry` からの State 変更通知によって参加者の `extensions` と送信用の `ExtensionPipeline`、および開いている拡張機能タブの表示が即座に更新される（Issue #301）。
  - 各シーンの State（`std::name`、`std::bgm<N>`、`bgm::<slot>::playing`、`bgm::<slot>::volume`、`bgm::<slot>::started_at`、`bgm::<slot>::paused_at`、`bgm::<slot>::removed`、`std::b-img*`、`view::transform`、`piece::<id>::*`）も `pantry` から冪等に適用される。BGM 関連キーのスナップショット適用時は、辞書順に左右されないよう `removed` → トラック（`std::bgm<N>`）→ タイムスタンプ（`started_at` / `paused_at`）→ `playing` / `volume` の順で処理する。再接続やシーン切り替え時にも同じスナップショットが安全に再適用される。

### WASM

- `utils/saignant.ts`: `/wasm/alhalte_saignant.wasm` を読み込み、`render_generast_abi_v2`（存在する場合）または `render_generast_abi` と `get_full_css_abi` を呼び出す。現在の `characters` から `{ "<id>": "<name>" }` の名前マップを構築して v2 ABI に渡し、メンションや `fluct` / `assign` 内のキャラクター名を解決する（v2 をエクスポートしない古い WASM では v1 にフォールバックし、名前マップは渡さない）。描画キャッシュのキーにはキャラクター一覧のシグネチャを含め、`CharacterList` / `CharacterInfo` が更新された際は `LogTab` がキャッシュと `renderedContentMap` を破棄して再描画する。Vitest 実行時は `node:fs` で実ファイルを読み込む。
- `wasm/runner.ts`: 拡張機能 WASM の `alhalte_abi_version`（1 または 2）、`alloc`、`dealloc`、`process_generast`、および任意の `self_introduction` を呼び出す。ABI 1 は 3 要素タプル（ID なし）、ABI 2 は 4 要素タプル `(id, name, icon_svg, description)` をデコードする。ABI 2 の自己紹介 ID がマニフェストの ID と一致しない WASM は `wasmMissing: true` として扱い実行しない。`ExtensionPipeline` は有効（`enabled: true`）な拡張機能を `priority` 昇順に適用する。
- 開発時のビルドと配置には `nix run .#saignant-wasm`（または `nix develop -c ./scripts/build-saignant-wasm.sh`）を用いる。`public/wasm/alhalte_saignant.wasm` はリポジトリへのコミット対象であり、Rust 側の `abi::ABI_VERSION` と一致することを `crates/alhalte-saignant/tests/artifact_test.rs` で検証する。

### 主な画面・コンポーネント

| 領域 | コンポーネント |
| --- | --- |
| アプリ全体 | `App.svelte` |
| ホーム | `Home/HomeScreen.svelte`、`utils/home.ts`、`utils/recentRooms.ts` |
| フィールド | `Field/FieldPanel.svelte`、`Field/FieldPiece.svelte`、`lib/components/BoardCanvas.svelte` |
| コントロール | `Control/ControlPanel.svelte`、`CommonHeader.svelte`、`CommonFooter.svelte`、`SceneSwitcher.svelte` |
| タブ | `LogTab`、`SceneGraphTab`（グラフ / リスト）、`SceneManagementTab`（`SceneSettingsTab`）、`CharacterManagementTab`、`ExtensionManagementTab` |
| モーダル | `AssetPickerModal`、`CharacterEditModal`、`SceneMoveModal`、`RoomPermissionModal`、`GenericImagePicker`、`IconImagePicker` |
| アカウント | `Control/AccountMenu.svelte`、`utils/account.ts` |
| 門前払い | `Gate/GateScreen.svelte` |
| 接続状態 | `utils/connection.ts`（グローバルバナーは持たない） |

横長画面では左側にフィールド・右側にコントロールパネルを配置し、縦長画面ではフィールド全面表示＋ボトムシート構成とする。コントロールパネルはサイドシートとボトムシートの 2 形態および開閉状態を持つ。縦長画面のボトムシートはドラッグハンドルや半開状態を持たず、開くと常に全開（ビューポート高さから noph-ui の上部マージンのみを引いた高さ）となる（Issue #262）。

## データ形式・状態

### 主要な型

- `VisitedRoom` / `RecentRoom`
- `Character`（`statuses`、`parameters`、`chatPalette`、`hideStatus`、`isNarrative`、`placedAsPiece`、`piecePosition`）
- `Scene`（`juices`、`viewTransform`、`members`）
- `LogRecord`（`content`、`speaker`、`speakerId`、`timestamp`、`chunkId`、および任意の `chunkIndex` / `position` / `contentTin` / `kind` / `event` / `cached`）
- `ExtensionItem`（`wasmBytes`、`priority`、`enabled`、`cellarTinId`）

### ローカル保存と `pantry` ストレージ（Issue #302）

- **`pantry` による管理（State / Log / Tin）**:
  - **Scene State**: シーンごとの State を `pantry` が保持し、変更を `argenterie` へ通知する。
  - **Scene Log**: シーンごとの `biblio`（確定済み `frozen chunk` の `TinId` 列）、`active chunk`、`head`、および通信失敗時の未送信リクエスト保留キュー（pending queue）を `pantry` が管理する。起動時・オンライン復帰時は `biblio`・`active chunk`・`head` をサーバーと同期したうえで保留キューを再送する。
  - **Tin キャッシュとアップロード状態**: 画像（長辺最大 1920px の WebP へ変換）・音声（最大 128kbps の Opus へ変換）・拡張機能 WASM などの Tin を `pantry` のキャッシュアダプタ（CacheStorage / IndexedDB / Memory）に保持し、`"putted"` / `"put-failed"` の状態を追跡する。オンライン復帰時には `"put-failed"` の Tin を自動再送する。
- **`localStorage`**: アカウント ID、セッション ID、訪問済み部屋、最近の部屋、個人・部屋アセットのメタデータ（`alhalte_pantry_assets`）、部屋ログのローカルキャッシュ（`alhalte:room-logs:<roomId>`）、テーマ設定（`alhalte:theme`）、共通フッターのダイスピッカー表示設定（`alhalte:dice-picker-visible`、`'true'` / `'false'`、既定値は表示。読み書き失敗や不正値も表示として扱う）を保存する。
- **IndexedDB `alhalte-argenterie-outbox`（ストア名 `outbox`）**: 送信できなかった `message` / `character` 操作（型定義上は `state` / `asset` も許容）を `{ id, roomId, kind, createdAt, payload }` 形式で永続化する。`createdAt` から `OUTBOX_TTL_MS = 30 日` を超えたエントリは読み出し時に破棄（prune）する。IndexedDB が利用できない環境ではメモリストアへフォールバックする。
- **Cookie**: `session_id`（`SameSite=Lax`）。
- **`sessionStorage`**: サインイン遷移前の完全な戻り先 URL（クエリパラメータと部屋鍵フラグメントを含む）。サインイン処理中のみ保持し、`completePendingSignIn()`（`/api/session`）完了時に削除する。OAuth の `state` と `redirect_uri` は `maitre-d` が管理するためクライアントでは保持しない。
- **URL フラグメント**: 32 バイト（64 hex）の部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を生成する。

## 内部フロー

### 入室

1. `main.ts` が `App` をマウントする。管理者権限の有無は props の `isAdmin` ではなくサーバーが返す `role` から決定する。
2. `App.svelte` は `resolveRoomFromLocation` で現在の URL から部屋 ID と部屋鍵を解決する。訪問済み部屋（`VisitedRoom` / `RecentRoom`）の保存時は、URL フラグメントの部屋鍵（`#<roomKey>`）が失われないよう保持する（Issue #298）。
3. WebSocket を開く前に `GET /api/rooms/{room_id}` で入室 preflight を行う。成功時は返却された `name` を `currentRoom.name` に、`role`（4 段階）を `userRole` に反映してから `RoomSocket` を生成する。HTTP 403（`denied`）および 401 は `GateScreen`、404 は not-found 画面を表示し、ネットワークエラーや 5xx は 2 秒間隔・最大 5 回の自動再試行後に再試行ボタン付きエラー画面を表示する（403 / 401 / 404 は自動再試行しない）。
4. preflight が成功したら `RoomSocket` を生成し、アカウント ID とセッション ID をクエリに付与して接続する（`initialRoom` prop が指定されたテスト・プレビュー時は preflight と `userRole` の上書きをスキップする）。
5. 接続後、`pantry` に部屋の `roomKey` を設定し、ルートシーン `/` と現在のシーンの State および Log を同期する。`ServerFrame` の `SceneList`、`SceneInfo`、`CharacterList`、`CharacterInfo`、`StateSnapshot`、`StateUpdated`、`StateDeleted`、`LogEventAppended`、`LogEventEdited`、`LogEventDeleted`、`LogEventRewound`、`SceneCreated`、`SceneDeleted`、`CharacterMoved`、`CharacterDeleted` などを `pantry` および Svelte のリアクティブ状態へ反映する。
   - シーン名・BGM・背景画像・視点・駒配置・拡張機能設定は `pantry` が管理する Scene State（`StateSnapshot` / `StateUpdated` / `StateDeleted`）を正とする。
   - `scene.members` は `SceneList` / `SceneInfo` の `present_characters` を正とする。
   - キャラクター一覧は `CharacterList` / `CharacterInfo` / `CharacterDeleted` / `CharacterMoved` を正とする。
6. WebSocket の `Error` フレームや再試行不能なクローズコード（`1008` / `4403` / `4404`）を受信した場合も、`denied`（`isForbidden`）のフォールバックとして `GateScreen` を表示する。

`RoomRole` は `admin` / `participant` / `viewer` / `denied` の 4 段階であり、`utils/session.ts` の `roomRoleCapabilities` から `isAdmin` / `isParticipant` / `isViewer` / `isDenied` と各操作権限（`canSend` / `canManageScenes` / `canManageCharacters` / `canManageExtensions` / `canManageRoom`）を導出する。`isAdmin` は `effectiveRole === "admin"`、`isViewer` は `viewer` のみであり、`participant` を `viewer` 扱いにすることはない。`VisitedRoom.isAdmin` は `localStorage` の後方互換のためにのみ保存し、実際の権限判定には使用しない。

### 部屋作成

1. ホーム画面の「部屋の新規作成」ダイアログでは、表示用に 16 文字 hex の `room_id` と 32 バイト（64 文字 hex）の部屋鍵をクライアント側で生成する。
2. 「入室する」を押すと `POST /api/rooms` に `{ name, room_id }` を送信する。未サインイン（アカウント ID またはセッション ID がない）の場合はリクエストを送らず、ダイアログ内にサインイン導線を表示する。
3. リクエストが成功したときのみ、サーバーから返却された `room_id` とクライアントで生成した部屋鍵から招待 URL（`/rooms/<roomId>#<roomKey>`）を組み立て、「最近の部屋」へ保存して入室する。
4. HTTP 401 はサインイン導線、403・通信失敗・その他の HTTP エラーはダイアログ内のエラーメッセージとして表示し、ローカルのみでの部屋作成や入室は行わない（ダイアログを閉じずにそのまま再試行できる）。

### ローカルログイン画面とサインイン復帰（Issue #294）

`maitre-d --local-login` が有効な環境では、`GET /auth/login` は SPA の `/login?return_to=...` へ 302 リダイレクトする。`main.ts` は `pathname === '/login'` を検出すると `LoginScreen` をマウントし、この時点では `completePendingSignIn()` を呼ばない（未ログイン状態の 401 応答によって `sessionStorage` 内の戻り先 URL と部屋鍵フラグメントが消去されるのを防ぐため）。

- 画面は noph-ui の `Card` / `TextField`（アカウント名、および `type="password"`）/ `Button`（filled）で構成し、独自の生 `<input>` / `<button>` は使用しない。
- `POST /api/auth/local/login` に `{ account_name, password }` を JSON で送信し、成功時は `return_to`（同一オリジンのみ許可し、クロスオリジンは `/` にフォールバック）へ遷移する。
- HTTP 401 は「アカウント名またはパスワードが違います」、400 はサーバー側の入力エラー、404 は「ローカルログインが無効」、その他は汎用エラーをフォーム内の `aria-live` 領域に表示する。
- 遷移後、アプリのマウント時に `completePendingSignIn()` が `/api/session` を Cookie 付きで呼び出し、アカウント ID・セッション ID と部屋鍵フラグメントを含む元の URL を復元する。サインイン開始・復帰の関数名は認証方式に依存しない `beginSignIn` / `completePendingSignIn` に統一されており、Google OAuth とローカルログインの双方で共通のセッション復帰フローとして動作する（Issue #294）。

### 本番 SPA ビルド

`vite.config.ts` はライブラリビルド用であり `index.html` を出力しない。ポータブル版・`nix run .`・`maitre-d --web-root` 向けには `vite.config.web.ts` と `pnpm run build:app` を使用し、`dist-app/`（`index.html`、アセット一式、`public/` 配下の WASM）を生成する。`maitre-d` は `ServeDir` と `index.html` フォールバックによって `/login` や `/rooms/<id>` を配信する。

### 門前払い画面（`GateScreen`）

ロールが `denied` の場合（preflight の HTTP 403、`role: "denied"` の応答、または WebSocket の `1008` / `4403` / `4404` 切断）、フィールドやコントロールパネルの代わりに全画面で `GateScreen` を表示する。

- 中央見出し: 「アカウントに入室権限がありません」
- 説明文: 「部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？」
- 主操作ボタン: 「サインイン・アカウント切り替え」。すでにサインイン済みの場合はローカルセッションを破棄してから、`prompt=select_account` 付きのサインイン URL（`beginSignIn`）へ遷移する。
- 右上: `AccountMenu`（サインイン・アカウント切り替え・サインアウトが可能）。
- 副操作ボタン: 「ホームに戻る」（`isForbidden` を解除して `/` へ遷移する）。

### アカウント UI / サインインフロー（Issue #294）

1. `AccountMenu` はアカウント ID とセッション ID の両方が保存されている場合のみサインイン済みとみなす。
2. サインイン済みの場合は `GET /api/account/{account_id}` で表示名とアイコンの `TinId` を取得し、32 バイトの `TinId` から `/api/cellar/tin/<hex>`（または `pantry.get`）でアバター画像を表示する。取得中または取得失敗時はイニシャルまたは `account_circle` アイコンを表示する。
3. アバターを押すと M3 の `Menu` が開き、「別のアカウントでログイン」と「サインアウト」を選択できる。サインアウト時はアカウント・セッション情報（`alhalte:account-id` / `alhalte:session-id` および `session_id` Cookie）とサインイン用 `sessionStorage` を消去する。端末ごとの表示設定（`alhalte:theme` / `alhalte:dice-picker-visible`）および URL の部屋鍵フラグメントは保持する。
4. 未サインイン時は Filled tonal ボタンの「サインイン」を表示する。
5. `beginSignIn` は現在の完全な URL（クエリと `#<roomKey>` フラグメントを含む）を `sessionStorage` に保存し、フラグメントを除いた絶対 URL を `return_to` に指定して `GET /auth/login` へ遷移する（アカウント切り替え時は `prompt=select_account` を付与する）。`return_to` のオリジン検証・`state` 発行・OAuth プロバイダへの `redirect_uri` 指定はすべて `maitre-d` の責務であり、外部認可サーバーに `argenterie` のオリジンやポートが直接渡ることはない。
6. コールバックまたはローカルログイン完了後、`maitre-d`（または `LoginScreen`）は `return_to` へ遷移させる。アプリ起動時（`main.ts`）に `sessionStorage` に戻り先 URL が保存されていれば、`completePendingSignIn()` が `GET /api/session` を Cookie 付きで呼び出し、アカウント ID とセッション ID を保存したうえで `history.replaceState` により元の URL（部屋鍵フラグメントを含む）を復元する。401 の場合は未サインイン状態のまま保存済みの戻り先 URL を消去する。
7. `AccountMenu` はホーム画面の AppBar 右端、フィールド画面の右上、および門前払い画面の右上に配置する。テーマ設定はコントロールパネル共通ヘッダーのギアメニューに集約している。フィールドの `.field-container` は盤面パン操作のために pointer capture を行うが、`field-account-layer` / `field-overlay-layer` およびボタン・入力要素からの `pointerdown` はパンの対象外とし、アバターメニュー等のオーバーレイ操作を阻害しない（Issue #268）。

### 発言とログ表示・Log 管理（Issue #302）

1. `handleSendMessage` は送信前に有効な拡張機能の `ExtensionPipeline` を実行し、入力 `(Generast::String, speaker_id)` の ext-io Tin から得た変換結果を `LogRecord.contentTin` に格納する。パイプラインを実行できない場合は平文の `content` のみを保持し、`pipeline-warning` を表示する。`isSending` を立ててログ追記を要求する（送信中は `CommonFooter` の送信ボタンを波線 `CircularProgress` に差し替え、フッター全体を `aria-busy` で暗転させて二重送信を防ぐ）。
2. シーンごとのログ状態（`biblio`・`active chunk`・`head`）は `pantry` が管理する（Issue #302）。`AppendLog` がサーバーに受理され `LogEventAppended` が返ると、`pantry` が `active chunk` と `head` を更新し（`active chunk` が上限に達した場合は `frozen chunk` として `biblio` に `TinId` を追加し、新しい `active chunk` を作って `head` を `0` にリセットする）、購読している `LogTab` へ通知する。同様にして編集（`EditLog` / `LogEventEdited`）・削除（`DeleteLog` / `LogEventDeleted`）・巻き戻し（`RewindLog` / `LogEventRewound`）も `pantry` を通じて処理・通知される。
3. オフラインやタイムアウト等でリクエストが失敗した場合、`pantry` はエラーとともに未送信リクエストの保留キュー（pending queue）一覧を返す。`LogTab` は現在シーンの保留キューにある未送信レコードを取得済みログの末尾に表示し、その直上に波線 `OfflineQueueNotice kind="message"`（「サーバーと通信できませんでした」「メッセージは次回の接続時に送信されます」、件数付き）を表示する（Issue #148）。入力欄へのテキスト巻き戻しやフッター上部の重複バナー表示は行わない。
4. 起動時およびオンライン復帰時には、`pantry` がサーバーから最新の `biblio`・`active chunk`・`head` を同期したうえで、保留キュー内のリクエストを FIFO 順で再送（flush）する。再送中にエコーとキューが併存しても `clientOpId` / `LogRecord.id` で重複排除する。
5. `LogTab` は `pantry`（および `ChunkLogManager`）の変更通知を購読して未描画のレコードを描画する。送信時に確定した `contentTin`（`alhalte/generast` Tin）があるレコードはパイプラインを再実行せずそのまま Saignant WASM に渡し、旧形式のレコードのみ `buildGenerastFromText` で拡張機能パイプラインを適用する。
6. `renderGenerast` は Saignant WASM で Generast を HTML に変換し、`{@html}` で描画する。現在の `characters` から構築した名前マップを渡すため、本文中のメンションや `fluct` / `assign` も現在のキャラクター名で描画される（Issue #163）。WASM 未初期化時は HTML エスケープしたプレーンテキストにフォールバックする。
7. 入退場システムイベント（`character-came` / `character-gone`）の Tin はラベルに応じて postcard ペイロードをデコードし、`kind: 'system'` の控えめな M3 行として時刻付きで表示する（postcard バイト列を誤って UTF-8 チャット本文としてデコードしない）。
8. **話者の所有権（Issue #271）**: `speakerId` を持たないレコードはナレーションとして誰でも送信できる。`speakerId` を持つレコードは、実在するキャラクターであり、かつ `std::owner_account` が送信者と一致する場合にのみ送信できる（`admin` は全キャラクターを代演可能）。最終的な認可は `banquette` が `AppendLog` / `EditLog` の検査で行い、違反時は `SpeakerNotOwned` を返す。UI はフッターの話者ピッカーに `speakableCharacters`（`admin` は全件、`participant` は自己所有分、`viewer` / `denied` は空）のみを表示し、既定話者もその先頭（なければナレーション）とする。`SpeakerNotOwned` が返った場合は再試行せず、保留キュー内のレコードは一度だけナレーションへ変換して送り、それでも拒否された場合は破棄する。

### チャンクの読み込み（最新・過去）とログキャッシュ（Issue #302）

- シーンごとの `biblio`（確定済み `frozen chunk` の `TinId` ポインタ列）、`active chunk`、および `head` は `pantry` が管理し、`LogTab` の表示時やシーン切り替え時にサーバーの `LogBiblioSnapshot` / `LogChunkSnapshot` と同期する。シーンごとに in-flight 状態を管理し、二重ロードを防ぐ。
- 読み込み中はメッセージ一覧上に contained `LoadingIndicator` を表示し、「ログはありません」とは表示しない。応答後は `(chunkIndex, position)` およびレコード ID で重複排除して反映する。
- 上端スクロール（`handleScrollEdge('top')`）時は、現在読み込み済みの最小チャンク番号の 1 つ前の `frozen chunk`（`biblio` 内の該当 `TinId` または `queryLogChunk`）を 1 回だけ要求する。同一チャンクへの並行要求は in-flight Promise を共有し、最古のチャンク（インデックス 0）まで到達したら `null` を返してそれ以上の要求を行わない。
- 過去チャンクの挿入時は `(chunkIndex, position)` 順に整列し、閲覧中のスクロール位置を維持する（最下部へ強制ジャンプしない）。
- タイムアウトや通信エラー時は `OfflineQueueNotice kind="generic"`（「接続が回復したらログを再読み込みします」）を表示し、オンライン復帰時に `pantry` が再同期する。
- オフライン時にローカルキャッシュ由来のレコード（`cached: true`）のみを表示している間は、`LogTab` に「キャッシュ表示中」と表示する。サーバーから `LogBiblioSnapshot` / `LogChunkSnapshot` が届いたシーンではサーバー側の状態を正としてキャッシュ由来レコードを置き換える（サーバー上で削除・巻き戻しされたログをローカルキャッシュから復活させない）。

### シーングラフの復元・アカウント絞り込み・リスト表示

- `SceneGraphTab` の移動履歴（`movements`）は、ライブの `CharacterMoved` イベントとログ内の `character-came` / `character-gone` システムレコードから再構築した履歴を `reconcileMovements` で統合する。`character-came` のシーンを到着先、直前の `character-gone` のシーンを出発元として `fromSceneId` を復元し、タイムスタンプを持たないシステムイベントは `(chunkIndex, position)` 順に並べる。同一キャラクター・同一経路のライブ移動はログレコード側が読み込まれた時点で重複計上しない。
- **シーングラフのアカウント絞り込み（Issue #263）**: `participant` のグラフ表示では、自身のアカウント ID と `std::owner_account` から保有キャラクターを特定し、その移動履歴のみを用いて既知シーン集合 `V_known` を算出する（`filterOwnedMovements` → `collectKnownSceneIds`）。`V_known` は「保有キャラクターが過去に参加したシーン」に「ルートシーン `/`」「現在のシーン」「保有キャラクターが現在在籍するシーン」を加えた集合であり、毎回動的に計算する。グラフ構造自体には全キャラクターの移動を渡すことで、既知シーン間の遷移や他キャラクターが開拓したフロンティアを見失わないようにしつつ、`V_known` 外への移動はシーン ID と名前を伏せた独立の「？」ノードへの点線エッジとして描画する（同じ実シーンへの複数エッジも束ねない）。`admin` と `viewer` は全シーンへのアクセスが許可されるため、全グラフを表示する。なお、この絞り込みは UI 上の可視性制御であり、最終的なアクセス・送信認可は `banquette` の `can_access_scene` / `can_send_to_scene` が行う。
- **グラフ / リスト切り替え（Issue #273）**: `SceneGraphTab` の上部に `SegmentedButton`（グラフ / リスト、既定はグラフ）を配置する。リスト表示では `allScenes` を `Divider` 区切りで並べ、行クリックでグラフのノードと同じ `onNavigateToScene`（`App.handleSelectScene` → `enterScene`）を実行する。現在のシーンの行は `aria-current` とチェックアイコンで示す。シーン削除ボタン（右寄せのゴミ箱アイコン）は `capabilities.isAdmin && id !== "/"` の行にのみ表示し、グラフ表示と同じ確認ダイアログを経由する。

### メニュー配置

noph-ui の `Menu` は HTML の `popover` 要素であり、ブラウザの UA スタイルシートによって `position: fixed; inset: 0` が付与される。`utils/menuPosition.ts` はアンカー要素の viewport rect から表示座標を計算し、`position: fixed !important` / `margin: 0 !important` / `position-area: none !important` / `position-try-fallbacks: none !important` / `align-self: unset !important` / `justify-self: unset !important` で UA の自動配置を打ち消す。

- `inset` は shorthand プロパティであるため、使用しない辺を `removeProperty()` で消すと author 宣言が失われて UA の `0` が復活し、LTR 環境では `left: 0` / `top: 0` が優先されてメニューが左上（`x=0, y=0`）に潰れてしまう（Issue #241）。
- これを防ぐため、**毎回 4 辺すべてを longhand で明示的に指定する**（固定する辺は `<px> !important`、反対側の辺は `auto !important`）。文字列スタイル生成（`getMenuPositionStyle`）と DOM 直接適用（`applyMenuPosition` の `$effect`）で同一の宣言リストを用い、リサイズやスクロール時の再計算でも 4 辺の指定を維持する。

### BGM / 背景画像と鍵整合性（Issue #298 / #302）

BGM と背景画像はクライアント（`argenterie` / `pantry`）側で規約化した Scene State キーであり、`banquette` はこれらのキーの内部構造には関与しない（Issue #186）。

- **BGM State キー構造**:
  - BGM は Scene State のみで管理し、専用のログイベントは持たない。トラック情報は `std::bgm`（スロット 1、後方互換用）および `std::bgm<N>`（任意のスロット $N \ge 2$）に `{ tinId, mimeType, name }` の JSON Tin として保存する（旧形式の 64 文字 hex 文字列も受理し、MIME タイプ不明時は `audio/mpeg` とする）。
  - **行 = トラックが設定されたスロット（Issue #249）**: `createDefaultJuices` は `bgmSlots: []` で初期化し、`getNormalizedBgmSlots` はトラックが設定されているスロットのみを返す（`bgm::<slot>::removed` の tombstone があるスロットは常に除外する）。「スロット追加」ボタンは $\max(\text{既存行} \cup \text{tombstone}) + 1$ のスロット番号で `AssetPickerModal` を開き、選択後のサーバー ack が返った時点で行を生成する（削除済みスロット番号は再利用しない）。
  - **追加フローの失敗表示（Issue #275）**: まだ行が存在しないスロットでの追加エラー（部屋鍵なし、Tin の取得・復号失敗、`tinId` 欠落、`PermissionDenied` 等の ack 拒否）は、BGM セクションヘッダ直下の `bgm-add-error`（`role="status"`）に 1 つだけ表示する。既存行に対するエラーは従来どおり各行の `bgm-error-<slot>` に表示する。読み取り専用の `viewer` / `denied` では「スロット追加」ボタン自体を無効化する。
  - **再生状態・音量・タイムスタンプ（Issue #250）**: 再生状態は `bgm::<slot>::playing`（真偽値）、音量は `bgm::<slot>::volume`、再生基準時刻は `bgm::<slot>::started_at` / `bgm::<slot>::paused_at`（UNIX ミリ秒整数）に保存し、曲長 $D$ は各クライアントの `HTMLAudioElement.duration` から取得する。
    - 再生中の再生位置: $\text{offset} = (((\text{now} - \text{started\_at}) \bmod D) + D) \bmod D$
    - 一時停止中の再生位置: $\text{offset} = (((\min(\text{now}, \text{paused\_at}) - \text{started\_at}) \bmod D) + D) \bmod D$
    - 再開時は `started_at = now - (paused_at - started_at)` に更新して `paused_at` を削除し、`playing = true` とする。新規トラック選択時や先頭巻き戻し（⏪）時は `started_at = now` として `paused_at` を削除する。
  - 一時停止時は選択中トラックと復号済みの Object URL を保持し、再開時に再ダウンロードしない。トラック削除（`NONE` 選択または 🗑️ ボタン）時は `std::bgm<N>` / `volume` / `playing` / `started_at` / `paused_at` を削除したうえで `bgm::<slot>::removed = true` の tombstone を書き込み、遅延到着した古いブロードキャストやスナップショットで行が復活するのを防ぐ。トラック変更・削除・シーン切り替え・アンマウント時には Object URL を `revokeObjectURL` で解放する。
  - `SceneSettingsTab` の各操作（再生・一時停止・再開・巻き戻し・曲選択・音量変更・削除）は非同期でサーバー ack を待ち、トラック（`std::bgm<N>`）→ `started_at` → `paused_at` 削除 → `playing` の順（一時停止は `paused_at` → `playing = false`）に確定させてからローカルの `bgmSlots` / `scene.juices` を更新する。送信中は該当スロットのコントロールを波線 `CircularProgress` にして二重操作を防ぐ。
- **音声再生と Autoplay 制御**:
  - `utils/audio.ts` の `AudioManager` がスロットごとに単一の `HTMLAudioElement`（`loop = true`）を管理し、`pantry.get`（`downloadTin(tinId, roomKey)`）で復号したペイロードから Object URL を生成する。再生中はトランスポート識別子（`tinId` / `started_at` / `paused_at`）の変化または 1 秒を超えるドリフトが生じた場合にのみシークを行う。
  - ブラウザの Autoplay 制限はエラー表示せず（Issue #248）、最初のユーザー操作（`pointerdown`, capture, once）で再生意図のあるスロットを算出済みオフセットから開始する。
- **部屋鍵・WrappedKey の整合性と `invalid polyval tag` の防止（Issue #298）**:
  - BGM や背景画像の Tin を AES-256-GCM-SIV で復号する際、暗号化時と異なる `roomKey` や不整合な `wrappedKey` / `nonce` を渡すと認証タグ検証が失敗し `invalid polyval tag` エラーが発生する。
  - 特に、`localStorage`（`alhalte_pantry_assets`）に保存された個人スコープ（`scope: 'personal'`）のアセットを別の部屋でアップロードしたまま現在の部屋で選択した場合、元の Tin が別部屋の `roomKey` で `RoomKey` 暗号化されていると、現在の部屋の `roomKey` では復号できない。また、部屋遷移や訪問済み部屋リスト（`VisitedRoom` / `RecentRoom`）の更新時に URL フラグメント `#<roomKey>` が欠落・変質した場合も鍵不一致の原因となる。
  - これを防ぐため、以下の規約を徹底する。
    1. `App`・`pantry`・`SceneSettingsTab`・`AssetPickerModal`・`AudioManager` は常に現在の部屋の正規化された 32 バイト `roomKey`（64 hex）を一貫して共有し、`VisitedRoom` / `RecentRoom` でも `#<roomKey>` フラグメントを保持する。
    2. アセットの取り込みとアップロードは `pantry.put` を経由し、画像は長辺最大 1920px の WebP、音声は最大 128kbps の Opus へ変換する（Issue #302）。
    3. 個人アセットを含む既存アセットを現在の部屋の BGM（`std::bgm<N>`）や背景画像（`std::b-img`）に設定する際は、そのアセットの Tin が現在の部屋の `roomKey` で復号可能であること（現在の部屋の `roomKey` で暗号化または `Convergent` の `wrappedKey` が現在の部屋の `roomKey` でラップされて `cellar` に `"putted"` 済みであること）を保証し、別部屋で作成されたアセットは現在の部屋の `roomKey` で再エンコードまたは鍵ラップして `pantry.put` してから State に `tinId` を書き込む。

### シーンの管理操作

- 「シーンを新規作成」「ログを削除」「シーンを削除」「メンバー移動」はそれぞれ `requestCreateScene` / `requestRewindLog(sceneId, 0, 0)` / `requestDeleteScene` / `requestMoveCharacter` で送信し、成功 ack 後にのみローカル状態を更新する（楽観的更新は行わない）。
- **シーン検索と新規作成（Issue #269）**: コントロールパネル共通ヘッダーと「キャラクターを移動」モーダルは同一の `SceneSwitcher` コンポーネントを使用する。Enter キー（および移動モーダルの「移動」ボタン）は「入力文字列と完全一致する既存シーン → 先頭の候補シーン → 管理者の場合は新規作成」の優先順で解決する。新規作成時は `generateSceneId()` で ID を採番し、`CreateScene { name }` の ack 後に一覧へ追加する（ヘッダーからの作成時はそのまま `handleSelectScene(id)` で `enterScene` まで行う）。候補が空のときの案内文は、管理者には「シーンは新規作成されます」、非管理者には「該当するシーンはありません」と表示し、非管理者には作成導線を出さない。
- **キャラクター移動モーダル（Issue #269）**: キャラクター一覧の各行からルートにホイストされた「キャラクターを移動」モーダルを開く。既存シーンへの移動は `MoveCharacter` のみ、新規シーン名を指定した場合は `CreateScene` ack → `MoveCharacter` ack の順で実行し、操作中の `currentSceneId` は変更しない（管理者が作業中のシーンから強制遷移させられない、Issue #228 の不変条件）。
- シーン作成時の表示名は `CreateScene.name` として送り、`banquette` が `std::name` へ原子的に書き込む。シーン改名は `requestUpdateState(sceneId, 'std::name', name)` の `StateUpdated` ack 後に確定する。`SceneInfo` ブロードキャストにより他クライアントのシーン名もリロードなしで即時更新される。
- 未接続時はシーン作成・削除・ログ削除・メンバー移動をローカルで成功扱いにせず、管理領域の下部に「サーバーと通信できませんでした」を表示する。

### キャラクター同期と ccfolia 駒互換読み込み（Issue #300）

- キャラクタータブ（`CharacterManagementTab`）は表示時に `queryListCharacters` を発行し、問い合わせ中は contained `LoadingIndicator` を表示する。タイムアウト時は `OfflineQueueNotice kind="character"` を表示し、キャッシュ済み一覧を保持する。
- **ccfolia 駒互換読み込み（Issue #300）**:
  - `CharacterManagementTab` のヘッダー領域にある「新規作成」ボタンの隣に「ccfolia 駒互換読み込み」ボタンを配置する。
  - 押下時にクリップボード（またはペースト入力）から ccfolia の駒出力 JSON（`{ "kind": "character", "data": { "name", "memo", "initiative", "externalUrl", "status": [{ "label", "value", "max" }], "params": [{ "label", "value" }], "commands", "hideStatus", "color", ... } }`）を読み取ってパースし、Alhalte の `Character` 構造（`name`、`statuses`、`parameters`、`chatPalette`、`hideStatus`、`color` など）へマッピングして新規キャラクター作成フローを実行する。
  - 不正な JSON や `"kind": "character"` 以外のデータが渡された場合は、タブ内に分かりやすいエラーメッセージを表示し、不完全なキャラクター作成を行わない。
- `CharacterList` / `CharacterInfo` は `tinMapToCharacter` で全属性をデコードし、キャラクタータブとフィールド上の駒で同一の `statuses` / `parameters` / `chatPalette` / `icon` / `color` / `piecePosition` を表示する。
- `CreateCharacter` / `UpdateCharacter` が成功すると、`banquette` は部屋全体へ `CharacterInfo { character_id, data }` をブロードキャストし、`App` は ID をキーとして冪等に upsert する。`CharacterDeleted` / `CharacterMoved` も同様に全クライアントへ反映される。
- キャラクター編集の保存・削除はサーバー ack 後にのみ反映し、編集の入口は所有者（`isCharacterOwnedBy`）または管理者（`admin`）のみに許可する（Issue #267 / #228）。タイムアウト時は `kind: 'character'` として outbox に積み、再接続時に再送する（`PermissionDenied` はキューに積まずエラー表示する）。

### 駒の同期

- フィールド上の駒の配置状態・座標は、各シーンの State キー `piece::<id>::placed` / `piece::<id>::pos` を正とする。`pantry` が `StateSnapshot` / `StateUpdated` / `StateDeleted` を処理して `App` に通知し、`characters` の `placedAsPiece` / `piecePosition` を更新する。
- 配置の切り替え（toggle）やドラッグ終了時の位置確定は `requestUpdateState` の ack 後に確定する。ドラッグ操作中はフィールド上の描画を即時追従させ、同一駒の連続ドラッグは最後の位置のみに集約（coalesce）して送信する。
- 未接続時や ack 失敗時は変更を確定せず、`piece-action-error`（「ルームに接続していないため、駒を操作できません」）を表示してドラッグ開始前の位置へ戻す。
- キャラクター削除時は `banquette` が全シーンの `piece::<id>::*` を削除して `StateDeleted` をブロードキャストするため、他クライアント上に操作不能な幽霊駒が残ることはない。

### 拡張機能とリアルタイム状態同期（Issue #301 / #302）

- 拡張機能のメタデータはルートシーン `/` の State キー `extension::<id>` に `{ id, name, description, icon, priority, enabled, tinId }` の JSON Tin として保存する。WASM 本体は RoomKey 暗号化で `cellar` に格納し、State には `tinId` のみを保持する（State の 4KiB 制限を守るため）。
- **ABI 2 と自己紹介 ID（Issue #274）**: 拡張機能の表示メタデータ（`id` / `name` / `icon` / `description`）の正は WASM の `self_introduction` エクスポートである。ABI 2 は postcard エンコードされた `(id, name, icon_svg, description)` を返し、公式拡張機能では `id` がパッケージ名（マニフェスト ID）と一致する。ホストは ABI 1（`(name, icon_svg, description)`、ID なし）と ABI 2 を受理し、ABI 2 の自己紹介 ID がマニフェスト ID と一致しない WASM は `wasmMissing: true` として実行を拒否する（fail-closed）。
- **初回シードとサーバー正**: ルートシーン `/` の `extension::__seeded` をシード完了マーカーとする。マーカーが存在しない部屋では、管理者（GM）がサーバーに未登録の公式既定拡張メタデータを `requestUpdateState` で書き込み、最後に `extension::__seeded` を書き込む。マーカーが存在する部屋ではサーバー側の `/` State を正とし、サーバー上から削除された公式既定拡張を再入室・再接続時に復活させない。
- **`pantry` による常時同期と即時反映（Issue #301 / #302）**:
  - ルートシーン `/` の State（`extension::<id>` および `extension::__seeded`）は `pantry` が入室中常時保持・購読する。
  - 管理者が拡張機能の有効 / 無効（ON/OFF）の切り替え、優先度の並べ替え、カスタム拡張の追加・削除を行うと、`banquette` からの `StateUpdated` / `StateDeleted`（および `StateSnapshot`）が `pantry` に取り込まれ、変更通知を通じて全参加者の `App` と `ExtensionManagementTab` へ即座に反映される。
  - これにより、参加者が拡張機能タブを開いていない状態や別のシーンに滞在している状態でも、発言送信時に使われる `ExtensionPipeline` の有効拡張一覧と優先順序、および開いている拡張機能タブの UI 表示が再接続やタブの再表示なしでリアルタイムに同期される。
  - 同一 ID でサーバー上の `tinId` が変化した場合や手元に `wasmBytes` がない場合は、`pantry.get` で `cellar` から WASM を取得してインスタンス化する。
- **WASM 欠落時の扱い**: WASM の取得やインスタンス化に失敗した拡張機能は `wasmMissing: true` とし、タブの該当行に「wasm がありません」を表示して有効化 Switch を無効化する（並べ替えと削除は可能）。`ExtensionPipeline.executePipeline` は `wasmMissing` の有効拡張を黙ってスキップせず、警告を記録してフッター上部の `pipeline-warning` に表示したうえで原文（ext-io Tin）を送信する。
- 操作中・エラー時の UI: toggle / delete / reorder / upload は楽観的更新を行わず、`requestUpdateState` / `requestDeleteState` の成功後に反映する。送信中は該当行のアイコンを波線 `CircularProgress` にする。エラー時はタブ上部に「サーバーと通信できませんでした」を表示し、一覧のスクロール・閲覧は維持したまま変更操作のみを無効化する（Issue #149）。非管理者（`participant` / `viewer` / `denied`）には、行の非操作要素（名前・説明・アイコン・優先度バッジ）のコントラストを少し落として編集不可であることを視覚的に示す（Issue #244）。

### アセット管理とメディア変換（Issue #298 / #302）

- アセットのバイナリ本体とアップロード状態は `pantry` の Tin 管理（`get` / `put`）が担い、メタデータ一覧は `utils/assets.ts`（`personal` スコープは `localStorage` の `alhalte_pantry_assets`、`room` スコープはルートシーン `/` の State キー `asset::<id>`）で管理する。
- **取り込み時の自動再エンコードと `put`（Issue #302）**:
  - 画像ファイルは取り込み時に長辺最大 1920px の WebP へ、音声ファイルは最大 128kbps の Opus へ `pantry` が再エンコードしてから Tin 化する。
  - `pantry.put` により `cellar` へ格納し、完了した Tin は `"putted"` として記録する（すでに `"putted"` なら再アップロードしない）。通信失敗時は `"put-failed"` として記録し、カード内に「アップロードに失敗しました」と再試行ボタンを表示するとともに、オンライン復帰時に `pantry` が自動再送する。
  - アップロード進捗は実 XHR の `upload.onprogress` から取得し、カード内に波線 `LinearProgress` を表示する。
  - `room` スコープのアセットは `cellar` への格納成功後に `/` の State キー `asset::<id>` へメタデータ（`id`, `name`, `mediaType`, `mimeType`, `size`, `tinId`, `createdAt`, `ownerAccountId`）を保存する。`pantry` の `/` State 同期により、部屋内の他クライアントにも即座に共有される。
- **シーン背景画像**: `AssetPickerModal` で選択された画像を現在の部屋の `roomKey` で `pantry.put` し、シーンの State キー `std::b-img` に `{ tinId, mimeType }` の JSON Tin として保存する（Data URL 生データは State に入れない）。画像の幅・高さは `std::b-img::width` / `std::b-img::height` に数値 Tin として保存する。受信側は `pantry.get` で復号して `juices.bgImage` に反映する。

### 権限 UI

アカウント検索（`utils/session.ts` の `searchAccounts`）のみ HTTP を使用し、権限表の読み書きはすべて `RoomSocket` の権限フレームで行う（Issue #201）。

- `RoomPermissionModal` は開くたびに `requestQueryPermissions` で `PermissionsSnapshot` を取得し、M3 Search、`Divider`、Connected Button Group（拒否 / 閲覧者 / 参加者 / 管理者）で一覧を描画する。権限表はアカウント ID のみを持つため、表示名は `/api/accounts/search?q=<account_id>` で解決し、未解決の行は `#<account_id>` を表示する。
- ロールの変更・メンバー追加・既定権限へのリセット（「削除」ボタンは `SetMemberPermission(default_permission)` を送信する）は、`requestSetMemberPermission` の ack（`PermissionsUpdated`）が返ってから一覧を更新する（楽観的更新は行わない）。失敗時は一覧を変更せず `permission-error` を表示する。
- 既定の権限セレクタはモーダル内と `CommonHeader` のギアメニューにあり、`requestSetDefaultPermission` の ack 後にのみ表示を更新する。

`App` は `role` を `ControlPanel` 経由で各タブへ渡し、`roomRoleCapabilities` に基づいて操作可否を制御する。

| role | 送信（log / state） | キャラクター CRUD | シーン作成・削除・メンバー移動 | 拡張機能管理 | 権限表 / 既定の権限 |
| --- | --- | --- | --- | --- | --- |
| `admin` | 全シーン可。話者は全キャラクター（GM 代演、Issue #271） | 可（全キャラクター） | 可 | 可 | 可 |
| `participant` | アクセス可能シーンのみ（UI は現在のシーンと `/` のみ送信可とし、最終認可はサーバー）。話者は自己所有キャラクターのみ | 可（自己所有分のみ、Issue #267） | 不可 | 不可 | 不可 |
| `viewer` | 不可（入力欄・送信・ダイスを無効化） | 不可 | 不可 | 不可 | 不可 |
| `denied` | 不可（`GateScreen` を表示） | 不可 | 不可 | 不可 | 不可 |

## 既知の差分・制約

- `contentTin` は発言送信時に一度だけ生成する。拡張機能パイプラインが実行エラーとなった場合は平文の `content` のみを保存し、表示時は描画時パイプラインにフォールバックする。WASM が欠落している有効拡張は適用せず、`pipeline-warning` に警告を表示したうえで ext-io Tin をそのまま `contentTin` とする。
- システムイベント（入退場など）は `banquette` の Tin にタイムスタンプを持たないため、ログ単体での時刻は `0`（1970 年）となる（`kitchen` での HTML 生成時は前後の発言から補間する）。シーングラフでは `(chunkIndex, position)` 順に整列して扱う。
- `personal` スコープのアセットメタデータはブラウザの `localStorage` にのみ保存され、他クライアントへは自動公開されない。ただし BGM や背景画像としてシーンに設定される際は、現在の部屋の `roomKey` で復号可能な Tin として `pantry.put` されてからシーンの State に `tinId` が書き込まれる（Issue #298）。なお、`maitre-d` プロキシと `cellar` はアップロードされた `Bytes` をメモリ上にバッファリングする（上限 64 MiB）。
- 保留キュー（pending queue / outbox）の再送は at-least-once である。サーバー側で適用済みだが通信切断によりクライアントが ack を受け取れなかった操作は、再接続時に再送されて重複適用される可能性がある。
- 公式既定拡張の削除状態の永続性は、ルートシーン `/` の `extension::__seeded` マーカーに依存する。マーカーが書き込まれる前に削除された公式既定拡張は、初回のシード処理（「サーバーに存在しない公式既定拡張を書き込む」）によって復活し得る。一度シードが完了した後は、`pantry` の State 管理によって拡張機能の ON/OFF・追加・削除が全クライアントへリアルタイムに同期される（Issue #301 / #302）。
- 操作要求のタイムアウトは 8 秒（`DEFAULT_OPERATION_TIMEOUT_MS = 8000`）であり、サーバーが応答を返さずに破棄する操作ではタイムアウトが失敗シグナルとなる。
- Saignant WASM は v2 ABI でキャラクター名マップを受け取る。v2 をエクスポートしない古い WASM バイナリを読み込んだ場合のみ、メンション表示が `Anonymous` にフォールバックする。
- 部屋鍵は URL フラグメント（`#<roomKey>`）に保持し、サーバーへは送信しない。そのためブラウザ履歴や共有 URL にはフラグメントが含まれる。`App` と `pantry` は `roomKey` prop → `currentRoom.url` のフラグメント → `window.location.hash` の順で 64 文字 hex の部屋鍵を解決する。部屋鍵が存在しない場合はアセットや拡張機能 WASM のアップロードを開始せずエラーを表示する。
- Google OAuth の `redirect_uri` は `maitre-d` の固定 URL（`{public_url}/auth/google/callback`）のみを Google Cloud Console に登録する。開発環境で Vite dev server を使う場合は `maitre-d` の `public_url` を Vite の dev オリジンに合わせる。本番（`cafe` / ポータブル版 / `nix run .`）では `maitre-d` が SPA を同一オリジンで配信する。
- 入室可否は `GET /api/rooms/{id}` の preflight を正とし、WebSocket の `Error` フレームやクローズコード（`1008` / `4403` / `4404`）でも `GateScreen` へ遷移する。一方、`PermissionsSnapshot` や `PermissionsUpdated` は要求元にしか返らないため、他クライアントによる権限降格は再接続時または次回の権限照会・操作拒否時に検知される。
