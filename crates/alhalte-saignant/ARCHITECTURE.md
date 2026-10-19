# alhalte-saignant

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-saignant`  
> テスト: `crates/alhalte-saignant/tests/{log_test,render_test,css_test,escape_test,abi_test,artifact_test}.rs`

## 責務

scene log を解釈し、`Generast` を含めて意味のある HTML に変換する。CSS はカテゴリ別・2 レイヤ（Structural / Expressive）に分割し、Material 3 Expressive の見た目を当てる。ネイティブライブラリと `wasm32` の両方で動く。

## 公開 API / 外部接続点

### ログ

- `LogEntry { content: Option<Tin>, speaker: Option<Tin>, timestamp: Option<Tin>, extra: HashMap<String, Tin> }`
- `LogEntry::from_map` / `to_map`、`with_content_ast`、`with_speaker_id`、`with_unix_time`
- `render_log_entry(entry, resolver)`、`render_log_entry_with_options`
- `render_log_map` / `render_log_map_with_options`
- `RenderOptions { timestamp_format }`、`TimestampFormat::{Iso8601, Japanese, Standard}`

duck-typed なキー:

| キー | 期待する Tin | フォールバック |
| --- | --- | --- |
| `content` | `alhalte/generast` | 空文字 |
| `speaker` | `alhalte/generast/character-id` | `Anonymous` |
| `timestamp` | `alhalte/unix-time` | 0（1970-01-01） |

### Generast

- `render_generast(ast, resolver)`、`render_generast_tin(tin, resolver)`
- `CharacterResolver` trait。`resolve_name` と `resolve_avatar_url` を持ち、`Fn(u64) -> Option<String>`、`HashMap<u64, String>`、`HashMap<u64, (String, Option<String>)>` に実装済み。
- `NoopCharacterResolver` は常に `None`。

### CSS

- `CssCategory`: `Tokens`、`Log`、`Typography`、`RubyBoten`、`Layout`、`DiceJudgement`、`CharacterStatus`、`PillSpoiler`、`Error`
- `CssLayer`: `Structural`、`Expressive`
- `category_css(category, layer)`、`structural_css()`、`expressive_css()`、`full_css()`

### WASM ABI

`wasm32` 向けに以下をエクスポートする。現行バージョンは `2`。

- `alhalte_abi_version() -> i32`（2）。Rust 定数 `abi::ABI_VERSION` と一致する。
- `alloc(len) -> *mut u8`、`dealloc(ptr, len)`
- `render_generast_abi(ptr, len) -> u64`（v1、互換のため残す）: Generast Tin を受け取り、`NoopCharacterResolver` で HTML を `(ptr << 32) | len` で返す。mention は `Anonymous` のまま。
- `render_generast_abi_v2(ptr, len, names_ptr, names_len) -> u64`（v2）: 入力は v1 と同じ。加えて UTF-8 JSON `{ "<character id>": "<display name>" }` を受け取り、`HashMap<u64, String>` を `CharacterResolver` として使う。`names_ptr = null, names_len = 0` は名前なしとして扱う。JSON が不正、またはキーが decimal `u64` でない場合はそのキーを無視し、該当 mention は `Anonymous` にフォールバックする。
- `get_full_css_abi() -> u64`: `full_css()` を同じ形式で返す。v1 / v2 で変更しない。

v2 は `character-id` の mention と、`fluct` / `assign` のキャラクター名部分に名前を適用する。名前は HTML エスケープして出力する。

ビルドと配置は再現可能にするため、以下を使う。

```bash
nix run .#saignant-wasm
# または
nix develop -c ./scripts/build-saignant-wasm.sh
```

`scripts/build-saignant-wasm.sh` は `cargo build -p alhalte-saignant --target wasm32-unknown-unknown --release` の後、成果物を `web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm` へコピーする。checkout / Cargo / sysroot のパスは `--remap-path-prefix` で固定し、`CARGO_INCREMENTAL=0` でビルドする。commit 済み wasm の `alhalte_abi_version` とエクスポートは `tests/artifact_test.rs` が Rust 定数・クライアント定数と突き合わせる。

## 内部フロー

### ログ 1 件の描画

1. `speaker` から character id を取り、`CharacterResolver` で名前と avatar URL を引く。見つからなければ `Anonymous`、avatar はイニシャル。
2. `timestamp` を UNIX 秒として ISO 8601 と表示用文字列を作る。
3. `content` を `Generast` として復号し、`render_generast` で HTML にする。
4. `<article class="saignant-log-entry">` に avatar、speaker、`<time>`、content を組み立てる。

### Generast の描画

- `String` は HTML エスケープして出す。
- `Nested` は `meta.label` で分岐する。
  - `bold` → `<strong class="saignant-bold">`
  - `italic` → `<em class="saignant-italic">`
  - `h1`〜`h4` → 見出し
  - `hidden` → スポイラー
  - `text-color` / `pill` → `sanitize_css_color` を通した色を style に使う
  - `ruby` / `boten` / `indent` / `hanging` / `grounded` / `kerning` / `h-in-v` → 対応する class
- `Element` はラベルで分岐する。
  - `diceroll` → seed から `SeededEntropy` を作り、式を評価して出目と trace を表示
  - `judgement` → `degree`、`level_name`、`subject`、`target`、`roll` を表示
  - `character-id` → `@name` の mention
  - `fluct` / `assign` → 能力値の変動・代入
  - `syntax-error` → エラー表示
  - 未知ラベル → `saignant-element-unknown`

### CSS

`css/structural/*` は構造・位置・強調の正しさ、`css/expressive/*` は Material 3 Expressive の色・形・elevation・motion を担う。`full_css()` は Structural の後に Expressive を連結する。

### WASM

`render_generast_abi` / `render_generast_abi_v2` は以下を順に試す。

1. 入力を `Tin::from_plaintext` として読み、ラベルが `alhalte/generast` なら `Generast::from_tin`。
2. 失敗したら `decode_ext_io`（`(Generast, u64)`）を試す。
3. それも失敗したら生の postcard `Generast`、次に `(Generast, u64)` を試す。
4. それでも失敗したら `syntax-error` の HTML を返す。

v2 は名前 JSON を先に復号し、`HashMap<u64, String>` を resolver として同じ描画経路へ渡す。v1 は `NoopCharacterResolver` を渡す。

## 既知の差分・制約

- v1 `render_generast_abi` は互換のため `NoopCharacterResolver` のまま残す。WASM 経由で名前を解決するホストは v2 を使う必要がある。
- v2 の名前 JSON は表示名だけを受け取る。avatar URL は解決しない（ログ行の avatar はクライアントが別途 `characters` から引く）。
- `render_generast_abi` は入力の Tin が wire 形式（opener 付き）か平文かを厳密に区別しない。`Tin::from_plaintext` を先に試すフォールバック順で吸収している。
- HTML エスケープと CSS 色の sanitize は必須。`{@html}` に渡す前に saignant 側で処理する。
- `LogEntry` の `extra` は未知キーを保持するが、描画には使わない。
