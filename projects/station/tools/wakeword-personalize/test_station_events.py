import hashlib
import importlib.util
import io
import json
from datetime import datetime, timezone
from pathlib import Path
import tarfile
import tempfile
import unittest
import wave


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


station = _load("station_events_under_test", "station_events.py")
finetune = _load("finetune_under_test", "finetune.py")
calibrate = _load("calibrate_under_test", "calibrate.py")

MODEL = "a" * 64


def wav_bytes(seconds=4.0, value=1200):
    buffer = io.BytesIO()
    frames = int(seconds * 16000)
    with wave.open(buffer, "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(16000)
        handle.writeframes(value.to_bytes(2, "little", signed=True) * frames)
    return buffer.getvalue()


def sidecar(identity, kind="hit", utc="2026-11-01T10:15:00.123Z", **overrides):
    payload = {
        "schema": 1, "id": identity, "kind": kind, "utc": utc,
        "sample_rate": 16000, "pre_ms": 3000, "post_ms": 1000,
        "score_peak": 0.934, "cutoff": 0.9, "window": 10,
        "model_sha256": MODEL, "pulsar_version": "v0.5.1003",
        "dry_run": False, "outcome": None, "stt_text": None,
        "label": None, "label_source": None, "labeled_utc": None,
    }
    payload.update(overrides)
    return payload


def stats(listening=3600.0, dry_run=0.0, model=MODEL, since="2026-11-01T00:00:00Z"):
    return {"schema": 1, "since_utc": since, "listening_seconds": listening,
            "hits": 1, "near": 0, "miss": 0,
            "dry_run_listening_seconds": dry_run, "dropped": 0, "model_sha256": model}


def tar_bytes(files):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as archive:
        for name, data in files.items():
            info = tarfile.TarInfo("./" + name)
            info.size = len(data)
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def bundle(events, stats_payload=None):
    files = {}
    for ident, (payload, audio) in events.items():
        files[ident + ".json"] = json.dumps(payload).encode()
        files[ident + ".wav"] = audio
    if stats_payload is not None:
        files["stats.json"] = json.dumps(stats_payload).encode()
    return files


class Runner:
    def __init__(self, payload):
        self.payload = payload
        self.calls = []

    def __call__(self, argv, stdin=None):
        self.calls.append((list(argv), stdin))
        if argv[-1].startswith("tar -C"):
            return 0, self.payload, b""
        return 0, b"", b""


def pull(dest, files, **kwargs):
    runner = Runner(tar_bytes(files))
    summary = station.collect("192.0.2.13", dest, runner=runner, **kwargs)
    return runner, summary


class SidecarTests(unittest.TestCase):
    def test_accepts_contract_example(self):
        payload = station.parse_sidecar(sidecar("20261101T101500Z-hit-17"))
        self.assertEqual(payload["kind"], "hit")
        self.assertEqual(payload["schema"], 1)

    def test_rejects_unknown_schema_and_broken_fields(self):
        for change in ({"schema": 2}, {"kind": "unknown"}, {"sample_rate": 8000},
                       {"id": "20261101T101500Z-hit-17", "kind": "near"},
                       {"label": "maybe"}, {"model_sha256": "short"},
                       {"utc": "2026-11-01 10:15:00"}):
            with self.assertRaises(ValueError):
                station.parse_sidecar(dict(sidecar("20261101T101500Z-hit-17"), **change))

    def test_requires_pcm16_mono_16khz(self):
        station.validate_wav(wav_bytes(1.0))
        buffer = io.BytesIO()
        with wave.open(buffer, "wb") as handle:
            handle.setnchannels(2)
            handle.setsampwidth(2)
            handle.setframerate(16000)
            handle.writeframes(bytes(4000))
        with self.assertRaisesRegex(ValueError, "PCM16 mono"):
            station.validate_wav(buffer.getvalue())


class CollectTests(unittest.TestCase):
    def test_snapshot_is_immutable_and_dedupes_by_id_and_sha(self):
        with tempfile.TemporaryDirectory() as folder:
            files = bundle({"20261101T101500Z-hit-17": (sidecar("20261101T101500Z-hit-17"), wav_bytes())},
                           stats())
            runner, summary = pull(folder, files)
            self.assertEqual(summary["new_events"], 1)
            self.assertIn("tar -C /data/pulsar/wake-events -cf - .", runner.calls[0][0][-1])
            self.assertEqual(runner.calls[0][0][0], "ssh")
            snapshot = Path(summary["snapshot"])
            self.assertTrue((snapshot / "SHA256SUMS").is_file())
            self.assertTrue((snapshot / "20261101T101500Z-hit-17.wav").is_file())
            self.assertEqual(len(station.scan_events(folder)), 1)
            self.assertTrue(list((Path(folder) / "stats").glob("*.json")))

            _, again = pull(folder, files)
            self.assertEqual(again["new_events"], 0)
            self.assertEqual(again["skipped_existing"], 1)
            self.assertEqual(len(station.snapshot_dirs(folder)), 1)

    def test_rejects_unknown_schema_without_leaving_a_snapshot(self):
        with tempfile.TemporaryDirectory() as folder:
            payload = sidecar("20261101T101500Z-hit-17", schema=2)
            with self.assertRaisesRegex(ValueError, "unsupported schema"):
                pull(folder, bundle({"20261101T101500Z-hit-17": (payload, wav_bytes())}))
            self.assertEqual(station.snapshot_dirs(folder), [])
            self.assertEqual([p.name for p in Path(folder).iterdir()], [])

    def test_reused_id_with_other_audio_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            ident = "20261101T101500Z-hit-17"
            pull(folder, bundle({ident: (sidecar(ident), wav_bytes(value=1))}))
            with self.assertRaisesRegex(ValueError, "reused event id"):
                pull(folder, bundle({ident: (sidecar(ident), wav_bytes(value=2))}))

    def test_prune_only_touches_locally_labeled_events(self):
        with tempfile.TemporaryDirectory() as folder:
            hit, near = "20261101T101500Z-hit-17", "20261101T101530Z-near-18"
            files = bundle({hit: (sidecar(hit), wav_bytes()),
                            near: (sidecar(near, kind="near", score_peak=.7), wav_bytes())})
            runner, _ = pull(folder, files)
            self.assertEqual(len(runner.calls), 1)  # no deletion by default
            station.set_label(folder, hit, "false")

            runner = Runner(tar_bytes(files))
            summary = station.collect("192.0.2.13", folder, runner=runner, prune_labeled=True)
            self.assertEqual(summary["pruned"], [hit])
            self.assertEqual(len(runner.calls), 2)
            script = runner.calls[-1][1].decode()
            self.assertIn("./" + hit + ".wav", script)
            self.assertNotIn(near, script)

    def test_safe_prune_script_rejects_foreign_identifiers(self):
        with self.assertRaises(ValueError):
            station.prune_script(["../etc/passwd"])


class LabelTests(unittest.TestCase):
    def test_review_lists_unlabeled_suspected_hits_then_override_wins(self):
        with tempfile.TemporaryDirectory() as folder:
            ident = "20261101T101500Z-hit-17"
            pull(folder, bundle({ident: (sidecar(ident, outcome="empty_stt"), wav_bytes())}))
            rows = station.review_queue(folder)
            self.assertEqual([row["id"] for row in rows], [ident])
            self.assertTrue(Path(rows[0]["wav"]).is_file())

            station.set_label(folder, ident, "false", note="user confirmed")
            self.assertEqual(station.review_queue(folder), [])
            overrides = station.load_overrides(folder)
            self.assertEqual(overrides[ident]["label"], "false")
            event = station.scan_events(folder)[0]
            self.assertIsNone(event["sidecar"]["label"])
            self.assertEqual(station.effective_label(event, overrides), "false")

    def test_cannot_label_an_unknown_event(self):
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaises(KeyError):
                station.set_label(folder, "20261101T101500Z-hit-99", "false")


class ReportTests(unittest.TestCase):
    def test_counts_and_rates_use_listening_deltas_and_dry_run_separately(self):
        with tempfile.TemporaryDirectory() as folder:
            first = {"20261101T101500Z-hit-17": (sidecar(
                "20261101T101500Z-hit-17", label="false", label_source="user"), wav_bytes()),
                "20261101T102000Z-hit-18": (sidecar(
                    "20261101T102000Z-hit-18", outcome="empty_stt"), wav_bytes())}
            pull(folder, bundle(first, stats(3600, 0)),
                 moment=datetime(2026, 11, 1, 12, 0, tzinfo=timezone.utc))
            second = {"20261102T091500Z-hit-19": (sidecar(
                "20261102T091500Z-hit-19", utc="2026-11-02T09:15:00Z", label="false",
                label_source="user", dry_run=True), wav_bytes()),
                "20261102T091530Z-near-20": (sidecar(
                    "20261102T091530Z-near-20", kind="near", utc="2026-11-02T09:15:30Z",
                    score_peak=.7), wav_bytes())}
            pull(folder, bundle(second, stats(7200, 1800)),
                 moment=datetime(2026, 11, 2, 12, 0, tzinfo=timezone.utc))

            report = station.build_report(folder)
            combined = report["combined"]
            self.assertEqual(combined["listening_hours"], 2.0)
            self.assertEqual(combined["dry_run_hours"], 0.5)
            self.assertEqual(combined["confirmed_false"], 2)
            self.assertAlmostEqual(combined["confirmed_false_per_hour"], 1.0)
            self.assertEqual(combined["suspected_false"], 1)
            self.assertAlmostEqual(combined["suspected_false_per_hour"], 0.5)
            self.assertAlmostEqual(combined["near_per_hour"], 0.5)
            self.assertAlmostEqual(combined["dry_run_false_per_hour"], 2.0)
            self.assertIn("Confirmed false hits: 2 = 1.00 / h", station.report_text(report))

    def test_separate_models_do_not_share_listening_time(self):
        with tempfile.TemporaryDirectory() as folder:
            pull(folder, bundle({}, stats(3600, model="a" * 64)))
            pull(folder, bundle({}, stats(1800, model="b" * 64,
                                          since="2026-11-02T00:00:00Z")))
            report = station.build_report(folder)
            self.assertEqual(set(report["models"]), {"a" * 64, "b" * 64})
            self.assertEqual(report["combined"]["listening_hours"], 1.5)


class ManifestTests(unittest.TestCase):
    def test_splits_are_day_deterministic_and_balanced(self):
        self.assertEqual(station.assign_split("2026-11-01"), station.assign_split("2026-11-01"))
        days = [f"2026-{month:02d}-{day:02d}" for month in range(1, 13)
                for day in range(1, 29)]
        splits = [station.assign_split(day) for day in days]
        self.assertEqual(set(splits), {"train", "val", "test"})
        train_share = splits.count("train") / len(splits)
        self.assertTrue(0.62 < train_share < 0.78)
        with self.assertRaises(ValueError):
            station.assign_split("2026-11-01", train=.8, val=.3)

    def test_confirmed_false_and_near_become_hard_negatives(self):
        hit = "20261101T101500Z-hit-17"
        near = "20261101T101530Z-near-18"
        suspected = "20261101T102000Z-hit-19"
        confirmed = "20261101T103000Z-hit-20"
        missed = "20261101T104000Z-miss-21"
        answered = "20261101T105000Z-hit-22"
        events = {
            hit: (sidecar(hit, label="false", label_source="user", outcome="error"), wav_bytes()),
            near: (sidecar(near, kind="near", score_peak=.7), wav_bytes()),
            suspected: (sidecar(suspected, outcome="empty_stt"), wav_bytes()),
            confirmed: (sidecar(confirmed, label="true", label_source="user"), wav_bytes()),
            missed: (sidecar(missed, kind="miss", pre_ms=6000, post_ms=0,
                             label="missed", label_source="user"), wav_bytes(6.0)),
            answered: (sidecar(answered, outcome="answered"), wav_bytes()),
        }
        with tempfile.TemporaryDirectory() as folder:
            pull(folder, bundle(events))
            output = Path(folder) / "run/station-manifest.json"
            manifest = station.to_manifest(folder, output)
            records = {record["id"]: record for record in manifest["records"]}
            self.assertEqual(set(records), {hit, near, confirmed, missed})
            self.assertEqual({records[ident]["source_split"] for ident in records},
                             {station.assign_split("2026-11-01")})
            for ident in (hit, near):
                self.assertTrue(records[ident]["hard_negative"])
                self.assertTrue(records[ident]["station_false_positive"])
                self.assertEqual(records[ident]["block"], "background")
                self.assertEqual(records[ident]["exclude_intervals"], [])
                self.assertEqual(records[ident]["effective_seconds"], records[ident]["seconds"])
            self.assertEqual(records[hit]["station_origin"], "confirmed_false")
            self.assertEqual(records[near]["station_origin"], "near_miss")
            for ident in (confirmed, missed):
                self.assertEqual(records[ident]["block"], "station")
                self.assertEqual(records[ident]["targets"], [])
                self.assertEqual(records[ident]["review_regions"], [[0., records[ident]["seconds"]]])
                self.assertTrue(records[ident]["review_only"])
            audio = Path(folder) / "run/recordings" / (hit + ".wav")
            self.assertTrue(audio.is_file())
            self.assertEqual(hashlib.sha256(audio.read_bytes()).hexdigest(), records[hit]["sha256"])
            self.assertFalse((Path(folder) / "run/recordings" / (answered + ".wav")).exists())

            with self.assertRaisesRegex(FileExistsError, "exists"):
                station.to_manifest(folder, output)

            included = station.to_manifest(folder, output, include_suspected=True, force=True)
            by_id = {record["id"]: record for record in included["records"]}
            self.assertIn(suspected, by_id)
            self.assertEqual(by_id[suspected]["station_origin"], "suspected")

            nearless = station.to_manifest(folder, output, near_negatives=False, force=True)
            self.assertNotIn(near, {record["id"] for record in nearless["records"]})

    def test_near_miss_followed_by_a_miss_press_is_not_a_negative(self):
        near = "20261101T101530Z-near-18"
        missed = "20261101T101540Z-miss-19"
        lone = "20261101T120000Z-near-20"
        events = {
            near: (sidecar(near, kind="near", utc="2026-11-01T10:15:30Z", score_peak=.7),
                   wav_bytes()),
            missed: (sidecar(missed, kind="miss", utc="2026-11-01T10:15:40Z", pre_ms=6000,
                             post_ms=0, label="missed", label_source="user"), wav_bytes(6.0)),
            lone: (sidecar(lone, kind="near", utc="2026-11-01T12:00:00Z", score_peak=.6),
                   wav_bytes()),
        }
        with tempfile.TemporaryDirectory() as folder:
            pull(folder, bundle(events))
            manifest = station.to_manifest(folder, Path(folder) / "run/station-manifest.json")
            ids = {record["id"] for record in manifest["records"]}
            self.assertNotIn(near, ids)
            self.assertIn(lone, ids)
            self.assertIn(missed, ids)


class FinetuneMergeTests(unittest.TestCase):
    def test_negative_key_keeps_station_bucket_separate(self):
        station_entry = {"hard_negative": True, "station_false_positive": True}
        hard_entry = {"hard_negative": True}
        self.assertEqual(finetune.negative_key(station_entry, "train"), "station_train_neg")
        self.assertEqual(finetune.negative_key(hard_entry, "val"), "hard_val_neg")
        self.assertEqual(finetune.negative_key({}, "train"), "train_neg")

    def test_merge_manifests_rejects_duplicates_and_bad_schema(self):
        primary = {"schema": 1, "records": [{"id": "a", "source_split": "train",
                                             "block": "background", "sha256": "x", "seconds": 1}]}
        extra = {"schema": 1, "station_events": {"seed": "station-events"},
                 "records": [{"id": "b", "source_split": "val", "block": "background",
                              "sha256": "y", "seconds": 2}]}
        merged = finetune.merge_manifests(primary, [extra])
        self.assertEqual([record["id"] for record in merged["records"]], ["a", "b"])
        self.assertEqual(merged["extra_manifests"][0]["seed"], "station-events")
        self.assertEqual(primary["records"], [{"id": "a", "source_split": "train",
                                               "block": "background", "sha256": "x", "seconds": 1}])

        with self.assertRaisesRegex(ValueError, "Duplicate"):
            finetune.merge_manifests(primary, [{"schema": 1, "records": [primary["records"][0]]}])
        with self.assertRaisesRegex(ValueError, "schema"):
            finetune.merge_manifests(primary, [{"schema": 2, "records": []}])
        with self.assertRaisesRegex(ValueError, "source_split"):
            finetune.merge_manifests(primary, [{"schema": 1, "records": [
                {"id": "c", "source_split": "other", "block": "background",
                 "sha256": "y", "seconds": 1}]}])


class CalibrationTests(unittest.TestCase):
    def test_build_table_and_recommend_lowest_false_positive_setting(self):
        # pcm is (label, peak) so the fake replay is pure and needs no NumPy/TF.
        def replay(pcm, cutoff, window):
            return {"hits": [1.0] if pcm[1] >= cutoff else []}

        positives = [("pos", .99), ("pos", .96), ("pos", .93)]
        negatives = [("neg", .94)]
        table = calibrate.build_table(positives, negatives, replay,
                                      cutoffs=(.9, .95, .96), windows=(10,),
                                      listening_hours=2.0)
        rows = {row["cutoff"]: row for row in table["rows"]}
        self.assertEqual(rows[.9]["recall"], 1.0)
        self.assertAlmostEqual(rows[.9]["false_per_hour"], .5)
        self.assertEqual(rows[.96]["recall"], round(2 / 3, 4))
        self.assertEqual(rows[.96]["false_per_hour"], 0.0)
        recommendation = calibrate.recommend(table, .95, 10)
        self.assertEqual((recommendation["cutoff"], recommendation["window"]), (.96, 10))
        self.assertEqual(recommendation["baseline"]["recall"], round(2 / 3, 4))
        self.assertIn("Recommendation: cutoff 0.96, window 10",
                      calibrate.table_text(table, recommendation))

    def test_positive_selection_skips_backgrounds_and_station_ids(self):
        with tempfile.TemporaryDirectory() as folder:
            for name, meta in [("p", {"block": "normal"}), ("bg", {"block": "background"}),
                               ("gone", {"block": "normal", "excluded": True})]:
                (Path(folder) / (name + ".wav")).write_bytes(b"x")
                (Path(folder) / (name + ".json")).write_text(json.dumps(meta))
            (Path(folder) / "20261101T101500Z-hit-17.wav").write_bytes(b"x")
            selected = calibrate.collect_wavs(folder, skip_ids={"20261101T101500Z-hit-17"})
            self.assertEqual([path.name for path in selected], ["p.wav"])

    def test_recommend_requires_the_baseline_configuration(self):
        table = {"listening_hours": 1.0, "positives": 1, "negatives": 0, "rows": []}
        with self.assertRaises(ValueError):
            calibrate.recommend(table)

    def test_manifest_update_preserves_other_keys_and_validates(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "model.json"
            path.write_text(json.dumps({"model": "ru_ricardo_mww", "micro": {"other": 1}}))
            payload = calibrate.write_manifest(path, .93, 12)
            self.assertEqual(payload["model"], "ru_ricardo_mww")
            self.assertEqual(payload["micro"], {"other": 1, "probability_cutoff": .93,
                                                "sliding_window_size": 12})
            self.assertEqual(json.loads(path.read_text()), payload)
            for cutoff, window in ((1.0, 10), (0.0, 10), (.9, 0), (.9, 51)):
                with self.assertRaises(ValueError):
                    calibrate.write_manifest(path, cutoff, window)


if __name__ == "__main__":
    unittest.main()
