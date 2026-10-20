# alhalte-banquette

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-banquette`  
> テスト: `crates/alhalte-banquette/tests/spec_tests.rs`

## 責務

部屋ごとに 1 プロセスで動くルームデーモン。何を交換するかではなく、どう交換するかを担う。scene のログ・state、character の所有権、クライアントの入退室、WebRTC signaling の中継、cellar への永続化と**部屋全体の復元**を持つ。HTTP サーバーは持たず、`alhalte-maitre-d` と UNIX ソケットで話す。

## 公開 API / 外部接続点

### 外部接続点

| 接続先 | 方向 | 内容 |
| --- | --- | --- |
| `alhalte-maitre-d` | 双方向 | UNIX ドメインソケット。4 バイト big-endian 長 + postcard のフレーム。部屋 head はここでは報告しない（Issue #192）。 |
| `alhalte-cellar` | 送信・取得 | `POST /store`、`POST /store/{tin_id}`、`GET /tin/{tin_id}`、`POST /unregister`、`GET` / `PUT /alias/{name}`。封入した chunk、scene、room の永続化、head alias の読み書き、旧 head の解放。内部書き込みは `X-Alhalte-Service-Token` で認可する。 |

CLI（`CliArgs`）:

- `--room-id` / `ALHALTE_ROOM_ID`: 16 hex（正）または移行用 decimal の `u64`。内部の `RoomData.room_id` は `u64` のまま。
- `--admin`（別名 `--owner`）/ `ALHALTE_ADMIN_ACCOUNT`
- `--socket` / `ALHALTE_SOCKET_PATH`（既定 `/tmp/alhalte-banquette.sock`）
- `--cellar-url` / `ALHALTE_CELLAR_URL`（既定 `http://127.0.0.1:8080`）。起動時の head 読み込みもここへ行う。
- `--chunk-size-exponent` / `ALHALTE_CHUNK_SIZE_EXPONENT`（既定 8）
- `--heartbeat-interval-secs` / `ALHALTE_HEARTBEAT_INTERVAL_SECS`（既定 240）
- `--service-token` / `ALHALTE_SERVICE_TOKEN`: 内部書き込みと alias 更新用トークン（後方互換。argv に出るので非推奨）。値はログに出さない。
- `--service-token-file` / `ALHALTE_SERVICE_TOKEN_FILE`: 内部書き込み用トークンを 0600 file から読む。maitre-d が Bistrot から受け取った path をそのまま転送する。
- 復元用の引数は無い。head は cellar の `room/<room_id>/head` alias から読む（Issue #192）。旧 `--room-tin` / `ALHALTE_ROOM_TIN` は廃止し、指定すると clap が unknown argument で起動を拒否する。
- 秘密 file は `alhalte_banquette::secret_file::read_secret_file` が読み、group/other に読める file は拒否する（Issue #194）。

### 主な公開型

| API | 内容 |
| --- | --- |
| `Room` / `RoomData` / `RoomPermissionLevel` | 部屋全体。scene biblio、character biblio、権限（`default_permission` / `permissions` / `accessible_scenes`）、chunk exponent。`room_id` は `u64`。 |
| `Room::persist` / `Room::load_or_create` / `Room::load_from_cellar` / `Room::from_tin_with_head` / `Room::validate` | 部屋全体の保存・復元・不変条件検証。`persist` は store 後に head alias を移し、`load_or_create` は起動時に alias を読んで復元する（未設定なら新規部屋）。復元は room_id、root scene、権限表を検証する。 |
| `room_head_alias(room_id)` | 部屋 head を置く cellar alias 名（`room/<room_id 16 hex>/head`）。 |
| `HeartbeatOutcome` / `HeartbeatResult` | heartbeat の継続判定と、保存に成功した `RoomData` tin id。 |
| `Scene` / `SceneData` | 独立した log と state を持つ scene。 |
| `SceneLog` / `LogBiblio` / `LogChunk` | chunk 化された log と head。 |
| `SceneState` | `BTreeMap<String, Tin>`。4KiB 制限と `::` 名前空間検証。 |
| `Character` / `CharacterBiblio` | `HashMap<String, Tin>` と所有権。 |
| `RoomArchive` / `export_room_to_zip` | export ZIP（`room.tin` + `chunks/<tin-id>.tin`）の組み立てと読み戻し。`include_state=false` で scene state だけを落とす（#193）。 |
| `ClientFrame` / `ServerFrame` | クライアントと banquette の論理フレーム。 |
| `MaitreDToBanquette` / `BanquetteToMaitreD` | maitre-d と banquette の制御フレーム。`QueryAccountPermission` / `AccountPermissionResponse` で account の実効権限を返す。head を報告する frame は持たない（Issue #192）。 |
| `ProtocolCodec<T>` | 長さ前置 postcard コーデック。 |
| `BanquetteServer` / `ServerConfig` | UNIX ソケットサーバー。`run_until_shutdown` で明示的に停止できる。 |
| `CellarSink` / `HttpCellarSink` / `InMemoryCellarSink` | cellar 永続化の抽象。`store_tin` / `download_tin` / `unregister_tin` / `get_alias` / `set_alias` を持つ。 |
| `INTERNAL_WRITE_SESSION_ID` | 内部書き込みが送る session id（`0`）。合成 session は使わない。 |

