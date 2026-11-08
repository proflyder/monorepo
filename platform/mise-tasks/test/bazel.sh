#!/usr/bin/env bash
#MISE description="bazel test //... (every target in the workspace)"
# `//...` includes the Android targets, so the Android-only flags from
# platform/android/android.bazelrc (version defines, Java toolchain, dex
# workers, the APK ABI) must be active exactly as they were before the split.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec bazel test --config=android //...
