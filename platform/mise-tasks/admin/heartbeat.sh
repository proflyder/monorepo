#!/usr/bin/env bash
#MISE description="Deploy the Cloudflare heartbeat Worker (applies; --secrets pushes Worker secrets)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P heartbeat exec -- "$ROOT/projects/home/cloudflare/heartbeat/deploy.sh" "$@"
