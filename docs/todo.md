# wml2viewer TODO

- project wml2viewerに関するもののみ
- `SPEC.md`と`state.md`も参照


ステータス
- [x] 確認済み / 安定実装
- [+] 実装済み / 今後の拡張余地あり
- [*] 実装済みだが要再確認 or 既知の不具合あり
- [-] 設計保留
- [ ] 未実装

P0 = 至急
P1 = 優先度高い
P2 = 優先度やや高い
P3 = 優先度中
P4 = 優先度やや低い
P5 = 優先度低い

最終整理日: 2026-03-29

# 0.0.20
## 優先度1
- [ ] I/Oストリームの改善(zipのパフォーマンスの改善。FileSystemが面倒をみる。)
    - [*] 優先度によるI/O調停
      - viewer / companion / preload が同時に archive を触る burst を抑える
      - まずは render worker 側で `primary > companion > preload` の優先度を持たせる
      - thumbnail worker も render worker と同じ low-I/O gate に乗せ、primary / companion 中は `Preload` 相当の低優先度として扱う
      - browser / filer scan も primary load 中は低優先度として扱い、古い request は worker 側で途中キャンセルできるようにする
      - 現状の問題点:
        - render(primary/companion/preload) が別 worker のままで、archive read queue が一本化されていない
        - manga mode は `current + companion + preload` が絡み、通常表示より I/O burst しやすい
        - filer / thumbnail は低優先度化したが、visible 時の direct request は still UI 側から個別に発火する
      - 次にやること:
        - zip/tiff だけでも `current > companion > preload > filer > thumbnail` の単一 queue に寄せる
        - companion と preload の read path を統合し、同じ page の二重 open をなくす
        - thumbnail hint は worker 側で drop できるようにし、viewer 側の後段判定に頼らない
    - [*] ロードキャンセル/SKIPの実装(重いzip対策)
      - まずは zip 読み出し中に古い companion / preload を早めに打ち切れるようにする
      - ただし cancel は「別 worker が既に read 開始済み」の分までは止めきれていない
      - queue 一本化前提で cancel / skip を整理し直す
    - [ ] ワークアラウンドの削除
- [ ] キーバインドのカスタマイズ 
  - [ ] デフォルト・キーバインドの変更
  - [ ] 設定UI
- [ ] フォントサイズのデフォルトがSになっているので自動に変更
- [ ] 右クリックメニューの変更
    ```
    ファイル -> 再読み込み/移動/コピー/削除
    画像 -> フォーマットを変更して保存
    表示 -> 拡大/縮小/倍率を指定/原寸大/漫画モードのオン・オフ
    画像の情報 -> metadataを整形して表示
    設定 -> 設定画面へ
    このプログラムについて 
    ```
- [ ] ファイラーメニュー move, copy ,delete(trash), rename
- [ ] ファイラー／サブファイラー／サムネイルの表示 なるべく固定位置に
- [ ] ファイルが最後につくと自動でファイラーが開くのをデフォルトでオフに変更
- [ ] issue: zipファイルの途中で終わるとそのファイルではなくファイラーが開く問題

## 優先度2
- [+] Viewer/Filerがバラバラで持っているファイルスキャンの統合 I/Oストリームの改善
- [ ] LHAサポート(zipと同じレイヤーに置く)
- [ ] フォルダの内容更新
    - [x] [F5]の手動更新(default)
    - [ ] 自動更新 
    - [ ] 設定による切り替え
- [ ] 画像切り替時のエフェクト スライド(右左、左右、上下、下上、渦巻き消去／表示、フェイドイン／アウト)
- [*] ネットワークにおけるファイルコレクションの高速化

### UI
- [ ] アイコンの見直し
- [ ] メニューの名称の見直し
- [ ] UIの各種不具合の修正（大半がネットワーク共有フォルダに関するもの）
- [ ] 画像切り替時のエフェクト スライド(右左、左右、上下、下上、渦巻き消去／表示、フェイドイン／アウト)

### input/key events/mouse events(P1)
- [ ] イベントと入力バインドの分離。 Key Remapping UIの準備
- [ ] スクロールを移動に割り当てられる様にする

### INPUT
- [ ] P4 タッチパネル(android対応用)

### FileSystem

#### 仮想ファイル
- [*] キャッシングアルゴリズムの見直し/実装
    - [+] 既存の個別キャッシュ
      - zip index cache / ZipCacheReader chunk cache / local archive cache
      - browser/navigation shared listing cache / metadata cache / persistent snapshot
      - viewer preload / thumbnail in-memory cache
    - [*] provider protocol を統合する
      - local fs / listed / zip / temp-http を同じ source id / signature / open API で扱う基盤は追加済み
      - render worker / thumbnail worker は filesystem source open API を経由する
      - URL入力と app 起動入力は filesystem source input resolver を経由して temp-http へ落とせる
      - URL入力は FilesystemCommand / FilesystemResult 経由で worker thread から解決する
      - ResolveSourceInput は main filesystem worker を止めずに別スレッドで解決する
      - 新しい URL resolve request が来たら古い結果は捨てる
      - URL resolve request は CancelSourceInput で明示 cancel できる
      - temp-http は URL -> temp file の session cache を持つ
      - temp-http は URL hash ベースの persistent cache file と sidecar metadata(etag/last-modified) を持つ
      - stale な temp-http cache は conditional request で再検証し、304 なら再利用する
      - zip / listed / local fs / smb / http / cloud drive を同じ source key / metadata / open API で扱う
      - 現在は zip/listed/local fs/http まで統合が進行。smb は OS path 任せ、cloud drive は未対応
    - [*] 先読み（漫画モード加味すると最大2枚）
      - next preload と manga companion load はある
      - filesystem cache policy と統合された source-level prefetch queue にはなっていない
    - [*] インメモリキャッシュ
      - listing / metadata / zip chunk / preload / thumbnail は個別実装あり
      - source 単位の共通 cache budget / eviction は未統合
    - [ ] kvsベース
      - remote body / archive local copy / thumbnail / metadata snapshot を同じ永続 cache backend で扱う
    - [*] 更新されていないかだけチェック
      - filesystem persistent cache / zip index cache / local archive cache は path + size + mtime 相当で再検証する
      - path + size + mtime + provider-specific version(etag等) の signature ベースへ寄せる
      - http は etag/last-modified の再検証まで対応済み
      - cloud 側の provider-specific version 検証は未実装
    - [*] 速度が見込める場合はキャッシュしない
      - 現在は zip workaround の閾値 / network path 判定 / low-I/O archive の preload 抑制のみ
      - provider 共通の no-cache policy は未実装
    - [*] 古いファイルはちゃんと消す(LRUアルゴリズム)
      - 現在は ZipCacheReader chunk cache のみ限定的 LRU
      - archive-cache / filesystem-cache / thumbnail-cache の容量上限と世代管理は未実装
