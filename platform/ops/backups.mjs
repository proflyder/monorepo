// Backup target registry: the single source of truth for "what is backed up,
// where, when, how to check it and how it is restored".
//
// This is data, not code. Every target points at an existing procedure:
//
//   * `status`/`run`/`verify` are the commands documented in the same docs
//     sections as `restore_doc`. They are only turned into argv by
//     `platform/mise-tasks/lib/backup.mjs`; nothing here executes anything.
//   * `ssh` selects the documented host alias from
//     `~/.ssh/home-lab/config` (pve|k3s|haos). No LAN address is hard-coded.
//   * `run: null` means "only through a UI"; `manual_note` says how.
//
// `platform/mise-tasks/lib/backup.test.mjs` validates ids, required fields,
// action shapes and that every `restore_doc` file and heading still exist, so
// moving a docs section breaks the test instead of the operator.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../..");

/** Documented host aliases from `~/.ssh/home-lab/config` (`docs/OPERATIONS.md`). */
export const SSH_HOSTS = ["pve", "k3s", "haos"];

export const REQUIRED_FIELDS = [
  "id",
  "title",
  "schedule",
  "where",
  "restore_doc",
];

export const TARGETS = [
  {
    id: "pve-vzdump",
    title: "Proxmox nightly vzdump гостей 100/101/102/103",
    schedule: "daily 04:00 (PVE job `nightly`)",
    where: "PVE (ssh alias pve)",
    status: {
      ssh: "pve",
      shell: "pvesh get /cluster/backup; pvesm list backup | tail",
    },
    run: {
      ssh: "pve",
      shell: "vzdump 101 --storage backup --mode snapshot --compress zstd",
    },
    run_note:
      "Команда из docs для одного гостя; замените 101 на нужный VMID " +
      "(100 HAOS, 101 K3s, 102 VPN, 103 bb). Ночной job делает все сам.",
    verify: null,
    verify_note:
      "Проверка — restore в новый VMID без сети (`--unique`, отключить net0), " +
      "не поверх живой VM. Автоматически не запускается.",
    restore_doc: "docs/OPERATIONS.md#backup",
  },
  {
    id: "ha-supervisor",
    title: "Home Assistant full backup (Supervisor)",
    schedule: "daily 03:30, 3 copies (automatic encrypted backup)",
    where: "HAOS VM 100 — HA UI / Supervisor",
    status: null,
    manual_note:
      "Проверка — HA UI → Settings → System → Backups (документированной " +
      "CLI-команды нет; список бэкапов читает только offsite-скрипт).",
    run: null,
    verify: null,
    restore_doc: "docs/OPERATIONS.md#backup",
  },
  {
    id: "offsite-restic",
    title: "Offsite restic → Google Drive (rclone:gdrive:home-offsite-v2)",
    schedule: "daily 04:45 (offsite-backup.timer); prune/check — Sunday",
    where: "K3s node (ssh alias k3s)",
    status: {
      ssh: "k3s",
      shell:
        "systemctl list-timers offsite-backup.timer; " +
        "cat /var/lib/node-exporter-textfile/offsite_backup.prom",
    },
    run: { ssh: "k3s", shell: "systemctl start offsite-backup.service" },
    run_note:
      "Логи: journalctl -u offsite-backup.service -n 100 --no-pager.",
    verify: {
      ssh: "k3s",
      shell:
        "set -a; . /etc/offsite-backup/env; set +a; " +
        "restic check --read-data-subset=5%",
    },
    verify_note:
      "`restic check --read-data-subset=5%` — документированная воскресная " +
      "проверка; читает данные из Drive и нагружает канал. " +
      "`restic snapshots` — более легкая альтернатива.",
    restore_doc: "docs/OPERATIONS.md#offsite-backup-google-drive--restic",
  },
  {
    id: "bb-db",
    title: "BB server SQLite + full `bb server export` (CT 103)",
    schedule: "daily 03:30 (bb-db-backup.timer on CT 103)",
    where: "BB server CT 103 (ssh alias pve → pct exec 103)",
    status: {
      ssh: "pve",
      shell: "pct exec 103 -- systemctl list-timers bb-db-backup.timer",
    },
    run: {
      ssh: "pve",
      shell: "pct exec 103 -- systemctl start bb-db-backup.service",
    },
    verify: {
      ssh: "pve",
      shell: 'pct exec 103 -- sqlite3 /var/backups/bb/bb.db "PRAGMA integrity_check;"',
    },
    restore_doc: "docs/REBUILD.md#4a-bb-server-ct-103",
  },
  {
    id: "scheduler-db",
    title: "Scheduler SQLite + ha-bridge ledger (quiesced backup.sh)",
    schedule: "manual/on demand; offsite-restic copies it daily",
    where: "workstation, kubectl + admin kubeconfig",
    status: null,
    manual_note:
      "Документированной команды свежести нет: `backup.sh` — ручной, " +
      "ежедневная копия уходит в offsite-restic. Каталог смотреть через " +
      "`ls -lt local-data/local/backups/scheduler` (вне Git).",
    run: { shell: "projects/home/kubernetes/scheduler/backup.sh" },
    run_note:
      "Запуск — `mise run admin:scheduler-backup`: admin kubeconfig " +
      "приходит из OpenBao через fnox (профиль kube). " +
      "Скрипт сам останавливает writers и возвращает replicas.",
    verify: {
      shell:
        'B=$(ls -dt local-data/local/backups/scheduler/* | head -1); ' +
        '(cd "$B" && shasum -a 256 -c SHA256SUMS) && ' +
        'sqlite3 -readonly "$B/scheduler-data/scheduler.db" ' +
        "'pragma integrity_check'",
    },
    verify_note:
      "Проверяются контрольные суммы и целостность SQLite последнего " +
      "каталога backup.sh (процедура из docs/REBUILD.md).",
    restore_doc: "docs/REBUILD.md#scheduler-из-sqlite",
  },
  {
    id: "openbao-raft",
    title: "OpenBao raft snapshot (CronJob, 7 последних файлов)",
    schedule: "daily 03:17 UTC (openbao-raft-snapshot CronJob)",
    where: "K3s namespace openbao, PVC openbao-backups",
    status: {
      shell:
        "kubectl -n openbao get cronjob openbao-raft-snapshot; " +
        "kubectl -n openbao get jobs",
    },
    run_note:
      "Нужен admin kubeconfig: команду запускать через " +
      "`mise run secrets:exec -- -P kube exec -- …`. Job `snapshot-manual` сначала " +
      "удалить, если он уже есть: имя в examples из docs фиксированное.",
    run: {
      shell:
        "kubectl -n openbao create job --from=cronjob/openbao-raft-snapshot " +
        "snapshot-manual",
    },
    verify: null,
    verify_note:
      "Restore-drill — `projects/home/kubernetes/openbao/restore_drill.py` " +
      "(docs/REBUILD.md §6); нагружает стенд, запускать вручную.",
    restore_doc: "docs/KUBERNETES-OPERATIONS.md#backup--restore",
  },
];

