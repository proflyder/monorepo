#!/usr/bin/env bash
set -euo pipefail
apk="$1"
libs="$(unzip -Z1 "$apk" | grep '^lib/' || true)"
if [ -z "$libs" ]; then
  echo "no native libraries in $apk"
  exit 0
fi
abis="$(printf '%s\n' "$libs" | cut -d/ -f2 | sort -u)"
echo "APK ABIs: $(echo $abis)"
if [ "$abis" != "arm64-v8a" ]; then
  echo "expected only arm64-v8a native libraries, got: $(echo $abis)" >&2
  exit 1
fi
