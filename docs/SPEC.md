# wml2viewer SPEC

優先順位
1. キー操作の機能を最優先 [x]
2. viewerとrender(background, zoom) [x]
3. ファイル探索機能、リステッドファイル [x]
4. 非同期実装 [ ]
5. 画像表示とディレクトリ操作は分離する [x]
6. 画像先読みデコードを実装にする
7. マンガモード（サムネイルをみてページ移動する機能） [x]
8. 設定画面 [x]
9. 設定に付随する機能
10. リソース
11. ファイラー [x]
12. ネットワーク機能
13. OS依存機能
14. プラグイン
15. キー操作の変更
16. コマンド（file name / file copy / external command）

## 現行 config 構成

`config.toml` は `src/configs/config.rs` の `ConfigFile` と `src/options.rs` / `src/ui/viewer/options.rs` の `AppConfig` に対応しています。

- `viewer`
  - `animation`
  - `grayscale`
  - `manga_mode`
  - `manga_right_to_left`
  - `manga_separator.style` = `none` / `solid` / `shadow`
  - `manga_separator.color`
  - `manga_separator.pixels`
  - `background` = `solid` / `tile`
  - `fade` は `ViewerOptions` にはあるが、現行の永続化対象ではない

  - 漫画モードの設計：
    - 通常モードの別実装ではなく、通常モードに対する条件分岐として扱う
    - `current spread = current + companion(隣の1枚)` を 1 単位として render worker に渡す
    - `preload spread = 次の見開き2枚` も 1 単位として preload worker に渡す
    - portrait かつ viewport が見開き条件を満たす時だけ 2 枚表示する
    - landscape / 相方なし の場合は単ページ表示へフォールバックする
    - `companion` は UI 側の独立状態機械として増やさず、見開き単位 load の副産物として扱う
    - 先読みは「次の見開き1組」までに制限し、通常モードより積極的に増やさない

- `window`
  - `fullscreen`
  - `size` = `relative` / `exact`
  - `start_position` = `center` / `exact`
  - `remember_size`
  - `remember_position`
  - `ui_theme` = `system` / `light` / `dark`
  - `pane_side` = `left` / `right`

- `render`
  - `zoom_option` = `none` / `fit_width` / `fit_height` / `fit_screen` / `fit_screen_include_smaller` / `fit_screen_only_smaller`
  - `zoom_method` = `nearest` / `bilinear` / `bicubic` / `lanczos3`

- `resources`
  - `locale`
  - `font_size` = `auto` / `s` / `m` / `l` / `ll`
  - `font_paths`

- `navigation`
  - `end_of_folder` = `stop` / `next` / `loop` / `recursive`
  - `sort` = `os_name` / `name` / `date` / `size`

- `storage`
  - `path_record`
  - `path`

- `filesystem`
  - 現行では `thumbnail.suppress_large_files` のみを持つ
  - 旧仕様にあった `protocol` や `zip_encoding` は config ではなく実装側の役割に寄っている

- `runtime`
  - `current_file`
  - `workaround.archive.zip.threshold_mb`
  - `workaround.archive.zip.local_cache`
  - `workaround.archive.zip.local_cache` の既定値は現行実装では `false`

- `plugins`
  - `internal_priority`
  - `susie64`
  - `system`
  - `ffmpeg`

- `input`
  - `key_mapping`
  - 既定値は `src/options.rs` の `default_key_mapping()`

## 実装メモ

- `runtime.current_file` は終了時のスナップショットとして保存され、起動時の初期 path に使われます。
- startup は first image 表示を優先し、filesystem/filer worker の同期は後続 phase へ遅延できる構造に寄せています。
- `filesystem.thumbnail.suppress_large_files` はフィル更新時の負荷を抑えるための実装寄り設定です。
- `viewer.fade` は現状 UI / 永続化にまだ出していないため、仕様上は「runtime-only の予備」として扱います。
- `window.size` と `window.start_position` は TOML 上では tagged enum です。
- `ConfigFile` の読み書きは `serde` / `toml` で行います。

## I/O と優先度

