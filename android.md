# wml2viewer Android 実装仕様

この文書は、`wml2viewer 0.0.19` のAndroid実装について、これまでに決定・実装した仕様を現行`main`のコードを基準にまとめたものである。旧Android MVPの履歴、現行アーキテクチャ、UI、入力、ファイラー、SMB、codec、JNI、永続化、移行、ビルド、テストまでを対象とする。

- 対象バージョン: `0.0.19`
- application ID: `io.github.mith_mmk.wml2viewer`
- 最低OS: Android 10 / API 29
- compileSdk / targetSdk: 36
- UI: Jetpack Compose + Material 3
- Activity: `ComponentActivity`の単一Activity構成
- デスクトップ版: UI、`config.toml`、プラグイン仕様を変更しない
- iOS/iPadOS: この実装には含めず、`docs/ios-platform-contract.md`で接続契約だけを定義する

## 1. 実装の経緯

### 1.1 旧Android MVP（0.0.18）

旧版は`NativeActivity + eframe`を使用し、Rust側の`android_main`からデスクトップUIに近い画面を起動していた。SAFで選択したツリーはアプリ専用領域へ全体コピーし、`PathBuf`前提のビューアへsnapshotとして渡していた。KotlinとRustの連携にはmarker fileの監視も使っていた。

この方式には次の問題があった。

- Android固有の画面設計ができず、デスクトップ用設定や操作体系が混在する
- SAFツリー全体のコピーが容量・待ち時間・同期の面で不利
- URI、Android lifecycle、回転、process deathとの境界が不明瞭
- ファイラー、SMB、資格情報、OS codecを自然に統合しにくい

### 1.2 0.0.19の全面再実装

0.0.19では旧実装を廃止し、Androidを`ComponentActivity + Jetpack Compose`へ全面置換した。Android側がUI、SAF、SMB、資格情報、転送、OS codec、永続化を所有し、Rustはplatform非依存の画像・書庫・読書計画coreとJNI bridgeに限定した。

削除した旧要素は次のとおり。

- `NativeActivity`
- Android向けeframe backend
- Rustの`android_main`
- SAF全ツリーsnapshot
- marker file polling
- デスクトップUIとAndroid UIの共用
- デスクトップ`config.toml`のAndroid利用
- 旧タッチ設定

## 2. 全体アーキテクチャ

| 層 | 主な責務 | 所有してはならないもの |
|---|---|---|
| `wml2viewer-core` | 内蔵decode/encode、ZIP/LHA/listed-file、ページ順、見開き、前後anchor、先読み | Android API、Compose、URI、資格情報、翻訳済み文言 |
| `wml2viewer-android` | JNI、handle registry、request順序、cancel、DirectByteBuffer、書庫entry、encode結果 | SAF/SMB URI、ユーザー名、パスワード、UI state |
| Android data/platform | SAF、SMB2/3、Keystore、Room、WorkManager、cache、OS codec | デスクトップ設定、Rust UI |
| ViewModel / StateFlow | UI state、画面遷移、回転世代、gesture結果、one-shot effect | 直接のdisk/network処理 |
| Compose UI | viewer、filer、settings、スマホ/タブレット適応、accessibility、L10N | provider実装、資格情報の永続化 |

主なAndroid packageは次の責務で分割する。

- `ui`: Compose root、テーマ、言語、画面構成
- `ui/components`: viewer、filer、filmstrip、settings、dialog
- `ui/state`: ViewModel、StateFlow、UI event/effect
- `ui/touch`: 3×3判定とgesture arbitration
- `data/controller`: UIとprovider/JNIを接続するcontroller
- `data/source`: provider共通契約、転送、衝突処理
- `data/transfer`: Room journalとForeground Worker
- `data/config`: Proto DataStore、source profile、last location
- `data/cache`: 一時LRU cache
- `platform/saf`: SAF providerとgrant管理
- `platform/smb`: SMBJ、share列挙、connection管理
- `platform/security`: Android Keystore資格情報store
- `platform/codec`: ImageDecoder/Bitmap codecとrouting
- `nativebridge`: JNIの型付きKotlin wrapper

