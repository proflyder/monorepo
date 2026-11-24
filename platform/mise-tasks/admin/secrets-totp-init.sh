#!/usr/bin/env bash
#MISE description="Create the secrets-gateway TOTP secret in OpenBao; otpauth URI into a 0600 file"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/run.sh" "$ROOT/projects/home/kubernetes/secrets-gateway/totp_init.py" "$@"
