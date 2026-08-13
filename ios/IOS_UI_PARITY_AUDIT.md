# iOS UI / Android parity test matrix

この文書は、iOS版をAndroid版の機能基準（`android.md`）に合わせるための再現テストである。既存のUIテストはボタンの存在とsheet表示だけで、下記の挙動を検証していなかった。

## 2026-08-13 最終監査

初回監査で判明した日本語、3×3、gesture、animation、見開き、grayscale、基本export、foreground再列挙は実装済みである。今回さらに、picker完了のexactly-once化、Document Browserの全画面表示、security scope内の`NSFileCoordinator`読取、picker取消後の再列挙、広いiPadの固定filmstrip、codec routingの実decode順への接続を行った。描画はSwiftUI `Canvas`へ移行し、見開きは共通倍率で隣接描画する。既定0ptの綴じ目はiPhone/iPadのpixel回帰、0～64ptの設定値はlayout算術と設定移行testで検証する。

| ID | 残る差分 | `ios.md`要件 | 現状 / 必要な検証 |
| --- | --- | --- | --- |
| FILE-01 | Browser lifecycle | 閉じた時にsource再列挙 | 選択・folder picker取消は接続済み。Document Browser自体のprovider固有dismissは実機検証が必要 |
| FILE-02 | DocumentSource | `EntryRef`と`list/stat/open/materialize/thumbnail`、URL非公開 | `list/stat/coordinatedRead/materialize/thumbnail/refresh`の読取契約を実装。URLはsecurity-scope内部に限定し、UI/Rustへ公開しない。 |
| FILE-03 | bookmark | stale / 失効 / 未接続を再試行可能なempty stateへ | stale/失効・load失敗をretryable stateへ遷移し、設定変更・current indexをatomic bookmarkへ継続保存。実Providerの再認証は実機検証が必要 |
| FILE-04 | listed file | `.wmltxt`の包含folder要求とrelative containment | `.wmltxt`をself-contained archiveから分離し、包含folder許可後に相対entryを正規化・symlink containment検証してsourceへ接続。manifestの外部変更・復元も対応 |
| CODEC-01 | capability | 実fixtureによる起動時ImageIO probe | `supports`が固定UTI表のまま |
| CODEC-02 | animation fallback | OSがposter化した時の内部fallback、`OS_ONLY`明示error | frame列挙はあるが、poster化検出と専用errorが未実装 |
| EXPORT-01 | export形式 | 利用可能なPNG/JPEG/WebP lossy/losslessだけ提示 | PNG + share sheetを実装。形式選択・encoder probeは残差分だが、Exportsの24時間超一時ファイルは生成前にcleanupする |
| CACHE-01 | materialize policy | auto容量、LRU、lease、最低空き1GiB、backup除外 | 選択項目のみ64MiB上限とLRU eviction、起動時の孤児materialized file掃除を実装。動的空き容量・lease・最低空き1GiBは実機容量依存の残差分 |
| UI-03 | iPhone landscape | 上部chromeを隠す | 設定値だけで制御し、orientation連動は未実装 |
| LIFE-01 | memory warning | current spread以外を解放 | UIApplication memory warning observerでthumbnail・in-flight decodeをpurgeし、表示中spreadと入力を維持 |
| LIFE-02 | 外部rename | opaque IDで現在項目を維持 | `fileResourceIdentifier`をopaque IDへハッシュし、resource identifierを返すlocal/providerではrename後も維持する回帰を追加。identifierを返さないProviderは名前fallbackのため実機確認が必要 |

今回のSimulator結果:

- iPhone 17 Pro: unit/UI 49 passed、0 failures、iPad専用1 skipped
- iPad Pro 13-inch: unit/UI 50 passed、0 failures
- 見開き中央pixel、0pt/設定値のlayout算術、日本語「見開き間隔」、Files picker、ZIP/LZH/MAGを同じsuiteで回帰

以下の表は初回監査時点の失敗を残す履歴である。

