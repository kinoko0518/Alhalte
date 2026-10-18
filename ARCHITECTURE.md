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

### 入室

1. ブラウザは部屋 URL `/rooms/<roomId>#<roomKey>` を開く。`roomKey` はフラグメントにだけ存在し、サーバーへ送らない。
2. `argenterie` は `maitre-d` の `/ws/room/<roomId>?account_id=...&session_id=...` へ WebSocket 接続する。
3. `maitre-d` はセッションを検証し、部屋の権限を確認する。未許可なら 403 で拒否する。
4. 許可された接続は `banquette` の UNIX ソケットへ `ClientConnected` として流し込む。banquette が未起動なら起動し、接続待ちのフレームはキューに積む。
5. banquette はクライアントをルートシーン `/` に入室させる。

### 発言

1. `argenterie` は入力テキストを `Generast::String` として保持する。
2. 表示時に有効な拡張機能を優先度昇順で適用し、`(Generast, speaker_id)` の Tin を左畳み込みする。
3. 最終 Tin を `saignant` の `render_generast_abi` に渡し、HTML を得る。
4. 送信時は `AppendLog` を `banquette` へ送る。banquette は scene log に追記し、同一シーンのクライアントへ `LogEventAppended` を配信する。
5. chunk が満杯になると `alhalte/banquette/log/chunk` Tin に封入し、`cellar` へ保存する。

### アセット

1. クライアントは `pantry` の `encodeTin` で payload を RoomKey 暗号化・圧縮する（`argenterie` の `encodeRoomTin`）。
2. `maitre-d` の `/api/cellar/store` で保存可否を問い合わせ、210 なら `/api/cellar/store/{tinId}` で本体を送る。owner は maitre-d が session から付与する。
3. 取得時は `/api/cellar/tin/{tinId}` で本体と `X-Alhalte-Wrapped-Key` を受け取り、`pantry` が部屋鍵で復号する。
4. room アセットのメタデータは `/` の `asset::<id>`、拡張機能は `extension::<id>` に置き、cellar の `tinId` だけを共有する。

### エクスポート

1. `maitre-d` の `GET /api/room/<roomId>/export/zip?type=full|logs` が `banquette` に `ExportRoom` を送る。
2. banquette は権限のある scene と log chunk を `RoomData` にまとめて返す。
3. `maitre-d` は `alhalte-kitchen` で ZIP を組み立てて返す。
4. `alhalte-kitchen` CLI は ZIP / tar.gz を読み、`RoomGraph` を作り、HTML5 または EPUB に変換する。

### 起動

1. `alhalte-bistrot` が `Bistrot.toml` を読み、rops で秘密情報を復号する。
2. 各モジュールの引数と systemd unit を純粋関数で生成する。
3. `nix run .#bistrot` はビルド、unit 登録、maitre-d / cellar / argenterie の起動、ブラウザ起動を行う。
4. `banquette` は `maitre-d` が部屋ごとに起動する。


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

現行 main では、`maitre-d` が SQLite の `room_permissions(room_id, account_id, role)` に `gm` / `pl` / `viewer` を保持し、WebSocket 接続時の入場可否と viewer の mutate 遮断を行う。`banquette` 側は `done` フラグと `std::admins` による scene アクセス制御を持つ。

Issue #114 で、`maitre-d` は権限を持たず `banquette` に問い合わせる構成へ、`banquette` は `Denied` / `Viewer` / `Participant` / `Admin` の 4 段階へ移行中である。移行の詳細と現時点の未接続点は [banquette](crates/alhalte-banquette/ARCHITECTURE.md) と [maitre-d](crates/alhalte-maitre-d/ARCHITECTURE.md) に記す。

### エラー・安全規約

- Rust edition 2024。`unsafe` は FFI 境界（WASM ABI）以外で禁止。
- 公開 API に rustdoc。ライブラリ内で `unwrap` / `expect` / `panic!` を使わない（テストは可）。
- エラーは `thiserror` で型付けする。
- 外部ネットワークに依存するテストを書かない。

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

設計時の記述と現在の実装が異なる主な点を列挙する。Issue #114 の権限モデル移行は未完了で、現時点の main には反映されていない。各項目の詳細はリンク先のモジュール文書を参照。

| 領域 | 設計時の記述 | 現在の実装 |
| --- | --- | --- |
| 権限 | 部屋の `done` フラグと `std::admins` | `maitre-d` の SQLite `gm/pl/viewer` と `done` を併用。Issue #114 で 4 段階権限へ移行中。 |
| 拡張 ABI | `process-generast(Generast) -> Generast` | `process_generast(ptr, len) -> i64`。`(Generast, speaker_id)` の Tin を入出力し、`alhalte_abi_version` / `alloc` / `dealloc` / `self_introduction` を持つ。 |
| canner | クライアント側 wasm の `encode-*` / `encrypt-*` | Rust ライブラリとして実装済み。WASM ABI は未エクスポート。ブラウザでは `pantry` の TS 実装が暗号・展開を担う。 |
| アセット | `pantry` が cellar から取得・復号・キャッシュ | `argenterie` は `pantry` の `encodeTin` で RoomKey 暗号化した Tin を maitre-d の `/api/cellar/*` proxy 経由で cellar へ保存し、進捗・失敗・再試行をカードに出す。room アセットのメタデータは `/` の state、personal は localStorage のみ。拡張 wasm も同じ経路で `tinId` だけを state に持つ。 |
| kitchen の room key | ランダム 96bit nonce | `export_room_to_zip` / `export_room_to_tar_gz` は Zero Nonce で room-key 暗号化する。 |
| bistrot の banquette 引数 | `--socket <path>` | `generate_banquette_args` は `--socket-dir <dir>` を出力する。banquette CLI は `--socket` を取るため不一致。 |
| saignant の WASM 実行 | キャラクター名を解決 | `render_generast_abi` は `NoopCharacterResolver` を使うため、メンションは `Anonymous` になる。 |
| Markdown | 打ち消し線 `~~` を含む | パーサは `~~` を実装していない（自己紹介文には記載がある）。 |
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