- [ ] LHAサポート
- [*] httpの暫定実装
    - [+] reqwest blocking download -> temp file 化で表示
    - [+] UI の URL open と app 起動入力は filesystem source resolver 経由に移行
    - [+] UI の URL open は filesystem protocol を経由して worker thread で解決
    - [+] URL resolve は main filesystem worker loop と分離
    - [+] 新しい URL request が来た場合は stale result を返さない
    - [+] 新しい URL request が来た場合は古い request を cancel する
    - [+] 同一 URL は session 中 temp file を再利用する
    - [+] URL hash ベースの persistent cache file を使い、一定時間は再取得しない
    - [+] stale cache は etag / last-modified で再検証する
    - [+] URL 解決中は shared filesystem cache lock を握らない
    - [+] filesystem provider として worker protocol / persistent cache / revalidate / cancel を統合

#### UX
- [ ] Windows Userフォルダをエクスプローラっぽい表示に擬態するモード
    - User home
      - Desktop (CSIDL_MYDOCUMENTS)
      - Documents (CSIDL_MYDOCUMENTS)
      - Music (CSIDL_MYMUSIC)
      - Pictures (CSIDL_MYPICTURES)
      - Video (CSIDL_MYVIDEO)
      - Downloads
      - Favariies (CSIDL_FAVORITES)
- [ ] フォルダの内容更新のタイミング([F5]の手動更新 / 自動更新)


### Setting
- [ ] 表示名と配置の見直し(作業中)

## 最終確認
- [ ] wml2viewer/docs/todo.mdの更新
- [ ] wml2viewer/README.ja.mdとwml2viewer/README.mdの更新

## issues
- [x] P0 issue: 漫画モードの体感速度と挙動が現行で明確に劣化している
   - 単ページ表示より見開き表示の方が遅い状態を解消する
   - `companion` / `preload` / `next-prev` / `filesystem navigation` の責務を分離して切り分ける
   - `viewer` の通常モードを壊さないことを優先し、漫画モードだけを独立して再設計する
   - まずは `companion` 表示の成立条件と load path を固定し、その後で `next/prev`、最後に `preload` を戻す
   - 実機で `通常モード`, `漫画モード`, `zip`, `展開画像` を比較して回帰確認する

  - [ ] コメント：漫画モードって2枚先読みするだけでしょ。後はレンダーの仕事、単純化して実装できないの？

- [ ] Viewer/Filerがバラバラで持っているファイルスキャンの統合 I/Oストリームの改善
   - [+] directory scan / preview chunk / filter / metadata / sort を `filesystem.browser` へ寄せた
   - [+] `FilerCommand / FilerResult` 自体を filesystem 側の query/result モデルへ統合
   - [ ] source provider protocol の統合
    - local fs / listed / zip / temp-http の source key / signature / open API は導入済み
    - URL open / app startup input も filesystem 側の source resolver を通る
    - URL open request は filesystem worker protocol に乗っている
    - temp-http は session cache / persistent cache / etag revalidate / cancel まで導入済み
    - zip / listed / local fs / smb / http / cloud drive を同じ key / metadata / open API へ寄せる
   - [+] viewer の navigation cache と filer の browser scan cache を共有化
   - [+] filer のファイルリスト更新を viewer の current/pending navigation と同期
   - [+] filer の snapshot state (`directory / entries / selected / pending_request_id`) を `filesystem.browser` へ寄せた
   - [+] filer options を worker 永続 state として扱い、directory sync では差分更新だけ送る
   - [+] thumbnail worker 向け query hint を browser result から返す
   - [+] metadata cache を browser/navigation 共有 cache に統合し、永続化した
   - [ ] Filerのファイルリストがアップデートしたとき viewerに反映されない問題(データの同期) 
    - [ ] 油断していると画像の最初に飛ばされる
   - [+] 大規模フォルダ向け lazy load / incremental snapshot を filesystem 側の共通実装へ寄せる
   - [ ] thumbnail の共通永続キャッシュ層を追加
    - filesystem の共通 KVS / signature / eviction policy に乗せる
### startup sequence
- [*] issue: Explorer統合時 Command Lineが表示される問題(println!, eprintln!が悪い？ shell統合時はstdioをcmdに出さない改善)
- [*] issue: systemプラグイン有効時 Viewerの強制終了時 COM Surrogateが残ることがある(再現条件を確認中)
- [*] startup sequenceの見直し(完全な実装はbeta以降だが、初めからステートマシンの組み替えができるように考慮すること)
- [*] viewerワーカーの起動を再優先して `current_texture` のみ作る
- [*] 最初の画像をロードして単体ビューアーモードで表示する
- [ ] 各ワーカーを生成してから最初の画像を表示する
- [*] 各ワーカーを同期してマルチプル・ビューアーモードに切り替える
### code
- [ ] `src/ui/viewer/mod.rs` の state 分離を進めて `ViewerApp` をさらに薄くする
- [ ] `src/filesystem/browser.rs` の lazy load / incremental snapshot をさらに進めて大規模フォル

