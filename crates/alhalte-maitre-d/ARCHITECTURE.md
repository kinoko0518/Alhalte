# alhalte-maitre-d

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-maitre-d`  
> テスト: `crates/alhalte-maitre-d/tests/maitre_d_tests.rs`

## 責務

restaurant の認証・玄関口。Google OAuth で `sub` だけを継承し、インスタンス固有の salt を key にした BLAKE3 の 128bit hash（`sub_hash`）にして account を作る。生の `sub` は hash 計算後に zeroize し、DB・ログ・HTTP 応答のどこにも残さない（Issue #190）。64bit の session を発行・失効させる。HTTP API と WebSocket を提供し、部屋 ID ごとに通信を demultiplex して `alhalte-banquette` のローカル transport（Unix: UNIX socket、Windows: named pipe）へ流す。SQLite に部屋 registry（`rooms`）を持ち、部屋作成 API と ZIP export も持つ。部屋状態の head は持たない: banquette が cellar の `room/<room_id>/head` alias を読み書きする（Issue #192）。

認証経路は trait（`AuthProvider`）で切り替える。通常は Google OAuth（`GoogleAuthProvider`）を使い、`--local-login` を指定したときだけ、account 名 + パスワードのローカルログイン（`LocalAuthProvider` + Argon2id）に差し替わる。local 有効時は **別の SQLite ファイル**（`maitre-d-local.sqlite3`）を開き、通常の Google 用 DB には一切触らない。部屋状態は local でも cellar に永続化する。

## 公開 API / 外部接続点

### HTTP API

| メソッド | パス | 内容 |
| --- | --- | --- |
| `GET` | `/api/verify-session` | `owner` / `account_id` と `session_id` の有効性を返す。cellar の `MaitreDVerifier` と互換。 |
| `GET` | `/api/session` | `session_id`（Cookie / ヘッダ / クエリ）から `account`（`AccountResponse`）と `session_id` を返す。argenterie が OAuth 後に account を得るための軽量 API。 |
| `POST` | `/api/heartbeat` | `session_id` の `last_heartbeat` を更新する。 |
| `GET` | `/auth/login` | 絶対 URL の `return_to` を受け、state を発行して Google へ 302 する。`return_to` の origin は CORS allow-list のみ許可（オープンリダイレクト禁止）。`prompt` も任意で透過する。local 有効時は state を発行せず SPA の `/login?return_to=...` へ 302 する。 |
| `POST` | `/api/auth/local/login` | local 有効時のみ登録。JSON `{ account_name, password }` を検証し、Argon2id の PHC 文字列と照合して `session_id` Cookie を設定する。成功は `{ "status": "ok" }`、失敗は 400 / 401 / 500 の JSON。 |
| `GET` | `/auth/google/callback` | Google からの固定 callback。state を単回消費し、コードを交換して account / session を作り、`session_id` Cookie を設定して `return_to` へ 302 する。local 有効時は登録されない（404）。 |
| `POST` | `/auth/google/token` | ID token を検証し、account と session を返す。local 有効時は登録されない（404）。 |
| `GET` / `PATCH` | `/api/account/{account_id}` | 認証必須。account の取得（`sub_hash` を含まない `AccountResponse`）・handle name / icon の更新。 |
| `GET` | `/api/accounts/search?q=...` | 認証必須。handle name / ID で account を検索。 |
| `POST` | `/api/rooms` | 認証必須。部屋を `rooms` に登録し、作成者を `gm` にする。banquette は起動しない。 |
| `GET` | `/api/rooms/{room_id}` | 認証必須。部屋名・caller の role・done を返す。非メンバーかつ done=false は 403、未知は 404。 |
| `GET` / `POST` | `/api/room/{room_id}/permissions` | GET は認証 + メンバー、POST は認証 + GM。 |
| `DELETE` | `/api/room/{room_id}/permissions/{account_id}` | 認証 + GM。 |
| `GET` | `/api/room/{room_id}/export/zip?type=full|logs` | 部屋またはログの ZIP。cellar の head alias から復元できなければ 5xx。 |
| `POST` | `/api/cellar/store` | 認証付き cellar proxy。body は `{ tin_id, size, wrapped_key }` のみ（`owner` / `session_id` は 400）。cellar の `POST /store`（store check）へ転送し、`200 OK` + `{"result": ...}` の status / body をそのまま返す。 |
| `POST` / `PUT` | `/api/cellar/store/{tin_id}` | 認証付き cellar proxy。Tin 本体を cellar の `POST` / `PUT /store/{tin_id}` へ転送する。 |
| `GET` | `/api/cellar/tin/{tin_id}` | 認証付き cellar proxy。本体と `X-Alhalte-Wrapped-Key` を返す。 |
| `POST` | `/api/debug/account` | debug ビルドのみ。推測不能な合成 `sub` を hash した account を作る（テスト用。active DB に書く）。 |

`/accounts/search`、`/room/{room_id}/permissions`、`/room/{room_id}/export/zip` など、`/api` なしの別名も登録されている。CORS は明示的な origin allow-list で、`permissive()` は使わない。既定はローカル開発用の `http://127.0.0.1:5173` / `http://localhost:5173` で、`--cors-origin`（または `ALHALTE_CORS_ORIGINS`）で置き換える。alhalte-restaurant は `maitre_d_public_url()` と `restaurant.domain` から origin を生成して渡す。`allow_credentials` を使うため `Allow-Headers: *` は使わず、`X-Alhalte-*` を明示する。同じ allow-list が `GET /auth/login` の `return_to` 検証にも使われる。

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
| `alhalte-banquette` | ローカル transport（Unix: UNIX socket、Windows: named pipe）。`ProcessBanquetteSpawner` または Unix の `InProcessBanquetteSpawner` が起動し、長さ前置 postcard で通信する。export ZIP も banquette が組み立てる（#193）。 |
| `alhalte-cellar` | `--cellar-url` を base に `/api/cellar/*` を HTTP で転送する（`CellarProxy`）。 |

