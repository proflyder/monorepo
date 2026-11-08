#!/usr/bin/env bash
#MISE description="Install/refresh the offsite restic backup on the K3s node (--dry-run, --run-now)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
export SSH_CONFIG="${SSH_CONFIG:-$HOME/.ssh/home-lab/config}"
exec "$ROOT/platform/workstation/fnox.sh" -P offsite exec -- "$ROOT/projects/home/backup/offsite/install.sh" "$@" k3s
