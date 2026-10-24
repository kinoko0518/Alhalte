# alhalte-maitre-d

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-maitre-d`  
> テスト: `crates/alhalte-maitre-d/tests/spec_tests.rs`

## 責務

ブラウザ（cutlery）が直接通信する唯一の HTTP / WebSocket ゲートウェイ（玄関口）である。以下の責務を担う。

- Google OAuth 2.0 およびローカルログイン（アカウント名 + Argon2id パスワード）による認証、アカウント作成・検索・更新、セッション管理。
- 部屋レジストリ（`rooms` テーブル）の管理と、必要に応じた `alhalte-banquette` 子プロセスのオンデマンド起動・監視・グレースフルシャットダウン・回収（reap）。
- クライアント WebSocket 接続の終端と、部屋ごとのローカル IPC（UNIX ドメインソケット / Windows 名前付きパイプ）への多重化・逆多重化（mux / demux）。
- `alhalte-cellar` に対する認証付きリバースプロキシ（`/api/cellar/*`）。
- ビルド済み SPA（`--web-root`）およびステージング済み公式拡張機能（`/extensions/*`）の同一オリジン配信。

## 公開 API / 外部接続点

### CLI（`alhalte-maitre-d` バイナリ）

| 引数 / 環境変数 | 既定値 | 内容 |
| --- | --- | --- |
| `--host` / `ALHALTE_HOST` | `127.0.0.1` | 待ち受けホスト。 |
| `--port` / `ALHALTE_PORT` | `8080` | 待ち受けポート。 |
| `--db-path` / `ALHALTE_DB_PATH` | `maitre-d.sqlite3` | SQLite データベースのパス。`--local-login` 有効時は同じディレクトリ内の `maitre-d-local.sqlite3` に自動分離される。 |
| `--salt` / `ALHALTE_SALT` | 必須 | アカウント識別子（Google `sub` 等）を BLAKE3 key 導出でハッシュ化するためのインスタンス固有ソルト。 |
| `--local-login` / `ALHALTE_LOCAL_LOGIN` | `false` | Google OAuth の代わりにアカウント名 + パスワードによるローカル認証を有効にする。 |
| `--google-client-id` / `ALHALTE_GOOGLE_CLIENT_ID` | `--local-login` 時は不要 | Google OAuth クライアント ID。 |
| `--client-secret-file` / `ALHALTE_GOOGLE_CLIENT_SECRET_FILE` | 任意 | Google OAuth クライアントシークレットを格納したファイルのパス。 |
| `--google-client-secret` / `ALHALTE_GOOGLE_CLIENT_SECRET` | 任意 | クライアントシークレットの直接指定（後方互換用。`--client-secret-file` を優先）。 |
| `--public-url` / `ALHALTE_PUBLIC_URL` | `http://{host}:{port}` | 外部公開ベース URL。OAuth の固定 `redirect_uri`（`{public_url}/auth/google/callback`）や Cookie の `Secure` 属性判定、CORS 許可オリジンの基準に使われる。 |
| `--allowed-origin` / `ALHALTE_ALLOWED_ORIGINS` | 任意（カンマ区切り） | 追加で許可する CORS オリジンおよび OAuth `return_to` オリジン。 |
| `--cellar-url` / `ALHALTE_CELLAR_URL` | `http://127.0.0.1:8081` | `alhalte-cellar` のベース URL。`CellarProxy` および spawn する `banquette` に渡す。 |
| `--service-token-file` / `ALHALTE_CELLAR_SERVICE_TOKEN_FILE` | 任意 | `cellar` の内部書き込み認証トークンを格納したファイルのパス。spawn する `banquette` にそのまま伝播する。 |
| `--service-token` / `ALHALTE_CELLAR_SERVICE_TOKEN` | 任意 | 内部書き込み認証トークンの直接指定（後方互換用）。 |
| `--socket-dir` / `ALHALTE_SOCKET_DIR` | `/tmp/alhalte-sockets` | `banquette` とのローカル IPC ソケットを配置するディレクトリ（Windows では名前付きパイプ名にハッシュ化される）。 |
| `--banquette-bin` / `ALHALTE_BANQUETTE_BIN` | `alhalte-banquette` | 部屋プロセスとして起動する実行ファイルのパス（未指定時は現行実行ファイルの隣接バイナリを優先）。 |
| `--chunk-size-exponent` / `ALHALTE_CHUNK_SIZE_EXPONENT` | `8` | spawn する `banquette` に渡すチャンクサイズ指数 $n$（$2^n$ 件）。 |
| `--heartbeat-interval-secs` / `ALHALTE_HEARTBEAT_INTERVAL_SECS` | `240` | spawn する `banquette` に渡すハートビート・永続化間隔（秒）。 |
| `--max-tin-size` / `ALHALTE_MAX_TIN_SIZE` | `67108864` (64 MiB) | `/api/cellar/store/{tin_id}` が受け付ける最大ボディサイズ（バイト）。spawn する `banquette` にも `--max-tin-size` として伝播する（Issue #299）。 |
| `--web-root` / `ALHALTE_WEB_ROOT` | 任意 | ビルド済み SPA（`alhalte-argenterie/dist-app`）のディレクトリパス。指定時は静的ファイルと `index.html` フォールバックを配信する。 |
| `--extensions-dir` / `ALHALTE_EXTENSIONS_DIR` | `<data-dir>/extensions` | ステージング済み公式拡張機能（`manifest.json` と `.wasm`）のディレクトリ。`/extensions/*` で配信する。 |

### HTTP エンドポイント

#### 認証・セッション・アカウント

| メソッド・パス | 認証 | 内容 |
| --- | --- | --- |
| `GET /auth/login?return_to=...&prompt=...` | 不要 | サインイン開始エンドポイント。Google OAuth モードでは `return_to` を検証して `oauth_states` に保存し、Google 認可 URL へ 302 リダイレクトする。`--local-login` モードでは SPA の `/login?return_to=...` へ 302 リダイレクトする。 |
| `GET /auth/google/callback?code=...&state=...` | 不要 | Google OAuth の固定コールバック。`state` を単発消費してトークン交換・ID トークン検証を行い、`session_id` Cookie（`SameSite=Lax; Path=/`）を発行して `return_to` へ 302 リダイレクトする。 |
| `POST /auth/google/token` | 不要 | `{ id_token, handle_name? }` を検証してアカウントとセッションを発行する API。 |
| `POST /api/auth/local/login` | 不要 | `--local-login` 専用。`{ account_name, password }` を受け取り、未登録なら Argon2id ハッシュで新規作成、登録済みなら照合して `session_id` Cookie と `AuthResponse` を返す（誤パスワードは 401、Google モード時は 404）。 |
| `GET /api/session` | 要セッション | Cookie・`X-Alhalte-Session` ヘッダ・クエリのいずれかからセッションを解決し、`{ account, session_id }` を返す。サインイン後の `completePendingSignIn()` が利用する（Issue #294）。 |
| `GET /api/account/{account_id}` | 要セッション | 指定アカウントの公開プロフィール（`account_id`, `handle_name`, `icon_tin_id`, `created_at`）を返す。`sub_hash` は含めない。 |
| `PATCH /api/account/{account_id}` | 要本人セッション | 表示名（`handle_name`）やアイコン（`icon_tin_id`）を更新する。 |
| `GET /api/accounts/search?q={query}` | 要セッション | ハンドル名の部分一致またはアカウント ID でアカウントを検索し、最大 20 件返す。 |
| `POST /api/debug/account` | 不要（debug ビルド限定） | テスト用の合成アカウントとセッションを発行する（`#[cfg(debug_assertions)]` のみ有効）。 |

#### 部屋管理・権限・エクスポート

| メソッド・パス | 認証 | 内容 |
| --- | --- | --- |
| `POST /api/rooms` | 要セッション | `{ name, room_id? }` で部屋を `rooms` テーブルに登録し、呼び出し元を `creator_account` として記録する。この時点では `banquette` をまだ起動せず、`role: "admin"` を含む `RoomInfoResponse` を返す。 |
| `GET /api/rooms/{room_id}` | 要セッション | 入室プリフライト。`rooms` 未登録なら 404。登録済みなら `banquette` ブリッジを起動・接続して `QueryAccountPermission` で呼び出し元の権限を照会し、`Denied` なら 403、それ以外は `{ room_id, name, role }` を 200 で返す。 |
| `GET /api/room/{room_id}/permissions` | 要セッション | `banquette` に `QueryPermissions` を送り、返された権限マップに `accounts` テーブルの表示名・アイコン URL を結合して返す。 |
| `POST /api/room/{room_id}/permissions` | 要セッション | `{ account_id, role }` を受け取り、`banquette` に `SetMemberPermission` を転送する（`Admin` 以外は `banquette` が拒否して 403）。 |
| `DELETE /api/room/{room_id}/permissions/{account_id}` | 要セッション | 対象アカウントに `RoomPermissionLevel::Denied` を設定する `SetMemberPermission` を `banquette` に送る。 |
| `GET /api/room/{room_id}/export/zip?type=full\|logs`（別名 `/room/{room_id}/export/zip`） | セッション任意 | `rooms` 未登録なら 404。`banquette` に実効権限を照会して `Denied` なら 403。許可された場合は `ExportRoom` を送り、`banquette` が生成した ZIP バイト列を `application/zip` で返す。 |

#### 認証付き Cellar リバースプロキシ（`/api/cellar/*`）

ブラウザは `alhalte-cellar` に直接アクセスせず、必ず `maitre-d` の `/api/cellar/*` を経由する。

| メソッド・パス | 認証 | 転送先と動作 |
| --- | --- | --- |
| `POST /api/cellar/store` | 要セッション | クライアントからは `{ tin_id, size, wrapped_key }` のみを受け取る（ボディに `owner` や `session_id` が含まれていれば 400 で拒否）。セッションから解決したアカウント ID を `owner` として注入し、`cellar` の `POST /store` へ転送する。 |
| `PUT /api/cellar/store/{tin_id}` | 要セッション | ボディ上限は `--max-tin-size`（既定 64 MiB、Issue #299）。セッションから解決した `X-Alhalte-Owner`（および必要に応じて `X-Alhalte-Wrapped-Key`）を付与して `cellar` の `PUT /store/{tin_id}` へ転送する。 |
| `GET /api/cellar/tin/{tin_id}` | 不要（公開取得） | `cellar` の `GET /tin/{tin_id}` へ転送し、レスポンスボディと `content-type` / `content-disposition` / `x-alhalte-wrapped-key` ヘッダをブラウザへ返す。セッションがあれば `owner` クエリを付与して所有者固有の `wrapped_key` 解決を補助する。 |

### WebSocket エンドポイント

- パス: `/ws/room/{room_id}`（別名 `/room/{room_id}`、`/room/{room_id}/ws`、および `room` クエリ / `X-Alhalte-Room` ヘッダ付きの `/ws`）。
- 認証パラメータ: `session_id` Cookie、`X-Alhalte-Session` / `X-Alhalte-Account` ヘッダ、または `?account_id=...&session_id=...` クエリ。
  - `account_id` のみで `session_id` が無い場合、またはセッションと `account_id` が一致しない場合は `401 Unauthorized` で拒否する。
  - 有効な `session_id` があれば対応する `account_id` を採用する。
  - どちらも無い場合は匿名接続（`account_id = 0, session_id = 0`）として扱う。
- 認可: `rooms` に未登録の `room_id` は `404 Not Found` で拒否し、`banquette` を起動しない。登録済みの場合は `banquette` ブリッジを起動・取得し、`QueryAccountPermission(auth_account)` で実効権限を照会する。結果が `Denied` の場合は WebSocket アップグレード前に `403 Forbidden` で拒否する。
- アップグレード後: クライアントからは JSON テキスト（または `postcard` バイナリ）で `ClientFrame` を受け取り、そのまま `MaitreDToBanquette::ClientMessage` として `banquette` へ転送する。`banquette` からの `ServerFrame` は JSON テキストとしてクライアントへ送信する。権限検査はすべて `banquette` が担い、`maitre-d` 側でフレームの遮断や書き換えは行わない。

## データ形式

### SQLite スキーマ

Google OAuth モードでは `--db-path`（既定 `maitre-d.sqlite3`）、`--local-login` モードでは同じディレクトリの `maitre-d-local.sqlite3` を使用する。両モードの DB はテーブルも部屋も共有しない。

- **`accounts`**:
  - `account_id INTEGER PRIMARY KEY AUTOINCREMENT`
  - `sub_hash BLOB NOT NULL UNIQUE`: Google の `sub`（またはローカルログイン識別子）をインスタンスの `salt` と BLAKE3 `derive_key("alhalte/maitre-d/sub-hash v1", ...)` でハッシュ化した 32 バイト値。平文の `sub` は一切保存せず、ハッシュ計算後にメモリ上からゼロ消去（zeroize）する。
  - `handle_name TEXT NOT NULL`
  - `icon_tin_id BLOB`: 任意（32 バイトの `TinId`）。
  - `created_at INTEGER NOT NULL`
- **`local_credentials`**（`--local-login` 用）:
  - `account_name TEXT PRIMARY KEY`: 正規化済みのアカウント名（1〜64 文字、制御文字不可）。
  - `password_hash TEXT NOT NULL`: Argon2id（PHC 文字列形式）のパスワードハッシュ。
  - `account_id INTEGER NOT NULL UNIQUE REFERENCES accounts(account_id) ON DELETE CASCADE`
  - `created_at INTEGER NOT NULL`
- **`sessions`**:
  - `session_id INTEGER PRIMARY KEY`: 暗号論的乱数による非ゼロの `u64`（SQLite には `i64` ビットパターンで格納）。
  - `account_id INTEGER NOT NULL REFERENCES accounts(account_id)`
  - `created_at INTEGER NOT NULL`
  - `expires_at INTEGER NOT NULL`: 発行から 30 日（`SESSION_TTL_SECS = 2_592_000`）。
- **`oauth_states`**:
  - `state TEXT PRIMARY KEY`: 32 バイト乱数の 64 桁 hex。
  - `return_to TEXT NOT NULL`: リダイレクト先の URL（オリジン検証済み）。
  - `created_at INTEGER NOT NULL`、`expires_at INTEGER NOT NULL`（TTL 10 分、単発消費）。
- **`rooms`**:
  - `room_id TEXT PRIMARY KEY`: 正規化された 16 桁小文字 hex。
  - `name TEXT NOT NULL`: 部屋名（1〜100 文字、制御文字不可）。
  - `creator_account INTEGER NOT NULL`: 部屋を作成したアカウント ID。初回 `banquette` 起動時の `--admin-account` に渡される。
  - `created_at INTEGER NOT NULL`

※ 旧スキーマに存在した `room_permissions`、`room_heads`、`rooms.done`、`accounts.sub_encrypted` が残っている DB は、データ不整合や意図しない権限リセットを防ぐため起動時に `LegacySchemaUnsupported` 等で拒否する（fail-closed）。

## 内部フロー

### サインイン（Google OAuth / ローカルログイン）とセッション復元（Issue #294）

1. クライアント（`argenterie`）の `beginSignIn()` は、部屋鍵フラグメントを含む現在の完全な URL を `sessionStorage` に保存した上で、フラグメントを除いた URL を `return_to` パラメータに指定して `GET /auth/login?return_to=...` へ画面遷移する。
2. **Google OAuth モード (`--local-login` 無効時)**:
   - `maitre-d` は `return_to` のオリジンが許可リスト（`public_url`、`allowed_origins`、または `host = 0.0.0.0` 時のプライベート LAN オリジン）に含まれるか検証し、`oauth_states` にワンタイム `state` を保存して Google 認可画面へ 302 リダイレクトする。
   - 認可後、Google から固定コールバック `GET /auth/google/callback` に戻る。`maitre-d` は `state` を単発消費して認可コードをトークンに交換し、`sub` から `sub_hash` を導出してアカウントを取得または作成する。
   - 新規セッションを発行し、`Set-Cookie: session_id=...; Path=/; SameSite=Lax`（HTTPS 時は `; Secure`）を付与して `return_to` へ 302 リダイレクトする。
3. **ローカルログインモード (`--local-login` 有効時)**:
   - `GET /auth/login` は同一オリジンの `/login?return_to=...` へ 302 リダイレクトし、SPA が `LoginScreen` を表示する。
   - ユーザーがアカウント名とパスワードを入力すると、`POST /api/auth/local/login` が Argon2id で照合（未登録なら新規作成）し、`session_id` Cookie を設定してレスポンスを返す。その後クライアントは `return_to` へ遷移する。
4. **セッション復元**:
   - リダイレクト先のアプリマウント時に `completePendingSignIn()` が `GET /api/session` を Cookie 付きで呼び出し、`account` と `session_id` を受け取って保存するとともに、`sessionStorage` に退避していた元の URL（部屋鍵フラグメントを含む）を `history.replaceState` で復元する（Issue #294）。

### `ProcessBanquetteSpawner` と部屋プロセスのライフサイクル管理（Issue #291, #292）

1. **オンデマンド起動**:
   - `RoomManager::get_or_create_room(room_id, creator_account)` は、対象部屋のアクティブな `RoomBridge` が存在すればそれを再利用し、無ければ `BanquetteSpawner::spawn` を呼び出す。
   - `ProcessBanquetteSpawner` は `alhalte-banquette` 子プロセスに `--room-id`、`--socket`、`--admin-account`、`--cellar-url`、`--chunk-size-exponent`、`--heartbeat-interval-secs`、`--max-tin-size`、`--service-token-file` を渡して起動し、ローカルソケット（または名前付きパイプ）への接続が確立するまで指数バックオフで待機する。
   - `banquette` は起動時に自ら `cellar` の `room/<room_id>/head` エイリアスから状態を復元する。復元に失敗して `banquette` が即座に終了した場合、`maitre-d` は 500 エラーを返し、空の部屋で上書きしない。
2. **子プロセスの保持・監視・回収 (Reap)（Issue #292）**:
   - `ProcessBanquetteSpawner` は起動した各部屋の `tokio::process::Child` ハンドルを内部テーブルに保持し、プロセスの終了を監視して確実に `wait`（reap）する。これによりゾンビプロセスの残留を防ぐ。
   - 同じ `room_id` を再起動する際、もし古い `banquette` 子プロセスが残っていれば、新しいプロセスを起動する前に古いプロセスを停止・回収し、ソケットパスや `room/<room_id>/head` エイリアスの競合を防ぐ。
3. **グレースフルシャットダウン（Issue #291, #292）**:
   - 部屋のシャットダウン時または `maitre-d` 自身の停止時には、アクティブな各 `RoomBridge` から `MaitreDToBanquette::ShutdownRoom` フレームを送信する。
   - `banquette` は `ShutdownRoom` を受けて最終状態を `cellar` に永続化してから終了する。`ProcessBanquetteSpawner` は子プロセスの正常終了をタイムアウト付きで待機し、期限内に終了しない場合のみ強制終了（kill）して reap する。これにより Unix と Windows の双方で未保存状態の消失と孤児プロセスの残留を防ぐ。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- Google の生 `sub`、OAuth クライアントシークレット、ローカルログインの平文パスワード、`cellar.service-token` は DB・ログ・エラーレスポンス・コマンドライン引数（cafe / bistrot / glacier 経由時）に出力しない。
- `POST /api/cellar/store` でクライアントが `owner` または `session_id` フィールドを送った場合は `400 Bad Request` で拒否し、所有者の偽装を許さない（Issue #191）。
- `/api/cellar/store/{tin_id}` のリクエストボディ上限は `--max-tin-size`（既定 64 MiB）で制限し、超過時は `413 Payload Too Large` を返す（Issue #299）。

## 既知の差分・制約

- `/api/cellar/store/{tin_id}` および `/api/cellar/tin/{tin_id}` は転送対象の `Tin` バイト列を `Bytes` としてメモリ上にバッファリングする（最大 `--max-tin-size`、既定 64 MiB）。
- `POST /api/debug/account` は `#[cfg(debug_assertions)]` でのみビルドに含まれ、リリースバイナリには存在しない。
