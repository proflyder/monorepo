#!/usr/bin/env python3
"""Unit tests for the Hermes voice/TTS response sanitizer."""
from __future__ import annotations

import unittest
import io
import importlib.util
import sys
import json
import threading
import time
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


class CancelGuardTests(unittest.TestCase):
    def test_exact_normalized_cancel_phrases_match(self):
        for text in ["отмена", "Отмена!", "  отмени  ", "  Отмена!  ", "Передумала.",
                     "не надо", "НИЧЕГО", "отбой…", "забудь", "Неважно?"]:
            with self.subTest(text=text):
                payload = {"messages": [{"role": "user", "content": text}]}
                self.assertIsNotNone(voice_proxy.cancel_short_circuit(payload))

    def test_extra_meaning_and_non_user_roles_do_not_match(self):
        negatives = [
            [{"role": "user", "content": "отмена будильника"}],
            [{"role": "user", "content": "не надо музыку"}],
            [{"role": "user", "content": "не отмена"}],
            [{"role": "user", "content": "как отменить"}],
            [{"role": "user", "content": "«отмена»"}],
            [{"role": "user", "content": '"отмена"'}],
            [{"role": "user", "content": "от-мена"}],
            [{"role": "user", "content": "не, надо"}],
            [{"role": "user", "content": ""}],
            [{"role": "assistant", "content": "отмена"}],
            [{"role": "tool", "content": "отмена", "tool_call_id": "1"}],
            [{"role": "user", "content": [{"type": "text", "text": "отмена"}]}],
            [{"role": "user", "content": "привет"}],
        ]
        for messages in negatives:
            with self.subTest(messages=messages):
                self.assertIsNone(voice_proxy.cancel_short_circuit({"messages": messages}))

    def test_only_last_user_message_is_considered(self):
        payload = {"messages": [
            {"role": "user", "content": "отмена"},
            {"role": "assistant", "content": "Что отменить?"},
            {"role": "user", "content": "включи музыку"},
        ]}
        self.assertIsNone(voice_proxy.cancel_short_circuit(payload))


class CancelGuardTransportTests(unittest.TestCase):
    """The guard answers locally and never opens an upstream connection."""

    def setUp(self):
        self.received = None
        case = self

        class Upstream(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                case.received = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"choices":[{"message":{"role":"assistant","content":"upstream"}}]}')

        class QuietProxy(voice_proxy.Handler):
            def log_message(self, *args):
                pass

        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        self.proxy = ThreadingHTTPServer(("127.0.0.1", 0), QuietProxy)
        self.override = patch.object(
            voice_proxy, "UPSTREAM", f"http://127.0.0.1:{self.upstream.server_port}")
        self.override.start()
        for server in (self.proxy, self.upstream):
            threading.Thread(target=server.serve_forever, daemon=True).start()

    def tearDown(self):
        for server in (self.proxy, self.upstream):
            server.shutdown()
            server.server_close()
        self.override.stop()

    def request(self, text, stream=False, role="user"):
        payload = {"model": "hermes-agent", "stream": stream,
                   "messages": [{"role": role, "content": text}]}
        return urllib.request.urlopen(urllib.request.Request(
            f"http://127.0.0.1:{self.proxy.server_port}/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"}), timeout=2)

    def test_non_stream_cancel_returns_empty_assistant_without_upstream(self):
        with self.request("отмена") as response:
            body = json.loads(response.read())
        self.assertIsNone(self.received)
        choice = body["choices"][0]
        self.assertEqual(choice["finish_reason"], "stop")
        self.assertEqual(choice["message"]["role"], "assistant")
        self.assertEqual((choice["message"]["content"] or "").strip(), "")

    def test_stream_cancel_has_role_finish_done_and_no_upstream(self):
        with self.request("Отмена!", stream=True) as response:
            events = list(sse_events(response))
        self.assertIsNone(self.received)
        self.assertEqual(events[-1], ("message", "[DONE]"))
        chunks = [json.loads(data) for _, data in events[:-1]]
        self.assertEqual(chunks[0]["choices"][0]["delta"]["role"], "assistant")
        self.assertEqual(chunks[1]["choices"][0]["finish_reason"], "stop")
        spoken = "".join(
            chunk["choices"][0]["delta"].get("content", "")
            for chunk in chunks if not chunk["choices"][0]["finish_reason"]
        )
        self.assertEqual(spoken.strip(), "")

    def test_tool_continuation_and_free_text_still_reach_upstream(self):
        self.request("отмена", role="tool")
        self.assertIsNotNone(self.received)
        self.received = None
        self.request("включи музыку")
        self.assertIsNotNone(self.received)


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


