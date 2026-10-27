#!/usr/bin/env bash
# Run on PVE as root with versions.sh alongside this script.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/versions.sh"
mkdir -p /var/lib/vz/import
cd /var/lib/vz/import
haos="haos_ova-${HAOS_VERSION}.qcow2"
if [[ ! -f "$haos.xz" ]]; then
  curl -fL --retry 3 "https://github.com/home-assistant/operating-system/releases/download/${HAOS_VERSION}/$haos.xz" -o "$haos.xz.partial"
  mv "$haos.xz.partial" "$haos.xz"
fi
printf '%s  %s\n' "$HAOS_COMPRESSED_SHA256" "$haos.xz" | sha256sum -c -
[[ -f "$haos" ]] || xz -dk "$haos.xz"
ubuntu="${UBUNTU_RELEASE}-server-cloudimg-amd64.qcow2"
if [[ ! -f "$ubuntu" ]]; then
  curl -fL --retry 3 "https://cloud-images.ubuntu.com/${UBUNTU_RELEASE}/current/${UBUNTU_RELEASE}-server-cloudimg-amd64.img" -o "$ubuntu.partial"
  mv "$ubuntu.partial" "$ubuntu"
fi
# Upstream current moves. A mismatch requires deliberate manifest review,
# never silently accept a different image for a supposedly identical deployment.
printf '%s  %s\n' "$UBUNTU_IMAGE_SHA256" "$ubuntu" | sha256sum -c -
qemu-img check "$haos"
qemu-img check "$ubuntu"
pvesm set local --content iso,vztmpl,backup,import,snippets
