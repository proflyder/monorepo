#!/usr/bin/env bash
# Prove the Hermes image is reproducible (DECISIONS 36).
#
# Builds the same upstream revision twice into two different OCI layouts, with
# the layer cache disabled, and compares the manifest digests. Any difference
# means a floating input (base tag, apt index, timestamp) slipped into the
# Dockerfile and would cause a spurious rollout on every deploy.
#
# Needs `docker buildx` (any builder) and network access to GitHub, Docker Hub,
# GHCR, snapshots and PyPI. Runs in minutes and is a manual/`workflow_dispatch`
# check, not part of the default PR CI.
#
#   projects/home-assistant/deploy/images/hermes/verify-reproducible.sh
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

# Reuse an existing upstream checkout for repeated local runs; otherwise clone
# the full history (the pinned revision may be outside the default branch).
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