### プロトコル

`MaitreDToBanquette` は `ClientConnected`、`ClientDisconnected`、`ClientMessage`、`Ping`、`ExportRoom { request_id, caller_account, include_state }`、`QueryAccountPermission { request_id, account_id }` を持つ。

`BanquetteToMaitreD` は `SendToClient`、`BroadcastToScene`、`BroadcastToRoom`、`Pong`、`RoomTerminated`、`ExportRoomResponse { request_id, result: Result<Vec<u8>, String> }`、`AccountPermissionResponse` を持つ。head を報告する `RoomPersisted` は Issue #192 で削除した（head は cellar の alias が持つ）。`ExportRoomResponse` は ZIP bytes を運び、組み立ては banquette が行う（#193）。

`QueryAccountPermission` は `maitre-d` が「この部屋でのこの account の実効権限」を尋ねる制御フレームで、`Room::get_account_permission` の結果を `AccountPermissionResponse { request_id, account_id, permission }` で返す。DB の role と部屋の権限表を突き合わせるのは `maitre-d` 側の責務（#200）で、banquette は自分の権限表だけを答える。旧 `SetAdmins` / `AdminsUpdated`（DB の GM 集合を実行中の部屋へ push する経路）は #199 で撤去した。

権限変更はクライアント由来の `SetMemberPermission` / `SetDefaultPermission` だけで行う。成功すると `ServerFrame::PermissionsUpdated { default_permission, permissions }` を要求元へ返し、失敗時は `SetMemberPermissionFailed` / `SetDefaultPermissionFailed` の `Error` のみで、部屋の状態は変わらない。最後の Admin の降格は `LastAdminDemotion` で拒否する。`maitre-d` は成功 frame を観測するまで `rooms.done` を更新しない。

`ClientFrame` は scene 一覧・詳細・作成・削除・入退室、state の取得・更新・削除、log の append/edit/delete/rewind/取得、character の CRUD・移動、権限の変更・取得、WebRTC signaling、heartbeat 応答を持つ。`ServerFrame` はその結果と broadcast を返す。

