# Android v2 architecture (0.0.19)

Android 0.0.19 is a clean cutover from the former `NativeActivity + eframe` MVP. The Android app is a single `ComponentActivity` whose UI is implemented with Jetpack Compose. Rust does not own an Android window, Android storage identifiers, credentials, or localized UI text.

## Ownership boundary

| Layer | Owns | Must not own |
|---|---|---|
| `wml2viewer-core` | internal image decode/encode, archive pages, logical navigation, spread composition, prefetch decisions | Android APIs, eframe, URI grants, credentials, translated UI text |
| `wml2viewer-android` | JNI handle registry, cancellation/request ordering, archive access, owned RGBA/encoded-byte buffers | SAF/SMB identifiers, passwords, UI state |
| Compose UI | viewer/filer/settings screens, phone/tablet adaptation, gesture arbitration, localized text | direct file/network I/O |
| Android data/platform | SAF, SMB2/3, transfer journal, cache, Keystore, OS codecs | desktop `config.toml`, Rust UI |

Every external file is identified by `EntryRef(sourceId, opaqueEntryId)`. An opaque ID is interpreted only by its provider. Rust receives only an app-private materialized path and MIME type, never a SAF URI, SMB URI, username, domain, password, or credential ID.

## UI and input

- Compact width (`<600dp`) uses a full-screen filer and a bottom filmstrip sheet.
- Widths of `600dp` or more use a fixed two-pane layout. A landscape tablet can pin the filmstrip.
- Rotation, folding, and multi-window resize re-evaluate width while page identity and selection remain in `ViewerViewModel` state.
- Touch hit testing is relative to the displayed image rectangle, split into nine equal cells. The default map is previous/filer/next, previous/settings/next, previous/sub-filer/next.
- All cells accept only safe viewer actions or disabled. Destructive file actions cannot be assigned.
- Child UI, pan, pinch, long press, and double tap consume input before a pending zone tap. Zone taps are disabled over panels.
- Swipe is disabled by default; pinch zoom is enabled; double tap toggles fit; long press opens the quick menu.

Landscape manga navigation uses `auto`, `single`, or `spread`. The default shows portrait as one page and, in landscape, keeps the cover alone before pairing portrait pages. Landscape pages, unmatched pages, and source boundaries remain single. Navigation moves by the composed spread, and prefetch is limited to one following spread. These decisions are owned by `wml2viewer-core`: Kotlin sends only source-boundary IDs, portrait/cover flags, selection, viewport orientation, and typed options through the stateless `NativeReadingPlanner`; it must not independently recreate spread arithmetic.

## JNI ABI and ownership

The loaded library is `wml2viewer_android`, and every export belongs to `io.github.mith_mmk.wml2viewer.nativebridge.NativeBridge`. The ABI has four independently owned resource types plus one stateless reading call. `planReading` accepts at most 4,096 pages and 64 forward-prefetch spreads. It returns wire v1 as `version, total length, anchor, previous anchor, next anchor, logical count, visual count, preload count`, followed by the three index lists. Kotlin validates this wire and exposes only typed layout, direction, page, and plan models; ABI integers do not cross into UI code.

- Session handles allocate monotonically increasing request IDs. `beginRequest` accepts an allocated ID once, and decode/materialize operations publish results only while that ID is current. Cancellation and superseding requests make late work stale.
- Image handles expose RGBA width, height, stride, and a direct buffer. Animated images additionally expose composited frame count, loop count (`-1` unspecified, `0` infinite), per-frame duration, and an independently owned image handle for each requested frame. The parent buffer is the poster/first composited frame. Before publication, Rust rejects a poster above 4,096×4,096 pixels, more than 4,096 animation frames, or more than 128 MiB of checked aggregate poster/frame RGBA ownership. Kotlin playback keeps the parent handle and copies one requested frame at a time through a short-lived child handle, so general GIF/WebP playback does not retain every frame on the Kotlin heap. The Kotlin bitmap cache is byte-weighted and capped at 128 MiB; RGBA conversion is tiled instead of allocating a full-image `IntArray`.
- Android invokes the vendored `wml2` 0.0.23 decoder through an explicit limit-aware API. Canvas/frame dimensions, declared animation budgets, bounded zlib/LZW output, and cancellation are checked before the corresponding allocation or composition work; cancellation is also probed during long composition loops. The original unlimited API remains the desktop default for compatibility. Upstream provenance and the MIT license are recorded in `crates/wml2/UPSTREAM.md` and `crates/wml2/LICENSE`.
- Archive handles expose entry count, normalized relative entry name, declared size, internal decode, and encoded entry materialization. A direct encoded image, archive container, and one materialized ZIP/LHA/listed entry are each capped at 64 MiB; archive container plus materialized entry have a 128 MiB combined retained budget. Entry-count and total-uncompressed limits also apply. A local `.wmltxt` may resolve only a canonicalized sibling below its list directory; provider-backed listed entries are resolved by Android using the returned relative entry name.
- Encoded-byte handles expose a bounded direct buffer for Android OS codec/cache routing. Archive materialization and internal `encodeRgba` output use the same ownership contract. Encoding accepts only a direct RGBA buffer with valid width/height/stride and supports PNG, JPEG, and WebP; it rejects more than 100 million pixels, a stride above 16 MiB, or input/output ownership above 512 MiB. The allocation remains owned by Rust until `releaseBytes` succeeds.

