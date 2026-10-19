# alhalte-cellar

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-cellar`  
> テスト: `crates/alhalte-cellar/tests/cellar_tests.rs`

## 責務

tin の実体をファイルとして保存し、`(tin_id, path, owner, wrapped_key)` の参照行を SQLite に持つ、重複排除ストレージ。所有者の session を検証し、既に実体がある場合は再送させず参照だけを追加する。信頼された内部サービス（`alhalte-banquette`）は `X-Alhalte-Service-Token` で書き込める。参照されなくなった実体を GC で削除する。

## 公開 API / 外部接続点

### HTTP API

| メソッド | パス | 内容 |
| --- | --- | --- |
| `GET` | `/health` | ヘルスチェック。 |
| `POST` | `/store` | `StoreRequest` を受け取り、210 / 211 / 212 を返す。 |
| `POST` / `PUT` | `/store/{tin_id}` | 本体をアップロードする。 |
| `POST` / `PUT` | `/tin/{tin_id}` | 同上。 |
| `GET` | `/tin/{tin_id}` | 本体を返す。wrapped key があれば `X-Alhalte-Wrapped-Key` に hex で付ける。 |
| `GET` | `/tin/{tin_id}/meta` | `tin_id`、size、wrapped key を JSON で返す。 |
| `GET` | `/download/{tin_id}` | 本体を返す。 |
| `GET` | `/download/{tin_id}/meta` | メタデータを返す。 |
| `POST` | `/unregister` | `UnregisterRequest` で参照を削除する。 |
| `DELETE` | `/store/{tin_id}` / `/tin/{tin_id}` | 参照を削除する。 |
| `POST` | `/gc` | GC を実行し、`GcStats` を返す。 |

アップロード時の認証情報はヘッダ `X-Alhalte-Owner` / `X-Alhalte-Session-Id`、またはクエリ `owner` / `session_id`。wrapped key は `X-Alhalte-Wrapped-Key`（hex）またはクエリ `wrapped_key`。

`/store`、`/store/{tin_id}`、`/tin/{tin_id}`（upload）、`/unregister` はヘッダ `X-Alhalte-Service-Token` でも認可できる。これは `--service-token` で設定した内部トークンとの一致だけを検証し、ユーザー session とは独立している。ヘッダを付けたリクエストは session 検証へフォールバックしない（不一致は 401）。ヘッダのないリクエストは従来どおり `SessionVerifier` で検証する。

`/store/{tin_id}` と `/tin/{tin_id}` の upload は Axum 既定の 2 MiB を超える Tin を受け取れるよう、`DefaultBodyLimit::max(64 MiB)`（`MAX_STORE_BODY_BYTES`）を route 単位で設定する。他の route は既定のまま。

### ステータス

| コード | 意味 |
| --- | --- |
| 210 | Upload Required。本体を送る必要がある。 |
| 211 | Deduplicated。実体は既にあり、参照行を追加した。 |
| 212 | Already Owned。同じ owner が既に参照を持つ。 |

anti-oracle が有効な場合、`/store` は常に 210 を返し、既存実体への再送は `/dev/null` へ捨てる。

### 主な公開型

- `CellarConfig`、`CellarService`
- `StoreRequest`、`StoreResponse`、`StoreResult`、`StoreStatus`、`UnregisterRequest`、`TinReference`、`GcStats`
- `Database`、`StorageManager`、`SessionVerifier`、`InMemorySessionVerifier`、`MaitreDVerifier`
- `create_router`、`run_garbage_collection`、`parse_tin_id` / `format_tin_id`
- `CellarService::with_service_token` / `has_service_token` と `store_check_with_token` / `store_payload_with_token` / `unregister_with_token`、`handlers::SERVICE_TOKEN_HEADER`

CLI（`CliArgs`）: `--host`、`--port`、`--storage-path`、`--db-path`、`--anti-oracle`、`--maitre-d-url`、`--maitre-d-socket`、`--allow-all-sessions`、`--service-token`（`ALHALTE_CELLAR_SERVICE_TOKEN`）、`--gc-interval-secs`。

### 外部接続

`MaitreDVerifier` は `alhalte-maitre-d` の `GET /api/verify-session?owner=...&session_id=...` を HTTP または UNIX ソケットで呼ぶ。テストでは `InMemorySessionVerifier` を使う。

## データ形式

### DB

```sql
tin_references(
  tin_id BLOB NOT NULL,
  path TEXT NOT NULL,
  owner INTEGER NOT NULL,
  wrapped_key BLOB,
  PRIMARY KEY (tin_id, owner)
)
```

`tin_id` は 32 バイト BLOB。`path` は実体ファイルの相対名で、現在は tin id の hex。索引は `path` と `tin_id`。

### ファイル

`StorageManager` は `storage_path/<tin_id hex>` に実体を置く。書き込みは一時ファイル + `sync_all` + rename で原子的に行う。`.tmp.*` は GC の走査対象外。

## 内部フロー

### store check

1. `verify_session(owner, session_id)`。`X-Alhalte-Service-Token` がある場合は代わりにトークンを検証する。
2. anti-oracle なら常に 210。
3. 同じ owner の参照があれば 212。
4. 実体ファイルがあれば、参照行を追加して 211。
5. どちらもなければ 210。

### store payload

1. 認可（session または service token）を再検証する。
2. anti-oracle かつ実体が既にある場合はデータを捨てる。
3. 実体がなければ原子的に書き込む。
4. `(tin_id, path, owner, wrapped_key)` を upsert する。
5. `StoreResult` を返す。

### 内部サービス認可

`authorize` は service token が提示された場合、設定済みトークンと定数時間比較し、一致しなければ 401 を返す。トークン未設定の cellar にトークンが提示された場合も 401（設定情報を漏らさない）。トークンが提示されない場合は従来の session 検証だけを行う。トークンはログに出さず、`CellarService` の `Debug` にも含めない（`CellarService` は `Debug` を実装しない）。

### download

`owner` が指定された場合はその owner の参照を、指定されなければ任意の参照を引く。参照がなければ 404。実体を読んで返す。

### unregister / GC

`unregister` は認可（session または service token）を検証して `(tin_id, owner)` の参照を削除する。GC は storage 配下の tin ファイルを走査し、`tin_references` に 1 行も参照がないファイルを削除する。`gc_interval` があればバックグラウンドで定期実行する。banquette が部屋の新 head を保存したあと旧 head を `POST /unregister` するため、旧 `RoomData` は次の GC で回収される。

## 既知の差分・制約

- ダウンロードは session 検証を行わない。`owner` は参照の絞り込みにだけ使う。
- `/store` の 210 / 211 / 212 は HTTP ステータスそのものとして返る。通常の 2xx ではないため、クライアントは数値で分岐する必要がある。
- `path` は現状 tin id の hex で、ディレクトリ階層は持たない。
- anti-oracle 時の `/dev/null` は `StorageManager::discard_to_dev_null` による no-op で、実際のファイルディスクリプタは開かない。
- GC は DB の参照だけを見る。tin 間の参照関係（chunk 内の tin id など）は追跡しない。
- upload は `Bytes` extractor でリクエスト全量をメモリにバッファする（上限 64 MiB）。maitre-d proxy も同じ上限・同じバッファリングを行う。
- service token は平文で設定・比較する。ローテーション機構はなく、変更するには `cellar` / `maitre-d` / 全 banquette の再起動と設定更新が必要。
- service token は `--service-token` 引数でも渡せるため `ps` から読める。本番では環境変数か systemd の `EnvironmentFile` を使うのが望ましい。
- service token による書き込みでも `owner` は必要で、参照行の所有権はその owner に紐づく。banquette は部屋の管理者 account を owner として使う。