### Filesystems
- 非 network-aware format の I/O バースト対策
    - [ ] P0 issue: viewer の表示優先制御を再設計する
      - 対象は当面 `zip` と `tiff`
      - 初回表示で I/O を集中させない
      - 2枚目以降でも burst read / decode を起こさない
      - filer / thumbnail / preload / manga companion の副作用を切り分ける
      - network path では「表示優先」で段階的に後続処理を走らせる
    - [ ] P0 issue: viewer の漫画モードを通常表示と切り離して再設計する
      - `current` と `companion` の表示成立を先に固定し、`preload` は後段に回す
      - 漫画モード専用の同期探索と状態キャッシュを通常モードへ漏らさない
      - regressions を `next/prev`, `companion`, `preload` に分けて個別に確認する
    - [ ] zip: 巨大 archive の最初の1枚表示に極端に時間がかかる
      - 1.6GB zip で初回表示が 40 秒級になるケースあり
      - first page fast path / central directory open / 初回 index 構築のどこが支配的か再計測する
      - viewer 単体で最初の1枚を優先し、filer / thumbnail / companion / preload の副作用を抑える
    - [ ] zip: 2枚目以降が回収前よりもっさりしている
      - preload / branch navigation / companion / texture reuse の回帰が混ざりやすい
      - zip 内 next/prev, preload, manga companion を同じ軽量経路に揃えて再設計する
    - [ ] zip: ナビゲーションと漫画モードが速度改善のたびに壊れやすい
      - zip virtual child の next/prev と manga companion を viewer 側の場当たり分岐で持たない
      - navigation policy を filesystem 側へ寄せ、zip/listed/dir で共通に扱えるようにする
    - [ ] zip/tiff: 時間のかかる展開時に viewer 側が固まる問題
    - [*] zip: `bench_archive` 1.6G ベンチが安定して終わらない。bench の結果は `test\benchmarklog.txt` `.\test\bench.bat` で実行可能
    - [+] zip: zip crate は `BufRead` で 8KB のキャッシュしか効かないので、`ZipCacheReader` をラップして改善できるかチェック　`zipreader.md` 参照
    - [*] zip: 起動時の引数に zip を選ぶとナビゲーションできなくなるバグ(Filerで選択できる)
    - [*] zip: ローカルキャッシュ方針を見直す
      - 現在は bool (`local_cache`) で on/off のみ
      - provider 共通 cache policy 導入後に size budget / no-cache 条件へ置き換える
    - [ ] tiff: network 上での burst read / decode 対策
    - [ ] crate oxiarc-lzhufで、lzhアーカイブ対応 feature LHA で実装
    - [ ] listed file(.wmltxt)でhttpが表示出来ない問題

### syetem
- [+] LinuxはWSLでbuild。実行はVMで行う 現在buildは OK 起動もOK
- [-] MacはIntel MACの環境しかないので遅延 

- [ ] todo.mdの更新
- [ ] wml2viewerのREADME.ja.mdとREADME.mdの更新

### renderer
- [*] 精密モードが予想以上に重い(変換プロセスが何度も走っていないかチェック　-> アルゴリズムチェック)　[精密]モードが効くサイズは6000px越えが多いのでより遅い
- [ ] issue: subfiler の RightToLeft 表示は「右端開始」にしたいが、右端固定追従にはしない
  - 表示順や通常スクロールは壊さず、初期表示時だけ右端寄せする
  - `stick_to_right()` のような継続追従は再描画を増やすので使わない

### 非 network-aware format(P1)
- [+] zip: 時間のかかる zip 展開時に viewer 側が固まる問題 config のリセットで改善
- [ ] issue: viewer の zip / tiff の体感速度を再チューニング
  - 巨大 archive では first page fast path、通常サイズでは 2 枚目以降の応答性を優先して評価する
  - `bench_archive` だけでなく viewer 実機操作で比較する

### filer(P4)
- [ ] まれに固まる事がある フォルダに問題があるのかfilerに原因があるのか調査中
- [x] 拡張子wmlをwmltxtに変更
### Others
- [ ] コードのフルレビュー

### example(P3)
- [ ] bench_archive 1.6GBだと固まる問題

## 繰り返し
- [x] Windowsの動作チェック
- [+] Ubuntuの動作チェック
- [ ] 境界条件の動作チェック
- [ ] wml2viewer/resources/help.htmlのチェック
- [ ] wml2viewer/README.ja.md README.md の更新
- [ ] wml2viewer/docs/todo.mdのチェック


# モジュールベースの整理
## src/main.rs / src/app.rs
- [x] `wml2viewer <file>` 起動
- [x] `wml2viewer <directory>` 起動
- [x] 起動時に空表示の場合はファイラーを開く
- [x] 起動時ウィンドウサイズを設定から復元
- [x] 起動時ウィンドウ位置を設定から復元
- [x] 設定未指定時は起動スクリーン基準で 60% サイズ + 中央寄せ
- [x] 起動時 fullscreen を無効化するワークアラウンド
- [x] アプリアイコン設定
- [x] `resources/help.html` 出力の土台
- [+] app 起動時の初回 decode worker 化
- [+] startup を single-viewer -> sync -> multi-viewer で進める土台
- [+] 最初の画像を優先し、filesystem 初期化は表示後に同期
- [x] `--clean system`
- [x] `--clean cache`
- [-] 二重起動の制限は一旦取り下げ
- [*] フルスクリーン復帰時の安定性確認
- [+] example / benchmark 用の lib 化
- [ ] 拡張子をwml2本体から取得

## src/options.rs
- [x] ViewerAction / KeyBinding の整理
- [x] `Shift+G` grayscale toggle
- [x] `Shift+C` manga mode toggle
- [x] `Shift+V` subfiler toggle
- [x] `Ctrl+S` 保存ダイアログ起動
- [x] `F1` help 起動
- [ ] キーリマップ UI

## src/configs/config.rs
- [x] config load/save
- [x] startup path load/save
- [x] config import/export
- [x] `--config [path]`
- [x] window / render / resources / navigation の永続化
- [+] resources.font_paths の永続化
- [x] storage.path / storage.path_record の永続化
- [x] manga separator / UI theme の永続化
- [x] plugin config の永続化土台
- [x] workaround.archive.zip の永続化
- [+] filesystem.thumbnail 設定値の永続化
- [ ] config schema のバージョニング

## src/configs/resourses/mod.rs
- [x] システムロケール検出結果を resources へ適用
- [x] `ja_JP.UTF-8 -> ja_JP` 正規化
- [x] `ja_JP -> ja -> en` フォールバック
- [x] `zh_TW -> zh -> en` フォールバック
- [x] locale 別の system font 候補
- [+] locale system font を最優先にしつつ override font を前置
- [x] emoji font fallback
- [+] CJK / 顔文字向け fallback 強化
- [x] `Auto / S / M / L / LL` のフォントサイズ
- [x] DPI / 画面サイズベースの Auto サイズ
- [x] 外部 JSON resource 読み込み

## src/configs/resourses/english.rs
- [-] 外部 resource ローダ導入時に役割を再整理

## src/configs/resourses/japanese.rs
- [-] 外部 resource ローダ導入時に役割を再整理

