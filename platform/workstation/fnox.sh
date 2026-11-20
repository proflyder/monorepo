#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -z "${VAULT_ADDR:-}" ]]; then
  # shellcheck source=platform/workstation/env.sh
  source "$root/platform/workstation/env.sh"
fi
: "${VAULT_ADDR:?no OpenBao address: run \`mise run secrets:login\` first}"
export VAULT_ADDR
exec fnox -c "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/fnox.toml" "$@"
