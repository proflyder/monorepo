#!/usr/bin/env bash
# Roll back to the bootstrap proxy (quasar.yandex.net only).
set -euo pipefail

STACK="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
MITM="${MITM_DIR:-$HOME/Desktop/alice-mitm-bootstrap}"

echo "== stop stack https-proxy =="
cd "$STACK"
docker compose stop https-proxy || true

python3 - "$MITM/.env" <<'PY'
import sys
path = sys.argv[1]
lines = open(path).read().splitlines()
out = []
for line in lines:
    if line.startswith("DNS_NAMES="):
        out.append("DNS_NAMES=quasar.yandex.net")
    else:
        out.append(line)
open(path, "w").write("\n".join(out) + "\n")
PY

echo "== restart dns =="
"$MITM/scripts/proxy-ctl.sh" dns-stop || true
"$MITM/scripts/proxy-ctl.sh" dns-start
echo "== start bootstrap proxy =="
"$MITM/scripts/proxy-ctl.sh" proxy-start
"$MITM/scripts/proxy-ctl.sh" status
