# alhalte-cellar

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-cellar`  
> テスト: `crates/alhalte-cellar/tests/spec_tests.rs`

## 責務

`Tin` 実体の重複排除（deduplication）ストレージ、所有者ごとの参照管理、汎用エイリアス（部屋の永続化 head ポインタ等）の管理、および未参照実体のガベージコレクション（GC）を担う内部 HTTP サービスである。`cellar` 自身は `Tin` の復号や内部の `TinContent`（ラベル・ペイロード）の解釈を行わず、ワイヤー表現のバイト列とメタデータのみを管理する。

## 公開 API / 外部接続点

### CLI（`alhalte-cellar` バイナリ）

| 引数 / 環境変数 | 既定値 | 内容 |
| --- | --- | --- |
| `--host` / `ALHALTE_CELLAR_HOST` | `127.0.0.1` | 待ち受けホスト（内部サービスのためループバック固定を推奨）。 |
| `--port` / `ALHALTE_CELLAR_PORT` | `8081` | 待ち受けポート。 |
| `--data-dir` / `ALHALTE_CELLAR_DATA_DIR` | `./cellar-data` | SQLite DB（`cellar.db`）および実体ファイル（`entities/`）の保存先ディレクトリ。 |
| `--anti-oracle` / `ALHALTE_CELLAR_ANTI_ORACLE` | `false` | `true` の場合、他アカウントのみが所有する既存 `Tin` に対しても `POST /store` で常に `upload_required` を返し、クロスアカウントの存在確認（情報漏洩オラクル）を防ぐ。 |
| `--max-tin-size` / `ALHALTE_MAX_TIN_SIZE` | `67108864` (64 MiB) | 保存を許可する 1 つの `Tin` の最大サイズ（バイト）。`POST /store` の事前検査および `PUT /store/{tin_id}` のボディ上限に適用する（Issue #299）。 |
| `--gc-interval-secs` / `ALHALTE_CELLAR_GC_INTERVAL_SECS` | `21600` (6 時間) | 定期ガベージコレクションの実行間隔（秒）。 |
| `--service-token-file` / `ALHALTE_CELLAR_SERVICE_TOKEN_FILE` | 任意 | 内部サービス（`maitre-d` / `banquette`）からの書き込みを認可する共有トークンを格納したファイルのパス。 |
| `--service-token` / `ALHALTE_CELLAR_SERVICE_TOKEN` | 任意 | 共有トークンの直接指定（後方互換用。`--service-token-file` を優先）。 |

### HTTP エンドポイント

| メソッド・パス | 認可 | 内容 |
| --- | --- | --- |
| `POST /store` | ユーザーセッション または `X-Alhalte-Service-Token` | 保存前のメタデータハンドシェイク。JSON `{ tin_id, owner, size, wrapped_key, session_id? }` を受け取り、`200 OK` で `{ "result": "upload_required" \| "deduplicated" \| "already_owned" }` を返す。`size > max_tin_size` の場合は `413 Payload Too Large` で拒否する。 |
| `PUT /store/{tin_id}` | ユーザーセッション または `X-Alhalte-Service-Token` | `Tin` ワイヤーバイト列のアップロード。ヘッダ `X-Alhalte-Owner`（必須）と `X-Alhalte-Wrapped-Key`（任意、hex）を受け取る。上限 `max_tin_size`（既定 64 MiB）を超えるボディは `413 Payload Too Large` で拒否する。 |
| `GET /tin/{tin_id}` | 不要 | `Tin` ワイヤーバイト列を取得する（`application/octet-stream`）。対象参照に `wrapped_key` が記録されている場合は `X-Alhalte-Wrapped-Key` ヘッダ（hex）を付与する。`?owner=` または `X-Alhalte-Owner` 指定時はその所有者の参照行を優先する。 |
| `DELETE /tin/{tin_id}` | ユーザーセッション または `X-Alhalte-Service-Token` | `?owner=` または `X-Alhalte-Owner` で指定された所有者の `(tin_id, owner)` 参照行を削除する。実体ファイルの削除は後続の GC に委ねる。 |
| `GET /alias/{*name}` | 不要 | 指定名のエイリアス（例: `room/<room_id>/head`）を解決し、`{ name, tin_id, updated_at }` を返す（未登録は 404）。 |
| `PUT /alias/{*name}` | `X-Alhalte-Service-Token` 必須 | `{ tin_id }`（64 桁 hex）を受け取り、エイリアスを原子的に更新する。対象 `tin_id` の実体ファイルが `entities/` に存在しない場合は `404 Not Found` で拒否する。 |
| `DELETE /alias/{*name}` | `X-Alhalte-Service-Token` 必須 | 指定エイリアスを削除する（存在すれば `204 No Content`、無ければ `404 Not Found`）。 |

### 認証・認可モデル

- **内部書き込み (`X-Alhalte-Service-Token`)**:
  - `banquette` による部屋状態・ログチャンクの永続化やエイリアス更新（`PUT / DELETE /alias/{*name}`）は、`X-Alhalte-Service-Token` ヘッダを用いて認証する。
  - `cellar` は設定されたサービストークンと定数時間比較（constant-time comparison）で照合する。トークン未設定時または不一致時は内部専用操作を `401 Unauthorized` で拒否する。
- **ユーザー経由の書き込み (`SessionValidator`)**:
  - ブラウザからのアップロードは `maitre-d` の `/api/cellar/*` リバースプロキシを経由する。`maitre-d` が Cookie セッションを検証した上で確定した `owner` と `session_id` を注入して `cellar` に転送する。

## データ形式

### `Tin` コンテナと暗号方式ごとの扱い（Issue #272, #298）

`tin` クレート（`crates/tin-rs`）において、平文の `(label, payload)` を表す `TinContent` と、ワイヤー上の `Opener`（暗号・圧縮指定）および `TinId` を統合した `Tin` が定義される（Issue #272）。`cellar` はシリアライズされた `Tin` のワイヤーバイト列をそのまま保存し、先頭 1 バイトの `Opener` に応じて整合性検証と `wrapped_key` の管理を行う。

| 暗号方式 (`Opener.encryption`) | `TinId` の導出規則 | `cellar` での検証と `wrapped_key` の扱い |
| --- | --- | --- |
| `Plaintext` (`0000`) | ワイヤーバイト列全体の BLAKE3-256 | `PUT /store/{tin_id}` 時に受信バイト列の BLAKE3-256 を計算し、URL の `tin_id` と一致することを検証する。`wrapped_key` は不要（`None`）。 |
| `RoomKey` (`0010`) | ワイヤーバイト列全体の BLAKE3-256 | 部屋鍵と 12 バイトのランダム Nonce で直接暗号化されるため、部屋鍵ごとに異なる `tin_id` を持つ（Issue #298）。`PUT /store/{tin_id}` 時にワイヤー全体の BLAKE3-256 と `tin_id` の一致を検証する。`wrapped_key` は不要（`None`）。 |
| `Convergent` (`0001`) | 平文（`TinContent`）の BLAKE3 XOF 512bit の前半 256bit | `cellar` は平文を持たないため `PUT /store/{tin_id}` 時の BLAKE3 再計算をスキップする（クライアントが復号後に検証する）。後半 256bit の収束鍵を部屋鍵でラップした 60 バイト（`12B nonce ‖ 48B ciphertext`）を `(tin_id, owner)` ごとに `wrapped_key` として保存し、`GET /tin/{tin_id}` の `X-Alhalte-Wrapped-Key` ヘッダで返す。 |

### SQLite スキーマ（`<data-dir>/cellar.db`）

WAL モード（`PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;`）で動作する。

- **`tin_references`**:
  ```sql
  CREATE TABLE IF NOT EXISTS tin_references (
      tin_id BLOB NOT NULL,
      owner INTEGER NOT NULL,
      wrapped_key BLOB,
      PRIMARY KEY (tin_id, owner)
  );
  CREATE INDEX IF NOT EXISTS idx_tin_references_tin_id ON tin_references(tin_id);
  ```
  - `tin_id`: 32 バイトの `TinId`。
  - `owner`: 所有者アカウント ID（`u64` を `i64` ビットパターンで格納。`banquette` によるシステム所有は `0`）。
  - `wrapped_key`: 任意のラップ済み収束鍵バイト列。同一 `(tin_id, owner)` への再登録時は `COALESCE(excluded.wrapped_key, tin_references.wrapped_key)` により既存の鍵を保持または更新する。
- **`aliases`**:
  ```sql
  CREATE TABLE IF NOT EXISTS aliases (
      name TEXT PRIMARY KEY,
      tin_id BLOB NOT NULL,
      updated_at INTEGER NOT NULL
  );
  ```
  - `name`: エイリアス名（先頭・末尾の `/` を除いた文字列。空文字や `..` セグメント、制御文字は不可）。
  - `tin_id`: 指し示す 32 バイトの `TinId`。
  - `updated_at`: 更新時の UNIX 秒。

### ディスク上の実体配置

- 実体ファイルは `<data-dir>/entities/{hex(tin_id)}`（64 桁小文字 hex）に配置する。ファイル名は常に `tin_id` から一意に定まるため、DB にはファイルパスを保存しない。
- 書き込み時は `<data-dir>/entities/.tmp-{hex(tin_id)}-{random}` に書き出してから `rename` することで、クラッシュ時や並行読み取り時に不完全なファイルが見えることを防ぐ。

## 内部フロー

### 2 段階保存フロー（`POST /store` → `PUT /store/{tin_id}`）

1. クライアントまたは `banquette` が `POST /store` に `{ tin_id, owner, size, wrapped_key }` を送信する。
2. `size > max_tin_size`（既定 64 MiB）の場合は直ちに `413 Payload Too Large` を返す（Issue #299）。
3. `tin_references` と `entities/{hex(tin_id)}` の有無を確認する:
   - すでに同じ `(tin_id, owner)` の参照行が存在し、実体ファイルも存在する場合は `already_owned` を返す（`wrapped_key` が指定されていれば更新する）。
   - `--anti-oracle` が `false` で、別オーナーの参照行および実体ファイルが既に存在する場合は、新しい `(tin_id, owner, wrapped_key)` 参照行を DB に追加した上で `deduplicated` を返す（本体アップロード不要）。
   - 実体ファイルが存在しない場合（または `--anti-oracle` が `true` で呼び出し元が未所有の場合）は `upload_required` を返す。
4. `upload_required` を受け取った呼び出し元は、`PUT /store/{tin_id}` で `Tin` のワイヤーバイト列を送信する。
5. `cellar` はボディサイズが `max_tin_size` 以下であることを確認し、`Opener` を検査して `Plaintext` / `RoomKey` なら `BLAKE3(body) == tin_id` を検証する。検証通過後、実体ファイルを原子的に書き出し、`tin_references` に `(tin_id, owner, wrapped_key)` を登録する。

### ガベージコレクション（GC）

1. `gc_interval` ごと（または `CellarService::run_gc` 呼び出し時）に実行される。
2. `tin_references` テーブル内の全 `tin_id` 集合と、`aliases` テーブルが指し示す全 `tin_id` 集合の和集合（参照集合 $R$）を取得する。**エイリアスも有効な参照として扱う**ため、`room/<room_id>/head` が指している最新の部屋状態 `Tin` が誤って回収されることはない。
3. `<data-dir>/entities/` 内のファイルを走査し、一時ファイル（`.` 始まり）を除いて $R$ に含まれない `{hex(tin_id)}` ファイルを削除する。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- `tin_references` と `aliases` のいずれからも参照されていない実体ファイルのみが GC で削除される。
- `PUT /alias/{*name}` は実体ファイルが存在しない `tin_id` へのポインタ設定を `EntityNotFound`（404）で拒否し、ダングリングエイリアスの作成を防ぐ。
- `max_tin_size`（既定 64 MiB）を超える `Tin` はハンドシェイク段階と実体アップロード段階の双方で `PayloadTooLarge`（413）として拒否する。

## 既知の差分・制約

- `PUT /store/{tin_id}` および `GET /tin/{tin_id}` は `Tin` 全体をメモリ上のバイト列（最大 `--max-tin-size`、既定 64 MiB）として送受信する。
- `GET /tin/{tin_id}` は `TinId`（256bit 暗号論的ハッシュ）を知っている呼び出し元に対して認証なしで暗号化済みバイト列を返す。機密性は `Tin` 自体の暗号化（部屋鍵暗号化または収束暗号化）によって保証される。
