#!/usr/bin/env bash
# Build and optionally publish the container images of one deploy unit.
#
# Called from the unit job (`_unit.yml`) after its Bazel tests, in the same
# Bazel server, so the `<image>_image` actions are analysis/action cache hits.
# The docker/mixed branch (Hermes Buildx) lives here as well, keeping one job
# per unit.
#
#   build-images.sh --unit scheduler --builder bazel \
#     --images "scheduler home-deploy" --target-root //projects/home/kubernetes/scheduler \
#     --push true --tree-tag tree-<hash> --pushed-file "$RUNNER_TEMP/pushed.txt"
#
#   build-images.sh --unit hermes --builder mixed \
#     --images "hermes voice-proxy health-exporter home-deploy" \
#     --bazel-images '{"voice-proxy":"//solutions/voice-proxy",...}' \
#     --push true --tree-tag tree-<hash> --pushed-file "$RUNNER_TEMP/pushed.txt"
#
# The pushed digests are appended as `image=sha256:...` lines for the
# attestation matrix. Pushing is the caller's decision (`--push true` only on a
# same-repository pull request with registry credentials).
set -euo pipefail

REGISTRY="ghcr.io/proflyder"
HERMES_CACHE_REF="ghcr.io/proflyder/hermes:buildcache"
TREE_TAG_RE='^tree-[0-9a-f]{40}$'
DIGEST_RE='^sha256:[0-9a-f]{64}$'

usage() {
  cat >&2 <<'EOF'
usage: build-images.sh --unit <id> --builder <bazel|docker|mixed> --images "<a b>" \
  [--target-root <//pkg>] [--bazel-images <json>] \
  --push <true|false> [--tree-tag <tree-hash>] [--pushed-file <path>]
EOF
  exit 2
}

unit=""
builder=""
images=""
target_root=""
bazel_images="{}"
push="false"
tree_tag=""
pushed_file="${RUNNER_TEMP:-/tmp}/pushed.txt"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --unit) unit=${2:-}; shift 2 ;;
    --builder) builder=${2:-}; shift 2 ;;
    --images) images=${2:-}; shift 2 ;;
    --target-root) target_root=${2:-}; shift 2 ;;
    --bazel-images) bazel_images=${2:-}; shift 2 ;;
    --push) push=${2:-}; shift 2 ;;
    --tree-tag) tree_tag=${2:-}; shift 2 ;;
    --pushed-file) pushed_file=${2:-}; shift 2 ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

[[ -n "$unit" && -n "$images" ]] || usage
case "$builder" in
  bazel|docker|mixed) ;;
  *) echo "unknown image builder: '$builder'" >&2; exit 2 ;;
esac
case "$push" in
  true|false) ;;
  *) echo "--push must be true or false, got '$push'" >&2; exit 2 ;;
esac
if [[ "$push" == "true" ]]; then
  [[ "$tree_tag" =~ $TREE_TAG_RE ]] || {
    echo "refusing to push without a full tree tag: '$tree_tag'" >&2
    exit 2
  }
fi

summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$1" >> "$GITHUB_STEP_SUMMARY"
  fi
}

record() {
  printf '%s=%s\n' "$1" "$2" >> "$pushed_file"
}

mkdir -p "$(dirname "$pushed_file")"
: > "$pushed_file"

read -r -a image_list <<< "$images"

# The release image must be byte-identical to the built image. `push-image.sh`
# compares the digest the registry kept with the digest the build produced.
push_bazel_image() {
  local image=$1 target=$2 digest=$3 repo=$4
  platform/ci/push-image.sh bazel \
    --repository "$repo" \
    --target "$target" \
    --digest "$digest" \
    --tag "$tree_tag"
  record "$image" "$digest"
  summary "- \`${image}\`: \`${repo}@${digest}\` (\`${tree_tag}\`)"
}

build_bazel_image() {
  local image=$1 package=$2 target_root=$3
  local layout="bazel-bin/${package}/${image}_image"
  bazel build "${target_root}:${image}_image" \
    --platforms=//platform/bazel:linux_amd64
  local digest repo
  digest=$("$HOME_DEPLOY" oci digest "$layout")
  [[ "$digest" =~ $DIGEST_RE ]] || {
    echo "invalid digest for ${image}_image: '$digest'" >&2
    exit 1
  }
  repo="${REGISTRY}/${image}"
  if [[ "$push" == "true" ]]; then
    push_bazel_image "$image" "${target_root}:${image}_push" "$digest" "$repo"
  else
    summary "- \`${image}\`: \`${repo}@${digest}\`"
  fi
}

