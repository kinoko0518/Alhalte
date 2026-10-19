# AGENTS.md — Alhalte 実装エージェント向け規約

## 正とするもの
- 仕様はルート [ARCHITECTURE.md](ARCHITECTURE.md)（全体図・横断規約）と各モジュールの `ARCHITECTURE.md`（責務・公開 API・内部フロー）が正。Issue と矛盾する場合は該当文書を優先し、PR 本文で指摘すること。
- 仕様に書かれていない判断をした場合は、PR 本文の「判断した点」に必ず列挙する。各 ARCHITECTURE.md は勝手に書き換えない。

## 環境
- NixOS。すべてのコマンドは `nix develop -c <cmd>` で実行する（flake.nix の devShell に必要なツールが揃っている）。
- ツールが足りなければ `flake.nix` に追加してよい（その旨を PR に書く）。
- 作業は現在の worktree の中だけで行う。他の worktree、リポジトリ外のファイル、`~` 配下の設定は変更しない。
- Issue campaign 中に限り、リポジトリ内の `.worktrees/issue-<N>` に per-issue worktree を作成してよい。各エージェントは自分に割り当てられた worktree だけを変更し、main worktree と他の worktree には触れない。`.worktrees/` は `.gitignore` に追加済み。

## レイアウト
- Rust ライブラリ / バイナリ: `crates/<crate名>`（crate 名は ARCHITECTURE.md の見出しどおり。例: `crates/tin-rs` の package 名は `tin`、`crates/alhalte-banquette`）
- 拡張機能 wasm: `extensions/<名前>`
- Web: `web/<パッケージ名>`（pnpm workspace）
- 依存のバージョンは各 crate の Cargo.toml に直接書いてよい。共通の内部 crate は path 依存。

## コーディング規約
- Rust edition 2024、`unsafe` は FFI 境界以外で禁止。
- 公開 API には rustdoc を書く。エラーは `thiserror` で型付けし、ライブラリで `unwrap`/`expect`/`panic!` を使わない（テストは可）。
- 仕様の各要件に対応するテストを書き、テスト名またはコメントで ARCHITECTURE.md の該当箇所を示す（例: `// spec: tin-rs/opener 0010 nonce`）。
- 外部ネットワークに依存するテストを書かない。

## 完了前チェック（すべて通すこと）
```
nix develop -c cargo fmt --all
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo test --workspace
```
Web を触った場合は追加で `nix develop -c bash -c 'cd web && pnpm install && pnpm -r test'`。

## Git
- 1 Issue = 1 ブランチ `issue-<N>` = 1 PR。`main` へ直接 push しない。
- コミットメッセージは Conventional Commits（`feat(tin): ...`）。
- force push はしない。
