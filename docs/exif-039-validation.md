# WML2 0.0.39 adoption / Viewer 0.0.25

The desktop and shared core now require the real published registry package
`wml2 = "=0.0.39"`. Its downloaded package identifies WML2 commit
`40c43cef96253d27fd57bf97b5529611ff4c185f`, also the peeled `v0.0.39` tag.
The lockfile contains its registry checksum; no local/git override is shipped.
Workspace and Android/iOS version metadata advance together to 0.0.25, following
the previous patch release. Plugin preference features are unchanged.

When built-in decoding fails and no plugin succeeds, file and byte loading now
return the original error object, preserving its message, type and source chain.
Successful built-in decoding avoids plugins, and a successful fallback still
returns its image. Three deterministic tests cover those contracts without
changing global plugin configuration.

## Local verification

Windows x86_64 MSVC, Rust 1.99.0:

- `cargo test --workspace --locked`: 238 passed, no ignored/failing tests.
- Existing core orientation coverage passes for all eight pixel layouts,
  encoded EXIF in both decode paths, absent/invalid orientation, animation and
  cancellation.
- `cargo test --workspace --doc --locked`,
  `cargo check --workspace --examples --locked` and `cargo build --locked` pass.
- A private input was decoded using the registry package and this branch's
  actual core/file/byte Canvas sources. All paths produced identical RGBA;
  plugin fallback functions were replaced with panicking stubs and not called.
  No private image, extracted EXIF, personal log or screenshot is included here.
- The GUI binary was started with a dedicated config/temp directory and all
  plugin providers disabled. The native window started, but hidden-window
  capture did not establish a rendered image; visual GUI correctness remains
  unverified. This is not recorded as a successful visual-display check.

Hosted OS/ARM/Android/iOS checks must complete before merging/releasing. This
record does not claim those runs passed. No installed Viewer or user settings
were replaced. Publication follows the existing tagged/manual release workflow
after review and CI; this dependency-update PR itself does not publish binaries.
