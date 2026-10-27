#!/usr/bin/env bash
# Dumps the Yandex Station Mini firmware and filesystem for offline study.
#
#   ./scripts/dump-station.sh
#
# This adbd build has a broken `exec-out` service, so every binary stream is
# wrapped in base64 on the device and decoded locally.
#
# Layout:
#   dump/rootfs/rootfs.tar.gz    /bin /etc /lib /opt /root /sbin /system /usr /var
#   dump/rootfs/bkupgrade.tar.gz /bkupgrade (data + CA backup)
#   dump/rootfs/tee.tar.gz       /tee
#   dump/mtd/mtdN.bin            raw NAND partitions, bad blocks padded with 0x00
#   dump/meta/                   procfs, dmesg, mtdinfo, mount, ps, ...
set -euo pipefail

ADB="${ADB:-${HOME}/Library/Android/sdk/platform-tools/adb}"
SERIAL="${SERIAL:-192.168.1.50:5555}"
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-${ROOT}/dump}"

mkdir -p "${OUT}"/{rootfs,mtd,meta}

fetch_text() {
  local command="$1" destination="$2"
  "${ADB}" shell "${command}" 2>/dev/null | tr -d '\r' > "${destination}"
}

fetch_b64() {
  local command="$1"
  "${ADB}" shell "${command} | base64" 2>/dev/null | tr -d '\r\n' | base64 -D
}

echo "== connect =="
"${ADB}" connect "${SERIAL}" >/dev/null
"${ADB}" wait-for-device
"${ADB}" shell id

echo "== meta =="
fetch_text 'cat /proc/mtd' "${OUT}/meta/proc_mtd.txt"
fetch_text 'cat /proc/partitions' "${OUT}/meta/proc_partitions.txt"
fetch_text 'cat /proc/cmdline' "${OUT}/meta/proc_cmdline.txt"
fetch_text 'cat /proc/cpuinfo' "${OUT}/meta/proc_cpuinfo.txt"
fetch_text 'cat /proc/meminfo' "${OUT}/meta/proc_meminfo.txt"
fetch_text 'cat /proc/version' "${OUT}/meta/proc_version.txt"
fetch_text 'dmesg' "${OUT}/meta/dmesg.txt" || true
fetch_text 'mount' "${OUT}/meta/mount.txt"
fetch_text 'df -h' "${OUT}/meta/df.txt"
fetch_text 'ps' "${OUT}/meta/ps.txt" || true
fetch_text 'lsmod' "${OUT}/meta/lsmod.txt" || true
fetch_text 'ip addr' "${OUT}/meta/ip_addr.txt" || true
fetch_text 'ip route' "${OUT}/meta/ip_route.txt" || true
fetch_text '/usr/sbin/mtdinfo -a' "${OUT}/meta/mtdinfo.txt" || true
fetch_text 'find / -xdev -print' "${OUT}/meta/find_root.txt" || true
if "${ADB}" shell 'test -f /proc/config.gz' >/dev/null 2>&1; then
  fetch_b64 'cat /proc/config.gz' > "${OUT}/meta/config.gz" || true
  if [[ -s "${OUT}/meta/config.gz" ]]; then
    gzip -dc "${OUT}/meta/config.gz" > "${OUT}/meta/kernel_config.txt" 2>/dev/null || true
  fi
fi

echo "== rootfs (/) =="
fetch_b64 'tar -C / -cf - bin etc lib lib32 opt root sbin system usr var media mnt' \
  | gzip -9 > "${OUT}/rootfs/rootfs.tar.gz"

echo "== /bkupgrade =="
fetch_b64 'tar -C /bkupgrade -cf - .' \
  | gzip -9 > "${OUT}/rootfs/bkupgrade.tar.gz"

echo "== /tee =="
fetch_b64 'tar -C /tee -cf - .' \
  | gzip -9 > "${OUT}/rootfs/tee.tar.gz"

echo "== raw MTD (mtdblock + conv=noerror,sync) =="
for index in 0 1 2 3 4 5 6; do
  echo "  mtd${index}"
  fetch_b64 "dd if=/dev/mtdblock${index} bs=2048 conv=noerror,sync 2>/dev/null" \
    > "${OUT}/mtd/mtd${index}.bin"
done

echo "== checksums =="
(
  cd "${OUT}"
  shasum -a 256 mtd/*.bin rootfs/*.tar.gz meta/* > SHA256SUMS
)
ls -lh "${OUT}/mtd" "${OUT}/rootfs"
echo "done: ${OUT}"
