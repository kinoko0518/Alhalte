# alhalte-bistrot

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-bistrot`  
> テスト: `crates/alhalte-bistrot/tests/bistrot_tests.rs`

## 責務

restaurant の構成管理。`Bistrot.toml` を読み、rops（SOPS 互換の age 暗号）で秘密情報を復号し、各モジュールへ渡す引数と systemd unit を**純粋関数**として生成する。開発用の拡張機能 staging と manifest 生成も行う。

## 公開 API / 外部接続点

外部接続点は設定ファイルと CLI のみ。ネットワークアクセスはない。

### CLI

```text
alhalte-bistrot [--config Bistrot.toml] [--age-key KEY] <command>
```

| コマンド | 内容 |
| --- | --- |
| `args [module]` | `all` / `maitre-d` / `cellar` / `banquette` の引数を出力。 |
| `systemd` | unit を stdout または `--output-dir` に出力。 |
| `url` | argenterie の URL を出力。 |
| `validate` | 設定を検証し、要約を出力。 |
| `stage-extensions --output-dir DIR` | `.wasm` をコピーし `manifest.json` を生成。 |
| `encrypt --recipient age1...` | 平文 TOML を rops で暗号化。 |

### 主な公開型

- `BistrotConfig`、`RestaurantConfig`、`MaitreDConfig`、`CellarConfig`、`BanquetteConfig`、`ArgenterieConfig`、`ExtensionConfig`
- `ModuleArgs`、`generate_all_args`、`generate_maitre_d_args`、`generate_cellar_args`、`generate_banquette_args`、`generate_module_args`
- `SystemdOptions`、`SystemdUnits`、`generate_systemd_units`、`generate_*_service`
- `parse_bistrot_toml`、`load_from_path`、`decrypt_rops_toml`、`encrypt_bistrot_toml`

## 設定ファイル

`Bistrot.toml` は kebab-case を基本に、snake_case の別名も受け付ける。

```toml
[restaurant]
domain = "localhost"
host = "127.0.0.1"
data-dir = ".alhalte/data"
socket-dir = ".alhalte/sockets"

[maitre-d]
port = 8080
client-id = "..."
client-secret = "..."
salt = "..."
# db-path, socket-dir, banquette-bin, cellar-url は任意

[cellar]
port = 8081
anti-oracle = false
# storage-path, db-path は任意
# maitre-d-url = "http://127.0.0.1:8080"  # 既定は http://{restaurant.host}:{maitre-d.port}
# maitre-d-socket = ".alhalte/sockets/maitre-d.sock"  # 指定時は url より優先
# allow-all-sessions = false              # ローカル開発専用。true で全 session を許可

[banquette]
chunk-size-exponent = 8
# socket-dir は任意

[argenterie]
port = 5173

[[extensions]]
id = "alhalte-dsl"
name = "Alhalte DSL"
path = "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm"
enabled = true
```

`maitre-d` の `client-id` / `client-secret` / `salt` は必須。`chunk-size-exponent` は 1〜30。`argenterie.port` は 0 不可。`extensions[].id` は空不可。

パス省略時の既定:

| 設定 | 既定 |
| --- | --- |
| `maitre-d.db-path` | `{data_dir}/maitre-d.sqlite3` |
| `maitre-d.socket-dir` | `{socket_dir}` |
| `cellar.storage-path` | `{data_dir}/cellar/tin` |
| `cellar.db-path` | `{data_dir}/cellar/cellar.sqlite3` |
| `cellar.maitre-d-url` | `http://{restaurant.host}:{maitre-d.port}`（`0.0.0.0` / `::` は `127.0.0.1`） |
| `cellar.maitre-d-socket` | なし（指定時は `maitre-d-url` より優先） |
| `cellar.allow-all-sessions` | `false` |
| `banquette.socket-dir` | `{socket_dir}` |

## 内部フロー

### 設定読み込み

1. ファイルを文字列として読む。
2. `[sops]` または `ENC[` を含む場合は rops 暗号化とみなし、`ROPS_AGE` または `--age-key` で復号する。
3. 平文 TOML はそのまま `toml::from_str` する。
4. `BistrotConfig::validate` で必須値と範囲を検証する。

### 引数生成

- `generate_maitre_d_args`: host、port、db-path、client-id、client-secret、salt、socket-dir、cellar-url。`banquette-bin` があれば追加。
- `generate_cellar_args`: host、port、storage-path、db-path、session verifier（`cellar.maitre-d-socket` があれば `--maitre-d-socket <path>`、なければ `--maitre-d-url <url>`）、`allow-all-sessions` が true のとき `--allow-all-sessions`、`anti-oracle` が true のとき `--anti-oracle`。既定の URL は `http://{restaurant.host}:{maitre-d.port}`。
- `generate_banquette_args`: chunk-size-exponent と socket-dir。

すべて副作用なし。`generate_module_args` はモジュール名を正規化して振り分ける。

### systemd 生成

`SystemdOptions` の `bin_dir`、`working_dir`、`web_dir`、`pnpm_bin`、`argenterie_cmd`、`user`、`group`、`restart`、`restart_sec`、`extra_path` を使う。`generate_systemd_units` は以下を返す。

- `alhalte-maitre-d.service`
- `alhalte-cellar.service`
- `alhalte-argenterie.service`（`pnpm run dev` または `argenterie_cmd`）
- `alhalte-bistrot.target`（3 service を `Wants=`）

`ExecStart` の引数は空白・引用符・バックスラッシュをエスケープする。`alhalte-cellar.service` と `flake.nix` の直接起動 fallback は、どちらも `generate_cellar_args` と同じ引数（fallback は `alhalte-bistrot args cellar` の出力）を使う。

### 拡張機能 staging

`stage_extensions_with_base` は設定順を 0-based の `priority` とし、`<output_dir>/<id>.wasm` へコピーする。`ext.path` が存在しない場合は `Bistrot.toml` の親ディレクトリからの相対パスも試す。`manifest.json` は `id`、`name`、`url`、`enabled`、`priority` を持つ。

### rops

`is_rops_encrypted` は `[sops]` または `ENC[` で判定する。復号は `ROPS_AGE` 環境変数を使い、`decrypt_rops_toml_with_key` は scoped に環境変数を差し替える。暗号化は age recipient を受け取り、AES256GCM + SHA512 で rops TOML を作る。

## 既知の差分・制約

- `generate_banquette_args` は `--socket-dir <dir>` を出すが、`alhalte-banquette` の CLI は `--socket <path>` を取る。現状この引数は直接起動に使われておらず、maitre-d の spawner が `--socket` を組み立てる。
- `nix run .#bistrot` は systemd が使えない環境ではバックグラウンドプロセスへフォールバックする。この処理は `flake.nix` の `bistrot-runner` 側にある。
- argenterie は開発サーバー（Vite）として起動する。本番配信の unit は未定義。
- `Bistrot.toml` の `maitre-d` と `cellar` は必須。`restaurant`、`banquette`、`argenterie`、`extensions` は既定値を持つ。
- `cellar.allow-all-sessions` はローカル開発専用。有効時は `alhalte-cellar` が警告を出し、session 検証をすべて許可する。verifier が未設定でこのフラグも false の場合、`alhalte-cellar` は全拒否の verifier に落ちず起動エラーになる。
- rops の鍵はファイルに保存せず、`ROPS_AGE` か CLI 引数で渡す前提。
