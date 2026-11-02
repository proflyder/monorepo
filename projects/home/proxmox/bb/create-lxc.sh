#!/usr/bin/env bash
# Create the unprivileged Debian LXC used by the bb server (CT 103).
#
# Run as root on the Proxmox host. Addresses and storage are parameters; the
# examples in --help are documentation values (RFC 5737), never live values.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../versions.sh
# shellcheck disable=SC1091
source "$SCRIPT_DIR/../versions.sh"

CTID=103
HOSTNAME=bb
BRIDGE=vmbr0
IFACE=eth0
STORAGE=local-lvm
TEMPLATE_STORAGE=local
MEMORY=2048
SWAP=1024
CORES=2
DISK=16
PREFIX=24
TIMEZONE=Asia/Almaty
STARTUP=order=3,up=30,down=60
IP=
GATEWAY=
DNS=
MAC=${BB_HWADDR:-}
TEMPLATE=$DEBIAN_LXC_TEMPLATE
SSH_PUBKEY=
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: create-lxc.sh --ip ADDRESS --gateway ADDRESS --dns ADDRESS --mac MAC [options]

Required (real values come from secrets/NETWORK.md):
  --ip ADDRESS          static IPv4 address for the container, e.g. 192.0.2.30
  --gateway ADDRESS     LAN gateway, e.g. 192.0.2.1
  --dns ADDRESS         DNS server, e.g. 192.0.2.1
  --mac MAC             fixed interface MAC for the router DHCP reservation,
                        e.g. 02:00:00:00:00:30; BB_HWADDR is also accepted

Options:
  --ctid N              container ID (default 103)
  --hostname NAME       hostname (default bb)
  --prefix N            IPv4 prefix length (default 24)
  --bridge NAME         Proxmox bridge (default vmbr0)
  --iface NAME          interface inside the container (default eth0)
  --storage NAME        rootfs storage (default local-lvm)
  --template-storage N  storage for the Debian template (default local)
  --template NAME       template file name (default from versions.sh)
  --memory MiB          RAM (default 2048)
  --swap MiB            swap (default 1024)
  --cores N             vCPUs (default 2)
  --disk GiB            disk size (default 16)
  --timezone NAME       container timezone (default Asia/Almaty)
  --startup SPEC        PVE startup policy (default order=3,up=30,down=60)
  --ssh-pubkey FILE     public key installed for root
  --dry-run             validate and print the pct command, create nothing
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
    --mac) MAC=$2; shift 2 ;;
    --bridge) BRIDGE=$2; shift 2 ;;
    --iface) IFACE=$2; shift 2 ;;
    --storage) STORAGE=$2; shift 2 ;;
    --template-storage) TEMPLATE_STORAGE=$2; shift 2 ;;
    --template) TEMPLATE=$2; shift 2 ;;
    --memory) MEMORY=$2; shift 2 ;;
    --swap) SWAP=$2; shift 2 ;;
    --cores) CORES=$2; shift 2 ;;
    --disk) DISK=$2; shift 2 ;;
    --timezone) TIMEZONE=$2; shift 2 ;;
    --startup) STARTUP=$2; shift 2 ;;
    --ssh-pubkey) SSH_PUBKEY=$2; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ -n $IP && -n $GATEWAY && -n $DNS && -n $MAC ]] || {
  usage >&2
  die "--ip, --gateway, --dns and --mac are required"
}
[[ $CTID =~ ^[0-9]+$ ]] || die "invalid CTID"
if ! [[ $PREFIX =~ ^[0-9]+$ ]] || ((PREFIX < 1 || PREFIX > 32)); then
  die "invalid prefix"
fi
if ! [[ $MEMORY =~ ^[0-9]+$ ]] || ((MEMORY < 1)); then
  die "invalid memory"
fi
[[ $SWAP =~ ^[0-9]+$ ]] || die "invalid swap"
if ! [[ $CORES =~ ^[0-9]+$ ]] || ((CORES < 1)); then
  die "invalid cores"
fi
if ! [[ $DISK =~ ^[0-9]+$ ]] || ((DISK < 1)); then
  die "invalid disk"
fi
[[ $IP =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || die "invalid IPv4 address: $IP"
[[ $GATEWAY =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] ||
  die "invalid gateway: $GATEWAY"
[[ $DNS =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]] || die "invalid DNS: $DNS"
[[ $MAC =~ ^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$ ]] ||
  die "invalid MAC address: $MAC"
[[ -n $STARTUP ]] || die "startup policy cannot be empty"
[[ -n $SSH_PUBKEY ]] && [[ -r $SSH_PUBKEY ]] || [[ -z $SSH_PUBKEY ]] ||
  die "cannot read SSH key"

pct_args=(
  create "$CTID" "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE"
  --hostname "$HOSTNAME"
  --cores "$CORES"
  --memory "$MEMORY"
  --swap "$SWAP"
  --rootfs "$STORAGE:$DISK"
  --net0 "name=$IFACE,bridge=$BRIDGE,ip=$IP/$PREFIX,gw=$GATEWAY,hwaddr=$MAC"
  --nameserver "$DNS"
  --onboot 1
  --startup "$STARTUP"
  --timezone "$TIMEZONE"
  --unprivileged 1
  --features nesting=1
  --ostype debian
  --description "bb server host, managed from projects/home/proxmox/bb"
)

if ((DRY_RUN)); then
  printf 'pct'
  printf ' %q' "${pct_args[@]}"
  printf '\n'
  exit 0
fi

[[ $(id -u) == 0 ]] || die "run on the Proxmox host as root"

# Refuse to touch an existing guest, but treat that as success for re-runs.
if pct status "$CTID" >/dev/null 2>&1; then
  echo "CT $CTID already exists; leaving it unchanged."
  exit 0
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

# Locate or download the pinned Debian 13 amd64 template.
if ! pveam list "$TEMPLATE_STORAGE" | awk '{print $1}' |
  grep -qx "$TEMPLATE_STORAGE:vztmpl/$TEMPLATE"; then
  pveam update >/dev/null
  pveam download "$TEMPLATE_STORAGE" "$TEMPLATE"
fi

pct "${pct_args[@]}"

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

echo "CT $CTID ($HOSTNAME) created: $IP/$PREFIX gw $GATEWAY, onboot=1, startup $STARTUP"
echo "Next: push install-bb.sh and versions.sh into the CT and run install-bb.sh there."
