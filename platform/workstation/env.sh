# shellcheck shell=bash
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for candidate in "$root/secrets/openbao/addr" \
  "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null | sed 's#/.git$##')/secrets/openbao/addr"; do
  if [ -f "$candidate" ]; then
    BAO_ADDR="$(cat "$candidate")"
    VAULT_ADDR="$BAO_ADDR"
    export BAO_ADDR VAULT_ADDR
    break
  fi
done
unset root candidate

if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
  for ndk_candidate in "$ANDROID_HOME"/ndk/*/; do
    [ -d "$ndk_candidate" ] && ANDROID_NDK_HOME="${ndk_candidate%/}"
  done
  [ -n "${ANDROID_NDK_HOME:-}" ] && export ANDROID_NDK_HOME
fi
unset ndk_candidate
