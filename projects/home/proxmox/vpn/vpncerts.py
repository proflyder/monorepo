#!/usr/bin/env python3
"""Issue and revoke client certificates for the home ocserv VPN.

The client CA lives only on the workstation, under
``secrets/home/vpn/client-ca/`` (mode 700, private key 600). The VPN container
receives only the CA certificate (``client-ca.pem``) and a CRL
(``client-ca.crl``); it never sees the CA private key.

Commands:

    init-ca              create the private client CA and the first CRL
    issue USER           issue a client certificate and an importable .p12
    revoke TARGET        revoke by serial number or certificate file
    list                 show the certificate database

Passwords, keys and certificate contents are never printed. The import
password is written to a mode-600 file next to each .p12.
"""
from __future__ import annotations

import argparse
import datetime as dt
import os
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import sys
import tempfile

USER_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
DEVICE_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,31}$")
GROUP_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
SERIAL_RE = re.compile(r"^(?:0x)?([0-9a-fA-F]{1,40})$")

ROOT = Path(__file__).resolve().parents[4]
DEFAULT_CA_DIR = ROOT / "secrets" / "home" / "vpn" / "client-ca"
DEFAULT_CLIENTS_DIR = ROOT / "secrets" / "home" / "vpn" / "clients"
DEFAULT_ORG = "home VPN"
CA_DAYS = 3650
CLIENT_DAYS = 730
CA_KEY_BITS = 4096
CLIENT_KEY_BITS = 2048

CA_KEY = "ca-key.pem"
CA_CERT = "ca-cert.pem"
CA_CRL = "crl.pem"
CA_CONF = "openssl.cnf"


class CertError(RuntimeError):
    """A user-facing certificate operation failed."""


# --------------------------------------------------------------------------
# pure helpers (unit tested)
# --------------------------------------------------------------------------
def validate_user(user: str) -> str:
    if not USER_RE.match(user):
        raise CertError("user must match ^[a-z_][a-z0-9_-]{0,31}$")
    return user


def validate_device(device: str) -> str:
    if not DEVICE_RE.match(device):
        raise CertError("device must match ^[a-z0-9][a-z0-9_-]{0,31}$")
    return device


def client_basename(user: str, device: str) -> str:
    return f"{validate_user(user)}-{validate_device(device)}"


def parse_groups(value: str) -> list[str]:
    groups = [part.strip() for part in value.split(",") if part.strip()]
    if not groups:
        raise CertError("at least one group is required")
    for group in groups:
        if not GROUP_RE.match(group):
            raise CertError(f"invalid group name: {group}")
    return groups


def cert_subject(user: str, groups: list[str]) -> str:
    """CN is the ocserv username; each OU is a group (cert-group-oid)."""
    subject = f"/CN={validate_user(user)}"
    for group in groups:
        if not GROUP_RE.match(group):
            raise CertError(f"invalid group name: {group}")
        subject += f"/OU={group}"
    return subject


def normalize_serial(value: str) -> str:
    """Return an uppercase hex serial without a leading 0x, or raise."""
    match = SERIAL_RE.match(value.strip())
    if not match:
        raise CertError(f"not a serial number: {value}")
    return match.group(1).upper()


def openssl_config(ca_dir: Path, org: str = DEFAULT_ORG, days: int = CLIENT_DAYS) -> str:
    """Render the openssl ca configuration used for every operation."""
    return f"""\
# Managed by projects/home/proxmox/vpn/vpncerts.py. Do not edit by hand.
[ ca ]
default_ca = CA_default

[ CA_default ]
dir = {ca_dir}
database = $dir/index.txt
new_certs_dir = $dir/newcerts
certificate = $dir/{CA_CERT}
private_key = $dir/{CA_KEY}
serial = $dir/serial
crlnumber = $dir/crlnumber
default_md = sha256
default_days = {days}
default_crl_days = {CA_DAYS}
policy = policy_any
unique_subject = no
email_in_dn = no
rand_serial = no
copy_extensions = none
x509_extensions = client_cert
crl_extensions = crl_ext

[ policy_any ]
commonName = supplied
organizationName = optional
organizationalUnitName = optional

[ req ]
distinguished_name = req_dn
prompt = no
default_md = sha256

[ req_dn ]
CN = placeholder

[ client_cert ]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = clientAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer

[ crl_ext ]
authorityKeyIdentifier = keyid:always

[ v3_ca ]
basicConstraints = critical,CA:true
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always
nsComment = "{org} client CA"
"""