`/api/cellar/store`、`/api/cellar/store/{tin_id}` は Axum 既定の 2 MiB を超える Tin（アセット・拡張 wasm）を通すため、`DefaultBodyLimit::max(64 MiB)` を route 単位で設定する。他の route は既定のまま。

CLI（`CliArgs`）: `--host`、`--port`、`--db-path`、`--client-id`、`--client-secret`、`--salt`、`--public-url`（`ALHALTE_PUBLIC_URL`、既定 `http://127.0.0.1:8080`）、`--socket-dir`、`--banquette-bin`、`--cellar-url`、`--chunk-size-exponent`（既定 8）、`--banquette-heartbeat-interval-secs`（既定 240）、`--local-login`（`ALHALTE_LOCAL_LOGIN`）、`--local-db-path`（`ALHALTE_LOCAL_DB_PATH`）、`--web-root`（`ALHALTE_WEB_ROOT`）、`--unix-socket`（Unix のみ）、`--client-secret-file`（`ALHALTE_CLIENT_SECRET_FILE`、0600 file から読む。`--client-secret` より優先）、`--service-token-file`（`ALHALTE_SERVICE_TOKEN_FILE`、0600 file から読む。各 banquette へ path を転送する）、`--service-token`（`ALHALTE_SERVICE_TOKEN`、後方互換）、`--cors-origin`（繰り返し / カンマ区切り、`ALHALTE_CORS_ORIGINS`）。`--public-url` は OAuth の `redirect_uri`（`{public_url}/auth/google/callback`）と Cookie の `Secure` 判定に使う。alhalte-restaurant は `maitre-d.public-url` があればそれを、なければ `restaurant.domain` が非ループバックなら `https://{domain}`、ループバックなら maitre-d URL を渡す。

`--local-login` は account 名 + パスワードのローカル認証。開く DB は `CliArgs::active_db_path()` が返す **別ファイル**（`--local-db-path`、未指定なら `{--db-path のディレクトリ}/maitre-d-local.sqlite3`、`LOCAL_DB_FILE_NAME`）で、スキーマは通常 DB と同一。通常の Google 用 DB は開かない。部屋状態は通常どおり cellar に永続化し、`--no-persist` は存在しない。`--web-root` があると maitre-d が SPA を同一 origin で配信する。

