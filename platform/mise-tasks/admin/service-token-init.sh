#!/usr/bin/env bash
#MISE description="Create the scheduler service_token in OpenBao if missing (idempotent, prints nothing)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/run.sh" "$ROOT/platform/workstation/app_tokens.py" --init-service-token "$@"
