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

# A linked worktree has no (git-ignored) secrets/ directory, so fall back to the
# main checkout that owns the shared Git directory.
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  if type -P bazel >/dev/null 2>&1; then
    type -P bazel
    return 0
  fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then
    printf '%s\n' "$ROOT/secrets/local-tools/bazelisk"
    return 0
  fi
  local common main_root
  common=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || common=
  if [[ -n "$common" ]]; then
    main_root=$(dirname "$common")
    if [[ -x "$main_root/secrets/local-tools/bazelisk" ]]; then
      printf '%s\n' "$main_root/secrets/local-tools/bazelisk"
      return 0
    fi
  fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
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
cp "$ROOT/bazel-bin/solutions/windows-logon-provider/crates/provider/RicardoKompLogon.dll" out/
cp "$ROOT/bazel-bin/solutions/windows-logon-provider/crates/cp-selftest/cp-selftest.exe" out/

echo "== reproducible PE =="
python3 pe_artifacts.py normalize out/RicardoKompLogon.dll out/cp-selftest.exe

echo "== verification =="
python3 pe_artifacts.py verify out/RicardoKompLogon.dll

# `_native.yml` regenerates this from the uploaded files; producing it locally
# too keeps `out/` directly publishable with publish-oci.sh.
echo "== checksums =="
if command -v sha256sum >/dev/null 2>&1; then
  (cd out && sha256sum RicardoKompLogon.dll cp-selftest.exe > SHA256SUMS)
else
  (cd out && shasum -a 256 RicardoKompLogon.dll cp-selftest.exe > SHA256SUMS)
fi
cat out/SHA256SUMS

printf '\n'
ls -l out/RicardoKompLogon.dll out/cp-selftest.exe out/SHA256SUMS