/** GitHub-style heading slug, enough to verify `docs/*.md#anchor` links. */
export function slugifyHeading(heading) {
  return heading
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s/g, "-");
}

export function byId(list, id) {
  return list.find((target) => target.id === id) ?? null;
}

/**
 * Resolve repeated arguments / comma lists to targets. `all` (or no argument,
 * when `defaultAll`) expands to the registry order.
 */
export function resolveTargets(list, rawArgs = [], options = {}) {
  const defaultAll = options.defaultAll ?? false;
  const requested = rawArgs
    .flatMap((argument) => argument.split(","))
    .map((item) => item.trim())
    .filter(Boolean);

  if (!requested.length) {
    if (defaultAll) {
      return list;
    }
    throw new Error(
      "backup: name a target (see `mise run backup:list`)",
    );
  }
  if (requested.includes("all")) {
    if (requested.length > 1) {
      throw new Error("backup: `all` cannot be combined with other targets");
    }
    return list;
  }

  const unknown = [...new Set(requested.filter((id) => !byId(list, id)))].sort();
  if (unknown.length) {
    throw new Error(
      `backup: unknown target: ${unknown.join(", ")}.\n` +
        `Targets: ${list.map((target) => target.id).join(", ")}`,
    );
  }
  return list.filter((target) => requested.includes(target.id));
}

function isAction(value) {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof value.shell === "string" &&
    value.shell.trim() !== "" &&
    (value.ssh === undefined || SSH_HOSTS.includes(value.ssh))
  );
}

/** Validate the registry; returns a list of problems (empty means valid). */
export function validateTargets(list = TARGETS, options = {}) {
  const root = options.root ?? ROOT;
  const fileSystem = options.fs ?? fs;
  const readFile = (target) => fileSystem.readFileSync(target, "utf8");
  const fileExists = (target) => fileSystem.existsSync(target);

  const problems = [];
  const ids = list.map((target) => target.id);
  const duplicates = [...new Set(ids.filter((id) => ids.filter((value) => value === id).length > 1))];
  if (duplicates.length) {
    problems.push(`duplicate target ids: ${duplicates.join(",")}`);
  }

  for (const target of list) {
    const name = target.id || "<missing id>";
    for (const field of REQUIRED_FIELDS) {
      if (typeof target[field] !== "string" || !target[field].trim()) {
        problems.push(`${name}: missing '${field}'`);
      }
    }
    for (const field of ["status", "run", "verify"]) {
      if (!(field in target)) {
        problems.push(`${name}: missing '${field}' (use null when not applicable)`);
      }
    }
    for (const field of ["status", "run", "verify"]) {
      const action = target[field];
      if (action !== null && action !== undefined && !isAction(action)) {
        problems.push(
          `${name}: '${field}' must be null or { ssh?: pve|k3s|haos, shell: string }`,
        );
      }
    }
    if (target.run === null && !String(target.manual_note ?? "").trim()) {
      problems.push(`${name}: run is null but manual_note is empty`);
    }
    if (
      target.status === null &&
      target.run !== null &&
      !String(target.manual_note ?? "").trim()
    ) {
      problems.push(`${name}: status is null but manual_note is empty`);
    }

    if (typeof target.restore_doc === "string") {
      const match = /^(docs\/[^#]+\.md)#(.+)$/.exec(target.restore_doc);
      if (!match) {
        problems.push(
          `${name}: restore_doc must look like docs/<file>.md#<anchor>`,
        );
      } else {
        const [, file, anchor] = match;
        const absolute = path.resolve(root, file);
        if (!fileExists(absolute)) {
          problems.push(`${name}: restore_doc file is missing: ${file}`);
        } else {
          const headings = String(readFile(absolute))
            .split(/\r?\n/)
            .filter((line) => /^#{1,6}\s/.test(line))
            .map((line) => slugifyHeading(line.replace(/^#{1,6}\s+/, "")));
          if (!headings.includes(anchor)) {
            problems.push(
              `${name}: restore_doc anchor '${anchor}' is not a heading in ${file}`,
            );
          }
        }
      }
    }
  }
  return problems;
}

export default TARGETS;
