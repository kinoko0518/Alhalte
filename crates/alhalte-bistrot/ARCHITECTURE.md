# alhalte-bistrot

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-bistrot`  
> テスト: `crates/alhalte-bistrot/tests/bistrot_tests.rs`

## 責務

restaurant の構成管理。`Bistrot.toml` を読み、秘密情報を復号し（値単位の age armor を第一級、rops 全体暗号化は後方互換）、各モジュールへ渡す引数と systemd unit を**純粋関数**として生成する。復号した秘密は `<data-dir>/secrets/` に 0600 で書き、引数には file path だけを渡す。開発用の拡張機能 staging と manifest 生成も行う。

## 公開 API / 外部接続点

外部接続点は設定ファイルと CLI のみ。ネットワークアクセスはない。

### CLI

```text
alhalte-bistrot [--config Bistrot.toml] [--age-key KEY] <command>
```

| コマンド | 内容 |
| --- | --- |
| `args [module]` | `all` / `maitre-d` / `cellar` の引数を出力。秘密 file を `<data-dir>/secrets/` に 0600 で書き出す。 |
| `systemd` | unit を stdout または `--output-dir` に出力。秘密 file も書き出す。 |
| `url` | argenterie の URL を出力。 |
| `validate` | 設定を検証し、要約を出力。ファイルは書き出さない。 |
| `stage-extensions --output-dir DIR` | `.wasm` をコピーし `manifest.json` を生成。 |
| `encrypt --recipient age1...` | 平文 TOML を rops で全体暗号化（後方互換）。 |
| `encrypt-value --recipient age1...` | stdin の平文 1 値を age armor で暗号化し stdout へ。 |

### 主な公開型

- `BistrotConfig`、`RestaurantConfig`、`MaitreDConfig`、`CellarConfig`、`BanquetteConfig`、`ArgenterieConfig`、`ExtensionConfig`
- `ModuleArgs`、`generate_all_args`、`generate_maitre_d_args`、`generate_cellar_args`、`generate_module_args`
- `SystemdOptions`、`SystemdUnits`、`generate_systemd_units`、`generate_*_service`
- `ParseOptions`、`parse_bistrot_toml`、`parse_bistrot_toml_with_options`、`load_from_path*`、`decrypt_rops_toml`、`encrypt_bistrot_toml`
- `SecretFiles`、`secret_files`、`write_secret_files`、`encrypt_age_value`、`decrypt_age_value`、`is_age_armored_value`、`find_plaintext_secrets`

## 設定ファイル

`Bistrot.toml` は平文 TOML のままで、kebab-case を基本に snake_case の別名も受け付ける。秘密値だけを value 単位で age armor にできる。

```toml
[restaurant]
domain = "localhost"
host = "127.0.0.1"
data-dir = ".alhalte/data"
socket-dir = ".alhalte/sockets"

[maitre-d]
port = 8080
client-id = "..."
client-secret = """
-----BEGIN AGE ENCRYPTED FILE-----
...
-----END AGE ENCRYPTED FILE-----
"""
salt = "..."
# db-path, socket-dir, banquette-bin, cellar-url, public-url は任意
# debug-login = false   # 開発専用。true で maitre-d --debug-login を有効にする（Issue #150）

[cellar]
port = 8081
anti-oracle = false
# storage-path, db-path は任意
# maitre-d-url = "http://127.0.0.1:8080"  # 既定は http://{restaurant.host}:{maitre-d.port}
# maitre-d-socket = ".alhalte/sockets/maitre-d.sock"  # 指定時は url より優先
# allow-all-sessions = false              # ローカル開発専用。true で全 session を許可
# service-token = """                     # 内部書き込み用。age armor で暗号化する
# -----BEGIN AGE ENCRYPTED FILE-----
# ...
# -----END AGE ENCRYPTED FILE-----
# """

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

`maitre-d` の `client-id` / `client-secret` / `salt` は必須。`chunk-size-exponent` は 1〜30。`argenterie.port` は 0 不可。`extensions[].id` は空不可。`cellar.service-token` は設定する場合のみ非空でなければならない。`maitre-d.debug-login` は既定 `false` で、真偽値のみ。true はローカル開発専用で、起動する maitre-d は account 名だけでサインインできる状態になる（検証では禁止しないが、ドキュメントで明示する）。

### 秘密値の扱い（Issue #194）

