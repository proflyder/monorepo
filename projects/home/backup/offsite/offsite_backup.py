#!/usr/bin/env python3
"""Encrypted offsite backup of the home state via restic (any restic backend).

Run as root by ``offsite-backup.service`` on the K3s node. The script:

1. creates a fresh 0700 staging directory;
2. copies the newest OpenBao raft snapshot (fails if older than the limit);
3. takes an online SQLite backup of each database and verifies the copy with
   ``PRAGMA integrity_check``;
4. copies the ha-bridge ledger;
5. downloads the newest HA automatic backup through the HA WebSocket listing
   API and the REST download endpoint;
6. writes a secret-free ``MANIFEST.json`` with names, sizes, SHA-256 and source
   timestamps;
7. runs ``restic backup`` against the repository, performs retention (prune weekly) and a
   weekly ``restic check --read-data-subset=5%``;
8. writes Prometheus textfile metrics atomically and removes the staging dir.

Configuration comes only from a root-only env file (``/etc/offsite-backup/env``
by default). Values are never printed and never passed on a command line, so a
failure report cannot leak the restic password or backend credentials.

The HA backup is already encrypted by Home Assistant and is stored as-is.
OpenBao seal/recovery keys are deliberately *not* part of this backup: a guard
refuses any file whose name looks like a seal/recovery key.
"""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import gzip
import hashlib
import json
import os
import re
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import tarfile
import tempfile
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

UTC = dt.timezone.utc
DEFAULT_ENV = Path("/etc/offsite-backup/env")
SOURCES = ("openbao", "ha", "scheduler", "currency_bot", "ha_bridge", "bb")
FORBIDDEN_NAME = re.compile(r"(seal|recovery|\.key$)", re.IGNORECASE)
OPENBAO_NAME = re.compile(r"openbao-(\d{8}T\d{6}Z)\.snap$")
UTC_TIMESTAMP = "%Y%m%dT%H%M%SZ"


