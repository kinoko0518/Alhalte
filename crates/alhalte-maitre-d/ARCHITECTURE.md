# alhalte-maitre-d

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-maitre-d`  
> テスト: `crates/alhalte-maitre-d/tests/maitre_d_tests.rs`

## 責務

restaurant の認証・玄関口。Google OAuth で `sub` だけを継承し、インスタンス固有の salt で暗号化して account を作る。64bit の session を発行・失効させる。HTTP API と WebSocket を提供し、部屋 ID ごとに通信を demultiplex して `alhalte-banquette` の UNIX ソケットへ流す。SQLite に部屋 registry（`rooms`）と権限、部屋状態の head（`room_heads`）を保持し、部屋作成 API と ZIP export も持つ。

## 公開 API / 外部接続点

### HTTP API

| メソッド | パス | 内容 |
| --- | --- | --- |
| `GET` | `/api/verify-session` | `owner` / `account_id` と `session_id` の有効性を返す。cellar の `MaitreDVerifier` と互換。 |
| `GET` | `/api/session` | `session_id`（Cookie / ヘッダ / クエリ）から `account`（`AccountResponse`）と `session_id` を返す。argenterie が OAuth 後に account を得るための軽量 API。 |
| `POST` | `/api/heartbeat` | `session_id` の `last_heartbeat` を更新する。 |
| `GET` | `/auth/login` | 絶対 URL の `return_to` を受け、state を発行して Google へ 302 する。`return_to` の origin は CORS allow-list のみ許可（オープンリダイレクト禁止）。`prompt` も任意で透過する。 |
| `GET` | `/auth/google/callback` | Google からの固定 callback。state を単回消費し、コードを交換して account / session を作り、`session_id` Cookie を設定して `return_to` へ 302 する。 |
| `POST` | `/auth/google/token` | ID token を検証し、account と session を返す。 |
| `GET` / `PATCH` | `/api/account/{account_id}` | 認証必須。account の取得（`encrypted_sub` を含まない `AccountResponse`）・handle name / icon の更新。 |
| `GET` | `/api/accounts/search?q=...` | 認証必須。handle name / ID で account を検索。 |
| `POST` | `/api/rooms` | 認証必須。部屋を `rooms` に登録し、作成者を `gm` にする。banquette は起動しない。 |
| `GET` | `/api/rooms/{room_id}` | 認証必須。部屋名・caller の role・done を返す。非メンバーかつ done=false は 403、未知は 404。 |
| `GET` / `POST` | `/api/room/{room_id}/permissions` | GET は認証 + メンバー、POST は認証 + GM。 |
| `DELETE` | `/api/room/{room_id}/permissions/{account_id}` | 認証 + GM。 |
| `GET` | `/api/room/{room_id}/export/zip?type=full|logs` | 部屋またはログの ZIP。persisted head から復元できなければ 5xx。 |
| `POST` | `/api/cellar/store` | 認証付き cellar proxy。cellar の `POST /store`（store check）へ転送し、`200 OK` + `{"result": ...}` の status / body をそのまま返す。 |
| `POST` / `PUT` | `/api/cellar/store/{tin_id}` | 認証付き cellar proxy。Tin 本体を cellar の `POST` / `PUT /store/{tin_id}` へ転送する。 |
| `GET` | `/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。本体と `X-Alhalte-Wrapped-Key` を返す。 |
| `POST` | `/api/debug/account` | debug ビルドのみ。`sub = "0"` の account を作る。 |

`/accounts/search`、`/room/{room_id}/permissions`、`/room/{room_id}/export/zip` など、`/api` なしの別名も登録されている。CORS は明示的な origin allow-list で、`permissive()` は使わない。既定はローカル開発用の `http://127.0.0.1:5173` / `http://localhost:5173` で、`--cors-origin`（または `ALHALTE_CORS_ORIGINS`）で置き換える。Bistrot は `argenterie_url()` と `restaurant.domain` から origin を生成して渡す。`allow_credentials` を使うため `Allow-Headers: *` は使わず、`X-Alhalte-*` を明示する。同じ allow-list が `GET /auth/login` の `return_to` 検証にも使われる。

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