class StartupRecoveryTests(unittest.TestCase):
    def test_unavailable_upstream_is_openai_error_and_recovers_without_proxy_restart(self):
        class Proxy(voice_proxy.Handler):
            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        url = f"http://127.0.0.1:{server.server_port}/v1/models"
        try:
            with patch.object(Proxy, "_upstream", side_effect=urllib.error.URLError("refused")):
                for method, body in [("GET", None), ("POST", b'{"stream": false}')]:
                    with self.subTest(method=method), self.assertRaises(urllib.error.HTTPError) as ctx:
                        urllib.request.urlopen(urllib.request.Request(url, data=body, method=method))
                    self.assertEqual(ctx.exception.code, 503)
                    error = json.loads(ctx.exception.read())["error"]
                    # The HA llama_cpp parser calls .get on this object.
                    self.assertIsInstance(error, dict)
                    self.assertTrue(error.get("message"))
            with patch.object(Proxy, "_upstream", return_value=(200, b'{"data": []}', "application/json")):
                with urllib.request.urlopen(url) as response:
                    self.assertEqual(response.status, 200)
        finally:
            server.shutdown()
            server.server_close()

    def test_http_errors_normalize_legacy_string_or_html_and_keep_valid_error(self):
        for original in [b'{"error": "not ready"}', b'<html>not ready</html>', b'{"error": null}']:
            with self.subTest(body=original):
                body, content_type = voice_proxy.normalize_error(502, original, "text/html")
                self.assertEqual(content_type, "application/json")
                self.assertTrue(json.loads(body)["error"].get("message"))
        valid = b'{"error":{"message":"limited","type":"rate_limit_error"}}'
        self.assertEqual(voice_proxy.normalize_error(429, valid, "application/json")[0], valid)


