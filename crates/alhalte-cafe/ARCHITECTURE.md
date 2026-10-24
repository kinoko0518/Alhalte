# alhalte-cafe

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-cafe`  
> テスト: `crates/alhalte-cafe/tests/spec_tests.rs`

## 責務

systemd を使用せず、Linux / macOS / Windows のローカル環境で Alhalte インスタンス全体をワンショット起動するランチャーである。`nix run .` による開発・試用起動、および配布用ポータブルバンドル（`alhalte-cafe` / `alhalte-cafe.exe` のダブルクリック起動）のエントリポイントを担う。

具体的には、`alhalte-restaurant` を用いて `Restaurant.toml` の検証・秘密値復号・拡張機能 WASM のステージングを行った上で、`alhalte-cellar` と `alhalte-maitre-d` を子プロセスとして直接起動・監視し、終了時に逆順で停止する。

## 公開 API / 外部接続点

### CLI（`alhalte-cafe` バイナリ）

| 引数 / サブコマンド | 既定値 | 内容 |
| --- | --- | --- |
| `--config <path>` | カレントディレクトリまたは実行ファイル隣接の `Restaurant.toml` | 読み込む設定ファイルのパス。 |
| `--age-key <identity>` | `ROPS_AGE` / `ALHALTE_AGE_KEY` 環境変数 | age armor / rops 復号用の age 秘密鍵。 |
| `--bin-dir <dir>` | 実行ファイルと同じディレクトリ（無ければ `PATH`） | `alhalte-cellar` / `alhalte-maitre-d` / `alhalte-banquette` を探索するディレクトリ。 |
| `--web-root <dir>` | 実行ファイル隣接の `web` または `web/alhalte-argenterie/dist-app` | `maitre-d` に `--web-root` として渡すビルド済み SPA ディレクトリ。 |
| `--no-browser` | `false` | 起動完了時に既定ブラウザを自動で開かない。 |
| `up`（省略時の既定サブコマンド） | — | `cellar` と `maitre-d` を起動し、シグナルまたは子プロセス異常終了まで待機する。 |
| `stage-extensions --output-dir <dir>` | — | サーバーを起動せず、`Restaurant.toml` の `[[extensions]]` を検証して指定ディレクトリに `.wasm` と `manifest.json` を書き出す。 |
| `encrypt-value --recipient <age1...>` | — | 標準入力から読んだ平文シークレットを age armor に暗号化して標準出力へ書き出す。 |

### ライブラリ API（`alhalte_cafe`）

- `prepare_launch(config_path, age_key, bin_dir, web_root) -> Result<LaunchPlan>`: 設定読み込み、ディレクトリ作成、シークレットファイル書き出し（`<data-dir>/secrets/`）、拡張機能ステージング（`<data-dir>/extensions/`）、および `cellar` / `maitre-d` の起動コマンド構築（`--max-tin-size` 等の伝播を含む）を一括で行う。
- `ChildSupervisor`: 起動した `alhalte-cellar` と `alhalte-maitre-d` の子プロセスを保持し、終了監視と逆順停止（`maitre-d` → `cellar`）を担う。

## データ形式

`alhalte-cafe` 固有の永続データ形式は持たない。設定ファイルの構造・バリデーション規則・シークレットファイル形式はすべて [`alhalte-restaurant`](../alhalte-restaurant/ARCHITECTURE.md) に従う。

- **ローカル開発時の平文シークレット許容**: `Restaurant.toml` で `[maitre-d] local-login = true` が有効な場合、または `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` が設定されている場合に限り、ローカル即起動の利便性のため平文の `service-token` / `client-secret` を受理する。Google OAuth モード（`local-login = false`）では既定で age armor 暗号化を必須とする。

## 内部フロー

### ワンショット起動フロー（`alhalte-cafe [up]`）

1. **パスの解決**:
   - `--config` が省略された場合、カレントディレクトリの `Restaurant.toml`、続いて `alhalte-cafe` 実行ファイルと同じディレクトリの `Restaurant.toml` を探索する（ポータブル版を任意の作業ディレクトリやエクスプローラーからダブルクリック起動できるようにするため）。
   - 相対パスの `data-dir`、`socket-dir`、`[[extensions]].path` はすべて `Restaurant.toml` が置かれたディレクトリを基準に解決する。
2. **準備 (`prepare_launch`)**:
   - `alhalte-restaurant` で `Restaurant.toml` を読み込み、検証する。
   - `data-dir` および `socket-dir` を作成し、復号した秘密値を `<data-dir>/secrets/`（権限 `0600`）に書き出す。
   - `[[extensions]]` の WASM ファイルを検証し、`<data-dir>/extensions/` に `.wasm` と `manifest.json` をステージングする。
   - `alhalte-restaurant` の引数生成関数を用いて、`alhalte-cellar` と `alhalte-maitre-d` の起動引数を構築する。`[cellar] max-tin-size`（既定 64 MiB）は両方のプロセスに `--max-tin-size` として渡され、さらに `maitre-d` に `--banquette-bin`、`--web-root`、`--extensions-dir` を付与する（Issue #299）。
3. **プロセス起動とヘルスチェック**:
   - 最初に `alhalte-cellar` を起動し、`127.0.0.1:<cellar.port>` への TCP 接続が可能になるまで待機する（起動直後にクラッシュした場合は即座にエラー終了）。
   - 次に `alhalte-maitre-d` を起動し、`<maitre-d.port>` への TCP 接続が可能になるまで待機する。
   - `--no-browser` が指定されていなければ、OS 標準のブラウザ起動コマンド（Linux: `xdg-open`、macOS: `open`、Windows: `cmd /C start`）で公開 URL（`0.0.0.0` バインド時は `http://localhost:<port>`）を開く。
4. **監視とシャットダウン（Issue #291, #292）**:
   - `SIGINT`（Ctrl+C）/ `SIGTERM`（Unix）の受信、または `cellar` / `maitre-d` いずれかの予期しない終了を待機する。
   - シャットダウン時は **`maitre-d` → `cellar` の逆順**で停止する。`maitre-d` は停止時にアクティブな各 `banquette` へ `ShutdownRoom` フレームを送信し、`banquette` がその時点の最新状態を起動中の `cellar` へ永続化して終了・reap された後に `cellar` を停止する。これにより、Unix と Windows の双方で部屋の状態を失わずに安全に終了できる。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- `alhalte-cellar`、`alhalte-maitre-d`、`alhalte-banquette` のいずれかのバイナリが見つからない場合や、設定・拡張機能の検証に失敗した場合は、子プロセスを起動する前にエラーで終了する。
- 片方のサーバープロセスが異常終了した場合、残りのプロセスも即座に停止・reap して非ゼロ終了コードを返す。

## 既知の差分・制約

- `alhalte-cafe` 自身はデーモン化や自動再起動を行わない。常駐サーバー運用には `alhalte-bistrot`（systemd ユニット生成）または `alhalte-glacier`（NixOS モジュール）を使用する。
- ブラウザの自動起動はベストエフォートであり、ヘッドレス環境などでブラウザ起動コマンドが失敗してもサーバー自体はそのまま稼働を継続する。
