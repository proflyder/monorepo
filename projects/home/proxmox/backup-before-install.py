#!/usr/bin/env python3
"""Cold-copy local PVCs and host recovery files before a destructive OS install.

Run as root on the current K3s host. The output contains credentials and must
stay private. Workload replica counts are restored even if copying fails.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import subprocess
import time


def kube(*args):
    return subprocess.check_output(["k3s", "kubectl", *args], text=True)


def objects(kind, namespace):
    return json.loads(kube("-n", namespace, "get", kind, "-o", "json"))["items"]


def interrupted(signum, frame):
    raise SystemExit(f"Interrupted by signal {signum}; restoring workloads")


def wait_until(predicate, description, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(2)
    raise TimeoutError(description)


def main(output):
    if os.geteuid() != 0:
        raise SystemExit("Run as root on the K3s host")
    if output.exists():
        raise SystemExit("Refusing to overwrite an existing backup directory")
    output.mkdir(parents=True, mode=0o700)
    os.chmod(output, 0o700)
    for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(sig, interrupted)
    volumes = json.loads(kube("get", "pv", "-o", "json"))["items"]
    (output / "volumes.json").write_text(json.dumps(volumes, indent=2))
    deployments = [(ns, obj["metadata"]["name"], obj["spec"].get("replicas", 1))
                   for ns in ("monitoring", "home-assistant") for obj in objects("deploy", ns)]
    stateful = [("monitoring", obj["metadata"]["name"], obj["spec"].get("replicas", 1))
                for obj in objects("sts", "monitoring")]
    alloy = json.loads(kube("-n", "monitoring", "get", "ds", "alloy", "-o", "json"))
    selector = alloy["spec"]["template"]["spec"].get("nodeSelector", {})
    (output / "workload-replicas.json").write_text(json.dumps({"deployments": deployments, "statefulsets": stateful, "alloyNodeSelector": selector}, indent=2))
    try:
        # Stop the operator before scaling the stateful sets that it reconciles.
        for ns, name, _ in deployments:
            kube("-n", ns, "scale", "deploy/" + name, "--replicas=0")
        wait_until(lambda: not any(any(o["kind"] == "ReplicaSet" for o in p["metadata"].get("ownerReferences", []))
                                  for p in objects("pods", "monitoring")), "monitoring deployments did not terminate")
        for ns, name, _ in stateful:
            kube("-n", ns, "scale", "sts/" + name, "--replicas=0")
        kube("-n", "monitoring", "patch", "ds", "alloy", "--type=merge", "-p",
             json.dumps({"spec": {"template": {"spec": {"nodeSelector": {**selector, "migration-snapshot": "disabled"}}}}}))
        wait_until(lambda: not objects("pods", "home-assistant"), "application writers did not terminate")
        wait_until(lambda: not any(any("persistentVolumeClaim" in v for v in p["spec"].get("volumes", []))
                                  for p in objects("pods", "monitoring")), "telemetry writers did not terminate")
        wait_until(lambda: not any(any(o["kind"] == "DaemonSet" and o["name"] == "alloy"
                                  for o in p["metadata"].get("ownerReferences", []))
                                  for p in objects("pods", "monitoring")), "log collector did not terminate")
        positions = Path("/srv/k3s/alloy-positions")
        if positions.is_dir():
            subprocess.run(["cp", "-a", str(positions), str(output / "alloy-positions")], check=True)
        copied = 0
        for volume in volumes:
            if volume["status"]["phase"] != "Bound":
                continue
            source = volume["spec"].get("local", {}).get("path") or volume["spec"].get("hostPath", {}).get("path")
            if not source:
                raise RuntimeError("A volume has no supported local source")
            source = Path(source)
            if not source.is_dir() or source == Path("/") or output.is_relative_to(source):
                raise RuntimeError("Unsafe or missing volume source")
            destination = output / "volumes" / volume["metadata"]["name"]
            destination.parent.mkdir(mode=0o700, exist_ok=True)
            subprocess.run(["cp", "-a", str(source), str(destination)], check=True)
            copied += 1
        print(f"Cold-copied {copied} bound persistent volumes", flush=True)
        # Filesystem metadata/recovery credentials, not a byte-for-byte disk image.
        host_paths = ["etc", "boot", "boot/efi", "root", "usr/local", "var/lib/shim-signed", "var/lib/dkms", "usr/src",
                      "var/lib/tailscale", "var/backups/k3s-migration/native-hermes-home",
                      "var/backups/k3s-migration/native-compose"]
        import pwd
        home = Path(pwd.getpwuid(1000).pw_dir)
        for relative in (".ssh", ".config", ".local/bin", "nettop/ctl.sh"):
            path = home / relative
            if path.exists():
                host_paths.append(str(path).lstrip("/"))
        modules = Path("/lib/modules") / os.uname().release
        if modules.exists():
            host_paths.append(str(modules.resolve()).lstrip("/"))
        host_paths = [p for p in host_paths if Path("/", p).exists()]
        subprocess.run(["tar", "-C", "/", "--one-file-system", "-cpf", str(output / "host-recovery.tar"), *host_paths], check=True)
        (output / "packages.tsv").write_text(subprocess.check_output(["dpkg-query", "-W", "-f=${binary:Package}\t${Version}\n"], text=True))
        (output / "partition-table.sfdisk").write_text(subprocess.check_output(["sfdisk", "--dump", "/dev/nvme0n1"], text=True))
        subprocess.run(["vgcfgbackup", "-f", str(output / "lvm-metadata.vgcfg"), "ubuntu-vg"], check=True, capture_output=True)
    finally:
        for ns, name, replicas in stateful:
            kube("-n", ns, "scale", "sts/" + name, f"--replicas={replicas}")
        for ns, name, replicas in deployments:
            kube("-n", ns, "scale", "deploy/" + name, f"--replicas={replicas}")
        kube("-n", "monitoring", "patch", "ds", "alloy", "--type=json", "-p",
             json.dumps([{"op": "replace", "path": "/spec/template/spec/nodeSelector", "value": selector}]))
        print("Original workload replica counts restored", flush=True)
    cluster = output / "cluster"
    cluster.mkdir(mode=0o700)
    source = Path("/srv/k3s/runtime/server/db/state.db")
    with sqlite3.connect("file:" + str(source) + "?mode=ro", uri=True) as live, sqlite3.connect(cluster / "state.db") as saved:
        live.backup(saved)
        assert saved.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    for item in ("token", "cred", "tls"):
        src = Path("/srv/k3s/runtime/server") / item
        if src.is_dir():
            shutil.copytree(src, cluster / item, symlinks=True)
        else:
            shutil.copy2(src, cluster / item)
    (output / "secrets.json").write_text(kube("get", "secrets", "-A", "-o", "json"))
    (output / "configmaps.json").write_text(kube("get", "configmaps", "-A", "-o", "json"))
    # Check each regular SQLite database; telemetry TSDB and ONNX are not SQLite.
    checked = 0
    for path in (output / "volumes").rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        with path.open("rb") as stream:
            if stream.read(16) != b"SQLite format 3\x00":
                continue
        with sqlite3.connect("file:" + str(path) + "?mode=ro", uri=True) as db:
            assert db.execute("PRAGMA quick_check").fetchone()[0] == "ok", path.name
        checked += 1
    (output / "snapshot-summary.json").write_text(json.dumps({"bound_volumes": copied, "sqlite_databases_checked": checked,
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "scope": "Cold application/telemetry volumes and host recovery files; not a whole-disk image"}, indent=2))
    # Preserve original UID/GID/modes in copied volumes. The outer directory is
    # root-only; changing all inner permissions would corrupt restore fidelity.
    for path in output.iterdir():
        if path.is_file():
            path.chmod(0o600)
    print(f"Verified {checked} SQLite databases; private snapshot complete", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    main(args.output.resolve())