## 3. ファイル参照とprovider契約

外部ファイルはパス文字列ではなく、次の論理参照で識別する。

```text
EntryRef(providerId, opaqueId)
```

- `providerId`はsource/providerを識別する
- `opaqueId`はproviderだけが解釈する
- SAF URIやSMB pathをUIやRustの共通ロジックで解析しない
- `opaqueId`へ資格情報を含めない
- RustへSAF URI、SMB URI、username、domain、password、credential IDを渡さない

`SourceProvider`は次の操作を公開する。

- `list`
- `stat`
- `openRead`
- `create` / `createDirectory`
- `copy`
- `move`
- `rename`
- `trashOrDelete`
- `thumbnail`

操作可否はprovider種別の分岐ではなく、`SourceCapabilities`とentryごとの`effectiveCapabilities`から決める。読取専用項目やsource rootでは、copy/move/rename/deleteなどの未対応操作をUIに表示しない。source rootの削除は外部root削除ではなく、別の「登録解除」操作として扱う。

実providerは次の2種類である。

- `SafSourceProvider`: Android DocumentsProvider/SAFを直接操作
- `SmbSourceProvider`: SMBJ low-level APIでSMB2/3を操作

書庫内ページはRust coreが提供する読取専用virtual entryとして扱う。

## 4. Android UI

### 4.1 外観

既定テーマは「Cinematic Dark」である。

- 黒から墨色を基調にした画像優先UI
- 低彩度シアンのaccent
- edge-to-edge既定ON
- 半透明panel / bottom sheet
- 原則48dp以上のtouch target
- Light / System themeも選択可能
- Dynamic Colorは既定OFF
- system barのicon明暗は選択themeへ追従
- adaptive launcher iconとround iconを提供

### 4.2 端末区分

端末区分は現在の横幅だけでなく、`smallestScreenWidthDp`を使ってスマホとタブレットを分ける。スマホを横向きにしてもタブレットUIへ誤分類しない。

- スマホ: `smallestScreenWidthDp < 600`
- タブレット: `smallestScreenWidthDp >= 600`
- タブレットnavigation幅: 通常240dp、画面幅840dp以上では300dp

回転、fold、multi-window resizeではlayoutを再評価する。ViewModelが現在ページ、選択、filmstrip/filerのscroll stateを保持する。

### 4.3 スマホ縦向き

- viewer、filer、settingsは全画面で切り替える
- filerで画像または書庫を選択した時点でfilerを閉じ、decode中もviewerを前面にする
- directory選択時はfilerを維持する
- filer上部に明示的な「閉じる」ボタンを持つ
- sub-filerは下部filmstrip sheetとして表示する

### 4.4 スマホ横向き

横向きでもスマホはcompact UIのまま扱う。

- viewerでは上部chromeを隠し、画像表示領域を優先する
- filerは横2ペインにする
  - 左36%: source、folder、navigation
  - 右64%: 現在folderのfile一覧
- navigation側に「閉じる」ボタンを表示する
- file選択後はfilerを閉じてviewerへ戻る
- file一覧が上部操作行に押しつぶされ、1～2行しか見えない旧layoutは使用しない

### 4.5 タブレット

タブレットは常設2ペインを基本とする。

- 左: sourceとfolder navigation
- 右: file一覧、viewer、またはsettings
- filer画面では左にfolder、右にfileを分離する
- viewer画面では左navigationを残し、右をviewerにする
- 横向きではfilmstripを固定表示できる

### 4.6 panel入力遮断

filer、settings、filmstrip、quick menu、export dialogなどの子UI上ではviewerのzone tapを発火しない。panel側がpointer inputをconsumeし、背後のviewerへ入力を通さない。

## 5. 3×3位置タッチとgesture

### 5.1 判定座標

3×3の位置タッチは、表示画像の矩形ではなく**ViewerSurface全体の相対位置**で判定する。タブレットでは右側viewer paneのViewerSurfaceが基準となる。

