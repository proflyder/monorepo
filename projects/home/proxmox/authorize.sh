#!/usr/bin/env bash
# Run interactively on the operator's computer after a fresh PVE installation.
set -euo pipefail
HOST="${1:?Specify the installed Proxmox IPv4 address}"
KEY="${2:?Specify the dedicated private SSH key file}"
KNOWN_HOSTS="${3:?Specify a dedicated known_hosts file}"
[[ -f "$KEY" && -f "$KEY.pub" ]] || { echo 'SSH key pair not found' >&2; exit 1; }
echo 'On the Proxmox console, run: ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub'
echo 'Compare that fingerprint with the SSH prompt before accepting it.'
echo 'Enter the new root password only in this local terminal; do not send it in chat.'
ssh-copy-id -i "$KEY.pub" -o "UserKnownHostsFile=$KNOWN_HOSTS" -o StrictHostKeyChecking=ask "root@$HOST"
ssh -i "$KEY" -o "UserKnownHostsFile=$KNOWN_HOSTS" -o StrictHostKeyChecking=yes "root@$HOST" \
  'pveversion; ip -br addr show vmbr0; pvesm status'
