# alhalte-maitre-d

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-maitre-d`  
> テスト: `crates/alhalte-maitre-d/tests/maitre_d_tests.rs`

## 責務

restaurant の認証・玄関口。Google OAuth で `sub` だけを継承し、インスタンス固有の salt で暗号化して account を作る。64bit の session を発行・失効させる。HTTP API と WebSocket を提供し、部屋 ID ごとに通信を demultiplex して `alhalte-banquette` の UNIX ソケットへ流す。SQLite に部屋 registry（`rooms`）と権限を保持し、部屋作成 API と ZIP export も持つ。

## 公開 API / 外部接続点

### HTTP API

| メソッド | パス | 内容 |
| --- | --- | --- |
| `GET` | `/api/verify-session` | `owner` / `account_id` と `session_id` の有効性を返す。cellar の `MaitreDVerifier` と互換。 |
| `POST` | `/api/heartbeat` | `session_id` の `last_heartbeat` を更新する。 |
| `GET` | `/auth/google/url` | Google OAuth の認可 URL を返す。 |
| `GET` / `POST` | `/auth/google/callback` | 認可コードを交換し、account と session を返す。 |
| `POST` | `/auth/google/token` | ID token を検証し、account と session を返す。 |
| `GET` / `PATCH` | `/api/account/{account_id}` | account の取得・handle name / icon の更新。 |
| `GET` | `/api/accounts/search?q=...` | handle name / ID で account を検索。 |
| `POST` | `/api/rooms` | 認証必須。部屋を `rooms` に登録し、作成者を `gm` にする。banquette は起動しない。 |
| `GET` | `/api/rooms/{room_id}` | 認証必須。部屋名・caller の role・done を返す。非メンバーかつ done=false は 403、未知は 404。 |
| `GET` / `POST` | `/api/room/{room_id}/permissions` | 部屋の権限一覧取得・追加/更新。 |
| `DELETE` | `/api/room/{room_id}/permissions/{account_id}` | 権限削除。 |
| `GET` | `/api/room/{room_id}/export/zip?type=full|logs` | 部屋またはログの ZIP。 |
| `POST` | `/api/cellar/store` | 認証付き cellar proxy。cellar の `POST /store`（210/211/212 の store check）へ転送する。 |
| `POST` / `PUT` | `/api/cellar/store/{tin_id}` | 認証付き cellar proxy。Tin 本体を cellar の `POST` / `PUT /store/{tin_id}` へ転送する。 |
| `GET` | `/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。本体と `X-Alhalte-Wrapped-Key` を返す。 |
| `POST` | `/api/debug/account` | debug ビルドのみ。`sub = "0"` の account を作る。 |

`/accounts/search`、`/room/{room_id}/permissions`、`/room/{room_id}/export/zip` など、`/api` なしの別名も登録されている。CORS は `CorsLayer::permissive()`。

#### room ID 契約

room-scoped route の `{room_id}` は `RoomId`（`crates/alhalte-maitre-d/src/room_id.rs`）で受ける。

- 正は **16 hex**（`generateRoomUrl` が作る形）。`RoomId` の `Display` は常に 16 hex 小文字。
- 移行用に decimal の `u64` 文字列も受理する。16 文字すべてが数字の場合は hex として解釈する。`POST /api/rooms` も両方を受け付け、保存・返却は常に canonical 16 hex にする。
- 不正な ID は Axum の path / query 抽出で **400 Bad Request**。
- 未知の ID は部屋 registry（`rooms`）にないため **404 Not Found**。登録済みでも、account に role がなく `done=false` の場合は **403 Forbidden**。`done=true` なら非メンバーは `viewer` として扱う。
- `room_permissions.room_id` には `RoomId` の `Display`（16 hex）を保存する。WebSocket の権限判定も同じ正規形を使う。
- `GET /ws?room=<id>` と `X-Alhalte-Room` ヘッダも `RoomId` として解釈する。

### WebSocket

| パス | 内容 |
| --- | --- |
| `/ws` | クエリ `room` またはヘッダ `X-Alhalte-Room` で部屋を指定（16 hex / decimal）。 |
| `/ws/room/{room_id}` | パスで部屋を指定（16 hex / decimal）。 |
| `/room/{room_id}` | 招待 URL 互換。 |
| `/room/{room_id}/ws` | 同上。 |

認証情報はクエリ `account_id` / `session_id`、ヘッダ `X-Alhalte-Account` / `X-Alhalte-Session`、Cookie `session_id` の順で解決する。`session_id` だけがあり `account_id` がない場合は、session から account を引く。

### 外部接続

| 接続先 | 内容 |
| --- | --- |
| Google OAuth | `accounts.google.com` の認可 URL、`oauth2.googleapis.com/token`、`tokeninfo`。 |
| `alhalte-banquette` | UNIX ソケット。`ProcessBanquetteSpawner` または `InProcessBanquetteSpawner` が起動し、長さ前置 postcard で通信する。 |
| `alhalte-kitchen` | ZIP export のためにライブラリとして呼ぶ。 |
| `alhalte-cellar` | `--cellar-url` を base に `/api/cellar/*` を HTTP で転送する（`CellarProxy`）。 |

