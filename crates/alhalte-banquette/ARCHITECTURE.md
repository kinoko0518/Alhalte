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
| `alhalte-maitre-d` | 双方向 | UNIX ドメインソケット。4 バイト big-endian 長 + postcard のフレーム。部屋 head の報告もここで行う。 |
| `alhalte-cellar` | 送信・取得 | `POST /store`、`POST /store/{tin_id}`、`GET /tin/{tin_id}`、`POST /unregister`。封入した chunk、scene、room の永続化と旧 head の解放。内部書き込みは `X-Alhalte-Service-Token` で認可する。 |

CLI（`CliArgs`）:

- `--room-id` / `ALHALTE_ROOM_ID`: 16 hex（正）または移行用 decimal の `u64`。内部の `RoomData.room_id` は `u64` のまま。
- `--admin`（別名 `--owner`）/ `ALHALTE_ADMIN_ACCOUNT`
- `--socket` / `ALHALTE_SOCKET_PATH`（既定 `/tmp/alhalte-banquette.sock`）
- `--cellar-url` / `ALHALTE_CELLAR_URL`（既定 `http://127.0.0.1:8080`）
- `--chunk-size-exponent` / `ALHALTE_CHUNK_SIZE_EXPONENT`（既定 8）
- `--heartbeat-interval-secs` / `ALHALTE_HEARTBEAT_INTERVAL_SECS`（既定 240）
- `--room-tin` / `ALHALTE_ROOM_TIN`: 復元する `RoomData` の 64 hex tin id。指定時は必ず cellar から復元し、失敗したら起動しない。
- `--service-token` / `ALHALTE_CELLAR_SERVICE_TOKEN`: 内部書き込み用トークン。値はログに出さない。

### 主な公開型

| API | 内容 |
| --- | --- |
| `Room` / `RoomData` / `RoomPermissionLevel` | 部屋全体。scene biblio、character biblio、権限、chunk exponent。`room_id` は `u64`。 |
| `Room::persist` / `Room::load_from_cellar` / `Room::from_tin_with_head` | 部屋全体の保存・復元。復元は room_id と root scene を検証する。 |
| `HeartbeatOutcome` / `HeartbeatResult` | heartbeat の継続判定と、保存に成功した `RoomData` tin id。 |
| `Scene` / `SceneData` | 独立した log と state を持つ scene。 |
| `SceneLog` / `LogBiblio` / `LogChunk` | chunk 化された log と head。 |
| `SceneState` | `BTreeMap<String, Tin>`。4KiB 制限と `::` 名前空間検証。 |
| `Character` / `CharacterBiblio` | `HashMap<String, Tin>` と所有権。 |
| `ClientFrame` / `ServerFrame` | クライアントと banquette の論理フレーム。 |
| `MaitreDToBanquette` / `BanquetteToMaitreD` | maitre-d と banquette の制御フレーム。`BanquetteToMaitreD::RoomPersisted` で head と `done` を報告し、`SetAdmins` / `AdminsUpdated` で DB の GM 集合を実行中の部屋へ同期する。 |
| `ProtocolCodec<T>` | 長さ前置 postcard コーデック。 |
| `BanquetteServer` / `ServerConfig` | UNIX ソケットサーバー。`run_until_shutdown` で明示的に停止できる。 |
| `CellarSink` / `HttpCellarSink` / `InMemoryCellarSink` | cellar 永続化の抽象。`unregister_tin` を含む。 |
| `INTERNAL_WRITE_SESSION_ID` | 内部書き込みが送る session id（`0`）。合成 session は使わない。 |

### プロトコル

`MaitreDToBanquette` は `ClientConnected`、`ClientDisconnected`、`ClientMessage`、`Ping`、`ExportRoom`、`SetAdmins { admins }` を持つ。

`BanquetteToMaitreD` は `SendToClient`、`BroadcastToScene`、`BroadcastToRoom`、`Pong`、`RoomTerminated`、`ExportRoomResponse`、`RoomPersisted`、`AdminsUpdated { admins }` を持つ。`RoomPersisted { tin_id, done }` は `RoomData` の store が成功したときだけ送られ、`maitre-d` が `room_heads` と `rooms.done` を更新する。`done` を `RoomData` の外に出すことで wire schema を変えずに pre-spawn gate を永続化できる。

`SetAdmins` は maitre-d が DB の `room_permissions` を正として送る内部制御フレームで、`Room::apply_admin_set` が `std::admins` を置き換える。クライアント由来の `ClientFrame` ではないため caller の管理者チェックは行わない（認可は maitre-d 側で完了している）。成功すると `AdminsUpdated` を返す。`Room::set_admins` は従来どおりクライアント向けの管理者チェック付き経路として残す。

