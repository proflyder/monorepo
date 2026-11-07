# shellcheck shell=bash
# Sourced by mise ([env] _.source): exports the non-secret OpenBao address so the
# `bao` CLI works without flags. The token is never exported into the shell.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for candidate in "$root/secrets/openbao/addr" \
  "$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null | sed 's#/.git$##')/secrets/openbao/addr"; do
  if [ -f "$candidate" ]; then
    BAO_ADDR="$(cat "$candidate")"
    export BAO_ADDR
    break
  fi
done
unset root candidate
