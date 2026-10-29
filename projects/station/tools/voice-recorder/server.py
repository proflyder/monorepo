#!/usr/bin/env python3
"""Local-only recording console. No HA calls, settings edits or replacement daemon."""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import signal
import struct
import subprocess
import threading
import time
import traceback
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = "/tmp/ricardo-voice-recorder"
MAGIC = b"RICARDO_PCM_S16LE_16000\n"
BLOCKS = {"normal", "relaxed", "quiet", "whisper", "commands", "background"}


class SSHStream:
    """Popen-shaped binary stream using the existing password auth, no PTY."""
    def __init__(self, client, command, log_path):
        self.client = client
        stdin, stdout, stderr = client.exec_command(command, get_pty=False, timeout=15)
        self.stdout = stdout.channel.makefile("rb")
        try:
            stdin.close()
        except OSError:
            pass
        def drain():
            try:
                with log_path.open("wb") as log:
                    while data := stderr.read(1024):
                        log.write(data)
            except Exception:
                pass
        threading.Thread(target=drain, daemon=True).start()

    def wait(self, timeout):
        end = time.monotonic() + timeout
        while not self.stdout.channel.exit_status_ready():
            if time.monotonic() >= end:
                self.client.close()
                raise TimeoutError("SSH stream did not finish")
            time.sleep(.1)
        code = self.stdout.channel.recv_exit_status()
        self.client.close()
        return code


