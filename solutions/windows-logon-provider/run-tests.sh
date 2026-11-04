#!/usr/bin/env bash
# Host tests, rustfmt and clippy for host and Windows targets.
#
# CI runs this and ./build.sh (the actual cross-build).
set -euo pipefail

cd "$(dirname "$0")"

ROOT="$(cd ../.. && pwd)"

# A linked worktree has no (git-ignored) secrets/ directory, so fall back to the
# main checkout that owns the shared Git directory.
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  if type -P bazel >/dev/null 2>&1; then
    type -P bazel
    return 0
  fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then
    printf '%s\n' "$ROOT/secrets/local-tools/bazelisk"
    return 0
  fi
  local common main_root
  common=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || common=
  if [[ -n "$common" ]]; then
    main_root=$(dirname "$common")
    if [[ -x "$main_root/secrets/local-tools/bazelisk" ]]; then
      printf '%s\n' "$main_root/secrets/local-tools/bazelisk"
      return 0
    fi
  fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  return 1
}

BAZEL="$(bazel_bin)"

echo "== host tests, rustfmt and clippy =="
"$BAZEL" test //solutions/windows-logon-provider/...

echo "== windows-target rustfmt and clippy =="
"$BAZEL" test \
  //solutions/windows-logon-provider:rustfmt_windows \
  //solutions/windows-logon-provider:clippy_windows
