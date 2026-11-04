#!/usr/bin/env bash
# Publish the station release as a public OCI artifact in GHCR with ORAS.
#
# Runs on a push to `develop` from deploy-home, never on a pull request. The
# caller supplies the release artifact downloaded from the `_native.yml`
# `pulsar` artifact, authenticates `oras` to GHCR (docker/login-action or
# `oras login`) and has `oras` on PATH.
#
#   ./publish-oci.sh --source-dir /path/to/artifact
#
# The station updater reads this exact contract, so the names are frozen:
#   repository  ghcr.io/proflyder/pulsar-firmware
#   tags        <VERSION> and :latest
#   type        application/vnd.proflyder.pulsar.release.v1
#   annotations org.opencontainers.image.version=<VERSION>
#               org.opencontainers.image.source=https://github.com/proflyder/monorepo
#   layers      pulsar-aarch64.bin, pulsar-armhf.bin, SHA256SUMS,
#               zigbee-coordinator.ybl, zigbee-router.ybl
#
# The version tag is immutable: an existing tag is a hard failure, never an
# overwrite. A re-deploy of the current VERSION is a deploy-home
# `workflow_dispatch` with units=pulsar, which stops after `deploy-station` and
# does not run this script.
#
# Step outputs (when GITHUB_OUTPUT is set): repository, version, digest.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DEFAULT_REPOSITORY="ghcr.io/proflyder/pulsar-firmware"
DEFAULT_SOURCE="https://github.com/proflyder/monorepo"
REPOSITORY="${OCI_REPOSITORY:-$DEFAULT_REPOSITORY}"
SOURCE_URL="${OCI_SOURCE_URL:-$DEFAULT_SOURCE}"
VERSION_FILE="${VERSION_FILE:-$HERE/VERSION}"
SOURCE_DIR=""
OUTPUT_SUMMARY="${GITHUB_STEP_SUMMARY:-}"

usage() {
  echo "usage: $0 --source-dir DIR [--repository REPOSITORY]" >&2
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
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
  die "VERSION must be vMAJOR.MINOR.PATCH, got '$version'"

# `_native.yml` uploads a flat artifact; accept the files either at the top
# level or inside a directory named after the unit.
file_path() {
  local name="$1"
  if [[ -f "$SOURCE_DIR/$name" ]]; then
    printf '%s\n' "$SOURCE_DIR/$name"
  elif [[ -f "$SOURCE_DIR/pulsar/$name" ]]; then
    printf '%s\n' "$SOURCE_DIR/pulsar/$name"
  else
    die "missing $name in $SOURCE_DIR"
  fi
}

files=(
  "$(file_path pulsar-aarch64.bin)"
  "$(file_path pulsar-armhf.bin)"
  "$(file_path SHA256SUMS)"
  "$(file_path zigbee-coordinator.ybl)"
  "$(file_path zigbee-router.ybl)"
)
stage="$(dirname "${files[0]}")"

# Verify every layer against the uploaded SHA256SUMS before publishing, so a
# broken artifact never reaches the registry.
(
  cd "$stage"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c --quiet SHA256SUMS
  else
    shasum -a 256 -c --quiet SHA256SUMS
  fi
)

# The version tag is immutable. Only a definitive "not found" means it is safe
# to publish; any other registry/auth error fails the job instead of risking an
# overwrite.
if output="$(oras manifest fetch "$REPOSITORY:$version" 2>&1)"; then
  die "$REPOSITORY:$version already exists; bump projects/station/pulsar/VERSION"
elif ! grep -qiE 'not found|manifest_unknown|name unknown|404' <<< "$output"; then
  die "cannot check $REPOSITORY:$version: $output"
fi

# `cd` keeps the layer titles bare (the updater finds them by exact name).
(
  cd "$stage"
  oras push "$REPOSITORY:$version" \
    --artifact-type "application/vnd.proflyder.pulsar.release.v1" \
    --annotation "org.opencontainers.image.version=$version" \
    --annotation "org.opencontainers.image.source=$SOURCE_URL" \
    pulsar-aarch64.bin pulsar-armhf.bin SHA256SUMS \
    zigbee-coordinator.ybl zigbee-router.ybl
)
digest="$(oras resolve "$REPOSITORY:$version")"
# The station updater reads :latest.
oras tag "$REPOSITORY:$version" latest >/dev/null

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
    echo "### publish-native / pulsar"
    echo
    echo "- repository: \`$REPOSITORY\`"
    echo "- version: \`$version\` (and \`:latest\`)"
    echo "- artifact: \`$REPOSITORY@$digest\`"
    echo "- layers: \`pulsar-aarch64.bin\`, \`pulsar-armhf.bin\`, \`SHA256SUMS\`, \`zigbee-*.ybl\`"
  } >> "$OUTPUT_SUMMARY"
fi