- 秘密フィールドは `maitre-d.client-secret` と `cellar.service-token`。値が `-----BEGIN AGE ENCRYPTED FILE-----` で始まる文字列は age armor とみなし、`parse_bistrot_toml*` が TOML ツリーを再帰走査して復号してから `BistrotConfig` に deserialize する。
- identity は `--age-key` → `ROPS_AGE` → `ALHALTE_AGE_KEY` の順で解決する。x25519 identity のみ対応。
- **平文 secret は fail-closed**: 暗号化されていない文書に平文の `client-secret` / `service-token` が残っていると `BistrotError::PlaintextSecret`（対象フィールドの dotted path を含む）で失敗する。`ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` のときだけ許可する（ローカル開発専用。`ParseOptions::allow_plaintext_secrets` で明示指定も可）。
- rops で全体暗号化された文書は at-rest で暗号化済みなので、復号後の平文値は上記チェックの対象外。
- 復号した秘密は `<data-dir>/secrets/maitre-d-client-secret` と `<data-dir>/secrets/cellar-service-token` に **0600**（ディレクトリは 0700）で書く。引数生成は file path だけを返し、`ps` / systemd unit / ログに値は出ない。
- `--age-key` は `ROPS_AGE` 環境変数も受ける（clap の env）。鍵はファイルに保存しない。
- リポジトリ同梱の `Bistrot.toml` はローカル開発専用の平文 secret を持ち、`flake.nix` の bistrot-runner が `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` を設定する。実 credential は `Bistrot.local.toml`（gitignore）に armored 値で置く。テンプレートは `Bistrot.example.toml`。

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
| `maitre-d.debug-login` | `false`（debug 時の DB は `{data_dir}/maitre-d-debug.sqlite3` を maitre-d が選ぶ） |
| `banquette.socket-dir` | `{socket_dir}` |
| 秘密 file | `{data_dir}/secrets/` |

## 内部フロー

### 設定読み込み

1. ファイルを文字列として読む。
2. `[sops]` または `ENC[` を含む場合は rops 全体暗号化とみなし、`ROPS_AGE` または `--age-key` で復号する。
3. 平文 TOML はそのまま `toml::from_str` して `toml::Value` にする。
4. 平文の秘密フィールドを検出する（復号前のソース文書に対して）。
5. armored 値があれば identity を解決して再帰的に復号する。identity が無ければ `BistrotError::AgeKeyMissing`、復号失敗は `BistrotError::AgeDecrypt`、壊れた identity は `BistrotError::AgeIdentity`。
6. 平文 secret が残っていて許可されていなければ `BistrotError::PlaintextSecret`。
7. `BistrotConfig` に deserialize し、`BistrotConfig::validate` で必須値と範囲を検証する。

### 引数生成

- `generate_maitre_d_args`: host、port、db-path、client-id、salt、socket-dir、cellar-url、chunk-size-exponent、`--client-secret-file <data-dir>/secrets/maitre-d-client-secret`。`banquette-bin` があれば追加。`cellar.service-token` があれば `--service-token-file <data-dir>/secrets/cellar-service-token` を追加する（maitre-d が各 banquette へ path を伝播する）。`maitre-d.debug-login` が true なら `--debug-login` を追加する。debug 用 DB path は渡さない（maitre-d が `--db-path` の隣の `maitre-d-debug.sqlite3` を自分で選ぶ）。
- `generate_cellar_args`: host、port、storage-path、db-path、session verifier（`cellar.maitre-d-socket` があれば `--maitre-d-socket <path>`、なければ `--maitre-d-url <url>`）、`allow-all-sessions` が true のとき `--allow-all-sessions`、`anti-oracle` が true のとき `--anti-oracle`、`cellar.service-token` があれば `--service-token-file <path>`。既定の URL は `http://{restaurant.host}:{maitre-d.port}`。

`alhalte-banquette` は `alhalte-maitre-d` が部屋ごとに起動し、`--room-id` / `--admin` / `--socket` / `--chunk-size-exponent` / `--service-token-file` を spawner が組み立てるため、bistrot は静的な banquette 引数を生成しない（部屋 head は banquette が cellar の alias から自分で読む。#192）。`[banquette] chunk-size-exponent` と `[cellar] service-token` は `generate_maitre_d_args` 経由で maitre-d に渡り、spawner が各 banquette に伝播する。

すべて副作用なし。`generate_module_args` はモジュール名を正規化して振り分ける。秘密 file の書き出しは `write_secret_files`（`args` / `systemd` サブコマンドが実行時に行う）だけが担う。

### 秘密 file の書き出し

1. `<data-dir>/secrets/` を 0700 で作成する。
2. `maitre-d-client-secret` と `cellar-service-token` を `OpenOptions::mode(0o600)` で作成し、値を書き、既存 file の truncate で mode が緩まないよう再度 0600 を設定する。
3. 値を返さず `SecretFiles`（path のみ）を返す。秘密が未設定ならディレクトリも作らない。

`data-dir` が相対パスの場合、file path も相対パスになる。unit の `WorkingDirectory` と起動時の cwd が同じであることが前提（dev は `nix run .#bistrot` が repo root で起動する）。

