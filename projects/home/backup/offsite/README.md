# Encrypted offsite backup (restic → Google Drive via rclone)

Any restic backend works (S3, B2, rclone); the home deployment uses a
Google Drive remote with scope `drive.file`, so rclone only sees files it
created and a free account cannot be overcharged.

Daily copy of the state that only lives at home, encrypted client-side by
restic before it leaves the K3s node:

| Source | Content |
|---|---|
| `openbao` | newest raft snapshot from `/srv/k3s/volumes/openbao-backups` |
| `scheduler` | online SQLite copy of the scheduler database |
| `currency_bot` | online SQLite copies from the currency-bot PVC |
| `ha_bridge` | `ledger.json` from the ha-bridge PVC |
| `ha` | newest automatic HA backup, already encrypted by Home Assistant |
| `bb` | full `bb server export` from CT 103 (bb.db, auth secret, plugins, attachments), optional |

OpenBao seal/recovery keys are **not** in this backup. The script refuses any
file whose name matches `*seal*`, `*recovery*` or `*.key`. Recovery keys and the
restic password must be stored separately (password manager / paper), because
without them the offsite copy cannot be decrypted.

## Configuration

Root-only env file `/etc/offsite-backup/env` (never committed):

```sh
RESTIC_REPOSITORY=rclone:gdrive:home-offsite
RESTIC_PASSWORD_FILE=/etc/offsite-backup/restic.password
# rclone rewrites its config on OAuth refresh: keep it in a writable path.
# Use your own Google OAuth client (Desktop app, consent screen In production);
# rclone's shared client is being retired. drive.file sees only files created
# by that client, so changing the client means a new repository folder.
RCLONE_CONFIG=/var/lib/offsite-backup/rclone.conf
# S3-style alternative: RESTIC_REPOSITORY=s3:https://<endpoint>/<bucket> +
# AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY / AWS_DEFAULT_REGION
HA_URL=http://192.0.2.28:8123
HA_TOKEN_FILE=/etc/offsite-backup/ha.token
# bb server export pulled over ssh (optional; without BB_SSH_TARGET the
# source is skipped). The key is restricted on CT 103 to one forced command:
#   tar -C /var/backups/bb -cf - bb-server-export.tgz
BB_SSH_TARGET=root@192.0.2.30
BB_SSH_KEY_FILE=/etc/offsite-backup/bb_ssh_key
BB_SSH_KNOWN_HOSTS=/etc/offsite-backup/bb_known_hosts
# optional overrides:
# OPENBAO_SNAPSHOT_DIR=/srv/k3s/volumes/openbao-backups
# K3S_STORAGE_DIR=/srv/k3s/runtime/storage
# OFFSITE_METRICS_FILE=/var/lib/node-exporter-textfile/offsite_backup.prom
# OFFSITE_STATE_FILE=/var/lib/offsite-backup/state.json
# OFFSITE_STAGING_ROOT=/var/lib/offsite-backup
# HA_AGENT_ID=hassio.local
# MAX_SOURCE_AGE_HOURS=36
# RESTIC_MAINTENANCE_WEEKDAY=6
# MAX_STAGING_BYTES=2147483648   # refuse uploads above this
# MAX_REPO_BYTES=5368709120      # fail (and alert) when the repository grows past this
# BB_SSH_TIMEOUT_SECONDS=900     # ssh pull timeout
```

When `BB_SSH_TARGET` is empty, the `bb` source is skipped and a message is
written to the journal; the other sources are unaffected. The remote archive is
downloaded as a tar stream, the inner file is checked with `gzip -t`, and the
gzip is unpacked before upload: restic deduplicates the uncompressed
`bb/bb-server-export.tar` better across days. The source age comes from the
tar member mtime and is checked with `MAX_SOURCE_AGE_HOURS` like every other
source.

