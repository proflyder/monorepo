#!/usr/bin/env bash
#MISE description="Bootstrap the owner app token and service token in OpenBao (prints the owner token once)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/run.sh" "$ROOT/platform/workstation/app_tokens.py" "$@"
