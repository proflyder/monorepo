#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/ci/prebuilt-digests.sh
source "$ROOT/platform/ci/prebuilt-digests.sh"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
IMAGES_DIR="$ROOT/projects/home-assistant/deploy/images"
REGISTRY=${REGISTRY:-"ghcr.io/proflyder"}
IMAGE_TAG=${IMAGE_TAG:-$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)}
UPSTREAM=${HERMES_UPSTREAM:-https://github.com/NousResearch/hermes-agent.git}
PLATFORM="--platforms=//platform/bazel:linux_amd64"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

[[ $# -gt 0 ]] || { echo "usage: $0 hermes voice-proxy [...]" >&2; exit 2; }

bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if type -P bazel >/dev/null 2>&1; then type -P bazel; return; fi
  if type -P bazelisk >/dev/null 2>&1; then type -P bazelisk; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

record_digest() {
  local name=$1 digest=$2
  if [[ -n "${DIGEST_FILE:-}" ]]; then
    printf '%s=%s\n' "$name" "$digest" >> "$DIGEST_FILE"
  fi
}

push_bazel_tag() {
  local digest=$1 tag=$2 repo=$3 target=$4
  local bin out pushed
  bin=$(bazel_bin)
  out=$( cd "$ROOT" && "$bin" run "$target" \
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

push_bazel_image() {
  local image=$1 image_target=$2
  local bin package layout digest repo push_target
  bin=$(bazel_bin)
  echo "== building ${image} with Bazel"
  ( cd "$ROOT" && "$bin" build "$image_target" "$PLATFORM" )
  package="${image_target%%:*}"
  package="${package#//}"
  layout="$ROOT/bazel-bin/${package}/${image}_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; return 1; }
  digest=$(home_deploy oci digest "$layout")
  repo="${REGISTRY}/${image}"
  push_target="${image_target%_image}_push"
  push_bazel_tag "$digest" "$IMAGE_TAG" "$repo" "$push_target"
  if [[ "$IMAGE_TAG" != develop ]]; then
    push_bazel_tag "$digest" develop "$repo" "$push_target"
  fi
  record_digest "$image" "$digest"
  echo "pushed ${repo}@${digest} (tags ${IMAGE_TAG}, develop)"
}

for image in "$@"; do
  case "$image" in
    voice-proxy|health-exporter|home-deploy|hermes) ;;
    *)
      echo "unknown image: $image" >&2
      exit 2
      ;;
  esac
  prebuilt=$(prebuilt_digest "$image")
  if [[ -n "$prebuilt" ]]; then
    record_digest "$image" "$prebuilt"
    echo "using prebuilt ${REGISTRY}/${image}@${prebuilt} (tags already promoted)"
    continue
  fi
  if [[ "${DEPLOY_NO_BUILD:-0}" == "1" ]]; then
    echo "no promoted digest for ${image}: PR CI did not publish it for this tree and" >&2
    echo "DEPLOY_NO_BUILD=1 forbids building it on the deployment runner (DECISIONS 36)." >&2
    exit 1
  fi
  case "$image" in
    voice-proxy)
      push_bazel_image voice-proxy //solutions/voice-proxy:voice-proxy_image
      ;;
    health-exporter)
      push_bazel_image health-exporter \
        //projects/home-assistant/deploy/health-exporter:health-exporter_image
      ;;
    home-deploy)
      push_bazel_image home-deploy //projects/home/kubernetes/scheduler:home-deploy_image
      ;;
    hermes)
      revision=$(tr -d '[:space:]' < "$IMAGES_DIR/hermes/revision")
      [[ "$revision" =~ ^[0-9a-f]{40}$ ]] || {
        echo "invalid hermes revision in $IMAGES_DIR/hermes/revision: '$revision'" >&2
        exit 1
      }
      echo "== building hermes at $revision"
      git clone --quiet --filter=blob:none "$UPSTREAM" "$WORK/hermes-src"
      mkdir -p "$WORK/hermes"
      git -C "$WORK/hermes-src" archive "$revision" | tar -x -C "$WORK/hermes"
      digest=$("$IMAGES_DIR/hermes/build.sh" \
        --context "$WORK/hermes" \
        --source-repo "$WORK/hermes-src" \
        --revision "$revision" \
        --cache-from "type=registry,ref=${REGISTRY}/hermes:buildcache" \
        --cache-to "type=registry,ref=${REGISTRY}/hermes:buildcache,mode=max" \
        --push --tag "${REGISTRY}/hermes:${IMAGE_TAG}")
      if [[ "$IMAGE_TAG" != develop ]]; then
        docker buildx imagetools create --prefer-index=false \
          --tag "${REGISTRY}/hermes:develop" "${REGISTRY}/hermes@${digest}"
        moved=$(docker buildx imagetools inspect \
          --format '{{.Manifest.Digest}}' "${REGISTRY}/hermes:develop")
        [[ "$moved" == "$digest" ]] || {
          echo "develop tag reports ${moved}, expected ${digest}" >&2
          exit 1
        }
      fi
      record_digest hermes "$digest"
      echo "pushed ${REGISTRY}/hermes@${digest} (tags ${IMAGE_TAG}, develop)"
      ;;
  esac
done