秘密値（`--client-secret-file` / `--service-token-file`）は `alhalte_maitre_d::secret_file::read_secret_file` が 0600 file から読み、group/other に読める file は拒否する（Issue #194）。alhalte-restaurant は復号した秘密を `<data-dir>/secrets/` に 0600 で書き、引数には path だけを渡す。`--service-token-file` の path は spawner が各 banquette に `--service-token-file` として転送する。

### 主な公開型

- `RoomId`、`RoomIdParseError`、`parse_room_id`
- `Database`、`Account`、`Session`、`Room`、`RoomRole`、`RoomPermissionItem`（`Database::create_room` / `get_room` / `create_oauth_state` / `consume_oauth_state` を含む）、`OAUTH_STATE_TTL_SECS`
- `CreateRoomRequest`、`RoomInfoResponse`、`RoomSessionQuery`、`MAX_ROOM_NAME_LEN`
- `AccountResponse`（`sub_hash` を含まない account 応答 DTO）、`SessionResponse`、`DEFAULT_ALLOWED_ORIGINS`、`DEFAULT_PUBLIC_URL`、`SESSION_COOKIE_MAX_AGE_SECS`
- `OAuthProvider`、`GoogleOAuthProvider`、`MockOAuthProvider`、`GoogleUserInfo`、`OAuthLoginQuery`
- `AuthProvider`、`AuthKind`、`AuthIdentity`、`GoogleAuthProvider`、`LocalAuthProvider`、`DEFAULT_HANDLE_NAME`、`LOCAL_SUBJECT_PREFIX`、`MAX_ACCOUNT_NAME_LEN`、`parse_local_account_name`、`percent_encode`
- `LocalLoginRequest`、`LOCAL_DB_FILE_NAME`、`hash_password`、`verify_password`、`validate_password`
- `RoomBridge`、`RoomBridgeManager`、`BanquetteSpawner`、`BanquetteEndpoint`、`ProcessBanquetteSpawner`、`InProcessBanquetteSpawner`（Unix）
- `AppState`、`create_router`、`extract_session_id_from_cookie`、`is_allowed_return_to`、`session_cookie_header`
- `SubHash`、`hash_sub`、`SUB_HASH_BYTES`、`SUB_HASH_HEX_LEN`
- session cleanup: `previous_4am_local`、`cleanup_sessions_at`、`spawn_session_cleanup_task`

`RoomBridgeManager::new(spawner)` は registry だけで、DB も head も共有しない。`get_or_create_room` は banquette の起動とローカル transport 接続までを await して `Result` を返す。Issue #199 で DB role を実行中の部屋へ push する経路（`SetAdmins` / `sync_room_admins`）を、Issue #192 で head の受け渡しを撤去したため、起動時に渡すのは `spawn_banquette(room_id, admin_account)` だけである。`banquette` は cellar の `room/<room_id>/head` alias から自分で復元し、alias が無ければ `Room::new` が `admin_account` を `Admin` にする。復元失敗は spawn のエラーになる。

## 内部フロー

### Google 認証

OAuth の `redirect_uri` は常に `{public_url}/auth/google/callback` で、Google Cloud Console に登録する値もこれだけ。UI の origin / port は Google に渡さない。