- `SetMemberPermission { account_id, permission }` / `SetDefaultPermission { permission }` / `QueryPermissions` が権限フレーム。変更は Admin のみ、`QueryPermissions` は部屋に入れる account なら誰でもよい。成功時は `PermissionsUpdated`、取得時は `PermissionsSnapshot` を要求元だけに返す（部屋全体へは broadcast しない）。`can_access_scene` を満たさない account への応答は他の query と同じく無言。
- 全 mutate frame は `Room` の権限チェックを通る。`AppendLog` / `EditLog` / `DeleteLog` / `RewindLog` / `UpdateState` / `DeleteState` は対象 scene の `can_send_to_scene`、`CreateScene` / `DeleteScene` / `MoveCharacter` は Admin、character の CRUD は書き込み可能（`Participant` 以上）かつ所有者または Admin。`Viewer` は全 mutate が拒否される。拒否は `*Failed` の `Error` frame になる。
- `ListScenes` / `CreateScene` / `DeleteScene` / `MoveCharacter` の scene 一覧は `SceneSummary`（`id`、`name`、`present_character_count`、`present_characters`、`active_client_count`、`is_loaded`）の列 `SceneList` として返す。`present_characters` は scene に在籍する character id の昇順リストで、クライアントはこれで `scene.members` を再構築する。`name` は scene state `std::name` の UTF-8 文字列で、未設定なら scene id、`/` は作成時に `メインシーン` が入る。`SceneSummary` は protocol 上の summary であり、`RoomData` / `SceneData` の wire schema は変えず、永続化は従来どおり `SceneData.present_characters` が持つ。
- `CreateScene { scene_id, inherit_juices, name }` は `name` を `std::name` へ作成と同時に書き、`SceneCreated` を要求元へ返し、`SceneList` と新 scene の `SceneInfo` を room 全体へ broadcast する。`std::name` は juice 継承の対象外。scene の作成・削除は Admin のみ。
- `UpdateState` で `key == "std::name"` が成功したときは `StateUpdated` の ack / scene broadcast に加えて `SceneInfo` を room 全体へ broadcast する。これにより他クライアントは再読み込みなしで rename を反映できる。
- `EnterScene` は `can_access_scene` を満たす場合だけ入室を記録し、その scene の `StateSnapshot` を直接返す。満たさない場合は無言で何もしない（presence も記録しない）。クライアントは入室ごとの `QueryState` を必要としない（`QueryState` は従来どおり明示問い合わせ用に残る）。
- `CreateScene` / `DeleteScene` の成功時は要求元へ `SceneCreated` / `SceneDeleted` を返し、更新後の scene 一覧を `SceneList` として room 全体へ broadcast する。`MoveCharacter` の成功時は要求元への `CharacterMoved` ack に加えて room 全体へも `CharacterMoved` を broadcast し、移動元・移動先の `SceneInfo` を broadcast する。移動先 scene が暗黙に作られた場合は `SceneList` も broadcast する。ack と broadcast が重複してもクライアント側は冪等に適用する。
- character の CRUD は room 全体へ同期する（#160）。`CreateCharacter` 成功時は要求元へ `CharacterCreated`（採番 id）を返し、room 全体へ `CharacterInfo { character_id, data }` と、新しい character が入った scene の `SceneInfo` を broadcast する。新規 character は要求元が現在入室している scene（不明なら `/`）に所属する。`UpdateCharacter` 成功時は `CharacterUpdated` ack と room 全体への `CharacterInfo`（保存後の属性）を送る。`DeleteCharacter` 成功時は `CharacterDeleted` を room 全体へ broadcast し、全 scene の `present_characters` から除去したうえで `SceneInfo` を、残っている `piece::<id>::placed` / `piece::<id>::pos` を削除して `StateDeleted` を broadcast する。scene membership は `Scene.present_characters` が正で、`character::<id>::scene` mirror は書かない。
- state の `UpdateState` / `DeleteState` 成功時は、要求元への直接 ack と対象 scene への broadcast の両方を行う。ルート scene `/` の state は他 scene から編集される（アセット・拡張機能）ため、要求元が対象 scene に居なくても ack が届く。両方を受け取ったクライアントは同じ frame を 2 回適用するが、state 更新・削除は冪等。

## データ形式

| ラベル | 内容 |
| --- | --- |
| `alhalte/banquette` | 部屋全体の状態（`RoomData`）。 |
| `alhalte/banquette/scene` | scene の state と log biblio（`SceneData`）。 |
| `alhalte/banquette/log/chunk` | `Vec<Option<Tin>>` の log chunk。 |
| `alhalte/banquette/log/biblio` | 封入済み chunk の tin id 列と head `(u32, u32)`。 |
| `alhalte/banquette/log/character-came` | scene への入室イベント。 |
| `alhalte/banquette/log/character-gone` | scene からの退場イベント。 |

予約 state キー:

- scene 名の `std::name`（scene state の UTF-8 文字列。character 属性にも同名キーがある）
- character 属性の `std::name`、`std::owner_account`
- cutlery 予約: `asset::<id>`（`/` のアセットメタデータ）、`extension::<id>`（`/` の拡張機能メタデータ）

state キーは 1 個以上の `::` を含む必要がある。値は `to_plaintext().len()` が 4KiB 以下。管理者専用の state キーは無く、書き込みの可否は部屋の権限表（`can_send_to_scene`）だけで決まる。旧 `std::admins`（`/` に置いていた管理者リスト）は #199 で廃止し、管理者は `RoomData.permissions` が持つ。

## 内部フロー

### 起動

1. `Room::load_or_create(cellar, room_id, admin_account, chunk_exponent)` が cellar の `room/<room_id>/head` alias を読む（`room_head_alias`）。
   - alias が未設定なら、部屋は一度も永続化されていない。`Room::new` が `/` を作り、作成者を `Admin`、`default_permission` を `Denied`、作成者の `accessible_scenes` を `["/"]` にして `RoomData` に持たせる。
   - alias があれば、`Room::load_from_cellar` が cellar から tin を取得して復元する。
     - `Tin::decode` で復号し、tin id（内容ハッシュ）が alias の指す head と一致することを確認する。
     - `Room::from_tin` が `RoomData` を復元し、root scene `/` の存在と `Room::validate`（Admin が 1 人以上、`accessible_scenes` の整合）を検証する。
     - `RoomData.room_id` が `--room-id` と一致することを検証する。
     - alias の読み込み・取得・復号・検証のいずれかが失敗したら**空部屋で起動せず**、明確なエラーで終了する。旧 schema（`done` フラグを持つ #199 以前の `RoomData`）は decode 失敗として扱い、変換しない。