## src/dependent/mod.rs
- [x] OS 依存 API の窓口整理
- [x] root drive 一覧取得の UI 用ラッパ
- [x] 保存先フォルダ選択ダイアログの窓口
- [*] http/https 共通ダウンロード窓口（reqwest）
  - blocking download -> temp file 化の暫定対応
  - provider protocol へは未統合

## src/dependent/thirdparty/locale_config.rs
- [x] locale 正規化ヘルパ
- [x] resource locale fallback ヘルパ

## src/dependent/thirdparty/directories.rs
- [x] 設定ディレクトリ解決
- [x] 既定ダウンロードディレクトリ解決
- [x] 共通 temp ディレクトリ解決

## src/dependent/windows/mod.rs
- [x] Windows locale 取得
- [x] Windows 向け日本語/繁体字フォント候補
- [x] Windows emoji font 候補
- [+] `%LOCALAPPDATA%\\Microsoft\\Windows\\Fonts -> %WINDIR%\\Fonts` の検索順
- [x] Windows drive 列挙
- [x] フォルダ選択ダイアログ
- [+] 拡張子関連付け登録
- [+] 拡張子関連付け clean
- [x] winres による exe icon resource 登録

## src/dependent/linux/mod.rs
- [x] locale 環境変数取得
- [*] Linux font fallback 候補（数字が出ない）
- [+] `available_roots` 実装
- [*] build
- [ ] フォルダ選択ダイアログ

## src/dependent/darwin/mod.rs
- [x] locale 環境変数取得
- [x] macOS font fallback 候補
- [+] `available_roots` 実装
- [*] build
- [ ] フォルダ選択ダイアログ

## src/dependent/android/mod.rs
- [ ] Android 依存実装

## src/dependent/ios/mod.rs
- [ ] iOS 依存実装

## src/dependent/other/mod.rs
- [x] その他 OS 向け最低限の fallback

## src/dependent/plugins/mod.rs
- [x] plugin config 構造体
- [x] provider 別 default 設定
- [x] plugin 設定 UI 向けの土台
- [x] plugin / internal priority 設定
- [x] search path からの module 走査
- [+] plugin 優先順位の実行ロジック
- [x] MIME / wildcard 判定
- [+] decoder の実行ロジック
- [+] plugin 有効拡張子の列挙
- [ ] encoder / filter の実行ロジック

## src/dependent/plugins/system.rs
- [+] system provider の既定値
- [+] Windows WIC decode 実装
- [ ] macOS system codec 実装

## src/dependent/plugins/ffmpeg.rs
- [+] ffmpeg provider の既定値
- [+] external ffmpeg 実行による decode

## src/dependent/plugins/susie64.rs
- [+] susie64 provider の既定値
- [+] Windows 専用ロード
- [+] image plugin decode 実行
- [ ] archiver plugin 実行

## src/filesystem/mod.rs
- [x] 単一ファイル起動時に親ディレクトリの画像一覧を取得
- [x] `STOP` / `NEXT` / `LOOP` / `RECURSIVE`
- [x] filesystem worker 分離
- [x] directory 単位 cache
- [x] `.wml` / `.zip` を browser container として扱う
- [x] fileviewer から zip の中身を辿れる
- [x] sort order `os_name` / `name` / `date` / `size`
- [+] plugin 有効拡張子を filer / viewer に反映
- [+] browser query を `filesystem.browser` へ分離
- [+] virtual path / worker / navigator / cache / sort をモジュール分割
- [*] `RECURSIVE` の探索コスト最適化
- [x] filter 条件の filesystem 側統合
- [x] archive option (`FOLDER` / `SKIP` / `ARCHIVER`)
- [x] キャッシュのシリアライズ
- [x] browser query と navigation cache の共有化
- [x] metadata cache の共有化
- [x] filesystem 側での query/result protocol の一本化

## src/filesystem/browser.rs
- [+] filer の scan / preview / filter / sort / metadata 収集を集約
- [+] virtual directory と real directory の incremental snapshot を同じ経路へ統合
- [+] incremental snapshot 中の cache lock を listing snapshot 取得までに短縮
- [+] 大規模フォルダ向け lazy load を filesystem worker と共有
- [+] filter 条件を filesystem worker 側の永続 state として扱えるように整理
- [+] thumbnail worker と連動する query hint の追加

## src/filesystem/listed_file.rs
- [+] `.wmltxt` 判定
- [x] 相対 path 基準を ListedFile 親ディレクトリにする
- [x] コメント行 `#` を無視
- [-] `@command` / `@(...)` の本実装

## src/filesystem/zip_file.rs
- [x] zip 読み込み
- [x] zip virtual child path
- [x] 途中 entry エラーを飛ばして継続
- [x] zip 名の SJIS fallback decode
- [+] zip entry 自然順ソート
- [+] BufReader ベースの再読込
- [+] 大容量 / ネットワーク zip の low-I/O workaround
- [+] temp へのローカル archive cache
- [+] ZipCacheReader を使った chunk cache
- [+] metadata 読み取り時の plain file fallback
- [+] tail prefetch
- [+] benchmark で計測できる形に整理
- [*] source provider protocol への統合
  - source id / signature ベースの判定へ移行開始
  - open/fetch API の共通化は未実装
- [*] local archive cache の signature 検証と eviction policy
  - signature 検証までは実装済み
  - eviction policy / size budget は未実装
- [ ] zip encoding option
- [ ] `7z` / `rar` / `lzh` / `gzip`

## src/ui/mod.rs
- [x] viewer / render / input / menu / i18n の分離

## src/ui/i18n/mod.rs
- [-] `configs/resourses` への shim のみ

## src/ui/input/dispatch.rs
- [x] key/pointer から action 解決
- [ ] 未実装 action の no-op 整理
- [ ] 動的入力割当

## src/ui/input/mod.rs
- [x] egui input から viewer action dispatch
- [x] settings 表示中は viewer 入力を止める
- [x] text input 中は viewer shortcut を止める
- [x] `P` で settings を閉じる
- [+] 左クリックで settings 表示
- [+] 右クリックで次画像
- [+] 右ダブルクリックで fit toggle
- [+] 中クリックでメニュー
- [x] `F1` help
- [ ] タッチ UI

## src/ui/menu/mod.rs
- [x] menu 名前空間の分離

