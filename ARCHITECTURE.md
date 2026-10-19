# Alhalte

セルフホスト VTT のためのモジュール群。ccfolia などとは異なり、PL や GM の運用による仮想的な概念ではなく、**シーンを第一級概念**として扱う。

この文書はリポジトリ全体の見取り図である。各モジュールの責務、公開 API、外部接続点、内部フロー、実装上の制約は、それぞれのディレクトリにある `ARCHITECTURE.md` を正とする。より下位の詳細（関数単位の分岐、エラー型、テストケースなど）はコード内の rustdoc / コメントに委任する。

```text
全体構成   = restaurant
配布構成   = franchise
client 総体 = cutlery
```

## 用語

| 用語 | 意味 |
| --- | --- |
| restaurant | 1 つの Alhalte インスタンス全体。`Bistrot.toml` で構成する。 |
| franchise | restaurant から配布される構成単位。 |
| cutlery | クライアント側モジュール総体。 |
| tin | ラベル付き・自己記述的なバイナリコンテナ。暗号化と圧縮を opener で指定する。 |
| generast | チャットの汎用構文木。文字列、要素 Tin、ネストした装飾からなる。 |
| scene | 独立したログと state を持つ空間。ルート仮想シーンは `/`。 |
| room | banquette プロセスがホストするセッション空間。複数の scene を持つ。 |
| label | tin の payload 形式を表す 64bit 値。識別子文字列の XXH64（seed 0）。 |

## 全体構成

```text
                         ┌──────────────────────────────┐
                         │ Browser                      │
                         │  alhalte-argenterie (Svelte) │
                         │   ├─ @alhalte/pantry         │
                         │   ├─ alhalte-saignant.wasm   │
                         │   └─ extension .wasm         │
                         └───────┬───────────────┬──────┘
                                 │ HTTP          │ WebSocket
                    /api, /auth  │               │ /ws/room/{room_id}
                    /room/...    │               │
                                 ▼               ▼
                         ┌──────────────────────────────┐
                         │ alhalte-maitre-d (axum)      │
                         │ OAuth / account / session    │
                         │ room permission / WS demux   │
                         └───────┬───────────────┬──────┘
                                 │ UNIX socket   │ library
                                 │ postcard      │ alhalte-kitchen
                                 ▼               │
                         ┌──────────────────────────────┐
                         │ alhalte-banquette (1 room)   │
                         │ scene / log / state / chars  │
                         └───────┬──────────────────────┘
                                 │ HTTP
                                 │ /store, /tin/{id}
                                 ▼
                         ┌──────────────────────────────┐
                         │ alhalte-cellar (axum)        │
                         │ SQLite references + files    │
                         └──────────────────────────────┘

  pantry (browser) ── HTTP GET /tin/{id} ──▶ cellar
  alhalte-bistrot ── Bistrot.toml ──▶ 引数 / systemd unit
  alhalte-kitchen ── archive ──▶ RoomGraph ──▶ HTML5 / EPUB（saignant 使用）
```

### レイヤ

- **共有ライブラリ**: `tin`、`generast`。全モジュールが同じバイト表現と AST を使う。
- **サーバー**: `alhalte-maitre-d`（認証・玄関口）、`alhalte-banquette`（部屋プロセス）、`alhalte-cellar`（重複排除ストレージ）。
- **クライアント**: `web/alhalte-argenterie`（UI）、`web/alhalte-pantry`（tin キャッシュ・復号）、`alhalte-saignant`（HTML レンダラ、WASM でも動作）。
- **変換・配布**: `alhalte-canner`（再エンコード・暗号化）、`alhalte-kitchen`（アーカイブ・グラフ・EPUB/HTML5）、`extensions/*`（構文木変換 wasm）。
- **運用**: `alhalte-bistrot`（設定・引数・systemd 生成）。

## モジュール一覧

