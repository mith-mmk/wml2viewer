# CMYK JPEG colour-management regression fixtures

The baseline and progressive 16x16 JPEGs and their Python-stdlib generator are
copied from the MIT-licensed WML2 repository at commit
`3c256281dc5a4557fed49e209696be13922ce81d`, `wml2/tests/fixtures/review/`.
They are original synthetic DC-only JPEG blocks, not third-party photographs.
Run `python3 crates/fixtures/generate_jpeg_cmyk_blocks.py` to reproduce them.

- `jpeg-cmyk-blocks.jpg`: SHA-256 `e058e155eee7157c3f98c6f86be1ad45f99062c7188912711170f054b12250ee`
- `jpeg-cmyk-blocks-progressive.jpg`: SHA-256 `c99a44968083c8814b3118b61c5a0aa40079e131f436abccfd182042c252258d`

`cmyk_test_profile.rs` copies only the original test-profile generator from
WML2's `wml2/tests/common/cmyk_profile.rs` at the same commit. It builds an
ICC v2 CMYK-to-XYZ LUT with a D50-adapted sRGB matrix and linear subtractive
CMYK products. The Viewer test embeds this profile in APP2 in memory and
compares decoded pixels with an independent analytic sRGB formula, allowing
three code values for matrix/LUT quantization. It also checks that untagged
pixels retain their exact legacy values, including opaque alpha.
Both baseline/progressive input and bounded/unbounded Viewer decoding are
covered. A backend that silently falls back rather than applying ICC fails.
