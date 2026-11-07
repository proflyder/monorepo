#!/usr/bin/env python3
"""Regenerate the frozen `home-deploy` contract fixtures from the Go binary.

The Rust port is compared byte for byte with these fixtures (`parity_runner`).
They were captured while the Go implementation still existed; keep this script
so the fixtures can be re-created (or reviewed) before the Go code is deleted.

Usage:

    platform/deploy-rs/testdata/frozen/generate.py [--go-bin PATH]

With no `--go-bin`, the script builds `./platform/deploy/cmd/home-deploy` from
the repository root with `go build`. Cases whose command is not ported to Rust
yet are recorded with `"status": "skip"` and a reason, so the parity runner
reports them instead of failing.

The fake `kubectl`/`helm` scripts and the HTTP server for `preflight` only exist
to drive the Go binary during capture; every expected stdout, stderr, exit code,
file and trace is stored next to the case.
"""

from __future__ import annotations

import argparse
import base64
import http.server
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

FROZEN = Path(__file__).resolve().parent


def repo_root() -> Path:
    directory = FROZEN
    while directory != directory.parent:
        if (directory / "MODULE.bazel").is_file():
            return directory
        directory = directory.parent
    raise SystemExit("cannot find the repository root (MODULE.bazel)")


REPO = repo_root()
CASES = FROZEN / "cases"
MANIFEST: list[dict] = []
GO: Path
TRACE_INDEX = 0

SKIP_REASONS = {
}


FAKE_KUBECTL = """#!/bin/sh
# Fake kubectl used by the frozen fixtures. Records argv (with `-f FILE`
# payloads and stdin copied next to it) and answers from the files named by the
# FAKE_* variables. The real command names and order are the contract.
set -u
trace="${TRACE_DIR:-trace}"
mkdir -p "$trace"
idx=$(find "$trace" -maxdepth 1 -name '*.argv' | wc -l | tr -d ' ')
argfile="$trace/$idx.argv"
prev=
{
  for argument in "$@"; do
    if [ "$prev" = "-f" ]; then
      case "$argument" in
        -) cat >"$trace/$idx.stdin"; printf '<stdin>\\n' ;;
        *) cp "$argument" "$trace/$idx.payload"; printf '<payload>\\n' ;;
      esac
    else
      printf '%s\\n' "$argument"
    fi
    prev="$argument"
  done
} >"$argfile"

joined=" $* "
case "$joined" in
  *" get deployment "*)
    if [ "${FAKE_GET_FAIL:-0}" = "1" ]; then
      echo "Error from server (Forbidden): deployments.apps is forbidden" >&2
      exit 1
    fi
    live="${FAKE_LIVE:-live.json}"
    prev2=
    for argument in "$@"; do
      if [ "$prev2" = "deployment" ]; then
        if [ -f "live-$argument.json" ]; then
          live="live-$argument.json"
        fi
        break
      fi
      prev2="$argument"
    done
    case "$joined" in
      *jsonpath=*generation*)
        cat "${FAKE_GENERATION:-generation.txt}" 2>/dev/null || true
        exit 0
        ;;
      *" -o json"*)
        if [ -f "$live" ]; then
          cat "$live"
          exit 0
        fi
        echo "Error from server (NotFound): deployments.apps not found" >&2
        exit 1
        ;;
      *)
        if [ -f "$live" ]; then
          exit 0
        fi
        echo "Error from server (NotFound): deployments.apps not found" >&2
        exit 1
        ;;
    esac
    ;;
  *" get --raw "*) cat "${FAKE_METRICS:-metrics.txt}" 2>/dev/null; exit 0 ;;
  *" get job "*) cat "${FAKE_JOB_CONDITION:-job-condition.txt}" 2>/dev/null; exit 0 ;;
  *" logs job/"*) cat "${FAKE_SMOKE_LOG:-smoke-log.json}" 2>/dev/null; exit 0 ;;
  *" describe job "*) exit 0 ;;
  *" logs deployment/"*) cat "${FAKE_VOICE_LOG:-voice-log.txt}" 2>/dev/null; exit 0 ;;
  *" exec "*) exit "${FAKE_EXEC_CODE:-0}" ;;
  *" rollout status "*) exit "${FAKE_ROLLOUT_CODE:-0}" ;;
  *" apply "*) exit "${FAKE_APPLY_CODE:-0}" ;;
  *" delete "*) exit 0 ;;
  *) exit 0 ;;
esac
"""

FAKE_HELM = """#!/bin/sh
# Fake helm used by the frozen fixtures. Records argv/payloads and answers from
# the files named by the FAKE_HELM_* variables.
set -u
trace="${TRACE_DIR:-trace}"
mkdir -p "$trace"
idx=$(find "$trace" -maxdepth 1 -name '*.argv' | wc -l | tr -d ' ')
argfile="$trace/$idx.argv"
prev=
{
  for argument in "$@"; do
    if [ "$prev" = "-f" ]; then
      cp "$argument" "$trace/$idx.payload"
      printf '<payload>\\n'
    else
      printf '%s\\n' "$argument"
    fi
    prev="$argument"
  done
} >"$argfile"

joined=" $* "
case "$joined" in
  *" history "*)
    cat "${FAKE_HELM_HISTORY:-helm-history.json}" 2>/dev/null
    exit 0
    ;;
  *" upgrade "*)
    code="${FAKE_HELM_UPGRADE_CODE:-0}"
    if [ "$code" = "0" ] && [ -n "${FAKE_HELM_HISTORY_AFTER:-}" ] && [ -f "${FAKE_HELM_HISTORY_AFTER}" ]; then
      cp "${FAKE_HELM_HISTORY_AFTER}" "${FAKE_HELM_HISTORY:-helm-history.json}"
    fi
    exit "$code"
    ;;
  *" rollback "*) exit "${FAKE_HELM_ROLLBACK_CODE:-0}" ;;
  *) exit 0 ;;
esac
"""


