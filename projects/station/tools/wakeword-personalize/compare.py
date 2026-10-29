#!/usr/bin/env python3
"""Compare a single held-out replay, keeping annotation coverage explicit."""
import argparse
import json
from pathlib import Path


def match_targets(targets, hits):
    used = set()
    for hit in hits:
        valid = [(abs(hit - t["end"]), i) for i, t in enumerate(targets)
                 if i not in used and t["start"] - .15 <= hit <= t["end"] + 1.5]
        if valid:
            used.add(min(valid)[1])
    return len(used)


def summarize(manifest, replay):
    records = {r["id"]: r for r in replay["records"]}
    result = {"model_sha256": replay["model_sha256"], "blocks": {},
              "background_test_events": 0, "background_test_seconds": 0,
              "hard_negative_test_events": 0, "hard_negative_test_seconds": 0,
              "annotation_scope": "Only ASR-assisted verified subset, not all spoken targets",
              "matching": "One hit per target, start-0.15s <= hit <= end+1.5s; nearest unmatched end"}
    for entry in manifest["records"]:
        observed = records[entry["id"]]
        if entry["block"] == "background":
            if entry["partition"] == "test":
                result["background_test_seconds"] += entry["seconds"]
                result["background_test_events"] += sum(
                    not any(a <= h <= b for a, b in entry["exclude_intervals"]) for h in observed["hits"])
                if entry.get("hard_negative"):
                    result["hard_negative_test_seconds"] += entry["seconds"]
                    result["hard_negative_test_events"] += sum(
                        not any(a <= h <= b for a, b in entry["exclude_intervals"]) for h in observed["hits"])
            continue
        if entry["source_split"] != "test":
            continue
        block = result["blocks"].setdefault(entry["block"], {
            "matched": 0, "verified_targets": 0,
            "unverified_regions": 0, "raw_detector_events": 0})
        block["matched"] += match_targets(entry["targets"], observed["hits"])
        block["verified_targets"] += len(entry["targets"])
        block["unverified_regions"] += len(entry["review_regions"])
        block["raw_detector_events"] += len(observed["hits"])
    result["matched"] = sum(x["matched"] for x in result["blocks"].values())
    result["verified_targets"] = sum(x["verified_targets"] for x in result["blocks"].values())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["manifest", "baseline", "candidate", "output"]:
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    result = {name: summarize(manifest, json.loads(getattr(args, name).read_text()))
              for name in ["baseline", "candidate"]}
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False))
    for name, report in result.items():
        print(name, report["matched"], "/", report["verified_targets"], "verified targets;",
              report["background_test_events"], "background events in", round(report["background_test_seconds"], 1), "seconds")


if __name__ == "__main__":
    main()
