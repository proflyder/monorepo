#!/usr/bin/env bash
# LEGACY pre-Proxmox check for Tailscale Serve on the old Ubuntu host.
# Current runtime uses Tailscale Operator and HAOS:
#   projects/home/kubernetes/tailscale/verify-tailnet.sh
# This file remains for the historical host-Serve rollback/audit only.
#
# Run on old lab-host:
#   ssh labuser@192.0.2.25 'bash -s' < projects/home-assistant/scripts/verify-remote-access.sh
#
# It does not change anything, does not start/stop Tailscale, and does not
# print secrets. Exit code 1 means at least one FAIL.
set -uo pipefail

HA_CONFIG="${HA_CONFIG:-/home/labuser/nettop/homeassistant/config}"
HA_URL="${HA_URL:-http://127.0.0.1:8123}"

FAIL=0
WARN=0

pass() { printf 'PASS  %s\n' "$*"; }
warn() { printf 'WARN  %s\n' "$*"; WARN=$((WARN + 1)); }
fail() { printf 'FAIL  %s\n' "$*"; FAIL=$((FAIL + 1)); }
have() { command -v "$1" >/dev/null 2>&1; }

# tailscale/ss need root on this host; use passwordless sudo when available.
TS=(tailscale)
SS=(ss)
if [[ "$(id -u)" -ne 0 ]] && sudo -n true 2>/dev/null; then
  TS=(sudo -n tailscale)
  SS=(sudo -n ss)
fi

printf '== remote access check: %s ==\n' "$(hostname 2>/dev/null || echo unknown)"

# --- HA local -------------------------------------------------------------
if have curl; then
  code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "$HA_URL/" 2>/dev/null || true)"
  if [[ "$code" == "200" ]]; then
    pass "HA local: HTTP 200 ($HA_URL)"
  else
    fail "HA local: got '$code' from $HA_URL"
  fi
else
  fail "curl not found"
fi

# --- HA UI settings -------------------------------------------------------
if ! have jq; then
  warn "jq not found: HA JSON checks skipped"
elif ! sudo -n true 2>/dev/null; then
  warn "sudo -n not available: cannot read HA .storage; run as labuser with NOPASSWD"
