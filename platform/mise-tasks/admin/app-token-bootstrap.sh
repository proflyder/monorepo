#!/usr/bin/env bash
#MISE description="Issue the owner app token (0600 file), sync ESO, restart and verify scheduler; --rotate-service also rotates the service token"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P kube exec -- \
  "$ROOT/platform/workstation/run.sh" "$ROOT/platform/workstation/app_tokens.py" "$@"
