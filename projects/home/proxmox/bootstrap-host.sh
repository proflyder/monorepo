#!/usr/bin/env bash
# Run as root once on a fresh PVE 9 host. Leaves Debian sources untouched.
set -euo pipefail
[[ $(id -u) == 0 ]] && command -v pveversion >/dev/null
[[ $(dpkg-query -W -f='${Version}' pve-manager) == 9.* ]]
BACKUP=/var/backups/proxmox-bootstrap
mkdir -p "$BACKUP"
chmod 700 "$BACKUP"
if [[ ! -e "$BACKUP/apt-sources.tar" ]]; then
  tar -C /etc/apt -cpf "$BACKUP/apt-sources.tar" sources.list.d
fi
# Single-node lab uses LVM-thin, not Ceph. Subscription-only feeds return 401.
for source in /etc/apt/sources.list.d/pve-enterprise.sources /etc/apt/sources.list.d/ceph.sources; do
  if [[ -f "$source" ]]; then
    sed -i '/^Enabled:/d; /^$/d' "$source"
    printf 'Enabled: no\n' >> "$source"
  fi
done
cat > /etc/apt/sources.list.d/proxmox.sources <<'SOURCES'
Types: deb
URIs: http://download.proxmox.com/debian/pve
Suites: trixie
Components: pve-no-subscription
Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg
SOURCES
apt-get update
apt-get -s full-upgrade
