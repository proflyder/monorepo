#!/usr/bin/env bash
# Install the offsite backup on the K3s node from the operator's Mac.
#
#   ./install.sh [--dry-run] [--run-now] <ssh-target>
#
# Private inputs (git-ignored, never committed):
#   secrets/home/offsite-backup.env        RESTIC_REPOSITORY, backend settings,
#                                          HA_URL, HA_TOKEN_FILE, paths
#   secrets/home/offsite-restic.password   restic repository password
#
# The env and password files are streamed over ssh stdin; they never appear in
# argv or shell history. `restic init` runs only when the repository has no
# config yet.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
HERE="$ROOT/projects/home/backup/offsite"
LOCAL_ENV="${OFFSITE_ENV_FILE:-$ROOT/secrets/home/offsite-backup.env}"
LOCAL_PASSWORD="${OFFSITE_RESTIC_PASSWORD_FILE:-$ROOT/secrets/home/offsite-restic.password}"
LOCAL_HA_TOKEN="${OFFSITE_HA_TOKEN_FILE:-$ROOT/secrets/home/offsite-ha.token}"
# Optional rclone backend config (e.g. Google Drive, scope drive.file). It is
# placed under /var/lib/offsite-backup because rclone rewrites it on token refresh.
LOCAL_RCLONE_CONF="${OFFSITE_RCLONE_CONF:-$ROOT/secrets/home/offsite-rclone.conf}"
# Optional ssh material for pulling the bb server export from CT 103.
LOCAL_BB_KEY="${OFFSITE_BB_KEY_FILE:-$ROOT/secrets/home/offsite-bb.key}"
LOCAL_BB_KNOWN_HOSTS="${OFFSITE_BB_KNOWN_HOSTS_FILE:-$ROOT/secrets/home/offsite-bb.known_hosts}"
DRY_RUN=false
RUN_NOW=false
TARGET=""

usage() {
  cat <<'USAGE'
Usage: install.sh [--dry-run] [--run-now] <ssh-target>

  --dry-run   print the remote commands without changing the host
  --run-now   start offsite-backup.service once after enabling the timer
USAGE
}

while (($#)); do
  case "$1" in
    --dry-run) DRY_RUN=true ;;
    --run-now) RUN_NOW=true ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    *)
      if [[ -n "$TARGET" ]]; then
        echo "only one ssh target is supported" >&2
        exit 2
      fi
      TARGET="$1"
      ;;
  esac
  shift
done

[[ -n "$TARGET" ]] || { usage >&2; exit 2; }

# SSH_CONFIG selects a private ssh_config (e.g. ~/.ssh/home-lab/config).
# A non-root login runs every remote command through `sudo -n`.
SSH=(ssh)
if [[ -n "${SSH_CONFIG:-}" ]]; then SSH+=(-F "$SSH_CONFIG"); fi
SUDO=""
if ! $DRY_RUN && [[ "$("${SSH[@]}" "$TARGET" id -u)" != 0 ]]; then SUDO="sudo -n"; fi

as_root() {
  if [[ -n "$SUDO" ]]; then printf '%s bash -c %q' "$SUDO" "$1"; else printf '%s' "$1"; fi
}

remote() {
  if $DRY_RUN; then
    printf '+ ssh %s %s\n' "$TARGET" "$*"
  else
    "${SSH[@]}" "$TARGET" "$(as_root "$*")"
  fi
}

upload() {
  local source=$1 destination=$2 mode=$3
  if $DRY_RUN; then
    printf '+ ssh %s "cat > %s" < %s\n' "$TARGET" "$destination" "$source"
    return
  fi
  [[ -f "$source" ]] || { echo "missing local file: $source" >&2; exit 1; }
  "${SSH[@]}" "$TARGET" "$(as_root "cat > '$destination' && chmod '$mode' '$destination'")" < "$source"
}

if ! $DRY_RUN; then
  [[ -f "$LOCAL_ENV" ]] || { echo "missing $LOCAL_ENV" >&2; exit 1; }
  [[ -f "$LOCAL_PASSWORD" ]] || { echo "missing $LOCAL_PASSWORD" >&2; exit 1; }
else
  [[ -f "$LOCAL_ENV" ]] || echo "warning: $LOCAL_ENV does not exist (dry run)" >&2
  [[ -f "$LOCAL_PASSWORD" ]] || echo "warning: $LOCAL_PASSWORD does not exist (dry run)" >&2
fi
[[ -f "$LOCAL_HA_TOKEN" ]] || echo "warning: $LOCAL_HA_TOKEN does not exist; upload HA token separately" >&2
[[ -f "$LOCAL_BB_KEY" ]] || echo "warning: $LOCAL_BB_KEY does not exist; bb source stays disabled" >&2
[[ -f "$LOCAL_BB_KNOWN_HOSTS" ]] || echo "warning: $LOCAL_BB_KNOWN_HOSTS does not exist; bb source stays disabled" >&2

remote "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y restic rclone sqlite3 python3"
remote "install -d -m 0700 /etc/offsite-backup /var/lib/offsite-backup && install -d -m 0755 /usr/local/lib/offsite-backup /var/lib/node-exporter-textfile"

upload "$HERE/offsite_backup.py" /usr/local/lib/offsite-backup/offsite_backup.py 0755
upload "$HERE/README.md" /usr/local/lib/offsite-backup/README.md 0644
upload "$HERE/offsite-backup.service" /etc/systemd/system/offsite-backup.service 0644
upload "$HERE/offsite-backup.timer" /etc/systemd/system/offsite-backup.timer 0644

# Secrets are streamed over stdin and only readable by root.
upload "$LOCAL_ENV" /etc/offsite-backup/env 0600
upload "$LOCAL_PASSWORD" /etc/offsite-backup/restic.password 0600
if [[ -f "$LOCAL_HA_TOKEN" ]]; then
  upload "$LOCAL_HA_TOKEN" /etc/offsite-backup/ha.token 0600
fi
if [[ -f "$LOCAL_RCLONE_CONF" ]]; then
  upload "$LOCAL_RCLONE_CONF" /var/lib/offsite-backup/rclone.conf 0600
fi
if [[ -f "$LOCAL_BB_KEY" ]]; then
  upload "$LOCAL_BB_KEY" /etc/offsite-backup/bb_ssh_key 0600
fi
if [[ -f "$LOCAL_BB_KNOWN_HOSTS" ]]; then
  upload "$LOCAL_BB_KNOWN_HOSTS" /etc/offsite-backup/bb_known_hosts 0600
fi

remote "chown root:root /etc/offsite-backup/env /etc/offsite-backup/restic.password; for f in ha.token bb_ssh_key bb_known_hosts; do if [ -f /etc/offsite-backup/\$f ]; then chown root:root /etc/offsite-backup/\$f; fi; done"

# Initialise the repository only once. The env file is sourced on the remote
# host, so credentials never travel on a command line.
if $DRY_RUN; then
  printf '+ ssh %s %s\n' "$TARGET" "source env; restic cat config >/dev/null 2>&1 || restic init"
else
  if ! remote 'set -a; . /etc/offsite-backup/env; set +a; restic cat config >/dev/null 2>&1'; then
    remote 'set -a; . /etc/offsite-backup/env; set +a; restic init >/dev/null && echo "restic repository initialised"'
  fi
fi

remote "systemctl daemon-reload && systemctl enable --now offsite-backup.timer"
if $RUN_NOW; then
  remote "systemctl start offsite-backup.service"
fi

echo "offsite backup installed on $TARGET"
