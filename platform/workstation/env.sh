# shellcheck shell=bash
# Sourced by mise ([env] _.source): exports the non-secret OpenBao address so the
# `bao` CLI works without flags. The token is never exported into the shell.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for candidate in "$root/secrets/openbao/addr" \
  "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null | sed 's#/.git$##')/secrets/openbao/addr"; do
  if [ -f "$candidate" ]; then
    BAO_ADDR="$(cat "$candidate")"
    # fnox's vault provider (and the `vault` → `bao` shim) read VAULT_ADDR.
    VAULT_ADDR="$BAO_ADDR"
    export BAO_ADDR VAULT_ADDR
    break
  fi
done
unset root candidate

# rules_android_ndk resolves the NDK from ANDROID_NDK_HOME. Point it at the
# newest NDK installed under $ANDROID_HOME so `mise exec -- bazel` builds the
# Android app without a second export. CI sets it explicitly after
# `sdkmanager --install "ndk;<version>"`.
if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
  for ndk_candidate in "$ANDROID_HOME"/ndk/*/; do
    [ -d "$ndk_candidate" ] && ANDROID_NDK_HOME="${ndk_candidate%/}"
  done
  [ -n "${ANDROID_NDK_HOME:-}" ] && export ANDROID_NDK_HOME
fi
unset ndk_candidate
