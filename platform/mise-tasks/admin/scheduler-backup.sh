#!/usr/bin/env bash
#MISE description="Consistent scheduler SQLite backup into local-data (stops writers briefly)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/scheduler/backup.sh" "$@"
