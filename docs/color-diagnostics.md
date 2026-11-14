# Decode color diagnostics

WML2 0.0.47 diagnostics are retained on `DecodedImage` and `LoadedImage`, through cloning, resizing, worker delivery and preloading. Image Information uses those exact decode metadata and warnings, including EXIF, rather than decoding the current path again. This also works for archive bytes and protects cached results from a file changing on disk.

The three facts are separate: source ICC metadata, decoded output color coordinates, and completed color processing. Only the typed `ColorOutcome::from_metadata` protocol establishes the last two. Missing, malformed, future, unavailable plugin records and unmapped outputs remain unknown. Warning text and profile presence never establish conversion. Individual warnings retain decoder order; none reported is not proof of validation.

External WIC and Susie images have unknown diagnostics. ffmpeg's temporary BMP diagnostics are discarded because they cannot describe the original image or ffmpeg's processing. A builtin decode reached through plugin fallback retains its own diagnostics.

The panel uses WML2's zero-based decoded output index, not a filename or profile-based guess. TIFF/PSD/JXL animation records include a base/poster at output zero; the existing Viewer compositor uses the emitted animation frames as its poster/playback and maps these to output one onward. This change deliberately does not alter that pixel behavior. Unknown animation layouts have unknown output mapping, without borrowing output zero. TIFF source profiles use a page-specific record or the decoder's explicitly common source record.

ICC Profile is shown under its exact decoder label: it is not necessarily the original source profile (managed JXL supplies an sRGB output profile). Profiles are reported as retained bytes, not validated by the panel. Color outcomes describe decoder processing before Viewer effects/compositing and physical-monitor display, not monitor calibration.

The explicit JXL sRGB helper is retained so this metadata change does not switch the pixel path. Its 0.0.47 API returns no warning collection; the current implementation emits no recoverable warnings on successful decode. Generic bounded and unbounded decode paths retain the warning collection.
