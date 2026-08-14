# iOS UI / Android parity test matrix

この文書は、iOS版をAndroid版の機能基準（`android.md`）に合わせるための再現テストである。既存のUIテストはボタンの存在とsheet表示だけで、下記の挙動を検証していなかった。

## 2026-08-13 最終監査

初回監査で判明した日本語、3×3、gesture、animation、見開き、grayscale、基本export、foreground再列挙は実装済みである。今回さらに、picker完了のexactly-once化、Document Browserの全画面表示、security scope内の`NSFileCoordinator`読取、picker取消後の再列挙、広いiPadの固定filmstrip、codec routingの実decode順への接続を行った。描画はSwiftUI `Canvas`へ移行し、見開きは共通倍率で隣接描画する。既定0ptの綴じ目はiPhone/iPadのpixel回帰、0～64ptの設定値はlayout算術と設定移行testで検証する。

設定カテゴリは`ios.md`の8カテゴリ（Display、Manga、Touch、Files and restoration、Codec、Language and appearance、Cache、About）へ統合し、ThemeはLanguage and appearance内で設定する。独立したAppearanceカテゴリは持たない。

| ID | 残る差分 | `ios.md`要件 | 現状 / 必要な検証 |
| --- | --- | --- | --- |
| FILE-01 | Browser lifecycle | 閉じた時にsource再列挙 | 選択・folder picker取消は接続済み。Document Browser自体のprovider固有dismissは実機検証が必要 |
| FILE-02 | DocumentSource | `EntryRef`と`list/stat/open/materialize/thumbnail`、URL非公開 | `list/stat/coordinatedRead/materialize/thumbnail/refresh`の読取契約を実装。URLはsecurity-scope内部に限定し、UI/Rustへ公開しない。 |
| FILE-03 | bookmark | stale / 失効 / 未接続を再試行可能なempty stateへ | stale/失効・load失敗をretryable stateへ遷移し、設定変更・current indexをatomic bookmarkへ継続保存。実Providerの再認証は実機検証が必要 |
| FILE-04 | listed file | `.wmltxt`の包含folder要求とrelative containment | `.wmltxt`をself-contained archiveから分離し、包含folder許可後に相対entryを正規化・symlink containment検証してsourceへ接続。manifestの外部変更・復元も対応 |
| CODEC-01 | capability | 実fixtureによる起動時ImageIO probe | `ImageIOCodecRouter.capabilityProbe()`が起動時にUTI広告と生成fixtureのencode/decodeを確認し、成功形式だけを返す。単体テストで候補集合と一致を検証。 |
| CODEC-02 | animation fallback | OSがposter化した時の内部fallback、`OS_ONLY`明示error | GIF/APNG/WebPのcontainer markerとImageIO frame countを照合し、poster化を`osAnimationUnsupported`として内部codecへfallback。OS_ONLYはローカライズ済みエラー。52件の単体テストで検証。 |
| EXPORT-01 | export形式 | 利用可能なPNG/JPEG/WebP lossy/losslessだけ提示 | `ImageIOCodecRouter.availableExportFormats`で実encoderをprobeし、quick menuから形式を選択。PNG/JPEG/WebPは成功したencoderだけ提示し、24時間超のexport tempを生成前にcleanupする。53件の単体テストで回帰。 |
| CACHE-01 | materialize policy | auto容量、LRU、lease、最低空き1GiB、backup除外 | 合計上限は空き容量10%を256MiB〜2GiBへclampし、1GiB予約を差し引く。単一materializeは64MiB、LRU eviction、起動時孤児掃除、設定上書き、decode/archive中のpinを実装。 |
| UI-03 | iPhone landscape | 上部chromeを隠す | viewerは常設top chromeを持たず、portrait/landscapeとも画像面と3x3入力を主領域にする。横向きでアイコンが再出現しないことをiPhone回転UI testで検証。 |
| LIFE-01 | memory warning | current spread以外を解放 | UIApplication memory warning observerでthumbnail・in-flight decodeをpurgeし、表示中spreadと入力を維持 |
| LIFE-02 | 外部rename | opaque IDで現在項目を維持 | `fileResourceIdentifier`をopaque IDへハッシュし、resource identifierを返すlocal/providerではrename後も維持する回帰を追加。identifierを返さないProviderは名前fallbackのため実機確認が必要 |
| ERROR-01 | Provider failure UX | offline / 認証切れ / 一時停止を再試行可能な状態で表示 | File ProviderとURLの代表的なエラーコードを安定した英日メッセージへ正規化し、現在sourceの「再試行」とFiles再選択を常に提示。単体テストを追加。実Providerが返す個別コードの実機確認は受入時に行う |

今回のSimulator結果:

- iPhone 17 Pro: Swift unit 60 passed、UI 21 passed、iPad専用1 skipped、0 failures
- iPad Pro 13-inch: Swift unit 60 passed、focused UI 4 passed、0 failures
- 見開き中央pixel、0pt/設定値のlayout算術、日本語「見開き間隔」、Files picker、ZIP/LZH/MAGを同じsuiteで回帰

