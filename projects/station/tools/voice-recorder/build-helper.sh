#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
# Pass ZIG explicitly if it is not installed in this project's local Python environment.
ZIG="${ZIG:-$ROOT/.venv/lib/python3.14/site-packages/ziglang/zig}"
[[ -x "$ZIG" ]] || { echo 'Set ZIG to zig 0.14.1' >&2; exit 1; }
export CARGO_ZIGBUILD_ZIG_PATH="$ZIG" CARGO_ZIGBUILD_PYTHON_PATH=/nonexistent/python3
export CARGO_TARGET_DIR="$ROOT/secrets/local-tools/voice-recorder-target"
export RUSTFLAGS='-C relocation-model=static -C target-cpu=cortex-a53'
cargo zigbuild --manifest-path "$HERE/helper/Cargo.toml" --locked \
  --target armv7-unknown-linux-gnueabihf.2.30 --release
