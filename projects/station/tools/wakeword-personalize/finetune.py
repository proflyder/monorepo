#!/usr/bin/env python3
"""GPU-only bounded adaptation of an existing training checkpoint.

Run inside the existing trainer environment. Original weights/features are read-only;
personal data, checkpoints and export remain under the private run directory.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
from datetime import datetime, timezone
import wave


def negative_partition(entry, start, end):
    if entry["source_split"] == "test" or entry["partition"] == "test":
        return "test"
    if entry.get("hard_negative") and "validation_boundary" in entry:
        boundary, guard = entry["validation_boundary"], entry.get("validation_guard", 2.5)
        return "train" if end < boundary - guard else "val" if start > boundary + guard else "guard"
    return entry["partition"]


def protects_speech(reference, candidate, blocks, cutoff=.9):
    if not (len(reference) == len(candidate) == len(blocks)):
        raise ValueError("Protection labels and predictions do not align")
    required = {block: sum(p >= cutoff for p, b in zip(reference, blocks) if b == block) for block in set(blocks)}
    actual = {block: sum(p >= cutoff for p, b in zip(candidate, blocks) if b == block) for block in required}
    return all(actual[b] >= required[b] for b in required), actual


def balanced_indices(labels, limit):
    """Deterministic round-robin coverage without validation/test examples."""
    from collections import defaultdict, deque
    groups = defaultdict(deque)
    for index, label in enumerate(labels):
        groups[label].append(index)
    selected = []
    while len(selected) < limit and any(groups.values()):
        for label in sorted(groups):
            if groups[label] and len(selected) < limit:
                selected.append(groups[label].popleft())
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trainer", required=True, type=Path)
    parser.add_argument("--run", required=True, type=Path)
    parser.add_argument("--steps", type=int, default=1000)
    parser.add_argument("--registry", type=Path, help="Private model registry; defaults to trainer/training/model-registry/ricardo")
    parser.add_argument("--version", help="New immutable registry version ID")
    parser.add_argument("--base-weights", type=Path)
    parser.add_argument("--model-name", default="ru_ricardo_personal_v1")
    parser.add_argument("--manifest-version", type=int, default=2)
    parser.add_argument("--learning-rate", type=float, default=3e-5)
    args = parser.parse_args()
    root, run = args.trainer.resolve(), args.run.resolve()
    registry_spec = importlib.util.spec_from_file_location("model_registry", Path(__file__).with_name("registry.py"))
    registry = importlib.util.module_from_spec(registry_spec)
    registry_spec.loader.exec_module(registry)
    registry_path = args.registry.resolve() if args.registry else root / "training/model-registry/ricardo"
    version = args.version or (run.name + "-" + datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S").lower())
    if (registry_path / version).exists():
        raise FileExistsError("Registry version exists; refusing to overwrite")
    os.umask(0o077)
    os.chdir(root)
    import numpy as np
    import tensorflow as tf
    gpus = tf.config.list_physical_devices("GPU")
    if not gpus:
        raise RuntimeError("NVIDIA GPU required; refusing CPU fallback")
    for gpu in gpus:
        tf.config.experimental.set_memory_growth(gpu, True)
    tf.keras.utils.set_random_seed(42)
    rng = np.random.default_rng(42)
    spec = importlib.util.spec_from_file_location("existing_trainer", root / "train_mww.py")
    trainer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(trainer)
    cfg = trainer.load_config(str(root / "ricardo.yaml"))
    cfg.update(model_name=args.model_name, learning_rate=args.learning_rate, version=args.manifest_version)
    # Redirect all exports away from the old model and from the repo root.
    trainer.OUTPUT_DIR = run / "training-output"
    output = trainer.OUTPUT_DIR / cfg["model_name"]
    modeldir, featdir = output / "model", output / "mww_features"
    modeldir.mkdir(parents=True, exist_ok=True)
    (modeldir / "epochs").mkdir(exist_ok=True)
    featdir.mkdir(parents=True, exist_ok=True)
    if (modeldir / "training-metrics.csv").exists():
        raise RuntimeError("Run already trained; use a new run directory instead of overwriting it")
    manifest = json.loads((run / "manifest.json").read_text())
    data = {}
    for e in manifest["records"]:
        if (e["block"] == "background" and e["partition"] == "test") or e["source_split"] == "test":
            continue
        with wave.open(str(run / "recordings" / (e["id"] + ".wav"))) as wav:
            data[e["id"]] = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2")

    real = {key: [] for key in ["train_pos", "val_pos", "train_neg", "val_neg"]}
    val_pos_blocks = []
    train_pos_blocks = []
    hard = any(e.get("hard_negative") and e["source_split"] == "train" for e in manifest["records"])
    synthetic_hard = manifest.get("synthetic_hard_features")
    if synthetic_hard and not hard:
        raise ValueError("Synthetic confusables require labelled real hard negatives")
    if hard:
        real.update(hard_train_neg=[], hard_val_neg=[])
    counts = {key: 0 for key in real}
    for e in manifest["records"]:
        if e["id"] not in data:
            continue
        audio = data[e["id"]]
        if e["block"] == "background":
            part = e["partition"]
            if part == "test":
                continue
            if e.get("hard_negative") and e.get("mined_events"):
                # Match positive examples: reset the frontend on a short PCM
                # context around the actual false wake, not on an entire track.
                # Dense whole-track sampling otherwise mostly trains on pauses.
                for event in e["mined_events"]:
                    for variant in range(int(e.get("augment_variants", 32))):
                        shift = 0. if variant == 0 else float(rng.uniform(-.3, .1))
                        end = int((event + shift) * 16000)
                        start = end - 224 * 160
                        partition = negative_partition(e, start / 16000, end / 16000)
                        if partition not in {"train", "val"} or end > len(audio):
                            continue
                        if partition == "val" and variant:
                            continue
                        clip = audio[max(0, start):end].astype(np.float32)
                        if start < 0:
                            # Only explicitly negative audio is padded: retain an
                            # early false wake rather than silently discarding it.
                            clip = np.pad(clip, (-start, 0))
                        if partition == "train" and variant:
                            clip *= float(rng.uniform(.55, 1.6))
                            ratio = float(rng.uniform(.95, 1.05))
                            clip = np.interp(np.arange(0, len(clip), ratio), np.arange(len(clip)), clip)
                        frames = trainer.extract_all_frames(np.clip(clip, -32768, 32767).astype(np.int16))
                        if len(frames) >= 204:
                            key = "hard_" + partition + "_neg"
                            real[key].append(frames[-204:])
                            counts[key] += 1
                continue
            frames = trainer.extract_all_frames(audio[:int(e["effective_seconds"] * 16000)])
            stride = 5 if e.get("hard_negative") else 20 if part == "train" else 50
            for start in range(0, len(frames) - 204, stride):
                a, b = start / 100, (start + 204) / 100
                if any(a < y and b > x for x, y in e["exclude_intervals"]):
                    continue
                partition = negative_partition(e, a, b)
                if partition not in {"train", "val"}:
                    continue
                key = ("hard_" if e.get("hard_negative") else "") + partition + "_neg"
                real[key].append(frames[start:start + 204])
                counts[key] += 1
        else:
            for target in e["targets"]:
                part = target["partition"]
                if part not in {"train", "val"}:
                    continue
                counts[part + "_pos"] += 1
                variants = 32 if part == "train" else 1
                for v in range(variants):
                    after = float(rng.uniform(.08, .38)) if part == "train" else .18
                    end = int((target["end"] + after) * 16000)
                    start = end - 224 * 160
                    if start < 0 or end > len(audio):
                        # Preserve actual room PCM; edge targets without full context
                        # are excluded, not padded with falsely labelled silence.
                        continue
                    clip = audio[start:end].astype(np.float32)
                    if part == "train" and v:
                        clip *= float(rng.uniform(.55, 1.6))
                        # Small speed variations; never fabricate whisper from normal speech.
                        ratio = float(rng.uniform(.95, 1.05))
                        clip = np.interp(np.arange(0, len(clip), ratio), np.arange(len(clip)), clip)
                    clip = np.clip(clip, -32768, 32767).astype(np.int16)
                    frames = trainer.extract_all_frames(clip)
                    if len(frames) >= 204:
                        real[part + "_pos"].append(frames[-204:])
                        if part == "val":
                            val_pos_blocks.append(target.get("speech_style", e["block"]))
                        else:
                            train_pos_blocks.append(target.get("speech_style", e["block"]))
    if synthetic_hard:
        for part in ["train", "val"]:
            key = "hard_synth_" + part + "_neg"
            real[key] = np.load(run / synthetic_hard[part]).astype(np.float32)
            counts[key] = len(real[key])
    for key in real:
        if len(real[key]) == 0:
            raise RuntimeError("Empty personal partition: " + key)
        real[key] = np.asarray(real[key], dtype=np.float32)
        np.save(run / (key + ".npy"), real[key])
        print("PERSONAL_FEATURES", key, real[key].shape, flush=True)
    old = root / "training/output/ru_ricardo"
    synthetic = {key: np.load(old / "mww_features" / (name + ".npy"), mmap_mode="r")
                 for key, name in [("train_pos", "positive_train"), ("train_neg", "negative_train"),
                                   ("val_pos", "positive_val"), ("val_neg", "negative_val")]}
    model = trainer._build_model(cfg)
    weights = args.base_weights.resolve() if args.base_weights else old / "model/ru_ricardo_mww.weights.h5"
    model.load_weights(str(weights))
    origin_hash = hashlib.sha256(weights.read_bytes()).hexdigest()
    model.compile(optimizer=tf.keras.optimizers.Adam(args.learning_rate), loss="binary_crossentropy", jit_compile=False,
                  metrics=[tf.keras.metrics.BinaryAccuracy(), tf.keras.metrics.Recall(), tf.keras.metrics.Precision()])
    batch = cfg["batch_size"]
    if batch % (8 if hard else 4):
        raise ValueError("Batch must divide into four source buckets")
    def batches():
        while True:
            xs, ys = [], []
            buckets = [(real, "train_pos", 1, batch // 4), (synthetic, "train_pos", 1, batch // 4),
                       (real, "train_neg", 0, batch // (8 if hard else 4)),
                       (synthetic, "train_neg", 0, batch // (8 if hard else 4))]
            if hard:
                buckets.append((real, "hard_train_neg", 0, batch // (8 if synthetic_hard else 4)))
            if synthetic_hard:
                buckets.append((real, "hard_synth_train_neg", 0, batch // 8))
            for arrays, key, label, count in buckets:
                arr = arrays[key]
                xs.append(np.asarray(arr[rng.integers(0, len(arr), count)], dtype=np.float32))
                ys.append(np.full(count, label, dtype=np.float32))
            x, y = np.concatenate(xs), np.concatenate(ys)
            order = rng.permutation(batch)
            yield x[order], y[order]
    train = tf.data.Dataset.from_generator(batches, output_signature=(
        tf.TensorSpec((batch, 204, 40), tf.float32), tf.TensorSpec((batch,), tf.float32))).prefetch(2)
    # Validation uses held-out personal targets and a separate background take.
    # The user's test recordings are never accessed by training or checkpoint selection.
    vp = real["val_pos"]
    vn = real["val_neg"]
    xval = np.concatenate([vp, vn, np.asarray(synthetic["val_pos"][:200]), np.asarray(synthetic["val_neg"][:200])])
    yval = np.concatenate([np.ones(len(vp)), np.zeros(len(vn)), np.ones(200), np.zeros(200)])
    # Bound positive length imbalance without copying or augmenting validation audio.
    valweights = np.concatenate([np.full(len(vp), 200 / len(vp)), np.full(len(vn), 200 / len(vn)), np.ones(400)])
    if hard:
        hn = real["hard_val_neg"]
        xval = np.concatenate([xval, hn])
        yval = np.concatenate([yval, np.zeros(len(hn))])
        valweights = np.concatenate([np.full(len(vp), 200 / len(vp)), np.full(len(vn), 100 / len(vn)),
                                     np.ones(200), np.full(200, .5), np.full(len(hn), 200 / len(hn))])
    if synthetic_hard:
        shn = real["hard_synth_val_neg"]
        xval = np.concatenate([xval, shn])
        yval = np.concatenate([yval, np.zeros(len(shn))])
        valweights = np.concatenate([np.full(len(vp), 200 / len(vp)), np.full(len(vn), 100 / len(vn)),
                                     np.ones(200), np.full(200, .5), np.full(len(hn), 100 / len(hn)),
                                     np.full(len(shn), 100 / len(shn))])
    best = modeldir / "best.weights.h5"
    selection = []
    if hard:
        protected = np.asarray(val_pos_blocks)
        reference = model.predict(vp, batch_size=batch, verbose=0).reshape(-1)
        reference_counts = {block: int(np.sum(reference[protected == block] >= .9)) for block in set(val_pos_blocks)}
        hard_val = np.concatenate([real["hard_val_neg"], real["hard_synth_val_neg"]]) if synthetic_hard else real["hard_val_neg"]
        reference_false = int(np.sum(model.predict(hard_val, batch_size=batch, verbose=0) >= .9))
        reference_loss = float(model.evaluate(xval, yval, sample_weight=valweights, batch_size=batch, verbose=0)[0])
        model.save_weights(str(best))
        class ProtectedSelection(tf.keras.callbacks.Callback):
            def __init__(self):
                super().__init__()
                self.best_key = (reference_false, reference_loss)
                self.selected_epoch = 0
            def on_epoch_end(self, epoch, logs=None):
                predictions = self.model.predict(vp, batch_size=batch, verbose=0).reshape(-1)
                safe, counts_now = protects_speech(reference, predictions, val_pos_blocks)
                false = int(np.sum(self.model.predict(hard_val, batch_size=batch, verbose=0) >= .9))
                key = (false, float(logs["val_loss"]))
                if safe and key < self.best_key:
                    self.best_key, self.selected_epoch = key, epoch + 1
                    self.model.save_weights(str(best))
                logs["deployment_selection"] = false * 100 + logs["val_loss"] if safe else 1e6
                print("PROTECTED_VALIDATION", epoch + 1, "safe", safe, "counts", counts_now, "false", false, flush=True)
        selection = [ProtectedSelection()]
    (run / "training-provenance.json").write_text(json.dumps({
        "base_weights_sha256": origin_hash, "gpu": [str(g) for g in gpus], "seed": 42,
        "source_counts": counts, "learning_rate": args.learning_rate,
        "training_buckets": ("25% real-positive / 25% synthetic-positive / 12.5% real-hard-negative / 12.5% synthetic-confusable / 12.5% real-negative / 12.5% old-synthetic-negative" if synthetic_hard else "25% real-positive / 25% synthetic-positive / 25% hard-negative / 12.5% real-negative / 12.5% synthetic-negative" if hard else "25% real-positive / synthetic-positive / real-negative / synthetic-negative"),
        "test_used_in_training": False, "steps": args.steps}, indent=2))
    print("START_GPU_FINETUNING", args.steps, "steps", flush=True)
    history = model.fit(train, epochs=max(1, args.steps // 100), steps_per_epoch=100,
              validation_data=(xval, yval, valweights), validation_batch_size=batch, callbacks=selection + [
                  *([] if hard else [tf.keras.callbacks.ModelCheckpoint(str(best), monitor="val_loss", save_best_only=True, save_weights_only=True)]),
                  tf.keras.callbacks.ModelCheckpoint(str(modeldir / "epochs/epoch-{epoch:03d}.weights.h5"), save_weights_only=True),
                  tf.keras.callbacks.CSVLogger(str(modeldir / "training-metrics.csv")),
                  tf.keras.callbacks.EarlyStopping(monitor="deployment_selection" if hard else "val_loss", mode="min", patience=10 if hard else 3, start_from_epoch=4 if hard else 0, restore_best_weights=True)], verbose=2)
    provenance = json.loads((run / "training-provenance.json").read_text())
    provenance.update(completed_epochs=len(history.epoch), completed_steps=len(history.epoch) * 100,
                      selection="protected validation per speech block + hard negatives" if hard else "val_loss",
                      selected_epoch=selection[0].selected_epoch if hard else None)
    (run / "training-provenance.json").write_text(json.dumps(provenance, indent=2))
    model.load_weights(str(best))
    model.save_weights(str(modeldir / (cfg["model_name"] + "_mww.weights.h5")))
    # Cover every recorded speech style rather than taking only the first track.
    indices = balanced_indices(train_pos_blocks, 250)
    calibration = np.concatenate([real["train_pos"][indices], np.asarray(synthetic["train_pos"][:250])])
    provenance["calibration"] = {"real_train_indices": indices,
        "speech_blocks": sorted(set(train_pos_blocks)), "synthetic_train_examples": 250}
    (run / "training-provenance.json").write_text(json.dumps(provenance, indent=2))
    np.save(featdir / "positive_train.npy", calibration)
    os.chdir(run)
    trainer.phase_export(cfg)
    path = run / (cfg["model_name"] + "_mww.tflite")
    interp = tf.lite.Interpreter(model_path=str(path))
    interp.allocate_tensors()
    inp, out = interp.get_input_details()[0], interp.get_output_details()[0]
    if list(inp["shape"]) != [1, 3, 40] or inp["dtype"] != np.int8 or out["dtype"] != np.uint8:
        raise RuntimeError("Export is not compatible with the deployed streaming detector")
    print("EXPORT_VERIFIED", path.name, path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest(), flush=True)
    # A model version is complete only after the export, all epoch weights and
    # its recipe have been copied and verified in the private append-only registry.
    with tempfile.TemporaryDirectory(prefix="registry-export-", dir=run) as temporary:
        stage = Path(temporary)
        for source in modeldir.rglob("*.h5"):
            dest = stage / "weights" / source.relative_to(modeldir)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, dest)
        for source in [path, path.with_suffix(".json"), run / "training-provenance.json", run / "manifest.json", modeldir / "training-metrics.csv"]:
            shutil.copyfile(source, stage / source.name)
        shutil.copyfile(root / "train_mww.py", stage / "trainer-source.py")
        shutil.copyfile(Path(__file__), stage / "finetune-source.py")
        (stage / "config.json").write_text(json.dumps(cfg, indent=2, ensure_ascii=False))
        for name in ["synthetic-recipe.json", "generate_synthetic.py"]:
            if (run / name).is_file():
                shutil.copyfile(run / name, stage / name)
        saved = registry.snapshot(registry_path, version, stage, {
            "role": "candidate, not deployed", "personal_voice": True,
            "parent_weights_sha256": origin_hash, "epoch_checkpoints": "all completed epochs"})
        print("REGISTRY_SAVED", saved.name, "verified files", registry.verify(saved), flush=True)


if __name__ == "__main__":
    main()
