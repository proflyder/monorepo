#!/usr/bin/env bash
# Install the bb prerequisites and the consistent database backup timer.
#
# Run as root inside CT 103 after create-lxc.sh. This script intentionally does
# not enroll the machine: the enrollment command from the bb UI is a one-time
# credential and must be entered interactively as user `bb`, never passed here
# or written to logs.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../versions.sh
# shellcheck disable=SC1091
source "$SCRIPT_DIR/../versions.sh"

DATA_DIR=${BB_DATA_DIR:-}
USER_NAME=bb
USER_HOME=/home/bb

usage() {
  cat <<'EOF'
Usage: install-bb.sh [options]

Options:
  --data-dir PATH   bb data directory for the backup timer. Set it after
                    enrollment when `bb machine show` reports the real path;
                    when omitted, an existing /etc/bb-backup/env is preserved.
  -h, --help        show this help

Node.js version and SHA-256 come from the adjacent versions.sh (push it along
with this script). BB_DATA_DIR may also be exported instead of using --data-dir.
EOF
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --data-dir) DATA_DIR=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run inside the container as root"
: "${NODE_VERSION:?versions.sh did not define NODE_VERSION}"
: "${NODE_LINUX_X64_SHA256:?versions.sh did not define NODE_LINUX_X64_SHA256}"
[[ $(uname -m) == x86_64 ]] ||
  die "this installer pins node-$NODE_VERSION-linux-x64; got $(uname -m)"
if [[ -n $DATA_DIR ]]; then
  [[ $DATA_DIR == /* ]] || die "--data-dir must be an absolute path"
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl git sqlite3 xz-utils

# ----- Node.js from the official tarball -----------------------------------
NODE_ROOT=/usr/local/lib/nodejs/node-v$NODE_VERSION-linux-x64
if [[ $(/usr/local/bin/node --version 2>/dev/null || true) != "v$NODE_VERSION" ]]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  archive=node-v$NODE_VERSION-linux-x64.tar.xz
  curl -fsSL --retry 3 \
    "https://nodejs.org/dist/v$NODE_VERSION/$archive" \
    -o "$tmp/$archive"
  printf '%s  %s\n' "$NODE_LINUX_X64_SHA256" "$tmp/$archive" |
    sha256sum -c -
  install -d -m 755 /usr/local/lib/nodejs
  rm -rf "$NODE_ROOT"
  tar -xJf "$tmp/$archive" -C /usr/local/lib/nodejs
  for bin in node npm npx corepack; do
    ln -sfn "../lib/nodejs/node-v$NODE_VERSION-linux-x64/bin/$bin" \
      "/usr/local/bin/$bin"
  done
  rm -rf "$tmp"
  trap - EXIT
fi
[[ $(/usr/local/bin/node --version) == "v$NODE_VERSION" ]] ||
  die "Node.js installation did not produce v$NODE_VERSION"
/usr/local/bin/npm --version >/dev/null

# ----- dedicated user and a working systemd user bus ------------------------
if ! id "$USER_NAME" >/dev/null 2>&1; then
  useradd --create-home --home-dir "$USER_HOME" --shell /bin/bash \
    --user-group "$USER_NAME"
fi
usermod --home "$USER_HOME" --shell /bin/bash --lock "$USER_NAME"
install -d -o "$USER_NAME" -g "$USER_NAME" -m 700 "$USER_HOME"
loginctl enable-linger "$USER_NAME"

uid=$(id -u "$USER_NAME")
runtime=/run/user/$uid
bus_ok=0
for _ in $(seq 1 30); do
  if runuser -u "$USER_NAME" -- env \
    XDG_RUNTIME_DIR="$runtime" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime/bus" \
    systemctl --user show-environment >/dev/null 2>&1; then
    bus_ok=1
    break
  fi
  sleep 1
done
if ((!bus_ok)); then
  cat >&2 <<'EOF'
error: the systemd user bus for user `bb` is not reachable after enabling linger.

This unprivileged LXC cannot host the bb background service: bb would run
without a service and would not survive a reboot.
LXC не подходит, используйте VM: create a Debian 13 VM with the same resources,
run this installer there, and move the server to that VM instead.
Do not enroll this container.
EOF
  exit 1
fi
echo "systemd user bus for $USER_NAME: ok (linger enabled)"

# ----- consistent SQLite backup before the nightly vzdump -------------------
install -d -m 700 /etc/bb-backup /var/backups/bb
ENV_FILE=/etc/bb-backup/env
if [[ -n $DATA_DIR ]]; then
  printf 'BB_DATA_DIR=%q\n' "$DATA_DIR" >"$ENV_FILE"
elif [[ ! -e $ENV_FILE ]]; then
  cat >"$ENV_FILE" <<'EOF'
# Set this to the data directory reported by `bb machine show` after
# enrollment, for example /home/bb/.bb-machines/<server-host>.
BB_DATA_DIR=
EOF
fi
chmod 600 "$ENV_FILE"

install -m 755 "$SCRIPT_DIR/bb-db-backup.sh" /usr/local/sbin/bb-db-backup

cat >/etc/systemd/system/bb-db-backup.service <<'EOF'
[Unit]
Description=Consistent bb SQLite backup
Documentation=file:///usr/local/sbin/bb-db-backup
After=local-fs.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/bb-db-backup
Nice=10
IOSchedulingClass=best-effort
IOSchedulingPriority=7
NoNewPrivileges=yes
PrivateTmp=yes
EOF

cat >/etc/systemd/system/bb-db-backup.timer <<'EOF'
[Unit]
Description=Daily bb SQLite backup at 03:30

[Timer]
OnCalendar=*-*-* 03:30:00
Persistent=true
AccuracySec=1min
Unit=bb-db-backup.service

[Install]
WantedBy=timers.target
EOF

systemctl daemon-reload
systemctl enable --now bb-db-backup.timer

cat <<EOF

bb prerequisites installed:
  Node.js  $(/usr/local/bin/node --version)
  npm      $(/usr/local/bin/npm --version)
  user     $USER_NAME (linger enabled)
  backup   /var/backups/bb/bb.db, timer 03:30

Next: enrollment.
  1. In the bb UI open Settings -> Machines -> Add machine (manual provider).
  2. Enter a shell as the service user:
       runuser -u $USER_NAME -- env HOME=$USER_HOME \\
         PATH=$USER_HOME/.local/bin:/usr/local/bin:/usr/bin:/bin \\
         XDG_RUNTIME_DIR=$runtime DBUS_SESSION_BUS_ADDRESS=unix:path=$runtime/bus bash
  3. Paste the one-time enrollment command from the UI into that shell.
     Do not pass it to this installer, commit it, or paste it into a ticket/log.
  4. After enrollment, run \`bb machine show <machine>\` and put the reported
     data directory into $ENV_FILE as BB_DATA_DIR=...
     Then verify: systemctl start bb-db-backup.service
EOF
