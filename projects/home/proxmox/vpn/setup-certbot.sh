#!/usr/bin/env bash
# Configure automatic Let's Encrypt renewal via the Cloudflare DNS-01 plugin.
#
# Run as root inside the VPN container (CT 102), like install-ocserv.sh. The
# script is idempotent: it installs the plugin, migrates the existing lineage
# from manual DNS-01 to dns-cloudflare if needed, installs the ocserv deploy
# hook, verifies the result with `certbot renew --dry-run` and enables the
# certbot timer.
#
# The Cloudflare API token never lives in this repository. Pass the path to a
# credentials file (mode 600, root) prepared inside the CT, for example:
#
#   dns_cloudflare_api_token = <token>
#
# The hook below is embedded so this script can be pushed as a single file.
set -euo pipefail
umask 077

DOMAIN=vpn.example.com
CREDENTIALS=
PROPAGATION=30
DEPLOY_DIR=/etc/letsencrypt/renewal-hooks/deploy
DEPLOY_HOOK=$DEPLOY_DIR/ocserv-ssl.sh
SKIP_DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: setup-certbot.sh --domain NAME --credentials PATH [options]

Required:
  --domain NAME          public certificate name, e.g. vpn.example.com
  --credentials PATH     Cloudflare credentials file (root, mode 600)

Options:
  --propagation-seconds N  DNS propagation wait (default 30)
  --skip-dry-run         do not run `certbot renew --dry-run` at the end
EOF
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --domain) DOMAIN=$2; shift 2 ;;
    --credentials) CREDENTIALS=$2; shift 2 ;;
    --propagation-seconds) PROPAGATION=$2; shift 2 ;;
    --skip-dry-run) SKIP_DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run inside the container as root"
[[ -n $DOMAIN && -n $CREDENTIALS ]] || {
  usage >&2
  die "--domain and --credentials are required"
}
[[ -f $CREDENTIALS ]] || die "credentials file not found: $CREDENTIALS"
[[ $(stat -c '%a' "$CREDENTIALS") == 600 ]] ||
  die "credentials file must have mode 600: $CREDENTIALS"
grep -q '^dns_cloudflare_api_token[[:space:]]*=' "$CREDENTIALS" ||
  die "credentials file has no dns_cloudflare_api_token entry"

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  certbot python3-certbot-dns-cloudflare openssl

# ----- deploy hook ---------------------------------------------------------
# Copy the renewed certificate into ocserv, keeping the self-signed pair as a
# fallback. The private key is validated against the certificate before it is
# installed so a broken renewal never takes the VPN down.
install -d -m 755 "$DEPLOY_DIR"
cat >"$DEPLOY_HOOK" <<'HOOK'
#!/usr/bin/env bash
# Managed by projects/home/proxmox/vpn/setup-certbot.sh. Do not edit by hand.
# certbot deploy hook: install the renewed certificate into ocserv.
set -euo pipefail
umask 077

d=/etc/ocserv/ssl
lineage=${RENEWED_LINEAGE:-}
[[ -n $lineage ]] || { echo "RENEWED_LINEAGE is not set" >&2; exit 1; }
[[ -s $lineage/fullchain.pem && -s $lineage/privkey.pem ]] ||
  { echo "missing certificate files in $lineage" >&2; exit 1; }

cert_pub=$(openssl x509 -in "$lineage/fullchain.pem" -pubkey -noout \
  | openssl pkey -pubin -outform DER | openssl dgst -sha256)
key_pub=$(openssl pkey -in "$lineage/privkey.pem" -pubout -outform DER \
  | openssl dgst -sha256)
[[ $cert_pub == "$key_pub" ]] ||
  { echo "certificate and private key do not match" >&2; exit 1; }

# Keep the first self-signed pair as a rollback fallback.
[[ -e $d/selfsigned-cert.pem ]] || cp -p "$d/server-cert.pem" "$d/selfsigned-cert.pem"
[[ -e $d/selfsigned-key.pem ]] || cp -p "$d/server-key.pem" "$d/selfsigned-key.pem"

install -m 644 "$lineage/fullchain.pem" "$d/server-cert.pem"
install -m 600 "$lineage/privkey.pem" "$d/server-key.pem"
ocserv --test-config
systemctl restart ocserv
HOOK
chmod 700 "$DEPLOY_HOOK"

# ----- certificate lineage -------------------------------------------------
live=/etc/letsencrypt/live/$DOMAIN
if [[ -d $live ]]; then
  # Migrate the existing lineage from manual DNS-01 to dns-cloudflare without
  # reissuing. Keep every unrelated option untouched.
  renewal=/etc/letsencrypt/renewal/$DOMAIN.conf
  [[ -f $renewal ]] || die "renewal config not found: $renewal"
  cp -p "$renewal" "$renewal.bak.$(date +%Y%m%d%H%M%S)"
  python3 - "$renewal" "$CREDENTIALS" "$PROPAGATION" <<'PY'
import sys

path, credentials, propagation = sys.argv[1:4]
with open(path, encoding="utf-8") as fh:
    lines = fh.read().splitlines()

out = []
section = None
seen = {"authenticator": False, "credentials": False, "propagation": False}
for line in lines:
    stripped = line.strip()
    if stripped.startswith("[") and stripped.endswith("]"):
        section = stripped[1:-1]
    if section == "renewalparams":
        key = stripped.split("=", 1)[0].strip()
        if key in ("manual_auth_hook", "manual_public_ip_logging_ok", "renew_hook"):
            continue
        if key == "authenticator":
            out.append("authenticator = dns-cloudflare")
            seen["authenticator"] = True
            continue
        if key == "dns_cloudflare_credentials":
            out.append(f"dns_cloudflare_credentials = {credentials}")
            seen["credentials"] = True
            continue
        if key == "dns_cloudflare_propagation_seconds":
            out.append(f"dns_cloudflare_propagation_seconds = {propagation}")
            seen["propagation"] = True
            continue
    out.append(line)

if not seen["authenticator"]:
    out.append("authenticator = dns-cloudflare")
if not seen["credentials"]:
    out.append(f"dns_cloudflare_credentials = {credentials}")
if not seen["propagation"]:
    out.append(f"dns_cloudflare_propagation_seconds = {propagation}")

with open(path, "w", encoding="utf-8") as fh:
    fh.write("\n".join(out) + "\n")
PY
else
  certbot certonly \
    --cert-name "$DOMAIN" \
    --dns-cloudflare \
    --dns-cloudflare-credentials "$CREDENTIALS" \
    --dns-cloudflare-propagation-seconds "$PROPAGATION" \
    --non-interactive --agree-tos --register-unsafely-without-email \
    -d "$DOMAIN"
fi

systemctl enable certbot.timer
systemctl restart certbot.timer

if ((!SKIP_DRY_RUN)); then
  certbot renew --cert-name "$DOMAIN" --dry-run --no-random-sleep-on-renew
fi

echo "certbot configured for $DOMAIN via dns-cloudflare"