- letterbox部分も3×3領域に含める
- zoom、pan、`CONTAIN/WIDTH/HEIGHT/ORIGINAL`でzone境界を変えない
- system barとdisplay cutoutの`safeDrawing`領域は除外する
- window座標のinsetをViewerSurfaceのlocal座標へ変換する
- 有効なsurface矩形が空の場合は操作しない

既定割当は次のとおり。

| 左列 | 中央列 | 右列 |
|---|---|---|
| 前 | ファイラー | 次 |
| 前 | 設定 | 次 |
| 前 | サブファイラー | 次 |

9セルは設定画面から変更できる。割当可能なのはviewer内で安全に完結する`ViewerAction`だけであり、deleteやmoveなどの破壊的file操作は割り当てられない。

割当候補には、前後、最初/最後、zoom、fit、animation、grayscale、manga mode、filer/settings/sub-filer、quick menu、export、reload、無効がある。

### 5.2 gesture既定値

- swipe: OFF
- pinch zoom: ON
- pan: ON
- double tap: `CONTAIN`と`ORIGINAL`の一時切替
- long press: quick menu
- zoom範囲: 1.0～8.0
- zoom button倍率: 1回1.25倍

double tapのfit切替はruntime overrideであり、「初期fit」の永続設定を書き換えない。ページが変わると一時overrideを解除する。

### 5.3 優先順位

同一pointer sequenceでは次の優先順位を使う。

```text
pinch > pan > swipe > long press > double tap > single zone tap
```

上位gestureが成立した後にsingle tapへ降格させない。zoom中はpanを許可し、swipeはzoomが1.0の時だけ使用する。

### 5.4 回転中の保護

回転やviewport size変更時は`viewportGeneration`を進める。見開き再構成が必要な場合、次のspreadが同じgenerationでpublishされるまで`touchReady=false`とし、旧spreadと新surfaceを混在させたzone tapを禁止する。

- 旧decodeの完了はgeneration/request IDで棄却する
- 回転後もzoom倍率を維持する
- panは新しいsurfaceで画像が見える範囲へclampし、必要なら中央へ戻す
- `pointerInput`のkeyにsurface touch rectと`touchReady`を含める
- portrait → landscape → portraitの高速回転でも最後のgenerationだけを有効にする

## 6. マンガ・見開き

設定項目は次のとおり。

- layout: `AUTO` / `SINGLE` / `SPREAD`
- reading direction: RTL / LTR
- cover alone
- divider
- prefetch spreads

既定は`AUTO`、RTL、表紙単独、dividerなし、次の見開き1組を先読みする。

- `AUTO`: landscape viewportでのみ見開きを許可
- `SINGLE`: 常に単ページ
- `SPREAD`: 向きに関係なく見開きを許可
- portrait page同士だけを組にする
- 横長page、相方なし、source境界では単独表示
- cover alone有効時はsource先頭pageを単独表示
- RTLではnavigation順と物理的な左右表示順を分ける
- 回転後は現在の論理pageを含むcanonical spreadを再構成する

見開き、前後anchor、visual order、preloadは`wml2viewer-core`が所有する。Kotlinは`NativeReadingPlanner`を通じてstateless JNI `planReading`を呼び、同じ算術を再実装しない。

## 7. モバイル設定

Android設定はdesktopの`config.toml`を使用せず、Proto DataStoreの`MobileConfigV1`へ保存する。

設定カテゴリは次の8つに限定する。

1. 表示
2. マンガ
3. タッチ領域
4. ファイラーとSMB
5. コーデック
6. 言語と外観
7. キャッシュ
8. 情報

主な既定値は次のとおり。

- edge-to-edge: ON
- keep screen on: OFF
- initial fit: `CONTAIN`
- top chrome: ON。ただしcompact横向きviewerでは非表示
- filmstrip: ON
- manga: `AUTO` / RTL / cover alone
- theme: Cinematic Dark
- Dynamic Color: OFF
- language: OS設定に追従
- cache: Auto
- 最後の場所を記憶: ON

