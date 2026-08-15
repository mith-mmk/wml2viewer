#!/bin/sh
set -eu

platform="${1:?platform is required}"
archs="${2:-arm64}"
project_root="$(cd "$(dirname "$0")/.." && pwd)"

cargo_bin="$(command -v cargo || true)"
if [ -z "$cargo_bin" ]; then
  cargo_home="${CARGO_HOME:-${HOME:?}/.cargo}"
  [ -x "$cargo_home/bin/cargo" ] && cargo_bin="$cargo_home/bin/cargo"
fi
[ -n "$cargo_bin" ] || { echo "cargo not found; install Rust with rustup" >&2; exit 127; }

rustup_bin="$(command -v rustup || true)"
if [ -z "$rustup_bin" ]; then
  cargo_home="${CARGO_HOME:-${HOME:?}/.cargo}"
  [ -x "$cargo_home/bin/rustup" ] && rustup_bin="$cargo_home/bin/rustup"
fi
[ -n "$rustup_bin" ] || { echo "rustup not found; install the required Apple targets" >&2; exit 127; }

if ! "$cargo_bin" metadata --manifest-path "$project_root/Cargo.toml" --no-deps --format-version 1 | grep -q 'wml2viewer-ios'; then
  echo "wml2viewer-ios package is missing" >&2
  exit 2
fi

built_products="${BUILT_PRODUCTS_DIR:?}"
mkdir -p "$built_products"
library_count=0

for arch in $archs; do
  case "$platform:$arch" in
    iphoneos:arm64) target="aarch64-apple-ios" ;;
    iphonesimulator:arm64) target="aarch64-apple-ios-sim" ;;
    iphonesimulator:x86_64) target="x86_64-apple-ios" ;;
    *) echo "unsupported Apple architecture: $platform/$arch" >&2; exit 2 ;;
  esac

  if ! "$rustup_bin" target list --installed | grep -qx "$target"; then
    echo "Rust target is not installed: $target (run: rustup target add $target)" >&2
    exit 2
  fi

  "$cargo_bin" rustc --manifest-path "$project_root/Cargo.toml" --package wml2viewer-ios --target "$target" --release --lib -- --crate-type staticlib
  cp "$project_root/target/$target/release/libwml2viewer_ios.a" "$built_products/libwml2viewer_ios-$arch.a"
  library_count=$((library_count + 1))
done

if [ "$library_count" -eq 1 ]; then
  cp "$built_products/libwml2viewer_ios-$arch.a" "$built_products/libwml2viewer_ios.a"
else
  xcrun lipo -create "$built_products"/libwml2viewer_ios-*.a -output "$built_products/libwml2viewer_ios.a"
fi