def fresh(name: str) -> Path:
    directory = CASES / name
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    (directory / "expected").mkdir()
    return directory


def copy_input(directory: Path, relative: str, dest: str | None = None) -> str:
    source = REPO / relative
    target = directory / (dest or f"input/{source.name}")
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir():
        shutil.copytree(source, target)
    else:
        shutil.copy2(source, target)
    return target.relative_to(directory).as_posix()


def write_input(directory: Path, dest: str, contents: str) -> str:
    target = directory / dest
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(contents, encoding="utf-8")
    return dest


def freeze_file(directory: Path, relative: str, outputs: list[dict], golden: str | None = None) -> None:
    source = directory / relative
    target = directory / (golden or f"expected/{relative}")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
    source.unlink()
    outputs.append({"path": relative, "golden": target.relative_to(directory).as_posix()})


def freeze_dir(directory: Path, relative: str, outputs: list[dict]) -> None:
    source = directory / relative
    target = directory / f"expected/{relative}"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, target, dirs_exist_ok=True)
    shutil.rmtree(source)
    for path in sorted(target.rglob("*")):
        if path.is_file():
            original = path.relative_to(directory / "expected").as_posix()
            outputs.append({"path": original, "golden": f"expected/{original}"})


def normalize_trace(directory: Path) -> None:
    """Removes values that differ between two runs (smoke Job names)."""
    trace = directory / "trace"
    if not trace.is_dir():
        return
    for path in trace.iterdir():
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        text = re.sub(r"hermes-smoke-[0-9]+", "hermes-smoke-<id>", text)
        path.write_text(text, encoding="utf-8")


def record(
    directory: Path,
    name: str,
    argv: list[str],
    status: str = "implemented",
    reason: str | None = None,
    env: dict[str, str] | None = None,
    manifest_env: dict[str, str] | None = None,
) -> None:
    if status == "implemented" and name in SKIP_REASONS:
        status = "skip"
        reason = SKIP_REASONS[name]
    environment = os.environ.copy()
    environment.update(env or {})
    result = subprocess.run(
        [str(GO), *argv],
        cwd=directory,
        env=environment,
        capture_output=True,
    )
    normalize_trace(directory)
    (directory / "expected/stdout").write_bytes(result.stdout)
    (directory / "expected/stderr").write_bytes(result.stderr)
    case = {
        "name": name,
        "argv": argv,
        "cwd": f"cases/{directory.name}",
        "status": status,
        "exit": result.returncode,
        "stdout": "expected/stdout",
        "stderr": "expected/stderr",
    }
    if reason:
        case["reason"] = reason
    if manifest_env:
        case["env"] = manifest_env
    if (directory / "expected/stdout").stat().st_size == 0:
        del case["stdout"]
    if (directory / "expected/stderr").stat().st_size == 0:
        del case["stderr"]
    MANIFEST.append(case)


def add_outputs(name: str, outputs: list[dict]) -> None:
    for case in MANIFEST:
        if case["name"] == name:
            if outputs:
                case["outputs"] = outputs
            return
    raise SystemExit(f"unknown case {name}")


# --------------------------------------------------------------------------
# CLI and env/build-id
# --------------------------------------------------------------------------


def generate_cli_and_simple() -> None:
    for name, argv in [
        ("cli-no-args", []),
        ("cli-help", ["--help"]),
        ("cli-unknown-command", ["bogus"]),
        ("cli-build-id-missing-fingerprint", ["build-id"]),
        ("cli-build-id-single-dash", ["build-id", "-fingerprint", "hello"]),
        ("build-id-fingerprint", ["build-id", "--fingerprint", "hello"]),
        ("build-id-revision", ["build-id", "--revision", "abc123", "--fingerprint", "hello"]),
    ]:
        record(fresh(name), name, argv)

    private_env = "# comment\nKEY=value\nQUOTED=\"value with spaces\"\nexport EXPORTED=yes\nKEY_FIRST=first\nKEY_FIRST=second\n"
    for name, key, file in [
        ("env-get-value", "KEY", "input/private.env"),
        ("env-get-quoted", "QUOTED", "input/private.env"),
        ("env-get-exported", "EXPORTED", "input/private.env"),
        ("env-get-first-duplicate-wins", "KEY_FIRST", "input/private.env"),
        ("env-get-absent", "ABSENT", "input/private.env"),
        ("env-get-missing-file", "KEY", "input/missing.env"),
    ]:
        directory = fresh(name)
        if file == "input/private.env":
            write_input(directory, file, private_env)
        record(directory, name, ["env", "get", file, key])


# --------------------------------------------------------------------------
# Render
# --------------------------------------------------------------------------


def image(name: str, digit: int) -> str:
    return f"{name}@sha256:{str(digit) * 64}"


