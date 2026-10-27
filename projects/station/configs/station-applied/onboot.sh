#!/bin/sh

# "start" is passed by init.d during boot-up run level
if [ "$1" = "start" ]; then
    /data/quasar/yandex-block.sh >/dev/null 2>&1
    mkdir -p /bkupgrade/data/quasar

    # quasar processes allocates a lot of virtual memory
    # for anonymous and file mappings along with performing
    # a lot of "system" functions calls.
    #
    # "system" calls "fork" which may fail with ENOMEM when
    # heuristic overcommit policy is used. Hence, always allow overcommit.
    echo 1  > /proc/sys/vm/overcommit_memory

    # On every boot remove old OTA artifacts just in case
    rm -f /bkupgrade/update_*
    rm -f /bkupgrade/ota/*

    export LD_LIBRARY_PATH=/system/vendor/quasar:/system/vendor/quasar/libs:$LD_LIBRARY_PATH
    export PATH=/system/vendor/quasar:$PATH
    export GST_PLUGIN_PATH=/system/vendor/quasar/libs

    if [ ! -d "/bkupgrade/data/quasar" ]; then
        mkdir -p /bkupgrade/data/quasar
    fi

    if [ ! -f /data/quasar/initialized ]; then
        echo "First run. Initialization..."

        echo "Copying config..."
        mkdir -m 777 -p /data/quasar
        mkdir -m 777 -p /data/quasar/logs
        mkdir -m 777 -p /data/quasar/daemons_logs
        mkdir -m 777 -p /data/quasar/data
    fi

    chmod 777 /data/quasar
    chmod 777 /data/quasar/logs

    /system/vendor/quasar/yio_iptables_init.sh

    # FORTRESS-EGRESS-GUARD: deny all WAN except LAN/DHCP/NTP.
    # Must run after yio_iptables_init.sh, before Quasar starts.
    /data/quasar/egress-guard.sh >/dev/null 2>&1

    # DPP/Wi-Fi Easy Connect: stable QR bootstrap, listen only without network.
    [ -x /data/quasar/dpp-provision.sh ] && /data/quasar/dpp-provision.sh >/dev/null 2>&1 &

    /system/vendor/quasar/set_monotonic_time.sh

    # Activate ADB if one of marker files exists
    /system/vendor/quasar/activate_adb.sh check

    route delete default gateway 10.10.10.254 wlan1
    cd /system/vendor/quasar

    # remove dangling fluent-bit directory from older versions
    rm -rf /data/quasar/fluent-bit

    # remove old adb markers remnants
    rm -f /data/quasar_activate_adb_sound
    rm -f /data/quasar_activate_adb_quasmodrom
    rm -f /data/quasar_activate_adb_glagol
    rm -f /data/quasar_activate_network_adb

    /system/vendor/quasar/q save-last-monotonic-time

    /system/vendor/quasar/run.sh

    touch /data/quasar/initialized
    rm /bkupgrade/uImage*
elif [ "$1" = "stop" ]; then
    pkill quasar_launcher2
fi
[ -x /data/quasar/rootkeeper.sh ] && /data/quasar/rootkeeper.sh >/dev/null 2>&1 &