class VoiceRoutingTests(unittest.TestCase):
    """Profile routing through mcp-gateway with fake HA and gateway servers."""

    def setUp(self):
        case = self
        self.ha_state = "Личный"
        self.ha_down = False
        self.ha_calls = []
        self.announce_calls = []
        self.utterances = []
        self.results = []
        self.result_calls = []
        self.gateway_down = False
        self.gateway_reply = {"route": "hermes"}
        self.hermes_calls = []
        self.start_conversation_fails = False

        class HA(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _json(self, status, value):
                body = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                case.ha_calls.append(self.path)
                if case.ha_down:
                    self._json(503, {"error": "down"})
                    return
                self._json(200, {"entity_id": "input_select.ricardo_voice_profile",
                                 "state": case.ha_state})

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                payload = json.loads(self.rfile.read(length) or b"{}")
                case.announce_calls.append((self.path, payload))
                if case.start_conversation_fails and \
                        self.path.endswith("/api/services/assist_satellite/start_conversation"):
                    self._json(503, {"error": "unavailable"})
                    return
                self._json(200, {"changed_states": []})

        class Gateway(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def _json(self, status, value):
                body = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                payload = json.loads(self.rfile.read(length) or b"{}")
                if case.gateway_down:
                    self._json(503, {"error": "down"})
                    return
                if self.path.endswith("/v1/voice/utterance"):
                    case.utterances.append(payload)
                    self._json(200, case.gateway_reply)
                    return
                if self.path.endswith("/v1/voice/result"):
                    case.result_calls.append(payload)
                    reply = case.results.pop(0) if case.results else {"status": "running"}
                    self._json(200, reply)
                    return
                self._json(404, {"error": "unknown"})

        class Upstream(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                case.hermes_calls.append(json.loads(self.rfile.read(length) or b"{}"))
                body = json.dumps({"choices": [{
                    "message": {"role": "assistant", "content": "Ответ Hermes."},
                    "finish_reason": "stop"}]}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        class QuietProxy(voice_proxy.Handler):
            def log_message(self, *args):
                pass

        self.ha = ThreadingHTTPServer(("127.0.0.1", 0), HA)
        self.gateway = ThreadingHTTPServer(("127.0.0.1", 0), Gateway)
        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        self.proxy = ThreadingHTTPServer(("127.0.0.1", 0), QuietProxy)
        for server in (self.ha, self.gateway, self.upstream, self.proxy):
            threading.Thread(target=server.serve_forever,
                             kwargs={"poll_interval": 0.05}, daemon=True).start()
        voice_proxy._profile_cache = None
        voice_proxy._route_counts.clear()
        self.overrides = [
            patch.object(voice_proxy, "HA_URL", f"http://127.0.0.1:{self.ha.server_port}"),
            patch.object(voice_proxy, "HA_TOKEN", "test-ha-token"),
            patch.object(voice_proxy, "HA_TOKEN_FILE", ""),
            patch.object(voice_proxy, "VOICE_GATEWAY_URL",
                         f"http://127.0.0.1:{self.gateway.server_port}"),
            patch.object(voice_proxy, "VOICE_GATEWAY_TOKEN", "test-voice-token"),
            patch.object(voice_proxy, "VOICE_GATEWAY_TOKEN_FILE", ""),
            patch.object(voice_proxy, "UPSTREAM",
                         f"http://127.0.0.1:{self.upstream.server_port}"),
            patch.object(voice_proxy, "PROFILE_CACHE_MS", 2000),
            patch.object(voice_proxy, "VOICE_RESULT_RETRY", 0.05),
            patch.object(voice_proxy, "VOICE_RESULT_TIMEOUT", 5.0),
        ]
        for override in self.overrides:
            override.start()

    def tearDown(self):
        voice_proxy.wait_background_tasks(2.0)
        for server in (self.proxy, self.upstream, self.gateway, self.ha):
            server.shutdown()
            server.server_close()
        for override in self.overrides:
            override.stop()
        voice_proxy._profile_cache = None

    def wait_until(self, predicate, timeout=3.0, message="condition not met"):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            voice_proxy.wait_background_tasks(0.2)
            if predicate():
                return
            time.sleep(0.01)
        self.fail(message)

    def _open(self, text, stream=False, role="user"):
        payload = {"model": "hermes-agent", "stream": stream,
                   "messages": [{"role": role, "content": text}]}
        return urllib.request.urlopen(urllib.request.Request(
            f"http://127.0.0.1:{self.proxy.server_port}/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"}), timeout=5)

    def ask(self, text, stream=False, role="user"):
        """Return (spoken text, finish_seen) exactly as HA would read it."""
        with self._open(text, stream=stream, role=role) as response:
            if not stream:
                body = json.loads(response.read())
                choice = body["choices"][0]
                return choice["message"]["content"], choice["finish_reason"] == "stop"
            spoken, finished = "", False
            for event, data in sse_events(response):
                if data == "[DONE]":
                    continue
                chunk = json.loads(data)
                choice = chunk["choices"][0]
                if choice.get("finish_reason"):
                    finished = True
                    break
                spoken += choice["delta"].get("content", "")
        return spoken, finished

    # -- routing ----------------------------------------------------------

    def test_personal_profile_hermes_route_forwards_unchanged(self):
        spoken, finished = self.ask("что такое фотосинтез")
        self.assertEqual(spoken, "Ответ Hermes.")
        self.assertTrue(finished)
        self.assertEqual(len(self.utterances), 1)
        self.assertEqual(self.utterances[0]["profile"], "personal")
        self.assertEqual(len(self.hermes_calls), 1)

    def test_work_profile_bb_route_answers_without_hermes_and_announces_later(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 7, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "done",
                         "speech": "**Готово**: отчёт [тут](https://example.com/x)."}]
        spoken, finished = self.ask("сделай отчёт")
        self.assertEqual(spoken, "Передал в рабочий тред.")
        self.assertTrue(finished)
        self.assertEqual(self.hermes_calls, [])
        self.assertEqual(self.utterances[0]["profile"], "work")
        self.wait_until(lambda: len(self.announce_calls) >= 1, message="announce was not sent")
        self.assertEqual(len(self.result_calls), 1)
        self.assertEqual(self.result_calls[0]["threadId"], "thr_example")
        self.assertEqual(self.result_calls[0]["afterSeq"], 7)
        self.assertEqual(len(self.announce_calls), 1)
        path, payload = self.announce_calls[0]
        self.assertTrue(path.endswith("/api/services/script/turn_on"))
        self.assertEqual(payload["entity_id"], "script.schedule_announce")
        self.assertEqual(payload["variables"], {"message": "Готово: отчёт тут.", "chime": False})

    def test_late_question_starts_conversation_and_keeps_microphone_open(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 2, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "done", "speech": "Готово. Подтверждаете?"}]
        self.ask("проверь")
        self.wait_until(lambda: len(self.announce_calls) >= 1, message="announce was not sent")
        self.assertEqual(len(self.announce_calls), 1)
        path, payload = self.announce_calls[0]
        self.assertTrue(path.endswith("/api/services/assist_satellite/start_conversation"))
        self.assertEqual(payload, {
            "entity_id": "assist_satellite.iandeks_stantsiia_midi_assist_satellite",
            "start_message": "Готово. Подтверждаете?",
            "preannounce": False,
        })

    def test_late_statement_still_uses_the_announcement_script(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 2, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "done", "speech": "Готово."}]
        self.ask("проверь")
        self.wait_until(lambda: len(self.announce_calls) >= 1, message="announce was not sent")
        self.assertEqual(len(self.announce_calls), 1)
        path, payload = self.announce_calls[0]
        self.assertTrue(path.endswith("/api/services/script/turn_on"))
        self.assertEqual(payload["entity_id"], "script.schedule_announce")
        self.assertEqual(payload["variables"], {"message": "Готово.", "chime": False})

    def test_failed_start_conversation_falls_back_to_script_announce(self):
        self.start_conversation_fails = True
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 2, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "done", "speech": "Подтверждаете?"}]
        self.ask("проверь")
        self.wait_until(lambda: len(self.announce_calls) >= 2,
                        message="fallback announce was not sent")
        self.assertTrue(self.announce_calls[0][0].endswith(
            "/api/services/assist_satellite/start_conversation"))
        path, payload = self.announce_calls[1]
        self.assertTrue(path.endswith("/api/services/script/turn_on"))
        self.assertEqual(payload["variables"]["message"], "Подтверждаете?")

    def test_bb_route_streams_the_ack_and_never_calls_hermes(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 3, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "done", "speech": "Готово."}]
        spoken, finished = self.ask("проверь сборку", stream=True)
        self.assertEqual(spoken, "Передал в рабочий тред.")
        self.assertTrue(finished)
        self.assertEqual(self.hermes_calls, [])
        self.wait_until(lambda: len(self.announce_calls) >= 1, message="announce was not sent")
        self.assertEqual(self.announce_calls[0][1]["variables"]["message"], "Готово.")

    def test_local_route_answers_with_its_own_phrase(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "local", "speech": "Выключил режим агента."}
        spoken, finished = self.ask("выйди из режима агента")
        self.assertEqual(spoken, "Выключил режим агента.")
        self.assertTrue(finished)
        self.assertEqual(self.hermes_calls, [])
        self.assertEqual(self.announce_calls, [])

    def test_ack_never_ends_with_a_question(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 1, "speech": "Передал в рабочий тред?"}
        self.results = [{"status": "done", "speech": "Готово."}]
        spoken, _ = self.ask("проверь")
        self.assertTrue(spoken.endswith("."))
        self.assertNotIn("?", spoken)

    # -- fail-closed ------------------------------------------------------

    def test_gateway_error_for_work_stays_local(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": False,
                              "speech": "Рабочий тред не настроен."}
        spoken, _ = self.ask("проверь")
        self.assertEqual(spoken, "Рабочий тред не настроен.")
        self.assertEqual(self.hermes_calls, [])
        self.assertEqual(self.announce_calls, [])

    def test_unknown_route_is_not_forwarded_to_hermes(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "something-else", "speech": "Странно."}
        spoken, _ = self.ask("проверь")
        self.assertEqual(spoken, "Сервер агентов недоступен.")
        self.assertEqual(self.hermes_calls, [])

    def test_ha_down_fails_closed_for_every_profile(self):
        self.ha_down = True
        spoken, _ = self.ask("проверь")
        self.assertEqual(spoken, "Не могу подтвердить профиль.")
        self.assertEqual(self.utterances, [])
        self.assertEqual(self.hermes_calls, [])

    def test_unknown_profile_value_fails_closed(self):
        self.ha_state = "Непонятно"
        spoken, _ = self.ask("проверь")
        self.assertEqual(spoken, "Не могу подтвердить профиль.")
        self.assertEqual(self.hermes_calls, [])

    def test_fail_closed_phrase_is_streamed_too(self):
        self.ha_down = True
        spoken, finished = self.ask("проверь", stream=True)
        self.assertEqual(spoken, "Не могу подтвердить профиль.")
        self.assertTrue(finished)
        self.assertEqual(self.hermes_calls, [])

    def test_gateway_down_in_work_profile_never_uses_hermes(self):
        self.ha_state = "Рабочий"
        self.gateway_down = True
        spoken, _ = self.ask("проверь")
        self.assertEqual(spoken, "Сервер агентов недоступен.")
        self.assertEqual(self.hermes_calls, [])

    def test_gateway_down_in_personal_profile_keeps_hermes(self):
        self.ha_state = "Личный"
        self.gateway_down = True
        spoken, _ = self.ask("расскажи новости")
        self.assertEqual(spoken, "Ответ Hermes.")
        self.assertEqual(len(self.hermes_calls), 1)

    def test_routing_disabled_keeps_previous_behaviour(self):
        with patch.object(voice_proxy, "VOICE_GATEWAY_URL", ""):
            spoken, _ = self.ask("расскажи новости")
        self.assertEqual(spoken, "Ответ Hermes.")
        self.assertEqual(self.ha_calls, [])
        self.assertEqual(self.utterances, [])
        self.assertEqual(len(self.hermes_calls), 1)

    def test_non_user_last_message_is_not_routed(self):
        spoken, _ = self.ask("отмена", role="tool")
        self.assertEqual(spoken, "Ответ Hermes.")
        self.assertEqual(self.ha_calls, [])
        self.assertEqual(self.utterances, [])

    # -- ordering, cache and background work ------------------------------

    def test_cancel_guard_runs_before_profile_read_and_routing(self):
        spoken, finished = self.ask("Отмена!")
        self.assertEqual(spoken.strip(), "")
        self.assertTrue(finished)
        self.assertEqual(self.ha_calls, [])
        self.assertEqual(self.utterances, [])
        self.assertEqual(self.hermes_calls, [])

    def test_fresh_profile_is_cached_between_requests(self):
        self.ask("первый")
        self.ask("второй")
        self.assertEqual(len(self.ha_calls), 1)
        self.assertEqual(len(self.utterances), 2)

    def test_stale_cache_is_never_used_after_a_failed_read(self):
        with patch.object(voice_proxy, "PROFILE_CACHE_MS", 0):
            self.ask("первый")
            self.ha_down = True
            spoken, _ = self.ask("второй")
        self.assertEqual(spoken, "Не могу подтвердить профиль.")
        self.assertEqual(len(self.utterances), 1)

    def test_result_long_poll_waits_for_the_final_answer(self):
        self.ha_state = "Рабочий"
        self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                              "afterSeq": 4, "speech": "Передал в рабочий тред."}
        self.results = [{"status": "running"}, {"status": "waiting_input",
                                                 "speech": "Нужно уточнение."}]
        self.ask("проверь")
        self.wait_until(lambda: len(self.announce_calls) >= 1, message="announce was not sent")
        self.assertEqual(len(self.result_calls), 2)
        self.assertEqual(self.announce_calls[0][1]["variables"]["message"], "Нужно уточнение.")

    def test_result_timeout_does_not_speak_anything(self):
        with patch.object(voice_proxy, "VOICE_RESULT_TIMEOUT", 0.2), \
                patch.object(voice_proxy, "VOICE_RESULT_RETRY", 0.05):
            self.ha_state = "Рабочий"
            self.gateway_reply = {"route": "bb", "ok": True, "threadId": "thr_example",
                                  "afterSeq": 4, "speech": "Передал в рабочий тред."}
            self.ask("проверь")
            self.wait_until(lambda: not voice_proxy._background_threads, timeout=2.0,
                            message="announcer did not finish")
        self.assertEqual(self.announce_calls, [])

    def test_announce_text_sanitizes_and_shortens(self):
        self.assertLessEqual(len(voice_proxy.announce_text("слово " * 80)), 201)
        text = voice_proxy.announce_text("Смотри https://example.com/x и **готово**.")
        self.assertEqual(text, "Смотри и готово.")
        self.assertEqual(voice_proxy.announce_text(""), "")

    def test_long_question_truncation_keeps_the_question_mark(self):
        # Prefer the final question sentence when it fits.
        text = voice_proxy.announce_text("Первый факт. " + "слово " * 80 + ". Подтверждаете?")
        self.assertEqual(text, "Подтверждаете?")
        # If even the question is too long, cut it but keep the trailing "?".
        text = voice_proxy.announce_text("слово " * 80 + "Подтверждаете?")
        self.assertTrue(text.endswith("?"), text)
        self.assertLessEqual(len(text), 201)
        # A statement keeps the previous ellipsis truncation and no "?".
        text = voice_proxy.announce_text("слово " * 80)
        self.assertTrue(text.endswith("…"), text)
        self.assertFalse(text.endswith("?"))

    def test_route_counter_records_outcomes_without_text(self):
        self.gateway_reply = {"route": "hermes"}
        self.ask("первый")
        self.assertEqual(voice_proxy._route_counts["hermes"], 1)



