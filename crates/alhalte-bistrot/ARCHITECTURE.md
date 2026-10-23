# alhalte-bistrot

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-bistrot`  
> テスト: `crates/alhalte-bistrot/tests/systemd_tests.rs`

## 責務

`Restaurant.toml` から systemd unit を生成する**純粋関数的変換**だけを担う。

- `alhalte-cellar.service`
- `alhalte-maitre-d.service`
- `alhalte.target`

設定のパース・秘密復号・引数生成は [alhalte-restaurant](../alhalte-restaurant/ARCHITECTURE.md)、ワンショット起動は [alhalte-cafe](../alhalte-cafe/ARCHITECTURE.md)、NixOS 常駐サービスは [alhalte-glacier](../../nix/alhalte-glacier/ARCHITECTURE.md) が持つ。bistrot は `serve`、Vite dev server、Windows バンドル、拡張 staging を行わない。

## 公開 API / 外部接続点

外部接続点は `Restaurant.toml` と CLI のみ。ネットワークアクセスはない。

### CLI

```text
alhalte-bistrot [--config Restaurant.toml] [--age-key KEY] <command>
```

| コマンド | 内容 |
| --- | --- |
| `systemd` | unit を stdout または `--output-dir` に出力する。秘密 file も `<data-dir>/secrets/` に書き出す。 |
| `validate` | 設定を検証し、要約を出力する。ファイルは書き出さない。 |
| `encrypt --recipient age1...` | 平文 TOML を rops で全体暗号化する（後方互換）。 |
| `encrypt-value --recipient age1...` | stdin の平文 1 値を age armor で暗号化し stdout へ。 |

`systemd` の主なオプション:

- `--bin-dir DIR`（既定 `/usr/local/bin`）
- `--output-dir DIR`（省略時は stdout）
- `--working-dir DIR`
- `--web-root DIR`（省略時は `{data-dir}/web`。`maitre-d --web-root` に渡す）
- `--user` / `--group`

### 主な公開型

- `SystemdOptions`、`SystemdUnits`
- `generate_systemd_units`、`generate_cellar_service`、`generate_maitre_d_service`、`generate_alhalte_target`
- `DEFAULT_SYSTEMD_BIN_DIR`

## 内部フロー

### systemd 生成

1. `Restaurant.toml` を読み、`alhalte-restaurant` で復号・検証する。
2. 設定ファイルの親ディレクトリ基準で `data-dir` / `socket-dir` を絶対化する（cwd 非依存）。
3. 秘密 file を `<data-dir>/secrets/` に 0600 で書き出す。
4. `generate_systemd_units` が 3 unit を組み立てる。
   - `alhalte-cellar.service`: `generate_cellar_args` の引数。cellar は常に `127.0.0.1` に bind し、session verifier は `http://127.0.0.1:{maitre-d.port}`。
   - `alhalte-maitre-d.service`: `generate_maitre_d_args` の引数 + `--web-root`。SPA は maitre-d が同一 origin で配信する。
   - `alhalte.target`: 2 service を `Wants=` / `WantedBy=multi-user.target`。
5. `--output-dir` 指定時は `<dir>/alhalte-cellar.service`、`<dir>/alhalte-maitre-d.service`、`<dir>/alhalte.target` に書く。

ExecStart の引数は空白・引用符・バックスラッシュをエスケープする。unit に秘密値は含まれず、`--client-secret-file` / `--service-token-file` の path だけが入る。

## データ形式

`Restaurant.toml` のスキーマと秘密値の扱いは [alhalte-restaurant](../alhalte-restaurant/ARCHITECTURE.md) を参照。bistrot が生成する unit のファイル名は次の 3 つだけ。

- `alhalte-cellar.service`
- `alhalte-maitre-d.service`
- `alhalte.target`

## 既知の差分・制約

- `serve` / `url` / `args` / `stage-extensions` サブコマンドは削除した。起動は cafe、staging は cafe の `stage-extensions` を使う。
- `alhalte-argenterie.service` は生成しない。UI は maitre-d の `--web-root` 配信に統一した。
- `maitre-d.web-root` / `banquette-bin` は TOML に持たない。web-root は CLI / cafe / glacier の配置情報。
- `nix run .#bistrot` はビルド済みバイナリを実行するだけ。開発用のワンショット起動は `nix run .`（cafe）を使う。
- unit の `WorkingDirectory` は省略可能。相対パスは `systemd` 実行時に絶対化される。
- NixOS ではこの crate ではなく `nixosModules.default`（glacier）を使う。bistrot は NixOS 以外の systemd 向け。
