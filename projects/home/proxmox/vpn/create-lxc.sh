#!/usr/bin/env bash
# Create the unprivileged Debian LXC used by the home VPN (ocserv).
#
# Run as root on the Proxmox host. Addresses and storage are parameters; the
# defaults below are documentation examples only (RFC 5737), never live values.
set -euo pipefail

CTID=102
HOSTNAME=vpn
BRIDGE=vmbr0
IFACE=eth0
STORAGE=local-lvm
TEMPLATE_STORAGE=local
MEMORY=512
SWAP=512
CORES=1
DISK=4
PREFIX=24
IP=
GATEWAY=
DNS=
TEMPLATE=
SSH_PUBKEY=

usage() {
  cat <<'EOF'
Usage: create-lxc.sh --ip ADDRESS --gateway ADDRESS --dns ADDRESS [options]

Required (real values come from secrets/NETWORK.md):
  --ip ADDRESS          static IPv4 address for the container, e.g. 192.0.2.29
  --gateway ADDRESS     LAN gateway, e.g. 192.0.2.1
  --dns ADDRESS         DNS server, e.g. 192.0.2.1

Options:
  --ctid N              container ID (default 102)
  --hostname NAME       hostname (default vpn)
  --prefix N            IPv4 prefix length (default 24)
  --bridge NAME         Proxmox bridge (default vmbr0)
  --iface NAME          interface inside the container (default eth0)
  --storage NAME        rootfs storage (default local-lvm)
  --template-storage N  storage for the Debian template (default local)
  --template NAME       template file name, auto-detected when empty
  --memory MiB          RAM (default 512)
  --swap MiB            swap (default 512)
  --cores N             vCPUs (default 1)
  --disk GiB            disk size (default 4)
  --ssh-pubkey FILE     public key installed for root
EOF
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --ctid) CTID=$2; shift 2 ;;
    --hostname) HOSTNAME=$2; shift 2 ;;
    --ip) IP=$2; shift 2 ;;
    --prefix) PREFIX=$2; shift 2 ;;
    --gateway) GATEWAY=$2; shift 2 ;;
    --dns) DNS=$2; shift 2 ;;
    --bridge) BRIDGE=$2; shift 2 ;;
    --iface) IFACE=$2; shift 2 ;;
    --storage) STORAGE=$2; shift 2 ;;
    --template-storage) TEMPLATE_STORAGE=$2; shift 2 ;;
    --template) TEMPLATE=$2; shift 2 ;;
    --memory) MEMORY=$2; shift 2 ;;
    --swap) SWAP=$2; shift 2 ;;
    --cores) CORES=$2; shift 2 ;;
    --disk) DISK=$2; shift 2 ;;
    --ssh-pubkey) SSH_PUBKEY=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run on the Proxmox host as root"
[[ -n $IP && -n $GATEWAY && -n $DNS ]] || { usage >&2; die "--ip, --gateway and --dns are required"; }
[[ $CTID =~ ^[0-9]+$ ]] || die "invalid CTID"
[[ $PREFIX =~ ^[0-9]+$ ]] || die "invalid prefix"
[[ -n $SSH_PUBKEY ]] && [[ -r $SSH_PUBKEY ]] || [[ -z $SSH_PUBKEY ]] || die "cannot read SSH key"

# Refuse to touch an existing guest.
if pct status "$CTID" >/dev/null 2>&1; then
  die "CT $CTID already exists"
fi

# An address that answers ping or has a live neighbour entry is not free.
if ping -c1 -W1 "$IP" >/dev/null 2>&1; then
  die "$IP is already answering ping"
fi
if ip neigh show "$IP" 2>/dev/null | grep -qE 'REACHABLE|DELAY|PROBE'; then
  die "$IP has a live ARP neighbour"
fi
if command -v arping >/dev/null 2>&1; then
  if arping -c1 -w2 -I "$BRIDGE" "$IP" >/dev/null 2>&1; then
    die "$IP answered ARP on $BRIDGE"
  fi
fi

# Locate or download a Debian 13 amd64 template.
if [[ -z $TEMPLATE ]]; then
  pveam update >/dev/null
  TEMPLATE=$(pveam available --section system \
    | awk '$2 ~ /^debian-13-standard_/ && $2 ~ /_amd64\.tar\./ {print $2}' \
    | sort -V | tail -1)
  [[ -n $TEMPLATE ]] || die "no Debian 13 amd64 template available"
fi
if ! pveam list "$TEMPLATE_STORAGE" | awk '{print $1}' | grep -qx "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE"; then
  pveam download "$TEMPLATE_STORAGE" "$TEMPLATE"
fi

pct create "$CTID" "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE" \
  --hostname "$HOSTNAME" \
  --cores "$CORES" \
  --memory "$MEMORY" \
  --swap "$SWAP" \
  --rootfs "$STORAGE:$DISK" \
  --net0 "name=$IFACE,bridge=$BRIDGE,ip=$IP/$PREFIX,gw=$GATEWAY" \
  --nameserver "$DNS" \
  --onboot 1 \
  --unprivileged 1 \
  --features nesting=1 \
  --ostype debian \
  --description "Home VPN (ocserv), managed from projects/home/proxmox/vpn"

# /dev/net/tun for ocserv. PVE nesting usually adds it; keep this idempotent.
CONF=/etc/pve/lxc/$CTID.conf
if ! grep -q '^lxc.mount.entry: /dev/net/tun ' "$CONF"; then
  {
    echo 'lxc.cgroup2.devices.allow: c 10:200 rwm'
    echo 'lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file'
  } >>"$CONF"
fi

pct start "$CTID"
for _ in $(seq 1 60); do
  if pct exec "$CTID" -- true >/dev/null 2>&1; then break; fi
  sleep 1
done
pct exec "$CTID" -- true >/dev/null 2>&1 || die "CT $CTID did not start"

if [[ -n $SSH_PUBKEY ]]; then
  pct exec "$CTID" -- install -d -m 700 /root/.ssh
  pct push "$CTID" "$SSH_PUBKEY" /root/.ssh/authorized_keys --perms 600
fi

echo "CT $CTID ($HOSTNAME) created: $IP/$PREFIX gw $GATEWAY, onboot=1"
echo "Next: push install-ocserv.sh into the CT and run it there."
