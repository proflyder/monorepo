#!/bin/bash
# ==============================================================================
# Скрипт отката / прошивки дампов Яндекс Станции Мини 2 (Amlogic A113X)
# Поддерживает как откат через рабочий root ADB по сети, так и через Amlogic Burn Tool
# ==============================================================================

set -e

DEVICE_IP="${1:-192.168.0.154:5555}"
IMAGE_DIR="${2:-./images}"

echo "=== Яндекс Станция Мини 2: Мастер отката прошивки ==="
echo "Целевое устройство ADB: $DEVICE_IP"
echo "Каталог с дампами:      $IMAGE_DIR"
echo "------------------------------------------------------"

if [ ! -d "$IMAGE_DIR" ]; then
    echo "[-] Ошибка: каталог $IMAGE_DIR не найден."
    echo "    Создайте папку $IMAGE_DIR и поместите туда образы:"
    echo "    - boot.img       (ядро + initramfs, раздел mtd2)"
    echo "    - system.ubi     (UBI образ корневой системы, раздел mtd3)"
    echo "    - recovery.img   (рекавери, раздел mtd6)"
    exit 1
fi

echo "[1/4] Проверка связи со станцией через ADB..."
adb connect "$DEVICE_IP" || true
if ! adb -s "$DEVICE_IP" get-state 1>/dev/null 2>&1; then
    echo "[-] Устройство $DEVICE_IP недоступно по ADB!"
    echo "    Если станция окирпичена, используйте тестпоинт и Amlogic USB Burning Tool (см. README.md)."
    exit 1
fi
echo "[+] Станция на связи в режиме root!"

echo "[2/4] Отключение демонов Quasar для освобождения разделов..."
adb -s "$DEVICE_IP" shell "killall -9 maind quasar_launcher2 hardvold 2>/dev/null || true"
sleep 2

echo "[3/4] Прошивка разделов MTD / UBI..."

# 1. Прошивка Boot (mtd2)
if [ -f "$IMAGE_DIR/boot.img" ]; then
    echo " -> Загрузка и прошивка boot.img (mtd2)..."
    adb -s "$DEVICE_IP" push "$IMAGE_DIR/boot.img" /tmp/boot.img
    adb -s "$DEVICE_IP" shell "flash_erase /dev/mtd2 0 0 && nandwrite -p /dev/mtd2 /tmp/boot.img && rm /tmp/boot.img"
    echo " [+] Boot раздел успешно прошит!"
fi

# 2. Прошивка System UBI (mtd3)
if [ -f "$IMAGE_DIR/system.ubi" ]; then
    echo " -> Загрузка и прошивка system.ubi (mtd3)..."
    adb -s "$DEVICE_IP" push "$IMAGE_DIR/system.ubi" /tmp/system.ubi
    adb -s "$DEVICE_IP" shell "ubiformat /dev/mtd3 -f /tmp/system.ubi -y && rm /tmp/system.ubi"
    echo " [+] Корневой раздел System успешно прошит!"
elif [ -f "$IMAGE_DIR/system.img" ]; then
    echo " -> Загрузка и прошивка system.img (mtd3)..."
    adb -s "$DEVICE_IP" push "$IMAGE_DIR/system.img" /tmp/system.img
    adb -s "$DEVICE_IP" shell "flash_erase /dev/mtd3 0 0 && nandwrite -p /dev/mtd3 /tmp/system.img && rm /tmp/system.img"
    echo " [+] Раздел System успешно прошит!"
fi

# 3. Прошивка Recovery (mtd6)
if [ -f "$IMAGE_DIR/recovery.img" ]; then
    echo " -> Загрузка и прошивка recovery.img (mtd6)..."
    adb -s "$DEVICE_IP" push "$IMAGE_DIR/recovery.img" /tmp/recovery.img
    adb -s "$DEVICE_IP" shell "flash_erase /dev/mtd6 0 0 && nandwrite -p /dev/mtd6 /tmp/recovery.img && rm /tmp/recovery.img"
    echo " [+] Recovery успешно прошит!"
fi

echo "[4/4] Защита от автообновления и очистка пользовательских данных..."
adb -s "$DEVICE_IP" shell "rm -rf /data/quasar/data/updates/* /bkupgrade/* 2>/dev/null || true"

# Блокировка OTA серверов в hosts
adb -s "$DEVICE_IP" shell "
grep -q 'api.quasar.yandex.net' /etc/hosts || cat << 'EOF' >> /etc/hosts
127.0.0.1 api.quasar.yandex.net
127.0.0.1 quasar.s3.yandex.net
127.0.0.1 clck.yandex.net
EOF
"

echo "======================================================"
echo "[+] Откат на целевую прошивку завершен!"
echo "    Перезагружаю станцию: adb -s $DEVICE_IP reboot"
adb -s "$DEVICE_IP" shell "sync && reboot" || true
echo "======================================================"
