#!/usr/bin/env bash
#MISE description="No comments in code: every tracked source file (platform/quality/no_comments.mjs)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec node platform/quality/no_comments.mjs "$@"
