# alhalte-glacier

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `nix/alhalte-glacier/module.nix`（`flake.nix` の `nixosModules.default`）  
> テスト: `nix eval .#nixosConfigurations.alhalte-example.config.system.build.toplevel.drvPath`

## 責務

NixOS 上で Alhalte を常駐サービスとして宣言的に運用するための NixOS モジュール（`services.alhalte.*`）を提供する。`Restaurant.toml` を介さず、NixOS 標準の `mkEnableOption` / `mkOption` で設定を受け取り、`alhalte-cellar.service`、`alhalte-maitre-d.service`、および `alhalte.target` を構成する。ビルド済み SPA（`argenterie-spa`、公式拡張機能 WASM を同梱）の配信設定も自動で配線する。

## 公開 API / 外部接続点

### NixOS オプション（`services.alhalte.*`）

| オプション | 型 | 既定値 | 内容 |
| --- | --- | --- | --- |
| `enable` | `bool` | `false` | Alhalte サービス群を有効化する。 |
| `package` | `package` | `alhalte.packages.${system}.alhalte` | サーバーバイナリ（`alhalte-cellar`、`alhalte-maitre-d`、`alhalte-banquette`）を含むパッケージ。 |
| `webRoot` | `nullOr path` | `alhalte.packages.${system}.argenterie-spa` | `maitre-d` に `--web-root` として渡す SPA ディレクトリ。`null` の場合は静的配信を無効化する。 |
| `extensionsDir` | `nullOr path` | `${webRoot}/extensions`（`webRoot` 非 null 時） | `maitre-d` に `--extensions-dir` として渡す公式拡張機能ディレクトリ。 |
| `domain` | `str` | `"localhost"` | インスタンスの公開ドメイン名。 |
| `host` | `str` | `"127.0.0.1"` | `alhalte-maitre-d` の待ち受けアドレス。`alhalte-cellar` は常に `127.0.0.1` に固定される。 |
| `publicUrl` | `nullOr str` | `null` | 外部公開ベース URL（省略時は `domain` と `maitreDPort` から自動導出）。 |
| `allowedOrigins` | `listOf str` | `[]` | 追加で許可する CORS オリジンおよび OAuth `return_to` オリジンのリスト。 |
| `dataDir` | `path` | `"/var/lib/alhalte"` | SQLite DB および `cellar` 実体データを格納する永続ディレクトリ。 |
| `socketDir` | `path` | `"/run/alhalte"` | `maitre-d` と `banquette` が通信する UNIX ドメインソケットの配置ディレクトリ。 |
| `user` | `str` | `"alhalte"` | サービス実行用のシステムユーザー。 |
| `group` | `str` | `"alhalte"` | サービス実行用のシステムグループ。 |
| `maitreDPort` | `port` | `8080` | `alhalte-maitre-d` の待ち受けポート。 |
| `cellarPort` | `port` | `8081` | `alhalte-cellar` の待ち受けポート（`maitreDPort` と異なる値であること）。 |
| `antiOracle` | `bool` | `false` | `alhalte-cellar` の `--anti-oracle` を有効化する。 |
| `maxTinSize` | `ints.positive` | `67108864` (64 MiB) | 保存・転送を許可する `Tin` の最大バイト数。`alhalte-cellar` と `alhalte-maitre-d`（および spawn される `alhalte-banquette`）に `--max-tin-size` として渡される（Issue #299）。 |
| `chunkSizeExponent` | `ints.between 1 16` | `8` | `banquette` の 1 チャンクあたりのログ件数指数 $n$（$2^n$ 件）。 |
| `heartbeatIntervalSecs` | `ints.positive` | `240` | `banquette` のハートビート・永続化間隔（秒）。 |
| `localLogin` | `bool` | `false` | アカウント名 + パスワードによるローカル認証モードを有効化する。 |
| `clientId` | `nullOr str` | `null` | Google OAuth クライアント ID（`localLogin = false` 時は必須）。 |
| `clientSecretFile` | `nullOr path` | `null` | Google OAuth クライアントシークレットを格納した実行時ファイルのパス（`localLogin = false` 時は必須）。 |
| `salt` | `str` | —（必須） | アカウント識別子ハッシュ化用のインスタンス固有ソルト。 |
| `serviceTokenFile` | `path` | —（必須） | `maitre-d` / `banquette` / `cellar` 間の内部書き込み認証トークンを格納した実行時ファイルのパス。 |
| `openFirewall` | `bool` | `false` | `maitreDPort` を `networking.firewall.allowedTCPPorts` に追加する。 |