class BackupError(RuntimeError):
    """A failure that is safe to report without configuration values."""


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
@dataclass
class Config:
    restic_repository: str
    restic_password_file: str
    metrics_file: Path
    state_file: Path
    staging_root: Path
    openbao_dir: Path
    k3s_storage_dir: Path
    ha_url: str
    ha_token_file: Path
    ha_agent_id: str = "hassio.local"
    restic_bin: str = "restic"
    sqlite_bin: str = "sqlite3"
    # Optional bb server export pulled over ssh. Without BB_SSH_TARGET the
    # source is skipped; a configured target must expose only the export file.
    bb_ssh_target: str | None = None
    bb_ssh_key_file: Path = Path("/etc/offsite-backup/bb_ssh_key")
    bb_ssh_known_hosts: Path = Path("/etc/offsite-backup/bb_known_hosts")
    bb_ssh_timeout: float = 900.0
    ssh_bin: str = "ssh"
    gzip_bin: str = "gzip"
    max_source_age_hours: float = 36.0
    maintenance_weekday: int = 6  # Sunday
    # Guard rails against a runaway upload filling the (free) remote quota.
    max_staging_bytes: int = 2 * 1024**3
    max_repo_bytes: int = 5 * 1024**3
    restic_env: dict[str, str] = field(default_factory=dict)

    @classmethod
    def from_mapping(cls, values: dict[str, str]) -> "Config":
        def required(name: str) -> str:
            value = values.get(name, "").strip()
            if not value:
                raise BackupError(f"missing required setting {name}")
            return value

        def path(name: str, default: str | None = None) -> Path:
            value = values.get(name, "").strip() or default
            if not value:
                raise BackupError(f"missing required setting {name}")
            return Path(value)

        try:
            weekday = int(values.get("RESTIC_MAINTENANCE_WEEKDAY", "6"))
            max_age = float(values.get("MAX_SOURCE_AGE_HOURS", "36"))
            max_staging = int(values.get("MAX_STAGING_BYTES", str(2 * 1024**3)))
            max_repo = int(values.get("MAX_REPO_BYTES", str(5 * 1024**3)))
            bb_timeout = float(values.get("BB_SSH_TIMEOUT_SECONDS", "900"))
        except ValueError as exc:
            raise BackupError("invalid numeric setting") from exc
        if not 0 <= weekday <= 6:
            raise BackupError("RESTIC_MAINTENANCE_WEEKDAY must be 0..6")
        if max_age <= 0:
            raise BackupError("MAX_SOURCE_AGE_HOURS must be positive")
        if max_staging <= 0 or max_repo <= 0:
            raise BackupError("MAX_STAGING_BYTES/MAX_REPO_BYTES must be positive")
        if bb_timeout <= 0:
            raise BackupError("BB_SSH_TIMEOUT_SECONDS must be positive")

        # S3-style credentials or an rclone backend (RESTIC_REPOSITORY=rclone:...,
        # RCLONE_CONFIG pointing at a root-only, writable rclone.conf).
        credentials = {
            key: values[key]
            for key in values
            if values.get(key)
            and (key in ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_DEFAULT_REGION")
                 or key.startswith("RCLONE_"))
        }
        return cls(
            restic_repository=required("RESTIC_REPOSITORY"),
            restic_password_file=required("RESTIC_PASSWORD_FILE"),
            metrics_file=path(
                "OFFSITE_METRICS_FILE", "/var/lib/node-exporter-textfile/offsite_backup.prom"
            ),
            state_file=path("OFFSITE_STATE_FILE", "/var/lib/offsite-backup/state.json"),
            staging_root=path("OFFSITE_STAGING_ROOT", "/var/lib/offsite-backup"),
            openbao_dir=path("OPENBAO_SNAPSHOT_DIR", "/srv/k3s/volumes/openbao-backups"),
            k3s_storage_dir=path("K3S_STORAGE_DIR", "/srv/k3s/runtime/storage"),
            ha_url=required("HA_URL").rstrip("/"),
            ha_token_file=path("HA_TOKEN_FILE", "/etc/offsite-backup/ha.token"),
            ha_agent_id=values.get("HA_AGENT_ID", "hassio.local").strip() or "hassio.local",
            restic_bin=values.get("RESTIC_BIN", "restic").strip() or "restic",
            sqlite_bin=values.get("SQLITE3_BIN", "sqlite3").strip() or "sqlite3",
            bb_ssh_target=values.get("BB_SSH_TARGET", "").strip() or None,
            bb_ssh_key_file=path(
                "BB_SSH_KEY_FILE", "/etc/offsite-backup/bb_ssh_key"
            ),
            bb_ssh_known_hosts=path(
                "BB_SSH_KNOWN_HOSTS", "/etc/offsite-backup/bb_known_hosts"
            ),
            bb_ssh_timeout=bb_timeout,
            ssh_bin=values.get("SSH_BIN", "ssh").strip() or "ssh",
            gzip_bin=values.get("GZIP_BIN", "gzip").strip() or "gzip",
            max_source_age_hours=max_age,
            maintenance_weekday=weekday,
            max_staging_bytes=max_staging,
            max_repo_bytes=max_repo,
            restic_env=credentials,
        )


def parse_env_file(path: Path) -> dict[str, str]:
    """Parse a small KEY=VALUE env file without echoing any value."""
    values: dict[str, str] = {}
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export ") :].strip()
        if "=" not in line:
            raise BackupError(f"invalid env line {number}")
        key, value = line.split("=", 1)
        key = key.strip()
        if not key:
            raise BackupError(f"invalid env line {number}")
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
            value = value[1:-1]
        values[key] = value
    return values


