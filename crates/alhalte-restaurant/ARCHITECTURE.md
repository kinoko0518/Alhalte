# alhalte-restaurant

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-restaurant`  
> テスト: `crates/alhalte-restaurant/tests/restaurant_tests.rs`

## 責務

`Restaurant.toml` のモデルと、cafe / bistrot が共有する変換処理。

- TOML のパース・検証（`RestaurantToml`）
- 値単位の age armor 復号と rops 全体暗号化の後方互換
- 復号した秘密の `<data-dir>/secrets/` への 0600 書き出し
- `alhalte-maitre-d` / `alhalte-cellar` の引数生成（純粋関数）
- 公式拡張 wasm の staging と `manifest.json` 生成

起動（cafe）と systemd 生成（bistrot）はこの crate の外にある。ネットワークアクセスはない。

## 公開 API / 外部接続点

### 主な型

- `RestaurantToml`（ファイル全体）、`RestaurantConfig`（`[restaurant]`）
- `MaitreDConfig`、`CellarConfig`、`BanquetteConfig`、`ExtensionConfig`
- `ExtensionManifestEntry`
- `RestaurantError`

### パース・復号

- `parse_restaurant_toml` / `parse_restaurant_toml_with_key` / `parse_restaurant_toml_with_options`
- `load_restaurant_from_path` / `_with_key` / `_with_options`
- `ParseOptions`、`is_rops_encrypted`
- `encrypt_restaurant_toml`、`encrypt_age_armored_value`
- `decrypt_rops_toml` / `_with_key`

### 秘密

- `SecretFiles`、`secret_files`、`write_secret_files`
- `is_age_armored_value`、`decrypt_age_value`、`decrypt_age_values`、`find_plaintext_secrets`
- `resolve_age_identity_source`、`parse_age_identity`、`parse_age_recipient`

### 引数

```rust
pub struct MaitreDArgsOptions {
    pub web_root: Option<PathBuf>,
    pub banquette_bin: Option<PathBuf>,
}

pub fn generate_maitre_d_args(config: &RestaurantToml, options: &MaitreDArgsOptions) -> Vec<String>;
pub fn generate_cellar_args(config: &RestaurantToml) -> Vec<String>;
pub fn generate_all_args(config: &RestaurantToml, options: &MaitreDArgsOptions) -> ModuleArgs;
```

`web-root` と `banquette-bin` はインスタンス設定ではなく配置情報なので、引数生成のオプションとして渡す。

### `RestaurantToml` の派生値

- `maitre_d_url()`: ブラウザ用の loopback URL（`http://{connect_host}:{maitre-d.port}/`）
- `maitre_d_public_url()`: `public-url` → `https://{domain}` → loopback URL の順
- `cors_origins()`: public URL / domain / local Vite origins（5173）の明示 allow-list
- `is_lan_exposed()` / `lan_origin_ports()`: `restaurant.host` が wildcard または非 loopback のとき、maitre-d のポートを LAN origin として許可
- `cellar_url()` / `cellar_maitre_d_url()`: cellar は常に `127.0.0.1`
- `maitre_d_db_path()` / `cellar_storage_path()` / `cellar_db_path()` / `maitre_d_socket_dir()` / `banquette_socket_dir()` / `secrets_dir()`
- `resolve_relative_paths(base)`: `data-dir` / `socket-dir` の絶対化
- `stage_extensions` / `stage_extensions_with_base`

## 設定ファイル

`Restaurant.toml` は平文 TOML。kebab-case を基本に snake_case の別名も受け付ける。秘密値だけを value 単位で age armor にできる。

```toml
[restaurant]
domain = "localhost"
host = "127.0.0.1"          # 0.0.0.0 / 非 loopback で LAN 公開
data-dir = ".alhalte/data"
socket-dir = ".alhalte/sockets"

[maitre-d]
port = 8080                 # UI/API 共通
# public-url = "https://vtt.example.com"
# local-login = true
# client-id = "..."         # Google ログイン時のみ必須
# client-secret = "age armor ..."
salt = "..."

[cellar]
port = 8081
anti-oracle = false
# service-token = "age armor ..."

[banquette]
chunk-size-exponent = 8
# heartbeat-interval-secs = 240

[[extensions]]
id = "alhalte-dsl"
path = "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm"
enabled = true
```

### 検証