- viewer 本線を最優先とし、優先順位は `current > manga spread current > preload > filer > thumbnail` を基本とする
- hidden な filer / subfiler / thumbnail は viewer 本線を妨げないことを優先し、表示中でない限り積極的に work を走らせない
- thumbnail は補助機能であり、viewer のページ送りや見開き表示より優先してはならない
- preload は「現在の遷移を速くするための最小限」に留め、background I/O の肥大化を避ける

## 非 network-aware format の方針

- 対象は当面 `zip` と `tiff`
- 問題設定は「decoder の速度」ではなく「network path 上での I/O バースト抑制」とする
- `zip` は以下の段階的方針を取る
  - 初回1枚目は `probe_first_supported_zip_entry()` を使い、full index なしで最速表示を優先する
  - zip index は persistent cache を使い、2回目以降の metadata scan を抑える
  - access policy は `DirectOriginal` / `Sequential` を持ち、network path と size threshold と sampled profile から決める
  - 大きい network zip では「初回表示」と「連続閲覧」を同じ戦略で最適化しない
  - staged policy を前提にし、必要なら表示後に local cache warmup を行い、連続閲覧だけ cached archive へ切り替える
  - 状態遷移的にはファーストロード→メタデータスキャン→ネクストロードになるが、I/Oを減らすため、メタデータのキャッシュ戦略が重要
  - 現状、メタデータスキャンがO(n^2)的な動きをしているためO(n)を目指す
  - viewerの割り込みを優先し、メタデータのスキャンをとめてpreloadできるようにする
- `stored` / 実質無圧縮 zip は temp copy を強制しない

## Manga Mode 実装原則

- 漫画モードは通常モードの上に載る薄い分岐でなければならない
- 漫画モード専用の複雑な state machine を増やさない
- 変えるのは以下に限定する
  - 表示枚数
  - `next/prev` の進み方
  - preload の単位
- per-frame で branch 探索や companion 再同期を行わない
- 見開き target の計算は `current path / viewport / sort / archive mode` 変更時にだけ更新する
- render worker は `LoadPath` と `LoadSpread` を持ち、漫画モード時の current / preload は可能な限り `LoadSpread` を使う

## 代表的な動作

- Viewer
  - 単一画像、zip、`.wml.txt` を画像一覧として扱う
  - manga mode で 2 枚表示に対応する
  - preload / companion / overlay を worker 分離する

- FileSystem
  - 画像一覧の決定と `Next` / `Prev` / `First` / `Last` を担当する
  - `STOP` / `NEXT` / `LOOP` / `RECURSIVE` を持つ
  - virtual child path と container path を区別する

- Filer
  - directory scan を worker 化する
  - sort / filter / extension filter / thumbnail を持つ

## 旧SPECからの整理点

- `filesystem.protocol` ベースの構造は廃止し、実装側の worker / plugin / container 解決に寄せた
- `loader` / `mouse_setting` / `touch_setting` は現行 config にはない
- `zoomMethpod` などの旧表記は `render.zoom_method` に統一した
- `FileSystem` は「画像ローダと別プロセスの state」というより、viewer から呼ばれる worker として整理した

# 旧SPEC(設計書)

