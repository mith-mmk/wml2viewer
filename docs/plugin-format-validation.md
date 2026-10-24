# Per-format plugin validation

## 2026-10-23, NERO / Windows

Resumed existing commit a9bb446, integrating main at
23f25ddb55c629f5508ec89c6603548efcaf8b43 (Viewer 0.0.25 / registry WML2
0.0.39). The loader merge preserves the original decoder error when both the
preference and normal fallback fail. No dependency downgrade or release change.

Local x86_64-pc-windows-msvc validation with Rust 1.99.0:

- cargo fmt --all --check
- cargo test --workspace --locked --offline: 241 passed, zero failures/ignored
- cargo test --workspace --doc --locked --offline
- cargo check --workspace --examples --locked --offline
- cargo build --release --locked --offline
- git diff --check

Added disk-backed configuration reload coverage and a cross-platform loader test
for internal, disabled, and unknown preferences, original error preservation,
signature detection with a misleading extension, and clearing overrides. On
Windows the test also verifies an actual preferred WIC decode and MIME-filtered
fallback. The executable FFmpeg double remains Unix-only; it was not executed
on NERO. Optional external FFmpeg/Susie64 fixtures are not present locally, so
the existing conditional tests do not establish those backends' availability.

Actual Windows GUI inspection used a dedicated process, synthetic JPEG, and
isolated configuration, leaving installed settings and user applications alone.
Verified the Plugins tab, PNG internal selection, Apply and saved TOML, restoration
after closing/restarting Viewer, inheritance removing the saved override, Cancel
leaving the saved configuration unchanged, and disabled provider choices. Test
input timing was adjusted after initial automated clicks were missed; final
assertions were made against the resulting saved configuration. No private input,
local logs, or screen captures are included in this branch.

No merge, release, or CI wait is part of this work.

## 2026-10-22, Hadrianus / macOS

Base: origin/main at d8f58ae. Branch: codex/per-format-plugin-settings.
The original checkout remains on codex/ios-reimplementation. Its untracked
.DS_Store and tests/.DS_Store were preserved. No repository AGENTS.md or .agents
instructions were present. CI commands are taken from .github/workflows/ci.yml.

Passed on the final code:

- cargo fmt --all --check
- cargo test --workspace --locked: 234 tests passed, no failures
- cargo test --workspace --doc --locked
- cargo check --workspace --examples --locked
- cargo build --locked
- git diff --check

The executable decoder test verifies a different output pixel through both viewer
entry points, signature detection despite a misleading extension, no-hint byte
loads, missing executables, and resetting to the old internal-first behavior.
Other tests cover persistence through ConfigFile/AppConfig, old TOML defaults,
unknown and disabled providers, and extension aliases. Existing runtime mutation
tests now share a lock; previously separate module locks allowed them to race.

Native UI verification used an isolated /tmp/Wml2FormatQA.app launcher and
/tmp/wml2viewer-format-qa.toml, without changing the normal user configuration.
Verified the Plugins tab visually, its preference rows, unavailable choices being
disabled, PNG -> internal -> Apply, the saved image/png TOML entry, reopening
settings, Reset all format overrides -> Apply, and removal of the formats table.
This UI pass preceded adding the remaining built-in legacy format rows; the final
code includes those rows and passed the checks above.

An additional launch with a PNG fixture reached a running QA application, but
subsequent native screen retrieval timed out. Actual image display in that final
manual pass is therefore unverified. Windows system/Susie64 backends were not
executed on this Mac. No release, merge, or pull request was performed.
