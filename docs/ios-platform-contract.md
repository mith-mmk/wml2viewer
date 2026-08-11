# iOS and iPadOS platform contract

This document defines the later iOS integration boundary; 0.0.19 does not ship an iOS application.

## Application architecture

- SwiftUI owns all screens, window classes, localization, accessibility, and state restoration.
- `UIDocumentPickerViewController` and security-scoped bookmarks provide document access. The app uses the system Files/provider ecosystem and does not implement an iOS SMB browser.
- ImageIO supplies the OS decoder/encoder route. Capability probing and per-format routing mirror Android semantics, without sharing Android codec code.
- Keychain may store platform secrets needed by future non-Files services. Android Keystore ciphertext and SMB profiles are never migrated or shared.

## Rust boundary

The iOS adapter must wrap the same platform-independent `wml2viewer-core` image, archive, reading, and spread contracts used by Android. Session handles and request ordering belong to the platform adapter rather than the core itself, but must preserve the Android bridge semantics:

| Operation | Contract |
|---|---|
| create/release session | opaque 64-bit handle; release is idempotent and invalidates pending requests |
| allocate/begin/cancel request | monotonic request ID per session; stale results are rejected |
| decode materialized item | accepts only an app-private local path and MIME type |
| image metadata and RGBA access | width, height, stride, and borrowed RGBA bytes tied to an image handle |
| animation access | poster/first frame, composited frame count, loop count, per-frame duration, and an independently owned frame image handle |
| release image | explicit, idempotent ownership release |
| open/release archive | ZIP/LHA/listed-file entry count, normalized relative name, declared size, and an opaque archive handle |
| decode archive entry | internal decode from a bounded materialized entry while the request remains current |
| materialize encoded entry | owned bounded bytes for ImageIO or cache routing, with explicit release and no borrow after release |
| encode RGBA | bounded strided RGBA input to PNG/JPEG/WebP, returning an owned encoded-byte handle while the request remains current |
| error result | stable error code plus typed arguments, never localized text |

The Swift wrapper must make session, image, archive, and encoded-byte handles `Closeable`-equivalent resources and release them deterministically. Image and encoded buffers become invalid immediately after their owning handle is released. Frame handles remain valid after their parent animation image is released because every frame is independently owned. Platform code must reject wrong-kind handles, double release, use-after-release, stale completion, and callbacks into a destroyed scene.

## Provider boundary

Swift presents bookmarked documents to the core using its own `sourceId + opaqueEntryId` model. When seekable local access is unavailable, it materializes only the selected item to an app cache, coordinates the security-scoped access lifetime, and then calls Rust. For a provider-backed listed file, Swift resolves the normalized relative entry name through the same provider and materializes that selected entry; Rust must not interpret a provider URL as a filesystem path. A bookmark, provider URL, authorization token, or user credential must not cross the Rust boundary.

Android SAF, Android SMBJ, Room, WorkManager, and Keystore implementations are not portable components and must not be copied into the iOS target.