| モジュール | 実装 | 責務 | 文書 |
| --- | --- | --- | --- |
| tin-rs | `crates/tin-rs`（package `tin`） | 自己記述バイナリコンテナ、暗号・圧縮・tin id | [crates/tin-rs/ARCHITECTURE.md](crates/tin-rs/ARCHITECTURE.md) |
| generast-rs | `crates/generast-rs`（package `generast`） | 汎用構文木、ラベル、決定論的ダイス評価、判定木 | [crates/generast-rs/ARCHITECTURE.md](crates/generast-rs/ARCHITECTURE.md) |
| alhalte-banquette | `crates/alhalte-banquette` | 部屋プロセス、scene、log chunk、state、character | [crates/alhalte-banquette/ARCHITECTURE.md](crates/alhalte-banquette/ARCHITECTURE.md) |
| alhalte-maitre-d | `crates/alhalte-maitre-d` | Google OAuth、account/session、HTTP/WS 玄関口、demux | [crates/alhalte-maitre-d/ARCHITECTURE.md](crates/alhalte-maitre-d/ARCHITECTURE.md) |
| alhalte-cellar | `crates/alhalte-cellar` | tin 実体の重複排除保存、参照管理、GC | [crates/alhalte-cellar/ARCHITECTURE.md](crates/alhalte-cellar/ARCHITECTURE.md) |
| alhalte-canner | `crates/alhalte-canner` | 画像・音声の決定論的再エンコード、高低エントロピー暗号化 | [crates/alhalte-canner/ARCHITECTURE.md](crates/alhalte-canner/ARCHITECTURE.md) |
| alhalte-kitchen | `crates/alhalte-kitchen` | 部屋アーカイブ、滞在グラフ、HTML5/EPUB 変換 | [crates/alhalte-kitchen/ARCHITECTURE.md](crates/alhalte-kitchen/ARCHITECTURE.md) |
| alhalte-saignant | `crates/alhalte-saignant` | log / Generast の HTML レンダリング、CSS、WASM ABI | [crates/alhalte-saignant/ARCHITECTURE.md](crates/alhalte-saignant/ARCHITECTURE.md) |
| alhalte-bistrot | `crates/alhalte-bistrot` | `Bistrot.toml`、引数生成、systemd 生成、rops | [crates/alhalte-bistrot/ARCHITECTURE.md](crates/alhalte-bistrot/ARCHITECTURE.md) |
| alhalte-pantry | `web/alhalte-pantry` | ブラウザ側 tin キャッシュ、取得、復号、展開 | [web/alhalte-pantry/ARCHITECTURE.md](web/alhalte-pantry/ARCHITECTURE.md) |
| alhalte-argenterie | `web/alhalte-argenterie` | Svelte 製 cutlery、部屋 UI、拡張機能ホスト | [web/alhalte-argenterie/ARCHITECTURE.md](web/alhalte-argenterie/ARCHITECTURE.md) |
| 拡張機能 | `extensions/*` | `process_generast` を実装する wasm | [extensions/ARCHITECTURE.md](extensions/ARCHITECTURE.md) |

## 主要なデータフロー

### 部屋作成

1. クライアントは `maitre-d` の `POST /api/rooms` に `{ room_id?, name }` を送る（要 session）。`room_id` を省略するとサーバーが 16 hex を生成する。
2. `maitre-d` は SQLite の `rooms` に部屋を登録し、同じ transaction で作成者を `room_permissions` の `gm` にする。banquette はまだ起動しない。
3. `GET /api/rooms/{room_id}` が名前・caller の role・done を返す（`done` は `RoomData.default_permission != Denied` の射影）。部屋は WS / export で参照されるまでプロセスを持たない。

### 入室

1. ブラウザは部屋 URL `/rooms/<roomId>#<roomKey>` を開く。`roomKey` はフラグメントにだけ存在し、サーバーへ送らない。
2. `argenterie` は `maitre-d` の `/ws/room/<roomId>?account_id=...&session_id=...` へ WebSocket 接続する。
3. `maitre-d` はセッションを検証し、`rooms` に登録済みか確認する。未登録は 404、権限がなく `done=false` なら 403 で拒否する。
4. 許可された接続は `banquette` の UNIX ソケットへ `ClientConnected` として流し込む。bridge の `admin_account` は部屋の作成者 / DB の GM で、接続順には依存しない。banquette が未起動なら `room_heads` の head を渡して起動する。head があり復元できない場合は 500 で失敗し、空部屋は作らない。head がなければ新しい部屋を起動する。
5. banquette はクライアントをルートシーン `/` に入室させる。

### 部屋の永続化

1. banquette は heartbeat（既定 4 分）ごとに部屋全体を `alhalte/banquette` Tin として cellar に保存し、成功した tin id と `default_permission` を `RoomPersisted { tin_id, default_permission }` で `maitre-d` へ返す。
2. `maitre-d` は `room_heads(room_id, tin_id, updated_at)` に head を記録し、`default_permission != Denied` を `rooms.done` に射影する。`rooms.done` は pre-spawn の入場判定が `maitre-d` 再起動後も機能するための暫定フラグで、権限の正は `RoomData.default_permission` / `RoomData.permissions` にある（#200 で判定ごと置き換える）。
3. 新 head の保存後、banquette は旧 head の参照を best-effort で unregister し、cellar GC が実体を回収できるようにする。現在の head は削除しない。
4. idle 終了などの graceful shutdown、および standalone banquette の SIGTERM / SIGINT では、終了直前にもう一度 persist してから終了する。シグナルは `run_until_shutdown` の shutdown channel に配線する。
5. 次回 spawn 時は head から scenes / state / characters / log biblio / 権限（`default_permission` / `permissions` / `accessible_scenes`）を復元する。復元された `default_permission` は次回の `RoomPersisted` で `rooms.done` に書き戻す。

