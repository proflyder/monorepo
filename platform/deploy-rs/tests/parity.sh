#!/bin/sh
# Parity gate: run the Rust binary over the frozen Go contract fixtures.
#
# Bazel passes absolute runfiles paths for the runner, the binary and the
# fixture root; the runner exits non-zero on the first case that differs.
set -eu

FROZEN=$(dirname "$3")
exec "$1" "$2" "$FROZEN"