PVC paths are resolved at runtime by the `_<namespace>_<pvc>` suffix; the
generated `pvc-<uid>_...` name is never hard-coded. The HA backup list is read
from the HA WebSocket API (`backup/info`); the Supervisor is not reachable from
K3s. A small stdlib WebSocket client avoids an extra apt dependency.

## Install / run

```bash
projects/home/backup/offsite/install.sh --dry-run root@192.0.2.27
projects/home/backup/offsite/install.sh root@192.0.2.27
projects/home/backup/offsite/install.sh --run-now root@192.0.2.27
```

The installer copies `secrets/home/offsite-backup.env`,
`secrets/home/offsite-restic.password` and (when present)
`secrets/home/ha.token` over ssh stdin, runs `restic init` only when
`restic cat config` fails, and enables `offsite-backup.timer` (daily 04:45
local, `Persistent=true`). Put the HA token at `/etc/offsite-backup/ha.token`
or point `HA_TOKEN_FILE` at another root-only path.

For the `bb` source, put the private key in `secrets/home/offsite-bb.key` and
the pinned host key in `secrets/home/offsite-bb.known_hosts`; the installer
copies them to `/etc/offsite-backup/bb_ssh_key` and
`/etc/offsite-backup/bb_known_hosts` (0600). The key itself is installed on
CT 103 with `install-bb.sh --offsite-pubkey FILE --offsite-from ADDRESS`
(see `projects/home/proxmox/bb/README.md`).

Manual checks on the node:

```bash
systemctl list-timers offsite-backup.timer
systemctl start offsite-backup.service
journalctl -u offsite-backup.service -n 100 --no-pager
cat /var/lib/node-exporter-textfile/offsite_backup.prom
set -a; . /etc/offsite-backup/env; set +a; restic snapshots
```

`restic forget --keep-daily 7 --keep-weekly 4 --keep-monthly 6` runs daily;
`--prune` and `restic check --read-data-subset=5%` run on Sundays.

## Restore on a fresh machine

The offsite copy is useless without the restic password. Keep it outside the
home (password manager, plus an offline copy). Drive access is recovered by a
new `rclone authorize` on the same Google account.

1. Install restic and rclone on any machine with internet access.
2. Export the repository settings (do not save them in the repository):

   ```sh
   export RCLONE_CONFIG=/path/to/rclone.conf   # remote [gdrive], type drive, scope drive.file
   export RESTIC_REPOSITORY='rclone:gdrive:home-offsite'
   export RESTIC_PASSWORD='...'   # or RESTIC_PASSWORD_FILE
   ```

3. List and verify snapshots:

   ```sh
   restic snapshots
   restic check --read-data-subset=5%
   ```

4. Restore the newest snapshot to an empty directory:

   ```sh
   restic restore latest --target /srv/offsite-restore
   cat /srv/offsite-restore/MANIFEST.json
   ```

   `MANIFEST.json` lists every file with its size, SHA-256 and source timestamp.
   It contains no secrets.

5. Restore the pieces with the normal rebuild procedures
   (`docs/REBUILD.md`):
   - `openbao/*.snap` → OpenBao raft snapshot restore; unseal key is **not** in
     this backup and must come from the separate offline copy.
   - `sqlite/scheduler.db`, `sqlite/currency_bot-*.db` → stop the writer, copy
     the database into its PVC, run `PRAGMA integrity_check`.
   - `ha-bridge/ledger.json` → stop ha-bridge, copy into its PVC.
   - `home-assistant/backup_*.tar` → HA onboarding "restore from backup".
   - `bb/bb-server-export.tar` → bb server restore:
     `gzip -c bb-server-export.tar > bb-server-export.tgz`, then
     `bb server import bb-server-export.tgz --data-dir <dir>`; stop the old
     server first, run `bb server allow-connect`, and verify threads/plugins.
     The export contains bb.db, the auth secret, plugin data and attachments,
     so a bare `bb.db` is not enough for a full restore.

6. Rotate the restic password (`restic key add`, then remove the old key) and
   update `/etc/offsite-backup/restic.password` plus the offline copy.
