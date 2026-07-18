# wml2viewer 0.0.15 preview4

- `egui` と `wml2` を使った軽量ネイティブ画像ビューアです。

- WML21のメジャーアップデートになります（完全に別物）
- 現在、Windows 11(64bit) と Ubuntu 24.04(64bit)で動作確認してます
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

### Android 10以降（プレビュー）

Android版はStorage Access Frameworkで選択したフォルダをアプリ専用領域へ読取専用で同期し、画像・ZIP/LZH・漫画モードを閲覧できます。共有ストレージ上の元ファイルは変更しません。

必要環境はJDK 17、Android SDK 35、NDK r27c、RustのAndroidターゲット、`cargo-ndk`です。Gradle 8.9 Wrapperはリポジトリに同梱します。

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked
.\android\gradlew.bat assembleDebug
.\android\gradlew.bat installDebug
```

Linux/macOSでは`gradlew.bat`の代わりに`bash ./android/gradlew`を使います。Android Studioから実行する場合は`android`フォルダをプロジェクトとして開き、端末を選択して`app`構成を実行します。

生成物は`android/app/build/outputs/apk/debug/app-debug.apk`です。Debug版はAndroid Emulator用`x86_64`と実機用`arm64-v8a`を収録し、Release版は従来どおりarm64専用です。対象OSはAndroid 10以降で、ファイルの移動・コピー・削除・名称変更、外部プラグイン、Google Play配布は未対応です。フォルダを再選択すると、アプリ内の読取用スナップショットが置き換わります。

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
