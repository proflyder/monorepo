#!/usr/bin/env bash
#MISE description="Install PVE vzdump metrics on the host (no secrets; --dry-run)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
export SSH_CONFIG="${SSH_CONFIG:-$HOME/.ssh/home-lab/config}"
exec "$ROOT/projects/home/backup/pve/install.sh" "$@" pve