`SetDone` は `Room::set_done` が成功したときだけ `ServerFrame::DoneUpdated { done }` を要求元へ返す。失敗時は `SetDoneFailed` のみで、`maitre-d` は成功 frame を観測するまで `rooms.done` を更新しない。

`ClientFrame` は scene 一覧・詳細・作成・削除・入退室、state の取得・更新・削除、log の append/edit/delete/rewind/取得、character の CRUD・移動、WebRTC signaling、heartbeat 応答、BGM 変更を持つ。`ServerFrame` はその結果と broadcast を返す。

- `ListScenes` / `CreateScene` / `DeleteScene` / `MoveCharacter` の scene 一覧は `SceneSummary`（`id`、`name`、`present_character_count`、`active_client_count`、`is_loaded`）の列 `SceneList` として返す。`name` は scene state `std::name` の UTF-8 文字列で、未設定なら scene id、`/` は作成時に `メインシーン` が入る。
- `CreateScene { scene_id, inherit_juices, name }` は `name` を `std::name` へ作成と同時に書き、`SceneCreated` を要求元へ返し、`SceneList` と新 scene の `SceneInfo` を room 全体へ broadcast する。`std::name` は juice 継承の対象外。
- `UpdateState` で `key == "std::name"` が成功したときは `StateUpdated` の ack / scene broadcast に加えて `SceneInfo` を room 全体へ broadcast する。これにより他クライアントは再読み込みなしで rename を反映できる。
- `EnterScene` 成功時、caller が `can_access_scene` を満たす場合はその scene の `StateSnapshot` を直接返す。クライアントは入室ごとの `QueryState` を必要としない（`QueryState` は従来どおり明示問い合わせ用に残る）。
- `CreateScene` / `DeleteScene` の成功時は要求元へ `SceneCreated` / `SceneDeleted` を返し、更新後の scene 一覧を `SceneList` として room 全体へ broadcast する。`MoveCharacter` の成功時は `CharacterMoved` を返し、移動先 scene が暗黙に作られた場合は `SceneList` も broadcast する。
- state の `UpdateState` / `DeleteState` 成功時は、要求元への直接 ack と対象 scene への broadcast の両方を行う。ルート scene `/` の state は他 scene から編集される（アセット・拡張機能）ため、要求元が対象 scene に居なくても ack が届く。両方を受け取ったクライアントは同じ frame を 2 回適用するが、state 更新・削除は冪等。
- `ClientFrame::ChangeBgm { scene_id, slot, tin_id }` は `tin_id` の 64 hex 文字列を `TinId` に復元し、`ChangeBgmEvent` を log に append する。成功は既存の `LogEventAppended` broadcast で観測できる。BGM の現在値は `std::bgm` / `std::bgm<N>` の state が持つ（slot 番号は任意）。

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
- `std::bgm`、`std::bgm<N>`（BGM slot 1 / 任意 slot N の Tin ID）、`bgm::<slot>::volume`（音量）、`std::b-img`（背景画像の `{ tinId, mimeType }` JSON）、`std::b-img::width` / `std::b-img::height`（背景の表示寸法、数値）
- scene 名の `std::name`（scene state の UTF-8 文字列。character 属性にも同名キーがある）
- character 属性の `std::name`、`std::owner_account`
- cutlery 予約: `asset::<id>`（`/` のアセットメタデータ）、`extension::<id>`（`/` の拡張機能メタデータ）

state キーは 1 個以上の `::` を含む必要がある。値は `to_plaintext().len()` が 4KiB 以下。`std::admins` の変更は管理者のみ。

## 内部フロー

### 起動

1. `--room-tin` が指定された場合、`Room::load_from_cellar` が cellar から tin を取得する。
   - `Tin::decode` で復号し、tin id（内容ハッシュ）が head と一致することを確認する。
   - `Room::from_tin` が `RoomData` を復元し、root scene `/` の存在を検証する。
   - `RoomData.room_id` が `--room-id` と一致することを検証する。
   - 取得・復号・検証のいずれかが失敗したら**空部屋で起動せず**、明確なエラーで終了する。
2. `--room-tin` がない場合は `Room::new(room_id, creator_account, chunk_exponent)` が `/` を作り、作成者を管理者として `std::admins` に書き込む。
3. `BanquetteServer::run` が既存ソケットを削除し、UNIX ソケットを bind する。
4. heartbeat タスクを起動する。初回 tick は捨てる。
5. maitre-d からの接続ごとに `handle_connection` を spawn する。

