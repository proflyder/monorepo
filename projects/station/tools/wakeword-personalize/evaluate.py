#!/usr/bin/env python3
"""Offline streaming replay; never contacts the station. Results are private."""
import argparse
from collections import deque
import hashlib
import json
import os
from pathlib import Path
import wave

os.environ.setdefault("CUDA_VISIBLE_DEVICES", "-1")
os.environ.setdefault("TF_CPP_MIN_LOG_LEVEL", "3")


def replay(model, pcm, cutoff=.9, window=10):
    if not 0 < cutoff <= 1 or not isinstance(window, int) or window < 1:
        raise ValueError("Invalid detector cutoff/window")
    import numpy as np
    import tensorflow as tf
    from pymicro_features import MicroFrontend
    hits, scores = [], []
    front = MicroFrontend()
    frames, probs, warmup = [], deque(maxlen=window), 100

    def interpreter():
        obj = tf.lite.Interpreter(model_path=str(model), num_threads=2)
        obj.allocate_tensors()
        inp, out = obj.get_input_details()[0], obj.get_output_details()[0]
        if list(inp["shape"]) != [1, 3, 40] or inp["dtype"] != np.int8 or out["dtype"] != np.uint8:
            raise ValueError("Expected station-compatible int8 [1,3,40] -> uint8")
        return obj, inp, out

    obj, inp, out = interpreter()
    maximum = 0.
    for offset in range(0, len(pcm) - 159, 160):
        result = front.process_samples(pcm[offset:offset + 160].tobytes())
        if result is None or not len(result.features):
            continue
        frames.append(list(result.features))
        if len(frames) != 3:
            continue
        scale, zero = inp["quantization"]
        values = np.clip(np.rint(np.asarray(frames) / scale + zero), -128, 127).astype(np.int8)
        obj.set_tensor(inp["index"], values[None])
        obj.invoke()
        scale, zero = out["quantization"]
        prob = (float(obj.get_tensor(out["index"]).flat[0]) - zero) * scale
        frames.clear()
        probs.append(prob)
        if warmup > 0:
            warmup -= 3
            continue
        if len(probs) != window:
            continue
        mean = sum(probs) / window
        maximum = max(maximum, mean)
        when = (offset + 160) / 16000
        scores.append([round(when, 3), round(mean, 5)])
        if mean >= cutoff:
            hits.append(round(when, 3))
            # The deployed C++ shim recreates interpreter + frontend after a hit.
            obj, inp, out = interpreter()
            front = MicroFrontend()
            frames.clear()
            probs.clear()
            warmup = 100
    return {"hits": hits, "max_score": maximum, "scores": scores}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--recordings", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cutoff", type=float, default=.9)
    parser.add_argument("--window", type=int, default=10)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    report = {"model_sha256": hashlib.sha256(args.model.read_bytes()).hexdigest(),
              "cutoff": args.cutoff, "window": args.window, "warmup_frames": 100, "records": []}
    import numpy as np
    for path in sorted(args.recordings.glob("*.json")):
        if path.name.startswith("."):
            continue
        meta = json.loads(path.read_text())
        if meta.get("excluded"):
            continue
        with wave.open(str(path.with_suffix(".wav"))) as wav:
            if (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) != (16000, 1, 2):
                raise ValueError("Only original 16kHz mono PCM16 recordings are accepted")
            pcm = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2")
        result = replay(args.model, pcm, args.cutoff, args.window)
        report["records"].append({"id": meta["id"], "split": meta["split"], "block": meta["block"],
                                  "seconds": len(pcm) / 16000, **result})
        args.output.write_text(json.dumps(report, ensure_ascii=False))
        print(meta["split"], meta["block"], "hits", len(result["hits"]), flush=True)


if __name__ == "__main__":
    main()