## src/ui/menu/config/mod.rs
- [x] 設定画面の土台
- [x] viewer / render / window / navigation / plugins / resources タブ
- [x] `Apply` / `Cancel` で閉じる
- [x] staged apply
- [x] manga separator 設定
- [x] window theme 設定
- [+] plugin 設定画面の土台
- [x] plugin / internal priority 編集 UI
- [+] plugin search path 編集
- [+] plugin search path フォルダ選択ダイアログ
- [+] plugin module load test ボタン
- [+] plugin 変更時の再起動推奨ポップアップ
- [x] save path 記録設定
- [x] 適用/undo/初期化ボタン
- [+] 拡張子関連付けボタン
- [x] 設定画面の主要文言リソース化
- [+] workaround.archive.zip 設定 UI
- [+] thumbnail 抑制オプション
- [+] navigation.sort 変更時の filesystem/filer 再同期
- [ ] キーバインド編集 UI

## src/ui/menu/fileviewer/functions.rs
- [ ] Copy
- [ ] Move
- [ ] Trash
- [ ] Convert
- [ ] Similarity

## src/ui/menu/fileviewer/state.rs
- [x] filer state の分離
- [x] root drive 管理
- [x] view mode / sort / filter / URL input state
- [+] thumbnail size 可変 state
- [x] `available_roots` の曖昧 import 解消

## src/ui/menu/fileviewer/icons.rs
- [x] resources/icons の SVG を UI 描画へ接続
- [x] background 反転色での icon 描画
- [ ] SVG icon の共通化と他 menu への展開

## src/filesystem/browser.rs / src/ui/menu/fileviewer/state.rs
- [x] browser query/result を viewer から直接利用
- [x] directory scan の worker 分離
- [+] metadata 収集を `filesystem.browser` へ移動
- [+] sort / filter / ext filter / dir separate を `filesystem.browser` へ移動
- [x] 数値を含む自然順ソート
- [+] incremental snapshot preview
- [+] lazy load の段階化
- [ ] OS 準拠 name collation の強化
- [x] worker の thin adapter を削除して filesystem query へ直接接続

## src/ui/menu/fileviewer/thumbnail.rs
- [x] サムネイル worker
- [x] virtual zip/listed file のサムネイル生成
- [+] 巨大 zip bmp thumbnail の抑制
- [+] thumbnail抑制オプション
- [ ] 共通 KVS ベースの永続キャッシュ
- [ ] signature 付き失敗キャッシュ

## src/ui/menu/fileviewer/mod.rs
- [x] 一覧表示
- [x] サムネイル表示（小・中・大）
- [x] サムネイル格子グリッド表示
- [x] 詳細表示
- [x] 表示切り替えボタン
- [x] view / sort / dir separate をボタン化
- [x] metadata 表示
- [x] 昇順/降順切り替え
- [x] 名前/更新日時/サイズソート
- [x] フォルダとファイルを混ぜる/分ける
- [+] zip を folder/file のどちらとして分離ソートするかの切り替え
- [x] ファイル名部分一致フィルタ
- [x] 拡張子フィルタ
- [x] ドライブ選択
- [x] zip / archive の内容表示
- [x] URL 入力欄（http/https は reqwest ダウンロードで表示）
- [x] SVG アイコン素材を resources/icons に生成
- [x] SVG アイコンを UI に実表示
- [x] toolbar 文字ボタンの icon 置換
- [x] サブファイラー下部表示
- [x] サブファイラー閉じるボタン
- [x] 詳細表示で更新日時とサイズを表示
- [+] 更新日時のローカル時刻表示
- [x] ファイル選択時に filer を閉じる
- [x] サムネイルのフォルダ/アーカイブ icon 縮小
- [x] サムネイル中央の不要な button chrome 削減
- [x] サムネイルペインサイズ可変
- [+] 長いファイル名の中間省略（末尾 7 文字優先）
- [*] filer 表示時のさらなる高速化

## src/ui/render/layout.rs
- [x] 背景描画
- [x] 中央寄せ offset 計算
- [x] manga spread のレイアウト補助

## src/ui/render/texture.rs
- [x] texture upload 補助
- [x] texture size 制限時の downscale
- [ ] 分割 texture による巨大画像対応

## src/ui/render/worker.rs
- [x] render worker
- [x] load / resize request 分離
- [+] 単発 preload queue 連携

## src/ui/render/mod.rs
- [x] viewer から render 責務を切り出し
- [*] 変換パイプラインの追加整理

## src/ui/viewer/options.rs
- [x] viewer / render / window option struct
- [x] grayscale option
- [x] manga option
- [x] manga separator option
- [x] window ui theme option

## src/ui/viewer/animation.rs
- [x] アニメーション表示の基礎
- [ ] source-level prefetch queue との統合

## src/ui/viewer/mod.rs
- [x] ViewerApp が composition root として worker を束ねる
- [*] 画像 state と viewer state の完全分離
- [+] save / overlay の transient state 分離
- [x] render worker / filesystem worker / filer worker / thumbnail worker を統合
- [x] filer に引きずられない viewer 更新
- [x] manga mode の中央寄せ
- [x] manga mode でフォルダ跨ぎ時の FitScreen 再計算
- [x] resize イベントに寄せた FitScreen 再計算
- [x] filer から画像選択後に次画像移動できる
- [x] filer から画像選択後に FitScreen を再適用
- [x] 保存ダイアログ（保存先フォルダ選択 + 形式選択 + 名前変更）
- [x] 既定ダウンロードフォルダの利用
- [x] 保存完了時に save dialog を閉じる
- [x] 保存中 waiting 表示
- [x] cancel で save dialog を閉じる
- [x] grayscale 表示トグル
- [x] subfiler の明示トグル
- [x] manga separator 描画
- [x] status message の下部表示
- [x] ライトモード時の SVG 線色
- [x] separator shadow gradient
- [x] 起動時の manga Fit 再計算ループの抑制
- [+] low-I/O archive 時は preload 抑制
- [x] filer 表示時の manga レイアウトは実機で継続確認
- [+] app 起動時の初回 decode 完全 worker 化
- [+] startup path 解決の render worker 側移動
- [+] startup 後の filesystem 同期を実画像 path 優先へ変更
- [+] 単発 preload queue
- [+] message UI 整理
- [+] pending navigation 導入による event ordering 改善
- [+] 読み込み開始時の placeholder texture クリア
- [+] 画像切替時の zoom factor リセット
- [+] manga companion は navigator ready 後にのみ同期

