
# 0.0.13で実装されたが一部キャンセル
- [+] I/Oストリームの改善(zipのパフォーマンスの改善。FileSystemが面倒をみる。)
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
- [ ] サブファイラー：右→左表示の修正（右端に表示位置を固定し、カレント近傍優先で表示する）
- [ ] サブファイラー：サブファイラーの表示が カレント近傍優先優先ではない
- [ ] サブファイラー：カレントのファイルではなく最後から読み始める

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

- [ ] viewer: 読み込みが終わる前に次のファイルに進んでしまう
- [ ] zipファイルの立ち上がりが遅い問題
- [ ] P0 filer: フォルダ移動（特に zip -> zip）で固まる / 復旧待ちになる / 1枚目だけで止まる
- [ ] filer: zip内のファイルで終了したときそのファイルではなくファイラーを起動してしまう問題
- [ ] filerからzip内のファイルを選択するとwait画面が出ない問題

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

