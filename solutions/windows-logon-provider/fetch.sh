#!/usr/bin/env bash
set -euo pipefail

DEFAULT_REPOSITORY="ghcr.io/proflyder/windows-logon-provider"
REPOSITORY="${OCI_REPOSITORY:-$DEFAULT_REPOSITORY}"

die() {
  echo "fetch: $*" >&2
  exit 1
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <version|sha256:...> [DESTINATION]" >&2
  exit 2
fi

ref="$1"
destination="${2:-$(mktemp -d "${TMPDIR:-/tmp}/windows-logon-provider.XXXXXX")}"

case "$ref" in
  sha256:*) target="$REPOSITORY@$ref" ;;
  *) target="$REPOSITORY:$ref" ;;
esac

command -v oras >/dev/null 2>&1 || die "oras CLI is required (`mise install` provides it)"
command -v gh >/dev/null 2>&1 || die "GitHub CLI (gh) is required"

mkdir -p "$destination"
oras pull "$target" -o "$destination"

if [[ ! -f "$destination/SHA256SUMS" ]]; then
  die "artifact has no SHA256SUMS"
fi
(
  cd "$destination"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c --quiet SHA256SUMS
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c --quiet SHA256SUMS
  else
    die "sha256sum or shasum is required"
  fi
)

owner="${REPOSITORY#*/}"
owner="${owner%%/*}"
gh attestation verify "$destination/HomeKompLogon.dll" --owner "$owner"

if [[ "$ref" == sha256:* ]]; then
  resolved="$ref"
else
  resolved="$(oras resolve "$target")"
fi
printf 'verified files in %s\nresolved digest: %s\n' "$destination" "$resolved"