認証情報はクエリ `account_id` / `session_id`、ヘッダ `X-Alhalte-Account` / `X-Alhalte-Session`、Cookie `session_id` の順で解決する。`account_id` 単独は 401。`account_id` + 有効な session は両者が一致すれば受理し、`session_id` だけなら session から account を引く。どちらもない匿名接続は、`done=true` の公開部屋でのみ viewer として受理し、private 部屋は 403。

### 外部接続

| 接続先 | 内容 |
| --- | --- |
| Google OAuth | `accounts.google.com` の認可 URL、`oauth2.googleapis.com/token`、`tokeninfo`。 |
| `alhalte-banquette` | UNIX ソケット。`ProcessBanquetteSpawner` または `InProcessBanquetteSpawner` が起動し、長さ前置 postcard で通信する。 |
| `alhalte-kitchen` | ZIP export のためにライブラリとして呼ぶ。 |
| `alhalte-cellar` | `--cellar-url` を base に `/api/cellar/*` を HTTP で転送する（`CellarProxy`）。 |

`/api/cellar/store`、`/api/cellar/store/{tin_id}` は Axum 既定の 2 MiB を超える Tin（アセット・拡張 wasm）を通すため、`DefaultBodyLimit::max(64 MiB)` を route 単位で設定する。他の route は既定のまま。

CLI（`CliArgs`）: `--host`、`--port`、`--db-path`、`--client-id`、`--client-secret`、`--salt`、`--public-url`（`ALHALTE_PUBLIC_URL`、既定 `http://127.0.0.1:8080`）、`--socket-dir`、`--banquette-bin`、`--cellar-url`、`--chunk-size-exponent`（既定 8）、`--unix-socket`、`--client-secret-file`（`ALHALTE_CLIENT_SECRET_FILE`、0600 file から読む。`--client-secret` より優先）、`--service-token-file`（`ALHALTE_SERVICE_TOKEN_FILE`、0600 file から読む。各 banquette へ path を転送する）、`--service-token`（`ALHALTE_SERVICE_TOKEN`、後方互換）、`--cors-origin`（繰り返し / カンマ区切り、`ALHALTE_CORS_ORIGINS`）。`--public-url` は OAuth の `redirect_uri`（`{public_url}/auth/google/callback`）と Cookie の `Secure` 判定に使う。Bistrot は `maitre-d.public-url` があればそれを、なければ `restaurant.domain` が非ループバックなら `https://{domain}`、ループバック開発なら argenterie URL を渡す。

秘密値（`--client-secret-file` / `--service-token-file`）は `alhalte_maitre_d::secret_file::read_secret_file` が 0600 file から読み、group/other に読める file は拒否する（Issue #194）。Bistrot は復号した秘密を `<data-dir>/secrets/` に 0600 で書き、引数には path だけを渡す。`--service-token-file` の path は spawner が各 banquette に `--service-token-file` として転送する。

### 主な公開型

- `RoomId`、`RoomIdParseError`、`parse_room_id`
- `Database`、`Account`、`Session`、`Room`、`RoomRole`、`RoomPermissionItem`（`Database::get_room_head` / `set_room_head` / `get_room_admin_ids` / `create_oauth_state` / `consume_oauth_state` を含む）、`OAUTH_STATE_TTL_SECS`
- `CreateRoomRequest`、`RoomInfoResponse`、`RoomSessionQuery`、`MAX_ROOM_NAME_LEN`
- `AccountResponse`（`encrypted_sub` を含まない account 応答 DTO）、`SessionResponse`、`DEFAULT_ALLOWED_ORIGINS`、`DEFAULT_PUBLIC_URL`、`SESSION_COOKIE_MAX_AGE_SECS`
- `OAuthProvider`、`GoogleOAuthProvider`、`MockOAuthProvider`、`GoogleUserInfo`、`OAuthLoginQuery`
- `RoomBridge`、`RoomBridgeManager`、`BanquetteSpawner`、`ProcessBanquetteSpawner`、`InProcessBanquetteSpawner`
- `AppState`、`create_router`、`extract_session_id_from_cookie`、`is_allowed_return_to`、`session_cookie_header`
- `encrypt_sub` / `decrypt_sub`、`DEBUG_SUB_CIPHERTEXT`
- session cleanup: `previous_4am_local`、`cleanup_sessions_at`、`spawn_session_cleanup_task`

