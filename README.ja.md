# wml2viewer 0.0.12 preview2

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

## 0.0.12 の既知のIssue
- 時間のかかる ZIP展開の問題
- より洗練されたUIとアイコン
- `LHA` 対応とキーバインド UI は `0.0.13` へスライドしました。
