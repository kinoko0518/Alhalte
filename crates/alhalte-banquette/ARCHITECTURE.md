# alhalte-banquette

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-banquette`  
> テスト: `crates/alhalte-banquette/tests/spec_tests.rs`

## 責務

部屋ごとに 1 プロセスで動くルームデーモン。何を交換するかではなく、どう交換するかを担う。scene のログ・state、character の所有権、クライアントの入退室、WebRTC signaling の中継、cellar への永続化を持つ。HTTP サーバーは持たず、`alhalte-maitre-d` と UNIX ソケットで話す。

## 公開 API / 外部接続点

### 外部接続点

| 接続先 | 方向 | 内容 |
| --- | --- | --- |
| `alhalte-maitre-d` | 双方向 | UNIX ドメインソケット。4 バイト big-endian 長 + postcard のフレーム。 |
| `alhalte-cellar` | 送信・取得 | `POST /store`、`POST /store/{tin_id}`、`GET /tin/{tin_id}`。封入した chunk、scene、room の永続化。 |

CLI（`CliArgs`）:

- `--room-id` / `ALHALTE_ROOM_ID`
- `--admin`（別名 `--owner`）/ `ALHALTE_ADMIN_ACCOUNT`
- `--socket` / `ALHALTE_SOCKET_PATH`（既定 `/tmp/alhalte-banquette.sock`）
- `--cellar-url` / `ALHALTE_CELLAR_URL`（既定 `http://127.0.0.1:8080`）
- `--chunk-size-exponent` / `ALHALTE_CHUNK_SIZE_EXPONENT`（既定 8）
- `--heartbeat-interval-secs` / `ALHALTE_HEARTBEAT_INTERVAL_SECS`（既定 240）

### 主な公開型

| API | 内容 |
| --- | --- |
| `Room` / `RoomData` / `RoomPermissionLevel` | 部屋全体。scene biblio、character biblio、権限、chunk exponent。 |
| `Scene` / `SceneData` | 独立した log と state を持つ scene。 |
| `SceneLog` / `LogBiblio` / `LogChunk` | chunk 化された log と head。 |
| `SceneState` | `BTreeMap<String, Tin>`。4KiB 制限と `::` 名前空間検証。 |
| `Character` / `CharacterBiblio` | `HashMap<String, Tin>` と所有権。 |
| `ClientFrame` / `ServerFrame` | クライアントと banquette の論理フレーム。 |
| `MaitreDToBanquette` / `BanquetteToMaitreD` | maitre-d と banquette の制御フレーム。 |
| `ProtocolCodec<T>` | 長さ前置 postcard コーデック。 |
| `BanquetteServer` / `ServerConfig` | UNIX ソケットサーバー。 |
| `CellarSink` / `HttpCellarSink` / `InMemoryCellarSink` | cellar 永続化の抽象。 |

### プロトコル

`MaitreDToBanquette` は `ClientConnected`、`ClientDisconnected`、`ClientMessage`、`Ping`、`ExportRoom`、`QueryAccountPermission`、`SetAccountPermission`、`SetDefaultPermission` を持つ。

`BanquetteToMaitreD` は `SendToClient`、`BroadcastToScene`、`BroadcastToRoom`、`Pong`、`RoomTerminated`、`ExportRoomResponse`、`AccountPermissionResponse` を持つ。

`ClientFrame` は scene 一覧・詳細・作成・削除・入退室、state の取得・更新・削除、log の append/edit/delete/rewind/取得、character の CRUD・移動、WebRTC signaling、heartbeat 応答を持つ。`ServerFrame` はその結果と broadcast を返す。

## データ形式

| ラベル | 内容 |
| --- | --- |
| `alhalte/banquette` | 部屋全体の状態（`RoomData`）。 |
| `alhalte/banquette/scene` | scene の state と log biblio（`SceneData`）。 |
| `alhalte/banquette/log/chunk` | `Vec<Option<Tin>>` の log chunk。 |
| `alhalte/banquette/log/biblio` | 封入済み chunk の tin id 列と head `(u32, u32)`。 |
| `alhalte/banquette/log/change-bg-img` | 背景画像変更イベント。 |
| `alhalte/banquette/log/change-bgm` | BGM 変更イベント。`slot: u32` と `Option<TinId>`。 |
| `alhalte/banquette/log/character-came` | scene への入室イベント。 |
| `alhalte/banquette/log/character-gone` | scene からの退場イベント。 |

予約 state キー:

- `std::admins`（`/` の state。account-id の postcard リスト）
- `std::bgm`、`std::b-img`
- character 属性の `std::name`、`std::owner_account`

state キーは 1 個以上の `::` を含む必要がある。値は `to_plaintext().len()` が 4KiB 以下。`std::admins` の変更は管理者のみ。

## 内部フロー

### 起動

