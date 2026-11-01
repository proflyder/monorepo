#!/usr/bin/env python3
"""Calibrate cut-off and sliding window from streaming replay scores.

Reuses `evaluate.replay` so the streaming simulation is not duplicated. Runs
every (cutoff, window) pair on positive recordings and on confirmed station
false hits, reports recall vs false positives per listening hour and
recommends the lowest-FP setting that preserves the current recall (0.9/10).

The recommended pair can be written into the model manifest JSON, which Pulsar
reads as `micro.probability_cutoff` / `micro.sliding_window_size`.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[4]
DEFAULT_CUTOFFS = (0.5, 0.6, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95)
DEFAULT_WINDOWS = (5, 10)


def _load(name, filename):
    path = Path(__file__).with_name(filename)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parse_floats(text):
    values = [float(item) for item in str(text).split(",") if item.strip()]
    if not values or any(not 0 < value <= 1 for value in values):
        raise ValueError("cutoffs must be comma-separated values in (0, 1]")
    return tuple(values)


def parse_ints(text):
    values = [int(item) for item in str(text).split(",") if item.strip()]
    if not values or any(not 1 <= value <= 50 for value in values):
        raise ValueError("windows must be comma-separated integers in 1..50")
    return tuple(values)


def load_pcm(path):
    import numpy as np
    import wave
    with wave.open(str(path), "rb") as handle:
        if (handle.getframerate(), handle.getnchannels(), handle.getsampwidth()) != (16000, 1, 2):
            raise ValueError(f"{path}: expected PCM16 mono 16 kHz")
        return np.frombuffer(handle.readframes(handle.getnframes()), dtype="<i2")


def collect_wavs(directory, skip_excluded=True, skip_ids=frozenset()):
    directory = Path(directory)
    paths = sorted(directory.glob("*.wav"))
    if not paths:
        raise ValueError(f"no WAV files in {directory}")
    result = []
    for path in paths:
        if path.name.startswith(".") or path.stem in skip_ids:
            continue
        meta = path.with_suffix(".json")
        if meta.is_file():
            try:
                described = json.loads(meta.read_text())
            except (ValueError, OSError):
                described = {}
            if skip_excluded and described.get("excluded"):
                continue
            # Background takes are not positives; including them would make
            # recall meaningless.
            if described.get("block") == "background":
                continue
        result.append(path)
    return result


def negative_paths(events_root, include_suspected=False):
    """Confirmed false hits only; near misses are a separate metric."""
    station = _load("station_events_for_calibrate", "station_events.py")
    overrides = station.load_overrides(events_root)
    paths = []
    for event in station.scan_events(events_root):
        sidecar = event["sidecar"]
        label = station.effective_label(event, overrides)
        if sidecar["kind"] != "hit":
            continue
        if label == "false":
            paths.append(event["wav"])
        elif (include_suspected and label is None
              and sidecar.get("outcome") in station.SUSPECTED_OUTCOMES):
            paths.append(event["wav"])
    return paths


def build_table(positives, negatives, replay, cutoffs=DEFAULT_CUTOFFS,
                windows=DEFAULT_WINDOWS, listening_hours=0.0):
    if not positives:
        raise ValueError("at least one positive recording is required")
    if listening_hours < 0:
        raise ValueError("listening hours must not be negative")
    rows = []
    for cutoff in cutoffs:
        for window in windows:
            detected = 0
            for pcm in positives:
                if replay(pcm, cutoff, window)["hits"]:
                    detected += 1
            false_clips = 0
            false_hits = 0
            for pcm in negatives:
                hits = replay(pcm, cutoff, window)["hits"]
                if hits:
                    false_clips += 1
                    false_hits += len(hits)
            rows.append({
                "cutoff": round(float(cutoff), 4), "window": int(window),
                "positives": len(positives), "positives_detected": detected,
                "recall": round(detected / len(positives), 4),
                "negatives": len(negatives), "false_clips": false_clips,
                "false_hits": false_hits,
                "false_per_hour": (round(false_clips / listening_hours, 4)
                                   if listening_hours > 0 else None),
            })
    return {"listening_hours": round(float(listening_hours), 4),
            "positives": len(positives), "negatives": len(negatives),
            "rows": rows}


def recommend(table, cutoff=0.9, window=10):
    rows = table["rows"]
    baseline = next((row for row in rows
                     if abs(row["cutoff"] - cutoff) < 1e-9 and row["window"] == window), None)
    if baseline is None:
        raise ValueError(f"baseline ({cutoff}, {window}) is missing from the table")
    eligible = [row for row in rows if row["recall"] >= baseline["recall"] - 1e-9]
    # Lowest false-positive rate first; ties prefer the more conservative
    # detector (higher cutoff, longer smoothing).
    best = min(eligible, key=lambda row: (
        row["false_per_hour"] if row["false_per_hour"] is not None else float("inf"),
        -row["recall"], -row["cutoff"], -row["window"]))
    return {"cutoff": best["cutoff"], "window": best["window"],
            "recall": best["recall"], "false_per_hour": best["false_per_hour"],
            "baseline": {"cutoff": baseline["cutoff"], "window": baseline["window"],
                         "recall": baseline["recall"],
                         "false_per_hour": baseline["false_per_hour"]}}


def validate_threshold(cutoff, window):
    if isinstance(cutoff, bool) or not isinstance(cutoff, (int, float)) or not 0 < cutoff < 1:
        raise ValueError("probability_cutoff must be in (0, 1)")
    if isinstance(window, bool) or not isinstance(window, int) or not 1 <= window <= 50:
        raise ValueError("sliding_window_size must be an integer in 1..50")


def write_manifest(path, cutoff, window):
    """Merge calibrated thresholds into an existing model manifest JSON."""
    validate_threshold(cutoff, window)
    path = Path(path)
    payload = {}
    if path.is_file():
        payload = json.loads(path.read_text())
        if not isinstance(payload, dict):
            raise ValueError(f"{path}: manifest must be a JSON object")
    micro = payload.get("micro")
    if micro is not None and not isinstance(micro, dict):
        raise ValueError(f"{path}: micro must be a JSON object")
    micro = dict(micro or {})
    micro["probability_cutoff"] = float(cutoff)
    micro["sliding_window_size"] = int(window)
    payload["micro"] = micro
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix="manifest-", suffix=".json", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as handle:
            handle.write(json.dumps(payload, ensure_ascii=False, indent=2))
        os.replace(temporary, path)
    except Exception:
        if os.path.exists(temporary):
            os.unlink(temporary)
        raise
    return payload


def table_text(table, recommendation):
    lines = [f"Positives {table['positives']}, confirmed-false negatives "
             f"{table['negatives']}, listening {table['listening_hours']:.2f} h",
             "cutoff  window  recall  false clips  FP/h"]
    for row in table["rows"]:
        fp = "n/a" if row["false_per_hour"] is None else f"{row['false_per_hour']:.2f}"
        mark = ""
        if abs(row["cutoff"] - recommendation["baseline"]["cutoff"]) < 1e-9 \
                and row["window"] == recommendation["baseline"]["window"]:
            mark = "  <- baseline"
        lines.append(f"{row['cutoff']:<7.2f} {row['window']:<7d} "
                     f"{row['recall']:.3f}   {row['false_clips']:<11d} {fp}{mark}")
    best = recommendation
    lines.append(f"Recommendation: cutoff {best['cutoff']:.2f}, window {best['window']} "
                 f"(recall {best['recall']:.3f} vs baseline {best['baseline']['recall']:.3f})")
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--positives", required=True, type=Path,
                        help="Directory with positive 16 kHz PCM16 recordings")
    parser.add_argument("--events", required=True, type=Path,
                        help="Collected station events (confirmed false hits are negatives)")
    parser.add_argument("--listening-hours", type=float,
                        help="Override the total from stats.json snapshots")
    parser.add_argument("--include-suspected", action="store_true")
    parser.add_argument("--cutoffs", type=parse_floats, default=DEFAULT_CUTOFFS)
    parser.add_argument("--windows", type=parse_ints, default=DEFAULT_WINDOWS)
    parser.add_argument("--baseline-cutoff", type=float, default=0.9)
    parser.add_argument("--baseline-window", type=int, default=10)
    parser.add_argument("--output", type=Path, help="Write the full table as JSON")
    parser.add_argument("--manifest", type=Path,
                        help="Write micro.probability_cutoff/sliding_window_size here")
    args = parser.parse_args(argv)

    station = _load("station_events_for_calibrate", "station_events.py")
    evaluate = _load("station_evaluate", "evaluate.py")
    station_ids = {event["id"] for event in station.scan_events(args.events)}
    positives = [load_pcm(path) for path in collect_wavs(args.positives, skip_ids=station_ids)]
    negatives = [load_pcm(path) for path in negative_paths(args.events, args.include_suspected)]
    hours = args.listening_hours
    if hours is None:
        hours = station.build_report(args.events)["combined"]["listening_hours"]
    table = build_table(positives, negatives,
                        lambda pcm, cutoff, window: evaluate.replay(
                            args.model, pcm, cutoff, window),
                        args.cutoffs, args.windows, hours)
    recommendation = recommend(table, args.baseline_cutoff, args.baseline_window)
    result = dict(table)
    result["recommendation"] = recommendation
    result["model"] = str(args.model)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2))
    if args.manifest:
        write_manifest(args.manifest, recommendation["cutoff"], recommendation["window"])
    print(table_text(table, recommendation))
    return 0


if __name__ == "__main__":
    import sys
    sys.exit(main())