All handle IDs are process-global and never reused across handle types. Release functions are idempotent and return `false` for invalid, wrong-kind, or already released handles. A direct buffer becomes invalid immediately after its corresponding image or byte handle is released. Releasing a session rejects later request work but does not implicitly free already returned image, archive, or byte handles; their Kotlin `Closeable` wrappers release them explicitly.

Local files are read through one bounded path: metadata is checked before allocation, capacity uses fallible reservation, and the file is consumed through a `limit + 1` reader so concurrent growth is rejected. A limit failure uses stable error code `7` and never invokes the image decoder or archive parser for an already oversized source.

Request failure uses stable nonlocalized codes and typed JSON arguments: `0 none`, `1 invalid_handle`, `2 invalid_request`, `3 stale_request`, `4 cancelled`, `5 io`, `6 decode` (including archive), `7 limit`, and `8 encode`. Paths, URIs, usernames, credentials, and secrets are never included.

## Mobile configuration

`MobileConfigV1` is stored by Proto DataStore in `mobile_config_v1.pb`. It contains only Display, Manga, Touch regions, Filer and SMB profile metadata, Codec, Language and appearance, Cache, and information-facing settings. It has no window geometry, pane side, keyboard/mouse mapping, file association, or external plugin fields.

When “remember last location” is enabled, the same DataStore persists only a source ID, provider-opaque directory/page IDs, logical archive page index, and archive flag. Source profiles and URI grants are restored first after process recreation, then the controller reconstructs the breadcrumb and logical page without persisting a filesystem path, URI, or credential.

An SMB profile stores only its credential ID. The corresponding ciphertext is held below `noBackupFilesDir` and encrypted with an Android Keystore AES-256-GCM key using a random nonce and the profile ID as AAD.

## Providers and transfers

`SourceProvider` advertises `SourceCapabilities` and implements list/stat/openRead/create/copy/move/rename/trashOrDelete/thumbnail. UI actions must be derived from capabilities rather than provider type checks.

SAF operates directly on persisted document URIs. It does not copy a selected tree. SMB uses SMBJ low-level SMB2/3 APIs; SMB1 and automatic LAN discovery are not supported. Authentication stops after three failures. Share enumeration is isolated behind a service so a denied RPC can fall back to explicit share input.

Copy, move, and upload use a temporary destination name, close and size verification, then rename. Cross-provider moves additionally re-read and verify SHA-256 before removing the source. Collision policy is explicit (`overwrite`, `rename`, or `skip`) and can be applied to the remaining jobs. Failure and cancellation preserve the source and clean the temporary destination. Export through a transient system-selected tree offers `rename` or `skip`, but intentionally disables `overwrite`: without a durable grant and commit journal Android cannot recover the provider's old document after a process kill between replacement renames.

`TransferJobV1` is a Room journal consumed by a foreground `WorkManager` worker. Jobs can resume or roll back after process death. Notifications expose progress and cancellation. The journal, logs, DataStore, backups, crash output, and Rust bridge must never contain a plaintext password. The database filename is `transfer-jobs-v1.db`; its schema is additive and versioned.

## Codec and cache policy

The default codec route is internal first, then the Android OS. Per-format overrides are `default`, `internal_first`, `os_first`, `internal_only`, and `os_only`. Android `ImageDecoder` and Bitmap encoders run off the main thread. Only formats that pass the device round-trip probe are shown as OS-supported.

Materialized SMB/archive data is a temporary LRU. Auto capacity is 10% of free space, clamped to 256 MiB through 2 GiB while preserving at least 1 GiB free. In-use and in-transfer entries cannot be evicted.

## Migration and build

The first 0.0.19 launch releases legacy persisted URI grants and deletes only the former app-private `imported`, `.importing`, `config`, `picker.request`, and `import.ready` state. It never deletes an external document.

The supported toolchain is JDK 17, Gradle 9.1.0, AGP 9.0.1, built-in Kotlin/Compose compiler 2.2.10, Compose BOM 2026.06.00, NDK r27c (`27.2.12479018`), minSdk 29, and compile/targetSdk 36. Local caches and generated test data must be kept under `C:\temp\wml2viewer` (or an ignored `.test*` directory) and removed after validation.

Production signing and publishing are intentionally outside this implementation. CI produces a combined arm64/x86_64 debug APK, an arm64 unsigned release APK, and an arm64 unsigned release AAB.

CI runs the last-location instrumentation in two separate runner invocations with an actual `am force-stop` between seed and verification, and deletes a unique Android Keystore alias to verify the typed credential-reentry path. Emulator jobs also scan StrictMode/ANR output and application data/logs for a randomized credential sentinel. SMB2 and SMB3 integration run against a pinned Samba container; device-specific Pixel 6a latency and long-run native-memory acceptance remain hardware gates rather than hosted-emulator claims.