Android設定から除外するものは、window位置/size、pane side、keyboard/mouse mapping、file association、external plugin設定である。

## 8. I18N / L10N / accessibility

- 全表示文をAndroid resource keyへ置く
- 英語`values/strings.xml`と日本語`values-ja/strings.xml`は同一key集合を維持する
- Gradle taskで翻訳key差分を機械検査する
- BCP 47 language tagを使い、System / English / Japaneseを切替可能
- app内言語切替のためApp Bundle language splitを無効にする
- 日時、数値、file sizeはlocaleを考慮して表示する
- RTLをmanifestで有効にする
- touch mapの物理的な左/右配置はRTLで反転させない
- clickable rowはTalkBackで二重focusにならないsemanticsを使う
- 48dp touch target、長文折返し、pseudo localeをtest対象とする

## 9. SAF

SAFは選択treeを直接参照し、全treeをアプリ領域へcopyしない。

- `ACTION_OPEN_DOCUMENT_TREE`を使用
- read permissionは必須、writeはproviderが返した場合に保持
- persistable URI grantをsource profileと対応付ける
- 登録失敗時は、その操作で新しく取得したgrantだけを`NonCancellable`でrollbackする
- 既存grantは誤って解放しない
- 起動時にprofileのない孤立grantを照合して解放する
- `DocumentsContract`のentry flagsを`effectiveCapabilities`へ反映する
- rootの破壊的操作をUIから隠す
- permission失効はlocalizable errorとして扱う

copy/move/renameの`REPLACE`は、既存項目をhidden backupへ退避し、新項目のpublish成功後にbackupを削除する。途中失敗では新規項目を片付け、旧項目を復元する。

## 10. SMB

### 10.1 対応範囲

- library: SMBJ 0.14.0 low-level API
- protocol: SMB2 / SMB3のみ
- SMB1: 常に拒否
- port既定: 445
- LAN自動探索: 対象外
- authentication: username/password + optional domain、または明示guest
- share列挙: `IPC$\\srvsvc`の`NetShareEnumAll`
- 列挙拒否時: share名の手入力へfallback

### 10.2 登録UI

SMB追加dialogは3stepである。

1. server / port
2. authentication / guest / domain / encryption設定
3. 列挙されたshareの選択、またはshare名の手入力

passwordはauthentication stepで一度だけ入力する。share stepでは同じdialog state内で短時間保持し、password入力欄を再表示しない。列挙用copyと登録用copyは処理後に`CharArray`をzeroizeする。

### 10.3 接続とsecurity

- 認証失敗をnetwork retryしない
- retry可能なnetwork失敗だけ最大3回、250/750/1500msで再試行する
- signing/encryption状態を接続後に表示する
- encryptionが利用可能ならSMBJ negotiationを使う
- 「暗号化必須」ONで暗号化が成立しない接続は拒否する
- 未暗号化接続では常時warningを表示する
- guestはguest/anonymousのdialect差を吸収する
- share connectionをlease付きcacheで再利用する

保存済みSMB sourceは「資格情報の再入力」と「登録解除」を提供する。Keystore key失効やciphertext不正時は平文fallbackを行わず、資格情報を破棄して再入力を求める。

## 11. 資格情報と秘密情報

SMB passwordは次の方式で保存する。

- Android Keystoreの非抽出AES-256 key
- AES/GCM/NoPadding
- 12-byte random nonce
- profile IDをAADに使用
- ciphertext envelopeは`noBackupFilesDir/credentials-v1`
- Proto DataStoreにはone-way credential IDだけを保存
- 毎回のdevice authenticationは要求しない

passwordはログ、Room、DataStore、Rust、backup、crash reportへ平文で残さない。例外文は`SecretRedactor`でSMB user-infoやsecret形式をredactする。Gradleとinstrumentationでschemaおよび実データのsecret漏洩を検査する。

## 12. 転送、衝突、削除

copy、move、upload、exportは原則として次の手順を使う。

