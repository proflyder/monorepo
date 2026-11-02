"""Hermetic tests for the offsite backup script.

No restic binary, network or real home state is used: restic is a stub script,
HA is a local fake WebSocket/HTTP server, and every source is a temporary file.
"""
from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import socketserver
import sys
import sqlite3
import struct
import tempfile
import threading
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("offsite_backup", HERE / "offsite_backup.py")
backup = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = backup
SPEC.loader.exec_module(backup)

WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
DOWNLOAD_BODY = b"fake-home-assistant-backup"


class FakeWSConnection:
    def __init__(self, sock: socket.socket, initial: bytes = b""):
        self.sock = sock
        self.buffer = initial

    def _read_exact(self, size: int) -> bytes:
        while len(self.buffer) < size:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("closed")
            self.buffer += chunk
        data, self.buffer = self.buffer[:size], self.buffer[size:]
        return data

    def recv_json(self) -> dict:
        first, second = self._read_exact(2)
        length = second & 0x7F
        if length == 126:
            length = struct.unpack("!H", self._read_exact(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self._read_exact(8))[0]
        mask = self._read_exact(4) if second & 0x80 else b""
        payload = self._read_exact(length)
        if mask:
            payload = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        return json.loads(payload.decode("utf-8"))

    def send_json(self, value: dict) -> None:
        payload = json.dumps(value).encode("utf-8")
        header = bytearray([0x81])
        if len(payload) < 126:
            header.append(len(payload))
        elif len(payload) < 65536:
            header.append(126)
            header.extend(struct.pack("!H", len(payload)))
        else:
            header.append(127)
            header.extend(struct.pack("!Q", len(payload)))
        self.sock.sendall(bytes(header) + payload)


class FakeHAHandler(socketserver.BaseRequestHandler):
    def handle(self) -> None:
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = self.request.recv(4096)
            if not chunk:
                return
            data += chunk
        head, _, rest = data.partition(b"\r\n\r\n")
        lines = head.decode("latin-1").split("\r\n")
        request_line = lines[0]
        headers = {}
        for line in lines[1:]:
            key, _, value = line.partition(":")
            headers[key.strip().lower()] = value.strip()
        path = request_line.split(" ")[1]
        if path.startswith("/api/websocket"):
            self._websocket(headers, rest)
        elif path.startswith("/api/backup/download/"):
            if headers.get("authorization") != "Bearer test-token":
                self.request.sendall(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
                return
            body = DOWNLOAD_BODY
            self.request.sendall(
                b"HTTP/1.1 200 OK\r\nContent-Length: "
                + str(len(body)).encode()
                + b"\r\nConnection: close\r\n\r\n"
                + body
            )
        else:
            self.request.sendall(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")

    def _websocket(self, headers: dict, rest: bytes) -> None:
        key = headers.get("sec-websocket-key", "")
        accept = base64.b64encode(
            hashlib.sha1((key + WS_GUID).encode("ascii")).digest()
        ).decode("ascii")
        self.request.sendall(
            (
                "HTTP/1.1 101 Switching Protocols\r\n"
                "Upgrade: websocket\r\n"
                "Connection: Upgrade\r\n"
                f"Sec-WebSocket-Accept: {accept}\r\n\r\n"
            ).encode("ascii")
        )
        connection = FakeWSConnection(self.request, rest)
        connection.send_json({"type": "auth_required"})
        auth = connection.recv_json()
        if auth.get("type") != "auth" or auth.get("access_token") != "test-token":
            connection.send_json({"type": "auth_invalid"})
            return
        connection.send_json({"type": "auth_ok"})
        command = connection.recv_json()
        if command.get("type") != "backup/info":
            connection.send_json(
                {"id": command.get("id"), "type": "result", "success": False, "error": {}}
            )
            return
        connection.send_json(
            {
                "id": command.get("id"),
                "type": "result",
                "success": True,
                "result": {"backups": self.server.backups},
            }
        )


class FakeHAServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, backups: list[dict]):
        super().__init__(("127.0.0.1", 0), FakeHAHandler)
        self.backups = backups
        self.thread = threading.Thread(target=self.serve_forever, daemon=True)

    def __enter__(self) -> "FakeHAServer":
        self.thread.start()
        return self

    def __exit__(self, *exc_info: object) -> None:
        self.shutdown()
        self.server_close()
        self.thread.join(timeout=5)


def make_restic_stub(directory: Path) -> Path:
    stub = directory / "restic-stub"
    stub.write_text(
        "#!/bin/sh\n"
        'printf "%s\\n" "$*" >> "$RESTIC_LOG"\n'
        'if [ "$1" = "backup" ]; then\n'
        "  for last do :; done\n"
        '  cp "$last/MANIFEST.json" "$RESTIC_LOG.manifest"\n'
        "fi\n"
        "exit 0\n",
        encoding="utf-8",
    )
    stub.chmod(0o755)
    return stub


def make_sqlite(path: Path, value: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(path)
    try:
        connection.execute("CREATE TABLE sample (value TEXT)")
        connection.execute("INSERT INTO sample VALUES (?)", (value,))
        connection.commit()
    finally:
        connection.close()


class OffsiteBackupTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.now = backup.dt.datetime(2026, 11, 2, 5, 0, tzinfo=backup.UTC)
        self.openbao = self.root / "openbao"
        self.storage = self.root / "storage"
        self.staging_root = self.root / "staging"
        self.metrics = self.root / "offsite_backup.prom"
        self.state_file = self.root / "state.json"
        self.token = self.root / "ha.token"
        self.password = self.root / "restic.password"
        self.restic_log = self.root / "restic.log"
        self.token.write_text("test-token", encoding="utf-8")
        self.password.write_text("restic-password", encoding="utf-8")

        self.openbao.mkdir()
        self.snapshot = self.openbao / "openbao-20261102T040000Z.snap"
        self.snapshot.write_bytes(b"raft-snapshot")
        os.utime(self.snapshot, (self.now.timestamp() - 3600,) * 2)

        make_sqlite(
            self.storage / "pvc-abc_home-assistant_scheduler-data" / "scheduler.db",
            "schedule",
        )
        bridge = self.storage / "pvc-def_home-assistant_ha-bridge-data"
        bridge.mkdir(parents=True)
        (bridge / "ledger.json").write_text('{"delivered": 1}', encoding="utf-8")
        make_sqlite(
            self.storage / "pvc-ghi_currency-bot_currency-bot-data" / "currency.db",
            "rates",
        )

        self.backups = [
            {
                "backup_id": "newest",
                "date": (self.now - backup.dt.timedelta(hours=2)).isoformat(),
                "agent_id": "hassio.local",
            },
            {
                "backup_id": "older",
                "date": (self.now - backup.dt.timedelta(days=2)).isoformat(),
                "agent_id": "hassio.local",
            },
        ]
        self.server = FakeHAServer(self.backups)
        self.server.__enter__()
        self.addCleanup(self.server.__exit__, None, None, None)
        self.addCleanup(self.temp.cleanup)

    def config(self, **overrides: str) -> tuple[backup.Config, Path]:
        stub = make_restic_stub(self.root)
        values = {
            "RESTIC_REPOSITORY": "s3:http://127.0.0.1:9000/test-bucket",
            "RESTIC_PASSWORD_FILE": str(self.password),
            "OFFSITE_METRICS_FILE": str(self.metrics),
            "OFFSITE_STATE_FILE": str(self.state_file),
            "OFFSITE_STAGING_ROOT": str(self.staging_root),
            "OPENBAO_SNAPSHOT_DIR": str(self.openbao),
            "K3S_STORAGE_DIR": str(self.storage),
            "HA_URL": f"http://127.0.0.1:{self.server.server_address[1]}",
            "HA_TOKEN_FILE": str(self.token),
            "RESTIC_BIN": str(stub),
            "RESTIC_MAINTENANCE_WEEKDAY": str(self.now.weekday()),
        }
        values.update(overrides)
        return backup.Config.from_mapping(values), stub

    def test_parse_env_file_supports_comments_and_quotes(self) -> None:
        env = self.root / "env"
        env.write_text(
            "# comment\nexport A=1\nB='two words'\nC=\"three\"\n",
            encoding="utf-8",
        )
        self.assertEqual(
            backup.parse_env_file(env), {"A": "1", "B": "two words", "C": "three"}
        )

    def test_parse_env_error_does_not_echo_the_line(self) -> None:
        env = self.root / "env"
        env.write_text("SECRET_VALUE_WITHOUT_EQUALS\n", encoding="utf-8")
        with self.assertRaises(backup.BackupError) as raised:
            backup.parse_env_file(env)
        self.assertNotIn("SECRET_VALUE", str(raised.exception))

    def test_forbidden_names_are_refused(self) -> None:
        for name in ("openbao-seal.json", "recovery-keys.txt", "restic.key", "SEAL"):
            with self.subTest(name=name), self.assertRaises(backup.BackupError):
                backup.assert_safe_name(name)
        backup.assert_safe_name("ledger.json")

    def test_sqlite_backup_uses_online_copy_and_integrity_check(self) -> None:
        source = self.root / "source.db"
        make_sqlite(source, "value")
        dest = self.root / "copy.db"
        backup.sqlite_backup(source, dest, shutil.which("sqlite3") or "sqlite3")
        with sqlite3.connect(dest) as connection:
            self.assertEqual(connection.execute("SELECT value FROM sample").fetchone()[0], "value")

    def test_full_run_collects_all_sources_and_writes_metrics(self) -> None:
        cfg, _stub = self.config()
        with patch.dict(os.environ, {"RESTIC_LOG": str(self.restic_log)}):
            backup.execute(cfg, self.now)

        calls = self.restic_log.read_text(encoding="utf-8").splitlines()
        self.assertTrue(any(line.startswith("backup --tag offsite") for line in calls))
        forget = [line for line in calls if line.startswith("forget")]
        self.assertEqual(len(forget), 1)
        self.assertIn("--prune", forget[0])
        self.assertTrue(any(line.startswith("check --read-data-subset=5%") for line in calls))

        # Staging is removed and no seal/recovery key can appear in it.
        self.assertEqual(list(self.staging_root.iterdir()), [])

        metrics = self.metrics.read_text(encoding="utf-8")
        self.assertIn("home_offsite_backup_last_run_success 1", metrics)
        self.assertIn(
            f"home_offsite_backup_last_success_timestamp_seconds {int(self.now.timestamp())}",
            metrics,
        )
        for source in backup.SOURCES:
            self.assertIn(f'source="{source}"', metrics)

        state = json.loads(self.state_file.read_text(encoding="utf-8"))
        self.assertEqual(int(state["last_success"]), int(self.now.timestamp()))
        self.assertGreater(state["last_bytes"], 0)

        manifest = self._last_staged_manifest()
        self.assertEqual(
            {item["source"] for item in manifest["files"]},
            set(backup.SOURCES),
        )
        self.assertNotIn("token", json.dumps(manifest).lower())
        self.assertNotIn("password", json.dumps(manifest).lower())

    def _last_staged_manifest(self) -> dict:
        # The stub restic copies MANIFEST.json before the run removes staging.
        return json.loads(
            Path(str(self.restic_log) + ".manifest").read_text(encoding="utf-8")
        )

    def test_stale_openbao_keeps_previous_success_and_reports_failure(self) -> None:
        self.state_file.write_text(
            json.dumps({"last_success": 111.0, "last_bytes": 42, "sources": {"ha": 222.0}}),
            encoding="utf-8",
        )
        old = self.now - backup.dt.timedelta(hours=48)
        old_snapshot = self.openbao / "openbao-20261031T050000Z.snap"
        self.snapshot.rename(old_snapshot)
        os.utime(old_snapshot, (old.timestamp(), old.timestamp()))
        cfg, _stub = self.config()
        with patch.dict(os.environ, {"RESTIC_LOG": str(self.restic_log)}):
            with self.assertRaises(backup.BackupError):
                backup.execute(cfg, self.now)
        metrics = self.metrics.read_text(encoding="utf-8")
        self.assertIn("home_offsite_backup_last_run_success 0", metrics)
        self.assertIn("home_offsite_backup_last_success_timestamp_seconds 111", metrics)
        self.assertIn('home_backup_source_timestamp_seconds{source="ha"} 222', metrics)
        self.assertFalse(self.restic_log.exists())

    def test_restic_failure_is_reported_without_leaking_output(self) -> None:
        failing = self.root / "restic-failing"
        failing.write_text(
            "#!/bin/sh\n"
            "echo 's3://user:secret@example.invalid/bucket' >&2\n"
            "exit 1\n",
            encoding="utf-8",
        )
        failing.chmod(0o755)
        cfg, _stub = self.config(RESTIC_BIN=str(failing))
        with self.assertRaises(backup.BackupError) as raised:
            backup.execute(cfg, self.now)
        self.assertNotIn("secret", str(raised.exception))
        metrics = self.metrics.read_text(encoding="utf-8")
        self.assertIn("home_offsite_backup_last_run_success 0", metrics)

    def test_pvc_paths_are_resolved_by_suffix(self) -> None:
        scheduler = backup.find_pvc_file(
            self.storage, "_home-assistant_scheduler-data/scheduler.db"
        )
        self.assertTrue(scheduler.name == "scheduler.db")
        self.assertIn("pvc-abc_home-assistant_scheduler-data", str(scheduler))
        currency = backup.find_pvc_files(self.storage, "_currency-bot_currency-bot-data")
        self.assertEqual([item.name for item in currency], ["currency.db"])


if __name__ == "__main__":
    unittest.main()
