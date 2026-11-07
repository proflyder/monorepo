#!/usr/bin/env bash
#MISE description="Gitleaks working-tree scan (config and rules in the repository)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec gitleaks dir . --redact=100 --no-banner --config=.gitleaks.toml
