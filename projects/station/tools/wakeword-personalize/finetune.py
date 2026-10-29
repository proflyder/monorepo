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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trainer", required=True, type=Path)
    parser.add_argument("--run", required=True, type=Path)
    parser.add_argument("--steps", type=int, default=1000)
    parser.add_argument("--registry", type=Path, help="Private model registry; defaults to trainer/training/model-registry/ricardo")
    parser.add_argument("--version", help="New immutable registry version ID")
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
    cfg.update(model_name="ru_ricardo_personal_v1", learning_rate=3e-5, version=2)
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
    counts = {key: 0 for key in real}
    for e in manifest["records"]:
        if e["id"] not in data:
            continue
        audio = data[e["id"]]
        if e["block"] == "background":
            part = e["partition"]
            if part == "test":
                continue
            frames = trainer.extract_all_frames(audio[:int(e["effective_seconds"] * 16000)])
            for start in range(0, len(frames) - 204, 20 if part == "train" else 50):
                a, b = start / 100, (start + 204) / 100
                if any(a < y and b > x for x, y in e["exclude_intervals"]):
                    continue
                real[part + "_neg"].append(frames[start:start + 204])
                counts[part + "_neg"] += 1
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
    for key in real:
        if not real[key]:
            raise RuntimeError("Empty personal partition: " + key)
        real[key] = np.asarray(real[key], dtype=np.float32)
        np.save(run / (key + ".npy"), real[key])
        print("PERSONAL_FEATURES", key, real[key].shape, flush=True)
    old = root / "training/output/ru_ricardo"
    synthetic = {key: np.load(old / "mww_features" / (name + ".npy"), mmap_mode="r")
                 for key, name in [("train_pos", "positive_train"), ("train_neg", "negative_train"),
                                   ("val_pos", "positive_val"), ("val_neg", "negative_val")]}
    model = trainer._build_model(cfg)
    weights = old / "model/ru_ricardo_mww.weights.h5"
    model.load_weights(str(weights))
    origin_hash = hashlib.sha256(weights.read_bytes()).hexdigest()
    model.compile(optimizer=tf.keras.optimizers.Adam(3e-5), loss="binary_crossentropy", jit_compile=False,
                  metrics=[tf.keras.metrics.BinaryAccuracy(), tf.keras.metrics.Recall(), tf.keras.metrics.Precision()])
    batch = cfg["batch_size"]
    if batch % 4:
        raise ValueError("Batch must divide into four source buckets")
    def batches():
        while True:
            xs, ys = [], []
            for arrays, key, label in [(real, "train_pos", 1), (synthetic, "train_pos", 1),
                                       (real, "train_neg", 0), (synthetic, "train_neg", 0)]:
                arr = arrays[key]
                xs.append(np.asarray(arr[rng.integers(0, len(arr), batch // 4)], dtype=np.float32))
                ys.append(np.full(batch // 4, label, dtype=np.float32))
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
    best = modeldir / "best.weights.h5"
    (run / "training-provenance.json").write_text(json.dumps({
        "base_weights_sha256": origin_hash, "gpu": [str(g) for g in gpus], "seed": 42,
        "source_counts": counts, "learning_rate": 3e-5,
        "training_buckets": "25% real-positive / synthetic-positive / real-negative / synthetic-negative",
        "test_used_in_training": False, "steps": args.steps}, indent=2))
    print("START_GPU_FINETUNING", args.steps, "steps", flush=True)
    model.fit(train, epochs=max(1, args.steps // 100), steps_per_epoch=100,
              validation_data=(xval, yval, valweights), validation_batch_size=batch, callbacks=[
                  tf.keras.callbacks.ModelCheckpoint(str(best), monitor="val_loss", save_best_only=True, save_weights_only=True),
                  tf.keras.callbacks.ModelCheckpoint(str(modeldir / "epochs/epoch-{epoch:03d}.weights.h5"), save_weights_only=True),
                  tf.keras.callbacks.CSVLogger(str(modeldir / "training-metrics.csv")),
                  tf.keras.callbacks.EarlyStopping(monitor="val_loss", patience=3, restore_best_weights=True)], verbose=2)
    model.load_weights(str(best))
    model.save_weights(str(modeldir / (cfg["model_name"] + "_mww.weights.h5")))
    # Small calibration set includes both old synthesis and real capture features.
    calibration = np.concatenate([real["train_pos"][:250], np.asarray(synthetic["train_pos"][:250])])
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
        saved = registry.snapshot(registry_path, version, stage, {
            "role": "candidate, not deployed", "personal_voice": True,
            "parent_weights_sha256": origin_hash, "epoch_checkpoints": "all completed epochs"})
        print("REGISTRY_SAVED", saved.name, "verified files", registry.verify(saved), flush=True)


if __name__ == "__main__":
    main()