`RoomBridgeManager::new(spawner, db)` は registry の DB を共有し、`get_or_create_room` は head の読み込み・banquette の起動・UNIX ソケット接続までを await して `Result` を返す。Issue #199 で DB role（`room_permissions` の GM 集合）を実行中の部屋へ push する経路（`SetAdmins` / `sync_room_admins`）を撤去したため、起動時に渡すのは `spawn_banquette(room_id, admin_account, room_head)` だけである。`banquette` は復元した `RoomData.permissions` を使い、head が無い新規部屋では `Room::new` が `admin_account` を `Admin` にする。DB role と部屋権限表の接続は #200。`room_head` は `room_heads` の tin id で、`Some` のときは復元が必須。

## 内部フロー

### Google 認証

OAuth の `redirect_uri` は常に `{public_url}/auth/google/callback` で、Google Cloud Console に登録する値もこれだけ。argenterie の origin / port は Google に渡さない。

1. argenterie は現在の URL（クエリ・部屋鍵フラグメントを含む）を sessionStorage に退避し、fragment を除いた絶対 URL を `return_to` として `GET /auth/login` へ遷移する。
2. `/auth/login` は `return_to` の origin を CORS allow-list と照合する。`//evil`、別 origin、`javascript:` / `data:`、userinfo 付き URL は 400。問題なければ 32 byte の state を発行し、`oauth_states(state, return_to, created_at)` に保存して Google の認可 URL へ 302 する。`prompt` は `select_account` / `consent` / `none` のみ透過する。
3. `GET /auth/google/callback` は `state` を `consume_oauth_state` で単回消費する（10 分 TTL。使用済み・期限切れ・未知は 400）。state に結びついた `return_to` で再度 allow-list を検証する。
4. `exchange_code(code, redirect_uri)` が Google から `sub` を取得する。`redirect_uri` はリクエスト値ではなく固定の `{public_url}/auth/google/callback` を使う。
5. `encrypt_sub(salt, sub)` → `get_or_create_account` → `create_session` の順で account と session を作る。
6. callback は `session_id` Cookie（`Path=/; Max-Age=30 日; SameSite=Lax`、public_url が https なら `Secure`）を設定し、`return_to` へ 302 する。
7. argenterie は `GET /api/session` を Cookie 付きで呼び、account と session を localStorage / Cookie に保存する。sessionStorage の完全な戻り先 URL を `history.replaceState` で復元し、部屋鍵フラグメントを戻す。

`POST /auth/google/token` は ID token 用に残し、同じ account に新しい session を発行する。旧 `GET /auth/google/url` と、クライアントが `redirect_uri` を指定する `POST /auth/google/callback` は廃止した（既存アカウントがないため互換性は切っている）。

Google から継承するのは `sub` のみ。handle name は初回の既定値で、後から `PATCH /api/account/{id}` で変更できる。

### session 失効

`spawn_session_cleanup_task` は次回のローカル 4:00 まで sleep し、`cleanup_sessions_at` を実行する。`previous_4am_local` は実行時刻が 4:00〜4:05 の場合は前日 4:00 を返し、それ以外は当日 4:00 を返す。`last_heartbeat` が cutoff より前の session を削除する。

### WebSocket 接続

