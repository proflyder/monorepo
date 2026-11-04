#!/usr/bin/env bash
# Build and push the Hermes gateway and voice-proxy images to GHCR.
#
# This is the build+push half of the Hermes image flow. The deploy-home
# workflow then calls `home-deploy hermes rollout`, which promotes the pushed
# digests into the `runtime` Helm release, waits for the rollout, runs the voice
# smoke checks and rolls back to the previous revision on failure.
#
#   IMAGE_TAG=<git-sha> DIGEST_FILE=<path> ./build-and-push.sh hermes voice-proxy
#
# `DIGEST_FILE` receives one `<image>=<sha256:…>` line per pushed image; the
# workflow passes those digests to the rollout subcommand. A manual rollout can
# be run with `home-deploy hermes rollout --values-file <private values>
# --hermes-digest … --voice-proxy-digest …`; `projects/home/kubernetes/
# versions.sh` records the current pins.
#
# `hermes` stays a Docker build from the upstream revision in
# `projects/home/kubernetes/versions.sh`. `voice-proxy`, `health-exporter` and
# `home-deploy` are hermetic Bazel images (rules_oci/distroless) and are pushed
# byte-for-byte through their `oci_push` targets: `docker push` would rewrite the
# media types and change the digest.
#
# Authentication is the job-scoped GITHUB_TOKEN already logged in by the
# caller (`docker login ghcr.io`); the Bazel push target reads the same Docker
# config.
#
# `PREBUILT_DIGESTS` / `PREBUILT_DIGESTS_FILE` carry the digests `deploy-home`
# verified and promoted from the pull-request build (DECISIONS 36). Those images
# are neither rebuilt nor pushed here; the digest is only recorded for the
# rollout. Local/manual runs set nothing and behave exactly as before.
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
PLATFORM="--platforms=@rules_go//go/toolchain:linux_amd64"
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

# Build one Bazel image, push it under the commit and develop tags, and record
# the digest. `image_target` is the package target of the `<image>_image` rule.
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
    # deploy-home already verified the provenance and attached the commit and
    # develop tags to this exact digest; rebuilding could only produce a
    # different image than the one that passed PR CI.
    record_digest "$image" "$prebuilt"
    echo "using prebuilt ${REGISTRY}/${image}@${prebuilt} (tags already promoted)"
    continue
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
  esac
done
