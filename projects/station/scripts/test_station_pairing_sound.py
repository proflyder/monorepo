#!/usr/bin/env python3
"""Tests for the local sound-pairing CLI (not the Telegram bot)."""
from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "projects" / "station" / "scripts" / "station-pairing-sound.py"
PAIRING_LIB = (
    ROOT / "projects" / "station" / "resources" / "quasar-connect-bot" / "pairing.py"
)


@unittest.skipUnless(
    PAIRING_LIB.is_file(),
    "quasar-connect-bot checkout is absent (ignored resource; clone it to run this test)",
)
class PairingSoundCliTests(unittest.TestCase):
    def test_wav_generation_and_token_redaction(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "pair.wav"
            res = subprocess.run(
                ["python3", str(SCRIPT), "--ssid", "Home", "--password", "password123", "--out", str(out)],
                text=True,
                capture_output=True,
                timeout=30,
            )
            self.assertEqual(res.returncode, 0, res.stderr)
            self.assertTrue(out.exists())
            self.assertIn("token: random non-empty (not printed)", res.stdout)
            self.assertNotRegex(res.stdout.split("token:", 1)[-1], r"[0-9]{6,}")
            with wave.open(str(out)) as wav:
                self.assertEqual(wav.getnchannels(), 1)
                self.assertGreater(wav.getnframes(), 1000)

    @unittest.skipUnless(shutil.which("ffmpeg"), "ffmpeg is not installed")
    def test_ogg_generation(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "pair.ogg"
            res = subprocess.run(
                ["python3", str(SCRIPT), "--ssid", "Home", "--password", "password123", "--format", "ogg", "--out", str(out)],
                text=True,
                capture_output=True,
                timeout=60,
            )
            self.assertEqual(res.returncode, 0, res.stderr)
            self.assertTrue(out.exists())
            self.assertGreater(out.stat().st_size, 100)


if __name__ == "__main__":
    unittest.main(verbosity=2)
