#!/usr/bin/env bash
#MISE description="Pre-push gate: hermetic tests, repository audit, Gitleaks"
#MISE depends=["test"]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
node platform/quality/audit_repository.mjs
gitleaks dir . --redact=100 --no-banner --config=.gitleaks.toml
