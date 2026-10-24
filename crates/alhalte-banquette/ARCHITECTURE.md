# alhalte-banquette

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-banquette`  
> テスト: `crates/alhalte-banquette/tests/spec_tests.rs`

## 責務

1 つの部屋（room）を管理する独立プロセスである。シーン（scene）、ログチャンク（log chunk）、シーン状態（scene state）、キャラクター（character）、および部屋内のアカウント権限をメモリ上に保持し、`alhalte-maitre-d` から多重化されたローカル IPC 接続を受け付ける。定期的なハートビートおよび終了時（アイドルタイムアウト・`ShutdownRoom` 受信・シグナル受信）に、部屋全体の状態を `alhalte/banquette` ラベルの `Tin` として `alhalte-cellar` へ永続化する。

## 公開 API / 外部接続点

### CLI（`alhalte-banquette` バイナリ）

| 引数 / 環境変数 | 既定値 | 内容 |
| --- | --- | --- |
| `--room-id` / `ALHALTE_ROOM_ID` | 必須 | 16 桁 hex の部屋 ID（内部表現は `u64`、10 進文字列も受理）。 |
| `--socket` / `ALHALTE_BANQUETTE_SOCKET` | 必須 | ローカル IPC のエンドポイント（Unix は UNIX ドメインソケットのパス、Windows は `\\.\pipe\...` の名前付きパイプ）。 |
| `--admin-account` / `ALHALTE_ADMIN_ACCOUNT` | `0` | 部屋の新規作成時に初期 `Admin` として登録するアカウント ID。 |
| `--cellar-url` / `ALHALTE_CELLAR_URL` | 任意 | 永続化先の `alhalte-cellar` ベース URL。省略時はインメモリの `MemoryCellarSink` を使用する。 |
| `--chunk-size-exponent` / `ALHALTE_CHUNK_SIZE_EXPONENT` | `8` | 1 チャンクあたりのログ収容数 $2^n$ の指数 $n$（既定は $2^8 = 256$ 件）。 |
| `--heartbeat-interval-secs` / `ALHALTE_HEARTBEAT_INTERVAL_SECS` | `240` | ハートビートおよび定期永続化の間隔（秒）。 |
| `--max-tin-size` / `ALHALTE_MAX_TIN_SIZE` | `67108864` (64 MiB) | `cellar` と送受信する `Tin` の最大バイト数。 |
| `--service-token-file` / `ALHALTE_CELLAR_SERVICE_TOKEN_FILE` | 任意 | `cellar` への内部書き込み認証トークンを格納したファイルのパス。 |
| `--service-token` / `ALHALTE_CELLAR_SERVICE_TOKEN` | 任意 | 内部書き込み認証トークンの直接指定（後方互換用。`--service-token-file` を優先）。 |

### ローカル IPC（対 `alhalte-maitre-d`）

プラットフォーム固有のローカル IPC（Unix: `tokio::net::UnixListener`、Windows: `tokio::net::windows::named_pipe`）上で、4 バイトのビッグエンディアン長プレフィックス付き `postcard` フレームを送受信する（1 フレーム最大 16 MiB）。1 本の IPC 接続上で複数クライアントのセッションを多重化する。

#### `MaitreDToBanquette`（`maitre-d` → `banquette`）

| フレーム | 内容 |
| --- | --- |
| `ClientConnected { client_id, account_id, session_id }` | クライアントの接続通知。`account_id == 0` は匿名接続を表す。 |
| `ClientDisconnected { client_id }` | クライアントの切断通知。全シーンから退去させる。 |
| `ClientMessage { client_id, frame: ClientFrame }` | クライアントから送信された操作・照会フレーム。 |
| `ExportRoom { request_id, caller_account, include_state }` | 部屋のエクスポート要求。呼び出し元の権限に応じた ZIP バイト列を返す。 |
| `QueryAccountPermission { request_id, account_id }` | 指定アカウントの実効権限（`RoomPermissionLevel`）の照会。 |
| `SetMemberPermission { request_id, caller_account, target_account, permission }` | 指定アカウントの権限変更または削除（`Admin` のみ実行可）。 |
| `SetDefaultPermission { request_id, caller_account, permission }` | 部屋の既定権限の変更（`Admin` のみ実行可）。 |
| `QueryPermissions { request_id, caller_account }` | 既定権限および個別権限マップの照会。 |
| `ShutdownRoom` | `maitre-d` からのグレースフルシャットダウン要求。受信すると最終状態を `cellar` に永続化してプロセスを正常終了する（Issue #291）。 |

#### `BanquetteToMaitreD`（`banquette` → `maitre-d`）

| フレーム | 内容 |
| --- | --- |
| `SendToClient { client_id, frame: ServerFrame }` | 特定クライアントへのレスポンスまたは通知。 |
| `BroadcastToAll { frame: ServerFrame }` | 部屋内の全接続クライアントへのブロードキャスト。 |
| `BroadcastToScene { scene_id, frame: ServerFrame }` | 指定シーンに在籍するクライアントへのブロードキャスト。 |
| `DisconnectClient { client_id, reason }` | クライアントの強制切断要求。 |
| `ExportRoomResponse { request_id, result: Result<Vec<u8>, String> }` | `ExportRoom` に対する ZIP アーカイブの応答。 |
| `AccountPermissionResponse { request_id, permission }` | `QueryAccountPermission` に対する実効権限の応答。 |
| `PermissionMutationResponse { request_id, result }` | `SetMemberPermission` / `SetDefaultPermission` の成否応答。 |
| `PermissionsResponse { request_id, default_permission, permissions }` | `QueryPermissions` に対する権限表スナップショットの応答。 |

#### `ClientFrame` / `ServerFrame`（WebSocket クライアントとの論理プロトコル）

- **クライアント → サーバー (`ClientFrame`)**:
  - シーン操作: `CreateScene { id, name? }`、`DeleteScene { id }`、`EnterScene { scene_id }`、`LeaveScene { scene_id }`、`ListScenes`
  - ログ操作: `AppendLog { scene_id, entry: TinContent }`、`EditLog { scene_id, chunk_index, position, new_entry: TinContent }`、`RewindLog { scene_id, to_chunk_index, to_position }`、`QueryLogBiblio { scene_id }`、`QueryLogChunk { scene_id, chunk_index }`
  - ステート操作: `UpdateState { scene_id, key, value: TinContent }`、`DeleteState { scene_id, key }`、`QueryState { scene_id }`
  - キャラクター操作: `CreateCharacter { character_id?, data: HashMap<String, TinContent> }`、`UpdateCharacter { character_id, data: HashMap<String, TinContent> }`、`DeleteCharacter { character_id }`、`GetCharacter { character_id }`、`ListCharacters`、`MoveCharacter { character_id, to_scene_id }`
  - 権限操作: `SetMemberPermission { account_id, permission }`、`SetDefaultPermission { permission }`、`QueryPermissions`
  - 死活監視: `HeartbeatResponse { timestamp }`
- **サーバー → クライアント (`ServerFrame`)**:
  - シーン通知: `SceneList { scenes: Vec<SceneSummary> }`、`SceneInfo(SceneSummary)`、`SceneCreated { id }`、`SceneDeleted { id }`
  - ログ通知: `LogEventAppended { scene_id, chunk_index, position, entry: TinContent }`、`LogEventEdited { scene_id, chunk_index, position, new_entry: TinContent }`、`LogEventRewound { scene_id, to_chunk_index, to_position }`、`LogChunkSealed { scene_id, chunk_index, tin_id }`、`LogBiblioSnapshot { scene_id, biblio }`、`LogChunkSnapshot { scene_id, chunk_index, entries: Vec<Option<TinContent>> }`
  - ステート通知: `StateUpdated { scene_id, key, value: TinContent }`、`StateDeleted { scene_id, key }`、`StateSnapshot { scene_id, entries: BTreeMap<String, TinContent> }`
  - キャラクター通知: `CharacterCreated { character_id }`、`CharacterUpdated { character_id }`、`CharacterDeleted { character_id }`、`CharacterMoved { character_id, from_scene_id, to_scene_id }`、`CharacterInfo { character_id, data: HashMap<String, TinContent> }`、`CharacterList { characters: HashMap<u64, HashMap<String, TinContent>> }`
  - 権限通知: `PermissionsUpdated { default_permission, permissions }`、`PermissionsSnapshot { default_permission, permissions }`
  - 死活監視・エラー: `HeartbeatPing { timestamp }`、`Error { code, message }`

### HTTP（対 `alhalte-cellar`）

`CellarSink` トレイトの HTTP 実装（`HttpCellarSink`）が `alhalte-cellar` と通信する。内部書き込みにはすべて `X-Alhalte-Service-Token` ヘッダを付与し、`X-Alhalte-Owner: 0`（システム所有）として扱う。

- `POST /store` および `PUT /store/{tin_id}`: チャンク `Tin`（`alhalte/banquette/log/chunk`）および部屋状態 `Tin`（`alhalte/banquette`）の保存。リクエスト・レスポンスの上限は `--max-tin-size`（既定 64 MiB）に従う。
- `DELETE /tin/{tin_id}`: チャンク再封印時や旧 head 置き換え時の参照削除（ベストエフォート）。
- `GET /alias/room/{room_id}/head` および `PUT /alias/room/{room_id}/head`: 部屋の永続化 head ポインタの取得と原子的更新。
- `GET /tin/{tin_id}`: 起動時の部屋状態復元およびエクスポート時の過去チャンク取得。

## データ形式

### `Tin` と `TinContent`（Issue #272）

`tin` クレート（`crates/tin-rs`）の型分離に従い、ラベルとペイロードの組は `TinContent`（`label: u64`, `payload: Vec<u8>`）、ワイヤー上の圧縮・暗号化情報（`Opener`）と識別子（`TinId`）を伴うコンテナは `Tin` として扱う。

- メモリ上のシーン state（`SceneState`）、キャラクター属性（`Character`）、ログエントリ（`LogChunk`）、および IPC / WebSocket フレーム内の値はすべて `TinContent` を保持する。
- 文字列や整数などのプリミティブ値と `TinContent` の相互変換には、標準の `From` / `TryFrom`（`TryInto`）実装（`TinContent::from("...")`、`String::try_from(&content)`、`u64::try_from(&content)` など）を用いる。
- `cellar` に保存・取得するチャンクや部屋全体のスナップショットは、`TinContent` を `Opener`（平文 + LZ4 または無圧縮）でエンコードした `Tin` として扱う。

### ラベル定義

`label_hash(ident)`（XXH64, seed = 0）により以下のラベルを使用する。

| 識別子 | 定数名 | 用途 |
| --- | --- | --- |
| `alhalte/banquette` | `LABEL_BANQUETTE` | 部屋全体の永続化スナップショット（`RoomData` の postcard 直列化）。 |
| `alhalte/banquette/log/biblio` | `LABEL_LOG_BIBLIO` | シーンのログ書誌情報（`LogBiblio` の postcard 直列化）。 |
| `alhalte/banquette/log/chunk` | `LABEL_LOG_CHUNK` | ログチャンク（`Vec<Option<TinContent>>` の postcard 直列化）。 |
| `alhalte/banquette/log/character-came` | `LABEL_CHARACTER_CAME` | キャラクター入室システムログ（`(character_id: u64, from_scene: Option<String>)`）。 |
| `alhalte/banquette/log/character-gone` | `LABEL_CHARACTER_GONE` | キャラクター退室システムログ（`(character_id: u64, to_scene: Option<String>)`）。 |

### 永続化構造（`RoomData`）

```rust
pub struct RoomData {
    pub room_id: u64,
    pub chunk_size_exponent: u8,
    pub scenes: BTreeMap<String, PersistedScene>,
    pub characters: HashMap<u64, HashMap<String, TinContent>>,
    pub next_character_id: u64,
    pub default_permission: RoomPermissionLevel,
    pub permissions: BTreeMap<u64, RoomPermissionLevel>,
    pub accessible_scenes: BTreeMap<u64, BTreeSet<String>>,
}
```

- **ルートシーン `/` (`ROOT_SCENE_ID`)**: すべての部屋に必ず存在し、削除できない。部屋全体の共有設定（拡張機能メタデータ `extension::*` や部屋アセット `asset::*` など）の格納先としても使われる。
- **シーン状態 (`SceneState`)**: `BTreeMap<String, TinContent>`。1 エントリあたりの平文サイズ（`TinContent::to_plaintext().len()`、すなわち 8 バイトの label + payload）は最大 **4 KiB（4,096 バイト）** に制限される。予約キー `std::name`（`primitive/string`）をシーンの表示名として扱う。
- **キャラクター (`Character`)**: `HashMap<String, TinContent>`。予約キー `std::name`（表示名）と `std::owner_account`（所有者アカウント ID、`primitive/u64`）を持つ。
- **権限 (`RoomPermissionLevel`)**:
  - `Denied`: 入室・閲覧・操作すべて不可。
  - `Viewer`: 全シーンの閲覧（ステート・ログ・キャラクター・エクスポート）が可能。書き込みは不可。
  - `Participant`: ルートシーン `/` および `accessible_scenes` で許可されたシーン（自キャラクターが移動したシーンを含む）の閲覧・書き込みが可能。自キャラクターの作成・編集・削除が可能。
  - `Admin`: 全シーンの閲覧・書き込み、シーン作成・削除、全キャラクターの操作・移動、部屋の権限設定が可能。部屋には常に最低 1 人の `Admin` が存在しなければならない。

## 内部フロー

### 起動と状態復元

1. `maitre-d`（またはスタンドアロン起動）から `--room-id`、`--socket`、`--admin-account`、`--cellar-url`、`--service-token-file`、`--max-tin-size` 等を受け取って起動する。
2. `cellar` のエイリアス `GET /alias/room/{room_id}/head` を照会する。
   - **エイリアスが存在する場合**: 指し示された `tin_id` を `GET /tin/{tin_id}` で取得し、`Tin::decode` で検証・展開して `Room::from_persisted` で復元する。`room_id` の不一致、フォーマット破損、または `Room::validate`（最低 1 人の `Admin`、実在シーンのみの参照など）の失敗時はエラーで即座に終了し、**空の部屋で上書きしない**（fail-closed）。
   - **エイリアスが存在しない場合（404）**: `Room::new(room_id, chunk_size_exponent, admin_account)` でルートシーン `/` を持つ新規部屋を作成する。
3. ローカルソケット（または名前付きパイプ）をバインドし、`maitre-d` からの IPC 接続を待ち受ける。

### クライアント接続とシーン入退室

1. `ClientConnected` を受信すると、該当クライアントをルートシーン `/` に配置し、`SceneList` および `CharacterList` を送信する。
2. クライアントが `EnterScene { scene_id }` を送ると、`can_access_scene` で閲覧権限を検査する。許可されていれば対象シーンにクライアントを参加させ、そのシーンの `StateSnapshot` を即座に返す。
3. `CreateCharacter` では作成者のアカウント ID を `std::owner_account` に自動設定し、初期配置としてルートシーン `/` に参加させる。
4. `MoveCharacter`（`Admin` のみ実行可）では、移動元シーンに `character-gone`、移動先シーンに `character-came` のシステムログを追記し、対象キャラクターの所有者（`Participant`）の `accessible_scenes` に移動先シーンを追加する。

### ログの追記とチャンク封印

1. `AppendLog` / `EditLog` 受信時、送信者が対象シーンへの書き込み権限（`can_send_to_scene`）を持つか検査する。さらにログエントリに話者（`speaker_id`）が含まれる場合、`Admin` でなければ自身の所有キャラクター（`std::owner_account == caller`）であることを検証し、違反時は `SpeakerNotOwned` エラーを返す（Issue #271）。
2. アクティブチャンク（末尾の未封印チャンク）に `TinContent` を追加し、シーン内のクライアントに `LogEventAppended` をブロードキャストする。
3. アクティブチャンクのエントリ数が $2^{\text{chunk\_size\_exponent}}$ に達すると、チャンクを `alhalte/banquette/log/chunk` ラベルの `Tin`（LZ4 圧縮・平文）にエンコードして `cellar` に保存し、`LogBiblio` に `TinId` を記録して `LogChunkSealed` を通知する。
4. 封印済みチャンクに対して `EditLog` や `RewindLog` が行われた場合、`cellar` から該当チャンクを取得・変更して再封印し、旧チャンクの参照を `cellar` から削除する。

### ハートビート・永続化・グレースフルシャットダウン（Issue #291）

1. `heartbeat_interval` ごとに全クライアントへ `HeartbeatPing` を送り、応答のない非アクティブクライアントを切断する。
2. 続けて `Room::persist` を実行する:
   - 未封印のアクティブチャンクも含め、必要なチャンクを `cellar` に保存する。
   - `RoomData` を `alhalte/banquette` ラベルの `Tin` にエンコードして `cellar` に保存する。
   - 保存成功後、`PUT /alias/room/{room_id}/head` でエイリアスを新しい `TinId` に更新する。
   - 更新前の旧 head `TinId`（および更新された旧チャンク）の参照を `DELETE /tin/{old_id}` でベストエフォート削除し、`cellar` の GC が回収できるようにする。
3. **グレースフルシャットダウン**:
   - 以下のいずれかの終了契機が発生した場合、ハートビートループを停止し、終了直前に最終の `Room::persist` を 1 回実行してからプロセスを正常終了する:
     1. **IPC シャットダウン要求 (`MaitreDToBanquette::ShutdownRoom`)**: `maitre-d` が部屋を閉じる、または `maitre-d` 自身が停止する際に送信する制御フレーム（Issue #291）。Windows の名前付きパイプ環境を含め、OS シグナルに依存せず確実に最終永続化を行える。
     2. **アイドルタイムアウト (`HeartbeatOutcome::ShouldExit`)**: 接続クライアントが 0 人のままハートビート周期が経過した場合。
     3. **OS シグナル (`SIGTERM` / `SIGINT` / Windows Ctrl+C)**: `run_until_shutdown` に配線されたシャットダウンチャネル経由で捕捉した場合。

### エクスポート

1. `ExportRoom { request_id, caller_account, include_state }` を受信すると、まずメモリ上の最新状態を `cellar` にフラッシュし、呼び出し元が閲覧可能なシーンだけを抽出する。
2. `include_state == false`（`type=logs`）の場合は、全シーンの `state` を空にしてから `RoomData` を構築する。
3. `room.tin`（フィルタ済み `RoomData`）と `chunks/<tin_id_hex>.tin`（対象シーンの全チャンク）を無圧縮（Stored）の ZIP アーカイブにまとめ、`ExportRoomResponse` で `maitre-d` に返す。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- `Room::validate` による不変条件:
  - ルートシーン `/` が常に存在する。
  - `permissions` 内に最低 1 人の `Admin` が存在する（最後の `Admin` の降格・削除は `LastAdminRemoval` で拒否）。
  - `accessible_scenes` は実在するシーンのみを含み、書き込み権限を持たないアカウント（`Denied` / `Viewer`）のエントリを持たない。
- シーン状態の 1 キーあたりの平文サイズ上限は 4 KiB（`StateValueTooLarge`）。`cellar` へ永続化する `Tin` 全体の上限は `--max-tin-size`（既定 64 MiB）に従う。
- 永続化データの復元失敗時は空の部屋へフォールバックせず、起動エラーとして終了する。

## 既知の差分・制約

- シーン状態における BGM（`std::bgm*` / `bgm::*`）、背景画像（`std::b-img*`）、駒配置（`piece::*`）、アセット（`asset::*`）、拡張機能（`extension::*`）の各キーはクライアント（`alhalte-argenterie`）側の規約であり、`banquette` 自身は `std::name` 以外のステートキーを特別扱いしない（Issue #186）。
- `character-came` / `character-gone` のシステムログはペイロード内に時刻を持たない。クライアントやエクスポート変換側でチャンク内の順序に基づいて扱う。
- キャラクターを削除すると、全シーンのステートから `piece::<character_id>::*` キーがベストエフォートで削除され、`StateDeleted` がブロードキャストされる。
