#!/usr/bin/env python3
"""Model-only deploy with backup and HA/MA recovery checks. No binary/config upgrade."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import time
from urllib.request import Request, urlopen
import uuid

ROOT = Path(__file__).resolve().parents[4]
ENTITIES = ["assist_satellite.iandeks_stantsiia_midi_assist_satellite", "media_player.iandeks_stantsiia_midi"]


def payload(profile, root=ROOT):
    root = Path(root).resolve()
    paths = [(root / profile[key]).resolve() for key in ["model_path", "manifest_path"]]
    if not all(p.is_relative_to(root / "secrets/model-registry") for p in paths):
        raise ValueError("Only private registered models can be deployed with this profile")
    model = paths[0].read_bytes()
    if hashlib.sha256(model).hexdigest() != profile["model_sha256"]:
        raise ValueError("Model does not match pinned profile SHA256")
    manifest = json.loads(paths[1].read_text())
    micro = manifest["micro"]
    if micro["probability_cutoff"] != profile["probability_cutoff"] or micro["sliding_window_size"] != profile["sliding_window_size"]:
        raise ValueError("Manifest thresholds differ from the pinned profile")
    manifest["model"] = "ru_ricardo_mww.tflite"
    return model, json.dumps(manifest, ensure_ascii=False, indent=2).encode()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--profile", type=Path, default=ROOT / "projects/station/configs/wakeword-personal.json")
    p.add_argument("--connection", type=Path, default=ROOT / "secrets/voice-recorder/connection.json")
    p.add_argument("--ha-url", required=True)
    p.add_argument("--ha-token-file", type=Path, default=ROOT / "secrets/home/ha.token")
    args = p.parse_args()
    profile, connection = json.loads(args.profile.read_text()), json.loads(args.connection.read_text())
    model, manifest = payload(profile)
    auth = args.ha_token_file.read_text().strip()
    ssh = ["ssh", "-T", "-i", str(Path(connection["ssh_key"]).expanduser()), "-p", str(connection.get("ssh_port", 22)),
           "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes", "-o", "ConnectTimeout=8", connection["ssh_target"]]
    def remote(command, data=None, timeout=35):
        return subprocess.run(ssh + [command], input=data, capture_output=True, check=True, timeout=timeout).stdout
    def state(entity):
        req = Request(args.ha_url.rstrip("/") + "/api/states/" + entity, headers={"Authorization": "Bearer " + auth})
        with urlopen(req, timeout=5) as r: return json.load(r)["state"]
    if state("input_boolean.alarm_ringing") != "off":
        raise RuntimeError("Alarm is active; refusing to interrupt the station")
    before_states = {e: state(e) for e in ENTITIES}
    if any(s in {"unavailable", "unknown"} for s in before_states.values()) or before_states[ENTITIES[1]] == "playing" or before_states[ENTITIES[0]] != "idle":
        raise RuntimeError("HA/MA must be healthy and idle before deployment")
    before = remote("test ! -d /tmp/ricardo-voice-recorder.lock && test -f /data/pulsar/enabled && pidof pulsar >/dev/null && "
                    "sha256sum /data/pulsar/pulsar /data/pulsar/wakewords/ru_ricardo_mww.tflite").decode().splitlines()
    ident = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    journal = ROOT / "secrets/model-deployments" / ident
    journal.mkdir(parents=True, mode=0o700)
    backup = "/data/pulsar/backups/model-" + ident
    stage = "/tmp/ricardo-model-" + ident
    record = {"version": profile["registry_version"], "created_utc": datetime.now(timezone.utc).isoformat(),
              "backup": backup, "before_hashes": before, "status": "preparing", "live_voice_acceptance": "pending"}
    def save(): (journal / "deployment.json").write_text(json.dumps(record, ensure_ascii=False, indent=2))
    save()
    # This lock spans upload, switch, and reconnect verification, not only the shell command.
    remote("umask 077; mkdir /tmp/ricardo-model-deploy.lock || exit 1; "
           "if ! mkdir " + shlex.quote(stage) + "; then rmdir /tmp/ricardo-model-deploy.lock; exit 1; fi")
    switched = False
    try:
        remote("umask 077; cat > " + stage + "/model.tflite", model)
        remote("umask 077; cat > " + stage + "/model.json", manifest)
        command = f"""set -eu
test ! -d /tmp/ricardo-voice-recorder.lock
test "$(sha256sum {stage}/model.tflite | cut -d ' ' -f1)" = {profile['model_sha256']}
mkdir {backup}
chmod 700 {backup}
cp -p /data/pulsar/wakewords/ru_ricardo_mww.tflite {backup}/ru_ricardo_mww.tflite
cp -p /data/pulsar/wakewords/ru_ricardo_mww.json {backup}/ru_ricardo_mww.json
cp {stage}/model.tflite /data/pulsar/wakewords/ru_ricardo_mww.tflite.new
cp {stage}/model.json /data/pulsar/wakewords/ru_ricardo_mww.json.new
mv /data/pulsar/wakewords/ru_ricardo_mww.tflite.new /data/pulsar/wakewords/ru_ricardo_mww.tflite
mv /data/pulsar/wakewords/ru_ricardo_mww.json.new /data/pulsar/wakewords/ru_ricardo_mww.json
"""
        switched = True
        remote(command)
        record["status"] = "switched"
        save()
        remote("/data/pulsar/ctl.sh reload")
        good = False
        for _ in range(40):
            try:
                states = {e: state(e) for e in ENTITIES}
                if all(s not in {"unavailable", "unknown"} for s in states.values()):
                    good = True
                    record["after_states"] = states
                    break
            except Exception:
                pass
            time.sleep(2)
        if not good:
            raise RuntimeError("HA/MA did not recover; rolling back model")
        after = remote("sha256sum /data/pulsar/pulsar /data/pulsar/wakewords/ru_ricardo_mww.tflite").decode().splitlines()
        if after[0] != before[0] or after[1].split()[0] != profile["model_sha256"]:
            raise RuntimeError("Unexpected binary/model hash after deployment")
        record.update(status="deployed", after_hashes=after)
        save()
        registry = ROOT / "secrets/model-registry/ricardo"
        (registry / "active.json").write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print("Deployed:", profile["registry_version"], "HA and MA available; binary unchanged.")
    except Exception:
        if switched:
            remote(f"set -e; test -s {backup}/ru_ricardo_mww.tflite; test -s {backup}/ru_ricardo_mww.json; "
                   f"cp {backup}/ru_ricardo_mww.tflite /data/pulsar/wakewords/ru_ricardo_mww.tflite.rollback; "
                   f"cp {backup}/ru_ricardo_mww.json /data/pulsar/wakewords/ru_ricardo_mww.json.rollback; "
                   "mv /data/pulsar/wakewords/ru_ricardo_mww.tflite.rollback /data/pulsar/wakewords/ru_ricardo_mww.tflite; "
                   "mv /data/pulsar/wakewords/ru_ricardo_mww.json.rollback /data/pulsar/wakewords/ru_ricardo_mww.json; /data/pulsar/ctl.sh reload")
            record["status"] = "rolled_back"
            save()
        raise
    finally:
        remote(f"busybox rm -rf {stage}; rmdir /tmp/ricardo-model-deploy.lock", timeout=10)


if __name__ == "__main__":
    import os
    os.umask(0o077)
    main()