### 発言

1. `argenterie` は入力テキストを `Generast::String` として保持する。
2. 表示時に有効な拡張機能を優先度昇順で適用し、`(Generast, speaker_id)` の Tin を左畳み込みする。
3. 最終 Tin と現在のキャラクター名マップを `saignant` の `render_generast_abi_v2` に渡し、HTML を得る。
4. 送信時は `AppendLog` を `banquette` へ送る。banquette は scene log に追記し、同一シーンのクライアントへ `LogEventAppended` を配信する。
5. chunk が満杯になると `alhalte/banquette/log/chunk` Tin に封入し、`cellar` へ保存する。

### アセット

1. クライアントは `pantry` の `encodeTin` で payload を RoomKey 暗号化・圧縮する（`argenterie` の `encodeRoomTin`）。
2. `maitre-d` の `/api/cellar/store` で保存可否を問い合わせ、210 なら `/api/cellar/store/{tinId}` で本体を送る。owner は maitre-d が session から付与する。
3. 取得時は `/api/cellar/tin/{tinId}` で本体と `X-Alhalte-Wrapped-Key` を受け取り、`pantry` が部屋鍵で復号する。
4. room アセットのメタデータは `/` の `asset::<id>`、拡張機能は `extension::<id>` に置き、cellar の `tinId` だけを共有する。

### エクスポート

1. `maitre-d` の `GET /api/room/<roomId>/export/zip?type=full|logs` は `rooms` を引き、未登録は 404、非メンバーかつ `done=false` は 403 で拒否する。空の部屋を暗黙作成しない。
2. 許可された場合、`banquette` に `ExportRoom` を送る。bridge が未起動なら作成者 / DB GM を admin として起動し、`room_heads` の head から部屋を復元する。復元できない場合は 5xx を返し、空の ZIP を返さない。
3. banquette は権限のある scene と log chunk を `RoomData` にまとめて返す。
4. `maitre-d` は `alhalte-kitchen` で ZIP を組み立てて返す。
5. `alhalte-kitchen` CLI は ZIP / tar.gz を読み、`RoomGraph` を作り、HTML5 または EPUB に変換する。

### 起動

1. `alhalte-bistrot` が `Bistrot.toml` を読み、rops で秘密情報を復号する。
2. 各モジュールの引数と systemd unit を純粋関数で生成する。
3. `nix run .#bistrot` はビルド、unit 登録、maitre-d / cellar / argenterie の起動、ブラウザ起動を行う。
4. `banquette` は `maitre-d` が、`rooms` に登録済みの部屋を最初に WS / export で参照したときに部屋ごとに起動する。


## 横断的規約

### tin と label

すべての永続・通信データは tin を基本単位とする。完全なバイト表現、暗号方式、圧縮パラメータ、tin id の導出は [tin-rs](crates/tin-rs/ARCHITECTURE.md) を参照。

- `label = XXH64(identifier, seed = 0)`。`0` は `need not know`。
- 複数フィールドを持つ payload は、別段の定めがない限り postcard（serde）で直列化する。
- モジュール固有のラベルは各モジュール文書に列挙する。

### Generast

チャット本文は `Generast` の木で表す。ラベル定義、ダイス構文木、判定木は [generast-rs](crates/generast-rs/ARCHITECTURE.md) を参照。

- ルートの `alhalte/generast` 以外は暗号化しないことが推奨される。
- 拡張機能の入出力は `(Generast, speaker_id)` を postcard 直列化した Tin である。
- ダイスは seed から決定論的に評価する。同じ seed・同じ式からは常に同じ出目になる。

### 権限

権限の正は `banquette` の `RoomData` にある。`RoomPermissionLevel` は `Denied` / `Viewer` / `Participant` / `Admin` の 4 段階で、`RoomData.permissions`（account ごとの上書き）と `RoomData.default_permission`（未登録 account の既定）が全 scene の read / write を決める。`Room::validate` が「Admin が最低 1 人」「`accessible_scenes` は実在 scene のみ」「書けない account に scene を与えない」を load 後と権限変更後に検証する。旧 `done` フラグと `std::admins` は撤去し、旧 schema の `RoomData` は decode 失敗として起動を止める（空部屋にしない）。

