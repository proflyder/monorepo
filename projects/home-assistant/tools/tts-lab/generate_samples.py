#!/usr/bin/env python3
"""Generate TTS audition samples for the station voice lab.

This is a lab tool, not production code. It uses Silero v5 CIS base
(`v5_cis_base_nostress`, MIT) and writes 48 kHz mono WAV files.

Example:
    python3 -m venv .venv
    . .venv/bin/activate
    pip install torch silero soundfile
    python generate_samples.py --out ../../artifacts/tts-samples/silero-v5-cis
"""

from __future__ import annotations

import argparse
import os
import time
import urllib.request
from pathlib import Path

import soundfile as sf
import torch
from torch import package

MODEL_URL = (
    "https://huggingface.co/adeshkin/"
    "silero-models-v5-cis-base-nostress/resolve/main/"
    "v5_cis_base_nostress.pt"
)
DEFAULT_VOICES = [
    "ru_alexandr",
    "ru_bogdan",
    "ru_dmitriy",
    "ru_gamat",
    "ru_igor",
    "ru_marat",
    "ru_roman",
    "ru_safarhuja",
    "ru_sibday",
    "ru_eduard",
]
SHORT = "Привет! Я Рикардо. Чем могу помочь?"
LONG = (
    "Сейчас в Москве плюс двадцать три градуса, небольшой ветер. "
    "Будильник на семь тридцать установлен."
)


def download(url: str, dst: Path) -> Path:
    if dst.exists() and dst.stat().st_size > 80_000_000:
        return dst
    dst.parent.mkdir(parents=True, exist_ok=True)
    print(f"downloading {url} -> {dst}")
    tmp = dst.with_suffix(dst.suffix + ".part")
    urllib.request.urlretrieve(url, tmp)
    tmp.replace(dst)
    return dst


def load_model(path: Path):
    imp = package.PackageImporter(str(path))
    return imp.load_pickle("tts_models", "model")


def main() -> None:
    ap = argparse.ArgumentParser()
    default_out = Path(__file__).resolve().parents[4] / "artifacts/tts-samples/silero-v5-cis"
    ap.add_argument("--out", default=str(default_out))
    ap.add_argument("--model", default="")
    ap.add_argument("--voices", default=",".join(DEFAULT_VOICES))
    ap.add_argument("--text-short", default=SHORT)
    ap.add_argument("--text-long", default=LONG)
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    model_path = Path(args.model) if args.model else Path(
        os.environ.get("TTS_LAB_CACHE", Path.home() / ".cache" / "tts-lab")
    ) / "v5_cis_base_nostress.pt"
    if not model_path.exists():
        download(MODEL_URL, model_path)

    model = load_model(model_path)
    for voice in [v.strip() for v in args.voices.split(",") if v.strip()]:
        for tag, text in (("short", args.text_short), ("long", args.text_long)):
            t0 = time.time()
            audio = model.apply_tts(
                text=text, speaker=voice, sample_rate=48000
            ).detach().cpu().numpy()
            dt = time.time() - t0
            dur = len(audio) / 48000
            dst = out / f"{voice}_{tag}.wav"
            sf.write(dst, audio, 48000)
            print(
                f"{voice:14s} {tag:5s} dur={dur:5.2f}s "
                f"synth={dt:5.2f}s RTF={dt / dur:.3f} {dst}"
            )


if __name__ == "__main__":
    main()
