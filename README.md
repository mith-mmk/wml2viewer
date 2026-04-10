# wml2viewer 0.0.12 preview

A lightweight native image viewer built with `egui` and `wml2`.

- This is a major update of WML21 (essentially a completely new implementation)
- Currently tested on Windows 11 (64-bit) and Ubuntu 24.04
- This is a preview version, and specifications may change at a later date.

## Main Features
- Native support for jpeg/webp/bmp/tiff/png/gif/mag/maki/pi/pic
- Native support for Animation GIF/PNG/Webp 
- Direct browsing of zip files
- Plugin support: susie64 plugin(windows) / OS decoders(windows) / ffmpeg
- Browsing via listed files (.wmltxt)
- Manga mode
- English/Japanese support (font required)
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

### Known Issues (0.0.12)
- waiting indicator is still too minimal; could be replaced with clearer messages like now loading
- LHA support and keybinding UI are postponed to 0.0.13
- Extension check issue (fallback to WML0.0.19)