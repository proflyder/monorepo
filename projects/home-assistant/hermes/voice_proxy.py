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

Only loopback should expose this service.  It is not an authentication
boundary; keep it on 127.0.0.1 and pass the upstream bearer key through.
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

UPSTREAM = os.environ.get("HERMES_UPSTREAM", "http://127.0.0.1:8642").rstrip("/")
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
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        try:
            status, body, ctype = self._upstream("GET", self.path, None)
            self._send(status, body, ctype)
        except Exception as exc:  # pragma: no cover - network path
            self._send(502, json.dumps({"error": str(exc)}).encode(), "application/json")

    def do_POST(self) -> None:  # noqa: N802
        self._stream_started = False
        try:
            raw = self._read_body()
            payload = json.loads(raw or b"{}")
            if self.path.startswith("/v1/chat/completions"):
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
                self._send(502, json.dumps({"error": type(exc).__name__}).encode(), "application/json")

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
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"hermes voice proxy on http://127.0.0.1:{PORT} -> {UPSTREAM}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