1. destinationに一時名を作る
2. stream copyする
3. closeする
4. byte countを確認する
5. 必要な経路ではSHA-256を再読検証する
6. renameでfinal名へpublishする
7. moveの場合だけ、commit確認後にsourceを削除する

providerを跨ぐmoveはSHA-256一致後にだけsourceを削除する。失敗・cancel時はsourceを保持し、一時項目とreplacement backupを安全にcleanup/restoreする。同一source・同一pathへのmoveはno-opとして扱い、commit後の自己削除を防ぐ。

衝突時は次を選ぶ。

- 上書き / `REPLACE`
- 別名 / `KEEP_BOTH`
- skip / `SKIP`

現行controllerが扱う衝突確認は単発operationであり、「以後すべて」は表示しない。modelとeventには将来のbatch向け`applyToAll`を残しているが、`supportsApplyToAll=false`のため現行UIでは無効である。

削除はproviderのtrashを優先し、利用できない場合だけ永久削除として警告・確認する。

### 12.1 転送journal

- DB: Room `transfer-jobs-v1.db`
- entity: `TransferJobV1`
- execution: Foreground WorkManager
- notification: progressとcancel action
- process death後: Roomのphase、staging、planned final、backup、digestからresumeまたはrollback
- OSによるWorker停止とユーザーcancelを区別する
- OS停止やconstraint lossはjournalを保持して再queueする
- 明示cancelだけをterminal `CANCELLED`にする

## 13. codecと画像表示

### 13.1 routing

全体既定は`INTERNAL_FIRST`である。形式別に次を選択できる。

- `DEFAULT`
- `INTERNAL_FIRST`
- `OS_FIRST`
- `INTERNAL_ONLY`
- `OS_ONLY`

Android OS decodeは`ImageDecoder`、OS encodeはBitmap/platform encoderをworker threadで実行する。起動時に実fixtureまたはround-tripで端末能力を測定し、成功した形式だけをOS対応として設定画面へ表示する。対象形式はJPEG、PNG、GIF、WebP、BMP、ICO、HEIF、AVIF、DNGである。

SAFが`application/octet-stream`などgeneric MIMEを返した場合は、既知のfile extensionから形式を推定して形式別policyを適用する。

### 13.2 animation

- 内蔵codecはGIF/APNG/WebPのframe数、loop、durationを公開する
- Kotlinは親native imageを保持し、必要frameだけを短命なchild handleからcopyしてreleaseする
- OS decoderがanimated sourceをposter一枚として返した場合、無言で静止画扱いしない
- `OS_FIRST`は内蔵animationへfallbackする
- `OS_ONLY`または内蔵fallback失敗時は`OS_ANIMATION_UNSUPPORTED`を表示する
- animation ON/OFFはquick actionから切り替えられる

### 13.3 export

export formatは現在のrouteで利用可能なbackendだけから算出する。

- PNG
- JPEG
- WebP lossy
- WebP lossless

保存先は次の2種類である。

- 現在の書込可能directory（SAFまたはSMB）
- system pickerで選択したtree

system picker exportも`ACTION_OPEN_DOCUMENT_TREE`とSAF atomic writerを使う。transient grantではprocess death後のreplacement復旧を保証できないため、既存fileへの`REPLACE`は無効とし、`KEEP_BOTH`または`SKIP`を使う。

## 14. cacheとmemory上限

### 14.1 source cache

SMBやprovider itemは、Rustがseek可能なlocal fileを必要とする場合だけapp cacheへオンデマンドmaterializeする。

Auto上限は次の規則で計算する。

- 空き容量の10%
- 最小目標256MiB
- 最大2GiB
- 常に1GiB以上の空きを残す
- 単一entry上限512MiB
- lease中/transfer中entryはpinしてevictしない

### 14.2 viewer memory

- poster上限: 4096×4096 pixels
- animation上限: 4096 frames
- poster + animation RGBA所有量: 128MiB
- Kotlin Bitmap cache: 128MiB byte-weighted LRU
- RGBAからBitmapへの変換: 全画面`IntArray`を作らずtile変換
- direct encoded image: 64MiB
- archive container: 64MiB
- materialized archive entry: 64MiB
- archive + entryの同時保持: 128MiB
- oversized itemはdecode/parser呼出し前に拒否する