class UpdatesPollerTests(unittest.TestCase):
    """Dispatcher updates: announce only work-profile non-owner turns."""

    def setUp(self):
        voice_proxy.stop_updates_poller(1.0)

    def tearDown(self):
        voice_proxy.stop_updates_poller(1.0)

    def wait_until(self, predicate, timeout=2.0, message="condition not met"):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.01)
        self.fail(message)

    def start(self, fetch, *, profile="work", profile_reader=None,
              announcer=None, sleeper=None, stop=None):
        stop = stop if stop is not None else threading.Event()
        reader = profile_reader if profile_reader is not None else (lambda: profile)
        voice_proxy.start_updates_poller(
            stop=stop, fetch=fetch, profile_reader=reader,
            announcer=announcer or voice_proxy.announce, sleeper=sleeper)
        self.addCleanup(stop.set)
        return stop

    def test_bootstrap_has_no_history_and_next_call_uses_the_cursor(self):
        calls = []

        def fetch(after_seq, wait_seconds):
            calls.append((after_seq, wait_seconds))
            return {"cursor": 5, "items": []}

        stop = self.start(fetch)
        self.wait_until(lambda: len(calls) >= 2, message="poller did not bootstrap")
        stop.set()
        self.assertIsNone(calls[0][0])
        self.assertEqual(calls[1][0], 5)
        self.assertEqual(calls[0][1], 1)

    def test_work_profile_announces_joined_spoken_text(self):
        calls = []
        announced = []

        def fetch(after_seq, wait_seconds):
            calls.append((after_seq, wait_seconds))
            if len(calls) == 1:
                return {"cursor": 5, "items": []}
            if len(calls) == 2:
                return {"cursor": 9, "items": [
                    {"seq": 8, "spoken": "Первый."},
                    {"seq": 9, "spoken": "Второй."},
                ]}
            return {"cursor": 9, "items": []}

        def announcer(text):
            announced.append(text)
            return True, False

        self.start(fetch, profile="work", announcer=announcer)
        self.wait_until(lambda: announced, message="batch was not announced")
        self.assertEqual(announced, ["Первый. Второй."])

    def test_personal_profile_drops_the_batch_without_speaking(self):
        calls = []
        reads = []
        announced = []

        def fetch(after_seq, wait_seconds):
            calls.append(after_seq)
            if len(calls) == 1:
                return {"cursor": 5, "items": []}
            if len(calls) == 2:
                return {"cursor": 6, "items": [{"seq": 6, "spoken": "Не озвучивать."}]}
            return {"cursor": 6, "items": []}

        def announcer(text):
            announced.append(text)
            return True, False

        self.start(fetch, profile_reader=lambda: reads.append("read") or "personal",
                   announcer=announcer)
        self.wait_until(lambda: reads, message="profile was not read for the batch")
        time.sleep(0.2)
        self.assertEqual(announced, [])

    def test_question_uses_start_conversation(self):
        calls = []

        def fetch(after_seq, wait_seconds):
            calls.append(after_seq)
            if len(calls) == 1:
                return {"cursor": 5, "items": []}
            if len(calls) == 2:
                return {"cursor": 6, "items": [
                    {"seq": 6, "spoken": "Готово. Подтверждаете?"}]}
            return {"cursor": 6, "items": []}

        with patch.object(voice_proxy, "_start_conversation",
                          return_value=True) as start_conversation, \
                patch.object(voice_proxy, "_announce_via_script",
                             return_value=True) as script:
            self.start(fetch, profile="work")
            self.wait_until(lambda: start_conversation.called,
                            message="question did not open the microphone")
        start_conversation.assert_called_once_with("Готово. Подтверждаете?")
        script.assert_not_called()

    def test_errors_back_off_exponentially(self):
        stop = threading.Event()
        delays = []

        def sleeper(seconds):
            delays.append(seconds)
            if len(delays) >= 4:
                stop.set()

        def fetch(after_seq, wait_seconds):
            raise urllib.error.URLError("gateway down")

        thread = voice_proxy.start_updates_poller(
            stop=stop, fetch=fetch, sleeper=sleeper)
        thread.join(2.0)
        self.assertFalse(thread.is_alive())
        self.assertEqual(delays, [2.0, 4.0, 8.0, 16.0])

    def test_updates_switch_is_default_on_and_zero_disables(self):
        with patch.object(voice_proxy, "VOICE_UPDATES_DISABLED", False), \
                patch.object(voice_proxy, "routing_enabled", return_value=True):
            self.assertTrue(voice_proxy.updates_enabled())
        with patch.object(voice_proxy, "VOICE_UPDATES_DISABLED", True), \
                patch.object(voice_proxy, "routing_enabled", return_value=True):
            self.assertFalse(voice_proxy.updates_enabled())
        with patch.object(voice_proxy, "VOICE_UPDATES_DISABLED", False), \
                patch.object(voice_proxy, "routing_enabled", return_value=False):
            self.assertFalse(voice_proxy.updates_enabled())


