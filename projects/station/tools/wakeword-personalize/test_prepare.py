import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import wave

spec = importlib.util.spec_from_file_location("personal_prepare", Path(__file__).with_name("prepare.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
comparison_spec = importlib.util.spec_from_file_location("personal_compare", Path(__file__).with_name("compare.py"))
compare = importlib.util.module_from_spec(comparison_spec)
comparison_spec.loader.exec_module(compare)
train_spec = importlib.util.spec_from_file_location("personal_train", Path(__file__).with_name("finetune.py"))
finetune = importlib.util.module_from_spec(train_spec)
train_spec.loader.exec_module(finetune)


class PrepareTests(unittest.TestCase):
    def test_detector_rejects_invalid_cutoff_and_smoothing_before_loading_model(self):
        spec = importlib.util.spec_from_file_location("personal_evaluate", Path(__file__).with_name("evaluate.py"))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for cutoff, window in [(0., 10), (1.1, 10), (.9, 0), (.9, 1.5)]:
            with self.assertRaises(ValueError):
                module.replay(None, None, cutoff=cutoff, window=window)

    def test_freezes_nested_batchnorm_only(self):
        class Layer:
            trainable = True
            def __init__(self, layers=()): self.layers = layers
        class BN(Layer): pass
        first, second, conv = BN(), BN(), Layer()
        model = Layer([first, Layer([second, conv]), first])
        self.assertEqual(finetune.freeze_batchnorm(model, BN), 2)
        self.assertFalse(first.trainable)
        self.assertFalse(second.trainable)
        self.assertTrue(conv.trainable)

    def test_feature_window_matches_frontend_frame_timestamps(self):
        self.assertEqual(finetune.feature_window_bounds(2.24, 300), (18, 222))
        self.assertEqual(finetune.feature_window_bounds(3., 222), (18, 222))
        self.assertIsNone(finetune.feature_window_bounds(.5, 300))

    def test_continuous_extraction_refuses_test_labels(self):
        with self.assertRaisesRegex(ValueError, "train/val only"):
            list(finetune.continuous_examples(None, [{"partition": "test"}], None, None))

    def test_mixed_track_reports_sharp_and_normal_separately(self):
        manifest = {"records": [dict(id="mixed", source_split="test", block="mixed",
            targets=[dict(start=1., end=1.8, speech_style="normal"),
                     dict(start=5., end=5.8, speech_style="sharp")], review_regions=[])]}
        result = compare.summarize(manifest, {"model_sha256": "test", "records": [{"id": "mixed", "hits": [2.]}]})
        self.assertEqual(result["blocks"]["normal"]["matched"], 1)
        self.assertEqual(result["blocks"]["sharp"]["matched"], 0)
        self.assertEqual(result["verified_targets"], 2)

    def test_calibration_covers_later_speech_blocks_without_repeating_examples(self):
        labels = ["normal"] * 300 + ["quiet"] * 100 + ["whisper"] * 100
        selected = finetune.balanced_indices(labels, 250)
        self.assertEqual(len(set(selected)), 250)
        self.assertEqual({labels[i] for i in selected}, {"normal", "quiet", "whisper"})
        self.assertEqual(finetune.balanced_indices(["whisper", "normal"], 10), [1, 0])

    def test_comparison_accumulates_separate_recordings_of_same_speech_block(self):
        manifest = {"records": [dict(id=ident, source_split="test", block="whisper",
            targets=[{"start": 1., "end": 1.8}], review_regions=[]) for ident in ["old", "new"]]}
        replay = {"model_sha256": "test", "records": [
            {"id": "old", "hits": [2.]}, {"id": "new", "hits": []}]}
        result = compare.summarize(manifest, replay)
        self.assertEqual(result["matched"], 1)
        self.assertEqual(result["verified_targets"], 2)
        self.assertEqual(result["blocks"]["whisper"]["raw_detector_events"], 1)

    def test_whisper_regression_cannot_hide_behind_better_normal_voice(self):
        reference = [.95, .8, .96, .94]
        candidate = [.97, .98, .97, .85]
        ok, counts = finetune.protects_speech(reference, candidate, ["normal", "normal", "whisper", "whisper"])
        self.assertFalse(ok)
        self.assertEqual(counts, {"normal": 2, "whisper": 1})

    def test_hard_negative_windows_have_guard_and_test_is_never_training(self):
        entry = {"source_split": "train", "partition": "train", "hard_negative": True,
                 "validation_boundary": 80, "validation_guard": 2.5}
        self.assertEqual(finetune.negative_partition(entry, 10, 12.04), "train")
        self.assertEqual(finetune.negative_partition(entry, 78, 80.04), "guard")
        self.assertEqual(finetune.negative_partition(entry, 84, 86.04), "val")
        entry["source_split"] = "test"
        self.assertEqual(finetune.negative_partition(entry, 10, 12.04), "test")

    def test_repeated_events_do_not_double_count_an_utterance(self):
        targets = [{"start": 1., "end": 1.8}, {"start": 5., "end": 5.8}]
        self.assertEqual(compare.match_targets(targets, [.1, 2., 2.2, 10.]), 1)
        self.assertEqual(compare.match_targets(targets, [2., 6.]), 2)

    def test_deduplication_retains_independent_utterances(self):
        a = {"start": 1., "end": 1.8, "p": .6}
        b = {"start": 1.1, "end": 1.9, "p": .9}
        c = {"start": 4., "end": 4.8, "p": .8}
        self.assertEqual(prepare.deduplicate([c, b, a]), [b, c])

    def test_partitions_preserve_heldout_recordings_and_temporal_guard(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            recordings, align, asr = [root / n for n in ["recordings", "align", "asr"]]
            for d in [recordings, align, asr]: d.mkdir()
            def recording(ident, block, split, value):
                path = recordings / (ident + ".wav")
                with wave.open(str(path), "wb") as wav:
                    wav.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
                    wav.writeframes(bytes([value, 0]) * 16000 * 65)
                (recordings / (ident + ".json")).write_text(json.dumps({
                    "id": ident, "block": block, "split": split, "frames": 65 * 16000,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}))
                (asr / (ident + ".json")).write_text('{"segments":[]}')
            for i in range(3): recording(str(i), "background", "train", i)
            for ident, split, value in [("p", "train", 5), ("t", "test", 6)]:
                recording(ident, "normal", split, value)
                hits = [{"start": a, "end": a + .6, "p": .9} for a in [10., 52., 60.]]
                (align / (ident + ".json")).write_text(json.dumps({"candidates": [{"start": 0, "end": 65, "hits": hits}]}))
            result = prepare.prepare(recordings, align, asr)
            entries = {e["id"]: e for e in result["records"]}
            self.assertEqual([h["partition"] for h in entries["p"]["targets"]], ["train", "guard", "val"])
            self.assertEqual({h["partition"] for h in entries["t"]["targets"]}, {"test"})
            self.assertEqual({e["partition"] for e in entries.values() if e["block"] == "background"}, {"train", "val", "test"})
            # Mutated audio is rejected before features/training are produced.
            with (recordings / "p.wav").open("ab") as f: f.write(b"changed")
            with self.assertRaisesRegex(ValueError, "changed"):
                prepare.prepare(recordings, align, asr)


if __name__ == "__main__":
    unittest.main()
