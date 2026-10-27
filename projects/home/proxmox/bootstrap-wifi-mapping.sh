#!/usr/bin/env bash
# Preparation only: creates an isolated PCI mapping for the RTL8821CE.
# It does NOT attach the card to a VM and does NOT change networking.
# Do not run until a routed/NAT or external Wi-Fi bridge design exists.
# Hardware bootstrap; never relax IOMMU isolation or use unsafe interrupts.
set -euo pipefail
[[ $(id -u) == 0 ]]
DEVICE=/sys/bus/pci/devices/0000:04:00.0
[[ $(cat "$DEVICE/vendor") == 0x10ec && $(cat "$DEVICE/device") == 0xc821 ]]
[[ -L "$DEVICE/iommu_group" ]]
[[ $(find "$DEVICE/iommu_group/devices/" -mindepth 1 -maxdepth 1 | wc -l) == 1 ]]
GROUP=$(basename "$(readlink -f "$DEVICE/iommu_group")")
if ! pvesh get /cluster/mapping/pci/home-wifi --output-format json >/dev/null 2>&1; then
  pvesh create /cluster/mapping/pci --id home-wifi \
    --map "node=pve,path=0000:04:00.0,id=10ec:c821,iommugroup=$GROUP" \
    --description 'Dedicated RTL8821CE for the Wi-Fi/K3s infrastructure VM'
fi
pveum acl modify /mapping/pci/home-wifi --users terraform@pve --roles PVEMappingUser
modprobe vfio_pci
printf 'Isolated Wi-Fi PCI mapping ready; physical Ethernet unchanged\n'