## 初回に失敗を再現したテスト（履歴）

| ID | 対象 | 手順 | Android基準 | 現状 |
| --- | --- | --- | --- | --- |
| LOC-01 | 日本語 | iOS言語を日本語にしてcold launch | 全表示文が日本語 | `Localizable.xcstrings`に日本語値がなく英語のまま |
| LOC-02 | アプリ内言語 | Settings > Language > 日本語を選択して閉じる | 再起動せず日本語へ切替 | `MobileConfigV1.language`が`String(localized:)`へ反映されない |
| TAP-01 | 3×3上中央 | fixtureを開きsurface上端中央をタップ | `OPEN_FILER` | chrome表示時はgesture overlayの外。OS pickerが開くかを専用fixtureで確認する必要がある |
| TAP-02 | 3×3中央中央 | surface中央をタップ | `OPEN_SETTINGS` | resolverとUIテストを追加済み。chromeボタンを経由しない座標tapで確認する |
| TAP-03 | 3×3下中央 | surface下端中央をタップ | `OPEN_SUBFILER` | 現テストはbottom chromeのfilmstripボタンを拾う可能性があるため、chrome非表示fixtureが必要 |
| TAP-04 | 全9セル | 9セルを順番にタップ | Androidの`TouchMapConfig`割当 | `TouchZoneResolver`の9セルunit testを追加済み。surface座標からのdispatchは未完了 |
| GEST-01 | swipe既定値 | 1本指の左右スワイプ | 既定OFF、zoom中は無効 | `ViewerSurface`は常時ページ送り |
| GEST-02 | double tap | 同じ場所を2回タップ | fit modeの一時切替 | iOSはzoomを1↔2へ変更 |
| GEST-03 | long press | surfaceを0.45秒長押し | quick menu | iOSはfilmstripを開く |
| GEST-04 | panel遮断 | settings/filmstrip上で3×3相当の位置をタップ | 背面viewerへ伝播しない | sheet依存で、panel内のaction契約を未検証 |
| VIEW-01 | animation | GIF/APNG/WebP fixtureを開く | frame更新と停止/再開 | iOSはImageIOの初 frameのみ |
| VIEW-02 | 見開き | manga mode + 2ページfixture | reading-planのspread/RTL | iOSにreading-plan接続がない |
| VIEW-03 | grayscale | grayscale actionを実行 | 表示だけグレースケール | iOSに状態/actionがない |
| VIEW-04 | export | PNG/JPEG/WebPをexport | system picker/shareへ渡す | iOSにexport UI/actionがない |
| SOURCE-01 | 外部変更 | 閲覧中の項目をFilesでrename/delete | 再列挙し最寄りへ移動 | iOSに再列挙/reconcileがない |

## 実行方法

まず純粋なmodel/bridgeテストを実行する。

```sh
xcodebuild -project ios/Wml2Viewer.xcodeproj -scheme Wml2Viewer \
  -destination 'platform=iOS Simulator,id=<SIMULATOR_UDID>' \
  CODE_SIGNING_ALLOWED=NO -only-testing:Wml2ViewerTests test
```

次にiPhone/iPadで、cold launch、言語切替、fixture open、9セル座標tap、gesture優先順位、回転、panel遮断をUI XCTestで実行する。Document BrowserやFile Provider固有動作はUI XCTestの合否にせず、Files実機で確認する。

## 合格条件

- `Localizable.xcstrings`の英日key集合が一致し、日本語表示テストが通る。
- 9セルすべてが設定可能な`ViewerAction`へdispatchされ、surface全体（letterboxを含む）で判定される。
- pinch > pan > swipe > long press > double tap > single tapの順で、上位gesture成立後にsingleへ降格しない。
- animation、見開き、grayscale、export、外部変更の各テストがAndroid版と同じ状態遷移を通る。
- iPhone縦横、iPad縦横/Split View、VoiceOver、44pt以上の操作対象で同じテストを通る。
