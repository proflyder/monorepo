#!/usr/bin/env bash
# Verifies that the built APK actually carries the call feature's manifest:
# library manifests are only merged when the library sets exports_manifest.
set -euo pipefail
aapt2="$1"
apk="$2"

if ! command -v "$aapt2" >/dev/null 2>&1 && [ ! -x "$aapt2" ]; then
  echo "aapt2 not found: $aapt2" >&2
  exit 1
fi

manifest="$("$aapt2" dump xmltree --file AndroidManifest.xml "$apk")"

missing=0
require() {
  if ! grep -Fq "$1" <<<"$manifest"; then
    echo "missing from packaged AndroidManifest.xml: $1" >&2
    missing=1
  fi
}

require "dev.proflyder.home.feature.call.presentation.CallActivity"
require "dev.proflyder.home.feature.call.telecom.HomeConnectionService"
require "dev.proflyder.home.feature.call.CallForegroundService"
require "android.permission.MANAGE_OWN_CALLS"
require "android.permission.FOREGROUND_SERVICE_PHONE_CALL"

if [ "$missing" -ne 0 ]; then
  echo "the call feature manifest was not merged into the APK" >&2
  exit 1
fi

echo "APK manifest carries CallActivity, both call services and the call permissions"
