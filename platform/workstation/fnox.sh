#!/usr/bin/env bash
# Run fnox with the repository's fnox.toml from any directory (also from a
# linked worktree): `platform/workstation/fnox.sh -P <profile> exec -- <cmd>`.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -z "${VAULT_ADDR:-}" ]]; then
  # shellcheck source=platform/workstation/env.sh
  source "$root/platform/workstation/env.sh"
fi
: "${VAULT_ADDR:?no OpenBao address: run \`mise run secrets:login\` first}"
export VAULT_ADDR
exec fnox -c "$root/fnox.toml" "$@"
