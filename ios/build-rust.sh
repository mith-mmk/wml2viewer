#!/bin/sh
set -eu

platform="${1:?platform is required}"
archs="${2:-arm64}"
project_root="$(cd "$(dirname "$0")/.." && pwd)"

case "$platform" in
  iphoneos)
    target="aarch64-apple-ios"
    ;;
  iphonesimulator)
    case " $archs " in
      *" arm64 "*) target="aarch64-apple-ios-sim" ;;
      *) echo "Only arm64 Simulator builds are supported" >&2; exit 2 ;;
  esac
  ;;
  *)
    echo "Unsupported Apple platform: $platform" >&2
  exit 2
  ;;
esac

cargo_bin="$(command -v cargo || true)"
if [ -z "$cargo_bin" ] && [ -x "${HOME:-}/.cargo/bin/cargo" ]; then
  cargo_bin="${HOME}/.cargo/bin/cargo"
fi
if [ -z "$cargo_bin" ]; then
  echo "cargo was not found; install Rust with rustup or set PATH/CARGO" >&2
  exit 127
fi

# The Rust crate also exposes rlib/cdylib for other consumers.  Xcode embeds
# the static archive, so build only that crate type here; otherwise cargo also
# links the cdylib before Swift's Keychain symbols are available to the app.
"$cargo_bin" rustc --manifest-path "$project_root/Cargo.toml" --target "$target" --release --lib -- --crate-type staticlib
mkdir -p "${BUILT_PRODUCTS_DIR:?}"
cp "$project_root/target/$target/release/libwml2viewer.a" "$BUILT_PRODUCTS_DIR/libwml2viewer.a"
