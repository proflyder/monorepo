#!/bin/sh
# FORTRESS-YANDEX-BLOCK v1
# Blocks real Yandex at the packet level and pins DNS to the local Mac.
# Lives in /data (rw) and is re-applied by rootkeeper every 60s.

LOG=/data/quasar/yandex-block.log
log() { echo "$(date): $*" >> "$LOG"; }

# 1) IPv4 Yandex netblocks (quasar/uniproxy/music/passport/NTP live here)
for net in \
  5.255.0.0/16 \
  37.9.0.0/16 \
  77.88.0.0/16 \
  87.250.0.0/16 \
  93.158.0.0/16 \
  95.108.0.0/16 \
  213.180.0.0/16 \
  178.248.232.0/21 \
  185.32.187.0/24 \
  141.8.128.0/18
do
  while iptables -D OUTPUT -d "$net" -j REJECT 2>/dev/null; do :; done
  iptables -I OUTPUT 1 -d "$net" -j REJECT 2>/dev/null
  while iptables -D INPUT -s "$net" -j DROP 2>/dev/null; do :; done
  iptables -I INPUT 1 -s "$net" -j DROP 2>/dev/null
done

# 2) Yandex IPv6
while ip6tables -D OUTPUT -d 2a02:6b8::/32 -j REJECT 2>/dev/null; do :; done
ip6tables -I OUTPUT 1 -d 2a02:6b8::/32 -j REJECT 2>/dev/null
while ip6tables -D INPUT -s 2a02:6b8::/32 -j DROP 2>/dev/null; do :; done
ip6tables -I INPUT 1 -s 2a02:6b8::/32 -j DROP 2>/dev/null

# 3) unbound: only the Mac resolver, no Yandex DNS
if ! grep -q 'forward-addr: "192.0.2.11"' /etc/unbound/unbound.conf 2>/dev/null; then
  mount -o remount,rw / 2>/dev/null
  cat > /etc/unbound/unbound.conf <<'EOC'
server:
	msg-cache-size: 2m
	rrset-cache-size: 2m
	username: "nobody"
forward-zone:
	name: "."
	forward-addr: "192.0.2.11"
EOC
  sync
  mount -o remount,ro / 2>/dev/null
  /etc/init.d/S70unbound restart >/dev/null 2>&1
  log "unbound -> 192.0.2.11"
fi

# 4) /etc/hosts: exact Yandex names -> local Mac (authoritative for libc)
if ! grep -q 'YANDEX-BLOCK' /etc/hosts 2>/dev/null; then
  cp /etc/hosts /data/opt/backup/hosts.before-yandex-block 2>/dev/null
  mount -o remount,rw / 2>/dev/null
  cat >> /etc/hosts <<'EOC'
# YANDEX-BLOCK
192.0.2.11 quasar.yandex.ru quasar.yandex.net iot.quasar.yandex.net scbh.yandex.net uniproxy.alice.yandex.net uniproxy-quic.alice.yandex.net rpc.alice.yandex.net alice.yandex.ru alice.yandex.net alice-ats.yandex.net alice-ats.yandex.ru music.yandex.ru music.yandex.net api.music.yandex.net ws.api.music.yandex.net passport.yandex.ru passport.yandex.net oauth.yandex.ru login.yandex.ru mobile.yandex.net storage.mds.yandex.net avatars.mds.yandex.net strm.yandex.ru strm.yandex.net push.yandex.ru clck.yandex.net report.appmetrica.yandex.net startup.mobile.yandex.net mobileproxy.passport.yandex.net yandex.ru yandex.net yandex.com ya.ru
EOC
  sync
  mount -o remount,ro / 2>/dev/null
  log "hosts block added"
fi

# 5) chrony: no Yandex NTP; use Google IPs (no DNS dependency)
if grep -q 'ntp.yantp.net' /etc/chrony.conf 2>/dev/null; then
  mount -o remount,rw / 2>/dev/null
  cat > /etc/chrony.conf <<'EOC'
# Google NTP instead of Yandex ntp.yantp.net
server 216.239.35.0 iburst
server 216.239.35.4 iburst
server 216.239.35.8 iburst
server 216.239.35.12 iburst

driftfile /var/lib/chrony/drift
makestep 1.0 3
rtcsync
EOC
  sync
  mount -o remount,ro / 2>/dev/null
  /etc/init.d/S49chrony restart >/dev/null 2>&1
  log "chrony -> Google NTP"
fi

# 6) telemetry task removed from launch.json (persist across rootfs restores)
if [ -f /data/quasar/launch.json.notel ]; then
  if ! cmp -s /data/quasar/launch.json.notel /system/vendor/quasar/launch.json 2>/dev/null; then
    mount -o remount,rw / 2>/dev/null
    cp /data/quasar/launch.json.notel /system/vendor/quasar/launch.json
    sync
    mount -o remount,ro / 2>/dev/null
    log "launch.json telemetry removed"
  fi
fi

exit 0