1. `Room::new(room_id, creator_account, chunk_exponent)` が `/` を作り、作成者を管理者として `std::admins` に書き込む。
2. `BanquetteServer::run` が既存ソケットを削除し、UNIX ソケットを bind する。
3. 4 分間隔の heartbeat タスクを起動する。初回 tick は捨てる。
4. maitre-d からの接続ごとに `handle_connection` を spawn する。

### クライアント接続

1. `ClientConnected` で `ConnectedClient` を登録し、クライアントを `/` に入室させる。
2. `ClientMessage` を `ClientFrame` として処理する。
3. `ClientDisconnected` で scene から退室させ、必要なら scene を永続化してメモリを解放する。
4. `Ping` には `Pong` を返す。

### log

`SceneLog` は `1 << N` 件の chunk を持つ。`head = (chunk_index, position)`。

- `append`: head 位置へ `Some(Tin)` を書き、head を進める。chunk が満杯なら `LogChunk` Tin に封入し、Zstd で cellar へ保存して biblio に tin id を追加する。
- `edit`: head より前の位置を `Some(Tin)` で上書きする。封入済み chunk の場合は再 Tin 化して cellar へ再送する。
- `delete`: head より前の位置を `None`（tombstone）で上書きする。
- `rewind`: head を過去へ戻し、以降を切り捨てる。
- `read_chunk`: active chunk、メモリ上の sealed cache、cellar の順に読む。

各操作は `LogOperationEvent` になり、同一 scene のクライアントへ `LogEventAppended` / `Edited` / `Deleted` / `Rewound` として配信される。

### state

`SceneState` はキー検証、値サイズ検証、`std::admins` の管理者チェックを行う。更新・削除は同一 scene のクライアントへ `StateUpdated` / `StateDeleted` として配信される。WebRTC DataChannel が確立している場合の state 中継は signaling の枠組みだけを持ち、実データ経路はクライアント側の実装に委ねる。

### scene のライフサイクル

- scene からクライアントがいなくなったら、`/` 以外は `persist_and_unload` で `alhalte/banquette/scene` Tin として cellar に保存し、active chunk と sealed cache を解放する。
- 再度アクセスされたら `reload_if_needed` で cellar から復元する。
- heartbeat では、前回の周期以降にクライアント活動がなければ `missed_heartbeats` を増やす。3 回連続で応答がなければ `HeartbeatOutcome::ShouldExit` を返し、サーバーは終了する。

### character

`CharacterBiblio` は `next_id` を単調増加させて ID を払い出す。作成時に `std::owner_account` がなければ作成者を設定する。更新・削除は所有者または管理者のみ。`MoveCharacter` は移動元 scene に `character-gone`、移動先に `character-came` の log を追記する。

### export

`ExportRoom` を受けると、呼び出し元がアクセスできる scene の `RoomData` と chunk を集める。active chunk も export 用に封入し、`ExportRoomResponse` で返す。

## 権限モデル

### main（HEAD）時点

- `Room::done: bool`。true なら全部屋情報が公開。
- 管理者は `/` の `std::admins`。
- `can_access_scene` は `done || admin || scene_id == "/" || その scene に所有キャラクターがいる`。
- scene の作成は呼び出し元を検証していなかった。削除は管理者のみ。`/` の state 変更は管理者のみ。

### Issue #114 の移行中（issue-114 ブランチ）

`RoomPermissionLevel { Denied, Viewer, Participant, Admin }` を導入し、`RoomData` の `done` を `default_permission` と `permissions`、`accessible_scenes` に置き換える作業が進行中。

- `Denied`: 入場拒否。
- `Viewer`: 全 scene の log / state を受信できる。送信は不可。
- `Participant`: 管理者に移動させられた scene の log / state を送受信できる。
- `Admin`: 全 scene の送受信と権限変更。

`SetDone` は後方互換のため `true -> Viewer`、`false -> Denied` として残す。`SetMemberPermission` / `SetDefaultPermission` が追加されている。

## 既知の差分・制約

- Issue #114 のブランチでは変更が未完了で、`cargo check` が `room.rs` のパターンで失敗する状態。main には未反映。
- `maitre-d` 側の `AccountPermissionResponse` の受信処理が未接続で、権限問い合わせが完了しない経路がある。
- `RoomData` の `done` から `default_permission` への変更に伴い、`alhalte-kitchen` の `RoomArchive` がまだ `room_data.done` を参照している。
- `ChangeBgmEvent` は `slot: u32` を主とし、旧 `track_index: Option<u8>` をフォールバックとして受け付ける。
- log の実体は `HashMap<String, Tin>` を duck typing する運用で、`content` / `speaker` / `timestamp` のキー規約は cutlery と saignant に委ねる。
- WebRTC の DataChannel は signaling の中継までを実装し、state の DataChannel 経路そのものは未実装。

