#!/bin/sh

decode_provisioning_profile() {
  profile_path="$1"
  decoded_path="$2"
  if security cms -D -i "$profile_path" > "$decoded_path" 2>/dev/null; then
    return 0
  fi
  # Some macOS securityd states reject `security cms` while the same
  # detached CMS profile remains parseable. Keep profile discovery useful so
  # the caller can report the real next failure (usually a missing signing
  # identity) instead of claiming that the profile does not exist.
  openssl smime -inform DER -verify -noverify \
    -in "$profile_path" -out "$decoded_path" >/dev/null 2>&1
}

find_development_team() {
  signing_bundle_id="$1"
  signing_profile_plist="$2"
  signing_team="${DEVELOPMENT_TEAM:-}"
  if [ -n "$signing_team" ]; then
    printf '%s\n' "$signing_team"
    return 0
  fi

  signing_profile_root="${HOME:?}/Library/Developer/Xcode/UserData/Provisioning Profiles"
  for signing_profile in "$signing_profile_root"/*.mobileprovision; do
    [ -f "$signing_profile" ] || continue
    decode_provisioning_profile "$signing_profile" "$signing_profile_plist" || continue
    signing_identifier="$(plutil -extract Entitlements.application-identifier raw -o - "$signing_profile_plist" 2>/dev/null || true)"
    case "$signing_identifier" in
      *."$signing_bundle_id")
        plutil -extract TeamIdentifier.0 raw -o - "$signing_profile_plist"
        return 0
        ;;
    esac
  done
  return 1
}