# ---------------------------------------------------------------------------
# Generic helpers
# ---------------------------------------------------------------------------
def assert_safe_name(name: str) -> None:
    """Refuse seal/recovery/private-key material anywhere in the backup."""
    if FORBIDDEN_NAME.search(name):
        raise BackupError(f"refusing forbidden backup name: {name}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def atomic_write(path: Path, data: str, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def format_timestamp(epoch: float) -> str:
    return dt.datetime.fromtimestamp(epoch, tz=UTC).strftime(UTC_TIMESTAMP)


def epoch_age_hours(epoch: float, now: dt.datetime) -> float:
    return (now - dt.datetime.fromtimestamp(epoch, tz=UTC)).total_seconds() / 3600


def check_source_age(source: str, epoch: float, now: dt.datetime, max_hours: float) -> None:
    age = epoch_age_hours(epoch, now)
    if age > max_hours:
        raise BackupError(f"{source} source is stale ({age:.1f} h > {max_hours:.0f} h)")


def newest(paths: list[Path]) -> Path:
    if not paths:
        raise BackupError("no matching source file")
    return max(paths, key=lambda item: item.stat().st_mtime)


def find_pvc_file(base: Path, suffix: str) -> Path:
    """Resolve a PVC path by its ``_<namespace>_<pvc>`` suffix, never a uid."""
    matches = [path for path in base.glob(f"pvc-*{suffix}") if path.is_file()]
    return newest(matches)


def find_pvc_files(base: Path, suffix: str, pattern: str = "*.db") -> list[Path]:
    roots = [path for path in base.glob(f"pvc-*{suffix}") if path.is_dir()]
    files = [item for root in roots for item in root.glob(pattern) if item.is_file()]
    if not files:
        raise BackupError("no matching SQLite database")
    return sorted(files, key=lambda item: item.name)


# ---------------------------------------------------------------------------
# Source collection
# ---------------------------------------------------------------------------
@dataclass
class FileRecord:
    source: str
    path: str
    size: int
    sha256: str
    source_timestamp: float


def copy_source(source: str, src: Path, dest: Path, staging: Path) -> FileRecord:
    assert_safe_name(src.name)
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    os.chmod(dest, 0o600)
    return FileRecord(
        source=source,
        path=str(dest.relative_to(staging)),
        size=dest.stat().st_size,
        sha256=sha256_file(dest),
        source_timestamp=src.stat().st_mtime,
    )


def openbao_timestamp(path: Path) -> float:
    match = OPENBAO_NAME.search(path.name)
    if match:
        parsed = dt.datetime.strptime(match.group(1), UTC_TIMESTAMP).replace(tzinfo=UTC)
        return parsed.timestamp()
    return path.stat().st_mtime


def copy_openbao(cfg: Config, staging: Path, now: dt.datetime) -> FileRecord:
    candidates = [path for path in cfg.openbao_dir.glob("openbao-*.snap") if path.is_file()]
    if not candidates:
        raise BackupError("no OpenBao snapshot found")
    source = max(candidates, key=openbao_timestamp)
    timestamp = openbao_timestamp(source)
    check_source_age("openbao", timestamp, now, cfg.max_source_age_hours)
    dest = staging / "openbao" / source.name
    record = copy_source("openbao", source, dest, staging)
    record.source_timestamp = timestamp
    return record


def sqlite_backup(source: Path, dest: Path, sqlite_bin: str) -> None:
    """Use SQLite's online backup API and verify the resulting copy."""
    assert_safe_name(source.name)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if '"' in str(dest):
        raise BackupError("staging path must not contain a double quote")
    result = subprocess.run(
        [sqlite_bin, str(source), f'.backup "{dest}"'],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise BackupError(f"sqlite backup failed for {source.name}")
    verify = subprocess.run(
        [sqlite_bin, str(dest), "PRAGMA integrity_check;"],
        capture_output=True,
        text=True,
    )
    if verify.returncode != 0 or verify.stdout.strip() != "ok":
        raise BackupError(f"sqlite integrity check failed for {source.name}")
    os.chmod(dest, 0o600)


def copy_sqlite(cfg: Config, source: Path, dest_name: str, staging: Path) -> FileRecord:
    dest = staging / "sqlite" / dest_name
    sqlite_backup(source, dest, cfg.sqlite_bin)
    return FileRecord(
        source="",
        path=str(dest.relative_to(staging)),
        size=dest.stat().st_size,
        sha256=sha256_file(dest),
        source_timestamp=source.stat().st_mtime,
    )


# ---------------------------------------------------------------------------
# Minimal stdlib WebSocket client (HA backup listing)
# ---------------------------------------------------------------------------
class WebSocketError(BackupError):
    pass


class SimpleWebSocket:
    """Tiny RFC 6455 client sufficient for HA's JSON API.

    HA is reached over plain LAN HTTP (``ws://``); TLS is supported with the
    stdlib ``ssl`` module when ``HA_URL`` uses HTTPS. Keeping this in the
    stdlib avoids an extra apt package on the K3s node.
    """

    GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

    def __init__(self, url: str, timeout: float = 30.0):
        self._url = url
        self._timeout = timeout
        self._socket: socket.socket | None = None
        self._buffer = b""

    @classmethod
    def from_http_url(cls, url: str, timeout: float = 30.0) -> "SimpleWebSocket":
        parts = urllib.parse.urlsplit(url)
        scheme = {"http": "ws", "https": "wss"}.get(parts.scheme, parts.scheme)
        netloc = parts.netloc
        return cls(f"{scheme}://{netloc}/api/websocket", timeout=timeout)

    def __enter__(self) -> "SimpleWebSocket":
        self.connect()
        return self

    def __exit__(self, *exc_info: object) -> None:
        self.close()

    def connect(self) -> None:
        parts = urllib.parse.urlsplit(self._url)
        if parts.scheme not in ("ws", "wss"):
            raise WebSocketError("unsupported WebSocket scheme")
        host = parts.hostname or ""
        port = parts.port or (443 if parts.scheme == "wss" else 80)
        target = parts.path or "/"
        if parts.query:
            target += "?" + parts.query
        raw = socket.create_connection((host, port), timeout=self._timeout)
        if parts.scheme == "wss":
            raw = ssl.create_default_context().wrap_socket(raw, server_hostname=host)
        raw.settimeout(self._timeout)
        self._socket = raw
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        host_header = host if ":" not in host else f"[{host}]"
        if port not in (80, 443):
            host_header += f":{port}"
        request = (
            f"GET {target} HTTP/1.1\r\n"
            f"Host: {host_header}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self._socket.sendall(request.encode("ascii"))
        header = self._read_until(b"\r\n\r\n", limit=65536)
        status = header.split(b"\r\n", 1)[0]
        if b" 101 " not in status:
            raise WebSocketError("WebSocket upgrade rejected")

    def _read_until(self, marker: bytes, limit: int) -> bytes:
        assert self._socket is not None
        while marker not in self._buffer:
            if len(self._buffer) > limit:
                raise WebSocketError("WebSocket handshake too large")
            chunk = self._socket.recv(4096)
            if not chunk:
                raise WebSocketError("WebSocket closed during handshake")
            self._buffer += chunk
        head, _, rest = self._buffer.partition(marker)
        self._buffer = rest
        return head + marker

    def _read_exact(self, size: int) -> bytes:
        assert self._socket is not None
        while len(self._buffer) < size:
            chunk = self._socket.recv(max(4096, size - len(self._buffer)))
            if not chunk:
                raise WebSocketError("WebSocket closed")
            self._buffer += chunk
        data, self._buffer = self._buffer[:size], self._buffer[size:]
        return data

    def _send_frame(self, opcode: int, payload: bytes) -> None:
        assert self._socket is not None
        mask = os.urandom(4)
        header = bytearray([0x80 | opcode])
        length = len(payload)
        if length < 126:
            header.append(0x80 | length)
        elif length < 65536:
            header.append(0x80 | 126)
            header.extend(struct.pack("!H", length))
        else:
            header.append(0x80 | 127)
            header.extend(struct.pack("!Q", length))
        masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        self._socket.sendall(bytes(header) + mask + masked)

    def send_json(self, value: dict) -> None:
        self._send_frame(0x1, json.dumps(value, separators=(",", ":")).encode("utf-8"))

    def recv_json(self) -> dict:
        fragments: list[bytes] = []
        first_opcode: int | None = None
        while True:
            first, second = self._read_exact(2)
            fin = bool(first & 0x80)
            opcode = first & 0x0F
            masked = bool(second & 0x80)
            length = second & 0x7F
            if length == 126:
                length = struct.unpack("!H", self._read_exact(2))[0]
            elif length == 127:
                length = struct.unpack("!Q", self._read_exact(8))[0]
            mask = self._read_exact(4) if masked else b""
            payload = self._read_exact(length)
            if masked:
                payload = bytes(
                    byte ^ mask[index % 4] for index, byte in enumerate(payload)
                )
            if opcode == 0x8:
                raise WebSocketError("WebSocket closed by server")
            if opcode == 0x9:
                self._send_frame(0xA, payload)
                continue
            if opcode == 0xA:
                continue
            if opcode in (0x1, 0x2):
                first_opcode = opcode
            elif opcode != 0x0:
                raise WebSocketError("unexpected WebSocket frame")
            fragments.append(payload)
            if fin:
                if first_opcode != 0x1:
                    raise WebSocketError("unexpected binary WebSocket message")
                try:
                    value = json.loads(b"".join(fragments).decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                    raise WebSocketError("invalid JSON from WebSocket") from exc
                if not isinstance(value, dict):
                    raise WebSocketError("unexpected WebSocket message shape")
                return value

    def close(self) -> None:
        if self._socket is not None:
            try:
                self._send_frame(0x8, b"")
            except OSError:
                pass
            try:
                self._socket.close()
            finally:
                self._socket = None


def list_ha_backups(cfg: Config, token: str) -> list[dict]:
    """List HA backups through the WebSocket API.

    HA Core exposes the backup list only on its WebSocket API
    (``backup/info``); the Supervisor is not reachable from K3s. A tiny
    stdlib WebSocket client is the simplest dependency-free option here.
    """
    with SimpleWebSocket.from_http_url(cfg.ha_url) as websocket:
        first = websocket.recv_json()
        if first.get("type") == "auth_required":
            websocket.send_json({"type": "auth", "access_token": token})
            auth = websocket.recv_json()
            if auth.get("type") != "auth_ok":
                raise WebSocketError("HA WebSocket authentication failed")
        elif first.get("type") != "auth_ok":
            raise WebSocketError("unexpected HA WebSocket greeting")
        websocket.send_json({"id": 1, "type": "backup/info"})
        while True:
            message = websocket.recv_json()
            if message.get("type") != "result" or message.get("id") != 1:
                continue
            if not message.get("success"):
                raise WebSocketError("HA backup/info failed")
            result = message.get("result") or {}
            backups = result.get("backups") if isinstance(result, dict) else None
            if not isinstance(backups, list):
                raise WebSocketError("HA backup/info returned no backups")
            return [item for item in backups if isinstance(item, dict)]


def parse_backup_date(item: dict) -> float:
    for key in ("date", "created_at", "timestamp"):
        value = item.get(key)
        if isinstance(value, (int, float)):
            return float(value)
        if isinstance(value, str) and value:
            text = value.replace("Z", "+00:00")
            try:
                return dt.datetime.fromisoformat(text).timestamp()
            except ValueError:
                continue
    raise BackupError("HA backup has no usable timestamp")


def ha_download_target(cfg: Config, backup: dict) -> tuple[Path, float]:
    backup_id = str(backup.get("backup_id") or backup.get("id") or "")
    if not backup_id:
        raise BackupError("HA backup has no id")
    timestamp = parse_backup_date(backup)
    safe_id = re.sub(r"[^A-Za-z0-9._-]", "_", backup_id)
    dest = Path("home-assistant") / f"backup_{safe_id}.tar"
    return dest, timestamp


def fetch_ha(cfg: Config, token: str, staging: Path, now: dt.datetime) -> FileRecord:
    backups = list_ha_backups(cfg, token)
    if not backups:
        raise BackupError("HA has no backups")
    backup = max(backups, key=parse_backup_date)
    timestamp = parse_backup_date(backup)
    check_source_age("ha", timestamp, now, cfg.max_source_age_hours)
    relative, _ = ha_download_target(cfg, backup)
    assert_safe_name(relative.name)
    dest = staging / relative
    dest.parent.mkdir(parents=True, exist_ok=True)
    backup_id = str(backup.get("backup_id") or backup.get("id") or "")
    agent = str(backup.get("agent_id") or cfg.ha_agent_id)
    query = urllib.parse.urlencode({"agent_id": agent})
    url = (
        f"{cfg.ha_url}/api/backup/download/"
        f"{urllib.parse.quote(backup_id, safe='')}?{query}"
    )
    request = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    try:
        with urllib.request.urlopen(request, timeout=300) as response, dest.open("wb") as handle:
            if getattr(response, "status", 200) != 200:
                raise BackupError("HA backup download failed")
            shutil.copyfileobj(response, handle, length=1024 * 1024)
    except urllib.error.HTTPError as exc:
        raise BackupError(f"HA backup download failed (HTTP {exc.code})") from exc
    except urllib.error.URLError as exc:
        raise BackupError("HA backup download failed (connection error)") from exc
    os.chmod(dest, 0o600)
    return FileRecord(
        source="ha",
        path=str(relative),
        size=dest.stat().st_size,
        sha256=sha256_file(dest),
        source_timestamp=timestamp,
    )


# ---------------------------------------------------------------------------
# bb server export over ssh
# ---------------------------------------------------------------------------
BB_EXPORT_MEMBER = "bb-server-export.tgz"


def fetch_bb(cfg: Config, staging: Path, now: dt.datetime) -> FileRecord:
    """Pull the forced-command tar stream with the bb server export.

    The remote side runs ``tar -C /var/backups/bb -cf - bb-server-export.tgz``;
    the member is gzip, so it is checked with ``gzip -t`` and stored as an
    uncompressed tar for better restic deduplication.
    """
    if not cfg.bb_ssh_target:
        raise BackupError("bb source is not configured")
    for label, path in (
        ("bb ssh key", cfg.bb_ssh_key_file),
        ("bb known_hosts", cfg.bb_ssh_known_hosts),
    ):
        if not path.is_file():
            raise BackupError(f"{label} is missing")

    command = [
        cfg.ssh_bin,
        "-T",
        "-i",
        str(cfg.bb_ssh_key_file),
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        f"UserKnownHostsFile={cfg.bb_ssh_known_hosts}",
        "-o",
        "IdentitiesOnly=yes",
        "-o",
        "ConnectTimeout=15",
        cfg.bb_ssh_target,
    ]
    dest = staging / "bb" / "bb-server-export.tar"
    dest.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="bb-", dir=staging) as work:
        stream = Path(work) / "download.tar"
        try:
            with stream.open("wb") as handle:
                result = subprocess.run(
                    command,
                    stdout=handle,
                    stderr=subprocess.PIPE,
                    timeout=cfg.bb_ssh_timeout,
                )
        except FileNotFoundError as exc:
            raise BackupError("bb export download failed: ssh is not installed") from exc
        except subprocess.TimeoutExpired as exc:
            raise BackupError("bb export download timed out") from exc
        if result.returncode != 0:
            # Do not echo ssh stderr: it may contain host names from the config.
            raise BackupError(
                f"bb export download failed (ssh exit status {result.returncode})"
            )

        try:
            with tarfile.open(stream, mode="r:*") as outer:
                member = next(
                    (
                        item
                        for item in outer.getmembers()
                        if item.isfile() and Path(item.name).name == BB_EXPORT_MEMBER
                    ),
                    None,
                )
                if member is None:
                    raise BackupError(
                        f"bb export archive does not contain {BB_EXPORT_MEMBER}"
                    )
                timestamp = float(member.mtime)
                check_source_age("bb", timestamp, now, cfg.max_source_age_hours)
                extracted = outer.extractfile(member)
                if extracted is None:
                    raise BackupError("bb export member cannot be read")
                tgz = Path(work) / BB_EXPORT_MEMBER
                with tgz.open("wb") as handle:
                    shutil.copyfileobj(extracted, handle, length=1024 * 1024)
        except (tarfile.TarError, OSError) as exc:
            raise BackupError("bb export archive is not a valid tar") from exc
        os.chmod(tgz, 0o600)

        try:
            verified = subprocess.run(
                [cfg.gzip_bin, "-t", str(tgz)], capture_output=True, text=True
            )
        except FileNotFoundError as exc:
            raise BackupError("bb export check failed: gzip is not installed") from exc
        if verified.returncode != 0:
            raise BackupError("bb export is not a valid gzip archive")

        try:
            with gzip.open(tgz, "rb") as source, dest.open("wb") as handle:
                shutil.copyfileobj(source, handle, length=1024 * 1024)
        except (OSError, EOFError) as exc:
            raise BackupError("bb export could not be decompressed") from exc
        os.chmod(dest, 0o600)

    return FileRecord(
        source="bb",
        path=str(dest.relative_to(staging)),
        size=dest.stat().st_size,
        sha256=sha256_file(dest),
        source_timestamp=timestamp,
    )


# ---------------------------------------------------------------------------
# Staging, manifest, restic and metrics
# ---------------------------------------------------------------------------
def build_staging(cfg: Config, now: dt.datetime) -> tuple[Path, list[FileRecord], dict[str, float]]:
    cfg.staging_root.mkdir(parents=True, exist_ok=True)
    os.chmod(cfg.staging_root, 0o700)
    staging = Path(tempfile.mkdtemp(prefix="offsite-", dir=cfg.staging_root))
    os.chmod(staging, 0o700)
    records: list[FileRecord] = []
    timestamps: dict[str, float] = {}

    try:
        record = copy_openbao(cfg, staging, now)
        records.append(record)
        timestamps["openbao"] = record.source_timestamp

        scheduler = find_pvc_file(cfg.k3s_storage_dir, "_home-assistant_scheduler-data/scheduler.db")
        record = copy_sqlite(cfg, scheduler, "scheduler.db", staging)
        record.source = "scheduler"
        record.path = str(Path("sqlite") / "scheduler.db")
        records.append(record)
        timestamps["scheduler"] = record.source_timestamp

        ledger = find_pvc_file(cfg.k3s_storage_dir, "_home-assistant_ha-bridge-data/ledger.json")
        record = copy_source(
            "ha_bridge", ledger, staging / "ha-bridge" / "ledger.json", staging
        )
        records.append(record)
        timestamps["ha_bridge"] = record.source_timestamp

        currency = find_pvc_files(cfg.k3s_storage_dir, "_currency-bot_currency-bot-data")
        currency_timestamps: list[float] = []
        for index, source in enumerate(currency, start=1):
            name = f"currency_bot-{index}-{source.name}"
            record = copy_sqlite(cfg, source, name, staging)
            record.source = "currency_bot"
            records.append(record)
            currency_timestamps.append(record.source_timestamp)
        timestamps["currency_bot"] = max(currency_timestamps)

        token = cfg.ha_token_file.read_text(encoding="utf-8").strip()
        if not token:
            raise BackupError("HA token file is empty")
        record = fetch_ha(cfg, token, staging, now)
        records.append(record)
        timestamps["ha"] = record.source_timestamp

        if cfg.bb_ssh_target:
            record = fetch_bb(cfg, staging, now)
            records.append(record)
            timestamps["bb"] = record.source_timestamp
        else:
            print(
                "bb: BB_SSH_TARGET is not set; skipping bb server export",
                file=sys.stderr,
            )

        for path in staging.rglob("*"):
            if path.is_file():
                assert_safe_name(path.name)

        manifest = {
            "created_at": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "hostname": socket.gethostname(),
            "files": [
                {
                    "source": item.source,
                    "path": item.path,
                    "size": item.size,
                    "sha256": item.sha256,
                    "source_timestamp": format_timestamp(item.source_timestamp),
                    "source_timestamp_epoch": int(item.source_timestamp),
                }
                for item in records
            ],
        }
        manifest_path = staging / "MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        os.chmod(manifest_path, 0o600)
        return staging, records, timestamps
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def staging_bytes(staging: Path) -> int:
    return sum(path.stat().st_size for path in staging.rglob("*") if path.is_file())


def restic_environment(cfg: Config) -> dict[str, str]:
    env = dict(os.environ)
    env.update(cfg.restic_env)
    env["RESTIC_REPOSITORY"] = cfg.restic_repository
    env["RESTIC_PASSWORD_FILE"] = cfg.restic_password_file
    return env


def restic_command(cfg: Config, args: list[str], step: str) -> None:
    try:
        result = subprocess.run(
            [cfg.restic_bin, *args], env=restic_environment(cfg), capture_output=True, text=True
        )
    except FileNotFoundError as exc:
        raise BackupError(f"{step}: restic is not installed") from exc
    if result.returncode != 0:
        # Never include stderr/stdout: restic may echo the repository URL.
        raise BackupError(f"{step} failed with exit status {result.returncode}")


def restic_repo_bytes(cfg: Config) -> int:
    try:
        result = subprocess.run(
            [cfg.restic_bin, "stats", "--mode", "raw-data", "--json"],
            env=restic_environment(cfg), capture_output=True, text=True,
        )
    except FileNotFoundError as exc:
        raise BackupError("restic stats: restic is not installed") from exc
    if result.returncode != 0:
        raise BackupError(f"restic stats failed with exit status {result.returncode}")
    try:
        return int(json.loads(result.stdout).get("total_size", 0))
    except (ValueError, AttributeError) as exc:
        raise BackupError("restic stats returned unexpected output") from exc


def restic_backup(cfg: Config, staging: Path) -> None:
    restic_command(
        cfg,
        [
            "backup",
            "--tag",
            "offsite",
            "--tag",
            "home",
            "--host",
            socket.gethostname(),
            str(staging),
        ],
        "restic backup",
    )


def restic_maintenance(cfg: Config, now: dt.datetime) -> None:
    forget = [
        "forget",
        "--keep-daily",
        "7",
        "--keep-weekly",
        "4",
        "--keep-monthly",
        "6",
    ]
    if now.weekday() == cfg.maintenance_weekday:
        forget.append("--prune")
    restic_command(cfg, forget, "restic forget")
    if now.weekday() == cfg.maintenance_weekday:
        restic_command(cfg, ["check", "--read-data-subset=5%"], "restic check")


# ---------------------------------------------------------------------------
# Metrics
# ---------------------------------------------------------------------------
def load_state(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return {"last_success": 0.0, "last_bytes": 0, "sources": {}}
    if not isinstance(value, dict):
        return {"last_success": 0.0, "last_bytes": 0, "sources": {}}
    sources = value.get("sources")
    if not isinstance(sources, dict):
        sources = {}
    try:
        return {
            "last_success": float(value.get("last_success") or 0),
            "last_bytes": int(value.get("last_bytes") or 0),
            "sources": {str(key): float(item) for key, item in sources.items()},
        }
    except (TypeError, ValueError):
        return {"last_success": 0.0, "last_bytes": 0, "sources": {}}


def render_metrics(last_run: float, success: bool, state: dict) -> str:
    lines = [
        "# HELP home_offsite_backup_last_success_timestamp_seconds Unix time of the last complete offsite backup.",
        "# TYPE home_offsite_backup_last_success_timestamp_seconds gauge",
        f"home_offsite_backup_last_success_timestamp_seconds {state['last_success']:.0f}",
        "# HELP home_offsite_backup_last_run_timestamp_seconds Unix time of the last offsite backup attempt.",
        "# TYPE home_offsite_backup_last_run_timestamp_seconds gauge",
        f"home_offsite_backup_last_run_timestamp_seconds {last_run:.0f}",
        "# HELP home_offsite_backup_last_run_success Whether the last offsite backup attempt succeeded.",
        "# TYPE home_offsite_backup_last_run_success gauge",
        f"home_offsite_backup_last_run_success {1 if success else 0}",
        "# HELP home_offsite_backup_bytes Size of the last successful staging snapshot in bytes.",
        "# TYPE home_offsite_backup_bytes gauge",
        f"home_offsite_backup_bytes {state['last_bytes']}",
        "# HELP home_offsite_backup_repo_bytes Raw size of the remote restic repository in bytes.",
        "# TYPE home_offsite_backup_repo_bytes gauge",
        f"home_offsite_backup_repo_bytes {int(state.get('repo_bytes', 0))}",
        "# HELP home_backup_source_timestamp_seconds Source data timestamp in the last successful backup.",
        "# TYPE home_backup_source_timestamp_seconds gauge",
    ]
    for source in SOURCES:
        if source in state["sources"]:
            lines.append(
                f'home_backup_source_timestamp_seconds{{source="{source}"}} '
                f"{state['sources'][source]:.0f}"
            )
    return "\n".join(lines) + "\n"


def write_metrics(path: Path, last_run: float, success: bool, state: dict) -> None:
    atomic_write(path, render_metrics(last_run, success, state))


def write_failure_metrics(values: dict[str, str], now: dt.datetime) -> None:
    """Best-effort failure metric when configuration/loading itself failed."""
    try:
        metrics = Path(
            values.get(
                "OFFSITE_METRICS_FILE", "/var/lib/node-exporter-textfile/offsite_backup.prom"
            )
        )
        state_path = Path(values.get("OFFSITE_STATE_FILE", "/var/lib/offsite-backup/state.json"))
        write_metrics(metrics, now.timestamp(), False, load_state(state_path))
    except OSError:
        pass


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
def execute(cfg: Config, now: dt.datetime) -> dict:
    state = load_state(cfg.state_file)
    staging: Path | None = None
    success = False
    total = 0
    timestamps: dict[str, float] = {}
    try:
        staging, _records, timestamps = build_staging(cfg, now)
        total = staging_bytes(staging)
        if total > cfg.max_staging_bytes:
            raise BackupError(
                f"staging is {total} bytes, above MAX_STAGING_BYTES={cfg.max_staging_bytes}; refusing upload"
            )
        restic_backup(cfg, staging)
        restic_maintenance(cfg, now)
        repo = restic_repo_bytes(cfg)
        state["repo_bytes"] = repo
        if repo > cfg.max_repo_bytes:
            raise BackupError(
                f"repository is {repo} bytes, above MAX_REPO_BYTES={cfg.max_repo_bytes}"
            )
        success = True
    finally:
        if staging is not None:
            shutil.rmtree(staging, ignore_errors=True)
        if success:
            state["last_success"] = now.timestamp()
            state["last_bytes"] = total
            state["sources"] = timestamps
        atomic_write(cfg.state_file, json.dumps(state, sort_keys=True) + "\n", mode=0o600)
        write_metrics(cfg.metrics_file, now.timestamp(), success, state)
    return state


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--env", default=str(DEFAULT_ENV), help="root-only env file")
    args = parser.parse_args(argv)
    now = dt.datetime.now(UTC)
    values: dict[str, str] = {}
    try:
        values = parse_env_file(Path(args.env))
        cfg = Config.from_mapping(values)
        execute(cfg, now)
    except BackupError as exc:
        print(f"offsite backup failed: {exc}", file=sys.stderr)
        write_failure_metrics(values, now)
        return 1
    except Exception:
        # Keep tracebacks free of values that may contain credentials.
        print("offsite backup failed: unexpected error (see system journal)", file=sys.stderr)
        write_failure_metrics(values, now)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