## src/drawers/affine.rs
- [x] resize / interpolation 実装
- [ ] resize 品質と速度の細かな切り替え

## src/drawers/image.rs
- [x] image load
- [+] plugin fallback load
- [x] image save
- [x] SaveFormat 選択

## src/bench.rs / examples/*
- [+] decode / browser / archive ベンチマーク example
- [ ] 実装単位の詳細 benchmark 拡張
- [ ] 保存オプションの詳細化

## src/drawers/filter.rs
- [x] grayscale 系 filter は存在
- [ ] scaling系 filter
- [ ] エッジ系filter
- [ ] 色系filter
- [ ] viewer のフィルタパイプライン統合

## src/drawers/grayscale.rs
- [x] グレースケール処理の基礎

## src/drawers/canvas.rs
- [x] Canvas 基盤

## src/drawers/draw.rs
- [x] 基本描画

## src/drawers/clear.rs
- [x] クリア処理

## src/drawers/utils.rs
- [x] 補助関数

## src/drawers/error.rs
- [x] 描画エラー型

## src/error/mod.rs
- [x] 共通 error module の土台

## src/graphics/mod.rs
- [-] 役割の再整理


## レビュアーissue（整理版）
### src/ui/viewer/mod.rs
- [+] 画像ロードに失敗したとき、loading texture へ戻す
- [ ] `src/ui/viewer/mod.rs` の state 分離を進めて `ViewerApp` をさらに薄くする
- [ ] UIの責任範囲と描画領域をハッキリさせる ダイアログは別Windowで処理出来るならば別Windowで処理する
- [+] 大きなファイルを指定した場合、起動時に時間がかかるので、UIを先に起動して、画像展開中を表示
- [x] issue: マンガモード:フォルダが切り替わったとき前の画像がクリアされない（次のフォルダの初めからリスタート）
- [x] issue:マンガモード：画面がちらつく問題
- [x] issue: [`home`][`end`]を押したときzip(仮想フォルダ)の最初と最後ではなく、フォルダの最後のzipに飛ぶ
- [+] issue: viewer 画像が切り替わらないことがある
- [x] issue: 設定が即時適用されてしまう問題
- [x] issue: マンガモード:次のフォルダの画像を表示してしまう問題
- [x] issue: 最初にファイルがないフォルダを指定した時にフォルダを切り替えてもナビゲーションが反応しない
- [+] issue: fontとlocaleは設定で変更できるようにしてください(defaultはsystem)

### src/ui/menu/fileviewer/mod.rs
- [ ] UIアイコンの洗練
- [*] zip 内ファイルソートの実機確認
- [+] 数字入りファイルのソート順の Explorer 差分調整(確認中)
- [+] ファイラー/サブファイラー/viewer のファイル表示順の実機確認(確認中)
- [*] ファイラー: OS name collation の最終調整(確認中)
- [ ] ファイラーのサイズ表示を読みやすくする
- [ ] まれに固まる事がある フォルダに問題があるのかfilerに原因があるのか調査中
- [ ] issue:Linuxのファイラーで数字が化ける
- [x] issue: サムネイルが表示されない事がある問題
- [+] issue: ファイラーのサイズ表示を読みやすくする

### src/ui/menu/fileviewer
- [ ] コードの整理 モジュール境界をハッキリさせる
- [ ] 未実装 action の no-op 整理
- [ ] コードのフルレビュー
- [ ] `filesystem.browser` の lazy load / incremental snapshot をさらに進めて大規模フォル
- [*] issue: フォルダの分離モードが機能していない(フォルダが先、ファイルが後に来る挙動です)
- [*] issue: 数字のフォルダだけ前に来て、それ以外のフォルダが前に来たりと挙動がおかしい
- [+] issue: フォルダの分離モードでフォルダの降順が入れ替わらない
- [*] issue: ファイラーがハングアップすることがある問題
- [x] 上記、decoderのthreadがpanic!で落ちたときか？ watchdogが必要
- [+] issue: ファイラーの時刻表示をシステムに併せる。 UTCを使わない
- [*] フォーマットをLocaleを併せる crate icu を利用 日本語しか効いていない

### src/ui/menu/config/mod.rs
- [+] 設定で、thumbnail抑制オプションを保存できるようにする filesystem.thumbnail
- [x] issue: 設定 適用ボタンを押してから反映に時間がかかる問題
- [x] issue: 設定のLocaleの表示が2つある。[自動]はボタンにしてシステムロケールを設定してください(そのさい、反映させないでください)
- [x] issue: 設定: 分かりにくいので[保存先を記憶] → [画像保存先を記憶]に変更
- [x] issue: 設定: タブ[システム]を最後追加し[拡張子を登録]と[システム登録を削除]をウィンドウから移動
- [+] issue: 設定：ナビゲーション→[保存先を記憶]を押すと固まりやすい
- [x] issue: [適用] [キャンセル] が押されたとき設定を閉じる

### src/dependent/plugins/*
- [*] issue: system プラグイン実行時　強制終了時 COM Surrogateが残ることがある(再現条件を確認中)
- viewer 終了時に render worker へ明示 shutdown / join を追加
- [+] `src/dependent/plugins/*` に実ランタイムを足して internal(内蔵Codec) /system(OS Codec, Windows/MAC) / ffmpeg / susie64(windows only) の優先順位解決を実装する
- [x] ffmpegプラグイン(動作:windows o avif o jp2 x heic)
- [x] susie64プラグイン(動作:x avif o jp2 x heic)
- [x] Windows Codecプラグイン(動作: o avif x jp2 o heic)
- [x] 設定を変えた時、再起動を促すポップアップを出す
- [ ] [重要度低] Arm MACのテスト環境が無い MacOS Codecプラグイン(動作: o avif x jp2 o heic)

### src/configs/resourses/*
- [x] issue: WindowsとMacOSのfontの最優先はそのロケールのシステムフォント(default)にしてください。それを上書きする形にしてください。
- [x] issue: Windowsのfontの検索は、%LOCALAPPDATA%\Microsoft\Windows\Fonts → %WINDIR%\Fontsの順です。現在ハードコーディングされています
- [+] issue: fontとlocaleは設定で変更できるようにしてください(defaultはsystem)
- [x] issue: fontフォールバック表示システム（enロケールで他国語が出ない問題を回避）
- [x] ubuntuで数字が出ない
- [x] リソースenで日本語が表示出来ない問題