```jsonc
{
  "viewer": {
    "align": "center", // 画像の配置 "center", "right-up", "left-up", "right-down", "left-down", "left", "right", "up", "down"
    "background":  {"color": "#000000" }, // or , {"tile": {"color1": "", "color1": "", "size":"16x16"}}, 
    "fade": false, // bool フェードイン・アウト
    "animation": true, // bool アニメーションを有効にするか
  },
  "window": {
    "fullscreen": false, // bool モバイルでは無効
    // 起動時のwindowの位置 モバイルでは無効
    "start_position": "center", // || { "start_x": 0, "start_y": 0}  
    // 起動時のwindowのサイズ
    "size": "80%",  // {"width": int,"hight": int}
    "keep": false, // 終了時の状態を維持する
    "ui": "dark" // "dark", "system", "white"
  },
  "render": {
    "zoom": "FitScreen",　// ZoomOption
    "zoomMethpod": "bilinear", // "bicubic", "bilinear", "nearest neighbor", "lancos3"
    "minimize": "pixel mixing",   // 縮小は、pixel mixingのみ
    "orientation": true, // メタデータの回転情報を反映させるか
    "roatation": 0, // 画像を回転(degree)
    "flap": false, // 上下反転
    "flip": false, // 左右反転
    "monochrome": false, // モノクロモード
    "transpearent": "background", // "override" , "ignore", "background" // 透過色の扱い、 override 前の画像に上書き、 ignore 無視、 background （背景色に置き換え）
  },
  "reading": {
    "manga": {
      "enable": false, //コミックモード 横長の場合、最大2枚表示
      "r2l": true, // コミックモードの時、右から左か左から右か
      "partition_size": 0, // px
      "partition_color": "#000000",
      "partition_effect": "shadow" // "shadow"  , "solid",...
    },
    "slideshow": {
      "enable": false,
      "wait_time": 1.0, // sec
      "next_foloder":  "STOP" // フォルダの最後の時の挙動、 "STOP", "NEXT", "LOOP", "RECURSIVE"    
    }
  },
  "navigation": {
    "sort": {
      "sort_by": "os_name", // "date", "size", "name", "win_name", "linux_name", "os_name", "none", "random" nameは通常の名前sort、os_nameはosによる名前ソート
      "order": "asc", // "asc", "desc"
      "filter_by": ""　// filter condition
    },
    "end_of_folder": "RECURSIVE", //FolderOption, // フォルダの最後の時の挙動、 "STOP", "NEXT", "LOOP", "RECURSIVE"
    "archive": "FOLDER",// ArchiverOption "FOLDER"（フォルダの用に扱う）, "SKIP"（読まない）, "ARCHIVER"（複数画面の画像フォーマットの用に扱う）
  },
  "thumbnails": { // サムネイル
      "enable": false,
      "os_thumbnail": false, // OSのサムネイルがあれば横取りする
      "cache": { // サムネイルキャッシュ
        "enable": false, // 有効化
        "path": "default", //サムネイルの場所 defaultはOSデフォルト フォルダ内にサムネイルキャッシュは置かない
        // windowsの場合は %APPDATA%\Local\wml2viewer\cache\
      },
      "size": 64, //サムネイルのサイズ size x size (png)
  },
  "loader": {
    "max_size": 0, // 使用する最大メモリ（これを越える画像はリサイズロードする 0はOSの最大メモリの1/4）
    "split_file": true, // 複数画面が入って居る画像を順番に読み込むか true, 最初だけ表示するか false, アニメーション"animation":falseの場合にチェック
    "preload": true, // 先読みするか
    // ここから先は後でインプリ
    "plugin": false, // 外部プラグイン ffiで読み込む(winはsusie, macとlinuxはこれから考える)を有効にするか
  },
  "storage": {　// 保存オプション
    "path_record": false, // 引き継ぐ（未設定の場合はOS既定のダウンロードパス）
    "path": "", // ダウンロードパス
    "os_encoder": false, // OSのエンコーダを優先するか
    "plugin": false, // 外部プラグイン ffiで読み込む(winはsusie, macとlinuxはこれから考える)を有効にするか
    "ffmpeg": false, // 保存のプラグインにffmpegを使うか(ffmpeg dll/soを使う)
    "default": false, // 保存時デフォルトで使うフォーマット 省略時 png, png, bmp, tiff,jpeg, webp
    "png_option": null, // (default optimize=6), optimize = 0-9, exif = copy, none
    "jpg_option": null, // default quality=80, quality = 0 - 100 , exif = copy, none
    "bmp_option": null,　// default compless = none,　now constructions
    "tiff_option": null, // default compless=none,  compless = none, LZW, JPEG, exif = copy, none
    "webp_option": null, // default compless = lossy, quality=80, optimize=6,  compless lossless, quality = 0 - 100(lossy only), optimize = 0-9(default 6), exif = "copy", "none"
  },
  "FileSystem": { // FileSystemは画像ローダと分離して動くため、messageでやりとりする（PerfectViwer遅い理由はファイルクロールなので）
    "protocol": ["file", "zip", "ListedFile"], // 有効にするプロトコル "file"(must), "http", "smb"(モバイルのみ), "cloud"(モバイルのみ), "zip", "7z","ListedFile"
    "zip_encoding": "AUTO" // SJIS, Unicodeを自動判別 他 "CP932", "UTF-8",....
  },
  "input": {
    "key_mapping": {}, // {"key": "fanction"} ... 未指定はデフォルト KEYはJava Script準拠
    /*
      o ... Open (FileDialog)
      Shift+R ... Reload
      + .. Zoom Up
      - .. Zoom Down
      Space ... next image
      right allow ... next image
      Shift + right allow ... only 1page next (manga mode only)
      Shitf + Space ... prev image
      left allow .. prev image
      Shift + left allow ... only 1page prev (manga mode only)
      page up ... next folder
      page down ... prev folder
      home .. 1st image
      end .. last image
      Shift+G .. Glaysacle toggle
      Shift+C .. Comic mode toggle
      enter .. full screen
      F1 .. help
      P .. setting

      // no assinged function
      file delete
      file move
      file copy
      run exec (use external_cmd)
      exit (os default)
      filter() 
      crop
      resize

    
     */
    "mouse_setting": {}, // {"key": "function"} ... 右、中、左クリック、ホイール、 4button以上に対応（できる？）
    "touch_setting": {} // {"key": "function"} ... タッチの場所でメニュー（perfect viewerを参考）
  },
  "runtime": {
    "resource_path": null, // リソースファイルの場所　デフォルトは実行ファイルの中 指定がある場合、外部リソースを優先（基本は言語リソース？）
    "current_file": "", // path like string, 現在のファイルの場所（スナップショット）
    "startup_file": true, // true current_file, false ./(デフォルト) ただしコマンドラインで上書きされる
    "external ": { // 【要注意】 外部コマンドを利用する（pngを経由する） // 通常は無効
        "external_tmp": null, // 受け渡しに使うtmpフォルダ 無い場合は、環境変数 TMP -> TEMP の順で探す
        "external_cmd": [] // 外部コマンドのコマンドライン %i(入力名) %O(フォルダ) ...
    },
    "os_depend": { // OS依存の設定
    },
  },
  "plugins" : {
    "susie64": { // windowsのみ, 他のOSでは無視
      "enable": false,
      "search_path": ["./"], // exeと同じ場所
      "modules":
      [ {
        "enable" : false, //
        "path": "", // 省略可
        "plugin_name": "", //
        "type": "image", // or "archiver"
        "ext": [  // 対応するフォーマット
          {
            "enable": false,
            "mime": ["image/avif"],  //例 ["image/*] ワイルドカード可能
            "modules": [{
              "type": "decode", // "decode", "encode", "filter" 
              "priority": "high", // "lowest", "low", "middle", "high" (プラグインが複数ある場合どれを優先するか決める、lowは内製を再優先, middleは他のプラグインが無い場合、 highは最優先，"lowest"は最後の手段, バッティングした場合は、順位が上にある方が優先)
            }]
          } // , {}...
        ],
        "":
      }]
    },
    "system": { // OSバンドル系 windowsは WIC APIベース Windowsは一部Codecがオプションで存在しない場合があるので注意
      "enable": false,
      "search_path": "", // OSに依存するため設定不可
      // 以下同じ
    },
    "ffmpeg": { // ffmpegの動的ライブラリを呼ぶ linuxはシステムの変わり

    }
  }
}
  ```

ListedFile 以下の様なファイル
拡張子 .wml
```txt
#!WMLViewer2 ListedFile (version)
https://example.org/test.webp
\\pi4\data\images\sample.png
d:\data\images\sample.jpg
/home/user/images/sample.bmp
# コメント
@command
# @で始まるのはコマンド 複数行は@() で括る 実装予定だがまだ何も決まってない　取りあえず予約語
@(
 command1
 command2
 command3
 command4
)
```
