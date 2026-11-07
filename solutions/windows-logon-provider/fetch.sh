#!/usr/bin/env bash
# Fetch and verify a published provider artifact on the operator's Mac/Linux.
#
#   ./fetch.sh <version|sha256:...> [DESTINATION]
#
# Examples:
#   ./fetch.sh 1.0.0
#   ./fetch.sh sha256:0123...            # digest pin, the recommended form
#
# Requires `oras` and an authenticated `gh` (GitHub CLI). The reference is
# pulled with ORAS, SHA256SUMS must verify locally, and `gh attestation verify`
# must confirm the build provenance of RicardoKompLogon.dll. Any mismatch,
# missing tool or failed attestation aborts with a non-zero status. The resolved
# OCI digest is printed at the end and the unpacked files are kept in
# DESTINATION (a mktemp directory when omitted).
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

# The attestation owner is the registry namespace from the repository path, so
# the owner is not duplicated as a literal in this script.
owner="${REPOSITORY#*/}"
owner="${owner%%/*}"
gh attestation verify "$destination/RicardoKompLogon.dll" --owner "$owner"

if [[ "$ref" == sha256:* ]]; then
  resolved="$ref"
else
  resolved="$(oras resolve "$target")"
fi
printf 'verified files in %s\nresolved digest: %s\n' "$destination" "$resolved"
