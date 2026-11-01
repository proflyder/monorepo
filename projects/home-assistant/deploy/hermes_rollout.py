#!/usr/bin/env python3
"""Promote new Hermes/voice-proxy digests into the `runtime` Helm release.

deploy-home builds the two images, then calls this script with the digests it
pushed. The script owns the risky part of the rollout:

1. read the full private runtime values from the file materialised from
   OpenBao `secret/ci/deploy-home` key `runtime_values`;
2. if neither live image differs from the requested digest, exit without an
   upgrade (a build that did not change the voice images must not restart it);
3. remove the `storage` block and hand the workload values to `helm upgrade`,
   so the CI ServiceAccount never needs cluster-scoped RBAC. Persistent
   volumes/claims are applied separately by `apply-runtime.sh` and carry
   `helm.sh/resource-policy: keep`;
4. remember the deployed revision *before* the upgrade;
5. wait for the rollout and run the smoke checks (health exporter, a Hermes
   Assist turn through the in-cluster HA API, voice-proxy error log scan); the
   Assist turn retries a bounded warm-up window (up to 90 s, 5 s backoff)
   because a just-started Hermes can answer 500 `runtime_draining` before its
   turn machinery is ready;
6. on any failure after a revision was created, `helm rollback` to the
   remembered revision and re-check the voice circuit.

The HA token is never passed on a command line: the smoke Job mounts the
existing `ha-bridge-ha` Secret. The Assist request uses the cluster-internal
HA Service, so no private HA address or token enters GitHub.

Usage:
  hermes_rollout.py --values-file FILE --hermes-digest sha256:... \
      --voice-proxy-digest sha256:... [--force] [--simulate-smoke-failure]

  hermes_rollout.py --rollback-only --state-dir DIR   # workflow safety net
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import time

import yaml

RELEASE = "runtime"
NAMESPACE = "home-assistant"
CHART = pathlib.Path(__file__).resolve().parent / "chart"
HA_URL = "http://home-assistant-external.monitoring.svc.cluster.local:8123"
HA_SECRET = "ha-bridge-ha"
HA_AGENT = "conversation.hermes_agent"
SMOKE_TEXT = "Представься одним коротким предложением. Ничего не изменяй."
EXPECTED_ENDPOINTS = {
    "ha", "music_assistant", "piper", "whisper", "gigaam", "hermes", "voice_proxy",
}
CONTAINERS = {"hermes": 0, "voice-proxy": 1}
REPOS = {
    "hermes": "ghcr.io/proflyder/hermes",
    "voice-proxy": "ghcr.io/proflyder/voice-proxy",
}
ERROR_PATTERN = re.compile(r"(?im)\b(ERROR|CRITICAL|Traceback)\b")


class RolloutError(RuntimeError):
    """A rollout step failed; the caller rolls back when an upgrade happened."""


def _run(command: list[str], *, timeout: int | None = None, input_text: str | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(
        command, text=True, capture_output=True, timeout=timeout, input=input_text, check=False
    )


def _checked(command: list[str], *, timeout: int | None = None, input_text: str | None = None) -> str:
    result = _run(command, timeout=timeout, input_text=input_text)
    if result.returncode != 0:
        raise RolloutError(f"{' '.join(command)} failed: {result.stderr.strip() or result.stdout.strip()}")
    return result.stdout


def normalize_digest(name: str, value: str) -> str:
    """Return a digest-pinned image reference for `name` (hermes/voice-proxy)."""
    repository = REPOS[name]
    value = value.strip()
    if not value:
        raise ValueError(f"{name}: empty digest")
    if value.startswith("sha256:"):
        return f"{repository}@{value}"
    if re.fullmatch(r"[^@]+@sha256:[0-9a-f]{64}", value):
        return value
    raise ValueError(f"{name}: expected sha256:<64 hex> or <repo>@sha256:<64 hex>, got {value!r}")


def image_digest(image: str) -> str:
    _, _, digest = image.partition("@")
    return digest


def load_values(path: pathlib.Path) -> dict:
    values = yaml.safe_load(path.read_text()) or {}
    if not isinstance(values, dict):
        raise ValueError(f"{path}: expected a mapping")
    containers = values.get("workloads", {}).get("hermes", {}).get("containers")
    if not isinstance(containers, list) or len(containers) < 2:
        raise ValueError(f"{path}: workloads.hermes.containers must hold gateway and voice-proxy")
    return values


def workload_values(values: dict) -> dict:
    """Drop cluster-scoped storage; apply-runtime.sh owns PV/PVC."""
    return {key: value for key, value in values.items() if key != "storage"}


def write_values(values: dict) -> pathlib.Path:
    handle = tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False)
    with handle:
        yaml.safe_dump(values, handle, default_flow_style=False, sort_keys=False)
    path = pathlib.Path(handle.name)
    path.chmod(0o600)
    return path


def live_images(kubectl: str, namespace: str, deployment: str) -> dict[str, str]:
    output = _checked(
        [kubectl, "-n", namespace, "get", "deployment", deployment, "-o", "json"]
    )
    deployment = json.loads(output)
    return {
        container["name"]: container["image"]
        for container in deployment["spec"]["template"]["spec"]["containers"]
    }


def rollout_needed(current: dict[str, str], hermes: str, voice_proxy: str) -> bool:
    return (
        current.get("gateway") != hermes
        or current.get("voice-proxy") != voice_proxy
    )


def set_args(hermes: str, voice_proxy: str) -> list[str]:
    return [
        "--set", f"workloads.hermes.containers[{CONTAINERS['hermes']}].image={hermes}",
        "--set", f"workloads.hermes.containers[{CONTAINERS['voice-proxy']}].image={voice_proxy}",
    ]


def latest_revision(helm: str, namespace: str, release: str) -> int:
    result = _run([helm, "-n", namespace, "history", release, "-o", "json"])
    if result.returncode != 0:
        return 0
    try:
        revisions = json.loads(result.stdout or "[]")
    except json.JSONDecodeError:
        return 0
    return max((int(item["revision"]) for item in revisions), default=0)


def upgrade(helm: str, namespace: str, release: str, chart: pathlib.Path, values: pathlib.Path,
            hermes: str, voice_proxy: str, timeout: str) -> subprocess.CompletedProcess:
    return _run([
        helm, "upgrade", "--install", release, str(chart),
        "-n", namespace, "-f", str(values),
        *set_args(hermes, voice_proxy),
        "--wait=legacy", f"--timeout={timeout}",
    ])


def rollback(helm: str, namespace: str, release: str, revision: int, timeout: str) -> str:
    return _checked([
        helm, "rollback", release, str(revision),
        "-n", namespace, "--wait=legacy", f"--timeout={timeout}",
    ])


def health_metrics(kubectl: str, namespace: str) -> str:
    script = (
        "import urllib.request;"
        "print(urllib.request.urlopen('http://127.0.0.1:9116/metrics',timeout=5)"
        ".read().decode())"
    )
    return _checked([
        kubectl, "-n", namespace, "exec", "deployment/runtime-health-exporter",
        "--", "python3", "-c", script,
    ])


def parse_health(text: str) -> dict[str, int]:
    """Parse `ricardo_endpoint_up{endpoint="…"} N` into {endpoint: value}."""
    values: dict[str, int] = {}
    for line in text.splitlines():
        match = re.match(r'^ricardo_endpoint_up\{endpoint="([^"]+)"\}\s+([0-9.]+)', line)
        if match:
            values[match.group(1)] = int(float(match.group(2)))
    return values


def check_health(text: str) -> None:
    values = parse_health(text)
    missing = EXPECTED_ENDPOINTS - values.keys()
    if missing:
        raise RolloutError(f"health exporter is missing endpoints: {sorted(missing)}")
    down = {name: value for name, value in values.items() if name in EXPECTED_ENDPOINTS and value != 1}
    if down:
        raise RolloutError(f"health exporter endpoints are down: {down}")


def voice_proxy_log_errors(text: str) -> list[str]:
    return [line for line in text.splitlines() if ERROR_PATTERN.search(line)]


def check_voice_logs(text: str) -> None:
    errors = voice_proxy_log_errors(text)
    if errors:
        sample = "\n".join(errors[:10])
        raise RolloutError(f"voice-proxy logs contain errors:\n{sample}")


SMOKE_SCRIPT = r"""
import json, os, sys, urllib.error, urllib.request
token = open("/run/secrets/ha/token", encoding="utf-8").read().strip()
payload = json.dumps({
    "text": os.environ["SMOKE_TEXT"],
    "language": "ru",
    "agent_id": os.environ["HA_AGENT"],
}).encode()
request = urllib.request.Request(
    os.environ["HA_URL"].rstrip("/") + "/api/conversation/process",
    data=payload, method="POST",
    headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
)
try:
    with urllib.request.urlopen(request, timeout=150) as response:
        body = response.read().decode()
        status = response.status
