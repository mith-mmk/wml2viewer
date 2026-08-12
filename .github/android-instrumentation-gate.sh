#!/usr/bin/env bash
set -uo pipefail

screen_size="${1:?screen size is required}"
screen_density="${2:?screen density is required}"
application_id='io.github.mith_mmk.wml2viewer'
test_runner='io.github.mith_mmk.wml2viewer.test/androidx.test.runner.AndroidJUnitRunner'
app_apk='app/build/outputs/apk/debug/app-debug.apk'
test_apk='app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk'
test_status=0

cleanup() {
  adb shell am force-stop "$application_id" >/dev/null 2>&1 || true
  adb shell wm size reset >/dev/null 2>&1 || true
  adb shell wm density reset >/dev/null 2>&1 || true
}
trap cleanup EXIT

run_last_location_phase() {
  local phase="$1"
  local run_id="$2"
  local test_class='io.github.mith_mmk.wml2viewer.LastLocationProcessDeathInstrumentationTest'
  local output
  local command_status=0
  output="$(adb shell am instrument -w -r \
    -e class "$test_class" \
    -e lastLocationPhase "$phase" \
    -e lastLocationRunId "$run_id" \
    "$test_runner" 2>&1)" || command_status=$?
  printf '%s\n' "$output"
  if [ "$command_status" -ne 0 ] || \
     grep -Eq 'INSTRUMENTATION_STATUS_CODE: -[0-9]+|FAILURES!!!|INSTRUMENTATION_FAILED' <<<"$output" || \
     ! grep -Fq 'OK (1 test)' <<<"$output" || \
     ! grep -Fq 'INSTRUMENTATION_CODE: -1' <<<"$output"; then
    echo "Last-location process-death $phase phase failed" >&2
    return 1
  fi
}

adb shell wm size "$screen_size" || test_status=1
adb shell wm density "$screen_density" || test_status=1
adb logcat -c || test_status=1

leak_sentinel="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')"
echo "::add-mask::$leak_sentinel"
if ! bash ./gradlew connectedDebugAndroidTest --no-daemon \
  -Pandroid.testInstrumentationRunnerArguments.secretLeakSentinel="$leak_sentinel"; then
  test_status=1
fi

process_death_id="$(od -An -N8 -tx1 /dev/urandom | tr -d ' \n')"
if [ "$test_status" -eq 0 ]; then
  if adb install -r "$app_apk" && adb install -r -t "$test_apk"; then
    if run_last_location_phase seed "$process_death_id"; then
      adb shell am force-stop "$application_id" || test_status=1
      run_last_location_phase verify "$process_death_id" || test_status=1
    else
      test_status=1
    fi
  else
    echo 'Failed to reinstall instrumentation APKs for process-death validation' >&2
    test_status=1
  fi
fi

app_pid="$(adb shell pidof "$application_id" 2>/dev/null | tr -d '\r' | awk '{print $1}')"
strictmode_log=''
if [ -n "$app_pid" ]; then
  strictmode_log="$(adb logcat -d -v brief --pid "$app_pid" | grep -E -i \
    'StrictMode (ThreadPolicy|VmPolicy|policy) violation|android\.os\.strictmode\..*Violation' || true)"
fi
if [ -n "$strictmode_log" ]; then
  echo 'StrictMode disk, network, or resource leak violation detected' >&2
  printf '%s\n' "$strictmode_log" >&2
  test_status=1
fi

anr_log="$(adb logcat -d -v brief | grep -E -i \
  'ANR in io\.github\.mith_mmk\.wml2viewer|am_anr.*io\.github\.mith_mmk\.wml2viewer|Input dispatching timed out.*io\.github\.mith_mmk\.wml2viewer' || true)"
if [ -n "$anr_log" ]; then
  echo 'Application-not-responding event detected for wml2viewer' >&2
  test_status=1
fi

secret_log="$(adb logcat -d -v brief | grep -F "$leak_sentinel" || true)"
if [ -n "$secret_log" ]; then
  echo 'Plaintext credential sentinel detected in device logs' >&2
  test_status=1
fi

exit "$test_status"
