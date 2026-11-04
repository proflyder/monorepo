#!/usr/bin/env bash
# Publish the built provider as a public OCI artifact in GHCR with ORAS.
#
# Runs on a push to `develop` from deploy-home, never on a pull request. The
# caller supplies the build output downloaded from the `_native.yml` artifact,
# authenticates `oras` to GHCR (docker/login-action or `oras login`) and has
# `oras` on PATH.
#
#   ./publish-oci.sh --source-dir /path/to/artifact
#   ./publish-oci.sh --source-dir /path/to/artifact --tag tree-<git tree hash>
#
# Repository and tags:
#   ghcr.io/<owner>/windows-logon-provider:<VERSION>
#   ghcr.io/<owner>/windows-logon-provider:sha-<12 hex of GITHUB_SHA>
#
# `--tag` publishes the same artifact under exactly one tag (the PR build-once
# path uses `tree-<git tree hash>`); it does not touch the release tags. That
# path is idempotent for identical content and refuses different content.
#
# A tag is immutable by content: if `<VERSION>` already exists with the same
# SHA256SUMS this is a successful no-op; different content is a hard failure so
# a release bump is explicit. The same rule protects the commit tag.
#
# Step outputs (when GITHUB_OUTPUT is set): repository, version, digest.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEFAULT_REPOSITORY="ghcr.io/proflyder/windows-logon-provider"
REPOSITORY="${OCI_REPOSITORY:-$DEFAULT_REPOSITORY}"
VERSION_FILE="${VERSION_FILE:-$HERE/VERSION}"
SOURCE_DIR=""
TAG=""
OUTPUT_SUMMARY="${GITHUB_STEP_SUMMARY:-}"

usage() {
  echo "usage: $0 --source-dir DIR [--repository REPOSITORY] [--tag TAG]" >&2
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

version="$(tr -d '[:space:]' < "$VERSION_FILE")"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
  die "VERSION must be semver MAJOR.MINOR.PATCH, got '$version'"

commit="${GITHUB_SHA:-}"
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "GITHUB_SHA must be a full commit id"
sha_tag="sha-${commit:0:12}"

# The artifact layout from `_native.yml` is flat (out/ is the least common
# ancestor of the uploaded paths); accept an explicit out/ subdirectory too so a
# local `out/` directory can be passed directly.
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

file_path RicardoKompLogon.dll >/dev/null
file_path cp-selftest.exe >/dev/null
file_path hmac-selftest.exe >/dev/null
CHECKSUMS="$(file_path SHA256SUMS)"

# Verify the checksums before publishing, so a broken artifact never reaches
# the registry.
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

# The artifact type names this product, but its owner is taken from the
# registry path instead of being duplicated in the source tree.
owner="${REPOSITORY#*/}"
owner="${owner%%/*}"
ARTIFACT_TYPE="${OCI_ARTIFACT_TYPE:-application/vnd.${owner}.windows-logon-provider.v1}"
source_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-}"

# Returns 0 when `tag` exists with identical SHA256SUMS, 1 when it does not
# exist, and fails when it exists with different content.
check_existing_tag() {
  local tag="$1" ref tmp output
  ref="$(repo_ref "$tag")"
  if ! output="$(oras manifest fetch "$ref" 2>&1)"; then
    # Only a definitively missing tag means "safe to publish". Any other
    # failure (auth, network, registry error) must not silently fall through
    # to a push that could overwrite an existing version tag.
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
  die "tag $tag already exists with different content; bump VERSION to release"
}

push_artifact() {
  local destination="$1"
  # `cd` keeps the layer names bare (RicardoKompLogon.dll, ...) and the working
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
      --annotation "io.github.windows-logon-provider.toolchain=llvm-mingw-20261020" \
      "RicardoKompLogon.dll:application/vnd.microsoft.portable-executable" \
      "cp-selftest.exe:application/vnd.microsoft.portable-executable" \
      "hmac-selftest.exe:application/vnd.microsoft.portable-executable" \
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