### systemd 生成

`SystemdOptions` の `bin_dir`、`working_dir`、`web_dir`、`pnpm_bin`、`argenterie_cmd`、`user`、`group`、`restart`、`restart_sec`、`extra_path` を使う。`generate_systemd_units` は以下を返す。

- `alhalte-maitre-d.service`
- `alhalte-cellar.service`
- `alhalte-argenterie.service`（`pnpm run dev` または `argenterie_cmd`）
- `alhalte-bistrot.target`（3 service を `Wants=`）

`ExecStart` の引数は空白・引用符・バックスラッシュをエスケープする。`alhalte-cellar.service` と `flake.nix` の直接起動 fallback は、どちらも `generate_cellar_args` と同じ引数（fallback は `alhalte-bistrot args cellar` の出力）を使う。

### 拡張機能 staging

`stage_extensions_with_base` は設定順を 0-based の `priority` とし、`<output_dir>/<id>.wasm` へコピーする。`ext.path` が存在しない場合は `Bistrot.toml` の親ディレクトリからの相対パスも試す。宣言された wasm がどちらにも無い場合は `BistrotError::ExtensionWasmMissing { id, path }` で失敗し、コピーも `manifest.json` の書き込みも行わない（Issue #165）。`Bistrot.toml` のパス間違いや wasm のビルド忘れを起動時に検出するため、`manifest.json` には存在しない wasm を指す entry を作らない。全 wasm を解決してからコピーするため、失敗時に中途半端な staging 出力は残らない。`manifest.json` は `id`、`name`、`url`、`enabled`、`priority` を持つ。

### rops

`is_rops_encrypted` は `[sops]` または `ENC[` で判定する。復号は `ROPS_AGE` 環境変数を使い、`decrypt_rops_toml_with_key` は scoped に環境変数を差し替える。暗号化は age recipient を受け取り、AES256GCM + SHA512 で rops TOML を作る。

## 既知の差分・制約

- `generate_banquette_args` は削除した。banquette は maitre-d が部屋ごとに起動し、`--room-id` / `--admin` / `--socket` は設定ファイルから静的に決められないため。`args banquette` は未知のモジュールとしてエラーになる。
- `nix run .#bistrot` は systemd が使えない環境ではバックグラウンドプロセスへフォールバックする。この処理は `flake.nix` の `bistrot-runner` 側にある。
- argenterie は開発サーバー（Vite）として起動する。本番配信の unit は未定義。
- `Bistrot.toml` の `maitre-d` と `cellar` は必須。`restaurant`、`banquette`、`argenterie`、`extensions` は既定値を持つ。
- `maitre-d.debug-login` は開発専用のスイッチで、Bistrot は検証で止めない（`--debug-login` を渡すだけ）。debug 時の DB は `{data_dir}/maitre-d-debug.sqlite3` を maitre-d が選び、banquette は `--no-persist` で起動するため、通常 DB / 通常 cellar は変更されない。本番の `Bistrot.toml` では false のままにする。
- `cellar.allow-all-sessions` はローカル開発専用。有効時は `alhalte-cellar` が警告を出し、session 検証をすべて許可する。verifier が未設定でこのフラグも false の場合、`alhalte-cellar` は全拒否の verifier に落ちず起動エラーになる。
- `cellar.service-token` は age armor で暗号化して保持する前提。リポジトリ同梱の `Bistrot.toml` はローカル開発専用の平文既定値 `alhalte-local-service-token` を設定し、`ALHALTE_ALLOW_PLAINTEXT_SECRETS=1`（flake.nix の bistrot-runner が設定）でのみ読み込める。実 credential は `Bistrot.local.toml`（gitignore）に armored 値で置く（未設定・空文字は不可、未設定のままでは persistence が cellar に拒否される）。
- 生成される systemd unit の `ExecStart` に秘密値は含まれない（`--client-secret-file` / `--service-token-file` の path のみ）。unit ファイルと `ps` から秘密を読めない代わりに、`<data-dir>/secrets/` の 0600 file を読む権限モデルに依存する。`--service-token` / `--client-secret` の argv 指定は後方互換のため受理するが、Bistrot は生成しない。
- age の identity はファイルに保存せず、`--age-key` / `ROPS_AGE` / `ALHALTE_AGE_KEY` で渡す前提。
- `args` / `systemd` サブコマンドは秘密 file を書き出す副作用を持つ。`validate` / `url` は書き出さない。
- `data-dir` が相対パスの場合、生成される秘密 file path も相対パスになる。service の cwd が `args` を実行した cwd と一致することが前提。
- 秘密値の復号は起動時の 1 回だけで、復号結果をメモリに保持する。`sops` 全体暗号化と age armor の混在は許容するが、両方に同じ秘密を置かないこと。
