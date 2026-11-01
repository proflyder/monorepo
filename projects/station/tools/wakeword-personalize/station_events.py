#!/usr/bin/env python3
"""Collect, label and report real wake-word events recorded on the station.

The device writes `/data/pulsar/wake-events/` (contract: schema 1). This tool
pulls an immutable snapshot over ssh, keeps local relabels separate, reports
false positives per listening hour and turns confirmed events into a manifest
that `finetune.py` can merge with `--extra-manifest`.

Recordings, transcripts and metrics stay in `secrets/`; never commit them.
"""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tarfile
import wave

SCHEMA = 1
KINDS = ("hit", "near", "miss")
LABELS = ("false", "true", "missed")
OUTCOMES = ("answered", "empty_stt", "cancelled", "error", "dry_run", "arbiter_lost")
SUSPECTED_OUTCOMES = ("empty_stt", "cancelled")
REMOTE_DIR = "/data/pulsar/wake-events"
ROOT = Path(__file__).resolve().parents[4]
DEFAULT_DEST = ROOT / "secrets/datasets/station-events"
ID_PATTERN = re.compile(r"^([0-9]{8}T[0-9]{6}Z)-(hit|near|miss)-([0-9]+)$")
HEX64 = re.compile(r"^[0-9a-fA-F]{64}$")
SAFE_ID = re.compile(r"^[0-9A-Za-z:._-]+$")


def utc_now():
    return datetime.now(timezone.utc)


def stamp(moment=None):
    return (moment or utc_now()).strftime("%Y%m%dT%H%M%SZ")


def iso_utc(moment):
    return moment.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path, chunk=1 << 20):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        while True:
            block = handle.read(chunk)
            if not block:
                return digest.hexdigest()
            digest.update(block)


def parse_utc(text, field="utc"):
    if not isinstance(text, str):
        raise ValueError(f"{field} must be an ISO-8601 string")
    try:
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError as error:
        raise ValueError(f"{field} is not ISO-8601: {text!r}") from error
    if parsed.tzinfo is None:
        raise ValueError(f"{field} must carry a UTC offset")
    return parsed.astimezone(timezone.utc)


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def parse_sidecar(payload, name="<sidecar>"):
    """Validate one event sidecar against contract schema 1.

    Unknown schemas are rejected instead of being interpreted with a newer or
    older field meaning. Extra keys are preserved for forward-compatible data.
    """
    if not isinstance(payload, dict):
        raise ValueError(f"{name}: sidecar must be a JSON object")
    schema = payload.get("schema")
    _require(not isinstance(schema, bool) and schema == SCHEMA,
             f"{name}: unsupported schema {schema!r}; expected {SCHEMA}")
    ident = payload.get("id")
    _require(isinstance(ident, str) and ID_PATTERN.match(ident),
             f"{name}: invalid event id {ident!r}")
    kind = payload.get("kind")
    _require(kind in KINDS, f"{name}: unknown kind {kind!r}")
    _require(ID_PATTERN.match(ident).group(2) == kind,
             f"{name}: id kind does not match kind {kind!r}")
    parse_utc(payload.get("utc"), f"{name}: utc")
    _require(payload.get("sample_rate") == 16000,
             f"{name}: sample_rate must be 16000")
    for field in ("pre_ms", "post_ms"):
        value = payload.get(field)
        _require(isinstance(value, int) and not isinstance(value, bool) and value >= 0,
                 f"{name}: {field} must be a non-negative integer")
    peak = payload.get("score_peak")
    _require(isinstance(peak, (int, float)) and not isinstance(peak, bool) and 0 <= peak <= 1,
             f"{name}: score_peak must be in [0, 1]")
    cutoff = payload.get("cutoff")
    _require(isinstance(cutoff, (int, float)) and not isinstance(cutoff, bool) and 0 < cutoff <= 1,
             f"{name}: cutoff must be in (0, 1]")
    window = payload.get("window")
    _require(isinstance(window, int) and not isinstance(window, bool) and 1 <= window <= 50,
             f"{name}: window must be in 1..50")
    model = payload.get("model_sha256")
    _require(isinstance(model, str) and HEX64.match(model),
             f"{name}: model_sha256 must be 64 hex characters")
    _require(isinstance(payload.get("pulsar_version"), str) and payload["pulsar_version"],
             f"{name}: pulsar_version must be a non-empty string")
    _require(isinstance(payload.get("dry_run"), bool), f"{name}: dry_run must be boolean")
    outcome = payload.get("outcome")
    _require(outcome is None or outcome in OUTCOMES,
             f"{name}: unknown outcome {outcome!r}")
    stt = payload.get("stt_text")
    _require(stt is None or isinstance(stt, str), f"{name}: stt_text must be string or null")
    label = payload.get("label")
    _require(label is None or label in LABELS, f"{name}: unknown label {label!r}")
    source = payload.get("label_source")
    _require(source is None or source in ("user", "auto"),
             f"{name}: unknown label_source {source!r}")
    if payload.get("labeled_utc") is not None:
        parse_utc(payload["labeled_utc"], f"{name}: labeled_utc")
    return dict(payload)


