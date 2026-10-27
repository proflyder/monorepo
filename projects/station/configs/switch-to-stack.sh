#!/usr/bin/env bash
# Switch the station from the bootstrap proxy to the full v3rt3p stack.
# Reversible with switch-back.sh.
set -euo pipefail

STACK="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
MITM="${MITM_DIR:-$HOME/Desktop/alice-mitm-bootstrap}"
DOMAINS="$STACK/repos/https-proxy/domains/yandex.txt"

NAMES="$(awk 'NR > 1 && $0 !~ /^!/ && $0 !~ /^#/ && NF { print }' "$DOMAINS" | paste -sd, -)"
[[ -n "$NAMES" ]] || { echo "empty domain list" >&2; exit 1; }

echo "== dns names =="
echo "$NAMES"

python3 - "$MITM/.env" "$NAMES" <<'PY'
import sys
path, names = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines()
out, seen = [], False
for line in lines:
    if line.startswith("DNS_NAMES="):
        out.append("DNS_NAMES=" + names)
        seen = True
    else:
        out.append(line)
if not seen:
    out.append("DNS_NAMES=" + names)
open(path, "w").write("\n".join(out) + "\n")
PY

echo "== stop bootstrap proxy =="
"$MITM/scripts/proxy-ctl.sh" proxy-stop || true
echo "== restart dns with the full list =="
"$MITM/scripts/proxy-ctl.sh" dns-stop || true
"$MITM/scripts/proxy-ctl.sh" dns-start
echo "== start stack https-proxy =="
cd "$STACK"
docker compose up -d https-proxy
sleep 4
docker compose ps https-proxy

echo "== tls check =="
(openssl s_client -connect 127.0.0.1:443 -servername quasar.yandex.net -brief </dev/null 2>&1 & p=$!; sleep 6; kill $p 2>/dev/null) | head -8
