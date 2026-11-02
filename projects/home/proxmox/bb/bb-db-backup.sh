#!/usr/bin/env bash
# Consistent online copy of the bb SQLite database.
#
# Installed by install-bb.sh as /usr/local/sbin/bb-db-backup and run by
# bb-db-backup.timer at 03:30, before the 04:00 vzdump. The data directory is
# configured in /etc/bb-backup/env after enrollment:
#
#   BB_DATA_DIR=/home/bb/.bb-machines/<server-host>
#
# Without a configured database the run is a successful no-op with a warning,
# so the timer stays useful before the server is moved onto this host.
set -euo pipefail
umask 077

ENV_FILE=${BB_BACKUP_ENV_FILE:-/etc/bb-backup/env}
DEST=${BB_BACKUP_DIR:-/var/backups/bb}

if [[ -r $ENV_FILE ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

DATA_DIR=${BB_DATA_DIR:-}
if [[ -z $DATA_DIR ]]; then
  echo "warning: BB_DATA_DIR is not set in $ENV_FILE; skipping bb database backup" >&2
  exit 0
fi
DB=$DATA_DIR/bb.db
if [[ ! -f $DB ]]; then
  echo "warning: $DB does not exist; skipping bb database backup" >&2
  exit 0
fi
command -v sqlite3 >/dev/null 2>&1 || {
  echo "error: sqlite3 is not installed" >&2
  exit 1
}

install -d -m 700 "$DEST"
tmp=$(mktemp "$DEST/bb.db.tmp.XXXXXX")
trap 'rm -f "$tmp"' EXIT

sqlite3 "$DB" ".timeout 5000" ".backup '$tmp'"

result=$(sqlite3 "$tmp" 'PRAGMA integrity_check;')
if [[ $result != ok ]]; then
  echo "error: integrity_check failed for the bb backup:" >&2
  printf '%s\n' "$result" >&2
  exit 1
fi

chmod 600 "$tmp"
mv -f "$tmp" "$DEST/bb.db"
trap - EXIT
echo "bb database backup written to $DEST/bb.db"