def validate_wav(data, name="<wav>"):
    try:
        with wave.open(io.BytesIO(data), "rb") as handle:
            params = (handle.getnchannels(), handle.getsampwidth(), handle.getframerate())
            frames = handle.getnframes()
    except (wave.Error, EOFError) as error:
        raise ValueError(f"{name}: not a readable WAV: {error}") from error
    _require(params == (1, 2, 16000), f"{name}: expected PCM16 mono 16 kHz, got {params}")
    _require(frames > 0, f"{name}: empty recording")
    return frames / 16000


def wav_seconds(path):
    with wave.open(str(path), "rb") as handle:
        return handle.getnframes() / handle.getframerate()


def parse_stats(payload, name="stats.json"):
    if not isinstance(payload, dict):
        raise ValueError(f"{name}: must be a JSON object")
    schema = payload.get("schema")
    _require(not isinstance(schema, bool) and schema == SCHEMA,
             f"{name}: unsupported schema {schema!r}")
    parse_utc(payload.get("since_utc"), f"{name}: since_utc")
    for field in ("listening_seconds", "dry_run_listening_seconds"):
        value = payload.get(field)
        _require(isinstance(value, (int, float)) and not isinstance(value, bool) and value >= 0,
                 f"{name}: {field} must be a non-negative number")
    model = payload.get("model_sha256")
    _require(isinstance(model, str) and HEX64.match(model),
             f"{name}: model_sha256 must be 64 hex characters")
    for field in ("hits", "near", "miss", "dropped"):
        if field in payload:
            value = payload[field]
            _require(isinstance(value, int) and not isinstance(value, bool) and value >= 0,
                     f"{name}: {field} must be a non-negative integer")
    return payload


# --- local snapshots -------------------------------------------------------

def snapshot_dirs(root):
    root = Path(root)
    if not root.exists():
        raise FileNotFoundError(f"events directory does not exist: {root}")
    if (root / "snapshot.json").is_file():
        return [root]
    return sorted(path for path in root.iterdir()
                  if path.is_dir() and (path / "snapshot.json").is_file())


def read_sums(snapshot):
    path = Path(snapshot) / "SHA256SUMS"
    if not path.is_file():
        return {}
    result = {}
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        digest, _, name = line.partition("  ")
        if not name:
            continue
        result[name.lstrip("*")] = digest.strip()
    return result


def scan_events(root, verify_sums=True):
    """Return every locally stored event; snapshots are immutable, labels are not."""
    records = []
    for snapshot in snapshot_dirs(root):
        sums = read_sums(snapshot)
        for path in sorted(snapshot.glob("*.json")):
            if path.name == "snapshot.json":
                continue
            wav = path.with_suffix(".wav")
            if not wav.is_file():
                # A sidecar without audio is a partial pull; collect() never
                # commits one, so this cannot silently become a manifest entry.
                continue
            sidecar = parse_sidecar(json.loads(path.read_text()), name=str(path))
            digest = sums.get(wav.name)
            if digest is None:
                digest = sha256_file(wav)
            elif verify_sums and sha256_file(wav) != digest:
                raise ValueError(f"{wav}: SHA256SUMS mismatch")
            records.append({"id": sidecar["id"], "sidecar": sidecar, "wav": wav,
                            "sha256": digest, "seconds": wav_seconds(wav),
                            "snapshot": snapshot.name})
    ids = Counter(record["id"] for record in records)
    duplicates = sorted(ident for ident, count in ids.items() if count > 1)
    if duplicates:
        raise ValueError("Event present in more than one snapshot: " + ", ".join(duplicates))
    return records


