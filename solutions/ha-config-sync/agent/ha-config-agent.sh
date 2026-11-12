#!/bin/sh
# Forced-command agent for ha-config-sync on the Home Assistant OS host.
#
# The SSH key that reaches this script is restricted in authorized_keys:
#   restrict,command="HA_CONFIG_AGENT_ALLOWED_FROM=<node ip> /mnt/data/ha-config-sync/agent.sh"
# so the client cannot run a shell or forward anything. HAOS dropbear does not
# support an authorized_keys host restriction, so the source address is
# enforced here instead: when
# HA_CONFIG_AGENT_ALLOWED_FROM (space/comma separated addresses) is set, the
# client address from $SSH_CLIENT (or $SSH_CONNECTION) must match. The requested
# verb arrives in $SSH_ORIGINAL_COMMAND; stdin carries the request payload.
#
# Verbs (protocol 1):
#   version                -> "ha-config-agent 1"
#   hashes                 <- newline-separated relative paths
#                          -> "<sha256|MISSING> <path>" per line
#   apply                  <- "<path> <sha256> <base64 content>" per line
#                          -> backup id
#   restore <backup id>    -> empty
#   prune <keep>           -> empty
#
# A failure prints "ERROR <reason>" on stderr and exits non-zero. The roots are
# fixed for production and overridable only to run the tests locally.

set -eu
# No pathname expansion: a verb argument like `*` must stay a literal and be
# rejected by validation, never expand to files in the agent's cwd.
set -f

PROTOCOL_VERSION=1
AGENT_NAME=ha-config-agent
ROOT=${HA_CONFIG_AGENT_ROOT:-/mnt/data/supervisor/homeassistant}
STATE=${HA_CONFIG_AGENT_STATE:-/mnt/data/ha-config-sync}
STAGING=$STATE/staging
BACKUPS=$STATE/backups
LOCK=$STATE/lock
MANAGED_PATH_PATTERN='^(custom_sentences/[a-z]{2}/)?[a-z0-9_]+\.yaml$'
ALLOWED_FROM=${HA_CONFIG_AGENT_ALLOWED_FROM:-}

fail() {
  printf 'ERROR %s\n' "$*" >&2
  exit 1
}

# HAOS dropbear has no host-restriction option; enforce the source here.
check_source() {
  [ -n "$ALLOWED_FROM" ] || return 0
  client=${SSH_CLIENT:-${SSH_CONNECTION:-}}
  source=${client%% *}
  [ -n "$source" ] || fail "source not allowed"
  for allowed in $(printf '%s' "$ALLOWED_FROM" | tr ',' ' '); do
    if [ "$source" = "$allowed" ]; then
      return 0
    fi
  done
  fail "source not allowed"
}

valid_path() {
  case "$1" in
    configuration.yaml|secrets.yaml|scenes.yaml) return 1 ;;
    '') return 1 ;;
  esac
  printf '%s\n' "$1" | grep -Eq "$MANAGED_PATH_PATTERN"
}

