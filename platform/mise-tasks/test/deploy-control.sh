#!/usr/bin/env bash
#MISE description="Mocked-kubectl scheduler deploy-control regression (no Bazel build needed)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec bash projects/home/kubernetes/scheduler/test_deploy_control.sh