`/api/cellar/store`、`/api/cellar/store/{tin_id}` は Axum 既定の 2 MiB を超える Tin（アセット・拡張 wasm）を通すため、`DefaultBodyLimit::max(64 MiB)` を route 単位で設定する。他の route は既定のまま。

CLI（`CliArgs`）: `--host`、`--port`、`--db-path`、`--client-id`、`--client-secret`、`--salt`、`--socket-dir`、`--banquette-bin`、`--cellar-url`、`--chunk-size-exponent`（既定 8）、`--unix-socket`。

### 主な公開型

- `RoomId`、`RoomIdParseError`、`parse_room_id`
- `Database`、`Account`、`Session`、`Room`、`RoomRole`、`RoomPermissionItem`
- `CreateRoomRequest`、`RoomInfoResponse`、`RoomSessionQuery`、`MAX_ROOM_NAME_LEN`
- `OAuthProvider`、`GoogleOAuthProvider`、`MockOAuthProvider`、`GoogleUserInfo`
- `RoomBridge`、`RoomBridgeManager`、`BanquetteSpawner`、`ProcessBanquetteSpawner`、`InProcessBanquetteSpawner`
- `AppState`、`create_router`、`extract_session_id_from_cookie`
- `encrypt_sub` / `decrypt_sub`、`DEBUG_SUB_CIPHERTEXT`
- session cleanup: `previous_4am_local`、`cleanup_sessions_at`、`spawn_session_cleanup_task`

## 内部フロー

### Google 認証

1. `/auth/google/url` が client ID、redirect URI、state を含む認可 URL を返す。
2. callback または `/auth/google/token` が Google から `sub` を取得する。
3. `encrypt_sub(salt, sub)` が salt から鍵を導出し、AES-256-GCM-SIV（Zero Nonce）で暗号化する。
4. `get_or_create_account(encrypted_sub, handle_name, icon, now)` が account を引くか作る。
5. `create_session(account_id, now)` が 0 以外のランダムな 64bit session ID を発行する。
6. 応答は `Account` と `session_id`。ブラウザ側は Cookie と localStorage に保存する。

`argenterie` は `GET /auth/google/url` で認可 URL を取得し、現在のページ（クエリ・フラグメントを除く `redirect_uri`）へ戻る。戻り URL の `code` / `state` を検証し、`POST /auth/google/callback` で session に交換する。`GET /auth/google/callback` はサーバー単体で認可コードを確認するための経路で、ブラウザ UI は POST を使う。認可前のクエリと部屋鍵フラグメントは `argenterie` が sessionStorage に退避し、交換後に復元する。

Google から継承するのは `sub` のみ。handle name は初回の既定値で、後から `PATCH /api/account/{id}` で変更できる。

### session 失効

`spawn_session_cleanup_task` は次回のローカル 4:00 まで sleep し、`cleanup_sessions_at` を実行する。`previous_4am_local` は実行時刻が 4:00〜4:05 の場合は前日 4:00 を返し、それ以外は当日 4:00 を返す。`last_heartbeat` が cutoff より前の session を削除する。

### WebSocket 接続

1. クエリ・ヘッダ・Cookie から account / session を解決する。
2. 両方ある場合は `is_session_valid` で検証する。session だけの場合は 401。
3. `rooms` を引き、未登録なら **404**。`get_or_create_room` は呼ばない。
4. 登録済みでも、account に role がなく `done=false` なら **403** と「全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください」を返す。`argenterie` はこの拒否を門前払い画面（`GateScreen`）として表示し、アカウントのサインイン・切り替えを促す。`done=true` の非メンバーは `viewer` として接続できる。
5. bridge の `admin_account` は DB の GM（`room_permissions.role = 'gm'`）、なければ `rooms.creator_account` から決める。接続順や接続者を admin にしない。
6. `RoomBridgeManager::get_or_create_room` で部屋の bridge を取得する。bridge は banquette を起動し、UNIX ソケットへ接続する。接続完了までは `mpsc::unbounded_channel` にフレームをキューする。
7. bridge に client を登録し、`ClientConnected` を banquette へ送る。初期 scene は `/`。
8. クライアントからの `ClientFrame` は JSON または postcard で受け取り、viewer の mutate を遮断してから bridge へ渡す。
9. banquette からの `ServerFrame` は client_id 宛、scene 宛、room 全体宛に振り分ける。

`is_mutate_frame` は `AppendLog`、`EditLog`、`DeleteLog`、`RewindLog`、`UpdateState`、`DeleteState`、`CreateScene`、`DeleteScene`、`SetDone`、`CreateCharacter`、`UpdateCharacter`、`DeleteCharacter`、`MoveCharacter`、`ChangeBgm` を mutate とみなす。

### cellar proxy