### 部屋状態の永続化と復元

- heartbeat ごとに `Room::persist` が部屋全体を `alhalte/banquette` Tin（Zstd）として cellar に保存する。内部書き込みは `INTERNAL_WRITE_SESSION_ID = 0` と `X-Alhalte-Service-Token` を使い、合成 session は使わない。
- 保存に成功した tin id と `RoomData.done` を `HeartbeatResult::persisted_tin` と `RoomPersisted { tin_id, done }` で通知する。保存失敗時は通知せず、ログに残して次周期に再試行する。
- 保存に成功すると、直前の head tin を `CellarSink::unregister_tin` で best-effort に解放する。現在の head は解放しない。解放失敗は警告ログのみで、保存自体は成功扱い。
- graceful shutdown（idle による `ShouldExit`、`run_until_shutdown`、または standalone バイナリの SIGTERM / SIGINT）では終了直前に `persist` をもう一度行い、`RoomPersisted` を送ってからソケットを閉じる。SIGTERM / SIGINT は `main.rs` で `watch` channel に配線し、`run_until_shutdown` へ渡す。
- `room_id → head tin` のポインタは `alhalte-maitre-d` の `room_heads` が持つ。banquette 自身は head を永続化しない。

### クライアント接続

1. `ClientConnected` で `ConnectedClient` を登録し、クライアントを `/` に入室させる。
2. `ClientMessage` を `ClientFrame` として処理する。`EnterScene` 成功時は access 可能なら `StateSnapshot` を返し、クライアントは scene の state を即座に復元できる。
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

### Issue #158 の権限同期（このブランチ）

- DB の `room_permissions` を正とし、`maitre-d` が `SetAdmins { admins }` で GM 集合全体を送る。banquette は `Room::apply_admin_set` で `std::admins` を置き換え、`AdminsUpdated` を返す。この経路はクライアント由来ではないため caller の管理者チェックを行わない。
- `SetDone` の成功は `ServerFrame::DoneUpdated { done }` で通知する。`maitre-d` はこの frame を受けるまで `rooms.done` を更新しない。
- 4 段階権限モデル / scene 認可（#114）は別ブランチのまま。

## 既知の差分・制約

- Issue #114 のブランチでは変更が未完了で、`cargo check` が `room.rs` のパターンで失敗する状態。main には未反映。
- 権限同期は GM 集合（`std::admins`）の push のみで、`pl` / `viewer` の差分を banquette に送る専用 frame は持たない。viewer の mutate 遮断は引き続き `maitre-d` が行う。
- `RoomData` の `done` から `default_permission` への変更に伴い、`alhalte-kitchen` の `RoomArchive` がまだ `room_data.done` を参照している。
- `ChangeBgmEvent` は `slot: u32` を主とし、旧 `track_index: Option<u8>` をフォールバックとして受け付ける。
- log の実体は `HashMap<String, Tin>` を duck typing する運用で、`content` / `speaker` / `timestamp` のキー規約は cutlery と saignant に委ねる。
- WebRTC の DataChannel は signaling の中継までを実装し、state の DataChannel 経路そのものは未実装。
- `RoomData` は `HashMap` を含むため、内容が同じでも postcard の map 順序が変わり tin id が変わることがある。heartbeat ごとに head が作り直されるが、旧 head の unregister と GC で回収される。wire schema は変更していない。
- `persist` の失敗はログのみで、次周期までの再試行はない。メトリクスや専用の再試行キューは持たない。
- 合成 session（旧 `system_session_id = 1`）は廃止した。`--service-token` が未設定の場合、内部書き込みは cellar の session 検証に失敗する。
- graceful shutdown の経路は idle 終了と `run_until_shutdown`（および standalone バイナリの SIGTERM / SIGINT）。SIGTERM / SIGINT は `main.rs` で捕捉し、最終 persist を含む同じ graceful path に入る。シグナル配線そのものはプロセス起動を伴う統合テスト（`tests/spec_tests.rs` の `test_sigterm_runs_graceful_final_persist`）で確認する。
- `Room::load_from_cellar` の download は cellar 側で認証されない（現行 cellar の download は session 検証を行わない）。service token は書き込みだけを守る。
- `maitre-d` が記録した head が別インスタンスの persist で置き換わった場合、`maitre-d` はその旧 head を unregister しない（banquette が自分の直前 head だけを解放する）。

