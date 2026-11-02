#!/usr/bin/env bash
# Install vzdump freshness metrics on the Proxmox host from the operator's Mac.
#
#   ./install.sh [--dry-run] <ssh-target>
#
# Installs prometheus-node-exporter, enables its textfile collector, installs
# the metrics script and runs it every 15 minutes. The node exporter listens on
# :9100; Prometheus scrapes PVE through a private values file, never Git.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
HERE="$ROOT/projects/home/backup/pve"
DRY_RUN=false
TARGET=""

usage() {
  cat <<'USAGE'
Usage: install.sh [--dry-run] <ssh-target>

  --dry-run   print the remote commands without changing the host
USAGE
}

while (($#)); do
  case "$1" in
    --dry-run) DRY_RUN=true ;;
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

# SSH_CONFIG selects a private ssh_config (e.g. secrets/proxmox-install/ssh_config).
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

remote "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y prometheus-node-exporter python3"
remote "install -d -m 0755 /usr/local/lib/vzdump-metrics /var/lib/prometheus/node-exporter"

upload "$HERE/backup_metrics.py" /usr/local/lib/vzdump-metrics/backup_metrics.py 0755
upload "$HERE/vzdump-metrics.service" /etc/systemd/system/vzdump-metrics.service 0644
upload "$HERE/vzdump-metrics.timer" /etc/systemd/system/vzdump-metrics.timer 0644

# Enable the textfile collector on the stock Debian package. Keep any other
# ARGS the operator already set; only replace the one textfile setting.
remote 'touch /etc/default/prometheus-node-exporter; if grep -q -- "--collector.textfile.directory" /etc/default/prometheus-node-exporter; then sed -i "s#--collector.textfile.directory=[^\" ]*#--collector.textfile.directory=/var/lib/prometheus/node-exporter#" /etc/default/prometheus-node-exporter; else printf "%s\n" "ARGS=\"--collector.textfile.directory=/var/lib/prometheus/node-exporter\"" >> /etc/default/prometheus-node-exporter; fi'

remote "systemctl daemon-reload && systemctl enable prometheus-node-exporter && systemctl restart prometheus-node-exporter && systemctl enable --now vzdump-metrics.timer"
remote "systemctl start vzdump-metrics.service"

echo "vzdump metrics installed on $TARGET"
