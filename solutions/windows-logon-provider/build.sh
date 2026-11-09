#!/usr/bin/env bash
# Cross-build the Windows credential provider with Bazel on macOS or Linux.
#
# rules_rust targets x86_64-pc-windows-gnu and links the hermetic zig cc
# toolchain from MODULE.bazel; no Visual Studio or Windows SDK is involved.
# PE timestamps are zeroed so identical sources give identical bytes, then
# pe_artifacts.py checks the export table and the import allowlist.
set -euo pipefail

cd "$(dirname "$0")"

ROOT="$(cd ../.. && pwd)"
WINDOWS_PLATFORM="//platform/bazel/windows:x86_64_windows"

# Bazelisk comes from mise (`mise install`, see mise.toml).
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  if type -P bazel >/dev/null 2>&1; then
    type -P bazel
    return 0
  fi
  if type -P bazelisk >/dev/null 2>&1; then
    type -P bazelisk
    return 0
  fi
  echo "bazel/bazelisk not found: run \`mise install\` or set BAZELISK" >&2
  return 1
}

BAZEL="$(bazel_bin)"

echo "== bazel =="
"$BAZEL" --version

echo "== windows cross-build =="
"$BAZEL" build -c opt --platforms="$WINDOWS_PLATFORM" \
  //solutions/windows-logon-provider/crates/provider:provider \
  //solutions/windows-logon-provider/crates/cp-selftest:cp-selftest

mkdir -p out
cp "$ROOT/bazel-bin/solutions/windows-logon-provider/crates/provider/HomeKompLogon.dll" out/
cp "$ROOT/bazel-bin/solutions/windows-logon-provider/crates/cp-selftest/cp-selftest.exe" out/

echo "== reproducible PE =="
python3 pe_artifacts.py normalize out/HomeKompLogon.dll out/cp-selftest.exe

echo "== verification =="
python3 pe_artifacts.py verify out/HomeKompLogon.dll

# `_native.yml` regenerates this from the uploaded files; producing it locally
# too keeps `out/` directly publishable with publish-oci.sh.
echo "== checksums =="
if command -v sha256sum >/dev/null 2>&1; then
  (cd out && sha256sum HomeKompLogon.dll cp-selftest.exe > SHA256SUMS)
else
  (cd out && shasum -a 256 HomeKompLogon.dll cp-selftest.exe > SHA256SUMS)
fi
cat out/SHA256SUMS

printf '\n'
ls -l out/HomeKompLogon.dll out/cp-selftest.exe out/SHA256SUMS
