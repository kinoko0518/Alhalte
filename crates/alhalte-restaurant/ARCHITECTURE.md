# alhalte-restaurant

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-restaurant`  
> テスト: `crates/alhalte-restaurant/tests/spec_tests.rs`

## 責務

インスタンス全体の構成ファイルである `Restaurant.toml` のデータモデル、バリデーション、秘密値（age armor / rops）の復号とシークレットファイル書き出し、各サーバーモジュール（`maitre-d`、`cellar`、`banquette`）の CLI 引数生成、および公式・カスタム拡張機能 WASM のステージングを一元管理するライブラリクレートである。`alhalte-cafe` と `alhalte-bistrot` の双方がこのクレートを共有することで、ローカル起動と systemd 常駐運用の間で設定解釈の齟齬を防ぐ。

## 公開 API / 外部接続点

### 設定モデル・読み込み・検証

| 型 / 関数 | 内容 |
| --- | --- |
| `RestaurantConfig` | `Restaurant.toml` 全体のルート構造体（`restaurant`, `maitre_d`, `cellar`, `banquette`, `extensions`）。 |
| `RestaurantSection` | `[restaurant]` セクション（`domain`, `host`, `data_dir`, `socket_dir`）。 |
| `MaitreDConfig` | `[maitre-d]` セクション（`port`, `client_id`, `client_secret`, `salt`, `public_url`, `local_login`）。 |
| `CellarConfig` | `[cellar]` セクション（`port`, `anti_oracle`, `max_tin_size`, `service_token`）。 |
| `BanquetteConfig` | `[banquette]` セクション（`chunk_size_exponent`, `heartbeat_interval_secs`）。 |
| `ExtensionEntry` | `[[extensions]]` エントリ（`id`, `path`, `enabled`）。 |
| `load_config(path, age_key)` | TOML の読み込み、rops / age armor の復号、およびバリデーションを実行する。 |
| `load_config_with_options(path, age_key, options)` | `LoadOptions { allow_plaintext_secrets }` を指定して設定を読み込む。 |
| `parse_and_validate_toml(content)` | 復号済み TOML 文字列をパースし、設定値の整合性を検証する。 |

### 暗号化・復号・シークレットファイル管理

| 型 / 関数 | 内容 |
| --- | --- |
| `encrypt_secret_value(plaintext, recipient)` | 平文の秘密値を age 公開鍵（`age1...`）で暗号化し、`-----BEGIN AGE ENCRYPTED FILE-----` 形式の age armor 文字列を返す。 |
| `decrypt_secret_value(ciphertext, identity)` | age armor 文字列を秘密鍵（`AGE-SECRET-KEY-1...`）で復号する。 |
| `is_age_armored(value)` | 文字列が age armor 形式かどうかを判定する。 |
| `resolve_age_key(explicit)` | 明示引数 → `ROPS_AGE` 環境変数 → `ALHALTE_AGE_KEY` 環境変数の順で age 秘密鍵を解決する。 |
| `write_secret_files(config)` / `write_secret_files_in(dir, config)` | 復号済みの `client-secret` と `service-token` を `<data-dir>/secrets/` 配下に権限 `0600`（ディレクトリは `0700`）で原子的に書き出し、`SecretFilePaths` を返す。 |

### CLI 引数生成・拡張機能ステージング

| 型 / 関数 | 内容 |
| --- | --- |
| `generate_maitre_d_args_with_secrets(config, secrets)` | `alhalte-maitre-d` 用の CLI 引数列を生成する（秘密値は `--client-secret-file` / `--service-token-file` のファイルパスのみを渡す）。 |
| `generate_cellar_args_with_secrets(config, secrets)` | `alhalte-cellar` 用の CLI 引数列を生成する（`--service-token-file`、`--max-tin-size` 等を含む）。 |
| `generate_banquette_args(config, room_id, admin_account)` | `alhalte-banquette` 用の CLI 引数列を生成する（`--max-tin-size` 等を含む）。 |
| `stage_extensions(config, base_dir, output_dir)` | `[[extensions]]` の `.wasm` ファイルを検証・コピーし、WASM の `self_introduction`（ABI 1 / ABI 2）からメタデータを抽出して `manifest.json` を書き出す。 |

## データ形式

### `Restaurant.toml` の構造と既定値

```toml
[restaurant]
domain = "vtt.example.com"       # 既定: "localhost"
host = "127.0.0.1"               # 既定: "127.0.0.1"（maitre-d の待ち受けホスト）
data-dir = "/var/lib/alhalte"    # 既定: "/var/lib/alhalte"
socket-dir = "/run/alhalte"      # 既定: "/run/alhalte"

[maitre-d]
port = 8080                      # 既定: 8080（1..=65535）
local-login = false              # 既定: false
client-id = "..."                # local-login = false 時は必須
client-secret = """              # local-login = false 時は必須（age armor）
-----BEGIN AGE ENCRYPTED FILE-----
...
-----END AGE ENCRYPTED FILE-----
"""
salt = "..."                     # 必須（空文字不可）
# public-url = "https://vtt.example.com"  # 省略時は domain から自動導出

[cellar]
port = 8081                      # 既定: 8081（maitre-d.port と異なる値）
anti-oracle = false              # 既定: false
max-tin-size = 67108864          # 既定: 67108864 (64 MiB、1 以上のバイト数、Issue #299)
service-token = """              # 内部書き込み認可トークン（age armor）
-----BEGIN AGE ENCRYPTED FILE-----
...
-----END AGE ENCRYPTED FILE-----
"""