# Reject an escaping path, a symlink anywhere on the way and a non-file target.
check_target() {
  path=$1
  valid_path "$path" || return 1
  case "$path" in
    /*) return 1 ;;
    *..*) return 1 ;;
  esac
  current=$ROOT
  old_ifs=$IFS
  IFS=/
  # shellcheck disable=SC2086
  set -- $path
  IFS=$old_ifs
  total=$#
  index=0
  for component in "$@"; do
    index=$((index + 1))
    current=$current/$component
    if [ -L "$current" ]; then
      return 1
    fi
    if [ "$index" -lt "$total" ]; then
      if [ -e "$current" ] && [ ! -d "$current" ]; then
        return 1
      fi
    elif [ -e "$current" ] && [ ! -f "$current" ]; then
      return 1
    fi
  done
  return 0
}

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

decode_base64() {
  # BusyBox and GNU accept -d. BSD base64 (macOS) accepts -D; try both.
  if base64 -d >"$2" 2>/dev/null <<EOF
$1
EOF
  then
    return 0
  fi
  base64 -D >"$2" 2>/dev/null <<EOF
$1
EOF
}

acquire_lock() {
  mkdir -p "$STATE"
  if command -v flock >/dev/null 2>&1; then
    exec 9>"$LOCK"
    flock 9
    return 0
  fi
  # Portable fallback for development hosts without flock (the HAOS host has
  # busybox flock). mkdir is atomic.
  attempts=0
  while ! mkdir "$LOCK.d" 2>/dev/null; do
    attempts=$((attempts + 1))
    if [ "$attempts" -gt 100 ]; then
      fail "lock timeout"
    fi
    sleep 1
  done
  trap 'rmdir "$LOCK.d" 2>/dev/null || true' EXIT HUP INT TERM
}

apply_files() {
  mkdir -p "$STAGING"
  work=$STAGING/apply.$$.$(date -u +%Y%m%dT%H%M%SZ)
  mkdir -p "$work/files"
  : >"$work/paths"
  while IFS= read -r line || [ -n "$line" ]; do
    if [ -z "$line" ]; then
      continue
    fi
    path=${line%% *}
    rest=${line#* }
    if [ "$rest" = "$line" ]; then
      fail "malformed apply line"
    fi
    hash=${rest%% *}
    encoded=${rest#* }
    if [ "$encoded" = "$rest" ]; then
      fail "malformed apply line"
    fi
    check_target "$path" || fail "invalid path $path"
    case "$hash" in
      ''|*[!0-9a-f]*) fail "invalid sha256 for $path" ;;
    esac
    [ ${#hash} -eq 64 ] || fail "invalid sha256 for $path"
    if grep -qxF "$path" "$work/paths"; then
      fail "duplicate path $path"
    fi
    mkdir -p "$work/files/$(dirname "$path")"
    decode_base64 "$encoded" "$work/files/$path" || fail "invalid base64 for $path"
    actual=$(hash_file "$work/files/$path")
    [ "$actual" = "$hash" ] || fail "sha256 mismatch for $path"
    printf '%s\n' "$path" >>"$work/paths"
  done
  [ -s "$work/paths" ] || fail "empty apply"

  mkdir -p "$BACKUPS"
  id=$(date -u +%Y%m%dT%H%M%SZ)
  candidate=$id
  counter=1
  while [ -e "$BACKUPS/$candidate" ]; do
    counter=$((counter + 1))
    candidate=$id-$counter
  done
  backup=$BACKUPS/$candidate
  mkdir -p "$backup/files"
  : >"$backup/paths"
  : >"$backup/absent"
  while IFS= read -r path; do
    printf '%s\n' "$path" >>"$backup/paths"
    target=$ROOT/$path
    if [ -f "$target" ]; then
      mkdir -p "$backup/files/$(dirname "$path")"
      cp -p "$target" "$backup/files/$path"
    else
      printf '%s\n' "$path" >>"$backup/absent"
    fi
  done <"$work/paths"

  while IFS= read -r path; do
    target=$ROOT/$path
    mkdir -p "$(dirname "$target")"
    if ! mv "$work/files/$path" "$target"; then
      restore_backup "$candidate" || true
      fail "move failed for $path"
    fi
  done <"$work/paths"
  rm -rf "$work"
  printf '%s\n' "$candidate"
}

restore_backup() {
  id=$1
  printf '%s\n' "$id" | grep -Eq '^[0-9]{8}T[0-9]{6}Z(-[0-9]+)?$' || fail "invalid backup id"
  backup=$BACKUPS/$id
  [ -d "$backup" ] || fail "unknown backup $id"
  mkdir -p "$STAGING"
  work=$STAGING/restore.$$.$(date -u +%Y%m%dT%H%M%SZ)
  mkdir -p "$work"
  while IFS= read -r path; do
    if [ -z "$path" ]; then
      continue
    fi
    check_target "$path" || fail "invalid path in backup $path"
    target=$ROOT/$path
    if [ -f "$backup/files/$path" ]; then
      mkdir -p "$work/$(dirname "$path")"
      cp -p "$backup/files/$path" "$work/$path"
      mkdir -p "$(dirname "$target")"
      mv "$work/$path" "$target"
    else
      rm -f "$target"
    fi
  done <"$backup/paths"
  rm -rf "$work"
}

prune_backups() {
  keep=$1
  case "$keep" in
    ''|*[!0-9]*) fail "invalid keep count" ;;
  esac
  [ -d "$BACKUPS" ] || return 0
  count=0
  # shellcheck disable=SC2012 # backup ids are generated, never arbitrary names
  for entry in $(ls -1 "$BACKUPS" 2>/dev/null | sort -r); do
    count=$((count + 1))
    if [ "$count" -gt "$keep" ]; then
      case "$entry" in
        ''|*[!0-9A-Za-z-]*) continue ;;
      esac
      rm -rf "${BACKUPS:?}/$entry"
    fi
  done
}

list_hashes() {
  while IFS= read -r path || [ -n "$path" ]; do
    if [ -z "$path" ]; then
      continue
    fi
    check_target "$path" || fail "invalid path $path"
    target=$ROOT/$path
    if [ -f "$target" ]; then
      printf '%s %s\n' "$(hash_file "$target")" "$path"
    else
      printf 'MISSING %s\n' "$path"
    fi
  done
}

requested=${SSH_ORIGINAL_COMMAND:-}
verb=
arg=
if [ -n "$requested" ]; then
  # shellcheck disable=SC2086
  set -- $requested
  verb=$1
  arg=${2:-}
  [ "$#" -le 2 ] || fail "too many arguments"
fi

check_source
acquire_lock

case "$verb" in
  version)
    [ -z "$arg" ] || fail "version takes no arguments"
    printf '%s %s\n' "$AGENT_NAME" "$PROTOCOL_VERSION"
    ;;
  hashes)
    [ -z "$arg" ] || fail "hashes takes no arguments"
    list_hashes
    ;;
  apply)
    [ -z "$arg" ] || fail "apply takes no arguments"
    apply_files
    ;;
  restore)
    [ -n "$arg" ] || fail "restore needs a backup id"
    restore_backup "$arg"
    ;;
  prune)
    [ -n "$arg" ] || fail "prune needs a keep count"
    prune_backups "$arg"
    ;;
  '')
    fail "missing verb"
    ;;
  *)
    fail "unknown verb $verb"
    ;;
esac
