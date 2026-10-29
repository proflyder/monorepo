#!/usr/bin/env python3
"""Turn privately cached alignment into leakage-controlled training annotations.

ASR is an aid, not ground truth: unmatched speech regions are retained for review.
This script never edits recordings or changes their train/test labels.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import wave


def distance(a, b):
    row = list(range(len(b) + 1))
    for i, ca in enumerate(a):
        nxt = [i + 1]
        for j, cb in enumerate(b):
            nxt.append(min(nxt[-1] + 1, row[j + 1] + 1, row[j] + (ca != cb)))
        row = nxt
    return row[-1]


def target(word):
    text = re.sub("[^а-я]", "", word.lower().replace("ё", "е"))
    return len(text) >= 5 and distance(text, "рикардо") <= 2


def deduplicate(hits):
    result = []
    for h in sorted(hits, key=lambda h: h["start"]):
        if result and h["start"] < result[-1]["end"] and abs(h["end"] - result[-1]["end"]) < .4:
            if h.get("p", 0) > result[-1].get("p", 0):
                result[-1] = h
        else:
            result.append(h)
    return result


def prepare(recordings, alignments, full_asr):
    entries = []
    for path in sorted(recordings.glob("*.json")):
        if path.name.startswith("."):
            continue
        meta = json.loads(path.read_text())
        if meta.get("excluded"):
            continue
        wav_path = path.with_suffix(".wav")
        with wave.open(str(wav_path)) as wav:
            if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, 16000):
                raise ValueError("Unexpected WAV format")
            duration = wav.getnframes() / 16000
            if wav.getnframes() != meta["frames"]:
                raise ValueError("WAV frame count mismatch")
        if hashlib.sha256(wav_path.read_bytes()).hexdigest() != meta["sha256"]:
            raise ValueError("Recording changed since capture")
        entry = {"id": meta["id"], "source_split": meta["split"], "block": meta["block"],
                 "sha256": meta["sha256"], "seconds": duration, "interrupted": meta.get("interrupted", False)}
        if meta["block"] == "background":
            asr = json.loads((full_asr / path.name).read_text())
            words = [w for s in asr["segments"] for w in s.get("words", []) if target(w["word"])]
            entry["exclude_intervals"] = [[max(0, w["start"] - 2.5), min(duration, w["end"] + 2.5)] for w in words]
            entry["effective_seconds"] = max(0, duration - (1 if meta.get("interrupted") else 0))
        else:
            aligned = json.loads((alignments / path.name).read_text())
            regions = aligned["candidates"]
            hits = deduplicate([h for c in regions for h in c["hits"]
                                if h.get("p", 0) >= .2 and .15 <= h["end"] - h["start"] <= 1.8])
            cut = duration * .8
            for hit in hits:
                hit["partition"] = ("test" if meta["split"] == "test" else
                    "train" if hit["end"] < cut - 1.3 else
                    "val" if hit["start"] > cut + 1.3 else "guard")
            entry["targets"] = hits
            entry["review_regions"] = [[c["start"], c["end"]] for c in regions if not c["hits"]]
            entry["partition_boundary"] = cut
        entries.append(entry)
    backgrounds = sorted((e for e in entries if e["block"] == "background"), key=lambda e: e["id"])
    # Entire separate takes for negative validation and final evaluation, never
    # random overlapping windows from the same recording in different splits.
    full = []
    seen = set()
    for e in backgrounds:
        if e["source_split"] != "test" and not e["interrupted"] and e["seconds"] >= 60 and e["sha256"] not in seen:
            full.append(e)
            seen.add(e["sha256"])
    if len(full) < 3:
        raise ValueError("Need three independent background takes for train/val/test")
    for e in backgrounds:
        e["partition"] = ("test" if e["source_split"] == "test" else
            "val" if e["sha256"] == full[0]["sha256"] else
            "test" if e["sha256"] == full[1]["sha256"] else "train")
    train_hashes = {e["sha256"] for e in entries if e["source_split"] == "train"}
    test_hashes = {e["sha256"] for e in entries if e["source_split"] == "test"}
    if train_hashes & test_hashes:
        raise ValueError("Identical recording present in both train and test")
    return {"schema": 1, "annotation": "ASR-assisted subset, unmatched regions require review",
            "test_is_not_for_selection": True, "records": entries}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ["recordings", "alignments", "asr", "output"]:
        p.add_argument("--" + name, required=True, type=Path)
    a = p.parse_args()
    manifest = prepare(a.recordings, a.alignments, a.asr)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(manifest, ensure_ascii=False, indent=2))
    from collections import Counter
    counts = Counter((e["block"], h["partition"]) for e in manifest["records"] for h in e.get("targets", []))
    print("Targets by block/partition:", dict(counts))
    print("Unverified speech regions:", sum(len(e.get("review_regions", [])) for e in manifest["records"]))


if __name__ == "__main__":
    main()