def parse_index(text: str) -> list[dict[str, str]]:
    """Parse an openssl ``index.txt`` database.

    Each entry has: status (V/R/E), serial, expiry, revocation, subject.
    """
    entries = []
    for line in text.splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) < 6:
            raise CertError(f"malformed index entry: {line!r}")
        entries.append(
            {
                "status": fields[0],
                "expiry": fields[1],
                "revocation": fields[2],
                "serial": fields[3].upper(),
                "filename": fields[4],
                "subject": fields[5],
            }
        )
    return entries


def index_common_name(subject: str) -> str:
    for part in subject.split("/"):
        if part.startswith("CN="):
            return part[3:]
    return subject


def asn1_time_to_date(value: str) -> str:
    """Convert ``YYMMDDHHMMSSZ`` / ``YYYYMMDDHHMMSSZ`` to ``YYYY-MM-DD``."""
    value = value.strip()
    if value.endswith("Z"):
        value = value[:-1]
    if len(value) == 12:  # UTCTime: 2-digit year
        year = int(value[:2])
        year += 2000 if year < 50 else 1900
        value = f"{year:04d}" + value[2:]
    if len(value) != 14 or not value.isdigit():
        return value or "-"
    return f"{value[0:4]}-{value[4:6]}-{value[6:8]}"


