#!/usr/bin/env bash
set -euo pipefail

apk="$1"
expected="$2"

if [[ ! -f "$apk" ]]; then
  echo "APK not found: $apk" >&2
  exit 1
fi

packaged=0
if unzip -p "$apk" 'classes*.dex' | LC_ALL=C grep -aE 'com/google/firebase/appdistribution/impl/FirebaseAppDistributionImpl' > /dev/null; then
  packaged=1
fi

case "$expected" in
  absent)
    if [[ "$packaged" -ne 0 ]]; then
      echo "the Firebase App Distribution SDK is packaged in $apk" >&2
      echo "only the build uploaded to App Distribution may link the full SDK" >&2
      exit 1
    fi
    echo "APK carries only the App Distribution API artifact"
    ;;
  present)
    if [[ "$packaged" -eq 0 ]]; then
      echo "home_app_distribution=on did not package the App Distribution SDK in $apk" >&2
      exit 1
    fi
    echo "distribution APK carries the App Distribution SDK"
    ;;
  *)
    echo "expected 'absent' or 'present', got '$expected'" >&2
    exit 2
    ;;
esac