class AnnounceQueueTests(unittest.TestCase):
    """Producer side of the durable announcement queue."""

    def setUp(self):
        case = self
        self.requests = []

        class Scheduler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                length = int(self.headers.get("Content-Length", "0"))
                body = json.loads(self.rfile.read(length) or b"{}")
                case.requests.append((self.path, body))
                payload = json.dumps({"event": {"id": "ev_test"}}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Scheduler)
        threading.Thread(target=self.server.serve_forever,
                         kwargs={"poll_interval": 0.05}, daemon=True).start()
        self.overrides = [
            patch.object(voice_proxy, "SCHEDULER_URL",
                         f"http://127.0.0.1:{self.server.server_port}"),
            patch.object(voice_proxy, "SCHEDULER_TOKEN", "test-scheduler-token"),
            patch.object(voice_proxy, "SCHEDULER_TOKEN_FILE", ""),
            patch.object(voice_proxy, "VOICE_QUEUE_DISABLED", False),
        ]
        for override in self.overrides:
            override.start()

    def tearDown(self):
        for override in self.overrides:
            override.stop()
        self.server.shutdown()
        self.server.server_close()

    def test_enqueue_posts_a_durable_task_with_the_idempotency_key(self):
        self.assertTrue(voice_proxy.enqueue_announce(
            "Готово.", "work", "thr_example", "after:7", 1_700_000_000))
        self.assertEqual(len(self.requests), 1)
        path, body = self.requests[0]
        self.assertTrue(path.endswith("/ricardo.v1.SchedulerService/EnqueueTask"))
        self.assertEqual(body["taskType"], "voice.announce")
        self.assertEqual(body["queue"], "voice-work")
        self.assertEqual(body["idempotencyKey"], "voice:thr_example:after:7")
        payload = json.loads(body["payloadJson"])
        self.assertEqual(payload, {
            "text": "Готово.", "profile": "work", "thread_id": "thr_example",
            "seq": "after:7", "created_unix": 1_700_000_000,
        })

    def test_personal_profile_uses_its_own_queue(self):
        voice_proxy.enqueue_announce("Ответ.", "personal", "thr_p", "done:12")
        self.assertEqual(self.requests[0][1]["queue"], "voice-personal")
        self.assertEqual(self.requests[0][1]["idempotencyKey"],
                         "voice:thr_p:done:12")

    def test_scheduler_error_returns_false(self):
        with patch.object(voice_proxy, "_scheduler_rpc",
                          side_effect=urllib.error.URLError("down")):
            self.assertFalse(voice_proxy.enqueue_announce(
                "Готово.", "work", "thr_example", "after:1"))

    def test_result_announcer_enqueues_instead_of_speaking(self):
        enqueued, announced = [], []
        with patch.object(voice_proxy, "call_result", return_value={
                "status": "done", "speech": "**Готово**.", "completedSeq": 42,
            }), patch.object(voice_proxy, "enqueue_announce",
                             side_effect=lambda *a: enqueued.append(a) or True), \
                patch.object(voice_proxy, "announce",
                             side_effect=lambda text: announced.append(text) or (True, False)):
            voice_proxy._result_announcer("thr_example", 7, "personal")
        self.assertEqual(announced, [])
        self.assertEqual(enqueued, [("Готово.", "personal", "thr_example", "done:42")])

    def test_result_announcer_falls_back_when_enqueue_fails(self):
        announced = []
        with patch.object(voice_proxy, "call_result", return_value={
                "status": "done", "speech": "Готово."}), \
                patch.object(voice_proxy, "enqueue_announce", return_value=False), \
                patch.object(voice_proxy, "announce",
                             side_effect=lambda text: announced.append(text) or (True, False)):
            voice_proxy._result_announcer("thr_example", 7, "work")
        self.assertEqual(announced, ["Готово."])

    def test_queue_disabled_keeps_the_direct_announce(self):
        announced = []
        with patch.object(voice_proxy, "queue_enabled", return_value=False), \
                patch.object(voice_proxy, "call_result", return_value={
                    "status": "done", "speech": "Готово."}), \
                patch.object(voice_proxy, "announce",
                             side_effect=lambda text: announced.append(text) or (True, False)):
            voice_proxy._result_announcer("thr_example", 7, "work")
        self.assertEqual(announced, ["Готово."])

    def test_queue_switch_needs_url_and_token(self):
        with patch.object(voice_proxy, "VOICE_QUEUE_DISABLED", False):
            self.assertTrue(voice_proxy.queue_enabled())
        with patch.object(voice_proxy, "SCHEDULER_URL", ""):
            self.assertFalse(voice_proxy.queue_enabled())
        with patch.object(voice_proxy, "SCHEDULER_TOKEN", ""):
            self.assertFalse(voice_proxy.queue_enabled())
        with patch.object(voice_proxy, "VOICE_QUEUE_DISABLED", True):
            self.assertFalse(voice_proxy.queue_enabled())