### src/dependent/linux/mod.rs
- [+] Linux font fallback 候補（数字が出ない）
- [x] issue:Linuxのファイラーで数字が化ける
- [+] LinuxはWSLでbuild。実行はVMで行う 現在buildは OK 起動もOK
- [x] Ubuntu OK

### src/dependent/windows/mod.rs
- [x] issue: makiが表示出来ないバグ

### src/filesystem/zip_file.rs
- [*] zip 内ファイルソートの実機確認
- [+] 数字入りファイルのソート順の Explorer 差分調整(確認中)
- [*] 非 network-aware format の I/O バースト対策の一環として zip 展開の速度確認
- [*] zip 起動時がもたつく問題を修正, cache, ダミースクリーン+ Waiting画面など
- [+] issue: zip crateはBufferReadで8KBのキャッシュしか効いていないので、ZipCacheReaderをラップして改善できるかチェック　`zipreader.md` 参照
- [*] issue: 起動時にzipが指定されると長時間待たされる
- [*] issue: 起動時の引数にzipを選ぶとナビゲーションできなくなるバグ(Filerで選択できる)
- [ ] zip/tiff: 時間のかかる展開時にviewer側が固まる問題
- [*] local archive cache の方針見直し
  - 詳細は `FileSystem > 仮想ファイル > キャッシングアルゴリズムの見直し/実装` を参照
- [*] issue: `bench_archive` 1.6G は安定して終わらない。benchの結果は `test\benchmarklog.txt` `.\test\bench.bat`で実行可能

### 全体 / アーキテクチャ
- [*] 全体的にイベントの処理順番に引きずられているissueが多いので処理順を見直してください
- [x] plugin, 設定: プラグインと内製の優先順位の設定
- [x] todo.mdの更新
- [x] wml2viewerのREADME.ja.mdとREADME.mdの更新
- [ ] 拡張子wmlはwmltxtに変更しました


## 修正確認中issue
### system
- [ ] コードの整理 モジュール境界をハッキリさせる
  - [ ]未実装 action の no-op 整理
- [ ] P4 Arm MACのテスト環境が無い MacOS Codecプラグイン(動作: o avif x jp2 o heic) 
- [x] issue: 最初にファイルがないフォルダを指定した時にフォルダを切り替えてもナビゲーションが反応しない
    - [x] archive_benchmarkの実装を以下のファンクションでとってください

### ui
- [ ] UIアイコンの洗練
- [x] issue: WindowsとMacOSのfontの最優先はそのロケールのシステムフォント(default)。それを上書きする形にしてください。
- [x] issue: Windowsのfontの検索は、%LOCALAPPDATA%\Microsoft\Windows\Fonts → %WINDIR%\Fontsの順です。現在ハードコーディングされています
- [+] ファイラー/サブファイラー/viewer のファイル表示順の実機確認(確認中)
- [ ] UIの責任範囲と描画領域をハッキリさせる ダイアログは別Windowで処理出来るならば別Windowで処理する
    - [ ] Viewer: ベース 
        - [x] マンガモード: ベースを画像表示部分を二分割する
            - [x] A4の縦横比が1:√2のためwindowサイズを widt　h >= height*1.4に緩和
        - [x] message: overlay viewerの下1行にOverlay
           - [x] issue: Message Overlayが左下に固まってでる問題。横幅はWindowの幅に 長すぎる場合は...で省略
        - [x] サブファイラー：ベースの下側にOverlay 
      - [x] ファイラー: 左ペイン（最大 Width MAX, width >= height*1.5 緩和しましたの時はwidthの半分）縦長画面も考慮する
      - [x] ファイラー,設定: 左ペインと右ペインを切り替えられる様にする
    - [x] 設定: ダイアログ。設定が閉じられるまでViewerは固定される
    - [+] アラート: ダイアログ。アラートに紐付いたUIは閉じるまで固定される
### viewer texture / zoom
- [+] issue: 画像ロードに失敗したとき、前の画像がそのまま残り続ける
- [+] issue: 前の画像がそのまま残り続ける 各状態の`egui::Image::from_textur`をトレースすること
    - [+] issue: 最初画像のtextureが再利用されつづける問題
- [+] 参照する `texture` をトレースして `default_texture` / `prev_texture` / `current_texture` / `next_texture` に分ける
- [+] マンガモードでは各 texture が 1 枚か 2 枚かを画像サイズで動的に切り替える
- [+] フォルダをまたぐ時は texture を一度破棄して再生成する

#### filer
- [*] zip 内ファイルソートの実機確認
- [+] 数字入りファイルのソート順の Explorer 差分調整(確認中)
- [+] 設定で、thumbnail抑制オプションを保存できるようにする filesystem.thumbnail
- [+] issue: ファイラーがハングアップすることがある問題
    - [+] デフォルトではalertを抑制してください またalertにはファイル名を付けてください
- [+] issue: ファイラーのサイズ表示を読みやすくする
    - 000,000 方式
    - 1024byte 未満は B
    - 1024byteから100,000KB は KB
    - 100MB ～ 100,000MB は MB
    - 100GB ～ は GB
- [x] OSソート順 Unicode Collation Algorithmを利用
- [x] ja以外の時間ロケールが適用されていない
- [x] [分離] を[フォルダ分離表示]に
- [x] [ZIP=FIle]をデフォルト表示にして、切り替えは[ファイラー]から隠す(機能は後から使う)
- [x] [名前]を[名前のソート順]に修正し、ドロップボックスに変更 

### FileSystem
- [*] zip / archive cache policy を provider 共通方針へ寄せる
- [ ] listed fileが表示されない問題

### Input
- [x] P0 input系のイベントディスパッチを整理 マウスイベントが効かない原因を追及
- [x] P1 issue:マウス:デフォルト挙動との干渉チェック 追加されたマウスイベントが効かない
- [x] マウス:ダブルクリックが効かない ScreenFit <--> None のトグルにする
- [x] マウス:左クリック 次の画面を表示にする
- [x] マウス：ローラーはスクロール　デフォルトの挙動
- [x] マウス:右クリック 設定を表示する → 現在[簡易メニュー]が表示されて閉じられないので[設定]にしてください
- [x] issue: マウスイベントが画像内部でしか効かない(backgroundで効かない) VieweAppのイベント定義域にbackgroundが入って居ない？
- [x] issue: 左クリックは左ダブルクリックでないときにのみ発火(eguiに機能がないみたいなので時間で管理 default 500ms)