build_docker_image() {
  local image=$1
  local revision cache_args=()
  cache_args=(--cache-from "type=registry,ref=${HERMES_CACHE_REF}")
  if [[ "$push" == "true" ]]; then
    cache_args+=(--cache-to "type=registry,ref=${HERMES_CACHE_REF},mode=max")
  fi
  case "$image" in
    hermes)
      revision=$(tr -d '[:space:]' < projects/home-assistant/deploy/images/hermes/revision)
      if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
        echo "invalid hermes revision in projects/home-assistant/deploy/images/hermes/revision: '$revision'" >&2
        exit 1
      fi
      git clone --quiet --filter=blob:none \
        https://github.com/NousResearch/hermes-agent.git "$RUNNER_TEMP/hermes-src"
      mkdir -p "$RUNNER_TEMP/hermes"
      git -C "$RUNNER_TEMP/hermes-src" archive "$revision" \
        | tar -x -C "$RUNNER_TEMP/hermes"
      if [[ "$push" == "true" ]]; then
        local digest
        digest=$(projects/home-assistant/deploy/images/hermes/build.sh \
          --context "$RUNNER_TEMP/hermes" \
          --source-repo "$RUNNER_TEMP/hermes-src" \
          --revision "$revision" \
          --metadata-file "$RUNNER_TEMP/hermes-metadata.json" \
          "${cache_args[@]}" \
          --push --tag "${REGISTRY}/${image}:${tree_tag}")
        record "$image" "$digest"
        summary "- \`${image}\`: \`${REGISTRY}/${image}@${digest}\` (\`${tree_tag}\`)"
      else
        projects/home-assistant/deploy/images/hermes/build.sh \
          --context "$RUNNER_TEMP/hermes" \
          --source-repo "$RUNNER_TEMP/hermes-src" \
          --revision "$revision" \
          "${cache_args[@]}" \
          --oci-dir "$RUNNER_TEMP/hermes-oci"
        summary "- \`${image}\`: built, not published"
      fi
      ;;
    *)
      echo "unknown docker image: $image" >&2
      exit 2
      ;;
  esac
}

# Build the deploy CLI once in the same Bazel server; digest/push targets use it.
if [[ "$builder" == "bazel" || "$builder" == "mixed" ]]; then
  # Same platform as the unit checks and image targets: one analysis/server.
  bazel build //platform/deploy-rs:home-deploy \
    --platforms=//platform/bazel:linux_amd64
  HOME_DEPLOY="$PWD/bazel-bin/platform/deploy-rs/home-deploy"
  [[ -x "$HOME_DEPLOY" ]] || {
    echo "missing $HOME_DEPLOY" >&2
    exit 1
  }
  export HOME_DEPLOY
fi

case "$builder" in
  bazel)
    if [[ "$target_root" != //* ]]; then
      echo "bazel unit '$unit' without a //target_root: '$target_root'" >&2
      exit 2
    fi
    package="${target_root#//}"
    for image in "${image_list[@]}"; do
      build_bazel_image "$image" "$package" "$target_root"
    done
    ;;
  docker)
    for image in "${image_list[@]}"; do
      build_docker_image "$image"
    done
    ;;
  mixed)
    if [[ -z "$bazel_images" || "$bazel_images" == "{}" ]]; then
      echo "mixed unit '$unit' without deploy.bazel_images" >&2
      exit 2
    fi
    for image in "${image_list[@]}"; do
      image_root=$(printf '%s' "$bazel_images" \
        | jq -r --arg image "$image" '.[$image] // ""')
      if [[ -n "$image_root" ]]; then
        if [[ "$image_root" != //* ]]; then
          echo "deploy.bazel_images.${image} is not a Bazel package: '$image_root'" >&2
          exit 2
        fi
        build_bazel_image "$image" "${image_root#//}" "$image_root"
      else
        build_docker_image "$image"
      fi
    done
    ;;
esac

# The deploy CLI image is a language-independent artifact: deploy-home extracts
# its binary and never compiles the tool on a deployment runner. Publish it for
# every affected deployable unit unless the unit already pushed it above.
if [[ "$push" == "true" && ("$builder" == "bazel" || "$builder" == "mixed") ]]; then
  has_home_deploy=false
  for image in "${image_list[@]}"; do
    if [[ "$image" == "home-deploy" ]]; then
      has_home_deploy=true
    fi
  done
  if [[ "$has_home_deploy" == "false" ]]; then
    target_root="//projects/home/kubernetes/scheduler"
    package="${target_root#//}"
    bazel build "${target_root}:home-deploy_image" \
      --platforms=//platform/bazel:linux_amd64
    layout="bazel-bin/${package}/home-deploy_image"
    digest=$("$HOME_DEPLOY" oci digest "$layout")
    repo="${REGISTRY}/home-deploy"
    push_bazel_image home-deploy "${target_root}:home-deploy_push" "$digest" "$repo"
  else
    summary "- \`home-deploy\`: already built with the unit"
  fi
fi