1. `resolve_caller` が Cookie `session_id` / クエリ / ヘッダから session を解決し、DB から account を引く。リクエストの `owner` / `session_id` は使わない。
2. `/api/cellar/store` の JSON body は `owner` / `session_id` を session 由来の値で上書きしてから cellar へ送る。クライアントは owner を詐称できない。
3. `/api/cellar/store/{tin_id}` は `X-Alhalte-Owner` / `X-Alhalte-Session-Id` を付与し、`X-Alhalte-Wrapped-Key` と `Content-Type` を透過する。`tin_id` は 64 hex を検証してから転送する。
4. `GET /api/cellar/tin/{tin_id}` は `X-Alhalte-Wrapped-Key` をレスポンスへ透過する。
5. 上流の status / body はそのまま返し、接続失敗は 502、timeout は 504 に変換して内部 URL を漏らさない。cellar 未設定時は 503。

### 部屋作成

1. `POST /api/rooms` は session を検証する（401）。
2. `room_id` があれば `RoomId` として parse し、canonical 16 hex に正規化する。不正なら 400。なければ非ゼロのランダム u64 を生成する。
3. 名前を trim し、空・制御文字を含む・`MAX_ROOM_NAME_LEN`（100 文字）超を 400 で拒否する。
4. `rooms` に insert し、同じ transaction で作成者を `gm` として `room_permissions` に insert する。既存 ID は 409 Conflict。
5. banquette は起動しない。最初の WS / export までプロセスは作られない。

### 権限管理

`rooms(room_id, name, creator_account, created_at)` が部屋の registry で、`room_permissions(room_id, account_id, role)` に `gm` / `pl` / `viewer` を保存する。`room_id` は `RoomId` の正規形（16 hex 小文字）で、permissions API・export・WebSocket の権限判定で共通に使う。`RoomRole` は serde で snake_case。WebSocket 接続時の入場可否と viewer の mutate 遮断に使う。`SetDone` は bridge の in-memory `done` と `RoomBridgeManager` の room 状態にも反映する。

### ZIP export

1. `type` が `full` / `logs` 以外なら 400。
2. account と session が片方だけなら 401。両方あれば session を検証する。
3. `rooms` を引き、未登録なら **404**。登録済みでも caller に role がなく `done=false` なら **403**。どちらも banquette を起動しない。
4. bridge の admin は WS と同じく DB の GM / `rooms.creator_account` から決める。
5. bridge に `ExportRoom` を送り、banquette から `RoomData` と chunk を受け取る。
6. `logs` の場合は各 scene の state を空にする。
7. `alhalte_kitchen::export_room_to_zip` で ZIP を作り、`application/zip` と `Content-Disposition` を付けて返す。

## データベース

SQLite のテーブル:

```sql
accounts(account_id INTEGER PRIMARY KEY, encrypted_sub TEXT, handle_name TEXT,
         icon_tin_id BLOB, created_at INTEGER)
sessions(session_id INTEGER PRIMARY KEY, account_id INTEGER, last_heartbeat INTEGER,
         created_at INTEGER)
rooms(room_id TEXT PRIMARY KEY, name TEXT NOT NULL, creator_account INTEGER NOT NULL,
      created_at INTEGER NOT NULL)
room_permissions(room_id TEXT, account_id INTEGER, role TEXT, created_at INTEGER,
                 PRIMARY KEY (room_id, account_id))
```

`encrypted_sub` には索引がある。session は account に外部キーで紐づき、account 削除時に cascade する。`room_id` は文字列（`RoomId` の 16 hex 正規形）。`rooms` が部屋の第一級エンティティで、`room_permissions` はその認可だけを持つ。

起動時の migration は次の 2 つを行う。

1. `room_permissions.room_id` のうち、16 文字でない decimal 文字列を canonical 16 hex に書き換える（16 文字の all-digit は #154 の契約どおり hex として扱う）。
2. `rooms` 行を持たない permission 行から部屋を復元する。creator は最も古い GM（いなければ最も古い permission）、名前は `"Unnamed room"` を使う。

## 既知の差分・制約

- main 時点では権限の正が `maitre-d` の SQLite にあり、`banquette` は `done` と `std::admins` で判定する。Issue #114 では `maitre-d` が `banquette` に問い合わせる構成へ移行中。
- `gateway.rs` は `AccountPermissionResponse` の受信処理が未接続で、`pending_perm_queries` を登録しても応答が届かない経路がある。
- `RoomBridgeManager` の `room_done_status` は in-memory で、再起動すると失われる。永続化は `banquette` の room state 側に委ねる（#157）。
- `POST /api/rooms` は `rooms` と初期 GM を書くだけで banquette を起動しない。最初の WS / export で起動する。
- migration で復元した部屋の名前は `"Unnamed room"` になる（元の名前はクライアントの localStorage にしかないため）。
- `GET /api/rooms`（自分の role を持つ部屋一覧）は未実装。
- debug account は `encrypted_sub = "0"` で作られ、`decrypt_sub` も `"0"` を特別扱いする。debug ビルド限定。
- CORS は permissive。本番運用では reverse proxy 側で制限する前提。
- cellar proxy はリクエスト / レスポンスを全量メモリにバッファする（`reqwest` / `Bytes`、上限 64 MiB）。大きな Tin ほど maitre-d のメモリを消費する。

