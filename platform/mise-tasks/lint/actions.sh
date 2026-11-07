#!/usr/bin/env bash
#MISE description="actionlint over every GitHub Actions workflow"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec actionlint -color .github/workflows/*.yml
