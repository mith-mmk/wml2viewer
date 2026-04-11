# Viewer / FileSystem State Map

最終整理日: 2026-03-22

このメモは `src/ui/viewer/mod.rs` と `src/filesystem/mod.rs` の状態遷移を、実装に合わせて一度棚卸しするためのものです。
Viewer は描画と UI を持ち、FileSystem は「次に開くべき画像」を決める役です。両者を同じ state として扱うと干渉しやすいので、まず役割を切り分けます。

## Viewer 側の状態

### 1. 表示中の画像

- `current_navigation_path`
  - ナビゲーション上の現在位置です。
  - `request_load_target()` で読み込み対象を決めたあとに更新されます。
  - `FilesystemResult::NavigatorReady` で `load_path = None` のときは、この値だけ更新されます。

- `current_path`
  - 実際に表示中の画像パスです。
  - `apply_loaded_result()` で更新されます。
  - virtual child path や preload 経由でも、最終的にはここが表示画像の基準になります。

- `source` / `rendered` / `texture`
  - `source` は元画像、`rendered` は zoom / interpolation 後の画像、`texture` は egui に載せる GPU テクスチャです。
  - `rendered` が変わると `upload_current_frame()` が走ります。
  - `texture` は `source` / `rendered` の組より一段下の表示キャッシュです。

### 2. 画像ロード系の state

- `active_request`
  - `Some(Load(_))` か `Some(Resize(_))` のとき、render worker に対して未完了リクエストがあります。
  - `request_load_target()` で `Load` に入り、`request_resize_current()` で `Resize` に入ります。
  - `poll_worker()` で request_id が一致した `Loaded` / `Failed` を受けると `None` に戻ります。

- `pending_navigation_path`
  - `Load` 中の論理的な遷移先です。
  - `apply_loaded_result()` の先頭で `take()` され、成功時に `current_navigation_path` の更新に使われます。
  - `Failed` のときは `None` に戻されます。

- `pending_resize_after_load`
  - load 中に zoom 変更が入ったときの遅延フラグです。
  - `request_resize_current()` が load 中なら `true` にします。
  - load 完了後に `request_resize_current()` をもう一度呼んで解消します。

- `pending_fit_recalc`
  - Fit 系 zoom を再計算したい合図です。
  - load 成功、manga companion の切り替え、window size 変化で立ちます。
  - `update()` の中央パネルで再計算後に `false` へ戻ります。

### 3. FileSystem 連携 state

- `navigator_ready`
  - FileSystem worker が初期化済みで、ナビゲーション要求を受けられる状態です。
  - `FilesystemResult::NavigatorReady` で `true` になります。
  - worker 再生成時は `false` に戻ります。

- `active_fs_request_id`
  - FileSystem worker に投げた未完了リクエストの id です。
  - `init_filesystem()` / `request_navigation()` でセットします。
  - `NavigatorReady` / `PathResolved` / `NoPath` を受けると `None` へ戻します。

- `queued_navigation`
  - FileSystem が busy か未初期化のときに 1 件だけ保持される待ち行列です。
  - `request_navigation()` はここに上書きします。
  - `poll_filesystem()` で `active_fs_request_id` が空になったときに再送されます。

- `deferred_filesystem_init_path`
  - startup 時に load を先行させるための遅延初期化用 path です。
  - 初回 load 成功後、または load failed の後に FileSystem 初期化へ回ります。
  - 現在は startup 専用の一時 state に近いです。

### 4. UI オーバーレイ state

- `show_filer`
  - filer パネルの表示/非表示です。
  - empty mode、NoPath、ユーザー操作で変わります。

- `show_subfiler`
  - subfiler の表示/非表示です。
  - `current_directory()` と `filer.directory` が一致しているときだけ意味があります。

- `empty_mode`
  - 表示可能な画像がまだ無い状態です。
  - 起動時に filer を開いた場合や、No displayable file found のときに入ります。
  - `show_filer` と同時に `true` になり得ます。

