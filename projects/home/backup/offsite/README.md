# Encrypted offsite backup (restic → Cloudflare R2)

Daily copy of the state that only lives at home, encrypted client-side by
restic before it leaves the K3s node:

| Source | Content |
|---|---|
| `openbao` | newest raft snapshot from `/srv/k3s/volumes/openbao-backups` |
| `scheduler` | online SQLite copy of the scheduler database |
| `currency_bot` | online SQLite copies from the currency-bot PVC |
| `ha_bridge` | `ledger.json` from the ha-bridge PVC |
| `ha` | newest automatic HA backup, already encrypted by Home Assistant |

OpenBao seal/recovery keys are **not** in this backup. The script refuses any
file whose name matches `*seal*`, `*recovery*` or `*.key`. Recovery keys and the
restic password must be stored separately (password manager / paper), because
without them the offsite copy cannot be decrypted.

## Configuration

Root-only env file `/etc/offsite-backup/env` (never committed):

```sh
RESTIC_REPOSITORY=s3:https://<account-id>.r2.cloudflarestorage.com/<bucket>
RESTIC_PASSWORD_FILE=/etc/offsite-backup/restic.password
AWS_ACCESS_KEY_ID=<r2-access-key>
AWS_SECRET_ACCESS_KEY=<r2-secret-key>
AWS_DEFAULT_REGION=auto
HA_URL=http://192.0.2.28:8123
HA_TOKEN_FILE=/etc/offsite-backup/ha.token
# optional overrides:
# OPENBAO_SNAPSHOT_DIR=/srv/k3s/volumes/openbao-backups
# K3S_STORAGE_DIR=/srv/k3s/runtime/storage
# OFFSITE_METRICS_FILE=/var/lib/node-exporter-textfile/offsite_backup.prom
# OFFSITE_STATE_FILE=/var/lib/offsite-backup/state.json
# OFFSITE_STAGING_ROOT=/var/lib/offsite-backup
# HA_AGENT_ID=hassio.local
# MAX_SOURCE_AGE_HOURS=36
# RESTIC_MAINTENANCE_WEEKDAY=6
```

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

The offsite copy is useless without **both** the restic password and the R2
credentials. Keep them outside the home (password manager, plus an offline
copy of the password).

1. Install restic on any machine with network access to R2.
2. Export the repository settings (do not save them in the repository):

   ```sh
   export RESTIC_REPOSITORY='s3:https://<account-id>.r2.cloudflarestorage.com/<bucket>'
   export AWS_ACCESS_KEY_ID='...'
   export AWS_SECRET_ACCESS_KEY='...'
   export AWS_DEFAULT_REGION=auto
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

6. Rotate the restic password (`restic key add`, then remove the old key) and
   update `/etc/offsite-backup/restic.password` plus the offline copy.
