#!/usr/bin/env bash
#MISE description="actionlint over every GitHub Actions workflow"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec actionlint -color \
  -ignore 'unexpected key "background" for step to' \
  -ignore 'step must run script with "run" section or run action with "uses" section' \
  .github/workflows/*.yml