class Recorder:
    def __init__(self, station, output, helper, adb="adb", ssh_target=None, ssh_key=None, ssh_port=22, credentials=None):
        self.station, self.output, self.helper, self.adb = station, output, helper, adb
        self.output.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.lock = threading.RLock()
        self.state = "idle"
        self.message = "Обычный режим колонки. Запись выключена."
        self.session = None
        self.proc = None
        self.take = None
        self.last_client = time.monotonic()
        self.began = 0
        self.level = -100
        self.samples = 0
        self.restored = True
        self.worker = None
        self.ssh_target, self.ssh_key, self.ssh_port = ssh_target, ssh_key, ssh_port
        self.last_error = None
        self.credentials = credentials
        self.helper_hash = None

    def ssh_client(self):
        import paramiko
        if not self.ssh_target or not self.credentials:
            raise RuntimeError("Не настроен SSH-доступ.")
        text = Path(self.credentials).read_text()
        match = re.search(r"^\|\s*SSH-пароль\s*\|\s*`([^`]+)`\s*\|", text, re.M)
        if not match:
            raise RuntimeError("Не найдено поле SSH-пароль в приватном CREDENTIALS.md.")
        user, host = self.ssh_target.rsplit("@", 1)
        client = paramiko.SSHClient()
        client.load_system_host_keys()
        client.set_missing_host_key_policy(paramiko.RejectPolicy())
        client.connect(host, port=self.ssh_port, username=user, password=match[1],
            look_for_keys=False, allow_agent=False, timeout=8, banner_timeout=8, auth_timeout=8)
        client.get_transport().set_keepalive(3)
        return client

    def ssh_command(self, command):
        if not self.ssh_target or not self.ssh_key:
            raise RuntimeError("Укажите SSH-подключение и отдельный ключ колонки.")
        return ["ssh", "-T", "-i", str(self.ssh_key), "-p", str(self.ssh_port),
            "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
            "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=3", "-o", "ServerAliveCountMax=3",
            self.ssh_target, command]

    def run(self, *args, timeout=20):
        return subprocess.run([self.adb, "-s", self.station, *args],
            capture_output=True, check=True, timeout=timeout)

    def shell(self, command, timeout=20):
        if self.credentials:
            client = self.ssh_client()
            try:
                _, stdout, stderr = client.exec_command(command, timeout=timeout, get_pty=False)
                data = stdout.read()
                if stdout.channel.recv_exit_status() != 0:
                    raise RuntimeError("Команда на колонке не выполнена; проверьте состояние сеанса.")
                return data.decode(errors="replace").strip()
            finally:
                client.close()
        if self.ssh_target:
            return subprocess.run(self.ssh_command(command), capture_output=True, check=True,
                timeout=timeout).stdout.decode(errors="replace").strip()
        return self.run("shell", command, timeout=timeout).stdout.decode(errors="replace").strip()

    def install(self):
        if not self.helper.is_file():
            raise RuntimeError("Сначала соберите ricardo-voicestream по README.")
        self.helper_hash = hashlib.sha256(self.helper.read_bytes()).hexdigest()
        self.shell(f"test ! -d /tmp/ricardo-voice-recorder.lock && mkdir -p {BASE} && chmod 700 {BASE}")
        self.run("push", str(self.helper), BASE + "/ricardo-voicestream")
        self.run("push", str(HERE / "session.sh"), BASE + "/session.sh")
        self.run("push", str(HERE / "recover.sh"), BASE + "/recover.sh")
        self.shell(f"chmod 700 {BASE}/ricardo-voicestream {BASE}/session.sh {BASE}/recover.sh")

    def start(self):
        with self.lock:
            if self.state not in {"idle", "error"} or not self.restored:
                raise ValueError("Сначала завершите текущий сеанс и восстановите колонку.")
            self.state, self.message = "preparing", "Подготовка: временно освобождаем микрофон…"
            self.session = secrets.token_hex(16)
            self.last_client, self.began = time.monotonic(), time.monotonic()
            self.restored = False
            self.samples = 0
            self.worker = threading.Thread(target=self._capture, daemon=True)
            self.worker.start()

    def _capture(self):
        session = self.session
        try:
            self.last_error = None
            if self.credentials:
                self.shell("printf SSH_READY")
            else:
                subprocess.run(self.ssh_command("printf SSH_READY"), capture_output=True, check=True, timeout=12)
            self.install()
            if self.state == "stopping":
                return
            command = f"sh {BASE}/session.sh {session} 2>{BASE}/{session}-wrapper.log"
            if self.credentials:
                self.proc = SSHStream(self.ssh_client(), command, self.output / (session + "-transport.log"))
            else:
                with (self.output / (session + "-transport.log")).open("wb") as log:
                    self.proc = subprocess.Popen(self.ssh_command(command), stdout=subprocess.PIPE, stderr=log)
            threading.Thread(target=self._lease, args=(session,), daemon=True).start()
            if self.proc.stdout.readline() != MAGIC:
                raise RuntimeError("Не удалось получить PCM. Проверьте физический mute и диагностику сеанса.")
            with self.lock:
                if self.state == "preparing":
                    self.state, self.message = "ready", "Микрофон колонки подключён. Ассистент временно выключен."
            while True:
                data = self.proc.stdout.read(4096)
                if not data:
                    break
                self.consume(data)
            self.proc.wait(timeout=8)
        except Exception as exc:
            try:
                (self.output / (session + "-error.log")).write_text(traceback.format_exc())
            except OSError:
                pass  # Disk failure must never prevent device restoration.
            with self.lock:
                self.message = str(exc) if isinstance(exc, RuntimeError) else "Сбой соединения. Проверяем восстановление колонки."
                self.last_error = self.message
        finally:
            if self.proc and self.proc.stdout:
                try:
                    self.proc.stdout.close()
                except OSError:
                    pass
            if isinstance(self.proc, SSHStream):
                self.proc.client.close()
            with self.lock:
                self.finalize_safely(interrupted=True)
                self.state = "stopping"
            try:
                self.shell(f"test ! -d {BASE}/{session} || touch {BASE}/{session}/stop", timeout=5)
            except Exception:
                pass  # Device watchdog is independent of this process/connection.
            self._verify_restore()

    def _lease(self, session):
        count = 0
        while self.session == session and self.state in {"preparing", "ready", "stopping"}:
            now = time.monotonic()
            if self.state == "stopping" or now - self.last_client > 10 or now - self.began > 570:
                self.stop(interrupted=True)
                return
            if self.state == "preparing" and now - self.began > 30:
                self.stop(interrupted=True)
                return
            try:
                count += 1
                self.shell(f"test ! -d {BASE}/{session} || echo {count} > {BASE}/{session}/heartbeat", timeout=5)
            except Exception:
                self.stop(interrupted=True)
                return
            time.sleep(2)

    def consume(self, data):
        # An interrupted binary stream may end with an incomplete final sample.
        data = data[:len(data) // 2 * 2]
        values = struct.unpack("<" + "h" * (len(data) // 2), data)
        if not values:
            return
        rms = math.sqrt(sum(v * v for v in values) / len(values)) / 32768
        with self.lock:
            self.samples += len(values)
            self.level = round(20 * math.log10(max(rms, 1e-5)), 1)
            if self.take:
                take = self.take
                take["wav"].writeframesraw(data)
                take["frames"] += len(values)
                take["peak"] = max(take["peak"], max(abs(v) for v in values))
                limit = 480
                if time.monotonic() - take["monotonic"] >= limit:
                    self.finish_take()

    def start_take(self, block, split, note):
        if block not in BLOCKS or split not in {"train", "test"}:
            raise ValueError("Неизвестный блок или назначение записи.")
        with self.lock:
            if self.state != "ready" or self.take:
                raise ValueError("Дождитесь подключения микрофона и завершите предыдущий дубль.")
            ident = secrets.token_hex(16)
            path = self.output / (ident + ".wav")
            wav = wave.open(str(path), "wb")
            wav.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
            self.take = {"id": ident, "block": block, "split": split, "note": str(note)[:300],
                "wav": wav, "frames": 0, "peak": 0, "monotonic": time.monotonic(),
                "created": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "interrupted": False}

    def finish_take(self, interrupted=False):
        with self.lock:
            if not self.take:
                return
            take, self.take = self.take, None
            take.pop("wav").close()
            take.pop("monotonic")
            take["interrupted"] = interrupted
            take["seconds"] = round(take["frames"] / 16000, 3)
            take["sample_rate"] = 16000
            take["channels"] = 1
            take["format"] = "PCM_S16LE"
            take["source"] = "station VQE_OMNI_WITH_GAIN; wake and command detection disabled"
            take["session"] = self.session
            take["capture_helper_sha256"] = self.helper_hash
            take["sha256"] = hashlib.sha256((self.output / (take["id"] + ".wav")).read_bytes()).hexdigest()
            (self.output / (take["id"] + ".json")).write_text(json.dumps(take, ensure_ascii=False, indent=2))

    def stop(self, interrupted=False):
        with self.lock:
            if self.restored:
                return
            self.finalize_safely(interrupted=interrupted)
            self.state = "stopping"
            session = self.session
        try:
            self.shell(f"test ! -d {BASE}/{session} || touch {BASE}/{session}/stop", timeout=5)
        except Exception:
            pass

    def finalize_safely(self, interrupted=False):
        try:
            self.finish_take(interrupted=interrupted)
        except (OSError, wave.Error):
            self.last_error = "Не удалось сохранить дубль. Проверьте свободное место на Mac; восстановление колонки продолжается."

    def exclude(self, ident, excluded):
        if not isinstance(ident, str) or not re.fullmatch(r"[a-f0-9]{32}", ident) or not isinstance(excluded, bool):
            raise ValueError("Некорректный дубль.")
        with self.lock:
            path = self.output / (ident + ".json")
            if not path.is_file():
                raise ValueError("Дубль не найден.")
            meta = json.loads(path.read_text())
            meta["excluded"] = excluded
            path.write_text(json.dumps(meta, ensure_ascii=False, indent=2))

    def _verify_restore(self):
        for _ in range(25):
            try:
                status = self.shell("pidof pulsar >/dev/null && pidof supervise.sh >/dev/null && "
                    "test -f /data/pulsar/enabled && "
                    "test ! -d /tmp/ricardo-voice-recorder.lock && echo restored", timeout=3)
                if status == "restored":
                    with self.lock:
                        self.restored = True
                        self.state, self.message = "idle", "Pulsar снова запущен. Ассистент возвращён в обычный режим."
                    return
            except Exception:
                pass
            time.sleep(1)
        with self.lock:
            self.state = "error"
            self.message = "Восстановление не подтверждено. На колонке есть watchdog; проверьте связь и выполните recovery по README."

    def status(self):
        with self.lock:
            self.last_client = time.monotonic()
            take = self.take
            items = []
            for path in sorted(self.output.glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True):
                try:
                    item = json.loads(path.read_text())
                    if re.fullmatch(r"[a-f0-9]{32}", item.get("id", "")):
                        items.append(item)
                except (OSError, ValueError):
                    continue
            return {"state": self.state, "message": self.message, "restored": self.restored,
                "last_error": self.last_error,
                "db": self.level, "remaining": max(0, int(570 - (time.monotonic() - self.began))) if not self.restored else 0,
                "take": {"seconds": round(take["frames"] / 16000, 1), "block": take["block"]} if take else None,
                "files": items}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, code, data, content="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", content)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("X-Ricardo-Recorder", "local-v1")
        self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")
        self.end_headers()
        self.wfile.write(data)

    def valid_host(self):
        return self.headers.get("Host") == self.server.authority

    def do_GET(self):
        if not self.valid_host():
            return self.send(403, b'{}')
        if self.path == "/api/status":
            if not secrets.compare_digest(self.headers.get("X-Recorder-Token", ""), self.server.token):
                return self.send(403, b'{}')
            return self.send(200, json.dumps(self.server.recorder.status(), ensure_ascii=False).encode())
        if self.path == "/api/session-token":
            return self.send(200, json.dumps({"token": self.server.token}).encode())
        if self.path in {"/", "/app.js", "/style.css"}:
            name = {"/": "index.html", "/app.js": "app.js", "/style.css": "style.css"}[self.path]
            kind = {"/": "text/html; charset=utf-8", "/app.js": "text/javascript", "/style.css": "text/css"}[self.path]
            return self.send(200, (HERE / "web" / name).read_bytes(), kind)
        match = re.fullmatch(r"/audio/([a-f0-9]{32})", self.path)
        if match:
            path = self.server.recorder.output / (match[1] + ".wav")
            # Only completed, labelled takes can be played.
            if path.is_file() and path.with_suffix(".json").is_file():
                return self.send(200, path.read_bytes(), "audio/wav")
        self.send(404, b'{}')

    def do_POST(self):
        if not self.valid_host() or self.headers.get("Origin") != "http://" + self.server.authority or not secrets.compare_digest(self.headers.get("X-Recorder-Token", ""), self.server.token):
            return self.send(403, b'{}')
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 <= length <= 2048:
                return self.send(413, b'{}')
            body = json.loads(self.rfile.read(length) or b'{}')
            if not isinstance(body, dict):
                raise ValueError("Ожидался JSON-объект.")
            rec = self.server.recorder
            if self.path == "/api/start":
                if body.get("consent") is not True:
                    raise ValueError("Подтвердите временное отключение Pulsar.")
                rec.start()
            elif self.path == "/api/stop": rec.stop()
            elif self.path == "/api/take/start": rec.start_take(body.get("block"), body.get("split"), body.get("note", ""))
            elif self.path == "/api/take/stop": rec.finalize_safely()
            elif self.path == "/api/take/exclude": rec.exclude(body.get("id"), body.get("excluded"))
            else: return self.send(404, b'{}')
            self.send(200, b'{"ok":true}')
        except (ValueError, TypeError) as exc:
            self.send(400, json.dumps({"error": str(exc)}, ensure_ascii=False).encode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--station", required=True, help="ADB serial/address, never persisted in source")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--ssh-target", required=True, help="SSH user@host")
    auth = parser.add_mutually_exclusive_group(required=True)
    auth.add_argument("--ssh-key", type=Path)
    auth.add_argument("--credentials", type=Path, help="Private CREDENTIALS.md with SSH-пароль field")
    parser.add_argument("--ssh-port", type=int, default=22)
    parser.add_argument("--helper", type=Path, default=ROOT / "secrets/local-tools/voice-recorder-target/armv7-unknown-linux-gnueabihf/release/ricardo-voicestream")
    parser.add_argument("--output", type=Path, default=ROOT / "secrets/voice-recorder/recordings")
    parser.add_argument("--remove-device", action="store_true")
    parser.add_argument("--recover-device", action="store_true")
    parser.add_argument("--open-browser", action="store_true")
    args = parser.parse_args()
    os.umask(0o077)
    rec = Recorder(args.station, args.output, args.helper, args.adb,
        args.ssh_target, args.ssh_key.expanduser() if args.ssh_key else None, args.ssh_port, args.credentials)
    if args.recover_device:
        print(rec.shell(f"sh {BASE}/recover.sh", timeout=25))
        return
    if args.remove_device:
        rec.shell(f"test ! -d /tmp/ricardo-voice-recorder.lock && ! pidof ricardo-voicestream >/dev/null && busybox rm -rf {BASE}")
        print("Временные файлы инструмента удалены с колонки. Записи на Mac сохранены.")
        return
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.recorder, server.authority, server.token = rec, f"127.0.0.1:{server.server_port}", secrets.token_urlsafe(32)
    print(f"Откройте http://{server.authority} — колонка не переключается до явного старта.", flush=True)
    if args.open_browser and not (args.recover_device or args.remove_device):
        import webbrowser
        webbrowser.open("http://" + server.authority)
    def shutdown(*_):
        threading.Thread(target=server.shutdown, daemon=True).start()
    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    try:
        server.serve_forever()
    finally:
        rec.stop()
        if rec.worker:
            rec.worker.join(timeout=35)
        server.server_close()

if __name__ == "__main__":
    main()
