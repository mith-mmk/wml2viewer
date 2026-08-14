# wml2viewer iOS / iPadOS 再実装計画

この文書は、`android.md` の機能仕様と `docs/ios-platform-contract.md` の接続契約を基準に、wml2viewerをiPhone / iPad向けに再実装するための仕様を定義する。

- 対象OS: iOS / iPadOS 17以降
- bundle ID: `io.github.mith_mmk.wml2viewer`
- UI: SwiftUI。OSファイラー部分だけUIKitのDocument Browser / Document Pickerをbridgeする
- native core: `wml2viewer-core`と共通mobile bridge
- 対応端末: iPhone / iPad
- デスクトップ版: UI、`config.toml`、プラグイン仕様を変更しない
- Android版: Compose UI、JNI名、native library名、Kotlin ABIを変更しない

iOS版で最も重要なAndroid版との差は、アプリ内ファイラーと独自SMB実装を持たず、ファイルの探索・管理・クラウド接続・SMB接続をOSのFiles / Document Browserへ委譲する点である。アプリが所有するのは、選択されたファイルまたはフォルダを読み取って表示する処理、ユーザーが明示許可したsourceの登録一覧、閲覧中sourceのfilmstrip、viewer設定、復元情報だけである。FilesのRecents / Favoritesは複製しない。

## 1. 旧iOS案からの再実装方針

旧`codex/ios-mvp`は参考資料に留め、コードの移植元にはしない。次の要素は再利用しない。

- eframeによるiOS UI
- marker file pollingによるSwift / Rust連携
- 選択フォルダ全体のsnapshot copy
- アプリ内SMB browser、SMB client、SMB資格情報保存
- Files由来のURLを通常の永続filesystem pathとして扱う設計
- デスクトップ`config.toml`のiOS利用

再実装では、SwiftUIが画面と状態を所有し、Swift concurrencyを使ってFile Providerとnative coreを接続する。Rustは画像・書庫・listed-file・読書計画と、platform非依存のhandle/request管理だけを所有する。

## 2. 全体アーキテクチャ

| 層 | 主な責務 | 所有してはならないもの |
|---|---|---|
| `wml2viewer-core` | 内蔵decode/encode、ZIP/LHA/listed-file、ページ順、見開き、前後anchor、先読み | UIKit、SwiftUI、URL、bookmark、翻訳済み文言 |
| 共通mobile bridge | handle registry、request順序、cancel/stale、bounded I/O、memory上限、reading wire | JNI、Swift、File Provider URL、UI state |
| `wml2viewer-ios` | C ABI、FFI引数検証、panic遮断、共通bridgeの型変換 | security scope、bookmark、画面状態 |
| iOS data/platform | Document Browser、Document Picker、bookmark、NSFileCoordinator、materialize cache、ImageIO | Rust UI、Android SAF / SMBJ / Room / Keystore |
| `ViewerStore` | SwiftUI向けstate、画面遷移、request、viewport generation、one-shot effect | main threadでのdisk/network/decode |
| SwiftUI | viewer、filmstrip、settings、responsive layout、accessibility、L10N | provider I/O、native handleの生存期間管理 |

iOS側は次の責務単位で分ける。

- `App`: SwiftUI app entry、scene lifecycle、外部URL受信
- `UI/Viewer`: viewer surface、chrome、quick menu、empty/error state
- `UI/Filmstrip`: 現在のフォルダまたは書庫内ページだけを表示する補助navigation
- `UI/Settings`: mobile設定8カテゴリ
- `UI/Touch`: UIKit gesture recognizer bridge、3x3 hit test
- `State`: `ViewerStore`、viewport reducer、typed event/effect
- `Documents`: Document Browser / folder picker coordinator、`DocumentSource`、bookmark
- `Codec`: ImageIO capability probe、routing、CGImage変換
- `Cache`: materialized file LRU、decoded image cache、thumbnail cache
- `NativeBridge`: C headerのSwift wrapper、handle ownership、error mapping

disk、network、File Provider download、native decode、全画面pixel変換はmain actor上で実行しない。

## 3. OSファイラーとsourceモデル

### 3.1 OSファイラーへの全面委譲

3×3上中央の標準open actionは、アプリ内の「登録した場所」chooserを表示する。chooserには登録済みフォルダ／書庫と、「フォルダを追加」「ファイルを開く」「ファイル管理」を分離して表示する。「フォルダを追加」は`.folder`専用Document Picker、「ファイルを開く」は`.item`専用Document Picker、「ファイル管理」はDocument Browserを使用する。画像1ファイルのsecurity scopeから親フォルダや兄弟項目を推測しない。