class UpdatesQueueTests(unittest.TestCase):
    """The updates poller enqueues for the work profile and rescans on restart."""

    def setUp(self):
        voice_proxy.stop_updates_poller(1.0)

    def tearDown(self):
        voice_proxy.stop_updates_poller(1.0)

    def run_poller(self, fetch, *, enqueue, profile="personal", limit=4):
        stop = threading.Event()
        sleeps = []

        def sleeper(seconds):
            sleeps.append(seconds)
            if len(sleeps) >= limit:
                stop.set()

        voice_proxy.start_updates_poller(
            stop=stop, fetch=fetch, profile_reader=lambda: profile,
            announcer=lambda text: (True, False), sleeper=sleeper,
            enqueue=enqueue)
        thread = voice_proxy._updates_thread
        if thread is not None:
            thread.join(2.0)

    def test_bootstrap_rescans_a_recent_window(self):
        calls = []

        def fetch(after_seq, wait_seconds):
            calls.append(after_seq)
            return {"cursor": 500, "items": []}

        with patch.object(voice_proxy, "queue_enabled", return_value=True), \
                patch.object(voice_proxy, "VOICE_UPDATES_RESCAN", 300):
            self.run_poller(fetch, enqueue=lambda *a: True, limit=3)
        self.assertIsNone(calls[0])
        self.assertEqual(calls[1], 200)

    def test_bootstrap_without_queue_starts_at_the_cursor(self):
        calls = []

        def fetch(after_seq, wait_seconds):
            calls.append(after_seq)
            return {"cursor": 500, "items": []}

        with patch.object(voice_proxy, "queue_enabled", return_value=False):
            self.run_poller(fetch, enqueue=lambda *a: True, limit=3)
        self.assertEqual(calls[1], 500)

    def test_personal_profile_does_not_drop_the_work_answer(self):
        events = [{"seq": 9, "spoken": "Готово.", "completedAt": 1_700_000_000_000}]
        calls = []

        def fetch(after_seq, wait_seconds):
            calls.append(after_seq)
            if len(calls) == 1:
                return {"cursor": 10, "items": []}
            return {"cursor": 10, "items": events}

        enqueued = []
        with patch.object(voice_proxy, "queue_enabled", return_value=True), \
                patch.object(voice_proxy, "time") as fake_time:
            fake_time.time.return_value = 1_700_000_010
            self.run_poller(fetch, enqueue=lambda *a: enqueued.append(a) or True,
                            profile="personal", limit=3)
        self.assertTrue(enqueued)
        self.assertEqual(enqueued[0],
                         ("Готово.", "work", "dispatcher", "9", 1_700_000_000))

    def test_stale_update_is_not_enqueued(self):
        enqueued = []
        with patch.object(voice_proxy, "queue_enabled", return_value=True):
            queued, stale = voice_proxy._enqueue_updates(
                [{"seq": 1, "spoken": "Старое.", "completedAt": 1_000_000}],
                lambda *a: enqueued.append(a) or True, 1_700_000_000)
        self.assertEqual((queued, stale), (0, 1))
        self.assertEqual(enqueued, [])


