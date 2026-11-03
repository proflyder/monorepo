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
# `hermes` stays a Docker build from the upstream revision in
# `projects/home/kubernetes/versions.sh`. `voice-proxy` is now a hermetic Bazel
# image (//solutions/voice-proxy:voice-proxy_image, rules_oci/distroless) and is
# pushed byte-for-byte through the `oci_push` target: `docker push` would rewrite
# the media types and change the digest.
#
# Authentication is the job-scoped GITHUB_TOKEN already logged in by the
# caller (`docker login ghcr.io`); the Bazel push target reads the same Docker
# config.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
IMAGES_DIR="$ROOT/projects/home-assistant/deploy/images"
REGISTRY=${REGISTRY:-"ghcr.io/proflyder"}
IMAGE_TAG=${IMAGE_TAG:-$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)}
UPSTREAM=${HERMES_UPSTREAM:-https://github.com/NousResearch/hermes-agent.git}
PLATFORM="--platforms=@rules_go//go/toolchain:linux_amd64"
OCI="$ROOT/platform/ci/oci_image.py"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

[[ $# -gt 0 ]] || { echo "usage: $0 hermes voice-proxy [...]" >&2; exit 2; }

# Resolve Bazelisk without shadowing `bazel` with a function of that name.
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if type -P bazel >/dev/null 2>&1; then type -P bazel; return; fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then echo "$ROOT/secrets/local-tools/bazelisk"; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

record_digest() {
  local name=$1 digest=$2
  if [[ -n "${DIGEST_FILE:-}" ]]; then
    printf '%s=%s\n' "$name" "$digest" >> "$DIGEST_FILE"
  fi
}

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
  record_digest "${repository##*/}" "${digest#*@}"
  echo "pushed ${digest} (tags ${IMAGE_TAG}, develop)"
}

# Push the exact built manifest under one tag and prove the registry kept the
# digest (a rewritten manifest would mean a different image).
push_bazel_tag() {
  local digest=$1 tag=$2 repo=$3
  local bin out pushed
  bin=$(bazel_bin)
  out=$( cd "$ROOT" && "$bin" run //solutions/voice-proxy:voice-proxy_push \
    "$PLATFORM" \
    -- --repository "$repo" --tag "$tag" 2>&1 ) || {
      printf '%s\n' "$out" >&2
      echo "pushing ${repo}:${tag} failed" >&2
      return 1
    }
  pushed=$(printf '%s\n' "$out" | grep -oE 'sha256:[0-9a-f]{64}' | tail -1)
  if [[ "$pushed" != "$digest" ]]; then
    printf '%s\n' "$out" >&2
    echo "pushed digest ${pushed:-?} does not match built digest ${digest} for ${repo}:${tag}" >&2
    return 1
  fi
}

for image in "$@"; do
  case "$image" in
    voice-proxy)
      echo "== building voice-proxy with Bazel"
      bin=$(bazel_bin)
      ( cd "$ROOT" && "$bin" build //solutions/voice-proxy:voice-proxy_image "$PLATFORM" )
      layout="$ROOT/bazel-bin/solutions/voice-proxy/voice-proxy_image"
      [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
      digest=$(python3 "$OCI" digest "$layout")
      repo="${REGISTRY}/voice-proxy"
      push_bazel_tag "$digest" "$IMAGE_TAG" "$repo"
      if [[ "$IMAGE_TAG" != develop ]]; then
        push_bazel_tag "$digest" develop "$repo"
      fi
      record_digest voice-proxy "$digest"
      echo "pushed ${repo}@${digest} (tags ${IMAGE_TAG}, develop)"
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
