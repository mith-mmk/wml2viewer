# Vendored wml2

This directory is based on `wml2` version `0.0.23`, published from
<https://github.com/mith-mmk/wml2-on-rust> under the MIT license.

It is vendored so wml2viewer can add allocation-safe, limit-aware decoding while
preserving the existing unlimited public API and desktop behavior. Android uses
the explicit limited API.

The `avif` feature is kept opt-in in this vendored snapshot. When enabled, it
uses the published `avif-rust` decoder through the same callback contract as
the other built-in formats; the default feature set remains unchanged.