1. クエリ・ヘッダ・Cookie から account / session を解決する。
2. `account_id` があり session がない場合は **401**。`account_id` + session は `is_session_valid` で一致を検証し、`session_id` だけなら session から account を導出する。どちらもない場合だけ匿名として扱う。
3. `rooms` を引き、未登録なら **404**。`get_or_create_room` は呼ばない。
4. 登録済みでも、匿名かつ `done=false` は **403**。account に role がなく `done=false` も **403** と「全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください」を返す。`argenterie` はこの拒否を門前払い画面（`GateScreen`）として表示し、アカウントのサインイン・切り替えを促す。`done=true` の非メンバー / 匿名は `viewer` として接続できる。
5. bridge の `admin_account` は DB の GM（`room_permissions.role = 'gm'`）、なければ `rooms.creator_account` から決める。接続順や接続者を admin にしない。
6. `RoomBridgeManager::get_or_create_room` で部屋の bridge を取得する。bridge は `room_heads` の head を読み、banquette を起動して UNIX ソケットへ接続する。head がある場合は banquette が cellar から部屋を復元する。復元に失敗した場合、この時点で **500** を返し、空部屋を起動しない。
7. bridge に client を登録し、`ClientConnected` を banquette へ送る。初期 scene は `/`。
8. クライアントからの `ClientFrame` は JSON または postcard で受け取り、viewer の mutate を遮断してから bridge へ渡す。遮断は `room_permissions` の `viewer` 行だけを見る暫定ゲートで、`banquette` 側の権限表とは独立している（#200 で一本化）。
9. banquette からの `ServerFrame` は client_id 宛、scene 宛、room 全体宛に振り分ける。
10. banquette からの `RoomPersisted { tin_id, default_permission }` は `room_heads` に head を、`rooms.done` に `default_permission != Denied` を記録する。head が次回 spawn 時の復元点になり、`rooms.done` が pre-spawn の入場判定に使われる。
11. banquette からの `ServerFrame::PermissionsUpdated` を受けたときだけ `rooms.done` と bridge の in-memory `done` を `default_permission != Denied` で更新し、そのままクライアントへ転送する。`SetMemberPermission` / `SetDefaultPermission` のリクエスト時点では DB / shadow を更新しない。
12. banquette からの `AccountPermissionResponse` は現時点では debug ログに落とすだけ。`QueryAccountPermission` を送る経路は #200 で `GET /api/rooms/{id}` と WS ゲートに接続する。

`is_mutate_frame` は `AppendLog`、`EditLog`、`DeleteLog`、`RewindLog`、`UpdateState`、`DeleteState`、`CreateScene`、`DeleteScene`、`SetMemberPermission`、`SetDefaultPermission`、`CreateCharacter`、`UpdateCharacter`、`DeleteCharacter`、`MoveCharacter` を mutate とみなす。旧 `SetDone` は #199 で撤去した。

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

`rooms(room_id, name, creator_account, created_at, done)` が部屋の registry で、`room_permissions(room_id, account_id, role)` に `gm` / `pl` / `viewer` を保存する。`room_id` は `RoomId` の正規形（16 hex 小文字）で、permissions API・export・WebSocket の権限判定で共通に使う。`RoomRole` は serde で snake_case。WebSocket 接続時の入場可否と viewer の mutate 遮断に使う。

権限 API は session を必須とし、GET はメンバー（role 行あり）、POST / DELETE は GM だけが呼べる。Issue #199 以降、DB の role は実行中の banquette へ push しない（`SetAdmins` / `sync_room_admins` は撤去）。変更は DB に保存されるだけで、部屋の権限表は `banquette` の `SetMemberPermission` / `SetDefaultPermission` が管理する。したがって実行中の部屋には反映されず、次回 spawn でも反映されない。DB role と部屋権限表を接続するのは #200 の仕事で、そのとき `QueryAccountPermission` で `banquette` に問い合わせる。

`done` は `rooms.done` に永続化し、WS / `GET /api/rooms/{id}` / export の pre-spawn 判定はこの DB 値を参照する。`rooms.done` は `banquette` の `default_permission != Denied` を射影した暫定フラグで、`RoomPersisted.default_permission` と `ServerFrame::PermissionsUpdated` で更新する（更新は成功 frame を観測した後だけ）。`SetDefaultPermission` は banquette に転送するだけで、非管理者の要求は banquette が `SetDefaultPermissionFailed` を返し DB / UI は変わらない。`RoomBridgeManager::is_room_done` も DB を優先し、行がない場合だけ bridge にフォールバックする。#200 でこの gate ごと部屋権限表へ置き換える。

### ZIP export

1. `type` が `full` / `logs` 以外なら 400。
2. account と session が片方だけなら 401。両方あれば session を検証する。
3. `rooms` を引き、未登録なら **404**。登録済みでも caller に role がなく `done=false` なら **403**。どちらも banquette を起動しない。
4. bridge の admin は WS と同じく DB の GM / `rooms.creator_account` から決める。
5. `get_or_create_room` で bridge を取得する。bridge は persisted head から部屋を復元してから `ExportRoom` に応答する。head が壊れている場合は **500** を返し、空の ZIP を返さない。
6. bridge に `ExportRoom` を送り、banquette から `RoomData` と chunk を受け取る。
7. `logs` の場合は各 scene の state を空にする。
8. `alhalte_kitchen::export_room_to_zip` で ZIP を作り、`application/zip` と `Content-Disposition` を付けて返す。

