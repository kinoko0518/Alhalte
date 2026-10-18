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
| Vite dev proxy | `/api`、`/room` を `http://127.0.0.1:8080`、`/ws` を `ws://127.0.0.1:8080` へ転送。 |

### WebSocket

`utils/roomSocket.ts` の `RoomSocket` が `/ws/room/{roomId}?account_id=...&session_id=...` へ接続する。`ClientFrame` を JSON で送り、`ServerFrame` を JSON で受ける。heartbeat ping には自動で `HeartbeatResponse` を返す。未接続時の送信はキューに積み、再接続後に flush する。

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
- Cookie: `session_id`（`SameSite=Lax`）。
- `sessionStorage`: OAuth の `state`、認可後の戻り先 URL（クエリと部屋鍵フラグメントを含む）、OAuth の `redirect_uri`。サインイン中だけ保持し、完了時に削除する。
- URL フラグメント: 部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を作る。

## 内部フロー

### 入室

1. `main.ts` が `App` を mount する。開発エントリは `isAdmin: true` を渡す。
2. `App.svelte` は `resolveRoomFromLocation` で URL から部屋を解決する。
3. `RoomSocket` を作り、account / session をクエリに付けて接続する。
4. `ServerFrame` の `SceneList`、`CharacterList`、`StateUpdated`、`StateDeleted`、`LogEventAppended` などを Svelte の state に反映する。
5. 403 や「全体公開されていない部屋です」を受けたら `isForbidden` を立て、`GateScreen` を出す。

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

1. `handleSendMessage` が `LogRecord` を作り、`ChunkLogManager` に追加し、`RoomSocket.appendLog` で送る。
2. ログタブは `ChunkLogManager` の変更を購読し、未描画の record を処理する。
3. `buildGenerastFromText` が有効な拡張機能を priority 昇順に適用し、`Generast` Tin を作る。
4. `renderGenerast` が Saignant WASM で HTML にし、`{@html}` で表示する。WASM 未初期化時は HTML エスケープしたテキストを返す。
5. オフライン時は `pendingOfflineMessages` / `pendingOfflineStates` に積み、再接続後に flush する。

### 拡張機能

- `ExtensionManagementTab` で追加・削除・有効化・優先度変更を行う。変更できるのは管理者のみ。
- `handleExtensionFileChange` が `.wasm` を読み、`WasmExtensionInstance.instantiate` で検証し、`selfIntroduction()` があれば名前・アイコン・説明を表示する。
- `ExtensionPipeline` は wasm インスタンスを id ごとにキャッシュする。

### アセット

`utils/assets.ts` は画像・音声を `FileReader` で data URL にし、localStorage と in-memory の Pantry キャッシュアダプタへ保存する。`AssetPickerModal` は personal / room、image / audio で絞り込み、NONE カードと削除を提供する。

### 権限 UI

`utils/session.ts` が account 検索・部屋権限 CRUD を呼ぶ。`RoomPermissionModal` は M3 Search、Divider、Connected Button Group で viewer / PL / GM を切り替える。viewer の場合はフッターの送信・ダイス・scene 変更などを無効化する。入室拒否時は `GateScreen` を出し、右上の `AccountMenu` からサインイン・アカウント切り替え・サインアウトを行える。

## 既知の差分・制約

- `handleSendMessage` は `LogRecord` をそのまま `AppendLog.entry` に渡す。banquette の `ClientFrame::AppendLog` は `Option<Tin>` を期待するため、送信経路では entry を Tin へ正規化する必要がある。表示経路は `LogTab` 側で拡張機能と Saignant を通す。
- 送信時の `extensionPipeline.executePipeline` は結果を使わない。実質的な拡張機能適用はログ表示時（`LogTab.updateRenderedHtml`）に行われる。
- アセットは cellar / pantry の実ネットワーク経路に未接続。data URL を localStorage に保存するため、容量制限と機密性の両面で制約がある。
- Saignant WASM は character resolver を受け取らないため、mention は `Anonymous` になる。
- `main.ts` の `isAdmin: true` は開発用エントリの既定値。実運用では account / 権限から決定する必要がある。
- 部屋鍵は URL フラグメントに置く。サーバーへ送らない設計だが、ブラウザ履歴や共有 URL には残る。
- ホーム画面のテーマ設定 UI は Issue #116 で削除した。現在のテーマ選択はコントロールパネル共通ヘッダーの gear menu にあるパレットのみで、light / dark / system の切替 UI は持たない。
- OAuth の `redirect_uri` は現在のパス（クエリ・フラグメントを除く）を使う。Google Cloud Console 側でこの URL を登録する必要がある。`state` は sessionStorage に保存した値と照合し、一致しない交換は行わない。
- `AccountMenu` は account ごとに `GET /api/account/{id}` を呼ぶ。プロフィール取得に失敗してもサインイン状態は維持し、イニシャル表示にフォールバックする。
- 現行 main では WebSocket の HTTP 403 status をブラウザから直接読めないため、`isForbidden` は権限一覧 API の結果と `Error` frame の文字列から立てている。Issue #114 の権限モデル移行後は、banquette / maitre-d の正式な拒否通知に接続する必要がある。このため、`room_permissions` に行があり `done` が true の部屋でも、権限一覧に自分がいないだけで門前払い画面になる可能性がある。
