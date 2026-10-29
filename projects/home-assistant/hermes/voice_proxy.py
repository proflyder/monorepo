#!/usr/bin/env python3
"""Small loopback proxy that makes Hermes replies safe for Piper TTS.

Hermes' OpenAI-compatible API is the upstream.  The Home Assistant
``llama_cpp`` conversation agent points at this proxy instead of the Hermes
API directly.  The proxy forwards requests and sanitizes assistant text:

* removes Markdown links and bare URLs (TTS must not read them);
* removes parenthetical source citations such as
  ``([example.com](https://...))``;
* strips Markdown emphasis/headings/code/list markers;
* collapses whitespace.

It is not an authentication boundary. The default bind is loopback; the K3s
deployment binds the K3s node address so HAOS can reach it directly and uses
an nftables rule to allow only HAOS and the node itself.
"""
from __future__ import annotations

import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from unicodedata import normalize as unicode_normalize

UPSTREAM = os.environ.get("HERMES_UPSTREAM", "http://127.0.0.1:8642").rstrip("/")
# Loopback by default. The K3s deployment sets this to the LAN address so HAOS
# can reach the proxy without the retired HAProxy compatibility layer.
BIND = os.environ.get("HERMES_PROXY_BIND", "127.0.0.1")
PORT = int(os.environ.get("HERMES_PROXY_PORT", "8643"))
MAX_BODY = 10 * 1024 * 1024
VOICE_REASONING_EFFORT = os.environ.get("HERMES_VOICE_REASONING_EFFORT", "low")

# Order matters: remove whole citation parentheses before unwrapping links.
_CITATION = re.compile(r"\s*\(\[[^\]]+\]\(https?://[^)]+\)\)")
_MD_LINK = re.compile(r"\[([^\]]+)\]\(https?://[^)]+\)")
_URL = re.compile(r"https?://[^\s)\]}>\"']+")
_MD_LINE_PREFIX = re.compile(r"(?m)^\s*(?:#{1,6}\s*|[-*+]\s+|\d+[.)]\s+)")
_MD_MARKS = re.compile(r"[*_`~]+")
_EMPTY_PARENS = re.compile(r"\(\s*\)")
_SPACE = re.compile(r"\s+")
# STT may add a sentence-final mark; nothing else is stripped.  Quotes,
# hyphens and internal commas must survive so quoted or reworded phrases are
# not treated as an exact cancel command.
_TRAILING_PUNCT = ".!?…"

# Exact local cancel phrases already registered by the HA
# custom_sentences/ru/station_control.yaml StationCancel intent.  This guard is
# a last-resort path for HA's continued-conversation mode, where the pipeline
# sends the next utterance straight to the conversation agent and skips
# prefer_local_intents.  Do not broaden this into fuzzy matching.
CANCEL_PHRASES = frozenset(
    {
        "отмена",
        "отмени",
        "передумал",
        "передумала",
        "не надо",
        "ничего",
        "отбой",
        "забудь",
        "неважно",
    }
)

# HA's llama_cpp adapter drops an assistant message whose accumulated content
# is empty, then raises "Unable to get response".  A whitespace-only body keeps
# the message valid while the pipeline's tts_input.strip() check still skips
# TTS.  It is never spoken and never reaches Hermes.
CANCEL_SILENCE = " "

_cancel_total = 0


def normalize_utterance(text: str) -> str:
    """Return a compact lowercase form used only for exact cancel matching.

    Only NFKC, case folding, whitespace collapsing and sentence-final STT
    punctuation are removed.  Quotes, hyphens and internal commas stay, so
    «отмена», "отмена", от-мена and не, надо do not match.
    """
    text = unicode_normalize("NFKC", text).casefold().strip()
    return " ".join(text.rstrip(_TRAILING_PUNCT).split())


def cancel_short_circuit(payload: dict[str, Any]) -> str | None:
    """Return the matched cancel phrase for the last user message, or None.

    Only the final message is considered.  Tool results, assistant text,
    quoted history and messages with extra meaning are forwarded upstream.
    """
    messages = payload.get("messages")
    if not isinstance(messages, list) or not messages:
        return None
    last = messages[-1]
    if not isinstance(last, dict) or last.get("role") != "user":
        return None
    content = last.get("content")
    if not isinstance(content, str):
        return None
    normalized = normalize_utterance(content)
    return normalized if normalized in CANCEL_PHRASES else None


