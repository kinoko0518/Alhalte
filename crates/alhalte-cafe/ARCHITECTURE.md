# alhalte-cafe

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-cafe`  
> テスト: `crates/alhalte-cafe/tests/cafe_tests.rs`

## 責務

Linux / macOS / Windows 共通のワンショット起動。systemd を経由せずに Alhalte インスタンスを立ち上げる。

- ポータブルフォルダの実行ファイル1つ（`alhalte-cafe` / `alhalte-cafe.exe`）からの起動
- 開発中の試運転（`nix run .`）
- ビルド済み SPA を maitre-d の `--web-root` で配信
- 拡張 wasm の staging（ビルド補助）

設定のパース・秘密復号・引数生成は [alhalte-restaurant](../alhalte-restaurant/ARCHITECTURE.md) を使う。systemd 生成は [alhalte-bistrot](../alhalte-bistrot/ARCHITECTURE.md)、NixOS 常駐サービスは [alhalte-glacier](../../nix/alhalte-glacier/ARCHITECTURE.md)。

## 公開 API / 外部接続点

### CLI

```text
alhalte-cafe [--config Restaurant.toml] [--age-key KEY] [--web-root DIR] [--no-browser]
alhalte-cafe [--config Restaurant.toml] stage-extensions --output-dir DIR
```

- サブコマンドなし = serve。
- `--web-root` 省略時は `<config-dir>/web` → `<exe-dir>/web` の順で SPA を探す。
- `--no-browser` / `ALHALTE_NO_BROWSER` でブラウザ起動を抑制する。

### 主な公開型

- `CafeOptions`、`CafePlan`、`CafeError`
- `prepare_cafe`、`build_cafe_plan`、`run_cafe`
- `detect_lan_ipv4_addresses`、`detect_lan_urls`、`lan_url`

## 起動フロー

1. `--config` → cwd の `Restaurant.toml` → 実行ファイル隣の `Restaurant.toml` の順で設定を解決する。
2. 設定ファイルの親ディレクトリ基準で `data-dir` / `socket-dir` を絶対化する。
3. SPA を解決し、`index.html` を要求する。`nix run .` は Nix ビルドの SPA を `--web-root` で渡す。
4. `alhalte-maitre-d` / `alhalte-cellar` / `alhalte-banquette` を実行ファイル隣から解決する。存在しなければ「フォルダ全体を展開する」ようエラーを出す。
5. 秘密 file を `<data-dir>/secrets/` に 0600 で書き出す。平文 secret は `local-login = true` のときだけ許可する（`ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` でも可）。
6. cellar → maitre-d の順に spawn し、TCP 待受を待つ。maitre-d には `--web-root <解決済み SPA>` を渡す。banquette は maitre-d が部屋ごとに spawn する。
7. ブラウザを開く。macOS は `open`、Windows は `cmd /C start`、その他 Unix は `xdg-open`。
8. Ctrl+C または子の終了まで poll する。Windows は `taskkill /T /F`、Unix は子を kill。graceful な最終 persist は行わない（最大 heartbeat 分の損失）。
9. LAN 公開時（`restaurant.host` が wildcard / 非 loopback）は maitre-d のポートで LAN URL を表示する。

## データ形式

`Restaurant.toml` のスキーマは [alhalte-restaurant](../alhalte-restaurant/ARCHITECTURE.md) を参照。cafe は追加の設定ファイルを持たない。

## 既知の差分・制約

- systemd を使わない。自動再起動・ファイルログ・OS サービス登録はない。
- Vite dev server は起動しない。UI は常にビルド済み SPA を maitre-d が配信する。HMR が必要な場合は開発者が `pnpm dev` を別途使う。
- `--web-root` を明示した場合、相対パスはプロセスの cwd 基準。
- 兄弟バイナリが見つからない場合は起動しない（部分起動しない）。
- `stage-extensions` は `--config` をサブコマンドより前に置く必要がある（clap のグローバル引数）。
