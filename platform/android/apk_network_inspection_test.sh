#!/usr/bin/env bash
set -euo pipefail

apk="$1"
expected="$2"

if [[ ! -f "$apk" ]]; then
  echo "APK not found: $apk" >&2
  exit 1
fi

packaged=0
if unzip -p "$apk" 'classes*.dex' | LC_ALL=C grep -aE 'com/chuckerteam/chucker|dev/proflyder/home/diagnostics' > /dev/null; then
  packaged=1
fi

case "$expected" in
  absent)
    if [[ "$packaged" -ne 0 ]]; then
      echo "network inspection is packaged in $apk" >&2
      echo "release and distribution builds must not link //projects/mobile/android/app/diagnostics" >&2
      exit 1
    fi
    echo "APK carries no Chucker or diagnostics classes"
    ;;
  present)
    if [[ "$packaged" -eq 0 ]]; then
      echo "home_network_inspection=on did not package network inspection in $apk" >&2
      exit 1
    fi
    echo "debug APK carries network inspection"
    ;;
  *)
    echo "expected 'absent' or 'present', got '$expected'" >&2
    exit 2
    ;;
esac
