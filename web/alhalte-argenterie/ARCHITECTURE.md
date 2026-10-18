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
| `maitre-d` `/api/account/{account_id}` | account 取得・更新。 |
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
- URL フラグメント: 部屋鍵。`generateRoomUrl` は `/rooms/<roomId>#<roomKey>` を作る。

## 内部フロー

### 入室

1. `main.ts` が `App` を mount する。開発エントリは `isAdmin: true` を渡す。
2. `App.svelte` は `resolveRoomFromLocation` で URL から部屋を解決する。
3. `RoomSocket` を作り、account / session をクエリに付けて接続する。
4. `ServerFrame` の `SceneList`、`CharacterList`、`StateUpdated`、`StateDeleted`、`LogEventAppended` などを Svelte の state に反映する。
5. 403 や「全体公開されていない部屋です」を受けたら `isForbidden` を立て、入室拒否画面を出す。

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

`utils/session.ts` が account 検索・部屋権限 CRUD を呼ぶ。`RoomPermissionModal` は M3 Search、Divider、Connected Button Group で viewer / PL / GM を切り替える。viewer の場合はフッターの送信・ダイス・scene 変更などを無効化する。

## 既知の差分・制約

- `handleSendMessage` は `LogRecord` をそのまま `AppendLog.entry` に渡す。banquette の `ClientFrame::AppendLog` は `Option<Tin>` を期待するため、送信経路では entry を Tin へ正規化する必要がある。表示経路は `LogTab` 側で拡張機能と Saignant を通す。
- 送信時の `extensionPipeline.executePipeline` は結果を使わない。実質的な拡張機能適用はログ表示時（`LogTab.updateRenderedHtml`）に行われる。
- アセットは cellar / pantry の実ネットワーク経路に未接続。data URL を localStorage に保存するため、容量制限と機密性の両面で制約がある。
- Saignant WASM は character resolver を受け取らないため、mention は `Anonymous` になる。
- `main.ts` の `isAdmin: true` は開発用エントリの既定値。実運用では account / 権限から決定する必要がある。
- 部屋鍵は URL フラグメントに置く。サーバーへ送らない設計だが、ブラウザ履歴や共有 URL には残る。