### plugin
- [x] ffmpegプラグイン(動作:windows o avif o jp2 x heic)
- [x] susie64プラグイン(動作:x avif o jp2 x heic)
- [x] Windows Codecプラグイン(動作: o avif x jp2 o heic)
- [x] `src/dependent/plugins/*` に実ランタイムを足して internal(内蔵Codec) /system(OS Codec, Windows/MAC) / ffmpeg / susie64(windows only) の優先順位解決を実装する
- [x] ISSUE:プラグイン用フォルダ設定時、コマンドラインが表示される
- [x] ISSUE:プラグイン実行時、コマンドラインが表示される
- [+] ISSUE:ゴーストプロセスとしてCOM Surrogateが残るバグ(継続観察)
- 
### renderer
- [x] +/-が zoom[なし]以外で効かない fit計算とzoom計算が干渉している
    - [x] issue: [幅に合わせる]で効かない
    - [x] issue: ページを変えた時にzoom変更をリセット
- [x] fit の後に zoom の計算を入れる
- [x] issue:マンガモード:フォルダが切り替わったとき前の画像がクリアされない（次のフォルダの初めからリスタート）
- [x] issue:マンガモード：画面がちらつく問題
- [+] issue: viewer 画像が切り替わらないことがある
    - [+] 初期指定時
    - [x] issue: scanning folderが出ている時？
    - [+] reading folderが出ているとき
    - [x] issue: フォルダが切り替わったとき
    - [x] issue: マンガモード：半分しか書き換わらない時がある
- [x] issue: マンガモード:次のフォルダの画像を表示してしまう問題
    - [+] issue: マンガモード:次のフォルダの最初に前フォルダの画像を表示してしまう問題 [home]を押すと正しい表示になる
    - [x] issue: サムネイルが表示されない事がある問題
    - [x] 上記、decoderのthreadがpanic!で落ちたときか？ watchdogが必要
    - [+] issue: ファイラーの時刻表示をシステムに併せる。 UTCを使わない
      - [*] フォーマットをLocaleを併せる crate icu を利用 日本語なら YYYY/MM/DD HH:MM
    - [*] issue: フォルダの分離モードが機能していない(フォルダが先、ファイルが後に来る挙動です)
      - issue: 数字のフォルダだけ前に来て、それ以外のフォルダが後に来たりと挙動がおかしい
    - [+] issue: フォルダの分離モードでフォルダの降順が入れ替わらない
    - [*] 大きなファイルを指定した場合、起動時に時間がかかるので、UIを先に起動して、画像展開中を表示
      - [*] zipではUI先行起動まで対応(ファイラーに引きずられて遅くなる模様)
- [x] [精密]が予想以上に重いので[高速]をデフォルトに変更

#### viewer texture / zoom
- [*] issue: 画像ロードに失敗したとき、前の画像がそのまま残り続ける
- [*] issue: 前の画像がそのまま残り続ける 各状態の`egui::Image::from_textur`をトレースすること
    - [+] issue: 最初画像のtextureが再利用されつづける問題 
- [*] 参照する `texture` をトレースして `default_texture` / `prev_texture` / `current_texture` / `next_texture` に分ける
- [+] マンガモードでは各 texture が 1 枚か 2 枚かを画像サイズで動的に切り替える
- [*] フォルダをまたぐ時は texture を一度破棄して再生成する
- [x] `waiting`時の画像がただ点なのでマシなのに差し替える `now loading` など
- [x] 精密モードで表示時、一瞬フラッシュする（リサイズする前の画像が先に書き込まれている）


### font
- [x] P1 issue: ubuntuで数字が出ない（既知の現象なので原因を調査してfix）
- [x] P3 issue: fontフォールバック表示システム（enロケールで他国語が出ない問題を回避）
- [x] P3 user setting font -> system locale font -> cjk font -> emoji -> Last Resort の順で fallback させる

### setting
- [x] 設定を変えた時、再起動を促すポップアップを出す 
  - jpeg2000/avif/heicは ./samplesにサンプルあり susie64はjpeg2000だけ、ffmpegは両方可能のはず
- [x] issue: makiが表示出来ないバグ
- [+] issue: デコーダがErrorを起こしたときにpanic!を起こし次の画像が表示できなくなるバグ
    - [+] 壊れたbmp対策:decoderに緩和策を適用
- [x] issue: 設定: 分かりにくいので[保存先を記憶] → [画像保存先を記憶]に変更
- [x] 名前でソートのicon(sort.svg)を差し替えました
- [x] 設定: タブ[システム]を最後追加し[拡張子を登録]と[システム登録を削除]をウィンドウから移動
- [x] 設定: タブ[システム]: 
    - [x] get_prograname() ,get_version(), get_copyright(), get_auther() 取得したプログラム名、(C) 作者名 バージョン表記
    - [x] [拡張子を登録]と[システム登録を削除]
- [+] issue: 設定：ナビゲーション→[保存先を記憶]を押すと固まりやすい
- [x] issue: [`home`][`end`]を押したときzip(仮想フォルダ)の最初と最後ではなく、フォルダの最後のzipに飛ぶ
- [x] issue: 設定が即時適用されてしまう問題([モジュールを読み込む] [拡張子を登録] [システム登録を削除]以外の設定は[適用]が押されるまで遅延させてください その関係で[元に戻す]が効いていません)
- [x] issue: 設定のLocaleの表示が2つある。[自動]はボタンにしてシステムロケールを設定してください(そのさい、反映させないでください)
- [x] issue: 設定 適用ボタンを押してから反映に時間がかかる問題
- [x] 縮小・拡大に高速(GPU)/精密(CPU)を追加 設定にも適用（デフォルトは精密）
  - [x] 高速はeguiまかせ、精密は`drawers/affilne.rs`を利用
  - [x] 高速のアルゴリズムは、Nearest/Linearのみ、精密は、Nearest/Linear/Qubic/Lancos3 (縮小はPixelMixing)

### i18n
- [+] issue: fontとlocaleは設定で変更できるようにしてください(defaultはsystem)

### OS Depended
- [x] Ubuntu OK
- [x] リソースenで日本語が表示出来ない問題