- `overlay.loading_message`
  - スキャン/ロード中の下部メッセージです。
  - render / filesystem の busy 状態に応じて更新されます。

- `overlay.alert_message`
  - worker disconnect や重大な通知用です。

### 5. Manga / preload state

- `companion_*`
  - manga spread 用の 2 枚目画像です。
  - `desired_manga_companion_path()` が `Some` を返すときだけ有効です。
  - `clear_manga_companion()` と request id の不一致で stale result を捨てます。

- `preloaded_*`
  - 次に使う候補の先読み画像です。
  - `schedule_preload()` で発火し、`poll_preload_worker()` で cache されます。
  - `try_take_preloaded()` が path 一致時に即時採用します。

## Startup sequence

startup は beta 前でも state machine を組み替えやすいように、次の順で考えるのが安全です。

1. viewer worker を最優先で起動し、`current_texture` だけ作る。
2. 最初の画像をロードして、まずは単体ビューアーモードで表示する。
3. filer / filesystem / preload / thumbnail などの各 worker を生成する。
4. 最初の画像を表示し、UI を先に見せる。
5. 各 worker を同期して、マルチプル・ビューアーモードへ切り替える。

### このときの想定 state

- `default_texture`
  - loading 中の固定プレースホルダです。
  - startup 時に 1 回だけ作る想定です。

- `current_texture`
  - 現在表示中のメイン画像です。
  - 単体ビューアー時はこれだけで成立します。

- `prev_texture` / `next_texture`
  - 先読みや移動用の候補です。
  - beta 以降に段階的に増やす前提で、今は `preloaded_*` / `companion_*` に相当します。

- `single_viewer_mode`
  - viewer worker と `current_texture` だけが有効な初期状態です。
  - startup の遅延を filer に引きずられないための分離ポイントです。

- `multiple_viewer_mode`
  - filer / filesystem / preload / thumbnail の同期が済んだ後の通常運転です。
  - ここに入ってから補助 worker の状態を本格利用します。

## FileSystem 側の状態

### 1. worker の前提

- worker は `FileNavigator` を 1 個だけ持ちます。
- 1 回の `Init` で「現在地」と「最初の表示候補」を作ります。
- `Next` / `Prev` / `First` / `Last` は、すでに作られた navigator を進めるだけです。

### 2. 主要な状態遷移

- `FilesystemCommand::Init`
  - 入力 path を `resolve_navigation_path()` で正規化します。
  - 成功時は `FileNavigator::from_current_path()` を作り、`NavigatorReady` を返します。
  - 失敗時は `NoPath` を返します。

- `FilesystemCommand::SetCurrent`
  - navigator があれば current を差し替えます。
  - まだなければ `resolve_navigation_path()` 成功時だけ navigator を作ります。
  - 現状の viewer 側では `CurrentSet` をほぼ無視しているので、同期上は弱い state です。

- `FilesystemCommand::Next` / `Prev`
  - 現在の file list 内を移動します。
  - 端に達したら `EndOfFolderOption` に従って `Stop` / `Loop` / `Next` / `Recursive` を選びます。

- `FilesystemCommand::First` / `Last`
  - 現在のコンテナ内の端へ飛びます。

### 3. state が変わる条件

- `resolve_navigation_path()` が成功すると、container / virtual child / directory のいずれでも表示候補へ進めます。
- `resolve_start_path()` が成功すると、実際に load 可能な画像 path になります。
- `scan_directory_listing()` の結果が cache に入るので、同じ directory への再遷移は速くなります。

## 状態遷移の要点

### Viewer

- `request_load_target()`
  - branch が変われば manga companion を破棄します。
  - preload cache が一致すれば即時採用します。
  - そうでなければ `active_request = Load` で render worker に送ります。

- `request_resize_current()`
  - load 中なら `pending_resize_after_load = true` にします。
  - load 中でなければ `active_request = Resize` で render worker に送ります。
  - manga companion がある場合は companion 側も resize します。