[banquette]
chunk-size-exponent = 8          # 既定: 8（1..=16、1 チャンク = 2^n 件）
heartbeat-interval-secs = 240    # 既定: 240（1 以上）

[[extensions]]
id = "alhalte-dsl"               # package 名（ABI 2 の自己紹介 id と一致すること）
path = "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm"
enabled = true
```

### `max-tin-size` の伝播（Issue #299）

`[cellar] max-tin-size`（既定 `67_108_864` = 64 MiB）は、インスタンス全体で扱える `Tin` 1 個あたりの最大バイト数を定義する。

- `generate_cellar_args_with_secrets` は `--max-tin-size <max_tin_size>` を出力し、`alhalte-cellar` の `POST /store` 事前チェックおよび `PUT /store/{tin_id}` ボディ上限を設定する。
- `generate_maitre_d_args_with_secrets` も同じ `--max-tin-size <max_tin_size>` を出力し、`alhalte-maitre-d` の `/api/cellar/store/{tin_id}` リバースプロキシ上限と、spawn される `alhalte-banquette` への `--max-tin-size` 伝播を統一する。
- `generate_banquette_args` も `--max-tin-size <max_tin_size>` を出力する。

## 内部フロー

### 設定読み込みと秘密値の復号

1. ファイルを読み込み、`[rops]` メタデータが存在する場合は ROPS（ファイル単位暗号化、後方互換）として age 秘密鍵で復号する。
2. TOML を `RestaurantConfig` にデシリアライズした後、`maitre-d.client-secret` および `cellar.service-token` の各フィールドを検査する:
   - 値が age armor（`-----BEGIN AGE ENCRYPTED FILE-----`）であれば、解決した age 秘密鍵で復号する。鍵が未指定または不一致の場合は即座にエラーで失敗する（fail-closed）。
   - 値が平文である場合、`LoadOptions.allow_plaintext_secrets`（または環境変数 `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1`、あるいは `alhalte-cafe` で `local-login = true` の場合）が有効なときのみ受理し、それ以外は `PlaintextSecretRejected` で拒否する。
3. バリデーションを実行する:
   - `restaurant.domain`、`maitre-d.salt` が空でないこと。
   - `local-login = false` のとき `client-id` と `client-secret` が空でないこと。
   - `maitre-d.port != cellar.port` かつポートが `0` でないこと。
   - `banquette.chunk-size-exponent` が `1..=16`、`heartbeat-interval-secs >= 1`、`cellar.max-tin-size >= 1` であること。
   - `[[extensions]]` の `id` が重複しておらず、パス区切りや `..` を含まない安全な識別子であること。

### シークレットファイルの書き出し

1. `write_secret_files` は `<data-dir>/secrets` ディレクトリを作成（Unix では権限 `0700`）する。
2. 復号済みの `maitre-d.client-secret` を `maitre-d-client-secret` に、`cellar.service-token` を `cellar-service-token` に、それぞれ一時ファイル経由で権限 `0600` として原子的に書き出す。
3. 生成される CLI 引数には `--client-secret-file` / `--service-token-file` のパスのみを含め、`ps` や `systemctl cat` に秘密値が露出しないようにする。

### 拡張機能のステージング（`stage_extensions`）

1. `[[extensions]]` の各エントリについて、`path`（`Restaurant.toml` からの相対パスまたは絶対パス）のファイルを読み込み、先頭マジックバイト `\0asm\x01\0\0\0` を検証する。
2. `wasmtime` でモジュールをインスタンス化し、`alhalte_abi_version`（1 または 2）、`alloc`、`dealloc`、`process_generast` のエクスポートを確認する。
3. `self_introduction` がエクスポートされていれば呼び出し、ABI 1 なら `(name, icon_svg, description)`、ABI 2 なら `(id, name, icon_svg, description)` を `postcard` でデコードする。ABI 2 で返された `id` が `Restaurant.toml` の `id` と一致しない場合は `InvalidExtensionWasm` で拒否する（Issue #274）。
4. `<output_dir>/<id>.wasm` にコピーし、抽出したメタデータを含む `manifest.json` を `<output_dir>/manifest.json` に書き出す。

## 不変条件・エラー

- `#![forbid(unsafe_code)]`。ライブラリコード内で `unwrap` / `expect` / `panic!` を使用しない。
- 秘密値の復号失敗、平文シークレットの不許可、または拡張機能 WASM の ABI 不整合はすべてエラーとして返し、不完全な状態で起動を進めない（fail-closed）。
- `cellar` の待ち受けホストは `restaurant.host` の値にかかわらず常に `127.0.0.1` に固定し、外部ネットワークへ直接露出させない。

## 既知の差分・制約

- シークレットファイルのパーミッション制限（`0700` / `0600`）は Unix 系 OS（`#[cfg(unix)]`）で適用される。Windows では OS 標準のユーザープロファイル / ACL に委ねられる。
- `--client-secret` / `--service-token` による直接引数出力（`generate_maitre_d_args` / `generate_cellar_args`）は後方互換および単体テスト用に残されているが、`alhalte-cafe` / `alhalte-bistrot` / `alhalte-glacier` の実運用経路では常に `*_with_secrets`（ファイル渡し）を使用する。
