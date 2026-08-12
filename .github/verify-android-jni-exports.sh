#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 0 ]]; then
    echo "usage: $0 <libwml2viewer_android.so> [...]" >&2
    exit 2
fi

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bridge_file="${repository_root}/android/app/src/main/java/io/github/mith_mmk/wml2viewer/nativebridge/NativeBridge.kt"
symbol_prefix='Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_'

readelf="${LLVM_READELF:-}"
if [[ -z "${readelf}" ]]; then
    ndk_home="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
    if [[ -z "${ndk_home}" ]]; then
        echo "LLVM_READELF or ANDROID_NDK_HOME must be set" >&2
        exit 2
    fi
    readelf="${ndk_home}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf"
fi

if [[ ! -x "${readelf}" ]]; then
    echo "llvm-readelf is not executable: ${readelf}" >&2
    exit 2
fi
if [[ ! -f "${bridge_file}" ]]; then
    echo "NativeBridge declaration is missing: ${bridge_file}" >&2
    exit 2
fi

temporary_directory="$(mktemp -d)"
trap 'rm -rf "${temporary_directory}"' EXIT

sed -nE 's/^[[:space:]]*external[[:space:]]+fun[[:space:]]+([[:alnum:]_]+).*/\1/p' "${bridge_file}" \
    | sort -u \
    | sed "s/^/${symbol_prefix}/" \
    > "${temporary_directory}/expected"

if [[ ! -s "${temporary_directory}/expected" ]]; then
    echo "no Kotlin external methods were found" >&2
    exit 1
fi

for library in "$@"; do
    if [[ ! -f "${library}" ]]; then
        echo "JNI library is missing: ${library}" >&2
        exit 1
    fi
    "${readelf}" --dyn-syms --wide "${library}" \
        | sed -nE "s/.*(${symbol_prefix}[[:alnum:]_]+).*/\1/p" \
        | sort -u \
        > "${temporary_directory}/actual"
    if ! diff -u "${temporary_directory}/expected" "${temporary_directory}/actual"; then
        echo "JNI exports do not match NativeBridge.kt: ${library}" >&2
        exit 1
    fi
    echo "verified $(wc -l < "${temporary_directory}/actual") JNI exports: ${library}"
done
