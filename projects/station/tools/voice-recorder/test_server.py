import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import threading
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen
import wave
from unittest.mock import patch
from http.server import ThreadingHTTPServer

SPEC = importlib.util.spec_from_file_location("voice_recorder", Path(__file__).with_name("server.py"))
app = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(app)


class RecordingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.rec = app.Recorder("unused", Path(self.tmp.name), Path("unused"))
        self.rec.state = "ready"

    def tearDown(self):
        self.rec.finish_take()
        self.tmp.cleanup()

    def test_pcm_and_metadata_roundtrip(self):
        self.rec.start_take("whisper", "test", "quiet room")
        samples = [12, -40, 32767, -32768] * 4000
        pcm = struct.pack("<" + "h" * len(samples), *samples)
        self.rec.consume(pcm)
        self.rec.finish_take()
        files = self.rec.status()["files"]
        self.assertEqual(len(files), 1)
        meta = files[0]
        self.assertEqual((meta["block"], meta["split"], meta["frames"]), ("whisper", "test", 16000))
        with wave.open(str(Path(self.tmp.name) / (meta["id"] + ".wav"))) as wav:
            self.assertEqual((wav.getframerate(), wav.getnchannels(), wav.getsampwidth()), (16000, 1, 2))
            self.assertEqual(wav.readframes(16000), pcm)

    def test_invalid_label_and_overlapping_takes_rejected(self):
        with self.assertRaises(ValueError): self.rec.start_take("../bad", "train", "")
        self.rec.start_take("normal", "train", "")
        with self.assertRaises(ValueError): self.rec.start_take("normal", "train", "")

    def test_interrupted_take_preserved_and_exclusion_reversible(self):
        self.rec.start_take("normal", "train", "")
        self.rec.consume(b"\x01\x00" * 100)
        self.rec.finish_take(interrupted=True)
        meta = self.rec.status()["files"][0]
        self.assertTrue(meta["interrupted"])
        self.rec.exclude(meta["id"], True)
        self.assertTrue(self.rec.status()["files"][0]["excluded"])
        self.assertTrue((Path(self.tmp.name) / (meta["id"] + ".wav")).exists())
        self.rec.exclude(meta["id"], False)
        self.assertFalse(self.rec.status()["files"][0]["excluded"])
        with self.assertRaises(ValueError): self.rec.exclude("../outside", True)

    def test_inactive_microphone_refuses_take(self):
        self.rec.state = "idle"
        with self.assertRaises(ValueError): self.rec.start_take("normal", "train", "")

    def test_disk_failure_does_not_prevent_device_stop(self):
        self.rec.restored = False
        self.rec.session = "a" * 32
        with patch.object(self.rec, "finish_take", side_effect=OSError("disk full")), patch.object(self.rec, "shell") as shell:
            self.rec.stop()
        self.assertEqual(self.rec.state, "stopping")
        self.assertTrue(self.rec.last_error)
        self.assertIn("/stop", shell.call_args.args[0])

    def test_long_background_capture_preserves_voice_take_limit(self):
        with patch.object(app.time, "monotonic", return_value=0) as clock:
            self.rec.start_take("background", "train", "")
            clock.return_value = 181
            self.rec.consume(b"\x01\x00" * 512)
            self.assertIsNotNone(self.rec.take)
            clock.return_value = 481
            self.rec.consume(b"\x01\x00" * 512)
            self.assertIsNone(self.rec.take)
            self.assertEqual(self.rec.status()["files"][0]["frames"], 1024)
            self.rec.start_take("normal", "train", "")
            clock.return_value = 662
            self.rec.consume(b"\x01\x00" * 512)
            self.assertIsNone(self.rec.take)


class HTTPTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), app.Handler)
        self.server.recorder = app.Recorder("unused", Path(self.tmp.name), Path("unused"))
        self.server.authority = "127.0.0.1:" + str(self.server.server_port)
        self.server.token = "unit-test-only"
        self.url = "http://" + self.server.authority
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.tmp.cleanup()

    def request(self, path, body=None, headers=None):
        data = json.dumps(body).encode() if body is not None else None
        return urlopen(Request(self.url + path, data=data, headers=headers or {}), timeout=2)

    def test_host_and_csrf_protection(self):
        for headers in ({}, {"Origin": self.url}, {"Origin": "http://evil.invalid", "X-Recorder-Token": self.server.token}):
            with self.assertRaises(HTTPError) as err: self.request("/api/stop", {}, headers)
            self.assertEqual(err.exception.code, 403)
        with self.assertRaises(HTTPError) as err: self.request("/", headers={"Host": "evil.invalid"})
        self.assertEqual(err.exception.code, 403)
        with self.request("/api/stop", {}, {"Origin": self.url, "X-Recorder-Token": self.server.token}) as reply:
            self.assertEqual(reply.status, 200)

    def test_no_path_traversal_and_page_has_csp(self):
        with self.request("/") as reply:
            self.assertIn("frame-ancestors 'none'", reply.headers["Content-Security-Policy"])
        with self.assertRaises(HTTPError) as err: self.request("/audio/../../secrets")
        self.assertEqual(err.exception.code, 404)

    def test_cross_site_requests_cannot_keep_recording_alive(self):
        with self.assertRaises(HTTPError) as err: self.request("/api/status")
        self.assertEqual(err.exception.code, 403)
        with self.request("/api/status", headers={"X-Recorder-Token": self.server.token}) as reply:
            self.assertEqual(reply.status, 200)

    def test_start_requires_explicit_consent(self):
        headers = {"Origin": self.url, "X-Recorder-Token": self.server.token}
        with self.assertRaises(HTTPError) as err: self.request("/api/start", {}, headers)
        self.assertEqual(err.exception.code, 400)
        self.assertEqual(self.server.recorder.state, "idle")

if __name__ == "__main__":
    unittest.main()
