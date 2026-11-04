#!/usr/bin/env bash
# Deploy entry point for the `hermes` unit (deploy-home.yml matrix `script`).
#
# The rollout itself is the Go `home-deploy hermes rollout` subcommand
# (decision 35). This wrapper only resolves the binary from the checkout, so the
# workflow contract (`${{ matrix.script }}` + argv) stays unchanged and a manual
# operator run needs no Python.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"

home_deploy hermes rollout "$@"
