# JPEG XL colour-management test fixture

`jxl-icc-lossless.jxl` and `jxl-icc-lossless.srgb.rgba` are from the MIT-licensed
WML2 0.0.40 package, `tests/fixtures/review/`. WML2 records the reference as
libjxl 0.11.2 RGBA16 decoded using its reconstruction ICC profile, converted
to sRGB with LittleCMS 2.16, then rounded to RGBA8. Dimensions are 7 by 5.

- JPEG XL SHA-256: `5ca8a8e39a4640ce49188cd9896163f5cbd88d870503efee5f9b941843ddede8`
- Reference SHA-256: `c66cc73e3ebc7ab1dc427fabad0e165f3ac32f4d7d7c33f77cfd6d1600c375fd`
