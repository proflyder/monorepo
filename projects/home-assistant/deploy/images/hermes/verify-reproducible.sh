#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UPSTREAM=${HERMES_UPSTREAM:-https://github.com/NousResearch/hermes-agent.git}

revision=$(tr -d '[:space:]' < "$SCRIPT_DIR/revision")
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || {
  echo "invalid revision in $SCRIPT_DIR/revision: '$revision'" >&2
  exit 1
}

command -v docker >/dev/null 2>&1 || { echo "docker is required" >&2; exit 1; }
docker buildx version >/dev/null 2>&1 || { echo "docker buildx is required" >&2; exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [[ -n "${HERMES_SRC:-}" ]]; then
  [[ -d "$HERMES_SRC/.git" ]] || { echo "HERMES_SRC is not a git checkout: $HERMES_SRC" >&2; exit 1; }
  SRC=$HERMES_SRC
else
  echo "== ${revision}: cloning ${UPSTREAM}"
  git clone --quiet --filter=blob:none "$UPSTREAM" "$WORK/hermes-src"
  SRC=$WORK/hermes-src
fi
mkdir -p "$WORK/hermes"
git -C "$SRC" archive "$revision" | tar -x -C "$WORK/hermes"

build_once() {
  local out=$1
  "$SCRIPT_DIR/build.sh" \
    --context "$WORK/hermes" \
    --source-repo "$SRC" \
    --revision "$revision" \
    --oci-dir "$out" \
    --no-cache
}

echo "== first build"
first=$(build_once "$WORK/first")
echo "== second build"
second=$(build_once "$WORK/second")

echo "first:  $first"
echo "second: $second"
if [[ "$first" != "$second" ]]; then
  echo "Hermes image is NOT reproducible: two builds of $revision produced different digests" >&2
  exit 1
fi
echo "reproducible: $first"
