# Per-format plugin validation (2026-10-22, Hadrianus / macOS)

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