### 部屋 head と復元

- `room_heads(room_id, tin_id, updated_at)` が `room_id → RoomData tin id` のポインタを持つ。`tin_id` は 32 バイトで、`RoomData` の wire schema には手を入れない外部メタデータ。
- `get_or_create_room` は spawn 前に `Database::get_room_head` を読み、`spawn_banquette(room_id, admin, room_head)` に渡す。
  - `ProcessBanquetteSpawner` は `--room-tin <64 hex>` として子プロセスへ渡す。
  - `InProcessBanquetteSpawner` は spawn 前に `Room::load_from_cellar` で復元し、失敗したら `Err` を返す。
- head がない場合だけ `Room::new` で新しい部屋を作る。
- head がある場合の復元失敗（tin 不在・復号不能・room_id 不一致・root scene 不在）は spawn の失敗として扱い、WS は 500、export は 5xx を返す。
- `set_room_head` は置き換え前の tin id を返すが、旧 head の unregister は原則 banquette が行う。`maitre-d` は報告された head を記録するだけ。
- `rooms.done` は head とは独立に保存する。`ServerFrame::PermissionsUpdated` と `RoomPersisted.default_permission` から `default_permission != Denied` を射影して更新し、spawn 前の WS / export gate は `rooms.done` を読む。これにより `maitre-d` 再起動後、banquette が未起動でも公開部屋は非メンバーに開放され、private 部屋は 403 のままになる。射影は #200 で部屋権限表に置き換える。

## データベース

SQLite のテーブル:

```sql
accounts(account_id INTEGER PRIMARY KEY, encrypted_sub TEXT, handle_name TEXT,
         icon_tin_id BLOB, created_at INTEGER)
sessions(session_id INTEGER PRIMARY KEY, account_id INTEGER, last_heartbeat INTEGER,
         created_at INTEGER)
rooms(room_id TEXT PRIMARY KEY, name TEXT NOT NULL, creator_account INTEGER NOT NULL,
      created_at INTEGER NOT NULL, done INTEGER NOT NULL DEFAULT 0)
room_permissions(room_id TEXT, account_id INTEGER, role TEXT, created_at INTEGER,
                 PRIMARY KEY (room_id, account_id))
room_heads(room_id TEXT PRIMARY KEY, tin_id BLOB NOT NULL, updated_at INTEGER NOT NULL)
oauth_states(state TEXT PRIMARY KEY, return_to TEXT NOT NULL, created_at INTEGER NOT NULL)
```

`encrypted_sub` には索引がある。session は account に外部キーで紐づき、account 削除時に cascade する。`room_id` は文字列（`RoomId` の 16 hex 正規形）。`rooms` が部屋の第一級エンティティで、`room_permissions` はその認可だけを持つ。`rooms.done` は `RoomData.default_permission != Denied` を射影した pre-spawn gate 用のフラグで、`maitre-d` の再起動をまたぐ（権限の正は #199 以降 `banquette` の `RoomData` 側）。`room_heads.tin_id` は 32 バイトで、banquette が最後に保存した `RoomData` tin を指す。部屋が一度も保存されていなければ行がない。

`oauth_states` はログイン中の OAuth state を `return_to` と結びつけて保持する。`consume_oauth_state` は同じ transaction で SELECT と DELETE を行い、単回使用を保証する。`created_at` が 10 分（`OAUTH_STATE_TTL_SECS`）より古い state は削除して `None` を返す。`create_oauth_state` は期限切れ行を opportunistic に削除する。

起動時の migration は次の 3 つを行う。

1. `rooms` に `done` 列がなければ `done INTEGER NOT NULL DEFAULT 0` を追加する（既存部屋は private 扱い）。
2. `room_permissions.room_id` のうち、16 文字でない decimal 文字列を canonical 16 hex に書き換える（16 文字の all-digit は #154 の契約どおり hex として扱う）。
3. `rooms` 行を持たない permission 行から部屋を復元する。creator は最も古い GM（いなければ最も古い permission）、名前は `"Unnamed room"` を使う。

## 既知の差分・制約

