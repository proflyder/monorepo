#!/usr/bin/env bash
# Download the pinned target-determinator release and verify its checksum.
#
#   platform/ci/install-target-determinator.sh [destination]
#
# Prints the absolute path of the installed binary. The version and every
# per-platform sha256 are pinned in this file; a mismatch aborts.
set -euo pipefail

VERSION="0.34.0"
DEST="${1:-${RUNNER_TEMP:-/tmp}/target-determinator}"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

case "$(uname -s)" in
  Linux) OS="linux" ;;
  Darwin) OS="darwin" ;;
  *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) ARCH="amd64" ;;
  arm64 | aarch64) ARCH="arm64" ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

case "${OS}.${ARCH}" in
  linux.amd64) EXPECTED="115e1c63d39e2cd0d0b011c9fadc80f059f021176a4ae0de2232cdd83b1f8011" ;;
  linux.arm64) EXPECTED="e818a59b1813ba4053eb0011a5302932cdc32a7879ae019ac4ef8f879c3953a9" ;;
  darwin.amd64) EXPECTED="94fc83fc2097f258c55872ac8954fc22abd831b77ad19fcbcd8b9e4682943eed" ;;
  darwin.arm64) EXPECTED="1405ff844db1255fc1e10f28c04ed72ced648822c2a5d39a393a4d6a6b7b890d" ;;
  *) echo "unsupported platform: ${OS}.${ARCH}" >&2; exit 1 ;;
esac

URL="https://github.com/bazel-contrib/target-determinator/releases/download/v${VERSION}/target-determinator.${OS}.${ARCH}"
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

curl -sSfL -o "$TMP" "$URL"
ACTUAL="$(sha256 "$TMP")"
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo "checksum mismatch for ${URL}" >&2
  echo "expected: $EXPECTED" >&2
  echo "actual:   $ACTUAL" >&2
  exit 1
fi

install -m 0755 "$TMP" "$DEST"
echo "$DEST"
