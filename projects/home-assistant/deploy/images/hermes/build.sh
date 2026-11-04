#!/usr/bin/env bash
# Build the Hermes gateway image reproducibly (DECISIONS 36).
#
# This is the single Docker build used by PR CI (`.github/workflows/_images.yml`)
# and by deploy-home's fallback (`build-and-push.sh`). Both must pass the same
# arguments: a pinned base, the Debian snapshot from the Dockerfile and
# `SOURCE_DATE_EPOCH` = the upstream commit time. Buildx then rewrites layer
# timestamps (`rewrite-timestamp=true`), so two builds of the same revision and
# Dockerfile produce one manifest digest.
#
#   # CI: push straight under the tree tag and print the digest
#   build.sh --context "$DIR" --source-repo "$SRC" --revision "$SHA" \
#     --push --tag ghcr.io/proflyder/hermes:tree-<hash>
#
#   # Reproducibility check: two local OCI layouts in different directories
#   build.sh --context "$DIR" --source-repo "$SRC" --revision "$SHA" \
#     --oci-dir "$TMP/one"
#
# `--metadata-file` keeps Buildx's metadata (`containerimage.digest` and the
# descriptor); the digest is also printed as the last line on stdout.
#
# Buildx's own provenance attestation is disabled: it would add a manifest to
# the index (changing the digest) and duplicate the provenance that
# `actions/attest-build-provenance` publishes for the same digest.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKERFILE="$SCRIPT_DIR/Dockerfile"
PLATFORM=linux/amd64

usage() {
  cat >&2 <<'EOF'
usage: build.sh --context <dir> (--source-repo <git-dir> | --source-date-epoch <unix>)
                --revision <40-hex> (--push --tag <ref> | --oci-dir <dir>)
                [--metadata-file <file>] [--no-cache]
EOF
  exit 2
}

context=""
source_repo=""
source_date_epoch=""
revision=""
tag=""
push=false
oci_dir=""
metadata_file=""
no_cache=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --context) context=${2:-}; shift 2 ;;
    --source-repo) source_repo=${2:-}; shift 2 ;;
    --source-date-epoch) source_date_epoch=${2:-}; shift 2 ;;
    --revision) revision=${2:-}; shift 2 ;;
    --tag) tag=${2:-}; shift 2 ;;
    --push) push=true; shift ;;
    --oci-dir) oci_dir=${2:-}; shift 2 ;;
    --metadata-file) metadata_file=${2:-}; shift 2 ;;
    --no-cache) no_cache=(--no-cache); shift ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

[[ -n "$context" && -d "$context" ]] || { echo "missing build context: '$context'" >&2; exit 2; }
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || { echo "invalid revision: '$revision'" >&2; exit 2; }
[[ -f "$DOCKERFILE" ]] || { echo "missing Dockerfile: $DOCKERFILE" >&2; exit 1; }

if [[ -z "$source_date_epoch" ]]; then
  [[ -n "$source_repo" && -d "$source_repo" ]] || {
    echo "either --source-repo or --source-date-epoch is required" >&2
    exit 2
  }
  source_date_epoch=$(git -C "$source_repo" log -1 --format=%ct "$revision") || {
    echo "could not read the commit time of $revision from $source_repo" >&2
    exit 1
  }
fi
[[ "$source_date_epoch" =~ ^[0-9]+$ ]] || {
  echo "invalid SOURCE_DATE_EPOCH: '$source_date_epoch'" >&2
  exit 2
}

if [[ "$push" == true ]]; then
  [[ -n "$tag" ]] || usage
  output="type=image,name=${tag},rewrite-timestamp=true,push=true"
elif [[ -n "$oci_dir" ]]; then
  mkdir -p "$oci_dir"
  output="type=oci,dest=${oci_dir}/image.tar,rewrite-timestamp=true"
else
  usage
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
metadata="${metadata_file:-$work/metadata.json}"

echo "== building hermes at ${revision} (SOURCE_DATE_EPOCH=${source_date_epoch})" >&2
docker buildx build \
  --file "$DOCKERFILE" \
  --platform "$PLATFORM" \
  --build-arg "SOURCE_DATE_EPOCH=${source_date_epoch}" \
  --label "org.opencontainers.image.revision=${revision}" \
  --provenance=false \
  --output "$output" \
  --metadata-file "$metadata" \
  ${no_cache[@]+"${no_cache[@]}"} \
  "$context" >&2

digest=$(sed -n 's/^[[:space:]]*"containerimage.digest":[[:space:]]*"\(sha256:[0-9a-f]*\)".*/\1/p' "$metadata" | tail -1)
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "could not read containerimage.digest from ${metadata}" >&2
  cat "$metadata" >&2
  exit 1
}

if [[ "$push" == true ]]; then
  # The registry must keep the built manifest: reading the tag back proves the
  # digest the rollout will deploy is the digest this build produced.
  pushed=$(docker buildx imagetools inspect --format '{{.Manifest.Digest}}' "$tag") || {
    echo "could not inspect ${tag} after push" >&2
    exit 1
  }
  [[ "$pushed" == "$digest" ]] || {
    echo "pushed digest ${pushed} does not match built digest ${digest} for ${tag}" >&2
    exit 1
  }
fi

echo "$digest"