- `Denied`: 読み書き不可。`Viewer`: 全 scene を読めるが書けない。`Participant`: 管理者に移動させられた scene と `/` を読み書きできる。`Admin`: 全 scene の読み書きと権限変更。
- 全 mutate frame（log / state / scene 構造 / character CRUD / 移動）は `banquette` 自身が権限表で拒否する。`maitre-d` の viewer 遮断に依存しない。
- `maitre-d` は SQLite の `rooms(room_id, name, creator_account, created_at, done)` と `room_permissions(room_id, account_id, role)` をまだ持ち、WS 接続時の入場可否と viewer の mutate 遮断を行う。ただし #199 以降、DB の role は実行中の `banquette` へ push されない（`SetAdmins` は撤去）。`banquette` は `SetMemberPermission` / `SetDefaultPermission` / `QueryPermissions` で自身の権限表を更新し、`QueryAccountPermission` で `maitre-d` が問い合わせる。DB role と部屋権限表の接続は Issue #200 で行う。
- 部屋状態の永続 head は `room_heads` が持ち、`rooms.done` は `default_permission` の射影（pre-spawn gate 用の暫定値）、内部書き込みの認可は `cellar.service-token` が担う。

### エラー・安全規約

- Rust edition 2024。`unsafe` は FFI 境界（WASM ABI）以外で禁止。
- 公開 API に rustdoc。ライブラリ内で `unwrap` / `expect` / `panic!` を使わない（テストは可）。
- エラーは `thiserror` で型付けする。
- 外部ネットワークに依存するテストを書かない。

### 内部資格情報

ブラウザ以外の内部書き込み（banquette の部屋状態 persistence）は、ユーザー session を詐称せず `cellar.service-token` を使う。

- `Bistrot.toml` の `[cellar] service-token`（age armor で値単位暗号化。rops 全体暗号化も後方互換で可）を `maitre-d` と `cellar` に渡す。
- リポジトリ同梱の `Bistrot.toml` はローカル開発専用の平文既定値 `service-token = "alhalte-local-service-token"` を設定し、`maitre-d` / 各 banquette / `cellar` が同じ値を使う（平文 secret は `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` のときだけ受理）。本番は age armor の値に必ず差し替える（`Bistrot.example.toml` / gitignore 済み `Bistrot.local.toml`）。
- Bistrot は復号した `client-secret` / `service-token` を `<data-dir>/secrets/` に 0600 で書き、引数には `--client-secret-file` / `--service-token-file` の path だけを渡す。`maitre-d` は spawn する banquette に `--service-token-file` を伝播し、banquette は同じファイルから読んだトークンを `X-Alhalte-Service-Token` で cellar に送る。
- cellar は提示されたトークンを定数時間比較し、一致すれば内部書き込みを許可する。ユーザー session の検証は公開 proxy / 通常クライアント経路で維持する。
- トークンと OAuth client secret はログにも `ps` にも systemd unit にも出さない。`--service-token` / `--client-secret` の argv 指定は後方互換のため受理するが、Bistrot は生成しない。

## 拡張機能

拡張機能は `process_generast` をエクスポートする wasm で、有効なものを優先度順に適用する。共通 ABI、入出力、配布方法は [extensions/ARCHITECTURE.md](extensions/ARCHITECTURE.md) を参照。

公式拡張機能:

| 拡張機能 | 責務 | 文書 |
| --- | --- | --- |
| alhalte-dsl | ダイス記法、能力値変動・代入、メンション | [extensions/alhalte-dsl/ARCHITECTURE.md](extensions/alhalte-dsl/ARCHITECTURE.md) |
| aozora | 青空文庫記法（ルビ、傍点、見出し等） | [extensions/aozora/ARCHITECTURE.md](extensions/aozora/ARCHITECTURE.md) |
| markdown | Markdown 風装飾 | [extensions/markdown/ARCHITECTURE.md](extensions/markdown/ARCHITECTURE.md) |
| coc-6 | CoC 6 版判定 | [extensions/coc-6/ARCHITECTURE.md](extensions/coc-6/ARCHITECTURE.md) |
| coc-7 | CoC 7 版判定 | [extensions/coc-7/ARCHITECTURE.md](extensions/coc-7/ARCHITECTURE.md) |
| emoklore | エモクロア判定 | [extensions/emoklore/ARCHITECTURE.md](extensions/emoklore/ARCHITECTURE.md) |

## 実装と仕様の差分（要確認）

