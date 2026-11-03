#!/usr/bin/env bash
# Host-side tests for the portable part of the credential provider.
#
# Builds protocol.cpp + tests/test_protocol.cpp with the host compiler, so the
# PROTOCOL.md vectors and every fail-closed branch run on macOS without a
# Windows toolchain. Run: ./run-tests.sh
set -euo pipefail

cd "$(dirname "$0")"

CXX="${CXX:-clang++}"
BUILD_DIR="build/protocol-tests"
mkdir -p "$BUILD_DIR"

"$CXX" -std=c++17 -O2 -g -Wall -Wextra -Wpedantic -Werror \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  src/protocol/protocol.cpp \
  src/protocol/hmac_sha256_reference.cpp \
  tests/test_protocol.cpp \
  -o "$BUILD_DIR/test_protocol"

"$BUILD_DIR/test_protocol"
