# WML2 0.0.43 adoption validation

## Scope

Desktop and shared core now pin the published `wml2 =0.0.43` crate. The existing
sRGB RGBA8 display, animation composition, orientation and limits remain in use.
The core forwards its cooperative cancellation probe to the JPEG XL decoder and
maps the decoder's typed cancellation to `CoreErrorKind::Cancelled`.

This does not add paintcore/fontcore dependencies, high-precision editing, HDR,
arbitrary Gray ICC support, new rendering architecture, or fixes for viewer
issue #32. The application version has not been bumped for a new release.

## Automated checks

On 2026-11-10, Apple Silicon macOS with Rust/Cargo 1.97.0:

- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.
- `cargo test --workspace --locked`: 248 tests passed, no failed or ignored tests.
- `cargo test --workspace --features avif --locked`: 248 tests passed, no failed
  or ignored tests. This verifies the feature configuration, not native AVIF
  sample display or a cross-platform build.
- `cargo check --workspace --examples --locked`: passed.
- `cargo build --release --locked --bin wml2viewer --example bench_decode`:
  passed. Existing float-literal fallback and unused-code/import warnings remain.

The focused JPEG XL checks include the existing independent still-image ICC
reference and new animation references. All three frames of each synthetic
XYB/ICC16 and GrayAlpha8 input are compared with libjxl-generated sRGB pixels,
with exact alpha and at most one RGBA8 RGB code-value difference. Timing, loop
count, poster selection, frame/pixel/memory bounds and cancellation are checked.
The truncated-input test distinguishes a probe reaching WML2 from cancellation
only after decoding. See [fixture provenance](../crates/fixtures/jxl-animation-fixtures.md).

## Local sample decode checks

A release-linked, headless consumer exercised the shared core on local copies
of the samples. This is decode evidence, not GUI playback evidence.

- `animation_icos4d.jxl`: 128 x 128, 48 distinct frames, 50 ms per frame,
  2400 ms cycle, loop count 0.
- `animation_spline.jxl`: 320 x 320, 60 distinct frames, 20 ms per frame,
  1200 ms cycle, loop count 0.
- Existing GIF and PNG: decoded successfully.
- Synthetic GrayAlpha8 and XYB/ICC16: 32 x 24, three distinct frames,
  durations 50/90/130 ms, loop count 0.

The two conformance bitstreams originate from
[libjxl/conformance at 4bf0535](https://github.com/libjxl/conformance/tree/4bf053529c7cefd2951be453475bb3dccc7e7be8/testcases)
and were used locally without adding them to this repository. They carry the
upstream BSD-3-Clause bitstream license, not the source images' CC0 label.

## macOS GUI smoke test

After the test computer was unlocked, the release executable was launched on
hadrianus using a separate local app wrapper, copied samples and explicit test
configuration. System, Susie and FFmpeg plugins were disabled, so these checks
used the internal decoder path. Normal user settings and the pre-existing
checkout were not modified.

- `animation_icos4d.jxl` and `animation_spline.jxl`: the expected geometry was
  displayed, and separate screen captures showed different animation frames.
- Synthetic GrayAlpha8 and XYB/ICC16: grayscale/alpha and color images were
  displayed, with visible frame changes. The GrayAlpha sample was also viewed
  at 200% zoom using the context menu.
- Existing GIF and PNG: both displayed successfully. These local samples each
  contain one decoded frame, so this is not evidence for GIF/APNG animation.
- Forward and backward navigation through all six files updated both the
  displayed image and the window title. Returning to the spline animation
  resumed animated display.
- The session log contains successful load commits for all six files, no
  error/failed/panic-named events, and an empty stderr log. The test app was
  closed after the checks.

This is a display and navigation smoke test, not frame-rate measurement or
monitor color calibration. Pixel fidelity and source timing are covered by
the independent reference and headless checks above.

## Remaining verification

Interactive interruption during a long decode, thumbnails and plugin fallback
remain unverified. Cooperative cancellation and limits have automated coverage.
Windows/Linux runtime, Android/iOS app and simulator checks, remote CI and
release packaging have not been run for this change. No release, merge or
public PR is implied by these local results.
