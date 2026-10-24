# alhalte-bistrot

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-bistrot`  
> テスト: `crates/alhalte-bistrot/tests/spec_tests.rs`

## 責務

`Restaurant.toml` から systemd ユニット定義（`alhalte-cellar.service`、`alhalte-maitre-d.service`、`alhalte.target`）を純粋関数的に生成するデプロイ補助ツールである。設定のパース・検証・age 復号・シークレットファイルの書き出し・引数生成・拡張機能ステージングはすべて [`alhalte-restaurant`](../alhalte-restaurant/ARCHITECTURE.md) に委譲し、`bistrot` 自身は systemd ユニット文字列の組み立てとファイル出力に特化する。

## 公開 API / 外部接続点

### CLI（`alhalte-bistrot` バイナリ）

| 引数 / サブコマンド | 既定値 | 内容 |
| --- | --- | --- |
| `--config <path>` | `Restaurant.toml` | 読み込む設定ファイルのパス。 |
| `--age-key <identity>` | `ROPS_AGE` / `ALHALTE_AGE_KEY` 環境変数 | age armor / rops 復号用の age 秘密鍵。 |
| `validate` | — | 設定の復号とバリデーションのみを行い、結果を報告する。 |
| `systemd` | — | シークレットファイルと拡張機能を書き出し、systemd ユニットファイルを生成する。 |
| `systemd --output-dir <dir>` | 省略時は標準出力 | 生成したユニットファイル（`.service` / `.target`）を書き出すディレクトリ。 |
| `systemd --bin-dir <dir>` | `/usr/local/bin` | `ExecStart` 内の各バイナリ（`alhalte-cellar`、`alhalte-maitre-d`、`alhalte-banquette`）の配置ディレクトリ。 |
| `systemd --user <user>` | `alhalte` | サービスの実行ユーザーおよびグループ（`User=` / `Group=`）。 |
| `systemd --web-root <dir>` | 任意 | `alhalte-maitre-d` に `--web-root` として渡すビルド済み SPA ディレクトリ。 |
| `stage-extensions --output-dir <dir>` | `<data-dir>/extensions` | `[[extensions]]` の WASM 検証と `manifest.json` 書き出しのみを実行する。 |
| `encrypt-value --recipient <age1...>` | — | 標準入力の平文シークレットを age armor に暗号化して標準出力へ書き出す。 |

### ライブラリ API（`alhalte_bistrot::systemd`）

- `generate_units(config, options, secrets) -> SystemdUnits`: 設定と `SystemdOptions`（`bin_dir`, `user`, `web_root`）および `SecretFilePaths` から、`cellar_service`、`maitre_d_service`、`target` の各ユニット文字列を生成する純粋関数。
- `write_units(units, output_dir)`: 生成された 3 つのユニットファイルを指定ディレクトリへ書き出す。

## データ形式

### 生成される systemd ユニット

| ユニット名 | 役割と依存関係 |
| --- | --- |
| `alhalte-cellar.service` | `alhalte-cellar` 常駐サービス。`After=network.target`、`PartOf=alhalte.target`。`ReadWritePaths=<data-dir>` と systemd サンドボックス設定（`NoNewPrivileges=true`、`ProtectSystem=strict`、`ProtectHome=true`、`PrivateTmp=true`）を持つ。 |
| `alhalte-maitre-d.service` | `alhalte-maitre-d` 常駐サービス。`After=network.target alhalte-cellar.service`、`Requires=alhalte-cellar.service`、`PartOf=alhalte.target`。`ReadWritePaths=<data-dir> <socket-dir>` を持ち、必要に応じて `alhalte-banquette` を同一 cgroup 内で子プロセスとして起動する。 |
| `alhalte.target` | インスタンス全体を束ねるターゲットユニット。`Requires=alhalte-cellar.service alhalte-maitre-d.service`、`After=alhalte-cellar.service alhalte-maitre-d.service`。 |

- UI（`alhalte-argenterie`）はビルド済み SPA として `alhalte-maitre-d --web-root` から同一オリジン配信されるため、独立したフロントエンド用 systemd サービスは生成しない。
- `[cellar] max-tin-size`（既定 64 MiB）は `alhalte-restaurant` の引数生成を通じて `alhalte-cellar.service` と `alhalte-maitre-d.service` の双方の `ExecStart=` に `--max-tin-size` として反映され、`maitre-d` から起動される `alhalte-banquette` にも伝播する（Issue #299）。
- `ExecStart=` の各引数は空白や特殊文字を含んでも安全に解釈されるよう systemd 形式でクォート・エスケープされ、秘密値は `--client-secret-file` / `--service-token-file` のファイルパスのみが出力される。

## 内部フロー

### `alhalte-bistrot systemd` の実行手順

1. `alhalte-restaurant::load_config` で `Restaurant.toml` を読み込み、age 秘密鍵で `client-secret` / `service-token` を復号して設定値を検証する。
2. `write_secret_files` で `<data-dir>/secrets/`（権限 `0700`）にシークレットファイル（権限 `0600`）を書き出す。
3. `[[extensions]]` が定義されている場合、`stage_extensions` で WASM バイナリの ABI と自己紹介メタデータを検証し、`<data-dir>/extensions/` に `.wasm` と `manifest.json` を配置する。
4. `generate_units` で `alhalte-cellar.service`、`alhalte-maitre-d.service`、`alhalte.target` の内容を組み立てる。
5. `--output-dir` が指定されていれば各ファイルへ書き出し、未指定であれば標準出力へ出力する。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- 生成されるユニットファイル内には平文の `client-secret` や `service-token` を一切含めない（`systemctl cat` 等で第三者に秘密値が漏洩するのを防ぐため）。
- 設定検証または拡張機能 WASM の検証に失敗した場合はユニットファイルを生成せずエラー終了する（fail-closed）。

## 既知の差分・制約

- `alhalte-bistrot` はユニットファイルとシークレットファイル・拡張機能の配置までを行い、`systemctl daemon-reload` や `systemctl enable --now alhalte.target`、および OS ユーザー（`alhalte`）やデータディレクトリの所有者変更（`chown`）は運用者またはパッケージマネージャ側で実行する前提とする。NixOS 環境でこれらを含めて宣言的に管理する場合は [`alhalte-glacier`](../../nix/alhalte-glacier/ARCHITECTURE.md) を使用する。
