# alhalte-kitchen

> 上位文書: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)  
> 実装: `crates/alhalte-kitchen`  
> テスト: `crates/alhalte-kitchen/tests/{archive_test,query_test,transpile_test}.rs`

## 責務

部屋を固めた `.zip` / `.tar.gz` を読み、scene ごとの発言・入退室・BGM/背景変更を時系列に解き、キャラクターの滞在・移動グラフ `RoomGraph` を構築する。指定した経路または scene 列を `SerializedSession` に直列化し、`alhalte-saignant` を使って HTML5 または EPUB 3.0 に変換する。CLI も提供する。

## 公開 API / 外部接続点

外部接続点はファイルと CLI のみ。ネットワークアクセスはない。

### CLI

```text
alhalte-kitchen <ARCHIVE> [--format html5|epub] [--output PATH]
                [--character ID --from START --to END]
                [--scenes a,b,c] [--room-key HEX] [--title TITLE]
```

- `--format`: 既定 `html5`。
- `--output`: HTML5 は省略時に stdout、EPUB は既定 `output.epub`。
- `--room-key`: 32 バイト（64 hex）の部屋鍵。
- `--scenes`: カンマ区切りの scene ID 列。

### 主な公開型

- `RoomArchive`、`ArchiveScene`、`SceneEvent`、`ParsedSceneEvent`
- `RoomGraph`、`GraphNode`、`GraphEdge`、`NodeId`
- `SerializedSession`、`SerializedScene`
- `EpubOptions`、`EpubTranspiler`、`transpile_to_html5`、`transpile_to_epub`
- `export_room_to_zip`、`export_room_to_tar_gz`

## データ形式

### 入力アーカイブ

- `.zip` は `PK\x03\x04` などで判定する。`.tar.gz` は gzip ヘッダ `1f 8b` で判定する。
- 展開したファイルを Tin としてデコードする。部屋全体は `alhalte/banquette`、scene は `alhalte/banquette/scene`、chunk は `alhalte/banquette/log/chunk`。
- JSON 形式の `RoomData` / `SceneData` / `LogChunk` もフォールバックとして受理する。
- room key が与えられた場合、`0010` の Tin を復号する。

### 出力アーカイブ

`export_room_to_zip` / `export_room_to_tar_gz` は以下を書く。

- `room.tin`: `RoomData` を `alhalte/banquette` で postcard 化した Tin。
- `chunks/{tin_id_hex}.tin`: 各 `LogChunk` を `alhalte/banquette/log/chunk` で postcard 化した Tin。

room key が与えられた場合は `0010` で暗号化する。現状は Zero Nonce を使う。

## 内部フロー

### アーカイブ読み込み

1. ファイル一覧をメモリに読み込む。
2. 各ファイルを Tin としてデコードし、ラベルごとに `RoomData` / `SceneData` / `LogChunk` へ振り分ける。
3. chunk は tin id で map し、scene の `biblio.chunks` と突き合わせる。見つからない場合はファイル名に tin id の hex を含むか、scene のパスで探す。
4. scene が 1 つだけで未対応 chunk が残っている場合は、その scene に割り当てる。
5. scene が 0 件ならエラー。

### イベント解析

`ArchiveScene::parse_all_events` は chunk の各 entry を `SceneEvent` に変換する。

- `character-came` / `character-gone` / `change-bg-img` / `change-bgm` は専用 Tin。
- `alhalte/argenterie/log-record` の JSON Tin は `contentTin`（extension 適用後の `alhalte/generast` Tin bytes）を優先して decode し、無ければ `content` を Generast string として使う。`speakerId` は room の character 名に解決し、`speaker` しか無い旧レコードは名前から character ID を引く。
- `contentTin` は byte array / base64 / Tin JSON（`{label, payload}`）を受理し、`Generast` / `(Generast, speaker_id)` / 生 postcard の順に decode を試す。
- duck-typed map の `type` が `character-came` / `came` / `character-gone` / `gone` なら対応するイベント。
- それ以外は `LogEntry`（`content` / `speaker` / `timestamp`）として扱う。旧形式の postcard map / 直接 Generast / primitive string は従来どおり fallback する。
- timestamp がないイベントは、前後の発言 timestamp から前方・後方補間する。

### グラフ構築

1. 全 scene のイベントを timestamp、scene 内 order で安定ソートする。
2. scene ごとに初回 visit のノード `scene:0` を作る。初期 `present_characters` をそのノードに置く。
3. イベントを時系列に処理する。
   - `CharacterCame`: 直前の滞在ノードから現在ノードへ edge を張る。再訪なら visit index を増やす。
   - `CharacterGone`: 現在ノードからキャラクターを外す。
   - `Speech`: 話者の現在地が別 scene なら暗黙の移動 edge を張る。ログを現在ノードへ追加する。
   - BGM / 背景変更: 現在ノードの時刻範囲を更新する。
4. `character_names` は `std::name`、なければ `Character {id}`。

### 問い合わせと変換

- `find_path_for_character(character_id, start, end)` は BFS で有向経路を探す。
- `serialize_by_character_path` は経路の各ノードを `SerializedScene` にして `SerializedSession` を返す。
- `serialize_by_scene_ids` は指定 scene の全ログを順にまとめる。scene が graph にない場合は元アーカイブから発言だけ拾う。
- `to_html5` は `full_css()` と `render_log_entry` で単一 HTML を作る。
- `to_epub` は mimetype、META-INF、OEBPS、nav、chapter を ZIP に書く。mimetype は先頭・無圧縮。

## 既知の差分・制約

- `RoomArchive` は `done: bool` を持つ。Issue #114 で `RoomData` を `default_permission` へ変える予定だが、main の `RoomArchive` は `done` のままで、issue-114 ブランチでは `room_data.done` 参照が未追従。
- export の room-key 暗号化は Zero Nonce。仕様の「ランダム 96bit Nonce」とは異なる。
- `EpubOptions::default` の `modified` は固定値 `2026-10-17T00:00:00Z`。生成時刻ではない。
- EPUB の chapter は scene 単位。EPUB 3.0 の検証は `epubcheck` をテストで使う。
- グラフの scene 再訪判定はイベント順に依存する。明示的な入退室がない場合は発言 scene の変化で補う。