2. `BanquetteServer::run` が既存ソケットを削除し、UNIX ソケットを bind する。
3. heartbeat タスクを起動する。初回 tick は捨てる。
4. maitre-d からの接続ごとに `handle_connection` を spawn する。

### 部屋状態の永続化と復元

- heartbeat ごとに `Room::persist` が部屋全体を `alhalte/banquette` Tin（Zstd）として cellar に保存する。内部書き込みは `INTERNAL_WRITE_SESSION_ID = 0` と `X-Alhalte-Service-Token` を使い、合成 session は使わない。
- 保存に成功したら、まず `PUT /alias/room/<room_id>/head` で head alias を新しい tin へ移す。alias が唯一の復元点なので、この書き込みの失敗は `persist` の失敗として返す（旧 alias と旧 head は有効なまま残り、次周期に再試行する）。
- alias の更新後に、直前の head tin を `CellarSink::unregister_tin` で best-effort に解放する。現在の head は alias が指しているので解放しない。解放失敗は警告ログのみで、保存自体は成功扱い。
- 保存に成功した tin id は `HeartbeatResult::persisted_tin` に入る。保存失敗時は `None` で、ログに残して次周期に再試行する。maitre-d へ報告する frame は無い。
- graceful shutdown（idle による `ShouldExit`、`run_until_shutdown`、または standalone バイナリの SIGTERM / SIGINT）では終了直前に `persist` をもう一度行い、alias を最終状態へ移してからソケットを閉じる。SIGTERM / SIGINT は `main.rs` で `watch` channel に配線し、`run_until_shutdown` へ渡す。
- `room_id → head tin` のポインタは cellar の alias だけが持つ。maitre-d も banquette も DB / 専用ファイルに head を保存しない。

### クライアント接続

1. `ClientConnected` で `ConnectedClient` を登録し、クライアントを `/` に入室させる。
2. `ClientMessage` を `ClientFrame` として処理する。`EnterScene` は `can_access_scene` を満たす場合だけ入室を記録して `StateSnapshot` を返し、クライアントは scene の state を即座に復元できる。満たさない場合は無言で何もしない。
3. `ClientDisconnected` で scene から退室させ、必要なら scene を永続化してメモリを解放する。
4. `Ping` には `Pong` を返す。

### log

`SceneLog` は `1 << N` 件の chunk を持つ。`head = (chunk_index, position)`。

- `append`: head 位置へ `Some(Tin)` を書き、head を進める。chunk が満杯なら `LogChunk` Tin に封入し、Zstd で cellar へ保存して biblio に tin id を追加する。
- `append` の成功は `LogOperationEvent::Appended { position, entry }` を返し、`LogEventAppended` には append 後の head ではなく **実際に書き込んだ位置** を載せる。以前は append 後の head を送っていたため、クライアントの position が 1 進んでいた。
- `edit`: head より前の位置を `Some(Tin)` で上書きする。封入済み chunk の場合は再 Tin 化して cellar へ再送する。
- `delete`: head より前の位置を `None`（tombstone）で上書きする。
- `rewind`: head を過去へ戻し、以降を切り捨てる。
- `read_chunk`: active chunk、メモリ上の sealed cache、cellar の順に読む。

各操作は `LogOperationEvent` になり、同一 scene のクライアントへ `LogEventAppended` / `Edited` / `Deleted` / `Rewound` として配信される。

### state

`SceneState` はキー検証と値サイズ検証を行う。管理者専用の state キーは持たず、書き込みの可否は `Room` の権限チェック（`can_send_to_scene`）で決まる。更新・削除は同一 scene のクライアントへ `StateUpdated` / `StateDeleted` として配信される。WebRTC DataChannel が確立している場合の state 中継は signaling の枠組みだけを持ち、実データ経路はクライアント側の実装に委ねる。

### scene のライフサイクル

