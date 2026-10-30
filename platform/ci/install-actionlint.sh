#!/usr/bin/env bash
# Install the pinned actionlint release (linux/amd64) used by the quality job.
set -euo pipefail

VERSION="1.7.12"
SHA256="8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8"
DEST="${1:-/usr/local/bin/actionlint}"

if [ "$(uname -s)" != "Linux" ]; then
  if command -v actionlint >/dev/null 2>&1; then
    command -v actionlint
    exit 0
  fi
  echo "this script installs the linux/amd64 actionlint used in CI" >&2
  echo "on macOS install actionlint with 'brew install actionlint'" >&2
  exit 1
fi

TMP="$(mktemp)"
DIR="$(mktemp -d)"
trap 'rm -rf "$TMP" "$DIR"' EXIT

curl -sSfL -o "$TMP" \
  "https://github.com/rhysd/actionlint/releases/download/v${VERSION}/actionlint_${VERSION}_linux_amd64.tar.gz"
echo "${SHA256}  ${TMP}" | sha256sum -c -
tar -xzf "$TMP" -C "$DIR" actionlint
install -m 0755 "$DIR/actionlint" "$DEST"
echo "$DEST"
