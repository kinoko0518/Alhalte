# alhalte-saignant

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-saignant`  
> テスト: `crates/alhalte-saignant/tests/{log_test,render_test,css_test,escape_test,abi_test,artifact_test}.rs`

## 責務

シーンのログエントリおよび汎用構文木 `Generast` を解釈し、セマンティックな HTML へとレンダリングする。あわせて Material 3 Expressive に準拠したスタイルシートを Structural / Expressive の 2 レイヤ・カテゴリ別で提供する。Rust ネイティブライブラリとしても `wasm32-unknown-unknown` ターゲットの WASM モジュールとしても動作する。

## 公開 API / 外部接続点

### ログエントリの描画

- `LogEntry { content: Option<TinContent>, speaker: Option<TinContent>, timestamp: Option<TinContent>, extra: HashMap<String, TinContent> }`
- `LogEntry::from_map` / `to_map`、`with_content_ast`、`with_speaker_id`、`with_unix_time`
- `render_log_entry(entry, resolver)`、`render_log_entry_with_options`
- `render_log_map` / `render_log_map_with_options`
- `RenderOptions { timestamp_format }`、`TimestampFormat::{Iso8601, Japanese, Standard}`

ログマップにおける duck-typed な標準キーと期待される `TinContent` ラベル:

| キー | 期待するラベル | 欠落・不一致時のフォールバック |
| --- | --- | --- |
| `content` | `alhalte/generast` | 空文字列 |
| `speaker` | `alhalte/generast/character-id` | `Anonymous` |
| `timestamp` | `alhalte/unix-time` | `0`（1970-01-01T00:00:00Z） |

### Generast の描画

- `render_generast(ast, resolver)`、`render_generast_tin(tin_content, resolver)`
- `CharacterResolver` トレイト: `resolve_name(id)` と `resolve_avatar_url(id)` を提供する。クロージャ `Fn(u64) -> Option<String>`、`HashMap<u64, String>`、および `HashMap<u64, (String, Option<String>)>` に対して標準実装を備える。
- `NoopCharacterResolver`: すべての ID に対して `None` を返すデフォルトリゾルバ。

### CSS 生成

- `CssCategory`: `Tokens`、`Log`、`Typography`、`RubyBoten`、`Layout`、`DiceJudgement`、`CharacterStatus`、`PillSpoiler`、`Error`
- `CssLayer`: `Structural`（構造・配置）、`Expressive`（色彩・形状・モーション）
- `category_css(category, layer)`、`structural_css()`、`expressive_css()`、`full_css()`

### WASM ABI

`wasm32` ターゲット向けに以下の C ABI 関数をエクスポートする。現行の ABI バージョンは `2`。

- `alhalte_abi_version() -> i32`: `2` を返す（Rust 定数 `abi::ABI_VERSION` と一致）。
- `alloc(len) -> *mut u8`、`dealloc(ptr, len)`: ホスト・ゲスト間のバッファ受け渡し用メモリアロケータ。
- `render_generast_abi(ptr, len) -> u64`（ABI v1、後方互換用）: `Generast` を格納したコンテナバイト列を受け取り、`NoopCharacterResolver` を用いて描画した HTML の `(ptr << 32) | len` を返す。メンション等はすべて `Anonymous` となる。
- `render_generast_abi_v2(ptr, len, names_ptr, names_len) -> u64`（ABI v2）: 第 1・第 2 引数は v1 と同じ。追加で UTF-8 JSON `{ "<character id>": "<display name>" }` のバッファを受け取り、`HashMap<u64, String>` を `CharacterResolver` として適用する。`names_ptr = null` かつ `names_len = 0` の場合は名前マップなしとして扱う。不正な JSON や `u64` として解釈できないキーは無視し、該当 ID は `Anonymous` にフォールバックする。
- `get_full_css_abi() -> u64`: `full_css()` の UTF-8 バイト列を `(ptr << 32) | len` 形式で返す（v1 / v2 共通）。

v2 では、`alhalte/generast/character-id` によるメンション表示だけでなく、`fluct`（能力値増減）および `assign`（能力値代入）ノード内の対象キャラクター名にも表示名マップが適用される。出力されるすべてのキャラクター名は HTML エスケープされる。

WASM バイナリの再現ビルドとフロントエンドへの配置は以下のコマンドで行う。

```bash
nix run .#saignant-wasm
# または
nix develop -c ./scripts/build-saignant-wasm.sh
```

`scripts/build-saignant-wasm.sh` は `--remap-path-prefix` と `CARGO_INCREMENTAL=0` を指定して `wasm32-unknown-unknown` 向けにリリースビルドし、生成されたバイナリを `web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm` へ配置する。コミット済み WASM バイナリの ABI バージョンおよびエクスポート定義は `tests/artifact_test.rs` で検証される。

## 内部フロー

### ログ 1 件の描画

1. `speaker` からキャラクター ID を抽出し、`CharacterResolver` で表示名とアバター URL を解決する。未解決の場合は表示名を `Anonymous` とし、頭文字によるプレースホルダアバターを生成する。
2. `timestamp` から UNIX 秒を取り出し、ISO 8601 属性値と指定フォーマットの表示文字列を生成する。
3. `content` から `Generast` を復元し、`render_generast` で本文 HTML を生成する。
4. `<article class="saignant-log-entry">` 内にアバター、発言者名、`<time>`、および本文 HTML を組み立てて出力する。

### Generast の描画

- `Generast::String`: HTML エスケープを施して出力する。
- `Generast::Nested`: `meta.label` に応じて対応する HTML 要素とクラスで子要素を包む。
  - `bold` → `<strong class="saignant-bold">`
  - `italic` → `<em class="saignant-italic">`
  - `h1`〜`h4` → 対応する見出し要素
  - `hidden` → スポイラー要素
  - `text-color` / `pill` → `meta` から文字列を取り出し（`TryInto<String>`）、`sanitize_css_color` を通過した安全な色指定のみをインライン style に反映する
  - `ruby` / `boten` / `indent` / `hanging` / `grounded` / `kerning` / `h-in-v` → それぞれ対応する構造と CSS クラスを出力する
- `Generast::Element`: `label` に応じて要素を描画する。
  - `diceroll` → `seed` から `SeededEntropy` を初期化し、`DiceExpr` を決定論的に評価して合計値とロール履歴（trace）を描画する
  - `judgement` → `degree`（成功度段階）、`level_name`、`subject`（技能名等）、`target`（目標値）、および内包する `roll` を描画する
  - `character-id` → `@<表示名>` のメンション要素を描画する
  - `fluct` / `assign` → 対象キャラクターのステータス変動・代入結果を描画する
  - `syntax-error` → 構文エラー表示を描画する
  - 未知のラベル → `saignant-element-unknown` クラスのフォールバック要素を描画する

### WASM 入力のデコードフォールバック

`render_generast_abi` / `render_generast_abi_v2` は、ホストから渡されたバイト列を以下の順序で柔軟に解釈する。

1. `TinContent::from_bytes`（旧 `Tin::from_plaintext`）として読み取り、ラベルが `alhalte/generast` であれば `Generast::from_tin` で復元する。
2. 失敗した場合は拡張機能 I/O 形式 `decode_ext_io`（`(Generast, u64)`）としてデコードを試みる。
3. それも失敗した場合は、ラベルなしの生 postcard `Generast`、次いで生 postcard `(Generast, u64)` としてデコードを試みる。
4. いずれでも解釈できない場合は `syntax-error` 相当のエラー HTML を返す。

## 既知の差分・制約

- Issue #272 に伴い、平文の `{ label, payload }` コンテナは `TinContent`（および `to_bytes` / `from_bytes`、`TryFrom` トレイト変換）として扱う。
- ABI v1 の `render_generast_abi` は後方互換のために残されており、WASM 上でキャラクター名を解決するには ABI v2 の `render_generast_abi_v2` を呼び出す必要がある。
- ABI v2 の名前 JSON は表示名のみを受け取り、アバター URL は解決しない（ログ一覧のアバター表示はクライアント UI 側で `characters` state から直接参照する）。
- XSS および CSS インジェクションを防ぐため、すべてのテキストノードの HTML エスケープと色指定の `sanitize_css_color` 検証を `saignant` 内部で完結させてからクライアントの `{@html}` に渡す。
- `LogEntry.extra` は未知のキーと `TinContent` をそのまま保持するが、HTML 描画には使用しない。
