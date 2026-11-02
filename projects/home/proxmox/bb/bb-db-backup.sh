#!/usr/bin/env bash
# Consistent online copy of the bb SQLite database plus a full server export.
#
# Installed by install-bb.sh as /usr/local/sbin/bb-db-backup and run by
# bb-db-backup.timer at 03:30, before the 04:00 vzdump. The data directory is
# configured in /etc/bb-backup/env after enrollment:
#
#   BB_DATA_DIR=/home/bb/.bb-machines/<server-host>
#
# The run writes two artefacts into /var/backups/bb:
#
#   bb.db                  consistent SQLite copy (online backup + integrity check)
#   bb-server-export.tgz   `bb server export` archive (bb.db, auth secret,
#                          plugin data, attachments); the only officially
#                          restorable bb artefact (`bb server import`)
#
# Without a configured database the run is a successful no-op with a warning,
# so the timer stays useful before the server is moved onto this host. The
# SQLite copy is kept even when the export fails; a failed export makes the
# whole run exit non-zero so the timer reports it.
set -euo pipefail
umask 077

ENV_FILE=${BB_BACKUP_ENV_FILE:-/etc/bb-backup/env}
DEST=${BB_BACKUP_DIR:-/var/backups/bb}
BB_USER=${BB_USER:-bb}
BB_HOME=${BB_HOME:-/home/bb}
EXPORT_NAME=bb-server-export.tgz

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
export_tmpdir=
dest_widened=0
cleanup() {
  rm -f "$tmp"
  if [[ -n $export_tmpdir ]]; then rm -rf "$export_tmpdir"; fi
  if [[ $dest_widened == 1 ]]; then chmod 700 "$DEST"; fi
}
trap cleanup EXIT

# ----- consistent SQLite copy ----------------------------------------------
sqlite3 "$DB" ".timeout 5000" ".backup '$tmp'"

result=$(sqlite3 "$tmp" 'PRAGMA integrity_check;')
if [[ $result != ok ]]; then
  echo "error: integrity_check failed for the bb backup:" >&2
  printf '%s\n' "$result" >&2
  exit 1
fi

chmod 600 "$tmp"
mv -f "$tmp" "$DEST/bb.db"
echo "bb database backup written to $DEST/bb.db"

# ----- full, restorable server export --------------------------------------
# `bb server export` must run as the service user: the CLI and its data live
# below $BB_HOME. Resolve both the CLI and the uid instead of hard-coding them.
if [[ -n ${BB_CLI:-} ]]; then
  CLI=$BB_CLI
elif [[ -x $BB_HOME/.local/bin/bb ]]; then
  CLI=$BB_HOME/.local/bin/bb
else
  CLI=$(command -v bb 2>/dev/null || true)
fi
if [[ -z $CLI || ! -x $CLI ]]; then
  echo "error: bb CLI not found; cannot create $EXPORT_NAME" >&2
  exit 1
fi
if ! id "$BB_USER" >/dev/null 2>&1; then
  echo "error: user $BB_USER does not exist; cannot create $EXPORT_NAME" >&2
  exit 1
fi
uid=$(id -u "$BB_USER")
runtime=${XDG_RUNTIME_DIR:-/run/user/$uid}

export_tmpdir=$(mktemp -d "$DEST/bb-export.tmp.XXXXXX")
export_file=$export_tmpdir/$EXPORT_NAME
# The tmp dir is created by root; let the service user write the export there.
if [[ $(id -u) == 0 && $uid != 0 ]]; then
  chown "$BB_USER" "$export_tmpdir"
fi

if [[ $(id -u) == "$uid" ]]; then
  if ! HOME="$BB_HOME" XDG_RUNTIME_DIR="$runtime" \
    "$CLI" server export --out "$export_file"; then
    echo "error: bb server export failed; $DEST/bb.db was still written" >&2
    exit 1
  fi
else
  # /var/backups/bb is 0700 root, so the service user cannot traverse into the
  # export tmp dir. Allow traversal (not listing) only while the export runs.
  chmod 711 "$DEST"
  dest_widened=1
  export_status=0
  runuser -u "$BB_USER" -- env HOME="$BB_HOME" XDG_RUNTIME_DIR="$runtime" \
    "$CLI" server export --out "$export_file" || export_status=$?
  chmod 700 "$DEST"
  dest_widened=0
  if [[ $export_status != 0 ]]; then
    echo "error: bb server export failed; $DEST/bb.db was still written" >&2
    exit 1
  fi
fi

if [[ ! -s $export_file ]]; then
  echo "error: bb server export produced no data" >&2
  exit 1
fi
if ! gzip -t "$export_file" 2>/dev/null; then
  echo "error: $EXPORT_NAME is not a valid gzip archive" >&2
  exit 1
fi
if ! tar -tzf "$export_file" 2>/dev/null | grep -E '(^|/)files/bb\.db$' >/dev/null; then
  echo "error: $EXPORT_NAME does not contain files/bb.db" >&2
  exit 1
fi

chmod 600 "$export_file"
if [[ $(id -u) == 0 ]]; then
  chown root:root "$export_file"
fi
mv -f "$export_file" "$DEST/$EXPORT_NAME"
echo "bb server export written to $DEST/$EXPORT_NAME"
trap - EXIT
cleanup
