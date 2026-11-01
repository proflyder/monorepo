#!/usr/bin/env python3
"""GPU-only bounded personalization or full training from random initialization.

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


def negative_key(entry, partition):
    """Feature bucket for a negative window.

    Station false positives keep a separate bucket so they are not diluted by
    the rest of the hard negatives; other entries keep the existing keys.
    """
    if entry.get("station_false_positive"):
        return "station_" + partition + "_neg"
    return ("hard_" if entry.get("hard_negative") else "") + partition + "_neg"


def merge_manifests(primary, extras):
    """Merge extra manifests (e.g. station wake events) into the run manifest.

    Identifiers must be unique so a recording can never be counted twice with
    two different split decisions. The primary manifest keeps its metadata and
    the merged sources are recorded for provenance.
    """
    if not isinstance(primary, dict) or primary.get("schema") != 1:
        raise ValueError("Primary manifest schema must be 1")
    records = list(primary.get("records") or [])
    seen = {entry.get("id") for entry in records}
    sources = []
    for index, extra in enumerate(extras):
        if not isinstance(extra, dict) or extra.get("schema") != 1:
            raise ValueError(f"Extra manifest #{index + 1} schema must be 1")
        if not isinstance(extra.get("records"), list):
            raise ValueError(f"Extra manifest #{index + 1} has no records list")
        for entry in extra["records"]:
            identity = entry.get("id")
            if not identity or not isinstance(identity, str):
                raise ValueError("Manifest record without a string id")
            if identity in seen:
                raise ValueError("Duplicate manifest record id: " + identity)
            if entry.get("source_split") not in {"train", "val", "test"}:
                raise ValueError(f"{identity}: source_split must be train/val/test")
            for field in ("block", "sha256", "seconds"):
                if field not in entry:
                    raise ValueError(f"{identity}: missing {field}")
            seen.add(identity)
            records.append(entry)
        sources.append(extra.get("station_events") or {"records": len(extra["records"])})
    merged = dict(primary)
    merged["records"] = records
    if extras:
        merged["extra_manifests"] = sources
        merged["annotation"] = (primary.get("annotation") or "") + "; merged station wake events"
    return merged


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


def parse_pointwise_filters(text):
    """Parse the comma-separated MixedNet pointwise widths."""
    parts = [item.strip() for item in str(text).split(",")]
    if any(not item for item in parts):
        raise ValueError("Pointwise filters must not contain empty widths")
    try:
        filters = tuple(int(item) for item in parts)
    except ValueError as error:
        raise ValueError("Pointwise filters must be comma-separated integers") from error
    if not filters or any(value <= 0 for value in filters):
        raise ValueError("Pointwise filters must be positive integers")
    return filters


def _declared_architecture(cfg, changed=False):
    keys = ("architecture", "pointwise_filters", "first_conv_filters", "repeat_in_block",
            "mixconv_kernel_sizes", "first_conv_kernel_size", "stride", "spectrogram_length")
    architecture = {key: cfg.get(key) for key in keys if key in cfg}
    architecture["changed"] = changed
    return architecture


def apply_architecture(cfg, pointwise=None, first_conv=None, from_scratch=False):
    """Apply optional MixedNet width overrides to a trainer config.

    Streaming contract keys (kernels, stride, spectrogram length, residual
    blocks) are deliberately untouched. Without an explicit override the config
    is preserved verbatim, so non-MixedNet architectures keep their own fields.
    A width that differs from the config the checkpoint was trained with cannot
    be warm-started: Keras would either fail on shape mismatch or silently load
    a partial model, so the guard requires random initialization instead.
    """
    updated = dict(cfg)
    if pointwise is None and first_conv is None:
        return updated, _declared_architecture(updated)
    if str(updated.get("architecture", "mixednet")).lower() != "mixednet":
        raise ValueError("Pointwise/first-conv width overrides apply only to mixednet")
    current_pointwise = parse_pointwise_filters(updated["pointwise_filters"])
    current_first = int(updated["first_conv_filters"])
    pointwise = tuple(pointwise) if pointwise is not None else current_pointwise
    first_conv = int(first_conv) if first_conv is not None else current_first
    if not pointwise or any(int(value) <= 0 for value in pointwise):
        raise ValueError("Pointwise filters must be positive integers")
    if first_conv <= 0:
        raise ValueError("First convolution filters must be positive")
    repeat = updated.get("repeat_in_block")
    if repeat:
        blocks = [int(item) for item in str(repeat).split(",") if item.strip()]
        if len(blocks) != len(pointwise):
            raise ValueError("Pointwise width count must match repeat_in_block blocks")
    if (pointwise, first_conv) != (current_pointwise, current_first) and not from_scratch:
        raise ValueError(
            "Changed MixedNet width requires --from-scratch; never load weights of another shape")
    updated["pointwise_filters"] = ",".join(str(int(value)) for value in pointwise)
    updated["first_conv_filters"] = first_conv
    return updated, _declared_architecture(
        updated, changed=(pointwise, first_conv) != (current_pointwise, current_first))


def model_parameter_counts(model):
    """Measured topology count; HDF5 file bytes are not a parameter count."""
    import numpy as np
    total = int(model.count_params())
    trainable = int(sum(int(np.size(weight)) for weight in model.trainable_weights))
    return {"total": total, "trainable": trainable}


def export_signature(details):
    """Plain-Python description of an exported tensor; NumPy ints are not JSON-safe."""
    return {"shape": [int(value) for value in details["shape"]], "dtype": str(details["dtype"])}


def freeze_batchnorm(model, batchnorm_type):
    """Freeze normalization before compile, including nested model layers."""
    count, visited = 0, set()
    def visit(layer):
        nonlocal count
        if id(layer) in visited:
            return
        visited.add(id(layer))
        if isinstance(layer, batchnorm_type):
            layer.trainable = False
            count += 1
        for child in getattr(layer, "layers", []):
            visit(child)
    visit(model)
    return count


def feature_window_bounds(end_seconds, count, length=204):
    # Microfrontend's first frame covers [0,30ms], then one frame every 10ms.
    end = min(count, int((end_seconds - .03 + 1e-7) / .01) + 1)
    return (end - length, end) if end >= length else None


def continuous_examples(audio, requests, extractor, rng, variants=32):
    """Extract whole-track features once per augmentation, then slice windows.

    requests contain only train/val labels. Test recordings never enter here.
    The returned partition is authoritative; augmented validation is prohibited.
    """
    if any(r["partition"] not in {"train", "val"} for r in requests):
        raise ValueError("Continuous extraction accepts train/val only")
    import numpy as np
    for variant in range(variants):
        active = [r for r in requests if r["partition"] == "train" or variant == 0]
        if not active:
            continue
        ratio = 1. if variant == 0 else float(rng.uniform(.95, 1.05))
        gain = 1. if variant == 0 else float(rng.uniform(.55, 1.6))
        augmented = audio.astype(np.float32) * gain
        if ratio != 1.:
            augmented = np.interp(np.arange(0, len(augmented), ratio), np.arange(len(augmented)), augmented)
        frames = extractor(np.clip(augmented, -32768, 32767).astype(np.int16))
        for request in active:
            jitter = 0. if variant == 0 else float(rng.uniform(-.1, .1))
            end = (request["end"] + jitter) / ratio
            bounds = feature_window_bounds(end, len(frames))
            if bounds is None or end > len(augmented) / 16000:
                continue
            yield request, frames[bounds[0]:bounds[1]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trainer", required=True, type=Path)
    parser.add_argument("--run", required=True, type=Path)
    parser.add_argument("--steps", type=int, default=1000)
    parser.add_argument("--registry", type=Path, help="Private model registry; defaults to trainer/training/model-registry/ricardo")
    parser.add_argument("--version", help="New immutable registry version ID")
    origin = parser.add_mutually_exclusive_group()
    origin.add_argument("--base-weights", type=Path)
    origin.add_argument("--from-scratch", action="store_true",
                        help="Random initialization; BatchNorm learns from the full mixture")
    parser.add_argument("--extra-features", type=Path,
                        help="Directory with positive/negative train/val .npy for an additional voice")
    parser.add_argument("--extra-manifest", type=Path, action="append", default=[],
                        help="Additional schema-1 manifest (e.g. station wake events); repeatable")
    parser.add_argument("--model-name", default="ru_ricardo_personal_v1")
    parser.add_argument("--manifest-version", type=int, default=2)
    parser.add_argument("--learning-rate", type=float, default=3e-5)
    parser.add_argument("--freeze-batchnorm", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--feature-mode", choices=["continuous", "reset"], default="continuous")
    parser.add_argument("--config", type=Path,
                        help="Absolute trainer YAML; defaults to <trainer>/ricardo.yaml")
    parser.add_argument("--pointwise-filters", type=parse_pointwise_filters,
                        help="MixedNet pointwise widths, e.g. 128,128,128,128 (requires --from-scratch)")
    parser.add_argument("--first-conv-filters", type=int,
                        help="MixedNet first convolution width, e.g. 64 (requires --from-scratch)")
    args = parser.parse_args()
    root, run = args.trainer.resolve(), args.run.resolve()
    registry_spec = importlib.util.spec_from_file_location("model_registry", Path(__file__).with_name("registry.py"))
    registry = importlib.util.module_from_spec(registry_spec)
    registry_spec.loader.exec_module(registry)
    registry_path = args.registry.resolve() if args.registry else root / "training/model-registry/ricardo"
    config_path = args.config if args.config else root / "ricardo.yaml"
    if not config_path.is_absolute():
        raise SystemExit("--config must be an absolute path; the script changes to the trainer root")
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
    cfg = trainer.load_config(str(config_path))
    cfg, architecture = apply_architecture(cfg, args.pointwise_filters,
                                           args.first_conv_filters, args.from_scratch)
    architecture.update(config_path=str(config_path),
                        config_sha256=hashlib.sha256(config_path.read_bytes()).hexdigest())
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
    if args.extra_manifest:
        manifest = merge_manifests(
            manifest, [json.loads(path.read_text()) for path in args.extra_manifest])
    data = {}
    for e in manifest["records"]:
        if (e["block"] == "background" and e["partition"] == "test") or e["source_split"] == "test":
            continue
        with wave.open(str(run / "recordings" / (e["id"] + ".wav"))) as wav:
            data[e["id"]] = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2")

    real = {key: [] for key in ["train_pos", "val_pos", "train_neg", "val_neg"]}
    val_pos_blocks = []
    train_pos_blocks = []
    hard = any(e.get("hard_negative") and not e.get("station_false_positive")
               and e["source_split"] == "train" for e in manifest["records"])
    station_entries = [e for e in manifest["records"] if e.get("station_false_positive")]
    station = bool(station_entries)
    synthetic_hard = manifest.get("synthetic_hard_features")
    if synthetic_hard and not hard:
        raise ValueError("Synthetic confusables require labelled real hard negatives")
    if station:
        if not all(entry.get("hard_negative") for entry in station_entries):
            raise ValueError("Station negatives must be marked hard_negative")
        for required in ("train", "val"):
            if not any(entry["source_split"] == required for entry in station_entries):
                raise ValueError("Station manifest has no " + required
                                 + "-day events; collect more days or change --seed")
    if station and synthetic_hard:
        raise ValueError("Station negatives and synthetic_hard_features cannot share a mixture yet")
    if station and args.extra_features:
        raise ValueError("Station negatives and --extra-features cannot share a mixture yet")
    if hard:
        real.update(hard_train_neg=[], hard_val_neg=[])
    if station:
        real.update(station_train_neg=[], station_val_neg=[])
    counts = {key: 0 for key in real}
    for e in manifest["records"]:
        if e["id"] not in data:
            continue
        audio = data[e["id"]]
        if args.feature_mode == "continuous" and (e["block"] != "background" or e.get("mined_events")):
            requests = []
            if e["block"] != "background":
                for target in e["targets"]:
                    if target["partition"] in {"train", "val"}:
                        requests.append({"end": target["end"] + .18,
                            "partition": target["partition"], "positive": True,
                            "block": target.get("speech_style", e["block"])})
            else:
                for event in e["mined_events"]:
                    part = negative_partition(e, event - 2.24, event)
                    if part in {"train", "val"}:
                        requests.append({"end": event, "partition": part,
                                         "positive": False, "block": "hard-negative"})
            for request, frames in continuous_examples(audio, requests, trainer.extract_all_frames,
                    rng, variants=int(e.get("augment_variants", 32))):
                part = request["partition"]
                key = part + "_pos" if request["positive"] else "hard_" + part + "_neg"
                real[key].append(frames)
                counts[key] += 1
                if request["positive"]:
                    (val_pos_blocks if part == "val" else train_pos_blocks).append(request["block"])
            continue
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
                            key = negative_key(e, partition)
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
                key = negative_key(e, partition)
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
    extra = None
    if args.extra_features:
        if not hard or synthetic_hard:
            raise ValueError("Additional voice mixture requires real hard negatives, without synthetic_hard_features")
        extra = {key: np.load(args.extra_features / (name + ".npy"), mmap_mode="r")
                 for key, name in [("train_pos", "positive_train"), ("train_neg", "negative_train"),
                                   ("val_pos", "positive_val"), ("val_neg", "negative_val")]}
        if any(len(arr) == 0 or arr.shape[1:] != (204, 40) for arr in extra.values()):
            raise ValueError("Additional voice has empty or incompatible feature arrays")
    model = trainer._build_model(cfg)
    parameters = model_parameter_counts(model)
    architecture.update(parameters)
    print("ARCHITECTURE", cfg.get("architecture"),
          "POINTWISE", architecture.get("pointwise_filters"),
          "FIRST_CONV", architecture.get("first_conv_filters"),
          "MIXCONV", architecture.get("mixconv_kernel_sizes"),
          "STRIDE", architecture.get("stride"),
          "SPEC", architecture.get("spectrogram_length"),
          "PARAMS", parameters["total"],
          "TRAINABLE", parameters["trainable"], flush=True)
    origin_hash = None
    if not args.from_scratch:
        weights = args.base_weights.resolve() if args.base_weights else old / "model/ru_ricardo_mww.weights.h5"
        try:
            model.load_weights(str(weights))
        except (ValueError, RuntimeError) as error:
            raise SystemExit(
                "Checkpoint shapes do not match this architecture: " + str(weights)
                + ". A width change requires --from-scratch; "
                + "refusing to partially restore old weights") from error
        origin_hash = hashlib.sha256(weights.read_bytes()).hexdigest()
    else:
        print("RANDOM_INITIALIZATION_NO_CHECKPOINT", flush=True)
    frozen_bn = freeze_batchnorm(model, tf.keras.layers.BatchNormalization) if args.freeze_batchnorm and not args.from_scratch else 0
    print("FROZEN_BATCHNORM", frozen_bn, flush=True)
    learning_rate = (tf.keras.optimizers.schedules.CosineDecay(args.learning_rate, args.steps, alpha=.01)
                     if args.from_scratch else args.learning_rate)
    model.compile(optimizer=tf.keras.optimizers.Adam(learning_rate), loss="binary_crossentropy", jit_compile=False,
                  metrics=[tf.keras.metrics.BinaryAccuracy(), tf.keras.metrics.Recall(), tf.keras.metrics.Precision()])
    batch = cfg["batch_size"]
    if batch % (8 if (hard or station) else 4):
        raise ValueError("Batch must divide into four source buckets")
    def batches():
        while True:
            xs, ys = [], []
            buckets = [(real, "train_pos", 1, batch // 4), (synthetic, "train_pos", 1, batch // 4),
                       (real, "train_neg", 0, batch // (8 if (hard or station) else 4)),
                       (synthetic, "train_neg", 0, batch // (8 if (hard or station) else 4))]
            if hard:
                buckets.append((real, "hard_train_neg", 0,
                                batch // (8 if (synthetic_hard or station) else 4)))
            if station:
                # Station false positives get their own bucket; without personal
                # hard negatives they take the whole hard-negative share.
                buckets.append((real, "station_train_neg", 0,
                                batch // (8 if hard else 4)))
            if synthetic_hard:
                buckets.append((real, "hard_synth_train_neg", 0, batch // 8))
            if extra is not None:
                # Half positive, half negative; each new voice supplies both labels.
                buckets = [(real, "train_pos", 1, batch // 8),
                           (synthetic, "train_pos", 1, batch // 4),
                           (extra, "train_pos", 1, batch // 8),
                           (real, "train_neg", 0, batch // 8),
                           (real, "hard_train_neg", 0, batch // 8),
                           (synthetic, "train_neg", 0, batch // 8),
                           (extra, "train_neg", 0, batch // 8)]
            for arrays, key, label, count in buckets:
                arr = arrays[key]
                xs.append(np.asarray(arr[rng.integers(0, len(arr), count)], dtype=np.float32))
                ys.append(np.full(count, label, dtype=np.float32))
            x, y = np.concatenate(xs), np.concatenate(ys)
            order = rng.permutation(batch)
            yield x[order], y[order]
    train = tf.data.Dataset.from_generator(batches, output_signature=(
        tf.TensorSpec((batch, 204, 40), tf.float32), tf.TensorSpec((batch,), tf.float32))).prefetch(2)
    if args.from_scratch:
        train = train.unbatch().map(trainer._make_spec_augment_fn(204, cfg),
                                   num_parallel_calls=4).batch(batch).prefetch(2)
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
    if station:
        sn = real["station_val_neg"]
        xval = np.concatenate([xval, sn])
        yval = np.concatenate([yval, np.zeros(len(sn))])
        valweights = np.concatenate([valweights, np.full(len(sn), 100 / len(sn))])
    if synthetic_hard:
        shn = real["hard_synth_val_neg"]
        xval = np.concatenate([xval, shn])
        yval = np.concatenate([yval, np.zeros(len(shn))])
        valweights = np.concatenate([np.full(len(vp), 200 / len(vp)), np.full(len(vn), 100 / len(vn)),
                                     np.ones(200), np.full(200, .5), np.full(len(hn), 100 / len(hn)),
                                     np.full(len(shn), 100 / len(shn))])
    if extra is not None:
        ep, en = np.asarray(extra["val_pos"][:200]), np.asarray(extra["val_neg"][:200])
        xval = np.concatenate([xval, ep, en])
        yval = np.concatenate([yval, np.ones(len(ep)), np.zeros(len(en))])
        valweights = np.concatenate([valweights, np.ones(len(ep)), np.ones(len(en))])
    if extra is not None:
        bucket_recipe = ("12.5% personal-positive / 25% original-positive / "
                         "12.5% extra-voice-positive / 12.5% personal-negative / "
                         "12.5% real-hard-negative / 12.5% original-negative / "
                         "12.5% extra-voice-negative")
    elif synthetic_hard:
        bucket_recipe = ("25% real-positive / 25% synthetic-positive / "
                         "12.5% real-hard-negative / 12.5% synthetic-confusable / "
                         "12.5% real-negative / 12.5% old-synthetic-negative")
    elif station and hard:
        bucket_recipe = ("25% real-positive / 25% synthetic-positive / "
                         "12.5% real-negative / 12.5% old-synthetic-negative / "
                         "12.5% personal-hard-negative / 12.5% station-false-positive")
    elif station:
        bucket_recipe = ("25% real-positive / 25% synthetic-positive / "
                         "12.5% real-negative / 12.5% old-synthetic-negative / "
                         "25% station-false-positive")
    elif hard:
        bucket_recipe = ("25% real-positive / 25% synthetic-positive / "
                         "25% hard-negative / 12.5% real-negative / "
                         "12.5% synthetic-negative")
    else:
        bucket_recipe = "25% real-positive / synthetic-positive / real-negative / synthetic-negative"
    best = modeldir / "best.weights.h5"
    selection = []
    protected_selection = (hard or station) and not args.from_scratch
    if protected_selection:
        protected = np.asarray(val_pos_blocks)
        reference = model.predict(vp, batch_size=batch, verbose=0).reshape(-1)
        reference_counts = {block: int(np.sum(reference[protected == block] >= .9)) for block in set(val_pos_blocks)}
        hard_val_parts = []
        if hard:
            hard_val_parts.append(real["hard_val_neg"])
        if synthetic_hard:
            hard_val_parts.append(real["hard_synth_val_neg"])
        if station:
            hard_val_parts.append(real["station_val_neg"])
        hard_val = np.concatenate(hard_val_parts)
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
        "training_buckets": bucket_recipe,
        "initialization": "random" if args.from_scratch else "checkpoint",
        "learning_rate_schedule": "cosine to 1%" if args.from_scratch else "constant",
        "spec_augment": args.from_scratch,
        "class_weights": {"0": cfg["negative_class_weight"], "1": cfg["positive_class_weight"]} if args.from_scratch else None,
        "additional_voice_features": str(args.extra_features) if args.extra_features else None,
        "extra_manifests": [str(path) for path in args.extra_manifest],
        "mixture": {"real_positive": .125, "old_positive": .25, "extra_positive": .125,
                    "real_negative": .125, "hard_negative": .125, "old_negative": .125,
                    "extra_negative": .125} if extra is not None else None,
        "feature_mode": args.feature_mode,
        "architecture": architecture,
        "freeze_batchnorm": args.freeze_batchnorm and not args.from_scratch,
        "frozen_batchnorm_layers": frozen_bn,
        "test_used_in_training": False, "steps": args.steps}, indent=2))
    print("START_GPU_FINETUNING", args.steps, "steps", flush=True)
    history = model.fit(train, epochs=max(1, args.steps // 100), steps_per_epoch=100,
              class_weight={0: cfg["negative_class_weight"], 1: cfg["positive_class_weight"]} if args.from_scratch else None,
              validation_data=(xval, yval, valweights), validation_batch_size=batch, callbacks=selection + [
                  *([] if protected_selection else [tf.keras.callbacks.ModelCheckpoint(str(best), monitor="val_loss", save_best_only=True, save_weights_only=True)]),
                  tf.keras.callbacks.ModelCheckpoint(str(modeldir / "epochs/epoch-{epoch:03d}.weights.h5"), save_weights_only=True),
                  tf.keras.callbacks.CSVLogger(str(modeldir / "training-metrics.csv")),
                  tf.keras.callbacks.EarlyStopping(monitor="deployment_selection" if protected_selection else "val_loss", mode="min", patience=10 if protected_selection else 8 if args.from_scratch else 3, start_from_epoch=4 if protected_selection else 10 if args.from_scratch else 0, restore_best_weights=True)], verbose=2)
    provenance = json.loads((run / "training-provenance.json").read_text())
    provenance.update(completed_epochs=len(history.epoch), completed_steps=len(history.epoch) * 100,
                      selection="protected validation per speech block + hard negatives" if protected_selection else "val_loss",
                      selected_epoch=selection[0].selected_epoch if protected_selection else None)
    (run / "training-provenance.json").write_text(json.dumps(provenance, indent=2))
    model.load_weights(str(best))
    model.save_weights(str(modeldir / (cfg["model_name"] + "_mww.weights.h5")))
    # Cover every recorded speech style rather than taking only the first track.
    indices = balanced_indices(train_pos_blocks, 250)
    calibration = np.concatenate([real["train_pos"][indices], np.asarray(synthetic["train_pos"][:250])])
    provenance["calibration"] = {"real_train_indices": indices,
        "speech_blocks": sorted(set(train_pos_blocks)), "synthetic_train_examples": 250}
    if extra is not None:
        calibration = np.concatenate([calibration[:375], np.asarray(extra["train_pos"][:125])])
        provenance["calibration"].update(synthetic_train_examples=125, extra_voice_train_examples=125)
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
    provenance["export"] = {"path": path.name, "bytes": int(path.stat().st_size),
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "input": export_signature(inp), "output": export_signature(out)}
    (run / "training-provenance.json").write_text(json.dumps(provenance, indent=2))
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
        for name in ["synthetic-recipe.json", "generate_synthetic.py", "extra-features-provenance.json", "puriso-quality.json", "voice-lock.json"]:
            if (run / name).is_file():
                shutil.copyfile(run / name, stage / name)
        saved = registry.snapshot(registry_path, version, stage, {
            "role": "candidate, not deployed", "personal_voice": True,
            "parent_weights_sha256": origin_hash,
            "initialization": "random" if args.from_scratch else "checkpoint",
            "epoch_checkpoints": "all completed epochs"})
        print("REGISTRY_SAVED", saved.name, "verified files", registry.verify(saved), flush=True)


if __name__ == "__main__":
    main()
