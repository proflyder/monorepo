#!/usr/bin/env bash
# Cross-build the Windows credential provider on macOS with llvm-mingw.
#
# Toolchain (git-ignored): secrets/local-tools/llvm-mingw/, release 20261020,
# asset llvm-mingw-20261020-ucrt-macos-universal.tar.xz,
# sha256 e9c13e4ede38e715003ffc674d4d00a1088df8235439d1f0c16188ff0704c17e.
# Override the toolchain root with LLVM_MINGW=/path/to/llvm-mingw (the
# directory that contains bin/) or the bin directory directly with
# LLVM_MINGW_BIN=/path/to/bin. CI extracts the Linux x86_64 release and sets
# LLVM_MINGW, so the same script builds on macOS and on the runner.
#
# Outputs: out/RicardoKompLogon.dll, out/cp-selftest.exe, out/hmac-selftest.exe
set -euo pipefail

cd "$(dirname "$0")"

ROOT="$(cd ../.. && pwd)"
VERSION="${LLVM_MINGW_VERSION:-20261020}"

find_toolchain() {
  if [[ -n "${LLVM_MINGW:-}" ]]; then
    local root="${LLVM_MINGW%/}"
    if [[ -x "$root/bin/x86_64-w64-mingw32-clang++" ]]; then
      printf '%s\n' "$root/bin"
      return 0
    fi
    if [[ -x "$root/x86_64-w64-mingw32-clang++" ]]; then
      printf '%s\n' "$root"
      return 0
    fi
    echo "LLVM_MINGW=$LLVM_MINGW does not contain x86_64-w64-mingw32-clang++" >&2
    return 1
  fi
  if [[ -n "${LLVM_MINGW_BIN:-}" ]]; then
    printf '%s\n' "$LLVM_MINGW_BIN"
    return 0
  fi
  local candidate
  candidate="$ROOT/secrets/local-tools/llvm-mingw/llvm-mingw-${VERSION}-ucrt-macos-universal/bin"
  if [[ -x "$candidate/x86_64-w64-mingw32-clang++" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi
  local root="$ROOT/secrets/local-tools/llvm-mingw"
  if [[ -d "$root" ]]; then
    candidate="$(find "$root" -maxdepth 3 -type f -name x86_64-w64-mingw32-clang++ -print -quit)"
    if [[ -n "$candidate" ]]; then
      dirname "$candidate"
      return 0
    fi
  fi
  if command -v x86_64-w64-mingw32-clang++ >/dev/null 2>&1; then
    dirname "$(command -v x86_64-w64-mingw32-clang++)"
    return 0
  fi
  return 1
}

if ! TOOLCHAIN_BIN="$(find_toolchain)"; then
  echo "llvm-mingw not found." >&2
  echo "Install ${VERSION} under secrets/local-tools/llvm-mingw/ or set LLVM_MINGW/LLVM_MINGW_BIN." >&2
  exit 1
fi

CXX="$TOOLCHAIN_BIN/x86_64-w64-mingw32-clang++"
OBJDUMP="$TOOLCHAIN_BIN/llvm-objdump"
[[ -x "$CXX" ]] || { echo "missing $CXX" >&2; exit 1; }

mkdir -p out

COMMON_FLAGS=(
  -std=c++17
  -O2
  -DUNICODE
  -D_UNICODE
  -DWIN32_LEAN_AND_MEAN
  -D_WIN32_WINNT=0x0A00
  -DWINVER=0x0A00
  -fno-strict-aliasing
  -Wall
  -Wextra
  -Wno-unknown-pragmas
  -static
  -static-libgcc
)

LINK_FLAGS=(
  # Reproducible output: no link-time TimeDateStamp in the PE header and the
  # export table, so the same sources always produce the same bytes. The OCI
  # publisher relies on this (same VERSION + different bytes = hard fail).
  -Wl,--no-insert-timestamp
  -Wl,--dynamicbase
  -Wl,--nxcompat
  -Wl,--high-entropy-va
)

LIBRARIES=(
  -lwinhttp
  -lcrypt32
  -lbcrypt
  -lsecur32
  -lcredui
  -lole32
  -luuid
  -ladvapi32
  -lshlwapi
  -luser32
  -lshell32
)

echo "== toolchain =="
"$CXX" --version | head -n 1
echo "   bin: $TOOLCHAIN_BIN"

echo "== DLL =="
"$CXX" "${COMMON_FLAGS[@]}" "${LINK_FLAGS[@]}" -shared \
  -o out/RicardoKompLogon.dll \
  src/dll.cpp \
  src/provider.cpp \
  src/credential.cpp \
  src/worker.cpp \
  src/config.cpp \
  src/http.cpp \
  src/log.cpp \
  src/helpers.cpp \
  src/hmac_bcrypt.cpp \
  src/protocol/protocol.cpp \
  src/ricardo_logon.def \
  "${LIBRARIES[@]}"

echo "== self-test executable =="
"$CXX" "${COMMON_FLAGS[@]}" "${LINK_FLAGS[@]}" \
  -I src \
  -o out/cp-selftest.exe \
  src/selftest/cp_selftest.cpp \
  "${LIBRARIES[@]}"

echo "== CNG HMAC self-test executable =="
"$CXX" "${COMMON_FLAGS[@]}" "${LINK_FLAGS[@]}" \
  -I src \
  -o out/hmac-selftest.exe \
  src/selftest/hmac_selftest.cpp \
  src/hmac_bcrypt.cpp \
  src/protocol/protocol.cpp \
  "${LIBRARIES[@]}"

echo "== verification =="
"$OBJDUMP" -p out/RicardoKompLogon.dll > out/RicardoKompLogon.objdump.txt
"$OBJDUMP" -p out/cp-selftest.exe > out/cp-selftest.objdump.txt

for symbol in DllGetClassObject DllCanUnloadNow; do
  if ! grep -qE "[[:space:]]+[0-9]+[[:space:]]+0x[0-9a-f]+[[:space:]]+${symbol}\$" out/RicardoKompLogon.objdump.txt; then
    echo "missing export: $symbol" >&2
    exit 1
  fi
done

allowed_dlls="KERNEL32.dll ADVAPI32.dll bcrypt.dll crypt32.dll credui.dll secur32.dll shlwapi.dll ole32.dll USER32.dll WINHTTP.dll RPCRT4.dll ucrtbase.dll ntdll.dll"
unexpected="$(grep -o 'DLL Name: .*' out/RicardoKompLogon.objdump.txt | sed 's/DLL Name: //' |
  grep -viE '^(api-ms-win-[a-z0-9-]+\.dll|'"$(echo "$allowed_dlls" | tr ' ' '|')"')$' || true)"
if [[ -n "$unexpected" ]]; then
  echo "unexpected DLL imports:" >&2
  echo "$unexpected" >&2
  echo "static CRT/libc++ expected; check -static and -static-libgcc" >&2
  exit 1
fi

printf '\n'
ls -l out/RicardoKompLogon.dll out/cp-selftest.exe
echo "imports:"
grep -o 'DLL Name: .*' out/RicardoKompLogon.objdump.txt | sed 's/DLL Name: /  /' | sort -u
echo "exports:"
sed -n '/^Export Table:/,/^$/p' out/RicardoKompLogon.objdump.txt | tail -n +2