`UIFileSharingEnabled`を有効にし、アプリのDocumentsを「このiPhone/iPad内」とFinderのファイル共有へ公開する。ユーザーがFiles経由でWML2ViewerのDocumentsへ配置したフォルダも、他のFile Providerと同じDocument Picker入口から選択できるようにする。設定、bookmark、cache、一時materializeはDocumentsへ保存せず、Application SupportまたはCachesへ分離する。

copy、move、rename、delete、shareを行う管理画面には`UIDocumentBrowserViewController`を使用し、long press quick menuの「ファイル管理」から全画面で提示する。同等のファイル操作UIはアプリ内に実装しない。

- 公式仕様: [UIDocumentBrowserAction](https://developer.apple.com/documentation/uikit/uidocumentbrowseraction)
- ローカル、iCloud、第三者File Providerを同じOSのDocument Pickerから扱う
- Filesで接続済みのSMB serverも同じ入口から扱う
- SMB接続方法: [Apple Support](https://support.apple.com/en-mide/guide/iphone/iphe9aff429a/ios)
- 第三者クラウド: [Apple Support](https://support.apple.com/en-euro/102238)
- 操作可否、進捗、認証、衝突確認、provider間転送はOSとFile Providerに任せる
- providerが禁止する操作をアプリ独自のcopy/move処理で迂回しない
- ファイル操作のためのRoom相当DB、background transfer journal、通知serviceは作らない
- FilesのRecents / Favoritesをアプリ内に複製しない
- 正常に許可されたfolderとZIP / LHA / LZH書庫だけを「登録した場所」として表示する
- 書庫は内部ページを1件以上decodeできた時点で登録を確定し、空・破損書庫は登録しない
- 通常画像は包含フォルダへ昇格した後だけ登録し、単一画像bookmarkは最後の場所として最新1件だけを保持する

folder pickerとfile pickerは別のpresentationとして直列化する。フォルダ選択時は直下をsourceとして列挙し、ファイル選択時はその項目だけを開く。通常画像を選択した場合に限り、file pickerのdismiss完了後に`.folder` pickerを提示して包含フォルダの許可を追加取得する。

- 公式仕様: [Providing access to directories](https://developer.apple.com/documentation/uikit/providing-access-to-directories)
- pickerはcopyではなくopen-in-placeで使用する
- 選択は単一項目とする
- キャンセルはエラーにせず、直前のviewerを維持する
- picker表示中に重複してpickerを提示しない
- cold launch / warm launch / picker resultはflow ID付きの`DocumentOpenCoordinator`で直列化する
- providerの遅延callbackはpresentation IDとflow IDが一致する場合だけ採用する

### 3.1.1 登録した場所と第三者Provider

フォルダまたは自己完結書庫を正常に開いた時点でsecurity-scoped bookmarkを自動登録する。以後、その行を選択した場合はFiles UIを提示せず、選択された1件のbookmarkだけを`.withoutUI`でresolveしてsourceを再構成する。

- chooserの一覧生成時にbookmarkをresolve、folderを列挙、Providerへ接続しない
- 登録数を自動制限せず、個別の登録解除と全消去を提供する
- 同一項目の判定はFile Provider domain / item identifierをSHA-256したopaque IDで行い、取得不能時はbookmark dataのhashへfallbackする
- URL、path、raw provider identifier、bookmark bytesをUI、Rust、ログへ出さない
- 再認証では既存source IDとlogical indexを維持し、bookmarkだけを置換する
- 保存状態は`unknown`、`available`、`offline`、`authenticationRequired`、`permissionRevoked`、`providerUnavailable`へ正規化する
- 状態確認は行を選択した時だけ行い、失敗時も現在表示中のsourceと3×3入力を維持する
- Files Pickerの回路遮断中も、登録済みbookmarkの直接復元は許可する
- 通常Cancelは失敗回数へ加算せず、delegateなしdismiss、watchdog、手動復旧だけを加算する

OneDrive、Dropbox等にも専用SDK、OAuth、Graph API、独自ログイン、資格情報保存を追加しない。Apple Document ManagerまたはProvider extensionが異常終了した場合はchooserへ戻し、既に登録済みのsourceを直接開ける状態を維持する。初回許可前のOS / Provider障害を迂回して権限を得る方法は実装しない。Provider名によるblacklistは作らず、実機受入を「確認済み」「制限あり」「未確認」で管理する。

### 3.2 sourceとentryの識別

Swift共通ロジックでは、外部項目を次の論理参照で識別する。

```text
EntryRef(sourceId, opaqueEntryId)
```

- `sourceId`はアプリが発行するUUID
- `opaqueEntryId`は`DocumentSource`だけが解釈する
- UI、設定、RustはFile Provider URLやbookmarkを解析しない
- URL文字列、filesystem path、bookmark bytesをRustへ渡さない
- URLやbookmarkをログ、error引数、analyticsへ出さない

`DocumentSource`は読取専用契約として次を公開する。

- `list`
- `stat`
- `coordinatedRead`
- `materialize`
- `thumbnail`
- `refresh`

`create`、`copy`、`move`、`rename`、`delete`は追加しない。これらはDocument Browser / Filesの責務である。

### 3.3 単一ファイルとフォルダの動作

単一ファイルを選択した場合:

- 通常画像はまず1項目だけをsourceとして表示する
- ファイル／フォルダopen pickerを閉じた後、`.folder`のDocument Pickerで包含フォルダの明示的な許可を求める
- 包含フォルダの許可後だけ直下の対応項目を列挙し、元の選択画像の位置から前後移動できるようにする
- 包含フォルダの選択をキャンセルした場合は、単一画像sourceとして表示を維持する
- folder pickerは最初のpickerのdismiss完了後に直列提示し、選択ファイルの親を初期位置として提案する
- 許可後はFile Providerのresource identifierを優先し、同一ファイル名をfallbackとして元画像のindexを維持する
- folder bookmarkには選択項目のopaque entry IDとlogical indexを保存し、前後移動や外部変更後に更新する
- ZIP / LHA / LZHは書庫内entryをページとして扱う
- `.wmltxt`はlisted-fileとして開く
- 単一ファイルに与えられたsecurity scopeから親フォルダの権限を推測したり、兄弟項目を暗黙に列挙したりしない
- フォルダ許可が得られない間は前後移動先を持たず、そのページで停止する

フォルダを選択した場合:

- 選択フォルダ直下だけを列挙する
- File Provider placeholderでは`isRegularFile`が未取得でも除外せず、明確にdirectoryの項目だけを除外する
- サブフォルダを再帰的に混在させない
- 対応画像、ZIP、LHA/LZH、`.wmltxt`をsort設定に従って並べる
- subdirectoryはfilmstripへ表示しない
- 別フォルダへ移動する場合は再度OSファイラーを開く
- 選択項目が画像または書庫ならviewerを前面に戻す
- 空フォルダまたは対応項目がない場合は再選択可能なempty stateを表示する
- 対応項目が1件だけの場合は、接続成功を維持したまま「他の対応ファイルなし」を表示する

サブフォルダの再帰列挙は初版では実装しない。順序、重複、上限、Provider負荷、フォルダ終端動作を決定した後のTODOとする。

単一選択した`.wmltxt`が兄弟または子孫ファイルを参照する場合、ファイル単体のsecurity scopeでは解決しない。次の手順を使う。

1. listed-fileをparseし、正規化済み相対entry名を得る
2. 必要entryが許可範囲外なら「包含フォルダへのアクセスが必要」と表示する
3. folder pickerで包含フォルダを選択してもらう
4. `.wmltxt`が選択フォルダ配下にあることを確認する
5. `..`、absolute path、scheme付きpath、symlinkによるroot外脱出を拒否する
6. 同じFile Providerから対象entryだけをmaterializeする

## 4. security-scoped bookmarkと状態復元

security-scoped URLへのアクセスは処理単位で開始し、必ず同じ処理で終了する。

- `startAccessingSecurityScopedResource()`が成功した場合だけ`stopAccessingSecurityScopedResource()`を呼ぶ
- scope lifetimeをnative handleやSwiftUI view lifetimeへ暗黙に結び付けない
- 列挙、thumbnail、materialize、ImageIO decodeは`NSFileCoordinator`経由で行う
- coordination callback内のURLだけをその処理で使用する
- scene破棄後のcallbackはgenerationで棄却する

bookmarkは`BookmarkStore`がApplication Supportにatomic保存する。設定とは別ファイルにし、OS履歴は複製せず、ユーザーが明示許可した登録sourceだけをUIへ安全なsummaryとして表示する。

保存する情報:

- source ID
- fileまたはfolderを表すsecurity-scoped bookmark data
- source kind
- 表示用名称
- current entryのprovider-opaque ID
- archiveかどうか
- logical page index
- source kind、登録有無、登録日時、最終open日時
- hash化したprovider domain / item opaque ID
- 正規化済み最終status

保存しない情報:

- URL文字列
- filesystem path
- SMB server、username、password
- cloud authorization token
- temporary materialized path

復元手順は次のとおり。

1. `MobileConfigV1`を読み込む
2. `BookmarkStore`から最後のsourceを解決する
3. stale bookmarkなら同じアクセス中にbookmarkを再生成してatomic置換する
4. sourceを列挙し直す
5. current entryをopaque IDで復元する
6. current entryが消失していれば旧sort位置に最も近い項目を選ぶ
7. archiveならlogical page indexを範囲内へclampする
8. bookmark失効、provider未認証、SMB未接続、cloud未downloadならretry / reselect可能なempty stateを表示する

起動直後にpickerを自動表示しない。復元不能時は理由をローカライズして表示し、ユーザー操作でOSファイラーを開く。

アプリがforegroundへ戻った時、またはDocument Browserを閉じた時はsourceを再列挙する。外部rename / move / delete後も、古いURLを使い続けない。

## 5. SwiftUI UI

### 5.1 外観

既定テーマはAndroid版と同じ「Cinematic Dark」とする。

- 黒から墨色を基調にした画像優先UI
- 低彩度シアンのaccent
- edge-to-edge既定ON
- Light / System themeも選択可能
- iOSのDynamic Typeを尊重する
- Dynamic Color設定は持たない
- system barの明暗を選択themeへ追従させる
- touch targetは原則44pt以上
- iOS system fontを使用し、CJK fontをbundleしない

### 5.2 iPhone

- portrait / landscapeともviewerを全画面の主画面とする
- compact landscapeでは上部chromeを非表示にして画像領域を優先する
- filer actionは「登録した場所」chooserを表示する
- chooserの「フォルダを追加」「ファイルを開く」から用途別Document Pickerを表示する
- Document Browserはlong press quick menuの「ファイル管理」からだけ表示する
- settingsは全画面sheetまたはnavigation stackで表示する
- filmstripは下部sheetとして表示する
- file選択後はdecode完了を待たずviewerへ戻り、loading stateを表示する

### 5.3 iPad

- viewerを主領域とし、Android版の常設folder navigationは再現しない
- Files / Document Browserは全画面またはsystem標準presentationで表示する
- 十分な横幅がある場合、filmstripを固定表示できる
- Split View、Stage Manager、外部displayでgeometryが変わった時はlayoutを再評価する
- iPad判定は`userInterfaceIdiom == .pad`を基準とし、iPhone横向きをtablet UIとして扱わない

初版は単一scene / 単一active sourceとする。複数window対応は対象外とし、Info.plistでも複数sceneを有効化しない。

### 5.4 panel入力遮断

settings、source chooser、filmstrip、quick menu、export sheet、alert、Document Picker、Document Browser上ではviewerの3x3 tapを発火しない。overlay表示中はviewer gesture bridge全体を無効化する。overlayを閉じたpointer sequenceをsingle tapへ降格させない。

## 6. 3x3タッチとgesture

3x3判定は表示画像の矩形ではなく、safe areaを除いた`ViewerSurface`全体の相対位置を使う。letterbox部分も対象とし、zoom / pan / fit modeで境界を変えない。

既定割当:

| 左列 | 中央列 | 右列 |
|---|---|---|
| 前 | 登録した場所 | 次 |
| 前 | 設定 | 次 |
| 前 | filmstrip | 次 |

割当可能actionはviewer内で安全に完結するものに限定する。delete、moveなどのファイル操作は割り当てない。

- previous / next
- first / last
- zoom in / out / reset
- fit切替
- animation切替
- grayscale切替
- manga mode切替
- 登録した場所
- settings
- filmstrip
- quick menu
- export
- reload
- disabled

gesture既定値:

- swipe: OFF
- pinch zoom: ON
- pan: ON
- double tap: `CONTAIN` / `ORIGINAL`のruntime切替
- long press: quick menu
- zoom範囲: 1.0～8.0
- zoom button: 1回1.25倍

優先順位:

```text
pinch > pan > swipe > long press > double tap > single zone tap
```

SwiftUIが画面と描画を所有し、gesture arbitrationには薄い`UIViewRepresentable`を使う。内部のUIKit gesture recognizer間にfailure dependencyを設定し、同じpointer sequenceから複数actionを発行しない。

回転、Split View resize、Stage Manager resizeでは`viewportGeneration`を進める。見開き再構成が必要な時は、同じgenerationのspreadがpublishされるまで`touchReady=false`とする。

- 旧decode結果はgenerationとrequest IDで棄却する
- zoom倍率は維持する
- panは新surfaceで画像が見える範囲へclampする
- portrait / landscapeの高速切替では最後のgenerationだけを有効にする

## 7. マンガ・見開き

設定と既定値はAndroid版に合わせる。

- layout: `AUTO` / `SINGLE` / `SPREAD`
- reading direction: RTL / LTR
- cover alone: ON
- divider: OFF
- page spacing: 0pt（0～64ptで設定可能）
- prefetch spreads: 1

動作:

- `AUTO`はlandscape viewportだけ見開きを許可する
- `SINGLE`は常に単ページ
- `SPREAD`は向きに関係なく見開きを許可する
- portrait page同士だけを組にする
- 横長page、相方なし、source境界では単独表示する
- source先頭pageはcover alone有効時に単独表示する
- RTLのnavigation順と物理的な左右表示順を分ける
- 回転後は現在のlogical pageを含むcanonical spreadを再構成する
- 2ページは個別の半画面へfitせず、一つのspread canvasへ共通倍率で隣接描画する。中央の空きは`page spacing`だけとし、既定値0ptでは綴じ目を接触させる

見開き、前後anchor、visual order、prefetchは`wml2viewer-core`のreading plannerを唯一の実装とし、Swiftで同じ算術を再実装しない。

## 8. Mobile設定

iOS設定はdesktopの`config.toml`やAndroidのProto DataStoreを共有しない。version付き`Codable`型`MobileConfigV1`としてApplication Supportへatomic保存する。

カテゴリは次の8つとする。

1. 表示
2. マンガ
3. タッチ領域
4. Filesと復元
5. コーデック
6. 言語と外観
7. キャッシュ
8. 情報

主な既定値:

- edge-to-edge: ON
- keep screen awake: OFF
- initial fit: `CONTAIN`
- top chrome: ON。ただしiPhone landscape viewerでは非表示
- filmstrip: ON
- manga: `AUTO` / RTL / cover alone
- theme: Cinematic Dark
- language: System
- text size: System
- cache: Auto
- 最後の場所を記憶: ON

除外項目:

- window位置 / size
- pane side
- keyboard / mouse mapping
- file association設定
- external plugin設定
- Android Dynamic Color
- SAF profile
- SMB profile / credential
- app内ファイラーのsort / hidden file設定。ただしfolder sourceのページ順に使うsort設定は保持する

config writeはtemporary fileへの書込みとatomic replaceで行う。未知fieldはschema version migrationで安全にdefault化し、既存field名を変更しない。

## 9. I18N / accessibility

- String Catalogで英語と日本語を管理する
- すべての表示文をresource key化する
- build時に英語 / 日本語のkey差分を検査する
- languageはSystem / English / Japaneseを切替可能にする
- 日時、数値、file sizeはlocaleを考慮する
- 3x3の物理的な左右配置はRTL localeでも反転させない
- VoiceOver label、value、hint、selected stateを設定する
- clickable rowの二重focusを避ける
- 44pt touch target、Dynamic Type、長文折返し、pseudo languageをtest対象とする
- native error codeはSwift側でローカライズし、Rustから翻訳済み文言を返さない

## 10. codecと画像表示

### 10.1 routing

既定は`INTERNAL_FIRST`とする。形式別に次を選択できる。

- `DEFAULT`
- `INTERNAL_FIRST`
- `OS_FIRST`
- `INTERNAL_ONLY`
- `OS_ONLY`

OS decode / encodeにはImageIOを使用する。対応を固定表だけで決めず、実fixtureまたはround-tripによる起動時probeを行い、成功した形式だけをOS対応として設定画面へ表示する。

probe対象:

- JPEG
- PNG
- GIF
- WebP
- BMP
- ICO
- HEIF
- AVIF
- DNG

generic UTIまたは不明なcontent typeでは、既知の拡張子とsignatureから形式を決めてroutingする。provider表示名だけを信用しない。

### 10.2 animation

- 内蔵codecはGIF / APNG / WebPのframe数、loop、durationを公開する
- ImageIO経路は`CGImageSourceGetCount`とframe propertyを検査する
- OS decoderがanimated sourceをposter一枚として返した場合、静止画として成功扱いしない
- `OS_FIRST`は内蔵animationへfallbackする
- `OS_ONLY`またはfallback失敗時は`OS_ANIMATION_UNSUPPORTED`を表示する
- 親native imageを保持し、表示frameだけを短命なchild handleからcopyしてreleaseする

### 10.3 描画

SwiftUI `Canvas`で単ページまたは見開きの`CGImage`を描画する。内部RGBAから`CGImage`を生成する時は、所有bufferの生存期間を明示し、native handle release後のborrowed pointerへアクセスしない。

- full image相当の一時`[UInt32]`を作らない
- conversionはtileまたはbounded copyで行う
- current pageを最優先し、prefetchは低優先Taskに分ける
- source変更、ページ変更、回転では古いTaskをcancelする
- cancel不能なFile Provider callbackのlate resultはrequest IDで棄却する

### 10.4 export

export形式は現在利用可能なencoderだけから提示する。

- PNG
- JPEG
- WebP lossy
- WebP lossless

encode結果はアプリのtemporary directoryへ一度生成し、`UIDocumentPickerViewController(forExporting:)`またはshare sheetへ渡す。保存先選択、collision、provider transferはOSへ委譲する。

- viewerから外部sourceへ直接上書きしない
- current directoryへの暗黙保存を行わない
- export完了 / cancel後にtemporary fileを削除する
- process終了で残ったexport tempは次回起動時に期限付きcleanupする

## 11. cacheとmemory上限

Rustがseek可能なlocal fileを必要とする時だけ、選択項目をCachesへオンデマンドmaterializeする。フォルダ全体、隣接ファイル全体、cloud source全体をcopyしない。

Auto cache上限:

- 空き容量の10%
- 最小目標256MiB
- 最大2GiB
- 常に1GiB以上の空きを残す
- 単一materialized entry上限64MiB
- lease中 / decode中 / export中entryはevictしない

viewer memory上限:

- poster: 4096x4096 pixels
- animation: 4096 frames
- poster + animation RGBA: 128MiB
- decoded `CGImage` cache: 128MiB cost-based LRU
- direct encoded image: 64MiB
- archive container: 64MiB
- materialized archive entry: 64MiB
- archive + entryの同時保持: 128MiB
- oversized itemはdecoder / parser呼出し前に拒否する

materialized cache、thumbnail cache、export tempはbackup対象外とする。memory warning時はcurrent pageと使用中spread以外のdecoded / thumbnail cacheを解放する。

## 12. Rust bridgeとC ABI

### 12.1 共通bridgeへの抽出

`wml2viewer-android`内の次のplatform非依存処理を新しい共通mobile bridge crateへ抽出する。

- process-global handle registry
- sessionとrequest lifecycle
- bounded file read
- decode / encode memory limit
- image / animation ownership
- archive / encoded bytes ownership
- error codeとtyped args
- reading plan wire

`wml2viewer-android`はJNI変換だけを所有する薄いwrapperにする。次を完全に維持する。

- native library名`wml2viewer_android`
- 既存JNI export名
- Kotlin declaration
- handle semantics
- error code
- reading wire v1

### 12.2 iOS static library

workspaceに`wml2viewer-ios` crateを追加し、`staticlib`としてbuildする。C ABIは共通bridgeを包み、次を公開する。

- session create / release
- request ID allocate / begin / cancel
- local materialized item decode
- image width / height / stride / RGBA borrow
- animation frame count / loop / duration / child frame handle
- archive open / entry count / normalized name / declared size
- archive entry decode / materialize
- RGBA encodeとowned bytes handle
- reading plan wire v1
- request error codeとtyped JSON args
- image / archive / bytes release

全handleはnonzero `uint64_t`とし、process内で型を跨いで再利用しない。releaseは冪等で、invalid、wrong-kind、double releaseは`false`を返す。session releaseは新規requestを拒否するが、既に返却済みのimage / archive / bytesは各wrapperが明示releaseする。

C ABI規則:

- 全pointer / lengthの組合せを検証する
- nullは仕様で許可された場合だけ受け入れる
- integer overflowをchecked arithmeticで拒否する
- 全exportを`catch_unwind`で保護し、panicをSwiftへ越境させない
- borrowed RGBA / bytes pointerは所有handle release直後に無効となる
- Swiftはborrow中に必ずowned `Data` / `CGImage`へ必要分をcopyする
- error引数にpath、URL、bookmark、username、credential、secretを含めない

error codeはAndroidと一致させる。

| code | 意味 |
|---:|---|
| 0 | none |
| 1 | invalid handle |
| 2 | invalid request |
| 3 | stale request |
| 4 | cancelled |
| 5 | I/O |
| 6 | decode / archive |
| 7 | limit |
| 8 | encode |

### 12.3 Swift ownership wrapper

Swift側は次のfinal classを提供する。

- `NativeSession`
- `NativeImage`
- `NativeArchive`
- `NativeBytes`
- `NativeReadingPlanner`

各resource wrapperは明示`close()`と`deinit`の両方から同じ冪等release処理を呼ぶ。UIへraw handleやpointerを公開しない。concurrency domainを明示し、scene破棄後のpublishを禁止する。

## 13. Xcode projectとapp integration

`ios/WML2Viewer.xcodeproj`を追加し、iPhone / iPad共通targetを作成する。

- deployment target: 17.0
- supported device families: iPhone / iPad
- orientations: portrait、portrait upside downはiPadのみ、landscape left / right
- bundle ID: `io.github.mith_mmk.wml2viewer`
- Swift concurrency checkingを有効化する
- production signing設定は登録しない

Rust build phaseはXcodeの`PLATFORM_NAME`と`ARCHS`からtargetを選ぶ。

- device arm64: `aarch64-apple-ios`
- Simulator arm64: `aarch64-apple-ios-sim`
- Simulator x86_64: `x86_64-apple-ios`

生成したstatic archiveはDerivedData配下へ置き、repositoryへcommitしない。複数Simulator archが要求された場合だけ`lipo`で統合する。build scriptはrepository外の固定pathや個人環境変数へ依存しない。

Info.plistには次を登録する。

- 対応image UTType
- ZIP
- LHA / LZH custom imported UTType
- `.wmltxt` custom exported UTType
- document browser対応
- open in place
- 外部file open

独自SMB実装がないため、local network usage description、SMB entitlement、SMB用Keychain access groupは追加しない。

## 14. thread、cancel、performance

- `ViewerStore`は`@MainActor`とし、UI stateだけを更新する
- File Provider I/O、cache、ImageIO、native bridgeは専用actor / detached taskで実行する
- current decodeとprefetchは別sessionにする
- 優先度は`current > current spread companion > preload > filmstrip thumbnail`
- hidden filmstripのthumbnail生成を停止する
- request IDとviewport generationの両方が一致する結果だけをpublishする
- app background移行時はprefetchとthumbnailをcancelする
- current pageのcoordinated readは必要なら継続し、expiration時は安全にcancelする

性能目標:

- cached next / previous p95: 100ms以内
- 4K local cold display: 1秒以内
- 100ページ後のnative / RGBA memory: warm baseline比20%以内へ回復
- cloud / SMBの初回表示時間はproviderとnetwork依存として計測値を記録し、Simulatorだけで合格を主張しない

## 15. failureとedge case

次をtyped stateとして区別し、単一の「読み込み失敗」にまとめない。

- picker cancelled
- bookmark stale and renewed
- bookmark permission revoked
- provider authentication required
- SMB disconnected in Files
- cloud item not downloaded
- coordinated read failed
- source item moved or deleted
- materialize cache full
- native limit exceeded
- unsupported format
- OS animation unsupported
- stale / cancelled request
- export cancelled / failed

失敗時は既に表示済みのcurrent pageを可能な限り維持する。retryによって自動的に別ページへ進めない。再選択が必要な場合だけOSファイラーを開くactionを提示する。

## 16. テスト

### 16.1 Rust

- workspace unit / doc test
- format / clippy
- common bridgeのhandle型、wrong-kind、double release、use-after-release
- request ID単調増加、invalid begin、cancel、stale result
- decode / animation / archive / encode上限
- reading wire v1互換
- null pointer、invalid length、overflow
- C ABIからpanicが越境しないこと
- Android JNI wrapperの既存testとexport照合

### 16.2 Swift unit test

- `MobileConfigV1` serialize / migration / atomic replace
- bookmark作成、resolve、stale更新、権限失効
- security scopeの開始 / 終了回数
- relative path正規化とroot外脱出拒否
- 選択項目だけをmaterializeすること
- LRU上限、pin、memory warning
- codec capability probeとroute選択
- OS animation poster-only検出とfallback
- native wrapperのclose / deinit / double close
- error code localization
- last locationとarchive page復元
- 外部rename / move / delete後の再選択
- export temp cleanup

test doubleは`DocumentSource` protocolに対して実装し、実URL、資格情報、個人用cloud accountをunit test fixtureへ保存しない。生成fixtureは`.gitignore`済み`.test*`または`test_data`へ置き、終了後にcleanupする。

### 16.3 UI / Simulator test

- iOS 17のiPhone / iPad Simulator buildとlaunch
- iPhone portrait / landscape
- iPad portrait / landscape / Split View相当resize
- safe area、notch、home indicator、letterboxを含む3x3 hit test
- pinch > pan > swipe > long press > double tap > single tap
- overlay / settings / filmstripによる入力遮断
- 高速回転時の`touchReady`抑止
- current logical pageとzoom維持
- filmstrip選択と書庫entry移動
- 44pt touch target
- VoiceOver semantics
- Dynamic Typeと長文折返し
- English / Japanese / pseudo language
- cold / warm external URL open
- background / foreground、scene recreation相当、復元

OS pickerの内部実装詳細をUI testで固定しない。picker coordinatorのevent変換をfakeで検査し、OS UI自体は実機acceptanceで確認する。

### 16.4 Files / 実機acceptance

iPhoneとiPadで次を確認する。

- On My iPhone / iPadのfileとfolder
- iCloud Drive
- 少なくとも1つの第三者cloud provider
- FilesのConnect to Serverで接続した実SMB2/3 share
- file / folder open
- Document Browserのcopy / move / rename / delete / share
- providerが操作を禁止する場合にOS UIへ従うこと
- folder内の画像順、ZIP / LHA / LZH / `.wmltxt`
- Level 1 / LH5のLZHに含まれるMAG画像の表示と前後移動
- offline、認証切れ、download中断、再接続
- bookmarkによる再起動後復元
- Files側でcurrent itemをrename / move / deleteした後の再同期
- export先としてlocal / cloud / SMBを選択

provider固有操作、SMB、cloud latencyはSimulator CIだけで合格としない。

実機のfolder連続閲覧はDEBUG限定の受入レポートでも証跡化する。`ios/device-provider-acceptance.sh arm DEVICE_ID PROVIDER initial`で初回Files許可、続いて`reopen`で登録bookmarkからFiles UIを開かない直接再表示を個別に確認し、各操作後に`collect`で結果を回収する。レポートはProviderラベル、phase、列挙件数、対応件数、decode・前後移動・filmstrip・thumbnail・error復帰の成否だけを保持し、URL、path、file名、bookmark、credentialを保存しない。両phase成功を「確認済み」、reopenだけ成功を「制限あり」、証跡なしを「未確認」とする。Release buildにはこの診断経路を含めない。

### 16.5 回帰

- `cargo test --workspace --locked`
- `cargo test --workspace --doc --locked`
- `cargo check --workspace --examples --locked`
- desktop Windows / Linux / macOS build
- Android arm64 / x86_64 native build
- Android Gradle check、unit、instrumentation
- Kotlin declarationとJNI exportの一致

## 17. CIと成果物

macOS runnerへiOS jobを追加する。

1. Rust toolchainと3つのApple targetをinstallする
2. common bridgeとiOS C ABIをtestする
3. iOS device arm64 static libraryをbuildする
4. Simulator arm64 / x86_64 static libraryをbuildする
5. C headerとexport symbolの一致を検査する
6. Swift unit testを実行する
7. iPhone / iPad SimulatorでUI testを実行する
8. `CODE_SIGNING_ALLOWED=NO`でgeneric device buildを検証する
9. Simulator `.app`をCI artifactとして保存する

App Store archive、TestFlight、製品署名、provisioning profile、tag push後の自動公開は対象外とする。公開を追加する場合は専用environment approvalを必須とする。

## 18. 実装順序

1. Android bridgeのplatform非依存部分を共通crateへ抽出し、Android回帰testを通す
2. `wml2viewer-ios` C ABI、header、ABI testを実装する
3. Xcode project、Swift native wrapper、local fixture viewerを作る
4. `DocumentSource`、Document Browser、folder picker、bookmark復元を実装する
5. ImageIO routing、materialize cache、animation、archive / `.wmltxt`を接続する
6. viewer surface、gesture、見開き、filmstripを実装する
7. settings、L10N、accessibility、exportを実装する
8. lifecycle、外部変更、failure state、cache cleanupを固める
9. Simulator CIを追加する
10. iPhone / iPad実機でlocal、cloud、SMB acceptanceを完了する

各段階でAndroidとdesktopの回帰testを通し、最後にまとめて互換性を修復する進め方は取らない。

## 19. 対象外・制約

- iOS独自SMB client / browser
- SMB1
- LAN server自動探索
- SMB資格情報のKeychain保存
- アプリ内の汎用ファイラー
- FilesのRecents / Favoritesを複製した一覧
- File Providerが禁止する操作の迂回実装
- folder全体のsnapshot / offline固定保存
- background transfer journal
- 外部plugin codec
- 複数window / 複数active source
- 製品署名
- App Store / TestFlight公開

## 20. 完了条件

iOS版の実装完了は次をすべて満たした時点とする。

- iOS 17以降のiPhone / iPadでbuild、launch、外部file openが成功する
- OSファイラーからlocal、cloud、SMBの対応file / folderを選択できる
- app内SMB UI、SMB dependency、資格情報保存、local network permissionが存在しない
- folder全体をcopyせず、必要項目だけをmaterializeする
- 画像、animation、ZIP、LHA/LZH、`.wmltxt`、見開き、filmstrip、exportが動作する
- gesture、回転、state restoration、accessibility、英日表示のtestが通る
- C ABIのownership、cancel、stale、limit、error contractがtestで保証される
- Android JNI ABIと既存Android / desktop機能に回帰がない
- local、iCloud、第三者cloud、実SMBを使った実機acceptanceを記録する
