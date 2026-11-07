#!/usr/bin/env bash
#MISE description="bazel test //... (every target in the workspace)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec bazel test //...
