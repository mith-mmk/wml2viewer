# iOS build and verification

The iOS target is intended for iOS 17 or later. The Xcode project is `ios/Wml2Viewer.xcodeproj`.
The target includes both iPhone and iPad (`UIDeviceFamily` 1,2), supports portrait and landscape orientations, and uses the same touch/swipe/pinch viewer controls on iPad.

## Prerequisites

- Xcode with an iOS 17 or later SDK and Simulator runtime
- Rust targets `aarch64-apple-ios` and `aarch64-apple-ios-sim`
- A working Apple signing identity for device builds

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
open ios/Wml2Viewer.xcodeproj
```

The Xcode Rust build phase runs `ios/build-rust.sh`. It produces a static library in Xcode's derived-data directory; generated build products are not stored in the repository.

## Storage model

Files is the primary entry point on iOS. Cold-launch URLs, URLs delivered while the app is running, and picker results are serialized by one import coordinator. The selected folder or file is copied into `Documents/snapshots/<generation>/folder`; picker cancellation and failure are reported through an import status marker so the empty state can be retried. A successful folder import opens the first supported item and closes the auxiliary filer.

Rust reads the atomic `Application Support/current.json` reference and keeps the previous generation intact until a later cleanup. Rust and workers only receive the canonical snapshot path. Display names come from snapshot metadata and are never passed back as filesystem paths. The snapshot is read-only from the viewer's perspective; move, copy, delete, and rename are disabled on iOS.

The in-app filer remains available for browsing an imported snapshot. It is not shown on first launch, has an always-visible Close action, and can be reopened from the empty-state snapshot action or the long-press context menu. On iPhone it occupies the main content area; on iPad it is intended as an optional side panel.

The default touch behavior is shared with Android: left swipe advances, right swipe goes back, double tap toggles Fit, long press opens the context menu, and pinch zooms. Viewer page swipes are disabled while zoomed beyond Fit, and gestures are blocked by filer/settings/dialog overlays.

## Network model

`src/filesystem/provider.rs` defines the read-only `list`, `stat`, and `materialize` boundary. `src/filesystem/source.rs` adds provider-independent source entries and a remote navigation worker. Local snapshots remain on the existing `PathBuf` worker; SMB uses source identity for navigation and only materializes the selected page for the existing render worker.

SMB directory enumeration, metadata lookup, cancellation, versioned cache reuse, streaming materialization, and explicit reconnect are implemented at the provider boundary. A dedicated SMB worker/runtime reuses its client between operations. A credential reference resolves the password through the Swift/iOS Keychain bridge; only the username and opaque reference belong on the Rust side. The in-app SMB browser is separate from Files: Files creates local snapshots, while SMB remains a remote read-only source. HTTP/WebDAV and cloud OAuth providers must reuse the same read-only boundary.

The default remote cache and materialization limits are 512 MiB. Prefetch defaults to the next two and previous one page, and is cancelled at chunk boundaries when a newer source request supersedes it. A disconnected share keeps the current rendered page visible and reports a retryable paused state; it never advances automatically to another page.

## Simulator acceptance checklist

- Build and launch on an iOS 17+ Simulator without a second `UIApplicationMain` or a black screen.
- Launch on an iPad Simulator in portrait and landscape; confirm the filer panel remains usable beside the viewer.
- Cold-launch a file and warm-launch consecutive files from Files; confirm ordered import processing.
- Select a folder, import a snapshot, confirm the first supported item opens and the auxiliary filer closes, restart, and confirm restoration.
- Cancel and retry the picker; confirm failed copies do not leave a ready marker or partial snapshot.
- Open image, ZIP/LHA, and `.wmltxt` content; test tap, swipe, pinch zoom, Safe Area, and rotation.
- Test empty folders, duplicate names, interrupted imports, and failed copies.
- Test provider cancellation, authentication failure, disconnected shares, and cache reuse with a mock provider; confirm one real SMB2/3 share on a reachable local network before declaring SMB complete.
- Confirm no password, SMB URL, or absolute user path is written to config or logs.