- scene からクライアントがいなくなったら、`/` 以外は `persist_and_unload` で `alhalte/banquette/scene` Tin として cellar に保存し、active chunk と sealed cache を解放する。
- 再度アクセスされたら `reload_if_needed` で cellar から復元する。
- 開かれていない scene への membership / log 書き込み（`MoveCharacter` など）は、`Room::ensure_scene_loaded` で対象 scene を先に復元してから行う。未ロードのまま書き込むと次の `reload_if_needed` が tin で上書きするため、書き込みは失われる（#240）。
- heartbeat では、前回の周期以降にクライアント活動がなければ `missed_heartbeats` を増やす。3 回連続で応答がなければ `HeartbeatOutcome::ShouldExit` を返し、サーバーは終了する。

### character

`CharacterBiblio` は `next_id` を単調増加させて ID を払い出す。作成時に `std::owner_account` がなければ作成者を設定する。CRUD は `Participant` 以上かつ所有者または `Admin` のみ。`MoveCharacter` は **Admin のみ**が実行でき（`Participant` は自分の character でも動かせない）、移動元 scene に `character-gone`、移動先に `character-came` の log を追記し、移動先 scene を owner の `accessible_scenes` に追加する。これが participant に scene を開く唯一の経路である。scene membership は各 `Scene.present_characters` が保持し、`RoomData` の `SceneData.present_characters` として永続化されるため、reload / reconnect 後も復元される。`character::<id>::scene` は write-only の mirror だったため廃止し、`SceneInfo` / `SceneList` の `present_characters` を正とする。

### export

`ExportRoom { caller_account, include_state }` を受けると、呼び出し元がアクセスできる scene の `RoomData` と chunk を集め、`src/export.rs` の `export_room_to_zip` で ZIP を組み立てて `ExportRoomResponse { result: Result<Vec<u8>, String> }` として返す（#193）。scene の絞り込みは `Room::can_access_scene` に従う（`Viewer` / `Admin` は全 scene、`Participant` は `/` と付与された scene、`Denied` は無し）。`caller_account` が無い export は `default_permission` に従い、`RoomData` の構造を保つため `/` は常に含める。active chunk も export 用に封入する。

ZIP の構成は `room.tin`（`RoomData` Tin）と `chunks/<tin-id>.tin`（`LogChunk` Tin）で、各 entry は plaintext（Zstd 圧縮）の Tin。banquette は room key を持たないため room-key 暗号化はしない。`include_state == false`（`type=logs`）は各 scene の `states` だけを空にし、scene の同一性・biblio・present characters・character 表・権限表は残す。`RoomArchive::load_from_slice` が同じ ZIP を `RoomData` と chunk に読み戻す。

## 権限モデル

`RoomPermissionLevel { Denied, Viewer, Participant, Admin }` が部屋の全 scene の read / write を決める。権限の正は `RoomData` で、`permissions`（account ごとの上書き、`Denied` の明示も可）と `default_permission`（未登録 account の既定）を持ち、`Room` も同じフィールドを持つ。

- `Denied`: 読み書き不可。読み取り query には**無言**で応答しない（接続時の入場可否は現状 `maitre-d` のゲートが担う）。
- `Viewer`: 全 scene の log / state を受信できる。全 mutate を拒否される。
- `Participant`: 管理者に移動させられた scene と root `/` の log / state を送受信できる。`CreateScene` / `DeleteScene` / `MoveCharacter` は不可。
- `Admin`: 全 scene の送受信と権限変更。

### `Room::new`

作成者を `Admin`、`default_permission` を `Denied`、作成者の `accessible_scenes` を `["/"]` にする（部屋は既定で閉じている）。

### `Room::validate`

load 後と権限変更後に呼び、次を検証する。違反は `InvalidRoomData` で、起動時は空部屋にフォールバックしない。

- Admin が最低 1 人いる（`default_permission == Admin` でも満たす）。`room_id` の一致は `load_from_cellar` が別途検証する。
- 最後の Admin の降格は `set_member_permission` / `set_default_permission` が `LastAdminDemotion` で拒否する。
- `accessible_scenes` は実在する scene のみを指し、`can_write()`（`Participant` / `Admin`）の account にだけ付く。`Admin` の項目は不活性（`can_access_scene` は Admin を全許可）。scene 削除や降格で無効になった項目は `normalize_accessible_scenes` が落とす。

### 認可関数

- `is_admin(account)`: 実効権限が `Admin`。
- `get_account_permission(account)`: `permissions` の上書き、無ければ `default_permission`。
- `can_access_scene(account, scene)`: read 可否。`Denied` は常に false、`Viewer` / `Admin` は常に true、`Participant` は `/` と付与 scene のみ。
- `can_send_to_scene(account, scene)`: write 可否。`can_access_scene` と同じ scene 集合だが `Viewer` は常に false。