def load_overrides(root):
    path = Path(root) / "labels.json"
    if not path.is_file():
        return {}
    payload = json.loads(path.read_text())
    if payload.get("schema") != SCHEMA or not isinstance(payload.get("labels"), dict):
        raise ValueError(f"{path}: unsupported overrides file")
    return payload["labels"]


def save_overrides(root, labels):
    path = Path(root) / "labels.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps({"schema": SCHEMA, "labels": labels},
                                    ensure_ascii=False, indent=2, sort_keys=True))
    os.replace(temporary, path)


def effective_label(event, overrides):
    override = overrides.get(event["id"])
    if override and override.get("label") in LABELS:
        return override["label"]
    return event["sidecar"].get("label")


def set_label(root, ident, label, note=None, source="local"):
    if label not in LABELS:
        raise ValueError(f"label must be one of {', '.join(LABELS)}")
    events = {event["id"]: event for event in scan_events(root)}
    if ident not in events:
        raise KeyError(f"no locally stored event {ident}; run collect first")
    labels = load_overrides(root)
    labels[ident] = {"label": label, "label_source": source,
                     "labeled_utc": iso_utc(utc_now()), "note": note}
    save_overrides(root, labels)
    return labels[ident]


def review_queue(root):
    overrides = load_overrides(root)
    rows = []
    for event in scan_events(root):
        sidecar = event["sidecar"]
        if sidecar["kind"] != "hit":
            continue
        if effective_label(event, overrides) is not None:
            continue
        if sidecar.get("outcome") not in SUSPECTED_OUTCOMES:
            continue
        rows.append({"id": event["id"], "utc": sidecar["utc"],
                     "outcome": sidecar.get("outcome"), "dry_run": sidecar["dry_run"],
                     "score_peak": sidecar["score_peak"], "seconds": event["seconds"],
                     "wav": str(event["wav"]), "snapshot": event["snapshot"]})
    return sorted(rows, key=lambda row: row["utc"], reverse=True)


# --- collect ---------------------------------------------------------------

def ssh_argv(host, key=None):
    argv = ["ssh", "-T"]
    if key:
        argv += ["-i", str(Path(key).expanduser())]
    argv += ["-o", "BatchMode=yes", host]
    return argv


def subprocess_runner(argv, stdin=None):
    completed = subprocess.run(argv, input=stdin, capture_output=True)
    return completed.returncode, completed.stdout, completed.stderr


def read_tar(data):
    """Return regular files from a tar stream, keyed by basename without ./."""
    files, skipped = {}, []
    with tarfile.open(fileobj=io.BytesIO(data), mode="r|") as archive:
        for member in archive:
            if not member.isfile():
                continue
            name = member.name
            if name.startswith("./"):
                name = name[2:]
            if not name or "/" in name or name.startswith("."):
                skipped.append(member.name)
                continue
            handle = archive.extractfile(member)
            if handle is None:
                continue
            files[name] = handle.read()
    return files, skipped


def _sidecar_files(files):
    """Pair WAV and sidecar by id; incomplete pairs are a transient device state."""
    sidecars, incomplete = {}, []
    for name, data in files.items():
        if not name.endswith(".json") or name == "stats.json":
            continue
        ident = name[:-len(".json")]
        wav_name = ident + ".wav"
        if wav_name not in files:
            incomplete.append(ident)
            continue
        sidecars[ident] = (data, files[wav_name])
    for name in files:
        if name.endswith(".wav") and name[:-len(".wav")] + ".json" not in files:
            incomplete.append(name[:-len(".wav")])
    return sidecars, sorted(set(incomplete))


def write_sums(directory, names):
    lines = []
    for name in sorted(names):
        lines.append(f"{sha256_file(Path(directory) / name)}  {name}")
    (Path(directory) / "SHA256SUMS").write_text("\n".join(lines) + "\n")
    return lines


def _unique_dir(parent, base):
    candidate = Path(parent) / base
    index = 2
    while candidate.exists():
        # A plain "-2" suffix would sort before the unsuffixed name.
        candidate = Path(parent) / f"{base}{index:02d}"
        index += 1
    return candidate


def _unique_file(directory, base):
    path = Path(directory) / base
    index = 2
    while path.exists():
        path = Path(directory) / f"{base[:-5]}{index:02d}.json"
        index += 1
    return path