else
  http_json="$(sudo -n cat "$HA_CONFIG/.storage/http" 2>/dev/null || true)"
  if [[ -z "$http_json" ]]; then
    warn "cannot read $HA_CONFIG/.storage/http"
  else
    if jq -e '.data.stable.use_x_forwarded_for == true' >/dev/null 2>&1 <<<"$http_json"; then
      pass "HTTP: Trust X-Forwarded-For is on"
    else
      fail "HTTP: Trust X-Forwarded-For is off (Tailscale Serve will look like 127.0.0.1)"
    fi
    if jq -e '(.data.stable.trusted_proxies // []) | any(. == "127.0.0.1" or . == "127.0.0.1/32")' >/dev/null 2>&1 <<<"$http_json"; then
      pass "HTTP: trusted proxy 127.0.0.1 present"
    else
      fail "HTTP: trusted proxy 127.0.0.1 missing"
    fi
    if jq -e '(.data.stable.login_attempts_threshold // -1) > 0' >/dev/null 2>&1 <<<"$http_json"; then
      pass "HTTP: login attempts threshold is set"
    else
      fail "HTTP: login attempts threshold is unlimited (-1)"
    fi
    if jq -e '.data.stable.ip_ban_enabled == true' >/dev/null 2>&1 <<<"$http_json"; then
      pass "HTTP: IP banning is on"
    else
      warn "HTTP: IP banning is off"
    fi
  fi

  core_json="$(sudo -n cat "$HA_CONFIG/.storage/core.config" 2>/dev/null || true)"
  if [[ -z "$core_json" ]]; then
    warn "cannot read $HA_CONFIG/.storage/core.config"
  else
    internal="$(jq -r '.data.internal_url // empty' <<<"$core_json" 2>/dev/null)"
    external="$(jq -r '.data.external_url // empty' <<<"$core_json" 2>/dev/null)"
    if [[ "$internal" =~ ^https?:// ]]; then
      pass "HA URL: internal_url is set"
    else
      warn "HA URL: internal_url is empty"
    fi
    if [[ "$external" =~ ^https:// ]]; then
      pass "HA URL: external_url is HTTPS"
    else
      warn "HA URL: external_url is empty or not HTTPS"
    fi
  fi

  if [[ -f "$HA_CONFIG/ip_bans.yaml" ]]; then
    warn "HA: ip_bans.yaml exists (clear it if 127.0.0.1 is banned)"
  else
    pass "HA: no ip_bans.yaml"
  fi

  if sudo -n test -s "$HA_CONFIG/.storage/auth_module.totp" 2>/dev/null; then
    pass "HA: TOTP MFA module configured"
  else
    fail "HA: TOTP MFA module not configured"
  fi
fi

# --- Tailscale ------------------------------------------------------------
if ! have tailscale; then
  fail "Tailscale: client not installed"
else
  if "${TS[@]}" status >/dev/null 2>&1; then
    pass "Tailscale: status ok"
  else
    fail "Tailscale: not running or not authenticated"
  fi

  if systemctl is-enabled --quiet tailscaled 2>/dev/null; then
    pass "Tailscale: tailscaled enabled at boot"
  else
    warn "Tailscale: tailscaled is not enabled at boot"
  fi

  serve="$("${TS[@]}" serve status 2>/dev/null || true)"
  if grep -qE 'https://[^[:space:]]+' <<<"$serve" && grep -q '127\.0\.0\.1:8123' <<<"$serve"; then
    pass "Tailscale Serve: HTTPS -> 127.0.0.1:8123"
  elif [[ -z "$serve" ]]; then
    fail "Tailscale Serve: not configured"
  else
    fail "Tailscale Serve: expected HTTPS and 127.0.0.1:8123 in status"
  fi

  if have jq; then
    serve_json="$("${TS[@]}" serve status --json 2>/dev/null || true)"
    if [[ -z "$serve_json" ]]; then
      warn "Tailscale Funnel: status unavailable"
    elif jq -e '.AllowFunnel == null or (.AllowFunnel | length == 0)' >/dev/null 2>&1 <<<"$serve_json"; then
      pass "Tailscale Funnel: off"
    else
      fail "Tailscale Funnel: enabled (public exposure; disable it)"
    fi

    ts_json="$("${TS[@]}" status --json 2>/dev/null || true)"
    dns="$(jq -r '.Self.DNSName // empty' <<<"$ts_json" 2>/dev/null | sed 's/\.$//')"
    expiry="$(jq -r '.Self.KeyExpiry // empty' <<<"$ts_json" 2>/dev/null)"
    if [[ -n "$dns" ]]; then
      pass "Tailscale: MagicDNS name $dns"
      ip4="$("${TS[@]}" ip -4 2>/dev/null | head -1 || true)"
      if have curl && [[ -n "$ip4" ]]; then
        rcode="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 10 \
          --resolve "$dns:443:$ip4" "https://$dns/" 2>/dev/null || true)"
        if [[ "$rcode" =~ ^(200|301|302)$ ]]; then
          pass "Tailscale: remote HTTPS $rcode via $dns"
        else
          fail "Tailscale: remote HTTPS got '$rcode' via $dns"
        fi
      else
        warn "Tailscale: remote HTTPS check skipped (no curl or no IPv4)"
      fi
    else
      warn "Tailscale: no MagicDNS name (DNS/HTTPS disabled?)"
    fi
    if [[ -z "$expiry" || "$expiry" == 0001-01-01T00:00:00Z ]]; then
      pass "Tailscale: key expiry disabled"
    else
      warn "Tailscale: key expiry is $expiry (disable it for the server)"
    fi
  fi
fi

# --- NTP (TOTP depends on it) --------------------------------------------
if have timedatectl; then
  ntp="$(timedatectl show -p NTPSynchronized --value 2>/dev/null || true)"
  if [[ "$ntp" == "yes" ]]; then
    pass "NTP: clock synchronized"
  else
    warn "NTP: clock not synchronized (TOTP may fail)"
  fi
fi

# --- listeners ------------------------------------------------------------
if have ss; then
  if "${SS[@]}" -lntp 2>/dev/null | grep -qE '[:.]8123[[:space:]]'; then
    pass "Port: 8123 is listening"
  else
    fail "Port: 8123 is not listening"
  fi
  if "${SS[@]}" -lntp 2>/dev/null | grep -qE '[:.]443[[:space:]]'; then
    pass "Port: 443 is listening"
  else
    warn "Port: 443 is not listening (Serve not set up yet?)"
  fi
else
  warn "ss not found: port checks skipped"
fi

printf '\nSUMMARY: %d FAIL, %d WARN\n' "$FAIL" "$WARN"
if ((FAIL > 0)); then
  exit 1
fi
exit 0