### 権限変更

`SetMemberPermission` / `SetDefaultPermission` は Admin のみ。成功すると `PermissionsUpdated` を要求元へ返し、`RoomData` に反映して次回 persist で永続化する。`maitre-d` の DB role は実行中の部屋へ push しない（`SetAdmins` は撤去）。DB role と部屋権限表の突き合わせは `QueryAccountPermission` を使う #200 の仕事で、それまでの `rooms.done` は `default_permission` の射影にすぎない。

## 既知の差分・制約

- Issue #199 で `done` / `SetDone` / `std::admins` を撤去し、`RoomData` の wire schema を切った。旧 schema の部屋は decode 失敗として起動しない（変換しない）。`RoomData` の JSON / postcard 互換性は保証しない。
- `maitre-d` の DB role（`room_permissions`）と部屋の権限表はまだ接続していない。`SetAdmins` を撤去したため、実行中の部屋の権限は `SetMemberPermission` / `SetDefaultPermission` でしか変わらず、DB の role 変更は次回 spawn でも反映されない。接続は #200。
- その帰結として、`maitre-d` の DB で `pl` / `viewer` の account も banquette からは `Denied`（`default_permission`）に見える。`Viewer` の mutate 遮断は当面 `maitre-d` の WS ゲートが担い、banquette 側の拒否と二重になる。#200 で DB role を部屋権限表へ移すと解消する。
- `maitre-d` の pre-spawn gate（`rooms.done`）は #200 で部屋権限表に置き換えた。banquette は `QueryAccountPermission` で実効権限を答えるだけで、head や `default_permission` を報告しない（head 報告の `RoomPersisted` は #192 で削除）。
- `alhalte-kitchen` は main から削除し、`kitchen-freeze` ブランチに保存した（#184）。banquette の export は `RoomData` をそのまま ZIP に格納し、`export.rs` の `RoomArchive` で権限表ごと読み戻す。`done` 射影は持たない（#193）。
- `SetMemberPermission` / `SetDefaultPermission` の ack は要求元にだけ返し、部屋全体へは broadcast しない。降格された account は次回 `QueryPermissions` まで自分の権限変化を知らない（UI 追従は #201）。
- `EnterScene` は権限が無い場合に無言で何もしない。`maitre-d` は自前の `client_scenes` で broadcast 先を決めるため、接続時の入場可否は当面 `maitre-d` のゲートが正で、banquette 側の判定は defense in depth。#200 で一本化する。
- log の実体は `HashMap<String, Tin>` を duck typing する運用で、`content` / `speaker` / `timestamp` のキー規約は cutlery と saignant に委ねる。
- WebRTC の DataChannel は signaling の中継までを実装し、state の DataChannel 経路そのものは未実装。
- `RoomData` は `HashMap` を含むため、内容が同じでも postcard の map 順序が変わり tin id が変わることがある。heartbeat ごとに head が作り直されるが、旧 head の unregister と GC で回収される。
- `persist` の失敗はログのみで、次周期までの再試行はない。メトリクスや専用の再試行キューは持たない。
- 合成 session（旧 `system_session_id = 1`）は廃止した。`--service-token` が未設定の場合、内部書き込みは cellar の session 検証に失敗し、alias 書き込みも service token 必須なので 401 になる（persist は失敗し、次周期に再試行する）。
- graceful shutdown の経路は idle 終了と `run_until_shutdown`（および standalone バイナリの SIGTERM / SIGINT）。SIGTERM / SIGINT は `main.rs` で捕捉し、最終 persist を含む同じ graceful path に入る。シグナル配線そのものはプロセス起動を伴う統合テスト（`tests/spec_tests.rs` の `test_sigterm_runs_graceful_final_persist`）で確認する。
- `Room::load_from_cellar` の download は cellar 側で認証されない（現行 cellar の download は session 検証を行わない）。service token は書き込みだけを守る。
- `Room::load_or_create` は alias を読んでから復元する。alias が壊れた tin を指す・hash が一致しない・`RoomData.room_id` が違う場合は起動失敗で、空部屋にフォールバックしない。壊れた alias は自動修復しない（運用者が cellar 側で直す）。
- `maitre-d` は head を一切持たない（`room_heads` は撤去）。別インスタンスが persist して alias を置き換えた場合、旧 head の解放はそれを行った banquette が担う。