1. argenterie は現在の URL（クエリ・部屋鍵フラグメントを含む）を sessionStorage に退避し、fragment を除いた絶対 URL を `return_to` として `GET /auth/login` へ遷移する。
2. `/auth/login` は `return_to` の origin を CORS allow-list と照合する。`//evil`、別 origin、`javascript:` / `data:`、userinfo 付き URL は 400。問題なければ 32 byte の state を発行し、`oauth_states(state, return_to, created_at)` に保存して Google の認可 URL へ 302 する。`prompt` は `select_account` / `consent` / `none` のみ透過する。
3. `GET /auth/google/callback` は `state` を `consume_oauth_state` で単回消費する（10 分 TTL。使用済み・期限切れ・未知は 400）。state に結びついた `return_to` で再度 allow-list を検証する。
4. `exchange_code(code, redirect_uri)` が Google から `sub` を取得する。`redirect_uri` はリクエスト値ではなく固定の `{public_url}/auth/google/callback` を使う。
5. `hash_sub(salt, sub)` で 128bit hash を作り（`sub` はその場で zeroize される）、`get_or_create_account` → `create_session` の順で account と session を作る。
6. callback は `session_id` Cookie（`Path=/; Max-Age=30 日; SameSite=Lax`、public_url が https なら `Secure`）を設定し、`return_to` へ 302 する。
7. argenterie は `GET /api/session` を Cookie 付きで呼び、account と session を localStorage / Cookie に保存する。sessionStorage の完全な戻り先 URL を `history.replaceState` で復元し、部屋鍵フラグメントを戻す。

`POST /auth/google/token` は ID token 用に残し、同じ account に新しい session を発行する。旧 `GET /auth/google/url` と、クライアントが `redirect_uri` を指定する `POST /auth/google/callback` は廃止した（既存アカウントがないため互換性は切っている）。

Google から継承するのは `sub` のみ。`sub` はその場で hash 化・zeroize され、DB には `sub_hash` だけが残る。`sub_hash` は account を引くための key で、`Account`（DB エンティティ）は `Serialize` せず、HTTP 応答は `AccountResponse` を使う。`SubHash` の `Debug` は `SubHash(<redacted>)`、`GoogleUserInfo` の `Debug` も `sub` を伏せるので、ログ・エラー・`Debug` に生 `sub` も hash も出ない。handle name は初回の既定値で、後から `PATCH /api/account/{id}` で変更できる。

### ローカルログイン（local-login）

`--local-login` を付けたプロセスだけが通る、Google とは別の認証経路。`AuthProvider` が Google と Local を切り替え、`/auth/login` 以降の分岐は provider への問い合わせだけで決まる（`oauth_login_handler` は `login_redirect` が返ったらそこで 302 し、Google の state を発行しない）。

1. 起動時に `CliArgs::active_db_path()` で DB を選ぶ。local 有効なら `--local-db-path`、未指定なら `{--db-path のディレクトリ}/maitre-d-local.sqlite3`。通常の Google 用 DB は開かない（同一スキーマを適用する）。rooms registry も local DB に入るため、Google 用 DB とは部屋一覧を共有しない。
2. argenterie はサインイン導線から従来どおり `GET /auth/login?return_to=...` へ遷移する。`return_to` の allow-list 検証は Google と同じで、問題なければ `/login?return_to=<encoded>`（SPA）へ 302 する。
3. SPA の `/login` は noph-ui のフォームで account 名とパスワードを受け取り、`POST /api/auth/local/login` に JSON を送る。パスワードはログにも応答にも出さない。
4. ハンドラは名前を trim し、`alhalte/local-login/<name>` を合成 `sub` として `hash_sub` で `sub_hash` にする。既存 account の Argon2id PHC 文字列を DB ロック外で照合し、未知の名前は新しい account を作る。同じ名前は同じ account（名前が identity の key）。名前は 1〜100 文字（`MAX_ACCOUNT_NAME_LEN`）で制御文字を含まず、パスワードは 8〜128 文字。
5. 成功時は `session_id` Cookie を設定して `{ "status": "ok" }` を返す。SPA は `return_to` へ戻り、アプリ mount の `GET /api/session` が account / session を保存して部屋鍵フラグメントを復元する。
6. local 有効時は `POST /api/auth/local/login` だけを登録し、`/auth/google/callback` と `/auth/google/token` は登録しない（404）。Google 有効時は local API が 404 になる。
7. local でも banquette は通常どおり cellar へ永続化する（`--no-persist` は存在しない）。強制終了時は最大 heartbeat 分だけ失われる。

### session 失効