def prune_script(idents):
    lines = ["set -e", f"cd {shlex.quote(REMOTE_DIR)}"]
    for ident in sorted(idents):
        if not SAFE_ID.match(ident):
            raise ValueError(f"refusing to build a pruner for unsafe id {ident!r}")
        lines.append(f"rm -f ./{ident}.wav ./{ident}.json")
    return "\n".join(lines) + "\n"


def collect(host, dest=DEFAULT_DEST, key=None, runner=subprocess_runner,
            moment=None, prune_labeled=False):
    """Pull new events into an immutable snapshot; never deletes by default."""
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True, mode=0o700)
    existing = {event["id"]: event for event in scan_events(dest)} if snapshot_dirs(dest) else {}
    if existing:
        known = {event["id"]: event["sha256"] for event in existing.values()}
    else:
        known = {}
    moment = moment or utc_now()
    snapshot_stamp = stamp(moment)
    code, raw, error = runner(ssh_argv(host, key) + [f"tar -C {shlex.quote(REMOTE_DIR)} -cf - ."])
    if code:
        raise RuntimeError("remote tar failed: " + error.decode(errors="replace").strip()[:500])
    files, skipped_names = read_tar(raw)
    stats_payload = None
    if "stats.json" in files:
        stats_payload = parse_stats(json.loads(files["stats.json"].decode()),
                                    name="stats.json")
    sidecars, incomplete = _sidecar_files(files)

    accepted, already, conflicts = {}, [], []
    for ident in sorted(sidecars):
        sidecar_raw, wav_raw = sidecars[ident]
        sidecar = parse_sidecar(json.loads(sidecar_raw.decode()), name=ident + ".json")
        seconds = validate_wav(wav_raw, name=ident + ".wav")
        digest = sha256_bytes(wav_raw)
        if ident in known:
            if known[ident] == digest:
                already.append(ident)
            else:
                conflicts.append(ident)
            continue
        accepted[ident] = {"sidecar": sidecar, "sidecar_raw": sidecar_raw,
                           "wav_raw": wav_raw, "sha256": digest, "seconds": seconds}
    if conflicts:
        raise ValueError("Device reused event id with different audio: " + ", ".join(conflicts))

    snapshot = None
    if accepted:
        incoming = dest / f".incoming-{snapshot_stamp}-{os.getpid()}"
        if incoming.exists():
            raise RuntimeError(f"stale incoming directory: {incoming}")
        incoming.mkdir(mode=0o700)
        try:
            for ident in sorted(accepted):
                event = accepted[ident]
                (incoming / (ident + ".wav")).write_bytes(event["wav_raw"])
                (incoming / (ident + ".json")).write_bytes(event["sidecar_raw"])
            sums = write_sums(incoming, [name for ident in accepted
                                         for name in (ident + ".wav", ident + ".json")])
            metadata = {"schema": SCHEMA, "created_utc": iso_utc(moment),
                        "host": host, "source": REMOTE_DIR,
                        "events": len(accepted),
                        "by_kind": dict(Counter(event["sidecar"]["kind"]
                                               for event in accepted.values())),
                        "skipped_existing": len(already),
                        "incomplete_on_device": incomplete,
                        "skipped_entries": skipped_names,
                        "files": sums}
            (incoming / "snapshot.json").write_text(
                json.dumps(metadata, ensure_ascii=False, indent=2, sort_keys=True))
            snapshot = _unique_dir(dest, snapshot_stamp)
            os.replace(incoming, snapshot)
        except Exception:
            if incoming.exists():
                for child in incoming.iterdir():
                    child.unlink()
                incoming.rmdir()
            raise

    stats_file = None
    if stats_payload is not None:
        stats_dir = dest / "stats"
        stats_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        stats_file = _unique_file(stats_dir, snapshot_stamp + ".json")
        stats_file.write_text(json.dumps(stats_payload, ensure_ascii=False,
                                         indent=2, sort_keys=True))

    pruned = []
    if prune_labeled:
        overrides = load_overrides(dest)
        local = scan_events(dest)
        labeled = {event["id"] for event in local if effective_label(event, overrides)}
        if labeled:
            code, _, error = runner(ssh_argv(host, key) + ["sh -s"],
                                    prune_script(labeled).encode())
            if code:
                raise RuntimeError("remote prune failed: "
                                   + error.decode(errors="replace").strip()[:500])
            pruned = sorted(labeled)

    summary = {"snapshot": str(snapshot) if snapshot else None,
               "new_events": len(accepted), "skipped_existing": len(already),
               "incomplete_on_device": incomplete, "stats": str(stats_file) if stats_file else None,
               "pruned": pruned}
    if not accepted and stats_file is None and not pruned:
        print("No new wake events and no stats.json on the device.")
    return summary


