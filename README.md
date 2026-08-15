# wml2viewer 0.0.20

A lightweight native image viewer built with `egui` and `wml2`.

- This is a major update of WML21 (essentially a completely new implementation)
- Currently tested on Windows 11 (64-bit), Ubuntu 24.04, and Android 10+ (Pixel 6a x86_64 emulator)
- This is a preview version, and specifications may change at a later date.

## Main Features

- Native support for jpeg/webp/bmp/tiff/png/gif/mag/maki/pi/pic
- Native support for Animation GIF/PNG/Webp
- Direct browsing of zip files
- Plugin support: susie64 plugin(windows) / OS decoders(windows) / ffmpeg
- Browsing via listed files (.wmltxt)
- Manga mode
- English/Japanese support (font required)
- Android uses the system CJK font for Japanese, Chinese, and Korean names
- Smooth image browsing with multi-worker architecture
- OS integration features (Windows)

## Launch

wml2viewer

Command Line

```bash
wml2viewer Normal launch
wml2viewer [path] Launch with specified image
wml2viewer --config <path> [path] Launch with custom config file
wml2viewer --clean system Reset configuration
```

### Android 10+

Android 0.0.20 is a mobile-first Jetpack Compose application. It accesses selected folders directly through the Storage Access Framework, supports SMB2/3 sources, uses a dedicated 3×3 touch map and phone/tablet layouts, and keeps the desktop UI and `config.toml` unchanged. Passwords are encrypted with an Android Keystore key and are never passed to Rust.

Build prerequisites are JDK 17, Android SDK 36, NDK r27c (`27.2.12479018`), the Rust Android targets, and `cargo-ndk`. The repository includes the Gradle 9.1.0 Wrapper and uses AGP 9.0.1.

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
Push-Location android
.\gradlew.bat assembleDebug
.\gradlew.bat installDebug
Pop-Location
```

On Linux and macOS, run `cd android && bash ./gradlew assembleDebug` from the repository root. To run from Android Studio, open the `android` directory as a project, select a device, and run the `app` configuration.

The debug APK is written to `android/app/build/outputs/apk/debug/app-debug.apk` and includes `x86_64` plus `arm64-v8a`. Unsigned arm64 release outputs are `android/app/build/outputs/apk/release/app-release-unsigned.apk` and `android/app/build/outputs/bundle/release/app-release.aab`. Production signing and publication remain separate approval-gated work.

The Android app uses `MobileConfigV1` Proto DataStore rather than desktop `config.toml`. Long transfers are journaled in Room and run through foreground WorkManager. Files selected from SAF or SMB are materialized on demand into a temporary bounded LRU only when the Rust codec/archive core needs a local seekable item. See [Android architecture](docs/android-v2.md) and the [future iOS/iPadOS contract](docs/ios-platform-contract.md).

The Rust JNI bridge returns explicitly owned image, archive, and encoded-byte handles. Animated GIF/APNG/WebP decoding exposes composited frame count, loop count, duration, and independently releasable frame buffers so Compose can schedule playback without retaining a Rust borrow. The same byte-handle contract returns bounded PNG/JPEG/WebP output when Android routes RGBA encoding through the internal codec. Android spread, navigation-anchor, and prefetch planning is also delegated to `wml2viewer-core` through a stateless, versioned JNI result parsed by the typed Kotlin `NativeReadingPlanner`.

## Help

https://mith-mmk.github.io/wml2/help.html

## Configuration

Configuration is stored in OS-specific directories:

- Windows: %USERAPP%\mith-mmk\wml2\config\config.toml
- Linux: ~/.wml2/config/config.toml

###Example workaround for large / network ZIP:

```toml
[runtime.workaround.archive.zip]
threshold_mb = 256
local_cache = false

[filesystem.thumbnail]
suppress_large_files = true

[resources]
font_paths = ["C:/Windows/Fonts/NotoSansJP-Regular.otf"]
```

## Notes

- Low-I/O workaround is enabled for large or network-based ZIP files.
- Windows: file association can be managed via `Settings -> System`
- `ffmpeg` decoding is currently done via external `ffmpeg.exe`
- `susie64` (Windows only) supports only image plugin decoding
- system plugin:
  - Windows: WIC decode implemented
  - macOS: planned
- Enabling providers allows formats like `avif` and `jp2` to be handled

# update log

- 2026-04-17 0.0.14 preview3 released
- 2026-04-25 0.0.15 preview4 released, right click menu added, key bidings added, and some bugs fixed.
- 2026-05-17 0.0.16 preview4 released, adjusted the UI.
- 2026-05-31 0.0.17 beta1 released, add LZH support, images draw effects
- 2026-07-18 0.0.18 released, macOS build and Android build added
- 2026-08-11 0.0.19 prepared, Android rebuilt with Compose, direct SAF/SMB providers, secure credentials, mobile UI/config, and OS codec routing
- 2026-08-16 0.0.20 released, Android and iOS mobile viewers, CI action updates, and generated release notes