最新のiPhone Simulator focused UIでは、folderの左右移動・filmstrip、空ZIP後の復帰、設定画面の3件を連続実行し、3/3 passed。英日String Catalogと`.strings`のkey集合は77件で一致する。

設定カテゴリ統合後のiPhone Japanese UI test（設定表示・見開き間隔）は1/1 passed。
ジェスチャー仲裁の依存関係（pinch > pan > swipe > long press > double tap > single zone tap）を明示した後、同じiPhone focused UI 3件とDocument Browser管理導線1件がそれぞれ成功した。device向けarm64 Debug buildも成功した。

3x3の既定割当を`TouchMapConfig`として`MobileConfigV1`へ追加し、Androidと同じ安全な`ViewerAction`集合（前後、先頭/末尾、zoom、fit、animation、grayscale、manga、Files、設定、filmstrip、quick menu、export、reload、disabled）を設定画面から変更できるようにした。double tap / long pressも同じdispatcherへ接続し、旧設定JSONは既定値へ移行する。追加Swift unit 8件を含むViewerModelTests 65件、iPhoneの3x3前後/filmstrip・エラー復帰・日本語UI 4件が0 failuresで通過した。scene backgroundでthumbnail/animationを停止し、active復帰で再開するlifecycle unitも追加した。Files/Document ManagerがProviderクラッシュでdelegateを返さない場合は、同じpicker presentationが残ったままactiveへ戻ったことを検出し、350msの猶予後にpickerをキャンセル相当で閉じてviewerへ復帰するwatchdogも追加した。既に許可済みのbookmarkがある場合は、履歴画面を再表示せず「最後に開いた場所を復元」から直接sourceを再開できる。bookmarkを一時fixtureへ保存してFiles Pickerを一度も提示せずフォルダsourceを再構築するpositive testも通過した。

## 2026-08-14 実機Provider受入追記

`ios/device-provider-acceptance.sh` のarm/collectを、実機 iPad A16 でProviderごとに
個別実行した。過去セッションではlocal、iCloud、third-party、SMBの正常経路を確認したが、
最新セッションの証跡を優先する。レポートにはURL、path、file名、bookmark、credentialを保存していない。

| Provider | 列挙 | 対応 | 前後移動 | filmstrip | thumbnail | 結果 |
| --- | ---: | ---: | --- | --- | --- | --- |
| local | 25 | 8 | 成功 | 成功 | 成功 | passed |
| iCloud | 25 | 8 | 成功 | 成功 | 成功 | passed |
| third-party / OneDrive | 0 | 0 | 未確認 | 未確認 | 未確認 | in-progress |
| SMB | 25 | 8 | 成功 | 成功 | 成功 | passed |

実機受入の証拠範囲は次のとおりである。`passed`は上表の診断レポートで確認済み、
`pending`はOS UIまたはProvider状態を実際に操作する記録がまだ必要な項目である。

| 実機項目 | 状態 | 根拠 / 次の確認 |
| --- | --- | --- |
| local / iCloud / SMBのfolder列挙、前後移動、filmstrip、thumbnail | passed | iPad A16のProvider acceptance report |
| third-party / OneDriveのfolder列挙、前後移動、filmstrip、thumbnail | pending | 最新セッションはDocument Manager/Filesクラッシュ前に0件で停止 |
| local / iCloudの復帰可能エラー後の再表示 | passed | 同reportの`errorRecovery` |
| Files/Document Browserのcopy・move・rename・delete・share | pending | OS標準UIで各操作を実行し、viewer再列挙を記録 |
| offline、認証切れ、cloud download中断、再接続 | pending | 各Providerを意図的に切断・再認証してretry/reselectを記録 |
| `.tests/PRO.LZH`の84件MAGを実機で先頭・中間・末尾表示 | pending | Rust/Simulatorでは検証済み、実機Filesからのopen記録が必要 |
| `.wmltxt`の包含folder許可と相対entry表示 | pending | 実Provider上のmanifestでfolder scopeとcontainmentを記録 |
| Files側rename/move/delete後のcurrent item再同期 | pending | 外部変更を実機で行い、opaque ID/fallbackの結果を記録 |
| local/cloud/SMBをexport先にしたsystem picker転送 | pending | 各OS Providerへのexport完了・cancelを実機で記録 |
| iPhone実機の同一Provider matrix | pending | 現在の実機証拠はiPad A16のみ |

2026-08-14の再受入では、同じiPad A16でlocal Providerを再実行し、
`status=passed`（列挙25、対応8、前後移動、filmstrip、thumbnail）を回収した。
これは過去の診断表を置き換える最新local証跡である。
続けてiCloud Driveも再実行し、`status=passed`（列挙25、対応8、前後移動、filmstrip、thumbnail、
復帰可能エラーからの再表示）を回収した。
第三者Providerの1回目は`status=in-progress`（対応1件）で、複数ページ受入条件を満たさなかったため、
同Providerの再セッションを開始した。合格証跡としては扱わない。
再セッションではOneDrive上の3件フォルダについて列挙・前後移動・エラー復帰は成功したが、
`filmstripOpened=false`、`thumbnailDecoded=false`のため、Provider不良ではなくfilmstrip操作未完了として
`in-progress`に分類した。filmstripを開く下中央タップ（またはQuick menuのPages）を追加確認する。

