#!/usr/bin/env bash
#MISE description="Create a one-time secrets-gateway device enrollment code; code into a 0600 file"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/run.sh" "$ROOT/projects/home/kubernetes/secrets-gateway/device_enroll.py" "$@"
