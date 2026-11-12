#!/usr/bin/env bash
#MISE description="Install the ha-config-sync forced-command agent and key on HAOS (idempotent; --dry-run)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/projects/home/kubernetes/ha-config-sync/bootstrap.sh" "$@"