# --- report ----------------------------------------------------------------

def load_stats_history(root):
    root = Path(root)
    stats_dir = root if (root / "snapshot.json").is_file() else root / "stats"
    if stats_dir.is_dir() and (stats_dir / "snapshot.json").is_file():
        stats_dir = stats_dir.parent / "stats"
    if not stats_dir.is_dir():
        return []
    rows = []
    for path in sorted(stats_dir.glob("*.json")):
        payload = json.loads(path.read_text())
        parse_stats(payload, name=str(path))
        rows.append({"stamp": path.stem, "stats": payload})
    return rows


def _cumulative(rows, field):
    """Sum cumulative counters per model.

    Counters survive restarts, so consecutive snapshots are differenced. A
    lower value means the device reset its epoch; that value is added whole.
    """
    groups = defaultdict(list)
    for row in rows:
        groups[row["stats"]["model_sha256"]].append(float(row["stats"][field]))
    total = 0.0
    for values in groups.values():
        total += values[0]
        for previous, current in zip(values, values[1:]):
            total += current - previous if current >= previous else current
    return total


def listening_totals(stats_rows):
    per_model = {}
    models = {row["stats"]["model_sha256"] for row in stats_rows}
    for model in models:
        rows = [row for row in stats_rows if row["stats"]["model_sha256"] == model]
        per_model[model] = {
            "listening_hours": _cumulative(rows, "listening_seconds") / 3600,
            "dry_run_hours": _cumulative(rows, "dry_run_listening_seconds") / 3600,
            "stats_samples": len(rows),
        }
    return per_model


def _rate(count, hours):
    return round(count / hours, 4) if hours and hours > 0 else None


def build_report(root):
    events = scan_events(root)
    overrides = load_overrides(root)
    stats_rows = load_stats_history(root)
    listening = listening_totals(stats_rows)
    per_model = {}
    for model, hours in listening.items():
        per_model[model] = dict(hours)

    def row(model):
        return per_model.setdefault(model, {"listening_hours": 0.0, "dry_run_hours": 0.0,
                                            "stats_samples": 0})

    totals = Counter()
    labels = Counter()
    outcomes = Counter()
    for event in events:
        sidecar = event["sidecar"]
        label = effective_label(event, overrides)
        model = sidecar["model_sha256"]
        bucket = row(model)
        for key in ("events", "hits", "near", "miss", "confirmed_false",
                    "suspected_false", "labeled_true", "labeled_missed",
                    "dry_run_hits", "dry_run_confirmed_false"):
            bucket.setdefault(key, 0)
        totals["events"] += 1
        bucket["events"] += 1
        totals[sidecar["kind"]] += 1
        if sidecar["kind"] == "hit":
            bucket["hits"] += 1
        elif sidecar["kind"] == "near":
            bucket["near"] += 1
        else:
            bucket["miss"] += 1
        labels[label or "unlabeled"] += 1
        outcomes[sidecar.get("outcome") or "unknown"] += 1
        if sidecar["kind"] == "hit":
            if label == "false":
                bucket["confirmed_false"] += 1
            if label is None and sidecar.get("outcome") in SUSPECTED_OUTCOMES:
                bucket["suspected_false"] += 1
            if sidecar["dry_run"]:
                bucket["dry_run_hits"] += 1
                if label == "false":
                    bucket["dry_run_confirmed_false"] += 1
        if label == "true":
            bucket["labeled_true"] += 1
        if label == "missed":
            bucket["labeled_missed"] += 1

    for bucket in per_model.values():
        for key in ("events", "hits", "near", "miss", "confirmed_false",
                    "suspected_false", "labeled_true", "labeled_missed",
                    "dry_run_hits", "dry_run_confirmed_false"):
            bucket.setdefault(key, 0)
        bucket["confirmed_false_per_hour"] = _rate(bucket["confirmed_false"],
                                                   bucket["listening_hours"])
        bucket["suspected_false_per_hour"] = _rate(bucket["suspected_false"],
                                                   bucket["listening_hours"])
        bucket["near_per_hour"] = _rate(bucket["near"], bucket["listening_hours"])
        bucket["dry_run_false_per_hour"] = _rate(bucket["dry_run_confirmed_false"],
                                                bucket["dry_run_hours"])
        for key in ("listening_hours", "dry_run_hours"):
            bucket[key] = round(bucket[key], 4)

    combined = {"events": totals["events"], "hits": totals["hit"], "near": totals["near"],
                "miss": totals["miss"],
                "listening_hours": round(sum(b["listening_hours"] for b in per_model.values()), 4),
                "dry_run_hours": round(sum(b["dry_run_hours"] for b in per_model.values()), 4)}
    combined["confirmed_false"] = sum(b["confirmed_false"] for b in per_model.values())
    combined["suspected_false"] = sum(b["suspected_false"] for b in per_model.values())
    combined["confirmed_false_per_hour"] = _rate(combined["confirmed_false"],
                                                 combined["listening_hours"])
    combined["suspected_false_per_hour"] = _rate(combined["suspected_false"],
                                                 combined["listening_hours"])
    combined["near_per_hour"] = _rate(combined["near"], combined["listening_hours"])
    dry_false = sum(b["dry_run_confirmed_false"] for b in per_model.values())
    combined["dry_run_confirmed_false"] = dry_false
    combined["dry_run_false_per_hour"] = _rate(dry_false, combined["dry_run_hours"])

    dates = sorted(event["sidecar"]["utc"] for event in events)
    return {"schema": SCHEMA, "events_total": len(events),
            "range": {"from": dates[0], "to": dates[-1]} if dates else None,
            "snapshots": len(snapshot_dirs(root)),
            "stats_samples": len(stats_rows),
            "labels": dict(labels), "outcomes": dict(outcomes),
            "combined": combined,
            "models": dict(sorted(per_model.items()))}