`spawn_session_cleanup_task` は次回のローカル 4:00 まで sleep し、`cleanup_sessions_at` を実行する。`previous_4am_local` は実行時刻が 4:00〜4:05 の場合は前日 4:00 を返し、それ以外は当日 4:00 を返す。`last_heartbeat` が cutoff より前の session を削除する。

### WebSocket 接続

1. クエリ・ヘッダ・Cookie から account / session を解決する。
2. `account_id` があり session がない場合は **401**。`account_id` + session は `is_session_valid` で一致を検証し、`session_id` だけなら session から account を導出する。どちらもない場合だけ匿名として扱う。
3. `rooms` を引き、未登録なら **404**。`get_or_create_room` は呼ばない。
4. 登録済みでも、匿名かつ `done=false` は **403**。account に role がなく `done=false` も **403** と「全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください」を返す。`argenterie` はこの拒否を門前払い画面（`GateScreen`）として表示し、アカウントのサインイン・切り替えを促す。`done=true` の非メンバー / 匿名は `viewer` として接続できる。
5. bridge の `admin_account` は DB の GM（`room_permissions.role = 'gm'`）、なければ `rooms.creator_account` から決める。接続順や接続者を admin にしない。
6. `RoomBridgeManager::get_or_create_room` で部屋の bridge を取得する。bridge は banquette を起動してローカル transport へ接続する。head の有無は maitre-d では判定せず、banquette が cellar の alias を読んで復元する。復元に失敗した場合、この時点で **500** を返し、空部屋を起動しない。
7. bridge に client を登録し、`ClientConnected` を banquette へ送る。初期 scene は `/`。
8. クライアントからの `ClientFrame` は JSON または postcard で受け取り、viewer の mutate を遮断してから bridge へ渡す。遮断は `room_permissions` の `viewer` 行だけを見る暫定ゲートで、`banquette` 側の権限表とは独立している（#200 で一本化）。
9. banquette からの `ServerFrame` は client_id 宛、scene 宛、room 全体宛に振り分ける。
9. banquette からの `ServerFrame` は client_id 宛、scene 宛、room 全体宛に振り分ける。head を報告する `RoomPersisted` は #192 で撤去したので、maitre-d が head を記録する経路はない。
10. banquette からの `ServerFrame::PermissionsUpdated` を受けたときだけ bridge の in-memory 権限を更新し、そのままクライアントへ転送する。`SetMemberPermission` / `SetDefaultPermission` のリクエスト時点では DB / shadow を更新しない。
11. banquette からの `AccountPermissionResponse` は `QueryAccountPermission` の応答として `GET /api/rooms/{id}` と WS ゲートが使う。

`is_mutate_frame` は `AppendLog`、`EditLog`、`DeleteLog`、`RewindLog`、`UpdateState`、`DeleteState`、`CreateScene`、`DeleteScene`、`SetMemberPermission`、`SetDefaultPermission`、`CreateCharacter`、`UpdateCharacter`、`DeleteCharacter`、`MoveCharacter` を mutate とみなす。旧 `SetDone` は #199 で撤去した。

### cellar proxy

1. `resolve_caller` が Cookie `session_id` / クエリ / ヘッダから session を解決し、DB から account を引く。リクエストの `owner` / `session_id` は認可に使わない。
2. `/api/cellar/store` の JSON body は `{ tin_id, size, wrapped_key }` のみを受理し、`X-Alhalte-Owner` / `X-Alhalte-Session-Id` を session 由来の値で付与してから body をそのまま cellar へ送る。body に `owner` / `session_id` が入っていれば 400。クライアントは owner を詐称できない（クエリ / ヘッダの `owner` が session と食い違えば 403）。
3. `/api/cellar/store/{tin_id}` は `X-Alhalte-Owner` / `X-Alhalte-Session-Id` を付与し、`X-Alhalte-Wrapped-Key` と `Content-Type` を透過する。`tin_id` は 64 hex を検証してから転送する。
4. `GET /api/cellar/tin/{tin_id}` は session を検証するが、`X-Alhalte-Owner` / `X-Alhalte-Session-Id` を上流へ転送しない。cellar は tin id に一致する任意の参照を解決する（`owner` は参照ラベルの絞り込みにすぎない）。認証は proxy が担い、機密性は部屋鍵（`X-Alhalte-Wrapped-Key`）が担保するので、`X-Alhalte-Wrapped-Key` をレスポンスへ透過する（Issue #245）。
5. 上流の status / body はそのまま返し、接続失敗は 502、timeout は 504 に変換して内部 URL を漏らさない。cellar 未設定時は 503。