RENDER_CASES = [
    {
        "name": "render-scheduler",
        "unit": "scheduler",
        "env": "platform/deploy/testdata/env/scheduler.env.example",
        "sets": [
            "BUILD_ID=0123456789ab",
            "SCHEDULER_IMAGE=" + image("ghcr.io/proflyder/scheduler", 0),
            "HA_BRIDGE_IMAGE=" + image("ghcr.io/proflyder/ha-bridge", 1),
            "HA_CONTROL_IMAGE=" + image("ghcr.io/proflyder/ha-control", 2),
            "PREFLIGHT_IMAGE=" + image("ghcr.io/proflyder/home-deploy", 3),
        ],
        "files": [
            "projects/home/kubernetes/scheduler/manifests/scheduler.yaml",
            "projects/home/kubernetes/scheduler/manifests/bridge.yaml",
            "projects/home/kubernetes/scheduler/manifests/control.yaml",
            "projects/home/kubernetes/scheduler/manifests/control-service.yaml",
            "projects/home/kubernetes/scheduler/manifests/preflight-job.yaml",
        ],
    },
    {
        "name": "render-currency-bot",
        "unit": "currency-bot",
        "env": "platform/deploy/testdata/env/currency-bot.env.example",
        "sets": [
            "BUILD_ID=0123456789ab",
            "CURRENCY_BOT_IMAGE=" + image("ghcr.io/proflyder/currency-bot", 4),
            "CLOUDFLARED_IMAGE=" + image("cloudflare/cloudflared", 5),
            "CURRENCY_WEBHOOK_URL=https://bot.example.test/telegram/webhook",
            "CURRENCY_BOT_NODE_PORT=30100",
        ],
        "files": [
            "projects/home/kubernetes/currency-bot/manifests/currency-bot.yaml",
            "projects/home/kubernetes/currency-bot/manifests/cloudflared.yaml",
            "projects/home/kubernetes/currency-bot/manifests/network-policies.yaml",
            "projects/home/kubernetes/currency-bot/manifests/servicemonitor.yaml",
        ],
    },
    {
        "name": "render-mcp-gateway",
        "unit": "mcp-gateway",
        "env": "platform/deploy/testdata/env/mcp-gateway.env.example",
        "sets": [
            "BUILD_ID=0123456789ab",
            "MCP_GATEWAY_IMAGE=" + image("ghcr.io/proflyder/mcp-gateway", 6),
            "BB_CT_IP=192.0.2.30",
            "BB_PROJECT_STATION_LAB=proj_0000000000000001",
            "BB_PROJECT_ENGLISH_NOTES=proj_0000000000000003",
            "BB_DISPATCHER_THREAD_ID=thr_0000000000000002",
        ],
        "files": [
            "projects/home/kubernetes/mcp-gateway/manifests/deployment.yaml",
            "projects/home/kubernetes/mcp-gateway/manifests/network-policies.yaml",
            "projects/home/kubernetes/mcp-gateway/manifests/servicemonitor.yaml",
            "projects/home/kubernetes/mcp-gateway/manifests/catalog.yaml",
            "projects/home/kubernetes/mcp-gateway/manifests/bb-targets.yaml",
        ],
    },
    {
        "name": "render-wakegrant",
        "unit": "wakegrant",
        "env": "platform/deploy/testdata/env/wakegrant.env.example",
        "sets": [
            "BUILD_ID=0123456789ab",
            "WAKE_GRANT_IMAGE=" + image("ghcr.io/proflyder/wakegrant", 7),
            "WAKE_GRANT_NODE_PORT=30101",
        ],
        "files": ["projects/home/kubernetes/wakegrant/manifests/wakegrant.yaml"],
    },
    {
        "name": "render-nft",
        "unit": "scheduler",
        "env": "platform/deploy/testdata/env/scheduler.env.example",
        "sets": [
            "BUILD_ID=0123456789ab",
            "SCHEDULER_IMAGE=" + image("ghcr.io/proflyder/scheduler", 0),
            "HA_BRIDGE_IMAGE=" + image("ghcr.io/proflyder/ha-bridge", 1),
            "HA_CONTROL_IMAGE=" + image("ghcr.io/proflyder/ha-control", 2),
            "PREFLIGHT_IMAGE=" + image("ghcr.io/proflyder/home-deploy", 3),
        ],
        "files": [
            "projects/home/kubernetes/scheduler/nft/scheduler-nodeport.nft",
            "projects/home/kubernetes/currency-bot/nft/currency-bot-nodeport.nft",
        ],
    },
]

INGRESS_SETS = [
    "HOME_DOMAIN=home.example.test",
    "LAN_CIDR=192.0.2.0/24",
    "HAOS_ADDRESS=192.0.2.28",
    "HA_HOST=ha.home.example.test",
    "GRAFANA_HOST=grafana.home.example.test",
    "ALARMS_HOST=alarms.home.example.test",
    "CONTROL_HOST=control.home.example.test",
    "CURRENCY_HOST=currency.home.example.test",
]


