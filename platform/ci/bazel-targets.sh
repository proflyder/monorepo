#!/usr/bin/env bash
# Narrow a candidate target list to the targets affected between BASE and HEAD
# using bazel-contrib/target-determinator.
#
#   platform/ci/bazel-targets.sh \
#     --base "$PR_BASE_SHA" \
#     --candidates "//projects/mobile/android:android_tests //app:smoke_test" \
#     --universe '//projects/mobile/android/... + //packages/kotlin/...'
#
# Prints the space separated subset of --candidates that must run, and exits 0.
# Any failure (missing binary, unclean tree, timeout, query error) prints every
# candidate instead: the gate degrades to "run more", never to "run nothing".
#
# target-determinator needs the before-revision's analysis graph, so the caller
# must provide JDK/Android SDK and a full-history checkout.
set -uo pipefail

BASE=""
CANDIDATES=""
UNIVERSE="//..."
BAZEL="${BAZEL:-bazel}"
TD="${TARGET_DETERMINATOR:-target-determinator}"
TIMEOUT="${TARGET_DETERMINATOR_TIMEOUT:-900}"

while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE="$2"; shift 2 ;;
    --candidates) CANDIDATES="$2"; shift 2 ;;
    --universe) UNIVERSE="$2"; shift 2 ;;
    --bazel) BAZEL="$2"; shift 2 ;;
    --td) TD="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

fallback() {
  echo "target-determinator unavailable or failed ($1); using every candidate" >&2
  echo "$CANDIDATES"
  exit 0
}

[ -n "$CANDIDATES" ] || { echo ""; exit 0; }
[ -n "$BASE" ] || fallback "no base revision"
command -v "$TD" >/dev/null 2>&1 || [ -x "$TD" ] || fallback "binary not found: $TD"

CACHE_DIR="${TARGET_DETERMINATOR_CACHE:-${HOME}/.cache/target-determinator}"
AFFECTED_FILE="$(mktemp)"
LOG_FILE="$(mktemp)"
trap 'rm -f "$AFFECTED_FILE" "$LOG_FILE"' EXIT

run_td() {
  # `timeout` is GNU coreutils; macOS usually only has it when installed.
  if command -v timeout >/dev/null 2>&1; then
    timeout "$TIMEOUT" "$@"
  else
    "$@"
  fi
}

if ! run_td "$TD" \
  --bazel="$BAZEL" \
  --targets="$UNIVERSE" \
  --cache-dir="$CACHE_DIR" \
  --delete-cached-worktree \
  "$BASE" >"$AFFECTED_FILE" 2>"$LOG_FILE"; then
  tail -5 "$LOG_FILE" >&2 || true
  fallback "target-determinator exited non-zero"
fi

selected=""
for candidate in $CANDIDATES; do
  if grep -qxF "$candidate" "$AFFECTED_FILE"; then
    selected="${selected}${selected:+ }${candidate}"
  fi
done

if [ -z "$selected" ]; then
  echo "no candidate target is affected between ${BASE} and HEAD; skipping" >&2
fi
echo "$selected"