def report_text(report):
    combined = report["combined"]
    def show(value):
        return "n/a" if value is None else f"{value:.2f}"
    lines = [f"Wake events: {report['events_total']} "
             f"(hits {combined['hits']}, near {combined['near']}, miss {combined['miss']})",
             f"Listening: {combined['listening_hours']:.2f} h"
             f" (dry run {combined['dry_run_hours']:.2f} h) from {report['stats_samples']} stats snapshots",
             f"Confirmed false hits: {combined['confirmed_false']} = "
             f"{show(combined['confirmed_false_per_hour'])} / h",
             f"Suspected false (empty STT/cancelled, unlabeled): {combined['suspected_false']} = "
             f"{show(combined['suspected_false_per_hour'])} / h",
             f"Near misses: {combined['near']} = {show(combined['near_per_hour'])} / h",
             f"Dry-run confirmed false: {combined['dry_run_confirmed_false']} = "
             f"{show(combined['dry_run_false_per_hour'])} / h"]
    for model, bucket in report["models"].items():
        lines.append(f"  model {model[:12]}…: {bucket['events']} events, "
                     f"{bucket['listening_hours']:.2f} h, "
                     f"false {show(bucket['confirmed_false_per_hour'])} / h, "
                     f"suspected {show(bucket['suspected_false_per_hour'])} / h, "
                     f"near {show(bucket['near_per_hour'])} / h")
    return "\n".join(lines)


# --- manifest --------------------------------------------------------------

def assign_split(day, seed="station-events", train=0.70, val=0.15):
    """Deterministic UTC-day split so neighbouring clips never cross splits."""
    if not day or not isinstance(day, str):
        raise ValueError("day must be a UTC yyyy-mm-dd string")
    total = train + val
    if not (0 < train < 1 and 0 <= val < 1 and total <= 1):
        raise ValueError("split fractions must be positive and sum to at most 1")
    bucket = int(hashlib.sha256(f"{seed}|{day}".encode()).hexdigest()[:8], 16) % 10000
    if bucket < round(train * 10000):
        return "train"
    if bucket < round(total * 10000):
        return "val"
    return "test"


def _negative_entry(event, split, label, origin, seed):
    sidecar = event["sidecar"]
    return {
        "id": event["id"], "source_split": split, "block": "background",
        "sha256": event["sha256"], "seconds": round(event["seconds"], 3),
        "interrupted": False, "effective_seconds": round(event["seconds"], 3),
        "partition": split, "hard_negative": True, "station_false_positive": True,
        "station_origin": origin, "station_kind": sidecar["kind"],
        "station_label": label, "station_outcome": sidecar.get("outcome"),
        "station_day": sidecar["utc"][:10], "model_sha256": sidecar["model_sha256"],
        "dry_run": sidecar["dry_run"], "exclude_intervals": [],
        "split_seed": seed,
    }