def sanitize_text(text: str) -> str:
    """Return a TTS-friendly version of an assistant reply."""
    if not text:
        return ""
    text = _CITATION.sub("", text)
    text = _MD_LINK.sub(r"\1", text)
    text = _URL.sub("", text)
    text = _MD_LINE_PREFIX.sub("", text)
    text = _MD_MARKS.sub("", text)
    text = _EMPTY_PARENS.sub("", text)
    text = _SPACE.sub(" ", text).strip()
    return text


def sanitize_chat_payload(payload: dict[str, Any]) -> dict[str, Any]:
    """Sanitize assistant content in an OpenAI Chat Completions payload."""
    for choice in payload.get("choices", []) or []:
        message = choice.get("message")
        if isinstance(message, dict) and isinstance(message.get("content"), str):
            message["content"] = sanitize_text(message["content"])
    return payload


class StreamingSanitizer:
    """Release complete sentences, keeping split links/URLs out of speech.

    A boundary must be outside links/parentheses and followed by whitespace.
    The tail is held until completion; regexes must never see half a URL.
    """

    def __init__(self) -> None:
        self.pending = ""
        self.emitted = False

    def feed(self, text: str, *, final: bool = False) -> str:
        self.pending += text
        if len(self.pending.encode("utf-8")) > MAX_BODY:
            raise ValueError("stream text too large")
        end = len(self.pending) if final else 0
        if not final:
            brackets = parens = 0
            for match in re.finditer(r"https?://[^\s)\]]*|[\[\]()]|[.!?…]+(?=\s)", self.pending):
                token = match.group()
                if token.startswith(("http://", "https://")):
                    continue
                if token == "[":
                    brackets += 1
                elif token == "]":
                    brackets = max(0, brackets - 1)
                elif token == "(":
                    parens += 1
                elif token == ")":
                    parens = max(0, parens - 1)
                elif not brackets and not parens:
                    end = match.end()
        if not end:
            return ""
        raw, self.pending = self.pending[:end], self.pending[end:]
        # A truncated/incomplete link is never safe to speak at end of stream.
        if final:
            raw = re.sub(r"\[[^\]]*$|\[[^\]]+\]\([^)]*$", "", raw)
            raw = re.sub(r"https?(?::/{0,2})?$", "", raw)
        cleaned = sanitize_text(raw)
        if not cleaned:
            return ""
        result = (" " if self.emitted else "") + cleaned
        self.emitted = True
        return result


def prepare_chat_request(payload: dict[str, Any]) -> dict[str, Any]:
    """Hermes accepts reasoning through model_options, not max_tokens/HA options."""
    payload = dict(payload)
    payload["model_options"] = {
        **payload.get("model_options", {}),
        "reasoning_effort": VOICE_REASONING_EFFORT,
    }
    return payload


def sse_events(response):
    """Read SSE records incrementally, including multiline data and CRLF."""
    event, data = "message", []
    for raw in response:
        line = raw.decode("utf-8").rstrip("\r\n")
        if not line:
            if data:
                yield event, "\n".join(data)
            event, data = "message", []
        elif line.startswith("event:"):
            event = line[6:].strip()
        elif line.startswith("data:"):
            data.append(line[5:].removeprefix(" "))
        elif line.startswith(":"):
            yield "keepalive", ""
    if data:
        yield event, "\n".join(data)


def error_body(message: str) -> bytes:
    """HA's OpenAI client requires error to be an object, including at startup."""
    return json.dumps({"error": {"message": message, "type": "server_error",
                                 "code": "upstream_error"}}).encode()


def normalize_error(status: int, body: bytes, content_type: str) -> tuple[bytes, str]:
    if status < 400:
        return body, content_type
    try:
        payload = json.loads(body)
        error = payload.get("error") if isinstance(payload, dict) else None
        if isinstance(error, dict):
            return body, "application/json"
        if isinstance(error, str):
            return error_body(error), "application/json"
    except (ValueError, UnicodeError):
        pass
    return error_body(f"Hermes upstream returned HTTP {status}"), "application/json"


