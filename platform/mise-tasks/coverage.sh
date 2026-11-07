#!/usr/bin/env bash
#MISE description="Hermetic suite plus coverage; writes reports/TEST-COVERAGE.md (untracked)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/platform/quality/coverage.sh"
