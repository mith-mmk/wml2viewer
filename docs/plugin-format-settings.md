# Per-format decoder preferences

In Settings → Plugins, choose a preferred decoder for an image format. The default,
**Inherit global settings**, preserves the existing internal-first load and plugin
fallback. Returning one row to inheritance removes its override; **Reset all
format overrides** removes every override. Apply saves the draft, and Cancel
keeps the previous settings.

Enable and configure a provider in the global plugin settings before selecting it.
A preference tries that provider's enabled, matching modules first, retaining their
existing priority order and search paths. If it is disabled, unsupported on the
current platform, missing, fails to decode, or names an unknown provider, loading
continues with the existing internal-first fallback. It does not force an
unavailable decoder or disable the fallback. System decoding currently requires
Windows; Susie64 requires Windows x64. FFmpeg requires a configured executable.
Module MIME restrictions still apply.

The optional TOML table uses canonical MIME names and provider names:

```toml
[plugins.formats]
"image/avif" = "ffmpeg"
"image/tiff" = "system"
"image/jpeg" = "internal"
```

Existing settings without this table retain their values and behavior. The table
also accepts existing extension aliases, case-insensitively (for example `jpg`,
`.JPEG`, `tif`, `j2k`). UI edits canonicalize the edited entry and remove its aliases.
Unrecognized keys are preserved until all overrides are reset. Overrides do not
advertise new extensions to the filer: global enabled plugin capabilities continue
to determine that list.

Both file loads and byte loads (including archive entries) apply the preference.
The existing wml2 signature detector identifies supported formats before using a
path extension as fallback. Formats the detector cannot identify, including AVIF
when the internal AVIF feature is off, require a path hint. Unknown formats retain
the existing decode behavior.

Validation follows `.github/workflows/ci.yml`:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo test --workspace --doc --locked
cargo check --workspace --examples --locked
```

Regression coverage includes old TOML and AppConfig round-trips, alias removal,
disabled/unknown providers, and a deterministic executable decoder double through
the viewer's file and byte entry points. The double produces a different pixel
from the internal decoder so the test verifies which decoder actually ran. Runtime
configuration mutation tests share one lock to avoid cross-module races.
