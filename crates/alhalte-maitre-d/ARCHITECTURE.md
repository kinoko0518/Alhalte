# alhalte-maitre-d

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-maitre-d`  
> テスト: `crates/alhalte-maitre-d/tests/maitre_d_tests.rs`

## 責務

restaurant の認証・玄関口。Google OAuth で `sub` だけを継承し、インスタンス固有の salt で暗号化して account を作る。64bit の session を発行・失効させる。HTTP API と WebSocket を提供し、部屋 ID ごとに通信を demultiplex して `alhalte-banquette` の UNIX ソケットへ流す。部屋の権限管理 API と ZIP export も持つ。

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
| `GET` / `POST` | `/api/room/{room_id}/permissions` | 部屋の権限一覧取得・追加/更新。 |
| `DELETE` | `/api/room/{room_id}/permissions/{account_id}` | 権限削除。 |
| `GET` | `/api/room/{room_id}/export/zip?type=full|logs` | 部屋またはログの ZIP。 |
| `POST` | `/api/debug/account` | debug ビルドのみ。`sub = "0"` の account を作る。 |

`/accounts/search`、`/room/{room_id}/permissions`、`/room/{room_id}/export/zip` など、`/api` なしの別名も登録されている。CORS は `CorsLayer::permissive()`。

### WebSocket

| パス | 内容 |
| --- | --- |
| `/ws` | クエリ `room` またはヘッダ `X-Alhalte-Room` で部屋を指定。 |
| `/ws/room/{room_id}` | パスで部屋を指定。 |
| `/room/{room_id}` | 招待 URL 互換。 |
| `/room/{room_id}/ws` | 同上。 |

認証情報はクエリ `account_id` / `session_id`、ヘッダ `X-Alhalte-Account` / `X-Alhalte-Session`、Cookie `session_id` の順で解決する。`session_id` だけがあり `account_id` がない場合は、session から account を引く。

### 外部接続

| 接続先 | 内容 |
| --- | --- |
| Google OAuth | `accounts.google.com` の認可 URL、`oauth2.googleapis.com/token`、`tokeninfo`。 |
| `alhalte-banquette` | UNIX ソケット。`ProcessBanquetteSpawner` または `InProcessBanquetteSpawner` が起動し、長さ前置 postcard で通信する。 |
| `alhalte-kitchen` | ZIP export のためにライブラリとして呼ぶ。 |

CLI（`CliArgs`）: `--host`、`--port`、`--db-path`、`--client-id`、`--client-secret`、`--salt`、`--socket-dir`、`--banquette-bin`、`--cellar-url`、`--unix-socket`。

### 主な公開型

- `Database`、`Account`、`Session`、`RoomRole`、`RoomPermissionItem`
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
3. `room_permissions` に行がある部屋で、account に role がない場合は `done` を確認する。`done` でなければ 403 と「全体公開されていない部屋です。部屋の管理者に権限をリクエストしてください」を返す。`argenterie` はこの拒否を門前払い画面（`GateScreen`）として表示し、アカウントのサインイン・切り替えを促す。
4. `RoomBridgeManager::get_or_create_room` で部屋の bridge を取得する。bridge は banquette を起動し、UNIX ソケットへ接続する。接続完了までは `mpsc::unbounded_channel` にフレームをキューする。
5. bridge に client を登録し、`ClientConnected` を banquette へ送る。初期 scene は `/`。
6. クライアントからの `ClientFrame` は JSON または postcard で受け取り、viewer の mutate を遮断してから bridge へ渡す。
7. banquette からの `ServerFrame` は client_id 宛、scene 宛、room 全体宛に振り分ける。

`is_mutate_frame` は `AppendLog`、`EditLog`、`DeleteLog`、`RewindLog`、`UpdateState`、`DeleteState`、`CreateScene`、`DeleteScene`、`SetDone`、`CreateCharacter`、`UpdateCharacter`、`DeleteCharacter`、`MoveCharacter` を mutate とみなす。

### 権限管理

`room_permissions(room_id, account_id, role)` に `gm` / `pl` / `viewer` を保存する。`RoomRole` は serde で snake_case。WebSocket 接続時の入場可否と viewer の mutate 遮断に使う。`SetDone` は bridge の in-memory `done` と `RoomBridgeManager` の room 状態にも反映する。

### ZIP export

1. `type` が `full` / `logs` 以外なら 400。
2. account と session が片方だけなら 401。両方あれば session を検証する。
3. bridge に `ExportRoom` を送り、banquette から `RoomData` と chunk を受け取る。
4. `logs` の場合は各 scene の state を空にする。
5. `alhalte_kitchen::export_room_to_zip` で ZIP を作り、`application/zip` と `Content-Disposition` を付けて返す。

## データベース

SQLite のテーブル:

```sql
accounts(account_id INTEGER PRIMARY KEY, encrypted_sub TEXT, handle_name TEXT,
         icon_tin_id BLOB, created_at INTEGER)
sessions(session_id INTEGER PRIMARY KEY, account_id INTEGER, last_heartbeat INTEGER,
         created_at INTEGER)
room_permissions(room_id TEXT, account_id INTEGER, role TEXT, created_at INTEGER,
                 PRIMARY KEY (room_id, account_id))
```

`encrypted_sub` には索引がある。session は account に外部キーで紐づき、account 削除時に cascade する。`room_id` は文字列。

## 既知の差分・制約

- main 時点では権限の正が `maitre-d` の SQLite にあり、`banquette` は `done` と `std::admins` で判定する。Issue #114 では `maitre-d` が `banquette` に問い合わせる構成へ移行中。
- `gateway.rs` は `AccountPermissionResponse` の受信処理が未接続で、`pending_perm_queries` を登録しても応答が届かない経路がある。
- `RoomBridgeManager` の `room_done_status` は in-memory で、再起動すると失われる。永続化は `banquette` の room state 側に委ねる。
- debug account は `encrypted_sub = "0"` で作られ、`decrypt_sub` も `"0"` を特別扱いする。debug ビルド限定。
- CORS は permissive。本番運用では reverse proxy 側で制限する前提。

