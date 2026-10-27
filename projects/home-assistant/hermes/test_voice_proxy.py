#!/usr/bin/env python3
"""Unit tests for the Hermes voice/TTS response sanitizer."""
from __future__ import annotations

import unittest
import io
import importlib.util
import sys
import json
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
_spec = importlib.util.spec_from_file_location(
    "voice_proxy",
    ROOT / "projects" / "home-assistant" / "hermes" / "voice_proxy.py",
)
assert _spec and _spec.loader
voice_proxy = importlib.util.module_from_spec(_spec)
sys.modules["voice_proxy"] = voice_proxy
_spec.loader.exec_module(voice_proxy)
StreamingSanitizer = voice_proxy.StreamingSanitizer
prepare_chat_request = voice_proxy.prepare_chat_request
sanitize_chat_payload = voice_proxy.sanitize_chat_payload
sanitize_text = voice_proxy.sanitize_text
sse_events = voice_proxy.sse_events


class SanitizeTests(unittest.TestCase):
    def test_removes_markdown_link_and_url(self):
        text = "Новость. ([investing.com](https://example.com/a?b=1))"
        self.assertEqual(sanitize_text(text), "Новость.")

    def test_removes_bare_url(self):
        text = "Смотри https://example.com/news и всё."
        self.assertEqual(sanitize_text(text), "Смотри и всё.")

    def test_unwraps_link_text_without_url(self):
        text = "Источник: [Reuters](https://reuters.com/x)."
        self.assertEqual(sanitize_text(text), "Источник: Reuters.")

    def test_strips_markdown_marks_and_list_prefix(self):
        text = "## Заголовок\n- **важно**\n- `код`"
        self.assertEqual(sanitize_text(text), "Заголовок важно код")

    def test_keeps_normal_punctuation_and_parentheses(self):
        text = "Это тест (не ссылка), всё нормально."
        self.assertEqual(sanitize_text(text), text)

    def test_collapses_whitespace(self):
        self.assertEqual(sanitize_text("  привет \n\n  мир  "), "привет мир")

    def test_empty_and_none_values(self):
        self.assertEqual(sanitize_text(""), "")
        self.assertEqual(sanitize_text(None), "")  # type: ignore[arg-type]

    def test_sanitizes_all_choices(self):
        payload = {
            "choices": [
                {"message": {"content": "A https://a.example"}},
                {"message": {"content": "B [x](https://b.example)"}},
            ]
        }
        out = sanitize_chat_payload(payload)
        self.assertEqual(out["choices"][0]["message"]["content"], "A")
        self.assertEqual(out["choices"][1]["message"]["content"], "B x")


class StreamingTests(unittest.TestCase):
    def test_first_sentence_released_before_completion(self):
        s = StreamingSanitizer()
        self.assertEqual(s.feed("Первое предло"), "")
        self.assertEqual(s.feed("жение. Второе"), "Первое предложение.")
        self.assertEqual(s.feed(" предложение.", final=True), " Второе предложение.")

    def test_links_citations_and_urls_at_every_split(self):
        samples = [
            "Новость. ([investing.com](https://example.com/a?b=1)) Ещё новость.",
            "Источник: [Reuters](https://reuters.com/x). Следующий ответ.",
            "Смотри https://example.com/news и всё. Второе предложение.",
            "Ответ (не ссылка). **Второе** предложение.",
        ]
        for text in samples:
            for split in range(len(text) + 1):
                with self.subTest(text=text, split=split):
                    s = StreamingSanitizer()
                    result = s.feed(text[:split]) + s.feed(text[split:]) + s.feed("", final=True)
                    self.assertEqual(result, sanitize_text(text))
            s = StreamingSanitizer()
            result = "".join(s.feed(c) for c in text) + s.feed("", final=True)
            self.assertEqual(result, sanitize_text(text))

    def test_unfinished_link_is_discarded(self):
        s = StreamingSanitizer()
        self.assertEqual(s.feed("Ответ [источник](https://example", final=True), "Ответ")

    def test_sse_multiline_crlf_comments_and_unicode(self):
        data = ': ping\r\nevent: hermes.status\r\ndata: {"text":\r\ndata: "Привет"}\r\n\r\ndata: [DONE]\r\n\r\n'
        self.assertEqual(list(sse_events(io.BytesIO(data.encode()))), [
            ("keepalive", ""), ("hermes.status", '{"text":\n"Привет"}'), ("message", "[DONE]"),
        ])

    def test_reasoning_override_is_voice_only_and_preserves_options(self):
        original = {"stream": True, "model_options": {"reasoning_effort": "high", "fast": False}}
        prepared = prepare_chat_request(original)
        self.assertEqual(prepared["model_options"], {"reasoning_effort": "low", "fast": False})
        self.assertEqual(original["model_options"]["reasoning_effort"], "high")


