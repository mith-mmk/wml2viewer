#!/bin/sh

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
    security cms -D -i "$signing_profile" > "$signing_profile_plist" 2>/dev/null || continue
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