def _positive_candidate(event, split):
    sidecar = event["sidecar"]
    return {
        "id": event["id"], "source_split": split, "block": "station",
        "sha256": event["sha256"], "seconds": round(event["seconds"], 3),
        "interrupted": False, "review_only": True, "targets": [],
        # Word position inside a 4-6 s clip is unknown; never auto-positive.
        "review_regions": [[0.0, round(event["seconds"], 3)]],
        "station_label": sidecar.get("label"), "station_kind": sidecar["kind"],
        "station_outcome": sidecar.get("outcome"),
        "model_sha256": sidecar["model_sha256"],
    }


NEAR_FOLLOWUP_SECONDS = 15


def followed_by(moment, followups, seconds=NEAR_FOLLOWUP_SECONDS):
    """True when a hit/miss event starts within `seconds` after `moment`."""
    import bisect
    index = bisect.bisect_right(followups, moment)
    return index < len(followups) and (followups[index] - moment).total_seconds() <= seconds


def to_manifest(root, output, include_suspected=False, near_negatives=True,
                seed="station-events", train=0.70, val=0.15, copy_audio=True,
                force=False):
    output = Path(output)
    if output.exists() and not force:
        raise FileExistsError(f"{output} exists; use --force or a new run directory")
    events = scan_events(root)
    overrides = load_overrides(root)
    records, skipped, counts = [], [], Counter()
    # A near-miss followed by a hit or a "не услышала" press is likely a quiet real
    # wake word the user repeated; it must not become a negative unless labelled false.
    followups = sorted(parse_utc(item["sidecar"]["utc"]) for item in events
                       if item["sidecar"]["kind"] in ("hit", "miss"))
    for event in sorted(events, key=lambda item: item["id"]):
        sidecar = event["sidecar"]
        label = effective_label(event, overrides)
        day = sidecar["utc"][:10]
        split = assign_split(day, seed, train, val)
        if sidecar["kind"] in ("hit", "near"):
            if label == "false":
                records.append(_negative_entry(event, split, label, "confirmed_false", seed))
                counts["confirmed_false"] += 1
            elif sidecar["kind"] == "near" and label is None and near_negatives:
                if followed_by(parse_utc(sidecar["utc"]), followups):
                    skipped.append((event["id"], "near-miss followed by a hit/miss; label it"))
                    counts["near_followed_skipped"] += 1
                else:
                    records.append(_negative_entry(event, split, label, "near_miss", seed))
                    counts["near_miss"] += 1
            elif (sidecar["kind"] == "hit" and label is None
                  and sidecar.get("outcome") in SUSPECTED_OUTCOMES):
                if include_suspected:
                    records.append(_negative_entry(event, split, label, "suspected", seed))
                    counts["suspected_negative"] += 1
                else:
                    skipped.append((event["id"], "suspected false needs --include-suspected"))
                    counts["suspected_skipped"] += 1
            elif label in ("true", "missed"):
                records.append(_positive_candidate(event, split))
                counts["positive_candidate"] += 1
            else:
                skipped.append((event["id"], "no usable label"))
                counts["skipped"] += 1
        else:
            if label == "missed":
                records.append(_positive_candidate(event, split))
                counts["positive_candidate"] += 1
            else:
                skipped.append((event["id"], "miss without label=missed"))
                counts["skipped"] += 1

    recordings = output.parent / "recordings"
    if copy_audio:
        recordings.mkdir(parents=True, exist_ok=True, mode=0o700)
        selected = {record["id"] for record in records}
        for event in events:
            if event["id"] not in selected:
                continue
            destination = recordings / (event["id"] + ".wav")
            if destination.exists():
                if sha256_file(destination) != event["sha256"]:
                    raise ValueError(f"{destination}: existing audio differs from snapshot")
                continue
            temporary = destination.with_suffix(".wav.tmp")
            temporary.write_bytes(event["wav"].read_bytes())
            os.replace(temporary, destination)

    manifest = {
        "schema": SCHEMA,
        "annotation": ("Station wake events: user-confirmed false hits and near misses are "
                       "hard negatives; labelled positives are review-only regions"),
        "test_is_not_for_selection": True,
        "generated_utc": iso_utc(utc_now()),
        "station_events": {
            "source": str(root), "snapshots": [directory.name for directory in snapshot_dirs(root)],
            "seed": seed, "split": {"train": train, "val": val, "test": round(1 - train - val, 4)},
            "include_suspected": include_suspected, "near_negatives": near_negatives,
            "counts": dict(counts), "skipped": skipped,
            "recordings": str(recordings) if copy_audio else None,
        },
        "records": records,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_text(json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True))
    os.replace(temporary, output)
    return manifest


