# wml2viewer TODO
- project wml2viewerに関するもののみ

ステータス
- [x] 確認済み / 安定実装
- [+] 実装済み / 今後の拡張余地あり
- [*] 実装済みだが要再確認 or 既知の不具合あり
- [-] 設計保留
- [ ] 未実装

最終整理日: 2026-04-13

# 0.0.14
## 優先度1
- [x] ベンチマークモード（デバッグ用）
- [x] ロギング（デバッグ用）
  - [x] ロガーの実装
  - [x] ログ出力の追加（特にファイラーの状態変化、branch change、navigation など）
- [x] WML2からロード可能な画像形式をハードコーディングではなくAPI経由(get_encode_extentions, get_decoder_extentions)で取得する
- [x] フォントサイズのデフォルトがSになっているので自動に変更
- [ ] ファイラーソート順のバグ
  - [+] filerとviewerのソート順がおかしいバグ
  - [ ] 親フォルダのHome Endがおかしい（子フォルダに移動してしまう）

- [ ] P2 filer / subfiler 総合整理
    - 現在の残課題を filer 単独ではなく `viewer / filesystem / filer / subfiler` の同期問題としてまとめて扱う
    - sort の truth を 1 か所に寄せる
      - filer の sort が viewer navigation に反映されない
      - viewer の current order と filer の表示順がズレる
    - scroll / focus の追従
      - filer の表示位置が current に追従しない
      - 毎回下までスクロールしないといけない
      - subfiler も current 近傍を基準に表示したい
    - request / snapshot / current sync
      - filer scan 完了前後で viewer current と selected がズレる
      - branch change / recursive navigation / filer click の完了順で再発しやすい
      - [+] filer snapshot 変化時に current directory なら filesystem を再初期化して viewer/filer のズレを抑制
      - [*] viewer: branch change 時は相方ページ探索（adjacent lookup）を同期実行しない（主画像ロード優先で固まり回避）
    - scan / thumbnail / visible range
      - subfiler thumbnail が current 近傍優先ではない
      - visible range と current 近傍を優先する
    - 含める issue
      - zip -> zip で固まる / 1枚目で止まる
      - ファイラーのファイルは更新されるが viewer が更新されない
      - zip内のファイルで終了したときそのファイルではなくファイラーを起動してしまう
      - ファイラーで zip を選んだとき loading / wait 表示が弱い

- [ ] P3 filesystem: Recursive navigation が大きい実ディレクトリで止まる問題（継続調査）
    - `state-1775968737116-58500.jsonl` の計測で、停止の主因は zip 展開ではなく `kind=real` の directory scan だった
    - `filesystem.navigation.resolved elapsed_ms=71657` と `filesystem.scan_directory_listing kind=real elapsed_ms=71656` が一致
    - zip/listed の全 child 展開を lazy にしたことで `state-1775969500278-61124.jsonl` では最大 `1593ms` まで改善
    - 次にやること:
      - `Recursive` 用の親ディレクトリ列挙 cache を分離する
      - `child_directories()` 用の軽量 listing を導入して `files` 情報と分ける
      - `Next/Prev/Last` の request が large parent scan に巻き込まれないようにする
      - 上の `Recursive branch change` issue と密接に関連している
- [ ] P3 サブファイラー：右→左表示の修正（表示位置の固定）
- [ ] P3 サブファイラー：サブファイラーの表示が current 近傍優先ではない
- [ ] P3 サブファイラー：カレントのファイルではなく最後から読み始める
