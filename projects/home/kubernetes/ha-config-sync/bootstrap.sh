#!/usr/bin/env bash
# Owner bootstrap for ha-config-sync (idempotent, --dry-run supported).
#
#   mise run admin:ha-config-sync-bootstrap [-- --dry-run]
#
# Does three things, never printing secret values:
#   1. installs solutions/ha-config-sync/agent/ha-config-agent.sh as
#      /mnt/data/ha-config-sync/agent.sh (root, 0755) and creates the state dirs;
#   2. ensures secret/k8s/home-assistant/ha-config-sync holds an ed25519
#      ssh_private_key plus a known_hosts line pinned as [<HAOS addr>]:22222;
#   3. keeps exactly one authorized_keys line for ha-config-sync:
#      restrict,command="HA_CONFIG_AGENT_ALLOWED_FROM=<node ip> /mnt/data/ha-config-sync/agent.sh"
#      (dropbear has no host-restriction option; the agent enforces the source).
#
# The SSH connection is the owner's `ssh -F ~/.ssh/home-lab/config haos`. This
# script is read-only on the controller except for its temporary directory.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
AGENT="$ROOT/solutions/ha-config-sync/agent/ha-config-agent.sh"
HELPER="$ROOT/projects/home/kubernetes/ha-config-sync/openbao_kv.py"
SSH_CONFIG="${HOME_LAB_SSH_CONFIG:-$HOME/.ssh/home-lab/config}"
KNOWN_HOSTS_FILE="${HAOS_KNOWN_HOSTS_FILE:-$HOME/.ssh/home-lab/known_hosts_haos}"
STATE_ROOT="/mnt/data/ha-config-sync"
REMOTE_AGENT="$STATE_ROOT/agent.sh"
OPENBAO_PATH="k8s/home-assistant/ha-config-sync"

DRY_RUN=0
usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1 (supported: --dry-run)" ;;
  esac
  shift
done

say() {
  printf '%s\n' "$*"
}

[ -f "$AGENT" ] || die "missing agent script: $AGENT"
[ -f "$HELPER" ] || die "missing helper: $HELPER"
[ -f "$SSH_CONFIG" ] || die "missing SSH config: $SSH_CONFIG (run 'mise run secrets:pull')"
[ -f "$KNOWN_HOSTS_FILE" ] || die "missing $KNOWN_HOSTS_FILE (run 'mise run secrets:pull')"

HAOS_IP="${HAOS_IP:-}"
NODE_LAN_IP="${NODE_LAN_IP:-}"
if [ -z "$HAOS_IP" ] || [ -z "$NODE_LAN_IP" ]; then
  say "Reading HAOS_IP/NODE_LAN_IP from secret/k8s/flux-system/app-settings"
  settings="$(python3 "$HELPER" settings)"
  if [ -z "$HAOS_IP" ]; then
    HAOS_IP="$(printf '%s' "$settings" | python3 -c 'import json,sys; print(json.load(sys.stdin)["HAOS_IP"])')"
  fi
  if [ -z "$NODE_LAN_IP" ]; then
    NODE_LAN_IP="$(printf '%s' "$settings" | python3 -c 'import json,sys; print(json.load(sys.stdin)["NODE_LAN_IP"])')"
  fi
fi
case "$HAOS_IP" in
  ''|*[!0-9.]*) die "HAOS_IP is missing or not an IPv4 address" ;;
esac
case "$NODE_LAN_IP" in
  ''|*[!0-9.]*) die "NODE_LAN_IP is missing or not an IPv4 address" ;;
esac

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

public_key=""
key_present=0
known_hosts_present=0
if python3 "$HELPER" has ssh_private_key; then
  key_present=1
fi
if python3 "$HELPER" has known_hosts; then
  known_hosts_present=1
fi

if [ "$key_present" -eq 1 ]; then
  say "OpenBao $OPENBAO_PATH already has ssh_private_key (value hidden)"
else
  say "Generating an ed25519 key for ha-config-sync"
  if [ "$DRY_RUN" -eq 0 ]; then
    ssh-keygen -q -t ed25519 -N '' -C ha-config-sync -f "$tmpdir/key"
  fi
fi

if [ "$DRY_RUN" -eq 0 ]; then
  if [ "$key_present" -eq 1 ]; then
    python3 "$HELPER" get-key "$tmpdir/key"
  fi
  [ -s "$tmpdir/key" ] || die "no private key available"
  public_key="$(ssh-keygen -y -f "$tmpdir/key")"
fi

if [ "$known_hosts_present" -eq 0 ] || [ "$key_present" -eq 0 ]; then
  [ -s "$KNOWN_HOSTS_FILE" ] || die "$KNOWN_HOSTS_FILE is empty"
  say "Pinning HAOS host key as [$HAOS_IP]:22222"
  if [ "$DRY_RUN" -eq 0 ]; then
    awk 'NF >= 3 && $0 !~ /^#/ { print $2 " " $3; exit }' "$KNOWN_HOSTS_FILE" >"$tmpdir/hostkey"
    [ -s "$tmpdir/hostkey" ] || die "no host key line in $KNOWN_HOSTS_FILE"
    printf '[%s]:22222 ' "$HAOS_IP" >"$tmpdir/known_hosts"
    cat "$tmpdir/hostkey" >>"$tmpdir/known_hosts"
    printf '\n' >>"$tmpdir/known_hosts"
    python3 "$HELPER" put-key "$tmpdir/key" "$tmpdir/known_hosts"
  fi
else
  say "OpenBao already has the pinned known_hosts (value hidden)"
fi

say "Installing the forced-command agent at $REMOTE_AGENT"
if [ "$DRY_RUN" -eq 0 ]; then
  ssh -F "$SSH_CONFIG" haos "
    set -e
    mkdir -p '$STATE_ROOT/staging' '$STATE_ROOT/backups'
    chmod 0755 '$STATE_ROOT'
    cat > '$REMOTE_AGENT.new'
    chmod 0755 '$REMOTE_AGENT.new'
    mv '$REMOTE_AGENT.new' '$REMOTE_AGENT'
  " <"$AGENT"
else
  say "[dry-run] ssh -F $SSH_CONFIG haos 'mkdir -p ... && install agent'"
fi

authorized_line="restrict,command=\"HA_CONFIG_AGENT_ALLOWED_FROM=$NODE_LAN_IP $REMOTE_AGENT\" $public_key ha-config-sync"
say "Updating /root/.ssh/authorized_keys (one ha-config-sync line, owner lines kept)"
if [ "$DRY_RUN" -eq 0 ]; then
  encoded="$(printf '%s\n' "$authorized_line" | base64 | tr -d '\n')"
  ssh -F "$SSH_CONFIG" haos "
    set -e
    mkdir -p /root/.ssh
    touch /root/.ssh/authorized_keys
    grep -v 'ha-config-sync\$' /root/.ssh/authorized_keys > /root/.ssh/authorized_keys.new || true
    printf '%s' '$encoded' | base64 -d >> /root/.ssh/authorized_keys.new
    chmod 600 /root/.ssh/authorized_keys.new
    mv /root/.ssh/authorized_keys.new /root/.ssh/authorized_keys
  "
else
  say "[dry-run] would replace old ha-config-sync lines and append one restricted line"
fi

say ""
say "Verification (read-only):"
say "  ssh -F \"$SSH_CONFIG\" haos 'grep ha-config-sync /root/.ssh/authorized_keys'"
say "  mise run kubectl -- -n home-assistant rollout status deployment/ha-config-sync"
say "  mise run kubectl -- -n home-assistant logs deployment/ha-config-sync --tail=50"