vendored `wml2 0.0.23`のAndroid経路では、dimension、frame数、aggregate RGBA、zlib/LZW展開、composition loop、cancelをallocation前または処理途中で検査する。desktop互換の従来APIは変更しない。

## 15. JNI ABI

native library名は`wml2viewer_android`である。Kotlin側は`NativeBridge`を直接UIへ露出せず、`Closeable` wrapperを使う。

resource typeは次の4種類である。

- `NativeSessionHandle`
- `NativeImageHandle`
- `NativeArchiveHandle`
- `NativeBytesHandle`

加えて、handleを持たないstateless `planReading`を提供する。

### 15.1 request lifecycle

- sessionごとに単調増加request IDを払い出す
- `beginRequest`済みのcurrent requestだけが結果をpublishできる
- cancelまたは新しいrequestで古い結果をstaleにする
- session release後の新規requestを拒否する
- image/archive/bytesはsessionとは独立して明示releaseする
- releaseはidempotentで、invalid/wrong-kind/double releaseは`false`
- DirectByteBufferは対応handle release直後に無効となる

### 15.2 error

Rustは翻訳済み文字列ではなくstable codeと安全な引数だけを返す。

| code | 意味 |
|---:|---|
| 0 | none |
| 1 | invalid handle |
| 2 | invalid request |
| 3 | stale request |
| 4 | cancelled |
| 5 | I/O |
| 6 | decode/archive |
| 7 | limit |
| 8 | encode |

path、URI、username、credential、secretをerror引数へ含めない。

## 16. 状態復元とmigration

「最後の場所を記憶」がONの場合、Proto DataStoreへ次だけを保存する。

- source ID
- provider opaque directory ID
- opened entry opaque ID
- archiveかどうか
- logical page index

起動時はsource profileとSAF grant/SMB providerを先に復元し、その後breadcrumb、directory、opened file、archive logical pageを復元する。filesystem pathやpasswordは保存しない。

0.0.19初回起動では旧Android状態を完全resetする。

- 旧persisted URI grantを解放
- 旧app-private `imported` / `.importing` / `config`を削除
- 旧marker `picker.request` / `import.ready`を削除
- 外部documentは削除しない
- grant cleanup完了とその他cleanup retryを分離し、新v2 grantを次回誤解放しない

## 17. threadとperformance方針

- main threadでdisk/network/native decode/Bitmap全走査を行わない
- viewer decodeは専用dispatcherで実行する
- prefetchは別session/低優先経路とし、current pageの操作を待たせない
- transfer/thumbnailよりviewer操作を優先する
- generationとrequest IDの二重管理でlate resultを捨てる
- StrictModeのdisk/network/resource violationをCI logcatで検査する
- ANR signatureもCIで検査する

性能目標は次のとおりだが、端末性能に依存する数値はhosted emulatorだけで合格を主張しない。

- cached next/previous p95: 100ms以内
- 4K local cold display: 1秒以内
- 100ページ後のnative/RGBA memory: warm baseline比20%以内へ回復
- 固定Samba/LANのSMB cold first display: 3秒以内

## 18. buildと成果物

現行build設定は次のとおり。

- JDK 17
- Gradle Wrapper 9.1.0
- AGP 9.0.1
- Kotlin / Compose compiler plugin 2.2.10
- Compose BOM 2026.06.00
- NDK `27.3.13750724`
- Rust Android build: `cargo-ndk`
- Debug ABI: `arm64-v8a`, `x86_64`
- Release ABI: `arm64-v8a`

代表的なbuild command:

```powershell
cd android
.\gradlew.bat check assembleDebug assembleRelease bundleRelease --no-daemon
```

生成物は次のとおり。

- arm64 + x86_64 Debug APK
- arm64 unsigned Release APK
- arm64 unsigned Release AAB

