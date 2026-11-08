#!/usr/bin/env bash
#MISE description="Verify HTTPS routes, allow-list and tokens (read-only)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P ingress exec -- "$ROOT/projects/home/kubernetes/ingress/verify.sh" "$@"