class ProxyTransportTests(unittest.TestCase):
    """Real HTTP checks with a gated upstream: buffering the full answer must fail."""

    def setUp(self):
        self.release = threading.Event()
        self.received = None
        self.mode = "normal"
        case = self

        class Upstream(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                case.received = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if case.mode == "http_error":
                    self.send_response(429)
                    self.end_headers()
                    self.wfile.write(b'{"error":{"message":"limited"}}')
                    return
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()

                def send(delta, finish=None):
                    chunk = {"id": "test", "object": "chat.completion.chunk", "created": 1,
                             "model": "hermes-agent", "choices": [
                                 {"index": 0, "delta": delta, "finish_reason": finish}]}
                    self.wfile.write(("data: " + json.dumps(chunk) + "\n\n").encode())
                    self.wfile.flush()

                send({"role": "assistant"})
                self.wfile.write(b'event: hermes.tool.progress\ndata: {"tool":"private"}\n\n')
                send({"reasoning_content": "SECRET THINKING"})
                send({"content": "Первое предложение. "})
                case.release.wait(5)
                if case.mode == "truncated":
                    return
                send({"content": "Источник: [Reuters](https://example.com)."})
                send({}, "stop")
                self.wfile.write(b"data: [DONE]\n\n")

        class QuietProxy(voice_proxy.Handler):
            def log_message(self, *args):
                pass

        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        self.proxy = ThreadingHTTPServer(("127.0.0.1", 0), QuietProxy)
        self.override = patch.object(voice_proxy, "UPSTREAM", f"http://127.0.0.1:{self.upstream.server_port}")
        self.override.start()
        for server in (self.upstream, self.proxy):
            threading.Thread(target=server.serve_forever, daemon=True).start()

    def tearDown(self):
        self.release.set()
        for server in (self.proxy, self.upstream):
            server.shutdown()
            server.server_close()
        self.override.stop()

    def request(self):
        return urllib.request.urlopen(urllib.request.Request(
            f"http://127.0.0.1:{self.proxy.server_port}/v1/chat/completions",
            data=json.dumps({"model": "hermes-agent", "stream": True,
                             "messages": [{"role": "user", "content": "hi"}]}).encode(),
            headers={"Content-Type": "application/json"}), timeout=2)

    def test_early_delivery_clean_tail_and_ha_finish_order(self):
        with self.request() as response:
            events = sse_events(response)
            first = next(events)
            self.assertEqual(json.loads(first[1])["choices"][0]["delta"], {"role": "assistant"})
            early = json.loads(next(events)[1])
            self.assertEqual(early["choices"][0]["delta"]["content"], "Первое предложение.\n\n")
            self.assertFalse(self.release.is_set())
            self.release.set()
            remaining = list(events)
        self.assertTrue(self.received["stream"])
        self.assertEqual(self.received["model_options"]["reasoning_effort"], "low")
        self.assertEqual(remaining[-1], ("message", "[DONE]"))
        chunks = [early] + [json.loads(data) for _, data in remaining[:-1]]
        # Mirror HA's rule: a finish frame terminates reading before its delta is used.
        spoken = ""
        for chunk in chunks:
            choice = chunk["choices"][0]
            if choice["finish_reason"]:
                break
            spoken += choice["delta"].get("content", "")
        self.assertEqual(sanitize_text(spoken), "Первое предложение. Источник: Reuters.")
        self.assertNotIn("SECRET", str(chunks))
        self.assertNotIn("private", str(chunks))

    def test_midstream_disconnect_is_error_without_second_http_response(self):
        self.mode = "truncated"
        self.release.set()
        with self.request() as response:
            raw = response.read().decode()
        self.assertIn('"error"', raw)
        self.assertNotIn("[DONE]", raw)
        self.assertNotIn("HTTP/", raw)

    def test_http_error_keeps_status(self):
        self.mode = "http_error"
        with self.assertRaises(urllib.error.HTTPError) as ctx:
            self.request()
        self.assertEqual(ctx.exception.code, 429)


if __name__ == "__main__":
    unittest.main(verbosity=2)
