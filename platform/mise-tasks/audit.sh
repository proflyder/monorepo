#!/usr/bin/env bash
#MISE description="Repository audit: private data, links, gitlinks"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
exec node platform/quality/audit_repository.mjs
