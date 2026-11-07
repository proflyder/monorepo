#!/usr/bin/env bash
#MISE description="CI routing, gate and audit unit tests (node --test)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec node --test platform/ci/*.test.mjs platform/quality/*.test.mjs platform/mise-tasks/lib/*.test.mjs
