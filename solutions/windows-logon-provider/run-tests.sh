#!/usr/bin/env bash
# Host tests, rustfmt and clippy for host and Windows targets.
#
# CI runs this and ./build.sh (the actual cross-build).
set -euo pipefail

cd "$(dirname "$0")"

ROOT="$(cd ../.. && pwd)"

# Bazelisk comes from mise (`mise install`, see mise.toml).
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  if type -P bazel >/dev/null 2>&1; then
    type -P bazel
    return 0
  fi
  if type -P bazelisk >/dev/null 2>&1; then
    type -P bazelisk
    return 0
  fi
  echo "bazel/bazelisk not found: run \`mise install\` or set BAZELISK" >&2
  return 1
}

BAZEL="$(bazel_bin)"

echo "== host tests, rustfmt and clippy =="
"$BAZEL" test //solutions/windows-logon-provider/...

echo "== windows-target rustfmt and clippy =="
"$BAZEL" test \
  //solutions/windows-logon-provider:rustfmt_windows \
  //solutions/windows-logon-provider:clippy_windows