- `apply_loaded_result()`
  - `current_navigation_path` / `current_path` / `source` / `rendered` / `texture` を更新します。
  - 画像 load が成功したら FileSystem に `SetCurrent` を返します。
  - filer が開ける directory があれば、filer も更新します。
  - その後 preload を再スケジュールします。

- `poll_worker()`
  - load 失敗時は `save_dialog.message` にエラーを入れ、必要なら `next_image()` で次へ進みます。
  - worker disconnect 時は respawn して現在画像の再 load を試みます。

- `poll_filesystem()`
  - `NavigatorReady` で初期表示が決まります。
  - `PathResolved` で次画像遷移が load に変換されます。
  - `NoPath` で filer を開き、No displayable file found を出します。

### FileSystem

- `Init`
  - startup / reload の「今どこから始めるか」を確定します。
- `SetCurrent`
  - viewer の実際の表示とファイルナビゲーションの整合を取ります。
- `Next` / `Prev`
  - 1 枚ずつ、もしくは end-of-folder policy 付きで移動します。
- `First` / `Last`
  - 端へ飛ぶので、manga mode のページ移動と相性が強いです。

## 現在の矛盾状態

- `current_navigation_path` と `current_path` は同一に見えても、load 中は別物です。
  - ここを同じものとして扱うと、startup / preload / manga spread の分岐で壊れます。

- `FilesystemResult::CurrentSet` は viewer 側で実質 no-op です。
  - そのため `SetCurrent` の request_id は同期用途として弱く、state 更新の責務が分散しています。

- `queued_navigation` は 1 件しか持てません。
  - 連打時に最後の操作だけが残るので、履歴的には潰れます。

- `fade` は `ViewerOptions` にあるのに、現行の `config.toml` には保存されていません。
  - runtime state と永続化 state の境界が曖昧です。

- `show_filer` / `empty_mode` / `show_subfiler` は互いに独立です。
  - 例えば `show_filer = true && empty_mode = true` は正常な startup state です。

- `companion_*` と `preloaded_*` は、どちらも stale result を request id で捨てる設計です。
  - ただし新しい経路を足すと、ここを通らない stale cache が残る可能性があります。

## Filer / Manga の追加整理

最終整理日: 2026-04-11

この領域は、局所修正ほど悪化しやすいので、先に「誰がどの state を所有するか」を固定します。
特に `filer` と `manga companion` は viewer の従属 state として扱い、独立した truth を持たせない方針に寄せます。

### 1. 所有権

- Viewer primary state
  - `current_navigation_path`
  - `current_path`
  - `active_request`
  - `pending_navigation_path`
  - `navigator_ready`
  - `empty_mode`
  - owner:
    - `request_load_target()`
    - `apply_loaded_result()`
    - `poll_filesystem()`

- Filer state
  - `filer.directory`
  - `filer.entries`
  - `filer.selected`
  - `filer.pending_request_id`
  - owner:
    - `request_filer_directory()`
    - `poll_filer_worker()`
    - `sync_filer_directory_with_current_path()`
  - 注意:
    - `activate_filer_entry()` は `filer.selected` を更新してよい
    - ただし `current_directory() == filer.directory` の間は、最終的に `current_navigation_path` が選択の truth になる

- Manga companion state
  - `companion_navigation_path`
  - `companion_source`
  - `companion_rendered`
  - `companion_texture`
  - `companion_active_request`
  - owner:
    - `sync_manga_companion()`
    - `request_companion_load()`
    - `poll_companion_worker()`
    - `clear_manga_companion()`
  - 注意:
    - companion は primary の従属 state であり、primary と独立に生存してはいけない

### 2. 更新ルール

- `current_navigation_path` が変わったら:
  - `filer.selected` は同じ directory の範囲で追従してよい
  - `show_subfiler == true` なら `pending_subfiler_focus_path` を更新してよい
  - `companion_*` は再評価対象

- `current_path` が変わったら:
  - 表示中テクスチャと animation state だけを更新する
  - `filer.directory` は直接変えない