class QueueConsumerTests(unittest.TestCase):
    """Consumer side: lease only when idle, ack only after the answer finished."""

    def drive(self, *, profile="work", state_reader=None, events=None,
              announce_result=(True, False), limit=6):
        calls = {"calls": [], "leases": [], "acks": [], "announced": [],
                 "states": 0}
        clock = [1_700_000_000.0]
        stop = threading.Event()
        sleeps = [0]
        queue = list(events or [])

        def sleeper(seconds):
            sleeps[0] += 1
            clock[0] += seconds
            if sleeps[0] >= limit:
                stop.set()

        def lister(queue_name, consumer, lease, count):
            calls["leases"].append((queue_name, consumer, lease, count))
            return [queue.pop(0)] if queue else []

        def acker(event_id, consumer, outcome="ACK_OUTCOME_DELIVERED", detail=""):
            calls["acks"].append((event_id, outcome, detail))

        def announcer(text):
            calls["announced"].append(text)
            return announce_result

        def reader(entity_id):
            calls["states"] += 1
            if state_reader is None:
                return "idle" if entity_id == voice_proxy.VOICE_SATELLITE else "playing"
            return state_reader(entity_id, calls["states"])

        voice_proxy._queue_consumer(
            stop, profile_reader=lambda: profile, state_reader=reader,
            lister=lister, acker=acker, announcer=announcer,
            clock=lambda: clock[0], sleeper=sleeper)
        return calls

    def event(self, *, created=None, text="Готово."):
        payload = {"text": text, "profile": "work",
                   "thread_id": "thr_example", "seq": "1"}
        if created is not None:
            payload["created_unix"] = created
        return {"id": "ev_1", "payloadJson": json.dumps(payload),
                "createdUnix": str(created or 0)}

    def test_busy_or_unknown_station_never_leases(self):
        for states in [("listening", "playing"), ("processing", "playing"),
                       (None, "playing"), ("idle", None),
                       ("idle", "announcing")]:
            with self.subTest(states=states):
                calls = self.drive(
                    state_reader=lambda entity, _n, s=states: s[0]
                    if entity == voice_proxy.VOICE_SATELLITE else s[1],
                    events=[self.event(created=1_700_000_000)], limit=2)
                self.assertEqual(calls["leases"], [])
                self.assertEqual(calls["acks"], [])

    def test_leases_only_the_current_profile_queue_for_this_consumer(self):
        calls = self.drive(profile="personal")
        self.assertTrue(calls["leases"])
        self.assertEqual(calls["leases"][0][0], "voice-personal")
        self.assertEqual(calls["leases"][0][1], "voice-proxy-home")
        self.assertEqual(calls["leases"][0][2], 120)

    def test_stale_answer_is_acked_without_speaking(self):
        calls = self.drive(events=[self.event(created=1_699_000_000)], limit=3)
        self.assertEqual(calls["announced"], [])
        self.assertEqual(calls["acks"], [("ev_1", "ACK_OUTCOME_SKIPPED", "stale")])

    def test_answer_waited_over_two_minutes_is_prefixed_and_acked(self):
        with patch.object(voice_proxy, "VOICE_QUEUE_BUSY_GRACE", 0.0):
            calls = self.drive(events=[self.event(created=1_699_999_870)], limit=3)
        self.assertEqual(calls["announced"], ["Пока вас не было: Готово."])
        self.assertEqual(calls["acks"][0][0], "ev_1")
        self.assertEqual(calls["acks"][0][1], "ACK_OUTCOME_DELIVERED")

    def test_ack_waits_for_the_station_to_finish(self):
        sequence = ["busy", "busy", "idle"]

        def state_reader(entity, _n):
            value = sequence.pop(0) if sequence else "idle"
            if entity == voice_proxy.VOICE_SATELLITE:
                return "listening" if value == "busy" else "idle"
            return "playing"

        calls = self.drive(state_reader=state_reader,
                           events=[self.event(created=1_700_000_000)], limit=8)
        self.assertEqual(calls["announced"], ["Готово."])
        self.assertEqual(len(calls["acks"]), 1)
        # The first two pre-lease reads plus the busy waiter read happened
        # before the ack; the ack is not issued while the station is busy.
        self.assertGreaterEqual(calls["states"], 8)

    def test_announce_failure_is_not_acked(self):
        calls = self.drive(announce_result=(False, False),
                           events=[self.event(created=1_700_000_000)], limit=3)
        self.assertEqual(calls["announced"], ["Готово."])
        self.assertEqual(calls["acks"], [])

    def test_station_idle_rules(self):
        self.assertTrue(voice_proxy.station_idle("idle", "playing"))
        self.assertTrue(voice_proxy.station_idle("idle", "idle"))
        self.assertFalse(voice_proxy.station_idle("idle", "announcing"))
        self.assertFalse(voice_proxy.station_idle(None, "playing"))
        self.assertFalse(voice_proxy.station_idle("idle", None))
        self.assertFalse(voice_proxy.station_idle("unknown", "playing"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