設計時の記述と現在の実装が異なる主な点を列挙する。Issue #199 で `banquette` の 4 段階権限（`Denied` / `Viewer` / `Participant` / `Admin`）が main に入り、`maitre-d` の DB role 連携は Issue #200 で置き換える。各項目の詳細はリンク先のモジュール文書を参照。

| 領域 | 設計時の記述 | 現在の実装 |
| --- | --- | --- |
| 権限 | 部屋の `done` フラグと `std::admins` | `banquette` の `RoomData.permissions` / `default_permission` が正（4 段階）。`maitre-d` の SQLite `gm/pl/viewer` と `rooms.done` は pre-spawn gate 用の暫定投影で、実行中の部屋へは push しない（#200 で接続）。 |
| 部屋状態 | 部屋プロセスは揮発 | `maitre-d` の `room_heads` と cellar の `alhalte/banquette` Tin で永続化し、spawn 時に復元する。復元失敗は空部屋にフォールバックしない。 |
| 内部書き込み | ユーザー session で認可 | `cellar.service-token` による内部認証。合成 session（旧 `system_session_id = 1`）は廃止。 |
| 拡張 ABI | `process-generast(Generast) -> Generast` | `process_generast(ptr, len) -> i64`。`(Generast, speaker_id)` の Tin を入出力し、`alhalte_abi_version` / `alloc` / `dealloc` / `self_introduction` を持つ。 |
| canner | クライアント側 wasm の `encode-*` / `encrypt-*` | Rust ライブラリとして実装済み。WASM ABI は未エクスポート。ブラウザでは `pantry` の TS 実装が暗号・展開を担う。 |
| アセット | `pantry` が cellar から取得・復号・キャッシュ | `argenterie` は `pantry` の `encodeTin` で RoomKey 暗号化した Tin を maitre-d の `/api/cellar/*` proxy 経由で cellar へ保存し、進捗・失敗・再試行をカードに出す。room アセットのメタデータは `/` の state、personal は localStorage のみ。拡張 wasm も同じ経路で `tinId` だけを state に持つ。 |
| kitchen の room key | ランダム 96bit nonce | `export_room_to_zip` / `export_room_to_tar_gz` は Zero Nonce で room-key 暗号化する。 |
| saignant の WASM 実行 | キャラクター名を解決 | ABI v2 の `render_generast_abi_v2(ptr, len, names_ptr, names_len)` が名前 JSON を受け取り、mention / `fluct` / `assign` を解決する。v1 は互換のため残し、名前なしでは `Anonymous` になる。 |
| Markdown | 打ち消し線 `~~` を含む | パーサは `~~` を実装していない（自己紹介文には記載がある）。 |

- `rooms.done` は `RoomPersisted.default_permission` と `ServerFrame::PermissionsUpdated` から射影した暫定フラグで、WS / `GET /api/rooms/{id}` / export の pre-spawn 判定に使う。権限の正は `RoomData` 側にあり、#200 でゲートごと置き換える（`RoomData.done` という field は #199 で消滅した）。
- standalone の `alhalte-banquette` は SIGTERM / SIGINT を捕捉し、`run_until_shutdown` と同じ graceful shutdown 経路で最終 persist を行う（`crates/alhalte-banquette/tests/spec_tests.rs` の SIGTERM 統合テストで確認）。
| ディレクトリ名 | `generast-rs` | package 名は `generast`。文書内ではディレクトリ名と package 名を併記する。 |

## 開発

```bash
nix develop -c cargo fmt --all
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
nix develop -c bash -c 'cd web && pnpm install && pnpm -r test'
```

ローカル起動:

```bash
nix run .#bistrot -- --config Bistrot.toml
```

`Bistrot.toml` の例はリポジトリルートにあり、`restaurant`、`maitre-d`、`cellar`、`banquette`、`argenterie`、`[[extensions]]` を持つ。各設定値と生成される引数・unit は [alhalte-bistrot](crates/alhalte-bistrot/ARCHITECTURE.md) を参照。

## ドキュメント運用

- ルート `ARCHITECTURE.md` は全体図・モジュール索引・横断規約・既知差分だけを持つ。
- 各モジュールの `ARCHITECTURE.md` は「責務」「公開 API / 外部接続点」「内部フロー」「データ形式」「既知の差分・制約」を持つ。
- 関数内部の分岐、エラー型の全列挙、テストケースの説明はコード内の rustdoc / コメントへ置く。
- 既存コードに残る `ARCHITECTURE.md` の節参照は、該当モジュールの `ARCHITECTURE.md` を指すものとして読み替える。新規の参照はモジュール文書のパスを書く。
- 仕様判断を変えた場合は、該当モジュール文書の「既知の差分」と PR 本文の「判断した点」を更新する。