def generate_render() -> None:
    for spec in RENDER_CASES:
        directory = fresh(spec["name"])
        env_path = copy_input(directory, spec["env"], "input/env")
        files = [copy_input(directory, path, f"input/{Path(path).name}") for path in spec["files"]]
        sets: list[str] = []
        for item in spec["sets"]:
            sets += ["--set", item]
        argv = [
            "render",
            "--unit",
            spec["unit"],
            "--env",
            env_path,
            "--out",
            "out",
            "--rollout-key",
            "ROLLOUT_ID",
            *sets,
            *files,
        ]
        record(directory, spec["name"], argv)
        outputs: list[dict] = []
        freeze_dir(directory, "out", outputs)
        add_outputs(spec["name"], outputs)

    directory = fresh("render-ingress")
    sets = []
    for item in INGRESS_SETS:
        sets += ["--set", item]
    files = [
        copy_input(directory, f"projects/home/kubernetes/ingress/manifests/{name}", f"input/{name}")
        for name in [
            "issuers.yaml",
            "certificate.yaml",
            "certificate-staging.yaml",
            "middleware.yaml",
            "backends.yaml",
            "routes.yaml",
        ]
    ]
    argv = ["render", "--unit", "ingress", "--out", "out", *sets, *files]
    record(directory, "render-ingress", argv)
    outputs = []
    freeze_dir(directory, "out", outputs)
    add_outputs("render-ingress", outputs)

    # Behavioural cases, on tiny templates so the fixture stays readable.
    directory = fresh("render-env-and-set")
    write_input(directory, "input/private.env", "LAN_CIDR=192.0.2.0/24\n")
    write_input(directory, "input/template.yaml", "cidr=${LAN_CIDR}\n")
    record(
        directory,
        "render-env-and-set",
        ["render", "--unit", "scheduler", "--env", "input/private.env", "--out", "out", "--set", "LAN_CIDR=192.0.2.99", "input/template.yaml"],
    )
    outputs = []
    freeze_dir(directory, "out", outputs)
    add_outputs("render-env-and-set", outputs)

    directory = fresh("render-missing-value")
    write_input(directory, "input/template.yaml", "cidr=${LAN_CIDR}\n")
    record(directory, "render-missing-value", ["render", "--unit", "scheduler", "--out", "out", "input/template.yaml"])

    directory = fresh("render-disallowed-set")
    write_input(directory, "input/template.yaml", "x\n")
    record(
        directory,
        "render-disallowed-set",
        ["render", "--unit", "scheduler", "--out", "out", "--set", "NOPE=x", "input/template.yaml"],
    )

    directory = fresh("render-leftover")
    write_input(directory, "input/template.yaml", "host=${HOME_DOMAIN}${lower}\n")
    record(
        directory,
        "render-leftover",
        ["render", "--unit", "ingress", "--out", "out", "--set", "HOME_DOMAIN=home.example.test", "input/template.yaml"],
    )

    directory = fresh("render-unknown-unit")
    write_input(directory, "input/template.yaml", "x\n")
    record(directory, "render-unknown-unit", ["render", "--unit", "nope", "--out", "out", "input/template.yaml"])

    directory = fresh("render-plain")
    write_input(directory, "input/private.env", "LAN_CIDR=192.0.2.0/24\n")
    write_input(directory, "input/template.yaml", "cidr=${LAN_CIDR}\n")
    record(directory, "render-plain", ["render", "--unit", "scheduler", "--env", "input/private.env", "--out", "out", "input/template.yaml"])
    outputs = []
    freeze_dir(directory, "out", outputs)
    add_outputs("render-plain", outputs)

    # Single-dash form of the Go `flag` package.
    directory = fresh("render-single-dash")
    write_input(directory, "input/private.env", "LAN_CIDR=192.0.2.0/24\n")
    write_input(directory, "input/template.yaml", "cidr=${LAN_CIDR}\n")
    record(
        directory,
        "render-single-dash",
        ["render", "-unit", "scheduler", "-env", "input/private.env", "-out", "out", "input/template.yaml"],
    )
    outputs = []
    freeze_dir(directory, "out", outputs)
    add_outputs("render-single-dash", outputs)


# --------------------------------------------------------------------------
# ESO names and opaque Secret
# --------------------------------------------------------------------------


def generate_manifests() -> None:
    directory = fresh("eso-names")
    files = [
        copy_input(directory, f"projects/home/kubernetes/{name}", f"input/{Path(name).name}")
        for name in [
            "scheduler/manifests/external-secrets.yaml",
            "wakegrant/manifests/external-secrets.yaml",
            "currency-bot/manifests/external-secrets.yaml",
        ]
    ]
    record(directory, "eso-names", ["eso", "names", "--out", "names.txt", *files])
    outputs: list[dict] = []
    freeze_file(directory, "names.txt", outputs)
    add_outputs("eso-names", outputs)

    directory = fresh("secret-opaque")
    write_input(directory, "input/private.env", "BOOTSTRAP_TOKEN=s3cret\n")
    record(
        directory,
        "secret-opaque",
        [
            "secret",
            "opaque",
            "--name",
            "bootstrap",
            "--namespace",
            "home-assistant",
            "--key",
            "token",
            "--from-env-file",
            "input/private.env",
            "--env",
            "BOOTSTRAP_TOKEN",
        ],
    )
    missing = fresh("secret-opaque-missing")
    write_input(missing, "input/private.env", "OTHER=value\n")
    record(
        missing,
        "secret-opaque-missing",
        [
            "secret",
            "opaque",
            "--name",
            "bootstrap",
            "--namespace",
            "home-assistant",
            "--key",
            "token",
            "--from-env-file",
            "input/private.env",
            "--env",
            "MISSING",
        ],
    )