- `filer refresh` が入ったら:
  - 再 scan の対象 directory は `filer.directory`
  - 選択の基準は次の優先順にする
  - `current_directory() == filer.directory` のときは `current_navigation_path`
  - それ以外は `filer.selected`
  - このとき refresh を理由に `request_load_target()` を呼んではいけない

- `filer entry activate` が file のとき:
  - `filer.selected = navigation_path`
  - `set_filesystem_current(navigation_path)` を送る
  - `request_load_target(navigation_path, load_path)` を送る
  - `filer.directory` は変えない

- `filer entry activate` が container のとき:
  - `request_filer_directory(container, None)` を送る
  - viewer の `current_navigation_path` は変えない

- `manga mode` が on のとき:
  - companion の存在判定は `desired_manga_companion_path()` に一本化する
  - companion を直接残す条件は `desired == companion_navigation_path && companion_rendered.is_some()` のみ
  - primary の image 切り替え中は stale companion を残してよい理由がないので、切り替え開始時点で破棄寄りに扱う

### 3. 禁止事項

- `poll_filer_worker()` が `current_navigation_path` を更新してはいけない
- `refresh_current_filer_directory()` が暗黙に next/prev を起こしてはいけない
- `companion_*` が primary と別 branch を指したまま残ってはいけない
- `filer.selected` を truth と見なして `current_navigation_path` を上書きしてはいけない
- zip 内 file 選択時だけ wait 表示を省略してはいけない

### 4. 典型シーケンス

#### filer から zip 内 file を選ぶ

1. `activate_filer_entry()` が virtual child を受け取る
2. `filer.selected = navigation_path`
3. `set_filesystem_current(navigation_path)`
4. `request_load_target(navigation_path, load_path)`
5. wait 表示を出す条件を primary load と同じにする
6. `apply_loaded_result()` 成功後に `current_navigation_path` / `current_path` を確定する
7. `sync_filer_directory_with_current_path()` が同 directory なら選択だけ追従する
8. `sync_manga_companion()` が companion を再評価する

#### filer refresh

1. `refresh_current_filer_directory()`
2. `request_filer_directory(filer.directory, preferred_selected)`
3. `poll_filer_worker()` が `directory / entries / selected` を更新
4. この経路では viewer の load を起こさない

#### manga mode で next/prev

1. `next_image()` / `prev_image()`
2. `manga_navigation_target()` が 2 枚単位の target を返すか判定
3. `request_load_target()` が primary load を開始
4. stale companion を破棄
5. `apply_loaded_result()` 後に `sync_manga_companion()` が次の companion を再要求

### 5. manager を置く条件

以下のどれかを満たしたら、`viewer/mod.rs` の個別更新ではなく manager を置く方がよいです。

- `request_load_target()` と `poll_filesystem()` と `poll_filer_worker()` が同じ state を書き換える
- `filer.selected` と `current_navigation_path` の優先順位を call site ごとに変えてしまう
- `manga companion` の破棄条件が 3 箇所以上に散る
- wait 表示の条件が `active_request` 以外のフラグに分岐し始める

候補名は `ViewerStateManager` か `NavigationStateManager` です。
責務は次の 3 つに限定します。

- primary navigation state の reconcile
- filer selection / directory state の reconcile
- manga companion / preload の invalidate 判断

manager を置く場合でも、render / filesystem / filer worker の実行そのものは移さず、「state をどう確定するか」だけを集中管理します。

## コメントアウト候補

以下は「今すぐ消す」ではなく、責務が固まったらコメントアウト候補として見直したい箇所です。

- `FilesystemResult::CurrentSet`
  - viewer 側の no-op が続くなら、結果型ごと統合してもよいです。

- `deferred_filesystem_init_path`
  - startup sequence を明示的な state machine に分離できたら削れます。

- `pending_resize_after_load`
  - load / resize の二段遷移を 1 つの reconcile step にまとめられたら不要になります。

- `show_subfiler`
  - `show_filer && current_directory == filer.directory` で導出できるなら、派生 state に寄せられます。
