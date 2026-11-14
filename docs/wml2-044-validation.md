# WML2 0.0.44 dependency alignment validation

This is the historical 0.0.44 validation record. See the
[0.0.45 follow-up](wml2-045-validation.md) for the current dependency alignment.

## Scope

Desktop and shared core pin published WML2 0.0.44. The shared core's direct
bin-rs dependency is 0.0.11, matching the `BytesReader` passed to WML2's
`image_decoder_with_limits` and the reader types used by TIFF tests.
Cargo.lock updates only WML2 and the required reader/WebP/AVIF dependencies:
webp-rust 0.4.0, avif-rust 0.2.0 and bin-rs 0.0.11.

The remaining bin-rs 0.0.10 belongs to icc-profile 0.0.7 internally. It does
not cross the Viewer/WML2 reader boundary and is intentionally retained.
There is no reader compatibility shim, path override or new codec fork.
Viewer implementation source, fixtures, application version and the existing
sRGB RGBA8 display pipeline are unchanged by this alignment step.

## Automated checks

On 2026-11-11, hadrianus (Apple Silicon macOS), Rust/Cargo 1.97.0:

- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.
- `cargo test --workspace --locked`: 248 passed, 0 failed, 0 ignored.
- `cargo test --workspace --features avif --locked`: 248 passed, 0 failed,
  0 ignored.
- `cargo check --workspace --examples --locked`: passed.
- `cargo build --release --locked --features avif --bin wml2viewer --example bench_decode`:
  passed.
- `cargo build --release --locked --package wml2viewer-core --features avif`:
  passed for the local sample consumer.
- `cargo tree --workspace --locked -i bin-rs@0.0.11`, also with `--features avif`,
  confirms the Viewer/WML2/WebP/AVIF reader alignment.

The existing JPEG XL pixel-reference, animation metadata, bounds and
cooperative-cancellation tests pass with the new dependency graph. Existing
float-literal fallback and unused-code/import warnings remain.

## Local sample and GUI checks

A release-linked shared-core consumer decoded nine local sample copies:

- AVIF and static WebP: 900 x 900.
- Animated WebP: 1200 x 1200, seven distinct frames, 330 ms each, infinite loop.
- GrayAlpha8 and XYB/ICC16 JPEG XL: 32 x 24, three distinct frames,
  50/90/130 ms, infinite loop.
- `animation_icos4d.jxl`: 128 x 128, 48 distinct frames, 50 ms each.
- `animation_spline.jxl`: 320 x 320, 60 distinct frames, 20 ms each.
- Existing GIF and PNG: 900 x 900, one decoded frame each.

The AVIF-enabled release executable was then run with a separate configuration
and copied samples. System, Susie and FFmpeg plugins were disabled. All nine
images displayed with matching window titles. Separate screen observations
showed frame changes in animated WebP, icos4d and spline. Forward navigation
through the sample set and backward navigation from PNG through GIF to spline
worked. The session recorded successful load commits for all nine files, no
error/failed/panic-named events, and an empty stderr log. The test app was closed.

The original checkout, normal user configuration and source samples were not
modified. Local test copies, logs and the app wrapper are not repository files.
The [0.0.43 record](wml2-043-validation.md) documents the earlier sample origins
and JPEG XL reference coverage; those fixtures were not changed for this update.

## Verification limits

GUI checks are smoke tests, not frame-rate measurement or monitor calibration.
Interactive cancellation during a long decode, thumbnails and plugin fallback
were not tested. No new iOS app or simulator validation was performed; the
iOS-specific CI job remains removed. Other-platform GUI checks and release
packaging were not run locally. The PR's checks must be read for the final
commit rather than treating the previous 0.0.43 CI result as current.