製品署名、tag、push後のGitHub Release公開は自動承認しない。Release workflowは手動dispatchと`github-release` environment approvalを必要とする。

OneDrive上のlockを避けるため、local cache/build/test dataは`C:\temp\wml2viewer`または`.gitignore`済み`.test*`へ置き、終了後に削除する。

## 19. テストとCI

### 19.1 Rust

- workspace unit test / doc test
- examples check
- format / clippy
- Android arm64/x86_64 build
- Kotlin declarationとnative exportの完全一致検査
- handle、stale、cancel、double release、limit、animation、archive、reading plan test

### 19.2 Kotlin unit / Robolectric

- Proto config serialize/secret field検査
- 3×3 hit testとgesture priority
- viewport resize、zoom/pan clamp
- codec routing/capability presentation
- provider capabilityとroot operation mask
- SAF safe replacementとfailure injection
- SMB path/auth/error/redaction
- Keystore invalidation
- transfer state machine、cross-provider directory、crash recovery
- last location
- viewer dispatcherとmemory policy

### 19.3 instrumentation / Compose

- compact/expanded、599/600/700/840dp相当
- portrait/landscapeと実surface size交換
- whole-surface 3×3、letterbox、safe inset、panel入力遮断
- rotation中の`touchReady`抑止と論理page保持
- filer横2ペインとclose
- 48dp touch target、TalkBack semantics、RTL
- Activity recreation
- 二段階process-death seed/force-stop/restore
- Fake DocumentsProviderのpermission/capability
- OS codec fixture probe
- Keystore alias失効と再入力UI
- app data/log内secret sentinel検査

### 19.4 SMB integration

CIは固定Samba containerをSMB2/SMB3 matrixで起動し、次を実接続で検査する。

- username/passwordとguest
- dialect
- list/upload/copy/move/rename/delete
- collision policy
- reconnect
- 5MiB streamとSHA-256

### 19.5 実機確認履歴

Pixel 7aでdebug APKのinstall/launch、縦横回転、横向きfiler、zone tap、Activity recreationを確認した。直近の全instrumentationは29件完了、仕様上のskip 2件、失敗0であった。whole-surface tap修正後は関連layout instrumentation 13件を再実行し、失敗0を確認した。

## 20. 対象外・制約

- SMB1
- LAN上のserver自動探索
- cloud専用provider
- offline固定保存
- Android TV
- iOS/iPadOS app本体
- iOS独自SMB filer
- 外部plugin codec
- 製品署名
- Play Store / GitHub Releaseの自動公開

iOS/iPadOSは将来、SwiftUI、UIDocumentPicker/security-scoped bookmark、ImageIO、Files providerを使い、AndroidのSAF/SMBJ/Room/WorkManager/Keystore実装は共有しない。共有対象は`wml2viewer-core`と同等のRust ABI契約だけである。

## 21. 関連ファイル

- `docs/android-v2.md`: Android v2の詳細architectureとJNI ownership
- `docs/SPEC.md`: 製品全体仕様
- `docs/ios-platform-contract.md`: 将来のiOS/iPadOS接続契約
- `android/app/src/main/proto/mobile_config.proto`: mobile設定schema
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/ui/Wml2ViewerApp.kt`: responsive UI root
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/ui/components/ViewerSurface.kt`: viewer描画とgesture
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/ui/touch/TapZoneResolver.kt`: whole-surface 3×3判定
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/data/controller/AndroidMobileViewerController.kt`: production controller
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/data/source/SourceProvider.kt`: provider契約
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/platform/saf/SafSourceProvider.kt`: SAF実装
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/platform/smb/SmbSourceProvider.kt`: SMB実装
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/platform/security/KeystoreCredentialStore.kt`: 資格情報保護
- `android/app/src/main/java/io/github/mith_mmk/wml2viewer/platform/codec/AndroidCodecRouter.kt`: OS/internal codec routing
- `crates/wml2viewer-core`: platform非依存core
- `crates/wml2viewer-android`: JNI bridge