### 部屋作成

1. `POST /api/rooms` は session を検証する（401）。
2. `room_id` があれば `RoomId` として parse し、canonical 16 hex に正規化する。不正なら 400。なければ非ゼロのランダム u64 を生成する。
3. 名前を trim し、空・制御文字を含む・`MAX_ROOM_NAME_LEN`（100 文字）超を 400 で拒否する。
4. `rooms` に insert する。作成者は `creator_account` として記録し、banquette spawn 時の初期 Admin にだけ使う。既存 ID は 409 Conflict。
5. banquette は起動しない。最初の WS / export までプロセスは作られない。

### 権限管理

`rooms(room_id, name, creator_account, created_at)` が部屋の registry で、`room_id` は `RoomId` の正規形（16 hex 小文字）。権限は `banquette` の `RoomData.default_permission` / `permissions` が正で、`maitre-d` は権限テーブルを持たない。`RoomRole` は `admin` / `participant` / `viewer` / `denied` の transport DTO にすぎない。

`GET /api/rooms/{id}` / WebSocket / export は、bridge を spawn した後に `QueryAccountPermission` で banquette に問い合わせ、`Denied` を 403 / 4403 で拒否する。`creator_account` は spawn 時の初期 Admin にだけ使う。接続順や DB role から admin を決めない。

HTTP 権限 API は banquette への薄い proxy で、`QueryPermissions` / `SetMemberPermission` / `SetDefaultPermission` を転送する。DB は読まない/書かない。`rooms.done` と `room_permissions` は撤去済みで、旧行が残っている DB は起動時に `LegacyRoomPermissions` / `LegacyRoomDone` で拒否する（移行なし）。pre-spawn の `done` gate は存在しない。

### ZIP export

1. `type` が `full` / `logs` 以外なら 400。
2. account と session が片方だけなら 401。両方あれば session を検証する。
3. `rooms` を引き、未登録なら **404**。登録済みでも banquette に問い合わせた caller の実効権限が `Denied` なら **403**。どちらも banquette を起動しない。
4. bridge の admin は WS と同じく `rooms.creator_account` から決める。権限の正は banquette の `RoomData` で、`maitre-d` は権限表を持たない。
5. `get_or_create_room` で bridge を取得する。banquette が cellar の head alias から部屋を復元してから `ExportRoom` に応答する。復元に失敗した場合は **500** を返し、空の ZIP を返さない。
6. bridge に `ExportRoom { caller_account, include_state: type == "full" }` を送り、banquette が組み立てた ZIP bytes を受け取る。scene の絞り込みと `logs` の state 消去は banquette が行う（#193）。
7. 受け取った bytes に `application/zip` と `Content-Disposition` を付けて返す。`maitre-d` は ZIP を組み立てない。

### 部屋 head と復元

- `maitre-d` は部屋 head を持たない。`room_id → RoomData tin id` のポインタは cellar の alias `room/<room_id>/head` にあり、banquette が persist のたびに更新する（Issue #192）。
- `get_or_create_room` は spawn 前に何も読まず、`spawn_banquette(room_id, admin)` を呼ぶ。
  - `ProcessBanquetteSpawner` は `--room-tin` を渡さない（引数自体が廃止された）。
  - `InProcessBanquetteSpawner` は banquette と同じ `Room::load_or_create` を呼ぶ。
- banquette は alias が無ければ `Room::new` で新しい部屋を作り、あれば `Room::load_from_cellar` で復元する。
- 復元失敗（tin 不在・復号不能・内容 hash 不一致・room_id 不一致・root scene 不在）は spawn の失敗として扱い、WS は 500、export は 5xx を返す。maitre-d は空部屋にフォールバックしない。
- 旧 head の unregister は banquette が行う。`maitre-d` は head を記録しないので、置き換え前の tin id も知らない。

