# WML2 0.0.45 dependency alignment validation

## Scope and registry verification

Desktop and shared core pin published WML2 0.0.45. The shared core directly
uses bin-rs 0.1.0 with its existing `util` feature. The lock resolves
webp-rust 0.5.0, avif-rust 0.3.0 and icc-profile 0.0.8, with every reader
consumer using the single bin-rs 0.1.0 package. No fontcore or paintcore
dependency is introduced. Default and AVIF-enabled dependency trees agree.

The downloaded WML2 package's SHA-256 matches Cargo.lock:
`6f71a259b025f68a4f372cdc987e9f5cdc14fecf2e31d429b1829fb65a965896`.
Its packaged source information records commit
`9a46e05d82792033db09507b58a9bf2c6fdead80`.
All five updated codec/reader packages resolve from crates.io. The only local
packages in the final workspace graph are its five 0.0.29 workspace crates.
No patch configuration or temporary dependency path is committed.

Viewer implementation behavior, APIs, fixtures and the sRGB RGBA8 display
pipeline are unchanged. Application/workspace versions and matching Android/iOS
version metadata are prepared for 0.0.29, following the preceding release.
This version alignment does not publish either mobile application.
The core retains WML2's multithread, jpeg-xl and color-management features.
Desktop AVIF remains opt-in.

## Final automated checks

On 2026-11-11, Windows x86_64, Rust/Cargo 1.99.0, these checks passed with the
published registry dependencies, no patch argument and a fresh build directory:

- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo test --workspace --locked`: 251 passed, 0 failed, 0 ignored.
- `cargo test --workspace --features avif --locked`: 251 passed, 0 failed,
  0 ignored.
- `cargo test --workspace --doc --locked`: passed (no documentation tests).
- `cargo check --workspace --examples --locked`: passed.
- `cargo build --locked --features avif --bin wml2viewer --example bench_decode`:
  passed (debug build).
- `cargo build --locked --release --target x86_64-pc-windows-msvc`: passed
  with the default non-AVIF release features.
- `cargo tree --workspace --locked -i bin-rs`, also with `--features avif`:
  only bin-rs 0.1.0.

Existing tests cover all eight EXIF orientations, JPEG XL without double
rotation, independent ICC still-image pixel references, GrayAlpha8 and
XYB/ICC16 animation references, timing/composition, frame/pixel/memory bounds
and cooperative cancellation. Existing unused-code/import warnings remain;
this update does not include unrelated warning cleanup.

## Local sample and package checks

An external AVIF-enabled shared-core consumer was rebuilt without a patch and
decoded eight copied inputs using the registry dependencies:

- WML2 sample AVIF, WebP, PNG and GIF: 900 x 900, one distinct frame each.
- GrayAlpha8 and XYB/ICC16 JPEG XL fixtures: 32 x 24, three distinct frames,
  50/90/130 ms durations and infinite looping.
- ICC still-image JPEG XL fixture: 7 x 5.
- Orientation-6 JPEG XL fixture: 3 x 2 after orientation handling.

For every sample, default `decode` matched `decode_with_limits` configured
with `DecodeLimits::UNLIMITED`. An already-set cancellation probe returned
`CoreErrorKind::Cancelled`. This consumer did not exercise finite limits.
Finite bounds, independent pixel comparisons and in-decode cancellation are
established separately by the repository tests, not inferred from these sample
metadata checks. Consumer sources, sample copies,
JSON results and logs remain local validation artifacts.

The Windows release packaging recipe produced the executable, LICENSE, both
README files and help.html. The canonical and legacy Windows ZIP aliases were
byte-identical. These local package checks do not replace final-commit CI builds
or establish validation of the other platforms' release archives.

## Native UI and platform limits

The preliminary candidate's AVIF-enabled debug viewer started with a separate
configuration and copied samples, with system/Susie/FFmpeg plugins disabled.
Its window title identified the requested AVIF sample. Interactive visual
inspection was not completed. The test process was stopped and normal user
configuration was not changed.

No visual correctness, animation playback, interactive navigation/cancellation,
monitor calibration, thumbnails or plugin fallback pass is claimed. The final
registry executable was built but was not subjected to another GUI attempt.
No local Android/iOS application, simulator or other-platform UI was validated.

The local toolchain is newer than CI's Rust 1.97.1. The final commit still needs
Windows, Linux, Apple Silicon and Intel macOS workspace/docs/examples checks;
Windows/Linux ARM cross-checks; and Android build/JNI, SMB integration and
emulator gates. Existing iOS CI remains absent.

## Publication convention and release gate

All workspace crates are `publish = false`; the desktop application is released
through GitHub Releases rather than crates.io. Following `wml0.0.28`, the
prepared 0.0.29 application uses tag `wml0.0.29` after the ordered dependency
releases, review and required checks.

The unchanged release workflow builds Windows x86_64, Linux x86_64, macOS
aarch64 and macOS x86_64 ZIPs, plus the legacy Windows and ubuntu-latest aliases.
A `wml*` tag push only builds artifacts. Publication requires
`workflow_dispatch` with the existing `release_tag`, `publish_release = true`,
and the `github-release` environment. Keep its default non-AVIF feature set.
Android signing/store publication and iOS publication are separate work.
