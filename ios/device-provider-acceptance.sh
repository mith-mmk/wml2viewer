#!/bin/sh
set -eu

usage() {
  echo "usage: ios/device-provider-acceptance.sh arm DEVICE_ID {local|icloud|third-party|smb}" >&2
  echo "       ios/device-provider-acceptance.sh collect DEVICE_ID" >&2
  exit 2
}

mode="${1:-}"
device_id="${2:-}"
[ -n "$mode" ] && [ -n "$device_id" ] || usage

script_directory="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
project_root="$(CDPATH= cd -- "$script_directory/.." && pwd)"
. "$script_directory/device-signing.sh"
work_directory="$project_root/.test-ios-provider-acceptance"
derived_data="$work_directory/DerivedData"
session_file="$work_directory/session.plist"
result_name="wml2viewer-provider-acceptance.json"
result_file="$work_directory/$result_name"
bundle_id="io.github.mith-mmk.wml2viewer"

case "$mode" in
  arm)
    provider="${3:-}"
    case "$provider" in
      local|icloud|third-party|smb) ;;
      *) usage ;;
    esac

    mkdir -p "$work_directory"
    profile_plist="$work_directory/profile.plist"
    development_team="$(find_development_team "$bundle_id" "$profile_plist" || true)"
    if [ -z "$development_team" ]; then
      echo "No provisioning profile for $bundle_id was found." >&2
      echo "Set DEVELOPMENT_TEAM after installing an iOS Development profile." >&2
      exit 2
    fi

    xcodebuild \
      -project "$project_root/ios/Wml2Viewer.xcodeproj" \
      -scheme Wml2Viewer \
      -configuration Debug \
      -destination "platform=iOS,id=$device_id" \
      -derivedDataPath "$derived_data" \
      DEVELOPMENT_TEAM="$development_team" \
      CODE_SIGN_STYLE=Automatic \
      -quiet \
      build

    application="$derived_data/Build/Products/Debug-iphoneos/Wml2Viewer.app"
    [ -d "$application" ] || {
      echo "Built application was not found: $application" >&2
      exit 2
    }
    xcrun devicectl device install app --device "$device_id" "$application" \
      --json-output "$work_directory/install.json"

    token="$(date -u +%Y%m%dT%H%M%SZ)-$$"
    plutil -create xml1 "$session_file"
    plutil -insert token -string "$token" "$session_file"
    plutil -insert provider -string "$provider" "$session_file"
    rm -f "$result_file"

    xcrun devicectl device process launch --device "$device_id" --terminate-existing \
      --json-output "$work_directory/launch.json" \
      "$bundle_id" --provider-acceptance "$token" "$provider"

    echo "Acceptance session armed for $provider."
    echo "On the device: open a folder with at least two supported images, move forward and backward, then open the filmstrip and wait for a thumbnail."
    echo "When finished, run: ios/device-provider-acceptance.sh collect $device_id"
    ;;

  collect)
    [ -f "$session_file" ] || {
      echo "No armed acceptance session was found." >&2
      exit 2
    }
    token="$(plutil -extract token raw -o - "$session_file")"
    provider="$(plutil -extract provider raw -o - "$session_file")"

    attempt=0
    while [ "$attempt" -lt 10 ]; do
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

    [ -f "$result_file" ] || {
      echo "The device has not produced an acceptance report." >&2
      exit 1
    }
    actual_token="$(plutil -extract token raw -o - "$result_file")"
    actual_provider="$(plutil -extract provider raw -o - "$result_file")"
    status="$(plutil -extract status raw -o - "$result_file")"
    [ "$actual_token" = "$token" ] || {
      echo "Stale acceptance report token." >&2
      exit 1
    }
    [ "$actual_provider" = "$provider" ] || {
      echo "Acceptance provider label mismatch." >&2
      exit 1
    }

    plutil -p "$result_file"
    if [ "$status" != "passed" ]; then
      picker_requested="$(plutil -extract pickerRequested raw -o - "$result_file")"
      folder_supported="$(plutil -extract folderSupportedItemCount raw -o - "$result_file")"
      moved_forward="$(plutil -extract movedForward raw -o - "$result_file")"
      moved_backward="$(plutil -extract movedBackward raw -o - "$result_file")"
      filmstrip_opened="$(plutil -extract filmstripOpened raw -o - "$result_file")"
      thumbnail_decoded="$(plutil -extract thumbnailDecoded raw -o - "$result_file")"
      if [ "$picker_requested" != "true" ]; then
        echo "Acceptance did not start: this session observed no Files picker request." >&2
        echo "Relaunch the armed build; it should present Files automatically." >&2
      elif [ "$folder_supported" -lt 2 ]; then
        echo "Files opened, but no folder with at least two supported items was committed." >&2
        echo "Choose the folder itself, or choose one image and then authorize its containing folder." >&2
      elif [ "$moved_forward" != "true" ] || [ "$moved_backward" != "true" ]; then
        echo "The folder is connected, but forward and backward page movement is incomplete." >&2
      elif [ "$filmstrip_opened" != "true" ]; then
        echo "Page movement passed, but the filmstrip has not been opened." >&2
      elif [ "$thumbnail_decoded" != "true" ]; then
        echo "The filmstrip opened, but no thumbnail decode was observed yet." >&2
      else
        echo "Acceptance is incomplete; inspect the report fields above." >&2
      fi
      exit 1
    fi

    echo "Physical Files acceptance passed for $provider on $device_id."
    rm -rf "$work_directory"
    ;;

  *) usage ;;
esac
