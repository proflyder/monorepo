#!/usr/bin/env bash
# Build and push the Hermes gateway and voice-proxy images to GHCR.
#
# This is the build+push half of the Hermes image flow. The deploy-home
# workflow then calls `hermes_rollout.py`, which promotes the pushed digests
# into the `runtime` Helm release, waits for the rollout, runs the voice smoke
# checks and rolls back to the previous revision on failure.
#
#   IMAGE_TAG=<git-sha> DIGEST_FILE=<path> ./build-and-push.sh hermes voice-proxy
#
# `DIGEST_FILE` receives one `<image>=<sha256:…>` line per pushed image; the
# workflow passes those digests to the rollout script. A manual rollout can be
# run with `hermes_rollout.py --values-file <private values> --hermes-digest …
# --voice-proxy-digest …`; `projects/home/kubernetes/versions.sh` records the
# current pins.
#
# Authentication is the job-scoped GITHUB_TOKEN already logged in by the
# caller (`docker login ghcr.io`). The upstream Hermes revision comes from
# `projects/home/kubernetes/versions.sh`; the voice proxy is built from the
# tracked `projects/home-assistant/hermes/voice_proxy.py`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
IMAGES_DIR="$ROOT/projects/home-assistant/deploy/images"
HERMES_DIR="$ROOT/projects/home-assistant/hermes"
REGISTRY=${REGISTRY:-"ghcr.io/proflyder"}
IMAGE_TAG=${IMAGE_TAG:-$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)}
UPSTREAM=${HERMES_UPSTREAM:-https://github.com/NousResearch/hermes-agent.git}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

[[ $# -gt 0 ]] || { echo "usage: $0 hermes voice-proxy [...]" >&2; exit 2; }

push_image() {
  local repository=$1
  docker tag "$repository:build" "$repository:${IMAGE_TAG}"
  docker push "$repository:${IMAGE_TAG}" >/dev/null
  if [[ "$IMAGE_TAG" != develop ]]; then
    docker tag "$repository:build" "$repository:develop"
    docker push "$repository:develop" >/dev/null
  fi
  local digest
  digest=$(docker inspect --format '{{index .RepoDigests 0}}' "$repository:${IMAGE_TAG}")
  if [[ -n "${DIGEST_FILE:-}" ]]; then
    printf '%s=%s\n' "${repository##*/}" "${digest#*@}" >> "$DIGEST_FILE"
  fi
  echo "pushed ${digest} (tags ${IMAGE_TAG}, develop)"
}

for image in "$@"; do
  case "$image" in
    voice-proxy)
      echo "== building voice-proxy"
      docker build \
        -f "$IMAGES_DIR/voice-proxy/Dockerfile" \
        -t "${REGISTRY}/voice-proxy:build" \
        "$HERMES_DIR"
      push_image "${REGISTRY}/voice-proxy"
      ;;
    hermes)
      revision=$(sed -n 's/^HERMES_REVISION=//p' "$ROOT/projects/home/kubernetes/versions.sh")
      [[ -n "$revision" ]] || { echo "HERMES_REVISION missing in versions.sh" >&2; exit 1; }
      echo "== building hermes at $revision"
      git clone --quiet --filter=blob:none "$UPSTREAM" "$WORK/hermes-src"
      mkdir -p "$WORK/hermes"
      git -C "$WORK/hermes-src" archive "$revision" | tar -x -C "$WORK/hermes"
      docker build \
        -f "$IMAGES_DIR/hermes/Dockerfile" \
        --label "org.opencontainers.image.revision=${revision}" \
        -t "${REGISTRY}/hermes:build" \
        "$WORK/hermes"
      docker image inspect "${REGISTRY}/hermes:build" >/dev/null
      push_image "${REGISTRY}/hermes"
      ;;
    *)
      echo "unknown image: $image" >&2
      exit 2
      ;;
  esac
done