class Handler(BaseHTTPRequestHandler):
    server_version = "HermesVoiceProxy/2.0"

    def log_message(self, fmt: str, *args: Any) -> None:  # noqa: A003
        sys.stderr.write("voice-proxy: " + (fmt % args) + "\n")

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", "0") or "0")
        if length < 0 or length > MAX_BODY:
            raise ValueError("request body too large")
        return self.rfile.read(length)

    def _upstream(self, method: str, path: str, body: bytes | None) -> tuple[int, bytes, str]:
        headers = {
            "Authorization": self.headers.get("Authorization", ""),
            "Content-Type": self.headers.get("Content-Type", "application/json"),
            "Accept": self.headers.get("Accept", "application/json"),
        }
        req = urllib.request.Request(UPSTREAM + path, data=body, method=method, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=180) as resp:
                return resp.status, resp.read(), resp.headers.get("Content-Type", "application/json")
        except urllib.error.HTTPError as exc:
            return exc.code, exc.read(), exc.headers.get("Content-Type", "application/json")

    def _send(self, status: int, body: bytes, content_type: str) -> None:
        body, content_type = normalize_error(status, body, content_type)
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        try:
            status, body, ctype = self._upstream("GET", self.path, None)
            self._send(status, body, ctype)
        except Exception:
            self._send(503, error_body("Hermes upstream is unavailable"), "application/json")

    def _cancel_chunk(self, payload: dict[str, Any], *, delta: dict[str, Any],
                      finish_reason: str | None = None) -> dict[str, Any]:
        return {
            "id": "chatcmpl-ricardo-cancel",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": payload.get("model") or "hermes-agent",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}],
        }

    def _send_cancel_completion(self, payload: dict[str, Any]) -> None:
        """Answer a local cancel without any upstream Hermes model turn."""
        global _cancel_total
        _cancel_total += 1
        # Structural evidence only: never log the recognized text or payload.
        self.log_message(
            "cancel guarded count=%d stream=%s upstream=skipped",
            _cancel_total,
            bool(payload.get("stream")),
        )
        if payload.get("stream") is True:
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream; charset=utf-8")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "close")
            self.end_headers()
            self._stream_started = self.close_connection = True
            self._write_event(self._cancel_chunk(
                payload, delta={"role": "assistant", "content": CANCEL_SILENCE}))
            self._write_event(self._cancel_chunk(payload, delta={}, finish_reason="stop"))
            self._write_event("[DONE]")
            return
        body = {
            "id": "chatcmpl-ricardo-cancel",
            "object": "chat.completion",
            "created": int(time.time()),
            "model": payload.get("model") or "hermes-agent",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": CANCEL_SILENCE},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        }
        self._send(200, json.dumps(body).encode(), "application/json")

    def do_POST(self) -> None:  # noqa: N802
        self._stream_started = False
        try:
            raw = self._read_body()
            payload = json.loads(raw or b"{}")
            if self.path.startswith("/v1/chat/completions"):
                if cancel_short_circuit(payload) is not None:
                    self._send_cancel_completion(payload)
                    return
                payload = prepare_chat_request(payload)
                if payload.get("stream") is True:
                    self._stream_chat(payload)
                    return
            body = json.dumps(payload).encode()
            status, resp_body, ctype = self._upstream("POST", self.path, body)
            if self.path.startswith("/v1/chat/completions") and status == 200:
                try:
                    response = json.loads(resp_body)
                    if isinstance(response, dict):
                        response = sanitize_chat_payload(response)
                        resp_body = json.dumps(response, ensure_ascii=False).encode()
                except (ValueError, TypeError):
                    pass
            self._send(status, resp_body, ctype)
        except (BrokenPipeError, ConnectionResetError):
            # Closing upstream also interrupts Hermes' streaming agent turn.
            self.close_connection = True
        except Exception as exc:
            if self._stream_started:
                # Never append a second HTTP response or flush an untrusted tail.
                self.close_connection = True
                self.log_message("stream failed: %s", type(exc).__name__)
                try:
                    self._write_event({"error": {"message": "Voice upstream stream failed",
                                                 "type": "server_error"}})
                except OSError:
                    pass
            else:
                self._send(503, error_body("Hermes upstream is unavailable"), "application/json")

    def _write_event(self, payload: dict[str, Any] | str) -> None:
        data = payload if isinstance(payload, str) else json.dumps(payload, ensure_ascii=False)
        self.wfile.write(("data: " + data + "\n\n").encode("utf-8"))
        self.wfile.flush()

    def _stream_chat(self, payload: dict[str, Any]) -> None:
        req = urllib.request.Request(
            UPSTREAM + self.path, data=json.dumps(payload).encode(), method="POST",
            headers={"Authorization": self.headers.get("Authorization", ""),
                     "Content-Type": "application/json", "Accept": "text/event-stream"},
        )
        started = time.monotonic()
        try:
            response = urllib.request.urlopen(req, timeout=180)
        except urllib.error.HTTPError as exc:
            self._send(exc.code, exc.read(), "application/json")
            return
        with response:
            if not response.headers.get("Content-Type", "").startswith("text/event-stream"):
                raise ValueError("upstream did not return SSE")
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream; charset=utf-8")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Connection", "close")
            self.end_headers()
            self._stream_started = self.close_connection = True
            sanitizers: dict[int, StreamingSanitizer] = {}
            finished: set[int] = set()
            first_content = None
            for event, data in sse_events(response):
                if event == "keepalive":
                    # Preserve liveness and notice a disconnected HA client while thinking.
                    self.wfile.write(b": keepalive\n\n")
                    self.wfile.flush()
                    continue
                if data == "[DONE]":
                    if not sanitizers or set(sanitizers) != finished:
                        raise ValueError("upstream ended without finish_reason")
                    self._write_event("[DONE]")
                    self.log_message("stream complete first_text=%.3fs total=%.3fs reasoning=%s",
                                     first_content or 0, time.monotonic() - started, VOICE_REASONING_EFFORT)
                    return
                if event not in ("message", ""):
                    # Hermes tool progress, approvals and status are not speech.
                    continue
                chunk = json.loads(data)
                if chunk.get("object") != "chat.completion.chunk":
                    if "error" in chunk:
                        raise ValueError("upstream stream error")
                    continue
                choices, finish_choices = [], []
                for choice in chunk.get("choices", []):
                    index = choice.get("index", 0)
                    if index in finished:
                        continue
                    sanitizer = sanitizers.setdefault(index, StreamingSanitizer())
                    delta = choice.get("delta") or {}
                    reason = choice.get("finish_reason")
                    if reason not in (None, "stop", "length"):
                        raise ValueError("unsuccessful upstream finish")
                    text = sanitizer.feed(delta.get("content") or "", final=reason is not None)
                    if text and reason is None:
                        # Piper's sentence_stream waits for the next capitalized word.
                        # A paragraph boundary flushes this complete sentence immediately.
                        text += "\n\n"
                    safe_delta = {"content": text} if text else {}
                    if delta.get("role") == "assistant":
                        safe_delta["role"] = "assistant"
                    if text and first_content is None:
                        first_content = time.monotonic() - started
                    if reason is not None:
                        finished.add(index)
                        finish_choices.append({"index": index, "delta": {}, "finish_reason": reason})
                    if safe_delta:
                        choices.append({"index": index, "delta": safe_delta, "finish_reason": None})
                if choices:
                    # Whitelist wire fields; never forward reasoning or tool-call deltas.
                    safe_chunk = {k: chunk[k] for k in ("id", "object", "created", "model") if k in chunk}
                    safe_chunk["choices"] = choices
                    self._write_event(safe_chunk)
                if finish_choices:
                    # HA stops reading as soon as finish_reason is set. Flush the tail first.
                    safe_chunk = {k: chunk[k] for k in ("id", "object", "created", "model") if k in chunk}
                    safe_chunk["choices"] = finish_choices
                    self._write_event(safe_chunk)
            raise ValueError("upstream disconnected before DONE")


def main() -> int:
    server = ThreadingHTTPServer((BIND, PORT), Handler)
    print(f"hermes voice proxy on http://{BIND}:{PORT} -> {UPSTREAM}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
