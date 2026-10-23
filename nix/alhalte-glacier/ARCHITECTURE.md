# alhalte-glacier

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `nix/alhalte-glacier/module.nix`  
> flake 出力: `nixosModules.default` / `nixosConfigurations.alhalte-example`

## 責務

NixOS の常駐サービスとして Alhalte を定義する。`mkOption` / `mkEnableOption` で作った設定項目から `systemd.services.alhalte-cellar` / `systemd.services.alhalte-maitre-d` / `systemd.targets.alhalte` を生成する。

バイナリは root flake の `packages.<system>.alhalte`、SPA は `packages.<system>.argenterie-spa` を既定値として使う。`Restaurant.toml` は使わず、Nix option が設定の正になる。

## 設定項目（`services.alhalte.*`）

| オプション | 型 | 既定 | 内容 |
| --- | --- | --- | --- |
| `enable` | bool | false | サービスを有効化する。 |
| `package` | package | `packages.<system>.alhalte` | バイナリ一式。 |
| `webRoot` | path | `packages.<system>.argenterie-spa` | ビルド済み SPA。 |
| `domain` | str | `localhost` | 公開ドメイン。 |
| `host` | str | `127.0.0.1` | maitre-d の bind host。 |
| `maitreDPort` | port | 8080 | UI/API のポート。 |
| `cellarPort` | port | 8081 | cellar のポート（bind は 127.0.0.1 固定）。 |
| `dataDir` | path | `/var/lib/alhalte` | 永続データ。 |
| `socketDir` | path | `/run/alhalte` | banquette socket。 |
| `user` / `group` | str | `alhalte` | 実行ユーザー / グループ。 |
| `localLogin` | bool | false | ローカルパスワードログイン。 |
| `clientId` | str | `""` | Google OAuth client ID。 |
| `clientSecretFile` | path / null | null | OAuth client secret の file。 |
| `salt` | str | `""` | sub hash 用 salt。必須。 |
| `serviceTokenFile` | path / null | null | 内部 service token の file。 |
| `antiOracle` | bool | false | cellar の anti-oracle。 |
| `chunkSizeExponent` | int 1..30 | 8 | banquette の chunk size。 |
| `heartbeatIntervalSecs` | int > 0 | 240 | 部屋状態の保存間隔。 |
| `lan` | bool | false | private-network origin を許可する。 |
| `openFirewall` | bool | true | `lan` 時に maitre-d のポートを開ける。 |
| `publicUrl` | str / null | null | OAuth callback の基点。 |

## 生成するサービス

- `alhalte-cellar.service`
  - `--host 127.0.0.1 --port <cellarPort> --storage-path <dataDir>/cellar/tin --db-path <dataDir>/cellar/cellar.sqlite3 --maitre-d-url http://127.0.0.1:<maitreDPort>`
  - `--anti-oracle` / `--service-token-file` は必要時のみ。
- `alhalte-maitre-d.service`
  - `--host <host> --port <maitreDPort> --db-path <dataDir>/maitre-d.sqlite3 --salt <salt> --public-url <derived> --socket-dir <socketDir> --cellar-url http://127.0.0.1:<cellarPort> --chunk-size-exponent <n> --banquette-heartbeat-interval-secs <n> --web-root <webRoot>`
  - `--local-login` / `--client-id` / `--client-secret-file` / `--service-token-file` / `--cors-origin` / `--allow-lan-origin-port` は必要時のみ。
- `alhalte.target`: 2 service を `wants` し、`multi-user.target` から `wantedBy`。

## 内部フロー / 注意

- 秘密値は値ではなく file path だけを受け取る（`clientSecretFile` / `serviceTokenFile`）。sops-nix などで `0600` の file を用意する。
- `localLogin = false` のときは `clientId` と `clientSecretFile` が必須（assertion）。
- `salt` は常に必須（assertion）。
- `dataDir` / `socketDir` は `systemd.tmpfiles.rules` で `0700` 作成する。
- `lan && openFirewall` のとき `networking.firewall.allowedTCPPorts` に `maitreDPort` を追加する。
- `domain` が loopback でない場合は `publicUrl = https://{domain}`、loopback の場合は `http://127.0.0.1:{maitreDPort}`。
- `corsOrigins` は public URL / domain / local Vite origins（5173）の明示 allow-list。ワイルドカードは使わない。
- `nix flake check` は `nixosConfigurations.alhalte-example` を評価し、module の型・assertion・service 定義を検証する。