# --------------------------------------------------------------------------
# openssl wrapper
# --------------------------------------------------------------------------
class Openssl:
    def __init__(self, binary: str = "openssl", verbose: bool = False):
        self.binary = binary
        self.verbose = verbose

    def run(
        self,
        args: list[str],
        *,
        input_text: str | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess:
        result = subprocess.run(
            [self.binary, *args],
            input=input_text,
            text=True,
            capture_output=True,
        )
        if check and result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            raise CertError(f"openssl {args[0]} failed: {detail}")
        return result

    def x509_field(self, path: Path, field: str) -> str:
        out = self.run(["x509", "-in", str(path), "-noout", "-" + field]).stdout.strip()
        # openssl prints "serial=AB12" / "notAfter=..." for these options.
        return out.split("=", 1)[1].strip() if "=" in out else out

    def genrsa(self, path: Path, bits: int) -> None:
        self.run(
            [
                "genpkey",
                "-algorithm",
                "RSA",
                "-pkeyopt",
                f"rsa_keygen_bits:{bits}",
                "-out",
                str(path),
            ]
        )

    def gen_crl(self, ca_dir: Path) -> None:
        self.run(
            [
                "ca",
                "-config",
                str(ca_dir / CA_CONF),
                "-gencrl",
                "-out",
                str(ca_dir / CA_CRL),
                "-crldays",
                str(CA_DAYS),
            ]
        )


def _p12_pem(ssl: Openssl, archive: Path, password: str) -> str:
    """Extract the leaf certificate from a .p12, tolerating legacy providers."""
    args = [
        "pkcs12",
        "-in",
        str(archive),
        "-clcerts",
        "-nokeys",
        "-passin",
        "stdin",
    ]
    attempts = [
        args + ["-provider", "default", "-provider", "legacy"],
        args,
    ]
    last_error: CertError | None = None
    for attempt in attempts:
        try:
            return ssl.run(attempt, input_text=password + "\n").stdout
        except CertError as exc:  # older openssl has no -provider flags
            last_error = exc
    raise CertError(f"cannot read {archive}: {last_error}")


def _export_legacy_pkcs12(
    ssl: Openssl,
    out: Path,
    key: Path,
    cert: Path,
    ca_cert: Path,
    name: str,
    password: str,
) -> None:
    base = [
        "pkcs12",
        "-export",
        "-inkey",
        str(key),
        "-in",
        str(cert),
        "-certfile",
        str(ca_cert),
        "-name",
        name,
        "-passout",
        "stdin",
        "-out",
        str(out),
    ]
    attempts = [
        base + ["-legacy", "-provider", "default", "-provider", "legacy"],
        base + ["-legacy"],
        base,
    ]
    last_error: CertError | None = None
    for attempt in attempts:
        try:
            ssl.run(attempt, input_text=password + "\n")
            return
        except CertError as exc:
            last_error = exc
    raise CertError(f"cannot write legacy p12 {out}: {last_error}")


def _write_password(path: Path, password: str) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(password + "\n")
    os.chmod(path, 0o600)


def _read_password(path: Path) -> str:
    if not path.exists():
        raise CertError(f"password file not found: {path}")
    return path.read_text(encoding="utf-8").strip()


def _random_serial() -> str:
    value = secrets.token_hex(16).upper()
    return value.lstrip("0") or "1"


# --------------------------------------------------------------------------
# CA / certificate operations
# --------------------------------------------------------------------------
def ensure_ca(ca_dir: Path, org: str, ssl: Openssl, force: bool = False) -> None:
    if (ca_dir / CA_KEY).exists() and not force:
        raise CertError(f"client CA already exists in {ca_dir}; use --force to replace")
    ca_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(ca_dir, 0o700)
    (ca_dir / "newcerts").mkdir(exist_ok=True, mode=0o700)
    conf = openssl_config(ca_dir, org=org)
    conf_path = ca_dir / CA_CONF
    conf_path.write_text(conf, encoding="utf-8")
    os.chmod(conf_path, 0o600)

    for name in ("index.txt.attr",):
        (ca_dir / name).unlink(missing_ok=True)
    (ca_dir / "index.txt").write_text("", encoding="utf-8")
    (ca_dir / "serial").write_text(_random_serial() + "\n", encoding="utf-8")
    (ca_dir / "crlnumber").write_text("1000\n", encoding="utf-8")

    key_path = ca_dir / CA_KEY
    ssl.genrsa(key_path, CA_KEY_BITS)
    os.chmod(key_path, 0o600)
    ssl.run(
        [
            "req",
            "-new",
            "-x509",
            "-sha256",
            "-days",
            str(CA_DAYS),
            "-key",
            str(key_path),
            "-out",
            str(ca_dir / CA_CERT),
            "-config",
            str(conf_path),
            "-extensions",
            "v3_ca",
            "-subj",
            f"/CN={org} Client CA/O={org}",
        ]
    )
    os.chmod(ca_dir / CA_CERT, 0o644)
    ssl.gen_crl(ca_dir)
    os.chmod(ca_dir / CA_CRL, 0o644)


def _write_next_serial(ca_dir: Path, serial: str) -> None:
    """Replace the CA serial file so the next certificate gets ``serial``."""
    (ca_dir / "serial").write_text(normalize_serial(serial) + "\n", encoding="utf-8")


def issue(
    ca_dir: Path,
    clients_dir: Path,
    user: str,
    device: str,
    ssl: Openssl,
    days: int = CLIENT_DAYS,
    force: bool = False,
    groups: list[str] | None = None,
) -> dict[str, str]:
    user = validate_user(user)
    groups = groups or ["lan", "full"]
    base = client_basename(user, device)
    ca_key = ca_dir / CA_KEY
    ca_cert = ca_dir / CA_CERT
    if not ca_key.exists() or not ca_cert.exists():
        raise CertError(f"client CA not initialized in {ca_dir}; run init-ca first")

    clients_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(clients_dir, 0o700)
    modern = clients_dir / f"{base}.p12"
    legacy = clients_dir / f"{base}.legacy.p12"
    password_path = clients_dir / f"{base}.password"
    if not force:
        for path in (modern, legacy, password_path):
            if path.exists():
                raise CertError(f"{path} already exists; use --force")

    password = secrets.token_urlsafe(24)
    tmp = Path(tempfile.mkdtemp(prefix=".issue-", dir=ca_dir))
    try:
        os.chmod(tmp, 0o700)
        key = tmp / "client-key.pem"
        csr = tmp / "client.csr"
        cert = tmp / "client-cert.pem"
        ssl.genrsa(key, CLIENT_KEY_BITS)
        os.chmod(key, 0o600)
        ssl.run(
            [
                "req",
                "-new",
                "-sha256",
                "-key",
                str(key),
                "-subj",
                cert_subject(user, groups),
                "-config",
                str(ca_dir / CA_CONF),
                "-out",
                str(csr),
            ]
        )
        _write_next_serial(ca_dir, _random_serial())
        ssl.run(
            [
                "ca",
                "-batch",
                "-notext",
                "-config",
                str(ca_dir / CA_CONF),
                "-extensions",
                "client_cert",
                "-days",
                str(days),
                "-in",
                str(csr),
                "-out",
                str(cert),
            ]
        )
        serial = normalize_serial(ssl.x509_field(cert, "serial"))
        expiry = ssl.x509_field(cert, "enddate")

        common = [
            "-inkey",
            str(key),
            "-in",
            str(cert),
            "-certfile",
            str(ca_cert),
            "-name",
            base,
            "-passout",
            "stdin",
        ]
        ssl.run(
            [
                "pkcs12",
                "-export",
                "-keypbe",
                "AES-256-CBC",
                "-certpbe",
                "AES-256-CBC",
                "-macalg",
                "sha256",
                "-out",
                str(modern),
                *common,
            ],
            input_text=password + "\n",
        )
        _export_legacy_pkcs12(ssl, legacy, key, cert, ca_cert, base, password)
        os.chmod(modern, 0o600)
        os.chmod(legacy, 0o600)
        _write_password(password_path, password)

        # Verify the archives open with the stored password and hold this cert.
        for archive in (modern, legacy):
            exported = _p12_pem(ssl, archive, password)
            check = ssl.run(["x509", "-noout", "-serial"], input_text=exported)
            if normalize_serial(check.stdout.split("=", 1)[-1]) != serial:
                raise CertError(f"{archive} does not contain the issued certificate")
    finally:
        for path in tmp.iterdir():
            path.unlink(missing_ok=True)
        tmp.rmdir()

    return {
        "user": user,
        "base": base,
        "serial": serial,
        "expiry": asn1_time_to_date(expiry),
        "groups": ",".join(groups),
        "p12": str(modern),
        "legacy_p12": str(legacy),
        "password_file": str(password_path),
    }


def _cert_file_for_target(target: str, ca_dir: Path, clients_dir: Path, ssl: Openssl) -> Path:
    path = Path(target).expanduser()
    if path.exists():
        if path.suffix == ".p12" or path.suffix == ".pfx":
            password = _read_password(path.with_suffix(".password"))
            pem = _p12_pem(ssl, path, password)
            cert = ca_dir / f".revoke-{secrets.token_hex(8)}.pem"
            cert.write_text(pem, encoding="utf-8")
            os.chmod(cert, 0o600)
            return cert
        return path

    serial = normalize_serial(target)
    index = ca_dir / "index.txt"
    if not index.exists():
        raise CertError(f"certificate database not found: {index}")
    for entry in parse_index(index.read_text(encoding="utf-8")):
        if entry["serial"] == serial:
            candidate = ca_dir / "newcerts" / f"{serial}.pem"
            if candidate.exists():
                return candidate
            for candidate in (ca_dir / "newcerts").glob("*.pem"):
                found = ssl.x509_field(candidate, "serial")
                if normalize_serial(found) == serial:
                    return candidate
            raise CertError(
                f"certificate {serial} is in the database ({entry['filename']}); "
                "revoke it with its .p12 file or a PEM copy"
            )
    raise CertError(f"serial not found in the database: {serial}")


def revoke(ca_dir: Path, clients_dir: Path, target: str, ssl: Openssl) -> dict[str, str]:
    if not (ca_dir / CA_CERT).exists():
        raise CertError(f"client CA not initialized in {ca_dir}; run init-ca first")
    cert = _cert_file_for_target(target, ca_dir, clients_dir, ssl)
    serial = normalize_serial(ssl.x509_field(cert, "serial"))
    ssl.run(
        [
            "ca",
            "-config",
            str(ca_dir / CA_CONF),
            "-revoke",
            str(cert),
        ]
    )
    ssl.gen_crl(ca_dir)
    os.chmod(ca_dir / CA_CRL, 0o644)
    return {
        "serial": serial,
        "crl": str(ca_dir / CA_CRL),
        "revoked_at": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }


def list_entries(ca_dir: Path) -> list[dict[str, str]]:
    index = ca_dir / "index.txt"
    if not index.exists():
        raise CertError(f"certificate database not found: {index}")
    entries = parse_index(index.read_text(encoding="utf-8"))
    for entry in entries:
        entry["cn"] = index_common_name(entry["subject"])
        entry["expiry_date"] = asn1_time_to_date(entry["expiry"])
    return entries


# --------------------------------------------------------------------------
# Proxmox transport
# --------------------------------------------------------------------------
class Pve:
    def __init__(
        self,
        host: str,
        ctid: int,
        key: Path | None = None,
        known_hosts: Path | None = None,
    ):
        self.ctid = ctid
        self.ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15"]
        if key:
            self.ssh += ["-i", str(key)]
        if known_hosts:
            self.ssh += ["-o", f"UserKnownHostsFile={known_hosts}"]
        self.ssh.append(f"root@{host}")

    def run(self, command: str, input_text: str | None = None, check: bool = True):
        return subprocess.run(
            self.ssh + [command],
            input=input_text,
            text=True,
            capture_output=True,
            check=check,
        )

    def push_text(self, content: str, path: str, perms: int = 0o644) -> None:
        tmp = f"/tmp/vpncerts-{secrets.token_hex(8)}"
        try:
            self.run(f"cat > {shlex.quote(tmp)}", input_text=content)
            self.run(
                f"pct push {self.ctid} {shlex.quote(tmp)} {shlex.quote(path)} "
                f"--perms {perms:o}"
            )
        finally:
            self.run(f"rm -f {shlex.quote(tmp)}", check=False)


def push_ca(pve: Pve, ca_dir: Path) -> None:
    pve.push_text((ca_dir / CA_CERT).read_text(encoding="utf-8"), "/etc/ocserv/ssl/client-ca.pem")
    push_crl(pve, ca_dir)


def push_crl(pve: Pve, ca_dir: Path) -> None:
    pve.push_text((ca_dir / CA_CRL).read_text(encoding="utf-8"), "/etc/ocserv/ssl/client-ca.crl")
    pve.run(f"pct exec {pve.ctid} -- ocserv --test-config")
    pve.run(f"pct exec {pve.ctid} -- systemctl reload ocserv")


def pve_from_args(args: argparse.Namespace) -> Pve | None:
    if not args.pve:
        return None
    return Pve(args.pve, args.ctid, args.ssh_key, args.known_hosts)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)

    def common(p: argparse.ArgumentParser, *, pve_required: bool = False) -> None:
        p.add_argument("--ca-dir", type=Path, default=DEFAULT_CA_DIR)
        p.add_argument("--clients-dir", type=Path, default=DEFAULT_CLIENTS_DIR)
        p.add_argument("--openssl", default="openssl")
        p.add_argument("--pve", required=pve_required, help="Proxmox host address")
        p.add_argument("--ctid", type=int, default=102)
        p.add_argument("--ssh-key", type=Path)
        p.add_argument("--known-hosts", type=Path)

    init = sub.add_parser("init-ca", help="create the private client CA")
    init.add_argument("--org", default=DEFAULT_ORG)
    init.add_argument("--force", action="store_true")
    common(init)

    issue_p = sub.add_parser("issue", help="issue a client certificate")
    issue_p.add_argument("user")
    issue_p.add_argument("--device", required=True)
    issue_p.add_argument("--days", type=int, default=CLIENT_DAYS)
    issue_p.add_argument(
        "--groups",
        default="lan,full",
        help="comma-separated ocserv groups stored as certificate OUs",
    )
    issue_p.add_argument("--force", action="store_true")
    common(issue_p)

    revoke_p = sub.add_parser("revoke", help="revoke by serial or certificate file")
    revoke_p.add_argument("target")
    common(revoke_p)

    list_p = sub.add_parser("list", help="show issued certificates")
    common(list_p)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    ssl = Openssl(args.openssl)
    ca_dir: Path = args.ca_dir
    clients_dir: Path = args.clients_dir

    try:
        if args.command == "init-ca":
            ensure_ca(ca_dir, args.org, ssl, force=args.force)
            pve = pve_from_args(args)
            if pve:
                push_ca(pve, ca_dir)
            print(f"client CA: {ca_dir / CA_CERT}")
            print(f"private key: {ca_dir / CA_KEY} (never copy this to the server)")
            print(f"crl: {ca_dir / CA_CRL}")
            if not pve:
                print("not pushed; pass --pve to install client-ca.pem and the CRL")
            return 0

        if args.command == "issue":
            issued = issue(
                ca_dir,
                clients_dir,
                args.user,
                args.device,
                ssl,
                days=args.days,
                force=args.force,
                groups=parse_groups(args.groups),
            )
            print(f"user: {issued['user']}")
            print(f"device: {args.device}")
            print(f"groups: {issued['groups']}")
            print(f"serial: {issued['serial']}")
            print(f"expires: {issued['expiry']}")
            print(f"p12: {issued['p12']}")
            print(f"legacy p12: {issued['legacy_p12']}")
            print(f"import password file: {issued['password_file']}")
            return 0

        if args.command == "revoke":
            result = revoke(ca_dir, clients_dir, args.target, ssl)
            pve = pve_from_args(args)
            if pve:
                push_crl(pve, ca_dir)
            print(f"revoked serial: {result['serial']}")
            print(f"crl: {result['crl']}")
            if pve:
                print("CRL pushed to the VPN container and ocserv reloaded")
            else:
                print("CRL updated locally; pass --pve to push it and reload ocserv")
            return 0

        if args.command == "list":
            entries = list_entries(ca_dir)
            if not entries:
                print("no certificates")
                return 0
            for entry in entries:
                status = {"V": "valid", "R": "revoked", "E": "expired"}.get(
                    entry["status"], entry["status"]
                )
                print(
                    f"{status:8} {entry['serial']:>32}  {entry['expiry_date']}  "
                    f"{entry['cn']}"
                )
            return 0
    except CertError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    sys.exit(main())