- 部屋の権限の正は Issue #199 以降 `banquette` の `RoomData.permissions` / `default_permission`（4 段階）にある。`maitre-d` の `room_permissions`（`gm` / `pl` / `viewer`）は DB に残っているが、実行中の部屋へは push しない（`SetAdmins` / `AdminsUpdated` / `sync_room_admins` は撤去）。`pl` / `viewer` の account は banquette からは `Denied`（`default_permission`）に見えるため、viewer の mutate 遮断は当面 `maitre-d` の WS ゲートだけが担う。DB role を部屋権限表へ移すのは #200。
- `rooms.done` は `default_permission != Denied` の射影で、pre-spawn gate（WS / `GET /api/rooms/{id}` / export）専用の暫定値。`ServerFrame::PermissionsUpdated` と `RoomPersisted.default_permission` を受けた時点で更新する。権限の正ではないので、成功 frame を観測しない限り DB は変わらない。#200 で gate ごと置き換える。
- `RoomBridgeManager` の `is_room_done` は `rooms.done` を正とする（bridge の in-memory `done` は DB 行がない場合のフォールバック）。`maitre-d` を再起動しても公開部屋の非メンバーは pre-spawn gate を通れる。
- `QueryAccountPermission` / `AccountPermissionResponse` は実装済みだが、`maitre-d` 側に送信経路がなく、受信はデバッグログに留める。`GET /api/rooms/{id}` と WS ゲートへの接続は #200。
- head の記録は `RoomPersisted` を受けたときだけ行う。banquette が保存前に落ちた場合、`room_heads` は最後に成功した head のままになる。
- `maitre-d` は旧 head の unregister を行わない。別インスタンスが persist して head が置き換わった場合、その旧 tin は参照が残る（banquette は自分の直前 head だけを解放する）。
- `--service-token` は `--client-secret` と同様に引数で渡すため、`ps` や systemd unit から読める。リポジトリ同梱の `Bistrot.toml` はローカル開発用の既定値を設定し、本番では rops / `EnvironmentFile` で差し替える。`EnvironmentFile` 化は今後の課題。
- `get_or_create_room` は接続待ちの間（最大約 3 秒）manager の write lock を保持する。部屋数の多い同時接続ではボトルネックになり得る。
- `ProcessBanquetteSpawner` が起動した子プロセスは maitre-d 側で reap しない。maitre-d が再起動すると、古い banquette が残っていれば同じ socket path / head を奪い合う。systemd の cgroup（既定 `KillMode=control-group`）で一緒に停止する運用を前提とする。
- `POST /api/rooms` は `rooms` と初期 GM を書くだけで banquette を起動しない。最初の WS / export で起動する。
- migration で復元した部屋の名前は `"Unnamed room"` になる（元の名前はクライアントの localStorage にしかないため）。
- `GET /api/rooms`（自分の role を持つ部屋一覧）は未実装。
- debug account は `encrypted_sub = "0"` で作られ、`decrypt_sub` も `"0"` を特別扱いする。debug ビルド限定。
- CORS は明示的な origin allow-list。`--cors-origin` 未指定時はローカル開発 origin のみで、本番では Bistrot が argenterie URL / domain から生成した値を渡す。reverse proxy 側での制限も併用できる。
- OAuth の `redirect_uri` は `--public-url` から作る固定値だけで、argenterie の origin / port は Google に渡さない。旧 `GET /auth/google/url` / `POST /auth/google/callback` は削除した（互換性なし）。`GET /auth/login` の `return_to` は CORS allow-list の origin のみ許可し、`//evil`・別 origin・`javascript:` / `data:`・userinfo 付き URL は 400。
- callback の `session_id` Cookie は `{public_url}` の origin に付く。ブラウザから `/api/session` を Cookie 付きで呼ぶには、`public_url` の origin と argenterie の origin が一致している必要がある。ローカル開発では Bistrot が `--public-url` に argenterie URL を渡し、Vite の `/auth` proxy 経由で callback を受ける。本番は reverse proxy で同一 origin に揃える。
- `GET /api/session` は Cookie / `X-Alhalte-Session` / クエリのいずれかで session を解決し、未認証は 401。`account` は `AccountResponse` で `encrypted_sub` を含まない。
- cellar proxy はリクエスト / レスポンスを全量メモリにバッファする（`reqwest` / `Bytes`、上限 64 MiB）。大きな Tin ほど maitre-d のメモリを消費する。

