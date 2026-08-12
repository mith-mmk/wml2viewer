# iOS UI / Android parity test matrix

この文書は、iOS版をAndroid版の機能基準（`android.md`）に合わせるための再現テストである。既存のUIテストはボタンの存在とsheet表示だけで、下記の挙動を検証していなかった。

## 失敗を再現するテスト

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
