# JPEG XL animation fixtures

These deterministic synthetic fixtures and independent reference pixels are copied
unchanged from the MIT-licensed WML2 0.0.43 package, `tests/fixtures/review/`.
The package identifies source commit `43f56ff79b876454ef9f0064991c0178338178c1`.

Source: https://github.com/mith-mmk/wml2-on-rust/tree/43f56ff79b876454ef9f0064991c0178338178c1/wml2/tests/fixtures/review

- `jxl-animation-xyb-icc16.jxl`: three 32 by 24 XYB frames with 16-bit colour
  and alpha, an original RGB matrix/TRC ICC distinct from reconstructed sRGB.
  `jxl-animation-xyb-icc16.json` records its generator, libjxl 0.12.0 oracle,
  timing and hashes.
- `jxl-animation-grayalpha8.jxl`: three 32 by 24 SDR Gray/straight-alpha frames.
  Its entry in `jxl-animation-gray.json` records the independent libjxl oracle
  and hashes. Other cases described in that source manifest are not copied here.
- Both `.rgba16le` files contain all three independent sRGB RGBA16 canvases,
  row-major, without headers. Viewer tests explicitly round samples to RGBA8
  with `(value + 128) / 257`, allow one RGB code value for transform rounding,
  and require exact alpha.
- Both fixtures use frame durations 50, 90 and 130 ms and loop count 0.

SHA-256:

- XYB input: `bbcf461eb92390a9cdc84b9990e1114084dd807ecb8dc3d646fe369eed028414`
- XYB reference: `39502967dd3c7c251593b953e053d6af843a1768d8e3fd8646539cfb0bdfbd86`
- GrayAlpha input: `475990367003af2ff9d7be01915761f6d585aa5697f663f7dbcff795e34e8b1c`
- GrayAlpha reference: `b1c456061c168fcfc6505d3a8060621defccca9c190627fe78a4570bc9f2808a`

This coverage does not establish arbitrary ICC/LUT correctness, HDR rendering,
all JPEG XL animation features, or full colour-management equivalence on every OS.
