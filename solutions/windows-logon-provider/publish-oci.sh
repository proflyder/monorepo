#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEFAULT_REPOSITORY="ghcr.io/proflyder/windows-logon-provider"
REPOSITORY="${OCI_REPOSITORY:-$DEFAULT_REPOSITORY}"
VERSION_FILE="${VERSION_FILE:-$HERE/VERSION}"
SOURCE_DIR=""
TAG=""
VERSION_OVERRIDE=""
REPLACE=0
OUTPUT_SUMMARY="${GITHUB_STEP_SUMMARY:-}"

usage() {
  echo "usage: $0 --source-dir DIR [--repository REPOSITORY] [--tag TAG] [--version VERSION] [--replace]" >&2
}

die() {
  echo "publish-oci: $*" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir)
      SOURCE_DIR="${2:?--source-dir needs a value}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:?--repository needs a value}"
      shift 2
      ;;
    --tag)
      TAG="${2:?--tag needs a value}"
      shift 2
      ;;
    --version)
      VERSION_OVERRIDE="${2:?--version needs a value}"
      shift 2
      ;;
    --replace)
      REPLACE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$SOURCE_DIR" ]] || { usage; die "--source-dir is required"; }
[[ -d "$SOURCE_DIR" ]] || die "source directory not found: $SOURCE_DIR"
command -v oras >/dev/null 2>&1 || die "oras CLI is required on PATH"

version="${VERSION_OVERRIDE:-$(tr -d '[:space:]' < "$VERSION_FILE")}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
  die "version must be semver MAJOR.MINOR.PATCH, got '$version'"

commit="${GITHUB_SHA:-}"
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "GITHUB_SHA must be a full commit id"
sha_tag="sha-${commit:0:12}"

file_path() {
  local name="$1"
  if [[ -f "$SOURCE_DIR/$name" ]]; then
    printf '%s\n' "$SOURCE_DIR/$name"
  elif [[ -f "$SOURCE_DIR/out/$name" ]]; then
    printf '%s\n' "$SOURCE_DIR/out/$name"
  else
    die "missing $name in $SOURCE_DIR"
  fi
}

file_path HomeKompLogon.dll >/dev/null
file_path cp-selftest.exe >/dev/null
CHECKSUMS="$(file_path SHA256SUMS)"

(
  cd "$(dirname "$CHECKSUMS")"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c --quiet SHA256SUMS
  else
    shasum -a 256 -c --quiet SHA256SUMS
  fi
)

repo_ref() {
  if [[ "$1" == sha256:* ]]; then
    printf '%s@%s\n' "$REPOSITORY" "$1"
  else
    printf '%s:%s\n' "$REPOSITORY" "$1"
  fi
}

owner="${REPOSITORY#*/}"
owner="${owner%%/*}"
ARTIFACT_TYPE="${OCI_ARTIFACT_TYPE:-application/vnd.${owner}.windows-logon-provider.v1}"
source_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-}"

check_existing_tag() {
  local tag="$1" ref tmp output
  ref="$(repo_ref "$tag")"
  if ! output="$(oras manifest fetch "$ref" 2>&1)"; then
    if grep -qiE 'not found|manifest_unknown|name unknown|404' <<< "$output"; then
      return 1
    fi
    die "cannot check tag $tag: $output"
  fi
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/logon-provider-tag.XXXXXX")"
  if ! oras pull "$ref" -o "$tmp" >/dev/null 2>&1; then
    rm -rf "$tmp"
    die "tag $tag exists but cannot be pulled; refusing to overwrite it"
  fi
  if [[ -f "$tmp/SHA256SUMS" ]] && cmp -s "$CHECKSUMS" "$tmp/SHA256SUMS"; then
    rm -rf "$tmp"
    return 0
  fi
  rm -rf "$tmp"
  if [[ "$REPLACE" -eq 1 ]]; then
    echo "publish-oci: replacing $tag with the rebuilt artifact"
    return 1
  fi
  die "tag $tag already exists with different content; the computed version must increase (or use --replace to rebuild it on purpose)"
}

push_artifact() {
  local destination="$1"
  # `cd` keeps the layer names bare (HomeKompLogon.dll, ...) and the working
  # directory outside the pushed file set.
  (
    cd "$(dirname "$CHECKSUMS")"
    oras push "$destination" \
      --artifact-type "$ARTIFACT_TYPE" \
      --annotation "org.opencontainers.image.title=windows-logon-provider" \
      --annotation "org.opencontainers.image.version=$version" \
      --annotation "org.opencontainers.image.revision=$commit" \
      --annotation "org.opencontainers.image.created=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
      --annotation "org.opencontainers.image.source=${source_url}" \
      --annotation "io.github.windows-logon-provider.toolchain=bazel-rules_rust-zig-x86_64-windows-gnu" \
      "HomeKompLogon.dll:application/vnd.microsoft.portable-executable" \
      "cp-selftest.exe:application/vnd.microsoft.portable-executable" \
      "SHA256SUMS:text/plain"
  )
}

published_tags=""
if [[ -n "$TAG" ]]; then
  # Build-once PR path: exactly one tag, no release-tag side effects.
  if check_existing_tag "$TAG"; then
    echo "tag $TAG is already published with identical content; not rebuilding"
  else
    push_artifact "$(repo_ref "$TAG")"
  fi
  digest="$(oras resolve "$(repo_ref "$TAG")")"
  published_tags="$TAG"
else
  version_exists=false
  sha_exists=false
  if check_existing_tag "$version"; then version_exists=true; fi
  if check_existing_tag "$sha_tag"; then sha_exists=true; fi

  if $version_exists; then
    digest="$(oras resolve "$(repo_ref "$version")")"
    if ! $sha_exists; then
      oras tag "$(repo_ref "$version")" "$sha_tag" >/dev/null
      echo "tagged existing $version as $sha_tag"
    fi
    echo "version $version is already published with identical content; not rebuilding"
  else
    push_artifact "$(repo_ref "$version")"
    digest="$(oras resolve "$(repo_ref "$version")")"
    if ! $sha_exists; then
      oras tag "$(repo_ref "$version")" "$sha_tag" >/dev/null
    fi
  fi
  published_tags="$version, $sha_tag"
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "repository=$REPOSITORY"
    echo "version=$version"
    echo "digest=$digest"
  } >> "$GITHUB_OUTPUT"
fi

printf 'published %s:%s\ndigest: %s\n' "$REPOSITORY" "$version" "$digest"
if [[ -n "$OUTPUT_SUMMARY" ]]; then
  {
    echo "### publish-native / windows-logon-provider"
    echo
    echo "- repository: \`$REPOSITORY\`"
    echo "- tags: \`$published_tags\`"
    echo "- artifact: \`$REPOSITORY@$digest\`"
    echo "- artifact type: \`$ARTIFACT_TYPE\`"
  } >> "$OUTPUT_SUMMARY"
fi
