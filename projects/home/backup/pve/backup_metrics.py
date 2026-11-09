#!/usr/bin/env python3
"""Export Proxmox vzdump freshness as Prometheus textfile metrics.

Run by ``vzdump-metrics.timer`` every 15 minutes on the PVE host. The script
reads ``pvesh get /nodes/<node>/storage/<storage>/content --content backup
--output-format json`` (``pvesm list`` has no JSON output on PVE 9), keeps the
newest archive
per guest, resolves the guest name from ``qm config``/``pct config`` (or the
archive's ``.notes`` file) and atomically writes:

    home_vzdump_last_backup_timestamp_seconds{vmid,guest}
    home_vzdump_last_backup_bytes{vmid,guest}

The node-exporter textfile collector on PVE serves the file, and Prometheus
scrapes PVE through the `pve-node` job in
``projects/home/kubernetes/observability/values/prometheus.yaml`` (the private
target comes from the `PVE_NODE_TARGET` flux-settings key).
"""
from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

DEFAULT_STORAGE = "backup"
DEFAULT_METRICS = Path("/var/lib/prometheus/node-exporter/vzdump.prom")


@dataclass
class GuestBackup:
    vmid: int
    ctime: float
    size: int
    volid: str
    guest: str = ""


def run_command(command: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
    return subprocess.run(command, capture_output=True, text=True, timeout=timeout)


def parse_pvesm_output(text: str) -> list[dict]:
    """Accept a JSON array, JSON object with ``data``, or JSON lines."""
    stripped = text.strip()
    if not stripped:
        return []
    try:
        value = json.loads(stripped)
    except json.JSONDecodeError:
        value = None
    if isinstance(value, list):
        return [item for item in value if isinstance(item, dict)]
    if isinstance(value, dict):
        data = value.get("data", [])
        if isinstance(data, list):
            return [item for item in data if isinstance(item, dict)]
        return []
    entries = []
    for line in stripped.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(item, dict):
            entries.append(item)
    return entries


def newest_per_guest(entries: list[dict]) -> dict[int, GuestBackup]:
    result: dict[int, GuestBackup] = {}
    for item in entries:
        try:
            vmid = int(item["vmid"])
            ctime = float(item["ctime"])
        except (KeyError, TypeError, ValueError):
            continue
        try:
            size = int(item.get("size", 0) or 0)
        except (TypeError, ValueError):
            size = 0
        volid = str(item.get("volid", ""))
        current = result.get(vmid)
        if current is None or ctime > current.ctime:
            result[vmid] = GuestBackup(vmid=vmid, ctime=ctime, size=size, volid=volid)
    return result


def parse_guest_name(config: str) -> str:
    for line in config.splitlines():
        key, _, value = line.partition(":")
        if key.strip() in ("name", "hostname"):
            name = value.strip()
            if name:
                return name
    return ""


def guest_name(vmid: int, volid: str) -> str:
    for command in (["qm", "config", str(vmid)], ["pct", "config", str(vmid)]):
        try:
            result = run_command(command)
        except (OSError, subprocess.TimeoutExpired):
            continue
        if result.returncode == 0:
            name = parse_guest_name(result.stdout)
            if name:
                return name
    if volid:
        try:
            path_result = run_command(["pvesm", "path", volid])
        except (OSError, subprocess.TimeoutExpired):
            path_result = None
        if path_result is not None and path_result.returncode == 0:
            notes = Path(path_result.stdout.strip() + ".notes")
            try:
                first = notes.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                first = []
            if first and first[0].strip():
                return first[0].strip()
    return str(vmid)


def collect(storage: str) -> list[GuestBackup]:
    node = socket.gethostname().split(".")[0]
    result = run_command([
        "pvesh", "get", f"/nodes/{node}/storage/{storage}/content",
        "--content", "backup", "--output-format", "json",
    ])
    if result.returncode != 0:
        raise RuntimeError("pvesh storage content listing failed")
    guests = newest_per_guest(parse_pvesm_output(result.stdout))
    for item in guests.values():
        item.guest = guest_name(item.vmid, item.volid)
    return [guests[vmid] for vmid in sorted(guests)]


def render_metrics(guests: list[GuestBackup]) -> str:
    lines = [
        "# HELP home_vzdump_last_backup_timestamp_seconds Unix time of the newest vzdump archive for a guest.",
        "# TYPE home_vzdump_last_backup_timestamp_seconds gauge",
    ]
    for item in guests:
        lines.append(
            f'home_vzdump_last_backup_timestamp_seconds{{vmid="{item.vmid}",'
            f'guest="{_escape(item.guest)}"}} {item.ctime:.0f}'
        )
    lines.extend(
        [
            "# HELP home_vzdump_last_backup_bytes Size of the newest vzdump archive for a guest.",
            "# TYPE home_vzdump_last_backup_bytes gauge",
        ]
    )
    for item in guests:
        lines.append(
            f'home_vzdump_last_backup_bytes{{vmid="{item.vmid}",'
            f'guest="{_escape(item.guest)}"}} {item.size}'
        )
    return "\n".join(lines) + "\n"


def _escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


def write_metrics(path: Path, guests: list[GuestBackup]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(render_metrics(guests))
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--storage",
        default=os.environ.get("PVE_BACKUP_STORAGE", DEFAULT_STORAGE),
        help="pvesm storage with vzdump archives",
    )
    parser.add_argument(
        "--output",
        default=os.environ.get("VZDUMP_METRICS_FILE", str(DEFAULT_METRICS)),
        help="node-exporter textfile path",
    )
    args = parser.parse_args(argv)
    try:
        guests = collect(args.storage)
        if not guests:
            raise RuntimeError("no vzdump archives found")
        write_metrics(Path(args.output), guests)
    except Exception as exc:  # noqa: BLE001 - one concise failure line for journald
        print(f"vzdump metrics failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