# --------------------------------------------------------------------------
# Runtime split
# --------------------------------------------------------------------------

RUNTIME_RENDER = """---
apiVersion: v1
kind: PersistentVolume
metadata:
  name: runtime-hermes
spec:
  capacity:
    storage: 10Gi
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: runtime-hermes
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hermes
"""

RUNTIME_PRIVATE = """nodeName: lab-host
storage:
  hermes:
    size: 10Gi
workloads:
  hermes:
    replicas: 1
    containers:
    - name: gateway
      image: hermes@sha256:aaaa
  runtime-health-exporter:
    replicas: 1
    containers:
    - name: health-exporter
      image: ghcr.io/proflyder/voice-proxy@sha256:old
      command: ["python3", "/metrics/health_exporter.py"]
      volumeMounts:
      - name: metrics
        mountPath: /metrics
    volumes:
    - name: metrics
      configMap:
        name: runtime-health-exporter
"""


def generate_runtime_split() -> None:
    directory = fresh("runtime-split")
    write_input(directory, "input/render.yaml", RUNTIME_RENDER)
    write_input(directory, "input/private.yaml", RUNTIME_PRIVATE)
    record(
        directory,
        "runtime-split",
        [
            "runtime",
            "split",
            "--render",
            "input/render.yaml",
            "--private",
            "input/private.yaml",
            "--storage-out",
            "storage.yaml",
            "--values-out",
            "values.yaml",
            "--health-exporter-image",
            "ghcr.io/proflyder/health-exporter@sha256:new",
        ],
    )
    outputs: list[dict] = []
    freeze_file(directory, "storage.yaml", outputs)
    freeze_file(directory, "values.yaml", outputs)
    add_outputs("runtime-split", outputs)


# --------------------------------------------------------------------------
# OCI
# --------------------------------------------------------------------------


def generate_oci() -> None:
    for name, argv, archive in [
        ("oci-digest", ["oci", "digest", "layout"], None),
        ("oci-pack", ["oci", "pack", "layout", "--ref", "home.test/example:test", "--out", "pack.tar"], "pack.tar"),
        ("oci-docker", ["oci", "docker", "layout", "--ref", "home.test/example:test", "--out", "docker.tar"], "docker.tar"),
        ("oci-pack-single-dash", ["oci", "pack", "layout", "-ref", "home.test/example:test", "-out", "pack.tar"], "pack.tar"),
    ]:
        directory = fresh(name)
        shutil.copytree(REPO / "platform/deploy/testdata/oci/layout", directory / "layout")
        record(directory, name, argv)
        if archive is not None:
            outputs: list[dict] = []
            freeze_file(directory, archive, outputs)
            add_outputs(name, outputs)


# --------------------------------------------------------------------------
# Fake kubectl/helm drivers
# --------------------------------------------------------------------------


def install_fakes(directory: Path) -> None:
    fakes = directory / "fakes"
    fakes.mkdir()
    kubectl = fakes / "kubectl"
    kubectl.write_text(FAKE_KUBECTL, encoding="utf-8")
    kubectl.chmod(0o755)
    helm = fakes / "helm"
    helm.write_text(FAKE_HELM, encoding="utf-8")
    helm.chmod(0o755)


def write_trace_outputs(directory: Path, prefix: str, outputs: list[dict]) -> None:
    trace = directory / "trace"
    if not trace.is_dir():
        return
    for path in sorted(trace.iterdir()):
        if path.is_file():
            outputs.append({"path": f"trace/{path.name}", "golden": f"expected/trace/{path.name}"})
    shutil.copytree(trace, directory / "expected/trace", dirs_exist_ok=True)
    shutil.rmtree(trace)


def generate_converge() -> None:
    cases = [
        ("converge-create", "create", []),
        ("converge-manifest-wins", "manifest-wins", []),
        ("converge-drift-preserve", "drift-preserve", ["--preserve-replicas", "--preserve-env"]),
        ("converge-support-objects", "support-objects", []),
    ]
    for name, fixture, extra in cases:
        directory = fresh(name)
        install_fakes(directory)
        manifest = copy_input(directory, f"platform/deploy/testdata/converge/{fixture}/manifest.yaml", "manifest.yaml")
        live_source = REPO / f"platform/deploy/testdata/converge/{fixture}/live.json"
        if live_source.is_file():
            shutil.copy2(live_source, directory / "live.json")
        write_input(directory, "generation.txt", "18")
        outputs: list[dict] = []
        freeze_files = ["summary.txt"]
        argv = [
            "converge",
            "--kubectl",
            "fakes/kubectl",
            "--namespace",
            "home-assistant",
            "--name",
            "ha-bridge",
            "--file",
            manifest,
            "--summary-file",
            "summary.txt",
            *extra,
        ]
        record(directory, name, argv, env={"TRACE_DIR": "trace"}, manifest_env={"TRACE_DIR": "trace"})
        for relative in freeze_files:
            if (directory / relative).is_file():
                freeze_file(directory, relative, outputs)
        write_trace_outputs(directory, name, outputs)
        add_outputs(name, outputs)