- local-login も通常の Google 経路と同じく、spawn した banquette に `--service-token-file` を伝播し、heartbeat ごとに cellar の head alias を更新する。`--no-persist` は撤去した。

## データベース

SQLite のテーブル:

```sql
accounts(account_id INTEGER PRIMARY KEY, sub_hash TEXT NOT NULL UNIQUE,
         handle_name TEXT, icon_tin_id BLOB, created_at INTEGER)
sessions(session_id INTEGER PRIMARY KEY, account_id INTEGER, last_heartbeat INTEGER,
         created_at INTEGER)
rooms(room_id TEXT PRIMARY KEY, name TEXT NOT NULL, creator_account INTEGER NOT NULL,
      created_at INTEGER NOT NULL)
oauth_states(state TEXT PRIMARY KEY, return_to TEXT NOT NULL, created_at INTEGER NOT NULL)
```

`accounts.sub_hash` は `UNIQUE` で、account の同一性は生の Google `sub` ではなく salt 付き 128bit hash で決まる（`SubHash`、32 hex。`maitre-d` に逆変換はなく、hash から `sub` は復元できない）。session は account に外部キーで紐づき、account 削除時に cascade する。`room_id` は文字列（`RoomId` の 16 hex 正規形）。`rooms` が部屋の第一級エンティティで、権限も head も持たない（権限の正は `banquette` の `RoomData`、head は cellar の `room/<room_id>/head` alias）。

`oauth_states` はログイン中の OAuth state を `return_to` と結びつけて保持する。`consume_oauth_state` は同じ transaction で SELECT と DELETE を行い、単回使用を保証する。`created_at` が 10 分（`OAUTH_STATE_TTL_SECS`）より古い state は削除して `None` を返す。`create_oauth_state` は期限切れ行を opportunistic に削除する。

`accounts` には migration がない。Issue #190 で復号可能な `encrypted_sub` を one-way な `sub_hash` に置き換えた際、既存 account は存在しない前提で互換性を切っている。旧 schema（`accounts.encrypted_sub` がある、または `sub_hash` がない `accounts`）を検出すると、起動時に `MaitreDError::LegacySubSchema` で明示的に失敗する（自動移行も破壊的な作り直しもしない）。

`--local-login` は同じスキーマを持つ **別の SQLite ファイル**（既定 `maitre-d-local.sqlite3`）を開く。local account は `sub_hash` に `alhalte/local-login/<account 名>` の hash が入り、`accounts.password_hash` に Argon2id の PHC 文字列を持つ。分離はファイル単位なので、local と Google で account / session / rooms が混ざることはない。代償として rooms registry も分かれるため、local で作った部屋は Google モードの `rooms` には現れない（Issue #287）。

起動時の schema 処理:

1. `accounts` が旧 `encrypted_sub` schema、または `sub_hash` を持たない場合は `LegacySubSchema` で起動を拒否する。
2. `room_heads` テーブルに行が残っていれば `LegacyRoomHeads` で起動を拒否する。空ならテーブルを drop する（Issue #192。移行なし）。
3. `room_permissions` テーブルに行が残っていれば `LegacyRoomPermissions` で起動を拒否する。空ならテーブルを drop する。
4. `rooms.done` 列があり、`done <> 0` の行があれば `LegacyRoomDone` で起動を拒否する。空なら列を drop する。

## 既知の差分・制約

