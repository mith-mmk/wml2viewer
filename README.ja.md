# wml2viewer 0.0.19

- `egui` と `wml2` を使った軽量ネイティブ画像ビューアです。

- WML21のメジャーアップデートになります（完全に別物）
- 現在、Windows 11(64bit)、Ubuntu 24.04(64bit)、Android 10以降（Pixel 6a x86_64エミュレータ）で動作確認しています
- プレビュー版なので仕様は今後変わります

## 主な機能

- JPEG/Webp/BMP/Tiff/PNG/GIF/mag/maki/pi/picのネイティブ対応
- アニメーションGIF/PNG/webpのネイティブ対応
- マルチプラットフォーム対応
- zipファイルの直接閲覧
- プラグイン機能 susie64 plugin(Windows)/os デコーダ(Windows)/ffmpegに対応
- リステッドファイル(.wmltxt)によるブラウジング
- マンガモード
- 英語/日本語両対応(要フォント)
- Android版は日本語・中国語・韓国語の名前表示にシステムCJKフォントを使用
- マルチワーカーによる快適な画像ブラウジング
- OS連携機能(Windows)

## 起動

- 適当な実行用フォルダに投げ込んでから実行してください

```powershell
wml2viewer
```

## コマンドライン

- `wml2viewer` 通常起動
- `wml2viewer [path]` 画像を指定して起動
- `wml2viewer --config <path> [path]` 設定ファイルを指定して起動
- `wml2viewer --clean system`　設定を削除

### Android 10以降

Android 0.0.19はJetpack Composeで全面再実装したモバイル専用アプリです。Storage Access Frameworkで選択ツリーを直接参照し、SMB2/3、3×3位置タッチ、スマホ/タブレット別UIに対応します。デスクトップUIと`config.toml`は変更しません。SMBパスワードはAndroid Keystore鍵で暗号化し、Rustへ渡しません。

必要環境はJDK 17、Android SDK 36、NDK r27c（`27.2.12479018`）、RustのAndroidターゲット、`cargo-ndk`です。Gradle 9.1.0 WrapperとAGP 9.0.1を使用します。

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
Push-Location android
.\gradlew.bat assembleDebug
.\gradlew.bat installDebug
Pop-Location
```

Linux/macOSではrepository rootから`cd android && bash ./gradlew assembleDebug`を実行します。Android Studioから実行する場合は`android`フォルダをプロジェクトとして開き、端末を選択して`app`構成を実行します。

Debug APKは`android/app/build/outputs/apk/debug/app-debug.apk`に生成され、`x86_64`と`arm64-v8a`を収録します。未署名arm64 Releaseは`android/app/build/outputs/apk/release/app-release-unsigned.apk`と`android/app/build/outputs/bundle/release/app-release.aab`です。製品署名と公開は別途承認が必要です。

Android設定はデスクトップ`config.toml`ではなくProto DataStoreの`MobileConfigV1`へ保存します。長時間転送はRoomへjournal化し、Foreground WorkManagerで継続します。SAF/SMBの項目はRust codecや書庫coreがseek可能なローカル項目を必要とするときだけ、一時LRUへオンデマンド展開します。詳細は[Android設計](docs/android-v2.md)と[将来のiOS/iPadOS契約](docs/ios-platform-contract.md)を参照してください。

Rust JNI bridgeは画像・書庫・encoded bytesを明示的に所有するhandleとして返します。アニメーションGIF/APNG/WebPは合成済みframe数、loop回数、表示時間、独立してrelease可能なframe bufferを公開し、Compose側がRustのborrowを保持せずに再生できます。Androidが内蔵codecへRGBA encodeをroutingした場合も、size limit付きPNG/JPEG/WebP出力を同じbytes handleで返します。Androidの見開き・前後anchor・先読み計画もstatelessなversioned JNI結果をtyped Kotlin `NativeReadingPlanner`が解釈し、計算の所有元を`wml2viewer-core`へ統一します。

## ヘルプ

- https://mith-mmk.github.io/wml2/help.html

## 設定

設定は、[適用]ボタンを押すまで適用されません。また、OSごとの設定ディレクトリに保存されます。

- Windows: %USERAPP%\mith-mmk\wml2\config\config.toml
- Linux: ~/.wml2/config/config.toml

### 大容量 / ネットワーク ZIP 向けワークアラウンド例:

```toml
[runtime.workaround.archive.zip]
threshold_mb = 256
local_cache = false

[filesystem.thumbnail]
suppress_large_files = true

[resources]
font_paths = ["C:/Windows/Fonts/NotoSansJP-Regular.otf"]
```

## メモ

- 大きい ZIP やネットワーク上の ZIP では low-I/O ワークアラウンドが有効になります。
- Windows では `設定 -> システム` から拡張子の関連付けを操作できます。
- `ffmpeg` は現状 `ffmpeg.exe` を起動してデコード。
- `susie64` は Windows 専用で、image pluginのみでサポート。
- `system` は Windows では WIC decode までサポート。macOS system codec は今後の拡張対象です。
- plugin を有効化すると、`avif` や `jp2` などの拡張子も filer / viewer の対象になります。

# update log

- 2026-04-17: 0.0.14 preview3 公開
- 2026-04-25: 0.0.15 preview4 公開、右クリックメニュー追加、キー割り当て追加、いくつかのバグ修正
- 2026-05-17: 0.0.16 preview5 UIの調整
- 2026-05-31: 0.0.17 beta1 LZHサポート、画面エフェクトの追加
- 2026-07-18: 0.0.18 公開、macOSビルドとAndroidビルドを追加
- 2026-08-11: 0.0.19準備、ComposeによるAndroid全面再実装、SAF/SMB、資格情報保護、モバイルUI/設定、OS codec routingを追加
