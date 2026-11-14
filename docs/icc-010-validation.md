# ICC 0.1.0 registry integration (2026-11-14)

Baseline: Viewer main `7d56342241036fddf6d0e9463e15f7b88a9acb9d`.
Both Viewer manifests now use the published crates.io `wml2 = "=0.0.46"`.
The lockfile selects WML2 0.0.46 with checksum
`15f7a106337fa0cc32a6de0c4587fbbbf0073d12c36386e62a7100a607297e17`
and published `icc-profile` 0.1.0 with checksum
`b7f43fd73989d11494bf2c99530bbd0dad17b48d3de9604ae6819848324998e8`.
This replaces the initial Git candidate pin. The published version also
includes WML2's reviewed correction to error handling for later CMYK TIFF
pages. No machine-local path or Git dependency
is required for WML2.

The existing core `color-management` feature is retained, not newly enabled.
No Viewer UI, application version, or colour-policy settings change. Embedded
CMYK profiles can now be applied by the backend where older ICC versions
fell back. The test explicitly rejects silent fallback, verifies analytic
sRGB output for baseline/progressive CMYK JPEGs through both Viewer decode
paths, and verifies unchanged untagged pixels. Existing JPEG XL ICC tests
remain covered. See `crates/fixtures/cmyk-icc-fixture.md` for provenance.

## Verification

On macOS with rustc 1.97.0, using an absolute `CARGO_TARGET_DIR`:

- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.
- `cargo test --workspace --locked`: 249 tests passed (179 desktop library,
  4 desktop binary, 41 core, 8 iOS host tests, 17 mobile bridge).
- `cargo test --workspace --doc --locked`: passed; no documentation tests.
- `cargo check --workspace --examples --locked`: passed.
- `cargo tree --locked -i icc-profile`: confirms one ICC 0.1.0 through the
  published WML2 0.0.46 for desktop and core.

An initial run with a relative `.test-target` path failed nine existing
filesystem navigation tests because their helpers duplicated that relative
path. Rerunning with the same target directory specified as an absolute path
passed all tests. No unrelated filesystem code was changed. Existing compiler
warnings remain. These checks are repeated against the registry dependency.
GUI visual testing, physical-device runs and optional AVIF checks were not
performed locally. This integration is not a Viewer product release and has
not been merged into main.

The initial Git-pinned revision passed all 12 hosted CI jobs in
[run 43532668316](https://github.com/mith-mmk/wml2viewer/actions/runs/43532668316),
including desktop tests on Linux, Windows, macOS ARM/Intel, ARM cross-checks,
Android builds, three emulator instrumentation configurations, and SMB2/SMB3
integration. Registry-revision CI results are recorded on PR #37; the older
run alone is not evidence for the registry revision.