- `salt` は常に非空。
- `client-id` / `client-secret` は `!local-login` のときだけ非空を要求する。
- `domain` / `host` / `data-dir` / `socket-dir` は非空。
- `maitre-d.port` / `cellar.port` は 0 不可。
- `chunk-size-exponent` は 1..=30、`heartbeat-interval-secs` は 1 以上。
- `cellar.service-token` は設定時のみ非空。
- `extensions[].id` は空不可。

### 未知項目

`RestaurantToml` / `RestaurantConfig` / `MaitreDConfig` / `CellarConfig` / `BanquetteConfig` / `ExtensionConfig` は `deny_unknown_fields` を持つ。旧 `[argenterie]`、`cellar.host`、`maitre-d.web-root`、`extensions[].name` などは起動時にエラーになる。互換の読み替えは行わない。

### 既定値

| 設定 | 既定 |
| --- | --- |
| `restaurant.domain` | `localhost` |
| `restaurant.host` | `127.0.0.1` |
| `restaurant.data-dir` | Unix `/var/lib/alhalte` / Windows `data` |
| `restaurant.socket-dir` | Unix `/run/alhalte` / Windows `sockets` |
| `maitre-d.port` | `8080` |
| `maitre-d.db-path` | `{data-dir}/maitre-d.sqlite3` |
| `maitre-d.socket-dir` | `{socket-dir}` |
| `cellar.port` | `8081` |
| `cellar.storage-path` | `{data-dir}/cellar/tin` |
| `cellar.db-path` | `{data-dir}/cellar/cellar.sqlite3` |
| `banquette.chunk-size-exponent` | `8` |
| `banquette.heartbeat-interval-secs` | `240` |
| 秘密 file | `{data-dir}/secrets/` |

## 内部フロー

### 設定読み込み

1. ファイルを文字列として読む。
2. `[sops]` / `ENC[` を含む場合は rops 全体暗号化として復号する。
3. 平文 TOML を `toml::Value` にする。
4. 復号前のソース文書で平文 secret を検出する。
5. armored 値があれば identity を解決して再帰的に復号する。identity が無ければ `AgeKeyMissing`、復号失敗は `AgeDecrypt`、壊れた identity は `AgeIdentity`。
6. 平文 secret が残っていて許可されていなければ `PlaintextSecret`。
7. `RestaurantToml` に deserialize し、`validate` する。

### 引数生成

- `generate_maitre_d_args`: host / port / db-path / salt / public-url / socket-dir / cellar-url / chunk-size-exponent / banquette-heartbeat-interval-secs。client-id は非空のときだけ、client-secret は `--client-secret-file`、service-token は `--service-token-file`。local-login は `--local-login`。`MaitreDArgsOptions.web_root` があれば `--web-root`、`banquette_bin` があれば `--banquette-bin`。CORS は `cors_origins()`、LAN は `lan_origin_ports()`。
- `generate_cellar_args`: host は常に `127.0.0.1`。port / storage-path / db-path / `--maitre-d-url http://127.0.0.1:{maitre-d.port}`。anti-oracle と service-token-file を必要時だけ追加。
- すべて副作用なし。

### 秘密 file の書き出し

1. `<data-dir>/secrets/` を 0700 で作成する。
2. `maitre-d-client-secret` / `cellar-service-token` を 0600 で書く。
3. 値を返さず `SecretFiles`（path のみ）を返す。秘密が未設定ならディレクトリも作らない。

### 拡張 staging

`stage_extensions_with_base` は設定順を 0-based の `priority` とし、`<output_dir>/<id>.wasm` へコピーする。`ext.path` が存在しない場合は設定ファイルの親ディレクトリからの相対パスも試す。全 wasm を解決してからコピーするため、失敗時に中途半端な staging 出力を残さない。`manifest.json` は `id` / `url` / `enabled` / `priority` だけを持ち、表示メタデータは wasm の `self_introduction` が正。

## 既知の差分・制約

- `[argenterie]`、path override、`cellar.host`、`cellar.maitre-d-*`、`cellar.allow-all-sessions`、`banquette.socket-dir` は削除した。パスは `data-dir` / `socket-dir` から導出する。
- local-login では Google の client-id / client-secret を省略できる。salt は local アカウントの sub hash にも使うため常に必要。
- 平文 secret は fail-closed。`ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` のときだけ許可する。cafe は `local-login = true` の文書に限って自動で許可する。
- `cellar.service-token` は age armor で保持する前提。未設定のままでは banquette の部屋状態 persistence が cellar に拒否される。
- `cellar` は常に `127.0.0.1` に bind する。LAN 公開は maitre-d の UI/API だけ。