except urllib.error.HTTPError as exc:
    # Keep the status code: the caller retries transient 5xx while Hermes warms up.
    print(json.dumps({"ok": False, "status": exc.code, "error": "HTTPError: " + str(exc)}))
    sys.exit(1)
except Exception as exc:  # noqa: BLE001 - report, never print the token
    print(json.dumps({"ok": False, "error": type(exc).__name__ + ": " + str(exc)}))
    sys.exit(1)
data = json.loads(body)
reply = data.get("response", {})
speech = ""
if isinstance(reply, dict):
    block = reply.get("speech", {})
    if isinstance(block, dict):
        speech = (block.get("plain", {}) or {}).get("speech", "") or speech
if status != 200 or reply.get("response_type") == "error" or not speech.strip():
    print(json.dumps({"ok": False, "status": status, "response_type": reply.get("response_type"),
                      "speech": speech, "data": reply.get("data")}))
    sys.exit(1)
print(json.dumps({"ok": True, "speech": speech}))
"""

# Hermes restarts faster than its own warm-up: the first Assist turn after a
# rollout can hit HTTP 500 (`runtime_draining`) or a refused connection for a
# few seconds. Retrying that transient window is not a weaker check: the
# request must still succeed with speech before the rollout is accepted.
SMOKE_RETRY_WINDOW = 90
SMOKE_RETRY_DELAY = 5
_TRANSIENT_ERROR_MARKERS = (
    "HTTPError",   # status decides; kept for the message only
    "URLError",
    "TimeoutError",
    "ConnectionError",
    "ConnectionResetError",
    "RemoteDisconnected",
    "timed out",
    "connection refused",
)


def transient_smoke_result(result: dict) -> bool:
    """True when a failed Assist smoke may succeed once Hermes finishes warming."""
    if result.get("ok"):
        return False
    status = result.get("status")
    if isinstance(status, int) and 500 <= status < 600:
        return True
    if isinstance(status, int) and status != 200:
        # 4xx is a real request/auth problem, not warm-up.
        return False
    error = str(result.get("error", ""))
    return any(marker in error for marker in _TRANSIENT_ERROR_MARKERS)


def smoke_job(name: str, namespace: str, image: str) -> dict:
    return {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/name": "hermes-smoke"},
        },
        "spec": {
            "backoffLimit": 0,
            "activeDeadlineSeconds": 300,
            "ttlSecondsAfterFinished": 600,
            "template": {
                "metadata": {"labels": {"app.kubernetes.io/name": "hermes-smoke"}},
                "spec": {
                    "restartPolicy": "Never",
                    "automountServiceAccountToken": False,
                    "enableServiceLinks": False,
                    "securityContext": {
                        "runAsNonRoot": True,
                        "runAsUser": 1000,
                        "runAsGroup": 1000,
                        "seccompProfile": {"type": "RuntimeDefault"},
                    },
                    "containers": [{
                        "name": "smoke",
                        "image": image,
                        "imagePullPolicy": "IfNotPresent",
                        "command": ["python3", "-c", SMOKE_SCRIPT],
                        "env": [
                            {"name": "HA_URL", "value": HA_URL},
                            {"name": "HA_AGENT", "value": HA_AGENT},
                            {"name": "SMOKE_TEXT", "value": SMOKE_TEXT},
                            {"name": "PYTHONDONTWRITEBYTECODE", "value": "1"},
                        ],
                        "resources": {
                            "requests": {"cpu": "10m", "memory": "32Mi"},
                            "limits": {"memory": "128Mi"},
                        },
                        "securityContext": {
                            "allowPrivilegeEscalation": False,
                            "readOnlyRootFilesystem": True,
                            "capabilities": {"drop": ["ALL"]},
                        },
                        "volumeMounts": [{
                            "name": "ha-token",
                            "mountPath": "/run/secrets/ha",
                            "readOnly": True,
                        }],
                    }],
                    "volumes": [{
                        "name": "ha-token",
                        "secret": {"secretName": HA_SECRET, "defaultMode": 0o444},
                    }],
                },
            },
        },
    }


def job_condition(kubectl: str, namespace: str, name: str) -> str | None:
    result = _run([
        kubectl, "-n", namespace, "get", "job", name, "-o",
        "jsonpath={range .status.conditions[*]}{.type}={.status}{\"\\n\"}{end}",
    ])
    if result.returncode != 0:
        return None
    for line in result.stdout.splitlines():
        if line in ("Complete=True", "Failed=True"):
            return line.split("=")[0]
    return None


def wait_for_job(kubectl: str, namespace: str, name: str, timeout: int = 200) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        condition = job_condition(kubectl, namespace, name)
        if condition in ("Complete", "Failed"):
            return condition
        time.sleep(2)
    raise RolloutError(f"smoke job {name} did not finish within {timeout}s")


def _smoke_result(output: str) -> dict | None:
    """Parse the JSON line the smoke pod prints, if there is one."""
    try:
        parsed = json.loads(output.splitlines()[-1])
    except (IndexError, json.JSONDecodeError):
        return None
    return parsed if isinstance(parsed, dict) else None


def assist_attempt(kubectl: str, namespace: str, image: str) -> dict:
    """Run one smoke Job and return the JSON result it printed.

    Raises RolloutError only when the Job produced no parseable result (it did
    not start, or was killed). A structured failure is returned so the caller
    can decide whether it is transient warm-up.
    """
    name = f"hermes-smoke-{time.time_ns() // 1_000_000}"
    manifest = yaml.safe_dump(smoke_job(name, namespace, image), sort_keys=False)
    _checked([kubectl, "-n", namespace, "apply", "-f", "-"], input_text=manifest)
    try:
        condition = wait_for_job(kubectl, namespace, name)
        logs = _run([kubectl, "-n", namespace, "logs", f"job/{name}", "--tail=20"])
        output = logs.stdout.strip()
        result = _smoke_result(output)
        if result is not None:
            return result
        if condition == "Complete":
            raise RolloutError(f"Assist smoke produced unexpected output: {output!r}")
        detail = _run([kubectl, "-n", namespace, "describe", "job", name, "--tail=40"]).stdout
        raise RolloutError(
            f"Assist smoke job failed: {logs.stderr.strip() or output}\n{detail.strip()}"
        )
    finally:
        _run([kubectl, "-n", namespace, "delete", "job", name, "--ignore-not-found"])


def assist_smoke(
    kubectl: str,
    namespace: str,
    image: str,
    *,
    retry_window: int = SMOKE_RETRY_WINDOW,
    retry_delay: int = SMOKE_RETRY_DELAY,
    sleep=time.sleep,
    clock=time.monotonic,
) -> str:
    """Ask Hermes one Assist turn, retrying a bounded transient window.

    A freshly started Hermes connects Telegram and warms its turn machinery for
    several seconds after the pod is Ready; an Assist request in that window
    fails with HTTP 500 (`runtime_draining`) or a refused connection. Those are
    retried for at most `retry_window` seconds; every other failure (4xx, an
    empty reply, `response_type: error`) fails immediately, as before.
    """
    deadline = clock() + retry_window
    while True:
        result = assist_attempt(kubectl, namespace, image)
        if result.get("ok") and str(result.get("speech", "")).strip():
            return str(result["speech"]).strip()
        if not transient_smoke_result(result):
            raise RolloutError(f"Assist smoke failed: {result}")
        if clock() >= deadline:
            raise RolloutError(
                f"Assist smoke still failing after {retry_window}s: {result}"
            )
        reason = result.get("status") or result.get("error") or "transient failure"
        print(f"Assist smoke is not ready yet ({reason}); retrying in {retry_delay}s", flush=True)
        sleep(retry_delay)


def run_smoke(kubectl: str, namespace: str, image: str, *, simulate_failure: bool = False) -> None:
    _checked([kubectl, "-n", namespace, "rollout", "status", "deployment/hermes", "--timeout=240s"])
    if simulate_failure:
        raise RolloutError("simulated smoke failure requested")
    check_health(health_metrics(kubectl, namespace))
    speech = assist_smoke(kubectl, namespace, image)
    print(f"Assist reply: {speech}")
    logs = _checked([kubectl, "-n", namespace, "logs", "deployment/hermes", "-c", "voice-proxy", "--since=10m"])
    check_voice_logs(logs)
    print("voice-proxy logs: no errors")


def write_state(state: pathlib.Path, name: str, value: str) -> None:
    state.mkdir(parents=True, exist_ok=True)
    (state / name).write_text(value)


def rollback_only(helm: str, kubectl: str, namespace: str, state: pathlib.Path, timeout: str) -> int:
    applied = state / "upgrade-applied"
    previous = state / "previous-revision"
    if not applied.exists() or not previous.exists():
        print("hermes rollout: no upgrade was applied, nothing to roll back")
        return 0
    revision = int(previous.read_text().strip())
    print(f"hermes rollout: rolling back to revision {revision}")
    rollback(helm, namespace, RELEASE, revision, timeout)
    _checked([kubectl, "-n", namespace, "rollout", "status", "deployment/hermes", "--timeout=240s"])
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--values-file", type=pathlib.Path)
    parser.add_argument("--hermes-digest")
    parser.add_argument("--voice-proxy-digest")
    parser.add_argument("--release", default=RELEASE)
    parser.add_argument("--deployment", default="hermes")
    parser.add_argument("--namespace", default=NAMESPACE)
    parser.add_argument("--chart", type=pathlib.Path, default=CHART)
    parser.add_argument("--kubectl", default="kubectl")
    parser.add_argument("--helm", default="helm")
    parser.add_argument("--state-dir", type=pathlib.Path)
    parser.add_argument("--timeout", default="8m")
    parser.add_argument("--force", action="store_true", help="upgrade even when the digests are unchanged")
    parser.add_argument("--simulate-smoke-failure", action="store_true",
                        help="fail after the upgrade (rollback test)")
    parser.add_argument("--rollback-only", action="store_true")
    args = parser.parse_args(argv)

    state = args.state_dir or pathlib.Path(tempfile.mkdtemp(prefix="hermes-rollout-"))

    if args.rollback_only:
        return rollback_only(args.helm, args.kubectl, args.namespace, state, args.timeout)

    if not args.values_file or not args.hermes_digest or not args.voice_proxy_digest:
        parser.error("--values-file, --hermes-digest and --voice-proxy-digest are required")

    hermes = normalize_digest("hermes", args.hermes_digest)
    voice_proxy = normalize_digest("voice-proxy", args.voice_proxy_digest)
    values = load_values(args.values_file)

    current = live_images(args.kubectl, args.namespace, args.deployment)
    if not args.force and not rollout_needed(current, hermes, voice_proxy):
        print("hermes rollout: live images already match the built digests; nothing to do")
        return 0

    previous = latest_revision(args.helm, args.namespace, args.release)
    write_state(state, "previous-revision", str(previous))
    print(f"hermes rollout: revision {previous} -> new digests")

    values_path = write_values(workload_values(values))
    applied = False
    try:
        result = upgrade(
            args.helm, args.namespace, args.release, args.chart,
            values_path, hermes, voice_proxy, args.timeout,
        )
        applied = latest_revision(args.helm, args.namespace, args.release) > previous
        if applied:
            write_state(state, "upgrade-applied", "1")
        if result.returncode != 0:
            raise RolloutError(f"helm upgrade failed: {result.stderr.strip() or result.stdout.strip()}")

        run_smoke(
            args.kubectl, args.namespace, voice_proxy,
            simulate_failure=args.simulate_smoke_failure,
        )
    except (RolloutError, subprocess.TimeoutExpired) as exc:
        print(f"hermes rollout failed: {exc}", file=sys.stderr)
        if applied:
            print(f"hermes rollout: rolling back to revision {previous}", file=sys.stderr)
            try:
                rollback(args.helm, args.namespace, args.release, previous, args.timeout)
                run_smoke(args.kubectl, args.namespace, current.get("voice-proxy", voice_proxy))
                print(f"hermes rollout: rollback to revision {previous} verified", file=sys.stderr)
            except (RolloutError, subprocess.TimeoutExpired) as rollback_exc:
                print(f"hermes rollout: rollback verification failed: {rollback_exc}", file=sys.stderr)
                return 2
            return 1
        return 1
    finally:
        values_path.unlink(missing_ok=True)

    # A successful rollout leaves no pending rollback for the workflow safety net.
    (state / "upgrade-applied").unlink(missing_ok=True)
    print(f"hermes rollout: revision {latest_revision(args.helm, args.namespace, args.release)} is healthy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
