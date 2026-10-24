# Alhalte

**シーンを第一級概念**として扱うセルフホスト型 VTT（Virtual Tabletop）モジュール群。ccfolia などの既存ツールとは異なり、シーンを PL や GM の運用に委ねる仮想的な概念ではなく、システム上の独立した実体として管理する。

本文書はリポジトリ全体の見取り図である。各モジュールの責務、公開 API、外部接続点、内部フロー、および実装上の制約については、各ディレクトリの `ARCHITECTURE.md` を正とする。より低レイヤの詳細（関数単位の分岐、エラー型、テストケースなど）はコード内の rustdoc およびコメントに委ねる。

```text
全体構成   = restaurant
配布構成   = franchise
client 総体 = cutlery
```

## 用語

| 用語 | 意味 |
| --- | --- |
| restaurant | 1 つの Alhalte インスタンス全体。`Restaurant.toml` で設定・構成する。 |
| franchise | restaurant から配布される構成単位。 |
| cutlery | クライアント側モジュール群の総称。 |
| tin | ラベル付き・自己記述的なバイナリコンテナ。平文の中身を表す `TinContent`（`label` + `payload`）と、Opener（暗号化・圧縮方式・鍵情報）・`TinId`・エンコード済みバイト列を保持する自己完結型の `Tin` からなる。 |
| generast | チャット本文を表す汎用構文木。文字列、要素（`TinContent`）、およびネストした装飾ノードで構成される。 |
| scene | 独立したログと state を持つ空間。ルート仮想シーンは `/` で表す。 |
| room | `banquette` プロセスがホストするセッション空間。内部に複数の scene を持つ。 |
| label | `TinContent` の payload 形式を表す 64bit 値。識別子文字列の XXH64 ハッシュ（seed 0）。 |

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
                         └───────┬──────────────────────┘
                                 │ local IPC
                                 │ (UDS / named pipe)
                                 │ postcard
                                 ▼
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

  pantry (browser) ── HTTP GET/POST /api/cellar/* ──▶ maitre-d ──▶ cellar
  alhalte-cafe ────── Restaurant.toml ──▶ cellar / maitre-d を直接起動（systemd 非依存）
  alhalte-bistrot ─── Restaurant.toml ──▶ systemd unit 生成
  alhalte-glacier ─── NixOS options ────▶ systemd service（常駐）
```

### レイヤ構成

- **共有ライブラリ**: `tin`、`generast`。全モジュールが共通のバイナリ表現（`Tin` / `TinContent`）と構文木（`Generast`）を使用する。
- **サーバー**: `alhalte-maitre-d`（認証・HTTP/WS ゲートウェイ）、`alhalte-banquette`（部屋プロセス）、`alhalte-cellar`（重複排除ストレージ）。
- **クライアント**: `web/alhalte-argenterie`（部屋 UI・拡張機能ホスト）、`web/alhalte-pantry`（クライアント側のローカル状態管理：シーン単位の State 管理・Log 管理、および cellar 通信・WebP 1920px / Opus 128kbps 変換・再送管理を担う Tin 管理）、`alhalte-saignant`（HTML レンダラ、ネイティブおよび WASM で動作）。
- **変換・拡張**: `alhalte-canner`（画像・音声の決定論的再エンコード・暗号化）、`extensions/*`（構文木変換 WASM モジュール群）。なお、`alhalte-kitchen` は `kitchen-freeze` ブランチに保存し、`main` ブランチからは削除されている。
- **運用・配布**: `alhalte-restaurant`（設定モデル・引数生成・秘密情報管理）、`alhalte-cafe`（ワンショット起動）、`alhalte-bistrot`（systemd ユニット生成）、`alhalte-glacier`（NixOS モジュール）。

## モジュール一覧

| モジュール | 実装パス | 責務 | 文書 |
| --- | --- | --- | --- |
| tin-rs | `crates/tin-rs`（package `tin`） | 自己記述バイナリコンテナ（`Tin` / `TinContent`）、暗号・圧縮・`TinId` 導出 | [crates/tin-rs/ARCHITECTURE.md](crates/tin-rs/ARCHITECTURE.md) |
| generast-rs | `crates/generast-rs`（package `generast`） | 汎用構文木、ラベル定義、決定論的ダイス評価、判定木 | [crates/generast-rs/ARCHITECTURE.md](crates/generast-rs/ARCHITECTURE.md) |
| alhalte-banquette | `crates/alhalte-banquette` | 部屋プロセス、scene、log chunk、state、character 管理 | [crates/alhalte-banquette/ARCHITECTURE.md](crates/alhalte-banquette/ARCHITECTURE.md) |
| alhalte-maitre-d | `crates/alhalte-maitre-d` | 認証（Google OAuth / local-login）、account/session、HTTP/WS 玄関口、IPC 中継・子プロセス管理 | [crates/alhalte-maitre-d/ARCHITECTURE.md](crates/alhalte-maitre-d/ARCHITECTURE.md) |
| alhalte-cellar | `crates/alhalte-cellar` | Tin 実体の重複排除保存、参照カウント・alias 管理、GC | [crates/alhalte-cellar/ARCHITECTURE.md](crates/alhalte-cellar/ARCHITECTURE.md) |
| alhalte-canner | `crates/alhalte-canner` | 画像・音声の決定論的再エンコード、高低エントロピー暗号化 | [crates/alhalte-canner/ARCHITECTURE.md](crates/alhalte-canner/ARCHITECTURE.md) |
| alhalte-saignant | `crates/alhalte-saignant` | ログおよび `Generast` の HTML レンダリング、CSS 生成、WASM ABI | [crates/alhalte-saignant/ARCHITECTURE.md](crates/alhalte-saignant/ARCHITECTURE.md) |
| alhalte-restaurant | `crates/alhalte-restaurant` | `Restaurant.toml` の設定モデル、age/rops 復号、秘密ファイル展開、引数生成、拡張機能 staging | [crates/alhalte-restaurant/ARCHITECTURE.md](crates/alhalte-restaurant/ARCHITECTURE.md) |
| alhalte-cafe | `crates/alhalte-cafe` | ワンショット起動（Linux/macOS/Windows）、SPA 配信、拡張機能 staging | [crates/alhalte-cafe/ARCHITECTURE.md](crates/alhalte-cafe/ARCHITECTURE.md) |
| alhalte-bistrot | `crates/alhalte-bistrot` | `Restaurant.toml` から systemd ユニットへの純粋関数的変換 | [crates/alhalte-bistrot/ARCHITECTURE.md](crates/alhalte-bistrot/ARCHITECTURE.md) |
| alhalte-glacier | `nix/alhalte-glacier`（flake `nixosModules.default`） | NixOS 常駐サービスモジュール（`mk*Option`） | [nix/alhalte-glacier/ARCHITECTURE.md](nix/alhalte-glacier/ARCHITECTURE.md) |
| alhalte-pantry | `web/alhalte-pantry` | クライアント側ローカル状態管理（シーン別 State / Log 管理、cellar 通信・メディア変換・再送を伴う Tin 管理） | [web/alhalte-pantry/ARCHITECTURE.md](web/alhalte-pantry/ARCHITECTURE.md) |
| alhalte-argenterie | `web/alhalte-argenterie` | Svelte 製クライアント UI（cutlery）、部屋画面、ccfolia 駒互換読み込み、拡張機能ホスト | [web/alhalte-argenterie/ARCHITECTURE.md](web/alhalte-argenterie/ARCHITECTURE.md) |
| 拡張機能 | `extensions/*` | `process_generast` をエクスポートする WASM モジュール群 | [extensions/ARCHITECTURE.md](extensions/ARCHITECTURE.md) |

## 主要なデータフロー

### 認証・サインイン

1. `argenterie` は認証方式（Google OAuth / local-login）に依存しない統一インターフェースとして `beginSignIn` および `completePendingSignIn` を用いてサインインの開始・完了を処理する（#294）。
2. `maitre-d` は検証後にセッションを発行し、以降の HTTP / WebSocket 通信で認証状態を維持する。

### 部屋作成

1. クライアントはセッションを伴って `maitre-d` の `POST /api/rooms` に `{ room_id?, name }` を送信する。`room_id` を省略した場合はサーバー側で 16 桁の hex 文字列を生成する。
2. `maitre-d` は SQLite の `rooms` テーブルに部屋を登録する。`maitre-d` 自身は権限テーブルを持たず、作成者アカウントは `banquette` 起動時の初期 Admin 指定にのみ使用される。この時点では `banquette` プロセスを起動しない。
3. `GET /api/rooms/{room_id}` は部屋名と呼び出し元の権限を返す。権限は `banquette` へ `QueryAccountPermission` を送って問い合わせる。部屋は WebSocket 接続またはエクスポート要求で参照されるまでプロセスを持たない。

### 入室とプロセス管理

1. ブラウザは部屋 URL `/rooms/<roomId>#<roomKey>` を開く。`roomKey` は URL フラグメントにのみ保持され、サーバーへは送信されない。
2. `argenterie` は `maitre-d` の `/ws/room/<roomId>?account_id=...&session_id=...` へ WebSocket 接続を開始する。
3. `maitre-d` はセッションを検証し、`rooms` に登録済みかを確認する。未登録なら 404、権限が `Denied` なら 403 で拒否する。
4. 許可された接続は、ローカル IPC（Unix 系は UNIX ドメインソケット、Windows は名前付きパイプ）経由で `banquette` へ `ClientConnected` フレームとして中継される。ブリッジの `admin_account` には接続順によらず部屋作成者（`rooms.creator_account`）が設定される。
5. `banquette` が未起動の場合は `maitre-d` の `ProcessBanquetteSpawner` が子プロセスを spawn する。`ProcessBanquetteSpawner` は起動した `banquette` 子プロセスを追跡・reap し、終了時や再起動時に古いプロセスやソケットが競合しないようライフサイクルを管理する（#292）。
6. 起動した `banquette` は `cellar` の alias `room/<room_id>/head` から部屋状態を自律的に復元する（`maitre-d` は head を管理しない）。alias が存在するにもかかわらず復元に失敗した場合は 500 エラーとし、空の部屋で上書きしない。alias が未作成の場合のみ新規の部屋として初期化する。
7. `banquette` は接続したクライアントをルートシーン `/` に入室させ、`pantry` が初期のシーン State やログを同期する。

### シーン State・キャラクター・拡張機能の同期

1. クライアント側のローカル状態は `@alhalte/pantry` が一元管理する。`pantry` は単なる Tin キャッシュではなく、シーン単位の State 管理、シーン単位の Log 管理、および Tin 管理（`cellar` との送受信、画像の WebP 1920px 変換・音声の Opus 128kbps 変換、失敗時の再送管理）を担う（#302）。
2. 拡張機能の有効/無効状態（`extension::<id>` state）や各種シーン state の更新は、`banquette` から WebSocket 経由で配信され、`pantry` を通じて即座に全参加者の UI へリアクティブに同期・反映される（#301）。
3. `argenterie` のキャラクター管理タブでは、新規作成ボタンの横に「ccfolia 駒互換読み込み」ボタンを備え、クリップボード等の ccfolia 駒データを解釈してキャラクターとして即座に取り込める（#300）。

### 部屋の永続化と Graceful Shutdown

1. `banquette` は heartbeat（既定 4 分間隔）ごとに部屋全体の状態を `alhalte/banquette` ラベルの Tin として `cellar` に保存する。
2. 保存に成功すると、`cellar` の alias `room/<room_id>/head` を新しい Tin へ更新する。この alias が唯一の head ポインタであり、`maitre-d` 側には head を記録しない（#192）。
3. alias の更新後、`banquette` は旧 head に対する参照を best-effort で unregister し、`cellar` の GC が不要な実体を回収できるようにする。`cellar` の GC は `tin_references` と `aliases` の双方を有効な参照として扱う。
4. アイドルタイムアウト、Unix シグナル（SIGTERM / SIGINT）、および `maitre-d` から送信されるローカル IPC 制御フレーム（`ShutdownRoom`）のいずれによっても `banquette` の graceful shutdown がトリガーされる。これにより、Windows を含む全環境で終了直前に最終 persist が確実に実行される（#291）。
5. 次回 spawn 時には alias の head から scenes / state / characters / log biblio / 権限（`default_permission` / `permissions` / `accessible_scenes`）を復元する。破損した Tin やハッシュ不一致、`room_id` 不一致が検出された場合は起動失敗とし、空の部屋へのフォールバックは行わない。

### 発言

1. `argenterie` は入力テキストを `Generast::String` として保持する。
2. プレビューおよび送信時には、有効な拡張機能を優先度昇順に適用し、`(Generast, speaker_id)` を格納した `TinContent` / `Tin` を左畳み込みで変換する。
3. 最終的な構文木と現在のキャラクター名マップを `saignant` の `render_generast_abi_v2` に渡して HTML を生成する。
4. 送信時は `AppendLog` フレームを `banquette` へ送る。`banquette` は対象シーンのログに追記し、同一シーン内のクライアントへ `LogEventAppended` を配信する。
5. ログチャンクが上限に達すると `alhalte/banquette/log/chunk` ラベルの Tin に封入し、`cellar` へ永続化する。

### アセット管理

1. クライアントは画像・音声などのアセットを登録する際、`pantry` 内で決定論的変換（WebP 最大 1920px / Opus 128kbps）を適用した上で、`RoomKey` による暗号化と圧縮を行って Tin を構築する（#302）。
2. `maitre-d` の `/api/cellar/store` へ保存可否を問い合わせ、ステータス `210` が返った場合は `/api/cellar/store/{tinId}` で本体とラップ鍵（`X-Alhalte-Wrapped-Key`）を送信する。owner 情報は `maitre-d` がセッションから付与する。
3. 取得時は `/api/cellar/tin/{tinId}` から本体と `X-Alhalte-Wrapped-Key` を受け取り、`pantry` が部屋の鍵で復号する。BGM や各種アセットを `RoomKey` 暗号化 Tin として保存・取得する際、直接暗号化と収束暗号＋ラップ鍵（`X-Alhalte-Wrapped-Key`）の経路および鍵の整合性を厳密に保つことで、復号時の `invalid polyval tag` エラーを防止する（#298）。
4. 部屋共有アセットのメタデータはルートシーン `/` の `asset::<id>`、拡張機能は `extension::<id>` の state に配置し、`cellar` 上の `tinId` のみを共有する。

### エクスポート

1. `maitre-d` の `GET /api/room/<roomId>/export/zip?type=full|logs` は `rooms` テーブルを参照し、未登録なら 404、呼び出し元の権限が `Denied` なら 403 で拒否する。存在しない部屋を暗黙に作成することはない。
2. 認可された場合、`banquette` に `ExportRoom` を送る。プロセスが未起動なら部屋作成者を初期 Admin として起動し、`cellar` の alias `room/<room_id>/head` から部屋状態を復元した上で応答する（#193）。復元に失敗した場合は 5xx エラーを返し、空の ZIP を返さない。
3. `banquette` は呼び出し元がアクセス権を持つ scene の `RoomData` と log chunk を収集し、`room.tin` と `chunks/<tin-id>.tin` を含む ZIP バイト列を生成して返す。`type=logs` が指定された場合は、ZIP 化の前に全 scene の state を空にする。
4. `maitre-d` は受け取った ZIP バイト列に `application/zip` と `Content-Disposition` ヘッダを付与してそのままレスポンスとして返す。
5. HTML5 や EPUB への変換は `kitchen-freeze` ブランチの `alhalte-kitchen` が担い、`main` ブランチのサーバー経路は ZIP 出力までを責務とする。

### 起動と設定伝播

1. `alhalte-restaurant` が `Restaurant.toml` を読み込み、age armor または rops で暗号化された秘密情報を復号・検証する。
2. 各モジュールのコマンドライン引数は `alhalte-restaurant` の純粋関数で生成する。最大 Tin サイズ（`max-tin-size`）は `Restaurant.toml`（`alhalte-restaurant`）、各バイナリの CLI 引数（`banquette`、`cellar`、`maitre-d`、`cafe`、`bistrot`）、および `flake.nix` / `alhalte-glacier`（NixOS module）のすべてから一貫して設定・伝播できる（#299）。
3. `alhalte-cafe` は systemd を使わずに `cellar` → `maitre-d`（→ `banquette` は部屋ごとに on-demand spawn）を直接起動し、ビルド済み SPA を `maitre-d` の `--web-root` 経由で配信する。ポータブル版は `alhalte-cafe`（Windows では `alhalte-cafe.exe`）の実行だけで起動できる。
4. `alhalte-bistrot` は `Restaurant.toml` から `alhalte-cellar.service`、`alhalte-maitre-d.service`、および `alhalte.target` を生成する。UI は `maitre-d` が配信するため、`argenterie` 用の独立したサービスは生成しない。
5. `alhalte-glacier` は NixOS の `services.alhalte.*` オプションから上記と同じ 2 つの service と target を定義する。秘密情報は `clientSecretFile` / `serviceTokenFile` のファイルパスのみを受け取る。
6. 認証方式は Google OAuth と local-login の二択である。local-login はアカウント名とパスワード（Argon2id）を使用し、データベースを `maitre-d-local.sqlite3` に分離する（Google OAuth 用 DB とは `rooms` を含め共有しない）。部屋状態の `cellar` への永続化はどちらの認証方式でも共通である。

## 横断的規約

### Tin と Label

すべての永続化データおよび通信ペイロードは `tin` を基本単位とする。詳細なワイヤフォーマット、暗号・圧縮方式、`TinId` の導出規則については [tin-rs](crates/tin-rs/ARCHITECTURE.md) を参照。

- 平文の中身は `TinContent { label: Label, payload: Vec<u8> }` で表現する。プリミティブ型や `TinId` との相互変換には Rust 標準の `From` / `TryFrom`（`TryInto`）トレイト実装を用い、平文バイト列 `label(8B, u64 LE) || payload` との変換には `TinContent::to_bytes` / `from_bytes` を使う（#272）。
- エンコード済みの `Tin` は Opener（圧縮方式および暗号化方式・鍵/Nonce 情報）と `TinId`、バイト列を内包する自己完結したコンテナであり、呼び出し側が別途 `Opener` を引き回す必要はない（#272）。
- `label = XXH64(identifier, seed = 0)`。`0` は `need not know` を表す。
- 複数フィールドを持つ構造化 payload は、特段の定めがない限り postcard（serde）で直列化する（`TinContent::from_postcard` / `to_postcard`）。
- モジュール固有のラベルは各モジュールの `ARCHITECTURE.md` に列挙する。

### Generast

チャット本文は汎用構文木 `Generast` で表現する。ラベル定義、ダイス構文木、判定木の詳細については [generast-rs](crates/generast-rs/ARCHITECTURE.md) を参照。

- ルートの `alhalte/generast` 以外の内部ノード（`Nested.meta` や `Element`）は暗号化しないことが推奨される。
- 拡張機能の入出力は `(Generast, speaker_id)` を postcard 直列化した `alhalte/generast` ラベルの Tin（または `TinContent` バイト列）である。
- ダイスは発言者 ID と入力文字列から導出した seed を用いて決定論的に評価する。同一の seed と式からは常に同じ出目が得られる。

### 権限モデル

権限の唯一の正（Single Source of Truth）は `banquette` の `RoomData` にある。`RoomPermissionLevel` は `Denied` / `Viewer` / `Participant` / `Admin` の 4 段階で構成され、`RoomData.permissions`（アカウントごとの個別権限）と `RoomData.default_permission`（未登録アカウントの既定権限）によって全 scene の読み書き可否が決定される。`Room::validate` はロード直後および権限変更時に「Admin が最低 1 人存在すること」「`accessible_scenes` が実在する scene のみを指すこと」「書き込み権限のないアカウントに scene アクセス権を与えないこと」を検証する。旧仕様の `done` フラグや `std::admins` は廃止されており、旧スキーマの `RoomData` はデコード失敗として起動を停止する（空部屋で上書きしない）。

- `Denied`: 読み書き不可。
- `Viewer`: 全 scene を閲覧できるが書き込み不可。
- `Participant`: 管理者によって配置された scene とルートシーン `/` を読み書き可能。
- `Admin`: 全 scene の読み書きおよび権限設定の変更が可能。
- すべての状態変更フレーム（log / state / scene 構造 / character CRUD / シーン移動）は `banquette` 自身が権限表に基づいて認可判定を行う。
- `maitre-d` は SQLite の `rooms(room_id, name, creator_account, created_at)` のみを保持し、権限や head ポインタを持たない。WebSocket 接続、`GET /api/rooms/{id}`、エクスポート時の入場可否はすべて `banquette` へ `QueryAccountPermission` で問い合わせる。

### エラー・安全規約

- Rust edition 2024 を使用する。`unsafe` コードは FFI 境界（WASM ABI）以外では禁止（`#![forbid(unsafe_code)]`）とする。
- 公開 API には rustdoc を記述する。テストコードを除き、ライブラリ内で `unwrap` / `expect` / `panic!` を使用しない。
- エラー型は `thiserror` で定義する。
- 外部ネットワーク通信に依存するテストは記述しない。

### 内部資格情報

ブラウザ以外の内部書き込み（`banquette` による部屋状態の永続化など）では、ユーザーセッションを流用せず `cellar.service-token` を使用する。

- `Restaurant.toml` の `[cellar] service-token`（age armor による値単位暗号化、または後方互換の rops 全体暗号化）を `maitre-d` と `cellar` に渡す。
- リポジトリ同梱の `Restaurant.toml` にはローカル開発専用の平文既定値 `service-token = "alhalte-local-service-token"` が設定されており、`ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` または `cafe` の local-login モードで受理される。本番環境では必ず age armor で暗号化した値に差し替える。
- `alhalte-restaurant` は復号した `client-secret` と `service-token` を `<data-dir>/secrets/` 配下にパーミッション `0600` で書き出し、各プロセスには `--client-secret-file` / `--service-token-file` でファイルパスのみを渡す。`maitre-d` は spawn する `banquette` に `--service-token-file` を引き継ぎ、`banquette` は `X-Alhalte-Service-Token` ヘッダで `cellar` に提示する。
- `cellar` は提示されたトークンを定数時間比較で検証し、一致すれば内部書き込みを許可する。トークンや OAuth クライアントシークレットはログ、`ps` 出力、systemd ユニットファイルのいずれにも露出させない。

## 拡張機能

拡張機能は `process_generast` をエクスポートする WASM モジュールであり、有効化されたものを優先度順にパイプライン実行する。共通 ABI、入出力仕様、配布方法の詳細は [extensions/ARCHITECTURE.md](extensions/ARCHITECTURE.md) を参照。

公式拡張機能一覧:

| 拡張機能 | 責務 | 文書 |
| --- | --- | --- |
| alhalte-dsl | ダイス記法、能力値の変動・代入、メンション解析 | [extensions/alhalte-dsl/ARCHITECTURE.md](extensions/alhalte-dsl/ARCHITECTURE.md) |
| aozora | 青空文庫記法（ルビ、傍点、見出し、字下げ等） | [extensions/aozora/ARCHITECTURE.md](extensions/aozora/ARCHITECTURE.md) |
| markdown | Markdown 風インライン・見出し装飾 | [extensions/markdown/ARCHITECTURE.md](extensions/markdown/ARCHITECTURE.md) |
| coc-6 | クトゥルフ神話 TRPG（第 6 版）判定 | [extensions/coc-6/ARCHITECTURE.md](extensions/coc-6/ARCHITECTURE.md) |
| coc-7 | 新クトゥルフ神話 TRPG（第 7 版）判定 | [extensions/coc-7/ARCHITECTURE.md](extensions/coc-7/ARCHITECTURE.md) |
| emoklore | エモクロア TRPG 判定 | [extensions/emoklore/ARCHITECTURE.md](extensions/emoklore/ARCHITECTURE.md) |

## 初期設計からの主な変更点・実装上の制約

初期設計時の記述と現在の仕様・実装との主な対応関係を以下にまとめる。各項目の詳細はリンク先のモジュール文書を参照。

| 領域 | 初期設計の記述 | 現在の仕様・実装 |
| --- | --- | --- |
| Tin の型表現 | 単一の `Tin { label, payload }` とメソッド群 `from_*` / `as_*` | 平文内容は `TinContent { label, payload }` とし、`From` / `TryFrom` トレイトおよび `to_bytes` / `from_bytes` で変換する。`Tin` は Opener（圧縮・暗号・鍵情報）・`TinId`・バイト列を保持する自己完結型コンテナとし、`TinId` / `ConvergentKey` / `RoomKey` を Newtype として型安全に扱う（#272）。 |
| 権限管理 | 部屋の `done` フラグと `std::admins` | `banquette` の `RoomData.permissions` / `default_permission`（4 段階）を正とする。`maitre-d` は権限テーブルを持たず、`banquette` に問い合わせる（#199 / #200）。 |
| 部屋状態の永続化と終了処理 | 部屋プロセスは揮発、または Unix シグナルのみで終了 | `cellar` の `alhalte/banquette` Tin と alias `room/<room_id>/head` で永続化する（#192）。Unix シグナルに加え `maitre-d` からの IPC 制御フレーム `ShutdownRoom` でも graceful shutdown と最終 persist を行う（#291）。また `ProcessBanquetteSpawner` が子プロセスを追跡・reap する（#292）。 |
| 内部書き込み認可 | ユーザーセッションで認可 | `cellar.service-token` による内部認証を使用する。合成セッション（旧 `system_session_id = 1`）は廃止。 |
| 最大 Tin サイズ設定 | 固定上限または一部モジュールのみ | `max-tin-size` を `Restaurant.toml`（`alhalte-restaurant`）、全関連バイナリ引数（`banquette`, `cellar`, `maitre-d`, `cafe`, `bistrot`）、および `flake.nix` / `alhalte-glacier` から一貫して設定可能（#299）。 |
| クライアント状態・アセット管理 | `pantry` は Tin キャッシュ、`canner` はクライアント WASM | `@alhalte/pantry` がクライアント側のローカル状態管理（シーン別 State / Log 管理、および cellar 通信・WebP 1920px / Opus 128kbps 変換・再送管理を含む Tin 管理）を担う（#302）。RoomKey 暗号化 Tin と `X-Alhalte-Wrapped-Key` の整合性を保ち復号エラーを防ぐ（#298）。拡張機能 ON/OFF やシーン state の変更は `pantry` 経由で即座に全参加者へ同期される（#301）。 |
| クライアント認証・UI 機能 | 認証方式別のサインイン関数 | サインイン開始・完了を認証方式非依存の `beginSignIn` / `completePendingSignIn` に統一（#294）。キャラクター管理タブに「ccfolia 駒互換読み込み」を設置（#300）。 |
| 拡張機能 ABI | `process-generast(Generast) -> Generast` | `process_generast(ptr, len) -> i64`。`(Generast, speaker_id)` の Tin を入出力し、ABI v2 の `self_introduction` は `(id, name, icon_svg, description)` を postcard で返す（ホストは ABI 1/2 両対応）。 |
| saignant WASM ABI | キャラクター名を単一 ABI で解決 | ABI v2 の `render_generast_abi_v2(ptr, len, names_ptr, names_len)` が表示名 JSON を受け取り、mention / `fluct` / `assign` の名前を解決する（v1 は後方互換のため維持）。 |
| Markdown 記法 | 打ち消し線 `~~` を含む | パーサは `~~` を実装していない（自己紹介文には記載あり）。 |
| パッケージ名 | ディレクトリ名 `generast-rs` / `tin-rs` | Cargo package 名はそれぞれ `generast` / `tin`。文書内ではディレクトリ名と package 名を併記する。 |

## 開発コマンド

```bash
nix develop -c cargo fmt --all
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
nix develop -c bash -c 'cd web && pnpm install && pnpm -r test'
```

ローカル起動（引数なしで SPA ビルドから `cafe` 起動まで実行）:

```bash
nix run . -- --no-browser
```

systemd ユニットの生成:

```bash
nix run .#bistrot -- --config Restaurant.toml systemd --web-root "$PWD/web/alhalte-argenterie/dist-app"
```

`Restaurant.toml` の設定例はリポジトリルートにあり、`restaurant`、`maitre-d`、`cellar`、`banquette`、`[[extensions]]` セクションを持つ（`[argenterie]` は廃止）。各設定値と生成される引数・ユニットの詳細は [alhalte-restaurant](crates/alhalte-restaurant/ARCHITECTURE.md) および [alhalte-bistrot](crates/alhalte-bistrot/ARCHITECTURE.md) を参照。Windows / Linux / macOS 向けのポータブル版バンドルは `Restaurant.portable.toml` を使用し、`nix run .#windows-bundle` または `nix run .#portable-bundle` で生成する。NixOS 環境では `nixosModules.default`（`alhalte-glacier`）を使用する。

## ドキュメント運用原則

- ルート `ARCHITECTURE.md` は全体構成図、モジュール索引、主要なデータフロー、横断的規約、および全体に関わる設計変更の要約のみを記載する。
- 各モジュールの `ARCHITECTURE.md` は「責務」「公開 API / 外部接続点」「内部フロー」「データ形式」「既知の差分・制約」を記載する。
- 関数内部の細かい分岐、エラー型の全バリアント列挙、個別テストケースの説明はコード内の rustdoc やコメントに委ねる。
- 仕様や設計判断を変更した場合は、該当モジュールの `ARCHITECTURE.md` を更新して実装との整合性を保つ。