OneDriveでFiles／Document Managerがクラッシュした場合に備え、picker coordinatorのdismantle callbackを追加した。
delegate callbackが無いままpicker view controllerが破棄された場合は、選択キャンセル相当としてflowをidleへ戻し、
現在source・ページ・3x3入力を維持し、再選択可能な通知を表示する。`testPickerCrashDismissalReturnsToViewerAndPreservesSource`
がSimulatorで成功している。これはOneDrive extension自体を修復するものではなく、OS側クラッシュ後にアプリが固まらないための復旧経路である。

同日10:40 JSTの第三者Provider受入では、OneDriveの履歴を複数回表示した直後に
`com.apple.DocumentManager.Service` が5回連続で終了した。端末のsystemCrashLogsでは
アプリのbundle IDではなくAppleの`com.apple.DocumentManagerUICore.Service`が
`SIGABRT`し、`DOCSidebarViewController.reloadOutlineDiffableData`内の
`DUPLICATE_ITEM_IDENTIFIERS_IN_SECTION_SNAPSHOT` assertionが終了理由だった。
このためfolder commit前の受入レポートは列挙0・対応0となり、OneDrive Providerの内容を
アプリが読めなかった。アプリのpicker復旧経路は保持されるが、Appleの履歴UI assertion自体を
アプリから回避するAPIはない。delegate/dismantleまで届かないケースにもscene復帰watchdogで
viewer・3x3入力を戻せるが、OS/Provider側の修正または履歴を開かない導線での再受入が必要である。
同じbookmarkを持つ場合の再受入・復旧用に、pickerを起動せずsecurity-scoped bookmarkから最後のsourceを直接復元する
「最後に開いた場所を復元」操作をquick menuとエラー／通知に追加した。これにより、履歴サイドバーを
再表示できないProviderでも、過去に一度許可済みのフォルダは再列挙・前後移動の確認を継続できる。

上記`pending`は製品コードの未実装を意味せず、`ios.md`が要求する実Provider操作の証跡不足を意味する。

同じ実機（iPad A16、iOS 26.6、unlocked）で`ios/device-smoke.sh`を再実行し、XCTest、署名build、install、launch、native session/request/cancel/releaseの自己診断が成功した。

なお、Keychainを再設定した後の2026-08-14再確認では、`security find-identity -v -p codesigning`が
`0 valid identities found`を返し、Provisioning Profileも現行Bundle IDに対して解決できなかった。
そのため同日後半の実機smoke再実行は署名build開始前に停止している。これは製品コードの失敗ではなく、
Apple Development証明書／private key／ProfileをXcode Accountsから再取得した後に再試行する環境条件である。
Simulatorのunsigned buildとunit/UIテストには影響しない。
その後の`device-smoke.sh`再試行では、Profile解析fallbackによりBundle ID用Profileは検出できたが、
CoreDeviceServiceの初期化timeoutで端末操作へ進めなかった。
同じ環境でのgeneric Simulator `build-for-testing`再試行も、Swift/Rustコンパイルではなく
`CompileAssetCatalogVariant`中のCoreSimulatorService接続断で停止した。サービス復旧前の再試行結果は
製品ビルド失敗の証拠として扱わない。

空ZIPを含むfolderで、失敗項目を方向に応じてスキップし、前後移動後に
エラー表示と入力ロックが解除される回帰UI testも通過した。現在の作業ツリーは
製品変更なし（ユーザー所有の`.DS_Store`のみ未追跡）である。

以下の表は初回監査時点の失敗を残す履歴である。

## 初回に失敗を再現したテスト（履歴）

| ID | 対象 | 手順 | Android基準 | 現状 |
| --- | --- | --- | --- | --- |
| LOC-01 | 日本語 | iOS言語を日本語にしてcold launch | 全表示文が日本語 | `Localizable.xcstrings`に日本語値がなく英語のまま |
| LOC-02 | アプリ内言語 | Settings > Language > 日本語を選択して閉じる | 再起動せず日本語へ切替 | `MobileConfigV1.language`が`String(localized:)`へ反映されない |
| TAP-01 | 3×3上中央 | fixtureを開きsurface上端中央をタップ | `OPEN_FILER` | chrome表示時はgesture overlayの外。OS pickerが開くかを専用fixtureで確認する必要がある |
| TAP-02 | 3×3中央中央 | surface中央をタップ | `OPEN_SETTINGS` | resolverとUIテストを追加済み。chromeボタンを経由しない座標tapで確認する |
| TAP-03 | 3×3下中央 | surface下端中央をタップ | `OPEN_SUBFILER` | 現テストはbottom chromeのfilmstripボタンを拾う可能性があるため、chrome非表示fixtureが必要 |
| TAP-04 | 全9セル | 9セルを順番にタップ | Androidの`TouchMapConfig`割当 | `TouchMapConfig`の9セル設定とsurface座標dispatch、unit/UI回帰を実装済み |
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