- 部屋の権限の正は `banquette` の `RoomData.permissions` / `default_permission`（4 段階）にある。`maitre-d` は権限テーブルを持たず、`RoomRole` は transport DTO にすぎない。旧 `room_permissions` / `rooms.done` の行が残っている DB は起動時に拒否する（移行なし）。
- `GET /api/rooms/{id}` / WS / export は spawn 後に `QueryAccountPermission` を問い合わせ、`Denied` を 403 / 4403 で拒否する。pre-spawn の `done` gate はない。匿名は account_id `0` として default permission を問い合わせる。
- 旧 `gm` / `pl` の serde alias は削除した。role は `admin` / `participant` / `viewer` / `denied` のみ。
- `maitre-d` は部屋 head を持たない。`room_heads` テーブルと `get_room_head` / `set_room_head` / `--room-tin` / `RoomPersisted` 処理は #192 で撤去した。旧 `room_heads` に行が残る DB は起動時に `LegacyRoomHeads` で拒否する（移行なし、DB リセット）。
- 旧 head の unregister もしない。banquette が alias を移した後に自分の直前 head を解放する。
- 内部資格情報は alhalte-restaurant が `Restaurant.toml` の age armor 値を復号し、0600 の file に書いて `--client-secret-file` / `--service-token-file` で渡す。argv / `ps` / systemd unit に秘密値は出ない。詳細は [alhalte-bistrot](../alhalte-bistrot/ARCHITECTURE.md)。
- `get_or_create_room` は接続待ちの間（最大約 3 秒）manager の write lock を保持する。部屋数の多い同時接続ではボトルネックになり得る。
- `ProcessBanquetteSpawner` が起動した子プロセスは maitre-d 側で reap しない。maitre-d が再起動すると、古い banquette が残っていれば同じ socket path と head alias を奪い合う。systemd の cgroup（既定 `KillMode=control-group`）で一緒に停止する運用を前提とする。
- `POST /api/rooms` は `rooms` に行を書くだけで、権限行も banquette も作らない。最初の WS / export で起動し、creator が初期 Admin になる。
- `GET /api/rooms`（自分の role を持つ部屋一覧）は未実装。
- debug ビルド限定の `POST /api/debug/account` は「`alhalte/debug/` + 32 byte 乱数」を合成 `sub` として `hash_sub` し、通常の account と同じ経路で作る（テスト用。旧 `encrypted_sub = "0"` の特別扱い（`decrypt_sub`）は #190 で撤去）。これは認証モードではなく、local-login でも通常 DB でも同じ active DB に書く。
- `--local-login` はパスワード付きのローカル認証。`/api/auth/local/login` に CSRF token もレート制限もないため、信頼できる LAN 専用とする。パスワードは Argon2id で保存し、HTTP で送られる。
- local-login でも部屋状態は通常どおり cellar に永続化する。強制終了時に失われるのは最大 heartbeat（Windows パッケージは 60 秒）分。
- local-login は別 DB なので、rooms registry も Google 用 DB と分かれる。両モードで部屋を共有したい場合は rooms を別 DB に分離する設計変更が必要（Issue #287）。
- CORS は明示的な origin allow-list。`--cors-origin` 未指定時はローカル開発 origin のみで、本番では alhalte-restaurant が maitre-d public URL / domain から生成した値を渡す。reverse proxy 側での制限も併用できる。
- OAuth の `redirect_uri` は `--public-url` から作る固定値だけで、UI の origin / port は Google に渡さない。旧 `GET /auth/google/url` / `POST /auth/google/callback` は削除した（互換性なし）。`GET /auth/login` の `return_to` は CORS allow-list の origin のみ許可し、`//evil`・別 origin・`javascript:` / `data:`・userinfo 付き URL は 400。
- callback の `session_id` Cookie は `{public_url}` の origin に付く。ブラウザから `/api/session` を Cookie 付きで呼ぶには、`public_url` の origin と UI の origin が一致している必要がある。ローカル開発では maitre-d が SPA を同一 origin で配信し、`--public-url` もその origin になる（Vite dev は任意）。本番は reverse proxy で UI と API を同一 origin に揃える。
- `GET /api/session` は Cookie / `X-Alhalte-Session` / クエリのいずれかで session を解決し、未認証は 401。`account` は `AccountResponse` で `sub_hash` を含まない。
- cellar proxy はリクエスト / レスポンスを全量メモリにバッファする（`reqwest` / `Bytes`、上限 64 MiB）。大きな Tin ほど maitre-d のメモリを消費する。