def generate_state() -> None:
    # Continuous deployment already exists: record the live object.
    directory = fresh("state-record")
    install_fakes(directory)
    write_input(directory, "live.json", '{"metadata":{"name":"scheduler","generation":3},"spec":{}}')
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    record(
        directory,
        "state-record",
        ["state", "record", "--state-dir", "state", "--namespace", "home-assistant", "--name", "scheduler", "--kubectl", "fakes/kubectl"],
        env={"TRACE_DIR": "trace"},
        manifest_env={"TRACE_DIR": "trace"},
    )
    outputs: list[dict] = []
    freeze_file(directory, "state/scheduler.before.json", outputs)
    write_trace_outputs(directory, "state-record", outputs)
    add_outputs("state-record", outputs)

    # First install: the API server answers NotFound, so `.absent` is the record.
    directory = fresh("state-record-absent")
    install_fakes(directory)
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    record(
        directory,
        "state-record-absent",
        ["state", "record", "--state-dir", "state", "--namespace", "home-assistant", "--name", "scheduler", "--kubectl", "fakes/kubectl"],
        env={"TRACE_DIR": "trace"},
        manifest_env={"TRACE_DIR": "trace"},
    )
    outputs = []
    freeze_file(directory, "state/scheduler.absent", outputs)
    write_trace_outputs(directory, "state-record-absent", outputs)
    add_outputs("state-record-absent", outputs)

    # Fail-closed: a Forbidden/timeout probe is an error and writes no marker.
    directory = fresh("state-record-get-error")
    install_fakes(directory)
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    record(
        directory,
        "state-record-get-error",
        ["state", "record", "--state-dir", "state", "--namespace", "home-assistant", "--name", "scheduler", "--kubectl", "fakes/kubectl"],
        env={"TRACE_DIR": "trace", "FAKE_GET_FAIL": "1"},
        manifest_env={"TRACE_DIR": "trace", "FAKE_GET_FAIL": "1"},
    )
    outputs = []
    write_trace_outputs(directory, "state-record-get-error", outputs)
    add_outputs("state-record-get-error", outputs)

    directory = fresh("state-mark")
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    record(directory, "state-mark", ["state", "mark", "--state-dir", "state", "--name", "scheduler"])
    outputs = []
    freeze_file(directory, "state/scheduler.applied", outputs)
    add_outputs("state-mark", outputs)

    # Rollback restores the recorded object when the generation moved.
    directory = fresh("state-rollback")
    install_fakes(directory)
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    write_input(
        directory,
        "state/scheduler.before.json",
        json.dumps(
            {
                "metadata": {"name": "scheduler", "generation": 7, "resourceVersion": "42", "uid": "uid-1"},
                "spec": {"replicas": 1},
                "status": {"replicas": 1},
            },
            indent=2,
        )
        + "\n",
    )
    (directory / "state/scheduler.applied").write_text("")
    write_input(directory, "generation.txt", "8")
    record(
        directory,
        "state-rollback",
        ["state", "rollback", "--state-dir", "state", "--namespace", "home-assistant", "--kubectl", "fakes/kubectl"],
        env={"TRACE_DIR": "trace"},
        manifest_env={"TRACE_DIR": "trace"},
    )
    outputs = []
    write_trace_outputs(directory, "state-rollback", outputs)
    add_outputs("state-rollback", outputs)

    # Rollback of a first install deletes the object.
    directory = fresh("state-rollback-absent")
    install_fakes(directory)
    (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")
    (directory / "state/ha-bridge.absent").write_text("")
    (directory / "state/ha-bridge.applied").write_text("")
    record(
        directory,
        "state-rollback-absent",
        ["state", "rollback", "--state-dir", "state", "--namespace", "home-assistant", "--kubectl", "fakes/kubectl"],
        env={"TRACE_DIR": "trace"},
        manifest_env={"TRACE_DIR": "trace"},
    )
    outputs = []
    write_trace_outputs(directory, "state-rollback-absent", outputs)
    add_outputs("state-rollback-absent", outputs)


# --------------------------------------------------------------------------
# Preflight (fake HTTP)
# --------------------------------------------------------------------------


class FakeHTTP(http.server.BaseHTTPRequestHandler):
    responses: dict[str, tuple[int, str]] = {}

    def log_message(self, *args):  # noqa: D401 - silence the default logger
        return

    def _answer(self) -> None:
        code, body = self.responses.get(self.path, (404, ""))
        payload = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    do_GET = _answer
    do_POST = _answer


def start_server(responses: dict[str, tuple[int, str]]):
    handler = type("CaseHandler", (FakeHTTP,), {"responses": responses})
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def preflight_states(**overrides: str) -> list[dict]:
    states = {
        "input_boolean.alarm_ringing": "off",
        "input_text.scheduler_endpoint": "http://192.0.2.27:30098",
    }
    for key, value in overrides.items():
        states[key.replace("__", ".")] = value
    return [{"entity_id": key, "state": value} for key, value in states.items()]


def generate_preflight() -> None:
    cases = [
        ("preflight-ok", json.dumps(preflight_states()), 200, '{"ready":true}', "http://192.0.2.27:30098"),
        ("preflight-ringing-alarm", json.dumps(preflight_states(input_boolean__alarm_ringing="on")), 200, '{"ready":true}', "http://192.0.2.27:30098"),
        ("preflight-legacy-writer", json.dumps(preflight_states(automation__alarm_scheduler="on")), 200, '{"ready":true}', "http://192.0.2.27:30098"),
        ("preflight-endpoint-mismatch", json.dumps(preflight_states()), 200, '{"ready":true}', "http://192.0.2.99:30098"),
        ("preflight-not-ready", json.dumps(preflight_states()), 200, '{"ready":false}', "http://192.0.2.27:30098"),
        ("preflight-ha-http-error", None, 500, '{"ready":true}', "http://192.0.2.27:30098"),
        ("preflight-ha-invalid-json", "not json", 200, '{"ready":true}', "http://192.0.2.27:30098"),
        ("preflight-scheduler-http-error", json.dumps(preflight_states()), 200, '{"ready":true}', "http://192.0.2.27:30098", 500),
    ]
    for case in cases:
        name, states, ha_code, scheduler_body, endpoint = case[:5]
        scheduler_code = case[5] if len(case) > 5 else 200
        directory = fresh(name)
        write_input(directory, "input/ha.token", "ha-token\n")
        write_input(directory, "input/scheduler.token", "scheduler-token\n")
        responses = {
            "/home.v1.SchedulerService/GetStatus": (scheduler_code, scheduler_body),
        }
        if states is not None:
            responses["/api/states"] = (ha_code, states)
        else:
            responses["/api/states"] = (ha_code, "")
        (directory / "http.json").write_text(json.dumps(responses, indent=2) + "\n")
        server = start_server(responses)
        try:
            base = f"http://127.0.0.1:{server.server_port}"
            record(
                directory,
                name,
                [
                    "preflight",
                    "--ha-url",
                    base,
                    "--ha-token-file",
                    "input/ha.token",
                    "--scheduler-url",
                    base,
                    "--scheduler-token-file",
                    "input/scheduler.token",
                    "--expected-endpoint",
                    endpoint,
                ],
            )
        finally:
            server.shutdown()
            server.server_close()


# --------------------------------------------------------------------------
# Hermes rollout
# --------------------------------------------------------------------------

HERMES_VALUES = """storage:
  hermes:
    size: 10Gi
workloads:
  hermes:
    replicas: 1
    containers:
    - name: gateway
    - name: voice-proxy
  runtime-health-exporter:
    replicas: 1
    containers:
    - name: health-exporter
"""

HERMES_METRICS = "".join(
    f'ricardo_endpoint_up{{endpoint="{name}"}} 1\n'
    for name in ["ha", "music_assistant", "piper", "whisper", "gigaam", "hermes", "voice_proxy"]
)


def hermes_live(hermes: str, voice: str) -> str:
    return json.dumps(
        {
            "spec": {
                "template": {
                    "spec": {
                        "containers": [
                            {"name": "gateway", "image": hermes},
                            {"name": "voice-proxy", "image": voice},
                        ]
                    }
                }
            }
        }
    )


def exporter_live(image: str) -> str:
    return json.dumps({"spec": {"template": {"spec": {"containers": [{"name": "health-exporter", "image": image}]}}}})


def generate_hermes() -> None:
    digests = {
        "hermes": "sha256:" + "a" * 64,
        "voice": "sha256:" + "b" * 64,
        "exporter": "sha256:" + "c" * 64,
        "smoke": "ghcr.io/proflyder/home-deploy@sha256:" + "d" * 64,
    }
    hermes_image = "ghcr.io/proflyder/hermes@" + digests["hermes"]
    voice_image = "ghcr.io/proflyder/voice-proxy@" + digests["voice"]
    exporter_image = "ghcr.io/proflyder/health-exporter@" + digests["exporter"]

    def setup(directory: Path, *, live_hermes: str, live_voice: str, live_exporter: str, history: list[dict], history_after: list[dict] | None = None) -> None:
        install_fakes(directory)
        write_input(directory, "values.yaml", HERMES_VALUES)
        (directory / "chart").mkdir()
        write_input(directory, "live-hermes.json", hermes_live(live_hermes, live_voice))
        write_input(directory, "live-runtime-health-exporter.json", exporter_live(live_exporter))
        write_input(directory, "helm-history.json", json.dumps(history))
        if history_after is not None:
            write_input(directory, "helm-history-after.json", json.dumps(history_after))
        write_input(directory, "generation.txt", "1")
        write_input(directory, "metrics.txt", HERMES_METRICS)
        write_input(directory, "job-condition.txt", "Complete=True\n")
        write_input(directory, "smoke-log.json", '{"ok":true,"speech":"Готово.","status":200}\n')
        write_input(directory, "voice-log.txt", "")
        (directory / "state").mkdir()
    # Git does not track empty directories; keep the state dir in the fixture.
    (directory / "state/.keep").write_text("")

    env = {
        "TRACE_DIR": "trace",
        "FAKE_LIVE": "live-hermes.json",
        "FAKE_METRICS": "metrics.txt",
        "FAKE_JOB_CONDITION": "job-condition.txt",
        "FAKE_SMOKE_LOG": "smoke-log.json",
        "FAKE_VOICE_LOG": "voice-log.txt",
        "FAKE_GENERATION": "generation.txt",
        "FAKE_HELM_HISTORY": "helm-history.json",
        "FAKE_HELM_HISTORY_AFTER": "helm-history-after.json",
    }

    def rollout_args(extra: list[str] | None = None) -> list[str]:
        return [
            "hermes",
            "rollout",
            "--values-file",
            "values.yaml",
            "--hermes-digest",
            digests["hermes"],
            "--voice-proxy-digest",
            digests["voice"],
            "--exporter-digest",
            digests["exporter"],
            "--smoke-image",
            digests["smoke"],
            "--chart",
            "chart",
            "--state-dir",
            "state",
            "--kubectl",
            "fakes/kubectl",
            "--helm",
            "fakes/helm",
            "--timeout",
            "8m",
            *(extra or []),
        ]

    # No-op: the live images already match and nothing is upgraded.
    directory = fresh("hermes-noop")
    setup(directory, live_hermes=hermes_image, live_voice=voice_image, live_exporter=exporter_image, history=[{"revision": 3}])
    record(directory, "hermes-noop", rollout_args(), env=env, manifest_env=env)
    outputs: list[dict] = []
    write_trace_outputs(directory, "hermes-noop", outputs)
    add_outputs("hermes-noop", outputs)

    # Upgrade + smoke success.
    directory = fresh("hermes-upgrade-success")
    setup(
        directory,
        live_hermes="ghcr.io/proflyder/hermes@sha256:" + "0" * 64,
        live_voice="ghcr.io/proflyder/voice-proxy@sha256:" + "1" * 64,
        live_exporter=exporter_image,
        history=[{"revision": 1}],
        history_after=[{"revision": 1}, {"revision": 2}],
    )
    record(directory, "hermes-upgrade-success", rollout_args(), env=env, manifest_env=env)
    write_input(directory, "helm-history.json", json.dumps([{"revision": 1}]))
    outputs = []
    freeze_file(directory, "state/previous-revision", outputs)
    write_trace_outputs(directory, "hermes-upgrade-success", outputs)
    add_outputs("hermes-upgrade-success", outputs)

    # Smoke failure after the upgrade: Helm rollback, verification, exit 1.
    directory = fresh("hermes-smoke-failure")
    setup(
        directory,
        live_hermes="ghcr.io/proflyder/hermes@sha256:" + "0" * 64,
        live_voice="ghcr.io/proflyder/voice-proxy@sha256:" + "1" * 64,
        live_exporter=exporter_image,
        history=[{"revision": 1}],
        history_after=[{"revision": 1}, {"revision": 2}],
    )
    record(
        directory,
        "hermes-smoke-failure",
        rollout_args(["--simulate-smoke-failure"]),
        env=env,
        manifest_env=env,
    )
    write_input(directory, "helm-history.json", json.dumps([{"revision": 1}]))
    outputs = []
    for relative in ["state/previous-revision", "state/upgrade-applied"]:
        freeze_file(directory, relative, outputs)
    write_trace_outputs(directory, "hermes-smoke-failure", outputs)
    add_outputs("hermes-smoke-failure", outputs)

    # Workflow safety net: roll back the recorded revision.
    directory = fresh("hermes-rollback-only")
    setup(directory, live_hermes=hermes_image, live_voice=voice_image, live_exporter=exporter_image, history=[{"revision": 1}])
    write_input(directory, "state/previous-revision", "1")
    write_input(directory, "state/upgrade-applied", "1")
    record(
        directory,
        "hermes-rollback-only",
        [
            "hermes",
            "rollout",
            "--rollback-only",
            "--state-dir",
            "state",
            "--kubectl",
            "fakes/kubectl",
            "--helm",
            "fakes/helm",
            "--timeout",
            "8m",
        ],
        env=env,
        manifest_env=env,
    )
    outputs = []
    write_trace_outputs(directory, "hermes-rollback-only", outputs)
    add_outputs("hermes-rollback-only", outputs)

    # No markers: nothing to roll back.
    directory = fresh("hermes-rollback-only-noop")
    setup(directory, live_hermes=hermes_image, live_voice=voice_image, live_exporter=exporter_image, history=[{"revision": 1}])
    record(
        directory,
        "hermes-rollback-only-noop",
        ["hermes", "rollout", "--rollback-only", "--state-dir", "state", "--kubectl", "fakes/kubectl", "--helm", "fakes/helm"],
        env=env,
        manifest_env=env,
    )


# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------


def main() -> int:
    global GO
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go-bin", type=Path, help="Go home-deploy binary (built from the checkout when omitted)")
    arguments = parser.parse_args()

    if arguments.go_bin is not None:
        GO = arguments.go_bin.resolve()
    else:
        build = FROZEN / ".go-build"
        build.mkdir(exist_ok=True)
        GO = build / "home-deploy"
        subprocess.run(
            ["go", "build", "-o", str(GO), "./platform/deploy/cmd/home-deploy"],
            cwd=REPO,
            check=True,
        )
    if not GO.is_file():
        raise SystemExit(f"go binary not found: {GO}")

    if CASES.exists():
        shutil.rmtree(CASES)
    CASES.mkdir(parents=True)
    MANIFEST.clear()

    generate_cli_and_simple()
    generate_render()
    generate_manifests()
    generate_runtime_split()
    generate_oci()
    generate_converge()
    generate_state()
    generate_preflight()
    generate_hermes()

    (FROZEN / "manifest.json").write_text(
        json.dumps({"generator": "generate.py", "cases": MANIFEST}, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    shutil.rmtree(FROZEN / ".go-build", ignore_errors=True)
    print(f"wrote {len(MANIFEST)} cases to {FROZEN}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