# --- cli -------------------------------------------------------------------

def cmd_collect(args):
    summary = collect(args.host, args.dest, args.ssh_key, moment=None,
                      prune_labeled=args.prune_labeled)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


def cmd_report(args):
    result = build_report(args.events)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(report_text(result))


def cmd_label(args):
    record = set_label(args.events, args.id, args.label, note=args.note)
    print(json.dumps({"id": args.id, **record}, ensure_ascii=False))


def cmd_review(args):
    rows = review_queue(args.events)
    if args.json:
        print(json.dumps(rows, ensure_ascii=False, indent=2))
        return
    if not rows:
        print("No unlabeled suspected false hits.")
        return
    for row in rows:
        print(f"{row['id']}  {row['utc']}  outcome={row['outcome']}  "
              f"peak={row['score_peak']}  {row['wav']}")


def cmd_manifest(args):
    manifest = to_manifest(args.events, args.output, args.include_suspected,
                           not args.no_near_negatives, args.seed, args.train, args.val,
                           not args.no_copy_audio, args.force)
    print(json.dumps({"output": str(args.output),
                      "records": len(manifest["records"]),
                      "counts": manifest["station_events"]["counts"]},
                     ensure_ascii=False, indent=2))


def cmd_calibrate(args):
    path = Path(__file__).with_name("calibrate.py")
    spec = importlib.util.spec_from_file_location("station_calibrate", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.main(args.calibrate_argv or [])


def main(argv=None):
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    pull = sub.add_parser("collect", help="Pull new events into an immutable snapshot")
    pull.add_argument("--host", required=True)
    pull.add_argument("--ssh-key", type=Path)
    pull.add_argument("--dest", type=Path, default=DEFAULT_DEST)
    pull.add_argument("--prune-labeled", action="store_true",
                      help="Delete only locally verified, labelled files from the device")
    pull.set_defaults(handler=cmd_collect)

    report = sub.add_parser("report", help="Counts and false positives per listening hour")
    report.add_argument("--events", required=True, type=Path)
    report.add_argument("--output", type=Path)
    report.add_argument("--json", action="store_true")
    report.set_defaults(handler=cmd_report)

    label = sub.add_parser("label", help="Store a local relabel; originals stay immutable")
    label.add_argument("--events", required=True, type=Path)
    label.add_argument("--id", required=True)
    label.add_argument("--label", required=True, choices=list(LABELS))
    label.add_argument("--note")
    label.set_defaults(handler=cmd_label)

    review = sub.add_parser("review", help="List unlabeled suspected false hits")
    review.add_argument("--events", required=True, type=Path)
    review.add_argument("--json", action="store_true")
    review.set_defaults(handler=cmd_review)

    manifest = sub.add_parser("to-manifest", help="Build a manifest for finetune --extra-manifest")
    manifest.add_argument("--events", required=True, type=Path)
    manifest.add_argument("--output", required=True, type=Path)
    manifest.add_argument("--include-suspected", action="store_true",
                          help="Treat unlabeled empty-STT/cancelled hits as hard negatives")
    manifest.add_argument("--no-near-negatives", action="store_true")
    manifest.add_argument("--seed", default="station-events")
    manifest.add_argument("--train", type=float, default=0.70)
    manifest.add_argument("--val", type=float, default=0.15)
    manifest.add_argument("--no-copy-audio", action="store_true")
    manifest.add_argument("--force", action="store_true")
    manifest.set_defaults(handler=cmd_manifest)

    calibrate = sub.add_parser("calibrate", help="Forward remaining options to calibrate.py")
    calibrate.add_argument("calibrate_argv", nargs=argparse.REMAINDER)
    calibrate.set_defaults(handler=cmd_calibrate)

    args = parser.parse_args(argv)
    return args.handler(args)


if __name__ == "__main__":
    sys.exit(main())
