#!/usr/bin/env python3
"""Ensure a DNS-only Cloudflare A record for a private home host.

    cloudflare_dns.py <token-env-file> <name> <address>

The token is read from CLOUDFLARE_API_TOKEN=... in the env file, so it never
appears in argv. The record is created, corrected (content/proxied) or left
alone; the zone is the longest Cloudflare zone that contains the name.
"""
from __future__ import annotations

import json
import sys
import urllib.error
import urllib.request

BASE = "https://api.cloudflare.com/client/v4"


def read_token(path: str) -> str:
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line.startswith("CLOUDFLARE_API_TOKEN="):
                token = line.split("=", 1)[1].strip().strip('"').strip("'")
                if token:
                    return token
    raise SystemExit(f"CLOUDFLARE_API_TOKEN is missing in {path}")


class Cloudflare:
    def __init__(self, token: str):
        self.token = token

    def call(self, method: str, path: str, body: dict | None = None) -> dict:
        data = json.dumps(body).encode() if body is not None else None
        request = urllib.request.Request(
            BASE + path, data=data, method=method,
            headers={"Authorization": "Bearer " + self.token,
                     "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise SystemExit("Cloudflare API %s %s failed: %s" % (
                method, path, error.read().decode(errors="replace")))


def ensure_a_record(api: Cloudflare, name: str, address: str) -> str:
    zones = api.call("GET", "/zones?per_page=50")["result"]
    matches = [zone for zone in zones
               if name == zone["name"] or name.endswith("." + zone["name"])]
    if not matches:
        raise SystemExit(f"no Cloudflare zone contains {name}")
    zone = max(matches, key=lambda entry: len(entry["name"]))
    records = api.call(
        "GET", "/zones/%s/dns_records?type=A&name=%s" % (zone["id"], name))["result"]
    body = {"type": "A", "name": name, "content": address, "ttl": 1, "proxied": False}
    if records:
        record = records[0]
        if record["content"] == address and record.get("proxied") is False:
            return "DNS A %s -> %s already set (DNS only)" % (name, address)
        api.call("PUT", "/zones/%s/dns_records/%s" % (zone["id"], record["id"]), body)
        return "DNS A %s -> %s updated (DNS only)" % (name, address)
    api.call("POST", "/zones/%s/dns_records" % zone["id"], body)
    return "DNS A %s -> %s created (DNS only)" % (name, address)


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        raise SystemExit(__doc__)
    token_file, name, address = argv
    print(ensure_a_record(Cloudflare(read_token(token_file)), name, address))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
