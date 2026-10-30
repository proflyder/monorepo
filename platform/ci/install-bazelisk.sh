#!/usr/bin/env bash
# Install the pinned Bazelisk release (linux/amd64) used by CI.
#
# The version and checksum are pinned here so workflows do not repeat them.
# Local runs can use an existing Bazelisk instead: `BAZEL=bazelisk make ...`.
set -euo pipefail

VERSION="1.30.0"
SHA256="a248ae2b2b62b5baebb748f6fba7203bff1a27e5d01b55399a8406744cb539e8"
DEST="${1:-/usr/local/bin/bazelisk}"

if [ "$(uname -s)" != "Linux" ]; then
  if command -v bazelisk >/dev/null 2>&1; then
    command -v bazelisk
    exit 0
  fi
  echo "this script installs the linux/amd64 Bazelisk used in CI" >&2
  echo "on macOS use the Bazelisk you already have: BAZEL=\$(command -v bazelisk)" >&2
  exit 1
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

curl -sSfL -o "$TMP" \
  "https://github.com/bazelbuild/bazelisk/releases/download/v${VERSION}/bazelisk-linux-amd64"
echo "${SHA256}  ${TMP}" | sha256sum -c -
install -m 0755 "$TMP" "$DEST"
echo "$DEST"
