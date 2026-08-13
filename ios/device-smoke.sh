#!/bin/sh
set -eu

device_id="${1:-${DEVICE_ID:-}}"
if [ -z "$device_id" ]; then
  echo "usage: ios/device-smoke.sh DEVICE_ID" >&2
  echo "or set DEVICE_ID to a connected CoreDevice identifier" >&2
  exit 2
fi

script_directory="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
project_root="$(CDPATH= cd -- "$script_directory/.." && pwd)"
. "$script_directory/device-signing.sh"
work_directory="$project_root/.test-ios-device-smoke"
derived_data="$work_directory/DerivedData"
bundle_id="io.github.mith-mmk.wml2viewer"
result_name="wml2viewer-device-smoke.json"
result_file="$work_directory/$result_name"
profile_plist="$work_directory/profile.plist"
device_token="$(date -u +%Y%m%dT%H%M%SZ)-$$"

mkdir -p "$work_directory"
rm -f "$result_file" "$profile_plist"

development_team="$(find_development_team "$bundle_id" "$profile_plist" || true)"

if [ -z "$development_team" ]; then
  echo "No provisioning profile for $bundle_id was found." >&2
  echo "Set DEVELOPMENT_TEAM after installing an iOS Development profile." >&2
  exit 2
fi

xcrun devicectl device info details --device "$device_id" > "$work_directory/device.txt"

xcodebuild \
  -project "$project_root/ios/Wml2Viewer.xcodeproj" \
  -scheme Wml2ViewerUnitTests \
  -configuration Debug \
  -destination "platform=iOS,id=$device_id" \
  -derivedDataPath "$derived_data" \
  DEVELOPMENT_TEAM="$development_team" \
  CODE_SIGN_STYLE=Automatic \
  -quiet \
  test

application="$derived_data/Build/Products/Debug-iphoneos/Wml2Viewer.app"
[ -d "$application" ] || { echo "Built application was not found: $application" >&2; exit 2; }

xcrun devicectl device install app --device "$device_id" "$application" \
  --json-output "$work_directory/install.json"
xcrun devicectl device process launch --device "$device_id" --terminate-existing \
  --json-output "$work_directory/launch.json" \
  "$bundle_id" --native-self-test "$device_token"

attempt=0
while [ "$attempt" -lt 15 ]; do
  if xcrun devicectl device copy from \
    --device "$device_id" \
    --domain-type appDataContainer \
    --domain-identifier "$bundle_id" \
    --source "Library/Caches/$result_name" \
    --destination "$result_file" \
    --json-output "$work_directory/copy.json" >/dev/null 2>&1; then
    break
  fi
  attempt=$((attempt + 1))
  sleep 1
done

[ -f "$result_file" ] || { echo "The app did not produce its native self-test result." >&2; exit 1; }
actual_token="$(plutil -extract token raw -o - "$result_file")"
actual_status="$(plutil -extract status raw -o - "$result_file")"

if [ "$actual_token" != "$device_token" ]; then
  echo "Stale device smoke result: expected $device_token, got $actual_token" >&2
  exit 1
fi
if [ "$actual_status" != "ok" ]; then
  plutil -p "$result_file" >&2
  exit 1
fi

echo "iOS device tests passed on $device_id (XCTest + native bridge + install + launch)."
