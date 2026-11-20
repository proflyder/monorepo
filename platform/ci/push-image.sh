#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage:
  push-image.sh bazel  --repository <repo> --target <//pkg:image_push> --digest <sha256:…> --tag tree-<hash>
  push-image.sh docker --repository <repo> --source <local-ref> --tag tree-<hash>
EOF
  exit 2
}

mode=${1:-}
[[ -n "$mode" ]] || usage
shift

repository=""
target=""
digest=""
tag=""
source=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --repository) repository=${2:-}; shift 2 ;;
    --target) target=${2:-}; shift 2 ;;
    --digest) digest=${2:-}; shift 2 ;;
    --tag) tag=${2:-}; shift 2 ;;
    --source) source=${2:-}; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ "$tag" =~ ^tree-[0-9a-f]{40}$ ]] || {
  echo "refusing to push without a full tree tag: '$tag'" >&2
  exit 2
}

push_failed() {
  echo "pushing ${repository}:${tag} failed" >&2
  exit 1
}

digest_mismatch() {
  echo "pushed digest $1 for ${repository}:${tag} does not match the built digest ${digest}" >&2
  exit 1
}

case "$mode" in
  bazel)
    [[ -n "$repository" && -n "$target" && -n "$digest" ]] || usage
    [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
      echo "invalid built digest: '$digest'" >&2
      exit 2
    }
    if [[ -n "${BAZELISK:-}" ]]; then
      bin=$BAZELISK
    else
      bin=$(command -v bazel || true)
    fi
    [[ -n "$bin" ]] || { echo "bazel/bazelisk not found; set BAZELISK" >&2; exit 1; }
    out=$("$bin" run "$target" \
      --platforms=//platform/bazel:linux_amd64 \
      -- --repository "$repository" --tag "$tag" 2>&1) || {
      printf '%s\n' "$out" >&2
      push_failed
    }
    pushed=$(printf '%s\n' "$out" | grep -oE 'sha256:[0-9a-f]{64}' | tail -1 || true)
    [[ -n "$pushed" ]] || {
      printf '%s\n' "$out" >&2
      echo "could not read the pushed digest for ${repository}:${tag}" >&2
      exit 1
    }
    [[ "$pushed" == "$digest" ]] || digest_mismatch "$pushed"
    ;;
  docker)
    [[ -n "$repository" && -n "$source" ]] || usage
    docker tag "$source" "${repository}:${tag}"
    out=$(docker push "${repository}:${tag}" 2>&1) || {
      printf '%s\n' "$out" >&2
      push_failed
    }
    pushed=$(printf '%s\n' "$out" | grep -oE 'sha256:[0-9a-f]{64}' | tail -1 || true)
    [[ -n "$pushed" ]] || {
      printf '%s\n' "$out" >&2
      echo "could not read the pushed digest for ${repository}:${tag}" >&2
      exit 1
    }
    inspected=$(docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' \
      "${repository}:${tag}" | sed -n "s|^${repository//./\\.}@||p" | tail -1)
    [[ "$inspected" == "$pushed" ]] || {
      echo "local image record reports '${inspected:-none}' for ${repository}:${tag}," \
        "docker push reported '${pushed}'" >&2
      exit 1
    }
    printf '%s\n' "$pushed"
    ;;
  *)
    usage
    ;;
esac