## データ形式

### 生成される systemd ユニットとシステムリソース

- **`users.users.${cfg.user}` / `users.groups.${cfg.group}`**: 専用システムユーザー・グループを作成する。
- **`systemd.tmpfiles.rules`**: `dataDir`（権限 `0750`）および `socketDir`（権限 `0750`）を実行ユーザー所有で作成する。
- **`systemd.targets.alhalte`**: インスタンス全体を束ねるターゲット（`wantedBy = [ "multi-user.target" ]`）。
- **`systemd.services.alhalte-cellar`**:
  - `127.0.0.1:${cellarPort}` で待ち受け、`--data-dir "${dataDir}/cellar"`、`--max-tin-size ${maxTinSize}`、`--service-token-file "${serviceTokenFile}"`（および `antiOracle` 時は `--anti-oracle`）を指定して起動する。
- **`systemd.services.alhalte-maitre-d`**:
  - `alhalte-cellar.service` の起動後に開始し、`--db-path "${dataDir}/maitre-d.sqlite3"`、`--cellar-url "http://127.0.0.1:${cellarPort}"`、`--socket-dir "${socketDir}"`、`--banquette-bin "${package}/bin/alhalte-banquette"`、`--max-tin-size ${maxTinSize}`、`--service-token-file "${serviceTokenFile}"` 等を指定して起動する。
  - `webRoot` が設定されている場合は `--web-root`、`extensionsDir` が設定されている場合は `--extensions-dir` を渡す。
- 両サービスとも `NoNewPrivileges = true`、`ProtectSystem = "strict"`、`ProtectHome = true`、`PrivateTmp = true` のハードニング設定を適用する。

## 内部フロー

### NixOS 評価・アクティベーション時の流れ

1. `services.alhalte.enable = true` のとき、`assertions` によって以下を検証する:
   - `salt` が空文字でないこと。
   - `maitreDPort != cellarPort` であること。
   - `localLogin = false` の場合、`clientId`（非空）および `clientSecretFile` が設定されていること。
2. `flake.nix` の `argenterie-spa` パッケージはビルド時に `alhalte-saignant.wasm` と公式拡張機能 WASM（`alhalte-dsl`、`markdown`、`aozora`、`coc-6`、`coc-7`、`emoklore`）をビルドし、`alhalte-cafe stage-extensions` で `manifest.json` を生成した上で SPA をバンドルする。
3. システム起動時、`alhalte.target` により `alhalte-cellar.service` → `alhalte-maitre-d.service` の順で起動する。部屋への入室・参照が発生すると `maitre-d` が同一サービス cgroup 内で `alhalte-banquette` を起動する。

## 不変条件・エラー

- **Nix ストアへの秘密値混入防止**: OAuth クライアントシークレットと内部サービストークンは文字列オプションを持たず、`clientSecretFile` と `serviceTokenFile`（`/run/secrets/...` 等の実行時ファイルパス）のみを受け取る。これにより、全ユーザーから読み取り可能な `/nix/store` に秘密値が平文で書き込まれることを防ぐ。
- 必須オプションの欠落やポート競合は `nixos-rebuild` 時の `assertions` で即座に検出・拒否される。

## 既知の差分・制約

- `clientSecretFile` および `serviceTokenFile` の実体ファイル配置には、`sops-nix` や `agenix`、あるいは systemd の `LoadCredential` などの外部シークレット管理機構を併用する前提とする。
- TLS 終端（HTTPS / WSS）は `alhalte-maitre-d` 単体では行わず、前段の Nginx や Caddy などのリバースプロキシで行う想定である（その際 `services.alhalte.publicUrl = "https://..."` を設定することで、OAuth `redirect_uri` とセッション Cookie の `Secure` 属性が有効になる）。
