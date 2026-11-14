# ICC 0.1.0 candidate integration (2026-11-14)

Baseline: Viewer main `7d56342241036fddf6d0e9463e15f7b88a9acb9d`.
Both Viewer manifests pin WML2 candidate 0.0.46 to Git commit
`3c256281dc5a4557fed49e209696be13922ce81d` because that version is not yet
published on crates.io. No machine-local path or floating branch is required.
The lockfile selects the published crates.io `icc-profile` 0.1.0 with checksum
`b7f43fd73989d11494bf2c99530bbd0dad17b48d3de9604ae6819848324998e8`.
After an authorized WML2 publication, replace both Git pins with `=0.0.46`,
refresh the lockfile, and repeat these checks before merging.

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
  pinned WML2 candidate for desktop and core.

An initial run with a relative `.test-target` path failed nine existing
filesystem navigation tests because their helpers duplicated that relative
path. Rerunning with the same target directory specified as an absolute path
passed all tests. No unrelated filesystem code was changed. Existing compiler
warnings remain. GUI visual testing, device builds, optional AVIF checks,
Linux/Windows checks and hosted CI were not run here. This candidate is not a
Viewer product release and has not been merged into main.
