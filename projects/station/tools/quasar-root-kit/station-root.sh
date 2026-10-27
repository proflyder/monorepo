#!/bin/bash
# ============================================================================
#  station-root.sh — автоматический РУТ Яндекс Станций на quasar
#  ПУБЛИЧНАЯ СБОРКА: серты eMudhra-набора (eQuasar) — в комплекте; adb качается сам.
#  НАЗНАЧЕНИЕ: root-доступ, бэкап и обслуживание СВОИХ умных колонок пользователя
#  в его домашней сети (свои устройства, свой роутер, согласие владельца).
#  Проверено живьём: Лайт 2 (YNDX-00026, mDNS platform=plum) и Миди (YNDX-00054,
#  mDNS platform=cucumber, quasar/OTA=yandexmidi; рут на in-box 3.210.351.1.6,
#  раскладка совпала с разобранной OTA 0.369.1.50); на обеих fortress + OTA-запрет.
#  Пилот: Мини 3 Про (Amlogic A4/A113L2, Secure Boot, u-boot ENV под белым списком).
#  Платформа определяется по mDNS (platform=...) — скрипт универсальный, но на
#  НЕпроверенных прошивках предупреждает и собирает recon (--backup) для сверки.
#  Вектор: локальный прозрачный HTTPS-прокси для syncd с валидной цепочкой сертов quasar.yandex.net
#
#  ЧТО ДЕЛАЕТ САМ:
#   1. проверяет инструменты (качает adb если нет)
#   2. проверяет серты/цепочку (leaf ↔ key ↔ AAA-корень)
#   3. находит колонку в сети (mDNS) + её device_id/platform
#   4. проверяет локальную DNS-запись; если нет — ПЕЧАТАЕТ команду под твой роутер (роутеры у всех разные)
#   5. поднимает HTTPS-прокси :443 (sudo, спросит пароль) если ещё не running
#   6. дописывает adbd.remoteActivation=true в ответы /get_sync_info (rules.json)
#   7. ждёт открытия :5555 у колонки (если нет — попросит перегрузить её питанием)
#   8. adb connect + проверка uid=0 → РУТ
#   9. по флагам: --shell (интерактивный shell), --backup (бэкап + recon чужой прошивки),
#      --export (переносный tar.gz стенда — передать знакомому для проверки у него)
#
#  ЧТО НЕ МОЖЕТ — ПИШЕТ ГАЙД (что делать юзеру руками).
#
#  ЗАПУСК:   bash station-root.sh            → ИНТЕРАКТИВНОЕ МЕНЮ (Enter = МАСТЕР — главный путь)
#             bash station-root.sh --run     → МАСТЕР в терминале: ведёт до конца
#                          (DNS-гайд с перепроверкой  -> прокси -> РУТ -> fortress -> OTA-запрет
#                           -> статус -> напоминание убрать DNS-запись);
#                          БЕЗ tty --run — прежний неинтерактивный путь, те же exit-коды
#    тогглы:  --fortress / --fortress-off   самозащита adbd (deactivate->no-op + keeper)
#             --ota-off / --ota-on          запрет/разрешение OTA-прошивки колонки
#             --status                       состояние всех тогглов
#    прочее:  --shell --backup --export --router-off --ota-shield --help
#  Удалить локальную DNS-запись: bash station-root.sh --router-off
# ============================================================================
set -u
BASE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"        # ~/yndx/mitm
Y="$(dirname "$BASE")"                                        # ~/yndx
ADB="$Y/tools/platform-tools/adb"
LOG="$BASE/mitm.log"
RULES="$BASE/rules.json"
LASTF="$BASE/.last_column_ip"   # прошлый выбор колонки — меню/мастер не переспрашивают
ROUTER="${ROUTER:-$(ip route 2>/dev/null | awk '/^default/ {print $3; exit}')}"   # шлюз по умолчанию (или ROUTER=<ip> руками)
[ -n "$ROUTER" ] || ROUTER="192.168.1.1"
COLUMN_IP="${COLUMN_IP:-}"
QUASAR_HOST="quasar.yandex.net"
ADB_PORT=5555
MODE="run"; for a in "$@"; do case "$a" in
  --run)     MODE=run ;;
  -h|--help) MODE=help ;;
  --status)  MODE=status ;;
  --shell)   MODE=shell ;;
  --backup)  MODE=backup ;;
  --export)  MODE=export ;;
  --fortress) MODE=fortress ;;
  --fortress-off) MODE=fortressoff ;;
  --ota-off) MODE=otaoff ;;
  --ota-on)  MODE=otaon ;;
  --router-off) MODE=routeroff ;;
  --ota-shield) MODE=otashield ;;
esac; done

# МАСТЕР: --run в интерактивном терминале ведёт до конца; БЕЗ tty --run — прежнее
# неинтерактивное поведение (те же exit-коды, никаких read) — для вызовов из скриптов.
INTERACTIVE=0
[ -t 0 ] && INTERACTIVE=1

if [ "$MODE" = help ]; then
  cat <<EOF
station-root.sh — рут Яндекс Станций на quasar
Проверено живьём: Лайт 2 (plum) и Миди (cucumber/yandexmidi). Пилот: Мини 3 Про (Amlogic — см. --backup/--export).
СЕРТЫ В КОМПЛЕКТЕ: chain.pem + leaf.key лежат рядом со скриптом (README.md — «Серты»).
Вектор: локальный прозрачный HTTPS-прокси для syncd с валидной цепочкой сертов quasar.yandex.net

КОМАНДЫ:
  bash $0                БЕЗ аргументов в терминале = ИНТЕРАКТИВНОЕ МЕНЮ (Enter = МАСТЕР)
  bash $0 --run          МАСТЕР (в терминале): ведёт до конца — серты -> DNS-гайд (ждёт,
                         пока поставишь запись: Enter — перепроверить, q — выход)  -> прокси ->
                         РУТ -> предложение fortress + OTA-запрет -> статус -> напоминание
                         убрать DNS-запись с роутера. БЕЗ tty (--run из скрипта) — как
                         раньше: напечатает гайд и выйдет (exit 1), без интерактива.
  bash $0 --status       показать состояние всех тогглов на колонке
  bash $0 --fortress     вкл самозащиту adbd: deactivate -> no-op + keeper в /data
  bash $0 --fortress-off выкл самозащиту adbd (вернуть как у Яндекса)
  bash $0 --ota-off      вкл ЗАПРЕТ прошивки: Яндекс больше не может обновить колонку
  bash $0 --ota-on       выкл запрет (вернуть штатные OTA-обновления)
  bash $0 --backup       бэкап/разведка (recon) колонки: quasar.cfg, adb_options.inc, mtd, env, ps, dmesg...
                         (для НЕзнакомой прошивки — архив tar.gz, пришли его владельцу стенда)
  bash $0 --export       переносный tar.gz стенда (серты+скрипт+сервер+adb+README) —
                         для проверки на чужом ПК)
  bash $0 --shell        после рута — сразу интерактивный root-shell
  bash $0 --router-off   гайд: как удалить DNS-запись на своём роутере
  bash $0 --ota-shield   гайд: OTA-ЩИТ на РОУТЕРЕ — защита от фикс-прошивки ДО рута
                         (режем раздачу OTA: quasar.s3.yandex.net + *.svc.cdn.yandex.net;
                         голос/синк не трогаем; стенд/прокси не нужны)

ПОРЯДОК ДЛЯ НОВОЙ КОЛОНКИ (шаги 0-8; мастер = пункт меню 1 / --run в терминале):
  0) --ota-shield — щит на РОУТЕРЕ ещё ДО рута (защита от фикс-прошивки, стенд не нужен)
  1) DNS-запись quasar.yandex.net -> <этот ПК> на своём роутере (мастер печатает команды
     под Keenetic/MikroTik/OpenWrt/pi-hole/dnsmasq и ЖДЁТ: Enter — перепроверить, q — выход)
  2) мастер поднимает прокси :443 (sudo)                 —
  3) перегрузить колонку питанием (мастер ждёт :5555)   — всё это делает
  4) adb -> uid=0 => РУТ                                —      bash $0 --run
  5) fortress (deactivate -> no-op + keeper)            — мастер предложит сразу после рута
  6) --ota-off — запрет OTA (самый сильный слой)        — (по умолчанию ДА)
  7) --status — проверить, что всё стоит                — мастер покажет сам
  8) убрать DNS-запись с роутера — рут и защита теперь живут НА КОЛОНКЕ; мастер
     напомнит команды. OTA-щит (шаг 0) можно оставить — не мешает

ЗАЩИТА ОТ ФИКС-ПРОШИВКИ (если колонка лежала выключенной / сброшена до завода):
  Слои: 1) --ota-shield (роутер, до рута, всегда)  2) прокси-щит rules.json
  check_updates_mode=block — только трафик ЧЕРЕЗ нас (updatesd может ходить по
  прямым IP мимо)  3) --ota-off (после рута, ГЛАВНЫЙ, ТРИ подслоя: no-op в
  device_ota_update.sh + подмена ключа подписи ota_script.pub + updatesd вырезан
  из launch.json — апдейтер вообще не стартует).
  Правильный порядок: отрутить ПОКА вектор жив (сейчас) → --ota-off → дальше роутер-щит
  просто необязательный ремень.

ПЛАТФОРМЫ:
  • ПРОВЕРЕНО ЖИВЬЁМ: plum (Лайт 2) и cucumber/yandexmidi (Миди, YNDX-00054).
    Общий механизм у quasar: activate_adb.sh + adb_options.inc (маркеры
    /data/quasar/activate_adb_*), adbd-секция quasar.cfg (quasmodrom
    activate/deactivate/--network), выключатель remoteActivation в maind,
    onboot->check->adbd. У Миди нюанс: вход в OTA — apply_packed_ota из
    device_ota_update.sh (без fw_setenv boot_to_recovery) — обёртка -> no-op
    перекрывает и его.
  • СВЕРЕНО ПО РАЗОБРАННОЙ ПРОШИВКЕ: yandexmicro (Лайт 1) — тот же механизм.
  • ПРОЧИЕ (Мини 3 Про, Макс и др.) — вектор пускается тот же, но раскладка
    (разделы, ENV) не сверялась: Amlogic + Secure Boot, u-boot ENV под белым
    списком (только indication_brightness/silent) — тумблер boot_to_recovery
    может отсутствовать, обёртка device_ota_update.sh -> no-op остаётся главной
    защитой; после рута пришли recon (--backup).
  • скрипт сам предупредит и откажется, если файл не там; тогда пришли --backup-архив.

ПЕРВЫЙ ЗАПУСК — ЧТО СПРОСИТ:
  шаг 3: если колонку в mDNS не видит — задай её IP руками:
         COLUMN_IP=<ip> ROUTER=<ip роутера> bash $0
         (avahi-browse из пакета avahi-utils; IP колонки виден в вебе роутера)
  шаг 4: если DNS-записи quasar.yandex.net -> этот ПК нет — напечатает команды
         под Keenetic/MikroTik/OpenWrt/pi-hole/dnsmasq. В терминале мастер будет ждать:
         поставь запись и нажми Enter (перепроверит сам); без tty — запусти снова.
  шаг 5: спросит sudo-пароль (подъём HTTPS-прокси на :443).
  шаг 7: попросит ПЕРЕГРУЗИТЬ колонку питанием и будет ждать открытия :5555.
         Успех = строка "██ РУТ: uid=0(root) ██".

ПОСЛЕ РУТА:
  мастер (--run в терминале) сразу после рута сам предложит fortress + --ota-off
  (по умолчанию ДА) и покажет --status. Вручную то же самое:
  bash $0 --fortress && bash $0 --ota-off   # рут станет самовосстанавливающимся:
                                             # можно гасить прокси и убирать DNS-запись
  bash $0 --status                          # проверить, что всё стоит
  bash $0 --router-off                      # разбор + команды уборки DNS-записи

КАК ЭТО РАБОТАЕТ (пока не выключишь):
    DNS($ROUTER): $QUASAR_HOST -> <этот ПК>  =>  syncd колонки ходит в наш прокси,
    мы дописываем в /get_sync_info: adbd.remoteActivation=true (+network=true),
    maind (AdbSwitcher) запускает activate_adb.sh --quasmodrom activate --network,
    маркер /data/quasar/activate_adb_quasmodrom + adbd на :5555 переживают ребут.
    Рекомендуемый пакет: --fortress (adbd не снести конфигом) + --ota-off
    (обновление не придёт) => рут бессмертен без прокси и без DNS-записи.

БЕЗОПАСНОСТЬ:
    • прокси видит OAuth-токен колонки в логах (mitm.log) — не свети ими.
    • Подменённый DNS + серт на *.yandex.net — только для СВОЕЙ колонки в СВОЕЙ сети.
    • Полный сброс кнопкой сносит /data (маркеры, keeper) => рут пропадёт,
      повторяется весь путь (DNS-запись  -> прокси -> ребут -> fortress).
EOF
  exit 0
fi

C_G="\033[1;32m"; C_R="\033[1;31m"; C_Y="\033[1;33m"; C_B="\033[1;36m"; C_0="\033[0m"
C_M="\033[1;35m"; C_DIM="\033[2m"; C_W="\033[1;37m"
ok()   { echo -e "  ${C_G}[OK]${C_0} $*"; }
bad()  { echo -e "  ${C_R}[НЕТ]${C_0} $*"; }
info() { echo -e "  ${C_B}[..]${C_0} $*"; }
step() { echo -e "\n${C_B}=== ${*} ===${C_0}"; }
# платформы, проверенные живьём (рут + тогглы на реальных колонках)
is_tested() {
  case "${1:-}" in
    plum|yandexmidi|cucumber) return 0 ;;
    *) return 1 ;;
  esac
}
# человечье имя платформы (mDNS ≠ quasar/OTA: Миди в mDNS = cucumber, в OTA = yandexmidi)
plat_name() {
  case "$1" in
    plum)               echo "Лайт 2 (YNDX-00026)" ;;
    cucumber|yandexmidi) echo "Миди (YNDX-00054)" ;;
    yandexmicro)       echo "Лайт 1" ;;
    yandexmini)        echo "Мини 1" ;;
    yandexmini_2)     echo "Мини 2" ;;
    yandexstation)     echo "Станция 1" ;;
    yandexstation_2)  echo "Станция 2/Макс" ;;
    "")                echo "?" ;;
    *)                 echo "? ($1)" ;;
  esac
}

# ----------------------------------------------------------------------------
# ИНТЕРАКТИВНОЕ МЕНЮ — запуск без аргументов (аргументы остаются для скриптов)
if [ $# -eq 0 ]; then
  if [ ! -t 0 ]; then echo "нет аргументов и stdin не терминал — используй флаги (см. --help)"; exit 1; fi
  while :; do
    echo -e "\n${C_M}═══════════════════════════════════════════════${C_0}"
    echo -e "${C_M}   🔊 Яндекс Станции — рут-стенд (quasar)${C_0}"
    echo -e "${C_M}   ${C_DIM}проверено живьём: Лайт 2 · Миди   |   пилот: Мини 3 Про${C_0}"
    echo -e "${C_M}═══════════════════════════════════════════════${C_0}"
    echo -e "   ${C_DIM}── ГЛАВНЫЙ ПУТЬ (новичок: начни отсюда) ──────${C_0}"
    echo -e "   ${C_W}1)${C_0} ${C_G}МАСТЕР: рутировать колонку шаг за шагом${C_0}"
    echo -e "      ${C_G}РЕКОМЕНДУЮ — ведёт до конца:${C_0} ${C_DIM}DNS-запись  -> прокси -> рут -> fortress -> OTA-запрет -> статус -> уборка DNS${C_0}"
    echo -e "   ${C_W}2)${C_0} ${C_B}Статус${C_0} колонки и всех тогглов ${C_DIM}(быстрый просмотр)${C_0}"
    echo -e "   ${C_DIM}── ЗАЩИТА РУТА ───────────────────────────────${C_0}"
    echo -e "   ${C_W}3)${C_0} ${C_Y}Fortress adbd${C_0} — защита рута от выключения ${C_DIM}(вкл/выкл)${C_0}"
    echo -e "   ${C_W}4)${C_0} ${C_Y}OTA-запрет${C_0} — Яндекс не может обновить колонку ${C_DIM}(вкл/выкл)${C_0}"
    echo -e "   ${C_W}5)${C_0} ${C_Y}OTA-ЩИТ на роутере${C_0} ${C_DIM}(защита от фикс-прошивки ДО рута — стенд не нужен)${C_0}"
    echo -e "   ${C_DIM}── УТИЛИТЫ ───────────────────────────────────${C_0}"
    echo -e "   ${C_W}6)${C_0} Бэкап/разведка колонки ${C_DIM}(recon: чужую прошивку — прислать владельцу)${C_0}"
    echo -e "   ${C_W}7)${C_0} Root-shell на колонке ${C_DIM}(интерактив)${C_0}"
    echo -e "   ${C_W}8)${C_0} Гайд: удалить DNS-запись на своём роутере ${C_DIM}(Keenetic/MikroTik/OpenWrt...)${C_0}"
    echo -e "   ${C_DIM}── ПРОЧЕЕ ─────────────────────────────────────${C_0}"
    echo -e "   ${C_W}9)${C_0} ${C_DIM}Справка: все команды, как это работает, безопасность${C_0}"
    echo -e "   ${C_W}10)${C_0} ${C_M}Собрать переносный пакет${C_0} ${C_DIM}(tar.gz — стенд для другого ПК)${C_0}"
    echo -e "   ${C_W}q)${C_0} Выход"
    echo -en "\n${C_B}Выбор (Enter = 1 МАСТЕР / цифра/q):${C_0} "
    read -r ch || exit 0
    case "$ch" in
      ""|1) echo; bash "$0" --run ;;
      2) echo; bash "$0" --status ;;
      3) echo; read -rp "   Fortress: [1] включить / [2] выключить / иное=отмена: " f
         [ "$f" = 1 ] && { echo; bash "$0" --fortress; }
         [ "$f" = 2 ] && { echo; bash "$0" --fortress-off; } ;;
      4) echo; read -rp "   OTA-запрет: [1] включить / [2] выключить / иное=отмена: " f
         [ "$f" = 1 ] && { echo; bash "$0" --ota-off; }
         [ "$f" = 2 ] && { echo; bash "$0" --ota-on; } ;;
      5) echo; bash "$0" --ota-shield ;;
      6) echo; bash "$0" --backup ;;
      7) echo; bash "$0" --shell ;;
      8) echo; bash "$0" --router-off ;;
      9) echo; bash "$0" --help ;;
      10) echo; bash "$0" --export ;;
      q|Q) exit 0 ;;
      *) echo "  ...нет такого пункта"; continue ;;
    esac
    echo
    read -rp "Enter — назад в меню (q — выход): " e || exit 0
    [ "$e" = "q" ] && exit 0
  done
fi

# ----------------------------------------------------------------------------
if [ "$MODE" = run ] && [ "$INTERACTIVE" = 1 ]; then
  echo -e "${C_G}МАСТЕР${C_0}: веду до конца — DNS-запись  -> прокси -> РУТ -> fortress -> OTA-запрет -> статус -> уборка DNS."
  echo "  На каждом шаге подскажу, что делать; прервать — Ctrl+C."
fi

step "1/7 Инструменты"
miss=()
for t in python3 openssl dig curl timeout; do command -v "$t" >/dev/null || miss+=("$t"); done
[ ${#miss[@]} -gt 0 ] && { bad "не хватает: ${miss[*]} — поставь пакетами дистрибутива"; exit 1; }
command -v avahi-browse >/dev/null || info "avahi-browse нет (пакет avahi-utils) — колонку придётся задать руками: COLUMN_IP=<ip> bash $0"
if [ ! -x "$ADB" ]; then
  info "adb нет — качаю platform-tools..."
  mkdir -p "$Y/tools" && cd "$Y/tools"
  curl -sSLo pt.zip https://dl.google.com/android/repository/platform-tools-latest-linux.zip || { bad "не скачался adb"; exit 1; }
  7z x -y pt.zip >/dev/null 2>&1 || unzip -o pt.zip >/dev/null 2>&1 || { bad "нет 7z/unzip для распаковки"; exit 1; }
  rm -f pt.zip; [ -x "$ADB" ] || { bad "adb так и не появился"; exit 1; }
fi
ok "python3, openssl, dig, curl, adb ($("$ADB" version 2>/dev/null | head -1 | awk '{print $NF}'))"

# ----------------------------------------------------------------------------
if [ "$MODE" = run ]; then            # серты нужны только полному пути
step "2/7 Серты и цепочка (лист quasar.yandex.net, кросс от AAA Certificate Services)"
for f in chain.pem leaf.key; do
  [ -s "$BASE/$f" ] || { bad "нет $BASE/$f — серты лежат в комплекте рядом со скриптом (README.md)"; exit 1; }
done
k1=$(openssl x509 -in <(sed -n '1,/^-----END CERTIFICATE-----$/p' "$BASE/chain.pem") -noout -pubkey 2>/dev/null | openssl sha256 | awk '{print $NF}')
k2=$(openssl pkey -in "$BASE/leaf.key" -pubout 2>/dev/null | openssl sha256 | awk '{print $NF}')
[ "$k1" = "$k2" ] || { bad "chain.pem НЕ соответствует leaf.key"; exit 1; }
if [ -s "$Y/roots/aaa_root.pem" ]; then
  openssl verify -CAfile "$Y/roots/aaa_root.pem" -untrusted "$BASE/chain.pem" \
    <(sed -n '1,/^-----END CERTIFICATE-----$/p' "$BASE/chain.pem") >/dev/null 2>&1 \
    && ok "цепочка до AAA Certificate Services валидна" \
    || bad "цепочка до AAA не бьётся (но колонка может всё равно принять)"
else
  info "нет roots/aaa_root.pem для офлайн-проверки цепочки — пропускаю (не обязательно)"
fi
openssl x509 -in <(sed -n '1,/^-----END CERTIFICATE-----$/p' "$BASE/chain.pem") -noout -checkend 86400 >/dev/null 2>&1 \
  || bad "лист ПРОСРОЧЕН — перевыпусти свой серт (см. README.md — «Серты»)"
ok "серт на $(openssl x509 -in <(sed -n '1,/^-----END CERTIFICATE-----$/p' "$BASE/chain.pem") -noout -subject | sed 's/.*= //')"
fi

# --- режим --ota-shield: роутер-щит от прошивки ДО рута (стенд/прокси/колонка не нужны) ---
[ "$MODE" = otashield ] && {
  echo -e "\n${C_B}=== OTA-ЩИТ на РОУТЕРЕ: защита от фикс-прошивки ДО рута ===${C_0}"
  echo -e "  ${C_Y}Кейс:${C_0} колонка лежала выключенной / сброшена до завода → Яндекс выпустил фикс →"
  echo "  при первом выходе в сеть updatesd сам скачает и применит прошивку, а наши"
  echo "  патчи (--ota-off, fortress) ставятся только ЧЕРЕЗ рут. Щит = не дать ДОСТАВКУ OTA."
  echo
  echo -e "  ${C_B}ЧТО РЕЖЕМ (DNS -> 0.0.0.0):${C_0} хосты раздачи OTA"
  echo "    • quasar.s3.yandex.net                (fallback-раздача)"
  echo "    • *.svc.cdn.yandex.net                (основной CDN; имя SID-ное, МЕНЯЕТСЯ)"
  echo -e "  ${C_B}ЧТО НЕ РЕЖЕМ:${C_0} quasar.yandex.net — на нём же /get_sync_info и голос; uniproxy тоже жив."
  echo
  echo -e "  ${C_W}▸ ПОСТАВИТЬ — под свой роутер (любой ОДИН вариант):${C_0}"
  echo -e "   ${C_G}Keenetic${C_0} (telnet $ROUTER / ssh admin@$ROUTER / веб-CLI http://$ROUTER/a):"
  echo "       ip host quasar.s3.yandex.net 0.0.0.0"
  echo "       ip host cdnriqbxswemmatxzibl.svc.cdn.yandex.net 0.0.0.0   # текущий CDN OTA (меняется!)"
  echo "       system configuration save"
  echo "       # (Keenetic wildcard в стат. DNS не везде есть: если прошивка придёт"
  echo "       #  с ДРУГИМ CDN-именем — обнови вторую запись)"
  echo -e "   ${C_G}MikroTik:${C_0}  /ip dns static add name=quasar.s3.yandex.net address=0.0.0.0"
  echo "             /ip dns static add regexp=\".*\\\\.svc\\\\.cdn\\\\.yandex\\\\.net\" address=0.0.0.0"
  echo "             # закрывает и IP-фолбэк: /ip firewall filter add chain=forward dst-address=93.158.134.158 action=drop"
  echo -e "   ${C_G}OpenWrt:${C_0}   uci add_list dhcp.@dnsmasq[0].address=/quasar.s3.yandex.net/0.0.0.0"
  echo "             uci add_list dhcp.@dnsmasq[0].address=/.svc.cdn.yandex.net/0.0.0.0"
  echo "             uci commit dhcp && /etc/init.d/dnsmasq restart"
  echo -e "   ${C_G}pi-hole:${C_0}   Local DNS -> DNS Records: quasar.s3.yandex.net = 0.0.0.0"
  echo "             (wildcard *.svc.cdn.yandex.net — dnsmasq-конфигом: address=/.svc.cdn.yandex.net/0.0.0.0)"
  echo -e "   ${C_G}dnsmasq:${C_0}   address=/quasar.s3.yandex.net/0.0.0.0"
  echo "             address=/.svc.cdn.yandex.net/0.0.0.0"
  echo
  echo -e "  ${C_B}Проверка:${C_0} dig +short quasar.s3.yandex.net @$ROUTER  → пусто/NXDOMAIN или 0.0.0.0"
  echo "            (Keenetic на 0.0.0.0 отвечает пусто — это тоже блок: хост не резолвится)"
  echo
  echo -e "  ${C_W}▸ СНЯТЬ ЩИТ (обратные команды — удалить обе записи):${C_0}"
  echo -e "   ${C_G}Keenetic:${C_0}  no ip host quasar.s3.yandex.net 0.0.0.0"
  echo "              no ip host cdnriqbxswemmatxzibl.svc.cdn.yandex.net 0.0.0.0"
  echo "              system configuration save"
  echo "              (или веб-интерфейс: Интернет -> DNS-сервер -> удалить строки с 0.0.0.0)"
  echo -e "   ${C_G}MikroTik:${C_0}  /ip dns static remove [find name=quasar.s3.yandex.net]"
  echo "              /ip dns static remove [find regexp=\".*\\\\.svc\\\\.cdn\\\\.yandex\\\\.net\"]"
  echo "              /ip firewall filter remove [find dst-address=93.158.134.158]"
  echo "              (последняя — если ставил IP-фолбэк-правило)"
  echo -e "   ${C_G}OpenWrt:${C_0}   uci del_list dhcp.@dnsmasq[0].address=/quasar.s3.yandex.net/0.0.0.0"
  echo "              uci del_list dhcp.@dnsmasq[0].address=/.svc.cdn.yandex.net/0.0.0.0"
  echo "              uci commit dhcp && /etc/init.d/dnsmasq restart"
  echo -e "   ${C_G}pi-hole:${C_0}   Local DNS -> DNS Records -> удали записи quasar.s3.yandex.net и *.svc.cdn.yandex.net"
  echo -e "   ${C_G}dnsmasq:${C_0}   удали строки address=/quasar.s3.yandex.net/0.0.0.0 и address=/.svc.cdn.yandex.net/0.0.0.0"
  echo "              и рестартни dnsmasq"
  echo
  echo -e "  ${C_B}Проверка снятия:${C_0} dig +short quasar.s3.yandex.net @$ROUTER — вернётся настоящий IP (93.158.134.158)"
  echo
  echo -e "  ${C_Y}⚠ НЮАНСЫ:${C_0}"
  echo "    • Смысл щита теряется только если колонка НЕ отрутована и НЕ под нашим прокси;"
  echo "      отрутованной (fortress + --ota-off) колонке щит вообще не нужен — снимай смело."
  echo "    • Музыка идёт через другие CDN; если что-то из медиа заикается —"
  echo "      убери правило svc.cdn, оставь только quasar.s3."
  echo "    • IP-фолбэк (fallbackIp в ответе check_updates, замечен 93.158.134.158):"
  echo "      DNS-щит его не закрывает; захочешь параноидально — firewall-правило"
  echo "      на роутере для этого адреса (MikroTik выше; Keenetic — «Правила доступа»)."
  echo
  echo -e "  ${C_B}Слои защиты от фикс-прошивки:${C_0}"
  echo "   1) РОУТЕР-ЩИТ (это) — работает всегда, стенд не нужен. Для НЕотрутованной колонки."
  echo "   2) ПРОКСИ-щит — на трафике ЧЕРЕЗ нас: server.py на /check_updates отдаёт"
  echo "      hasUpdate:false (rules.json: check_updates_mode=block). НЕ покрывает"
  echo "      updatesd: он ходит по прямым IP (fallbackIp) МИМО прокси — см. пункт 4."
  echo "   3) --ota-off (ПОСЛЕ рута) — ТРИ подслоя: no-op в device_ota_update.sh,"
  echo "      подмена ключа подписи ota_script.pub и updatesd, вырезанный из launch.json"
  echo "      (апдейтер не стартует при загрузке). Самый сильный слой, но требует рута."
  echo "   Итого: отрутить колонку ПОКА вектор жив (сейчас), поставить --ota-off —"
  echo "   и роутер-щит станет необязательным (но не лишним)."
  exit 0; }

# ----------------------------------------------------------------------------
# Шаг 3 нужен всем, КРОМЕ --export (переносный пакет собирается без колонки)
if [ "$MODE" != export ]; then
step "3/7 Локальный IP и поиск колонки (mDNS _yandexio._tcp — ЛЮБАЯ platform)"
LOCAL_IP=$(ip -4 route get "$ROUTER" 2>/dev/null | grep -oP 'src \K[\d.]+' | head -1)
[ -n "$LOCAL_IP" ] || LOCAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
[ -n "$LOCAL_IP" ] && ok "этот ПК: $LOCAL_IP" || { bad "не нашёл свой IP"; exit 1; }

# Один вызов avahi-browse: список всех quasar-колонок "ip platform device_id"
# формат TXT у avahi-browse: txt = ["cluster=yes" "platform=plum" "deviceId=..."]
MDNS_LIST=$(timeout 6 avahi-browse -d local -rt _yandexio._tcp 2>/dev/null | python3 -c '
import sys, re
devs = {}
cur = None
host = None
for line in sys.stdin:
    line = line.strip()
    if line.startswith("hostname ="):
        m = re.findall(r"\[([^\]]+)\]", line)
        host = m[0].replace(".local", "") if m else None
    elif line.startswith("address ="):
        m = re.findall(r"\[([0-9.]+)\]", line)
        cur = m[0] if m else None
    elif line.startswith("txt =") and cur:
        d = dict(re.findall(r"\"(\w+)=([^\"]*)\"", line))
        devs[(cur, d.get("platform","?"), d.get("deviceId","?"), host or "?")] = 1
for ip, p, did, h in devs:
    print(ip, p, did, h)
' 2>/dev/null)

if [ -n "$MDNS_LIST" ]; then
  echo "$MDNS_LIST" | while read -r mip mplat mdid mhost; do
    [ -n "$mip" ] && echo "        mDNS: $mip  (platform: ${mplat:-?} — $(plat_name "${mplat:-?}"), device_id: ${mdid:-?}, имя: ${mhost:-?})"
  done
fi

# ---- ВЫБОР КОЛОНКИ: в сети несколько — спрашиваем, с какой работаем ----
# (работает для ЛЮБОГО режима в tty: run/status/backup/...; COLUMN_IP задаёт цель явно)
# (mDNS-имена платформ НЕ совпадают с quasar/OTA-именами: Миди в mDNS = cucumber,
#  в quasar/OTA = yandexmidi; и то и то — она. plum = Лайт 2.)
if [ -z "$COLUMN_IP" ] && [ "$INTERACTIVE" = 1 ]; then
  NDEV=$(echo "$MDNS_LIST" | grep -c .)
  if [ "$NDEV" -gt 1 ]; then
    echo -e "\n  ${C_Y}В сети $NDEV колонки/колонок — с какой работаем?${C_0}"
    LAST_IP=$(cat "$LASTF" 2>/dev/null || true)
    DEF_N=""; DEF_IP=""; LAST_N=""; i=0
    while read -r mip mplat mdid mhost; do
      [ -z "$mip" ] && continue
      i=$((i+1))
      mark=""
      if timeout 2 bash -c "echo > /dev/tcp/$mip/5555" 2>/dev/null; then
        mark="${C_G}[рут уже есть]${C_0}"
      fi
      echo -e "   ${C_W}$i)${C_0} $mip   ${mplat} — $(plat_name "$mplat")   id: ${mdid:-?}   $mark"
      [ "$mip" = "$LAST_IP" ] && LAST_N=$i
      if [ -z "$mark" ] && [ -z "$DEF_N" ]; then DEF_N=$i; DEF_IP=$mip; fi
    done <<< "$(echo "$MDNS_LIST" | grep .)"
    # дефолт Enter: прошлый выбор (если колонка всё ещё в сети), иначе первая без рута
    if [ -n "$LAST_N" ]; then
      DEF_N=$LAST_N; DEF_IP=$(echo "$MDNS_LIST" | grep . | sed -n "${LAST_N}p" | awk '{print $1}')
      echo -e "   ${C_DIM}(Enter = $DEF_N — как в прошлый раз)${C_0}"
    fi
    [ -n "$DEF_N" ] || { DEF_N=1; DEF_IP=$(echo "$MDNS_LIST" | grep . | head -1 | awk '{print $1}'); }
    read -rp "   Выбор [Enter = $DEF_N] : " ch2
    if [ -n "$ch2" ] && echo "$ch2" | grep -qE '^[0-9]+$' && [ "$ch2" -ge 1 ] && [ "$ch2" -le "$i" ]; then
      COLUMN_IP=$(echo "$MDNS_LIST" | grep . | sed -n "${ch2}p" | awk '{print $1}')
    else
      COLUMN_IP=$DEF_IP
    fi
    ok "цель: $COLUMN_IP"
    echo "$COLUMN_IP" > "$LASTF" 2>/dev/null
  fi
fi

if [ -z "$COLUMN_IP" ]; then
  # авто-выбор (не-интерактив/скрипты): прошлый выбор -> проверенные -> первая найденная
  LAST_IP=$(cat "$LASTF" 2>/dev/null || true)
  if [ -n "$LAST_IP" ] && echo "$MDNS_LIST" | grep -q "^$LAST_IP "; then
    COLUMN_IP=$LAST_IP
  else
    COLUMN_IP=$(echo "$MDNS_LIST" | awk '$2=="plum"{print $1; exit}')
    [ -n "$COLUMN_IP" ] || COLUMN_IP=$(echo "$MDNS_LIST" | awk '$2=="yandexmidi"||$2=="cucumber"{print $1; exit}')
    [ -n "$COLUMN_IP" ] || COLUMN_IP=$(echo "$MDNS_LIST" | awk 'NF{print $1; exit}')
  fi
fi
# платформа и device_id выбранной (или заданной руками) колонки
SEL=$(echo "$MDNS_LIST" | awk -v ip="$COLUMN_IP" '$1==ip{print; exit}')
PLAT=$(echo "$SEL" | awk '{print $2}'); PLAT=${PLAT:-?}
COL_ID=$(echo "$SEL" | awk '{print $3}'); COL_ID=${COL_ID:-?}
COL_HOST=$(echo "$SEL" | awk '{print $4}'); COL_HOST=${COL_HOST:-?}

if [ -n "$COLUMN_IP" ]; then
  ok "колонка: $COLUMN_IP  (platform: $PLAT, device_id: $COL_ID${COL_HOST:+, имя: $COL_HOST})"
  if ! is_tested "$PLAT" && [ "$PLAT" != "?" ]; then
    echo -e "  ${C_Y}[!]${C_0} platform $PLAT ($(plat_name "$PLAT")): НЕ из проверенных — вектор тот же"
    echo "      (rewrite-правило remoteActivation в /get_sync_info), но раскладка не сверялась:"
    echo "      смотри предупреждения по ходу; после рута пришли владельцу стенда recon"
    echo "      (bash $0 --backup, tar.gz) — сверю и добавлю платформу в проверенные."
  fi
else
  bad "колонка в mDNS не видна."
  echo "  ${C_Y}ГАЙД: 1) колонка в той же Wi-Fi сети? 2) если avahi-browse нет — поставь: sudo apt install avahi-utils"
  echo "  3) или задай IP руками (виден в веб-интерфейсе роутера, список клиентов):"
  echo "       COLUMN_IP=<ip колонки> ROUTER=<ip роутера> bash $0${MODE:+ $MODE}${C_0}"
  [ "$MODE" = status ] && true || exit 1
fi
fi   # конец шага 3 (кроме --export)

# --- режим --router-off: ничего не трогаем, печатаем команды под твой роутер ---
[ "$MODE" = routeroff ] && {
  echo -e "\n${C_B}=== Удаление локальной DNS-записи ===${C_0}"
  echo -e "  ${C_W}▸ ГАЙД — удали СТАТИЧЕСКУЮ запись $QUASAR_HOST -> $LOCAL_IP на своём роутере:${C_0}"
  echo -e "   ${C_G}Keenetic:${C_0}  no ip host $QUASAR_HOST $LOCAL_IP   (потом: system configuration save)"
  echo -e "   ${C_G}MikroTik:${C_0}  /ip dns static remove [find name=$QUASAR_HOST]"
  echo -e "   ${C_G}OpenWrt:${C_0}   uci del_list dhcp.@dnsmasq[0].address=/$QUASAR_HOST/$LOCAL_IP ; uci commit dhcp"
  echo -e "   ${C_G}pi-hole:${C_0}   Local DNS -> DNS Records -> удали $QUASAR_HOST"
  echo -e "   ${C_G}dnsmasq:${C_0}   удали строку address=/$QUASAR_HOST/... и рестартни dnsmasq"
  echo
  # -- что будет с рутом БЕЗ нашей DNS-записи: зависит от того, стоит ли fortress --
  FSTATE="?"
  if [ -n "$COLUMN_IP" ] && [ -x "$ADB" ]; then
    "$ADB" connect "$COLUMN_IP:$ADB_PORT" >/dev/null 2>&1
    W=$("$ADB" -s "$COLUMN_IP:$ADB_PORT" shell "head -2 /system/vendor/quasar/activate_adb.sh" 2>/dev/null | tr -d '\r')
    echo "$W" | grep -q FORTRESS && FSTATE="on" || FSTATE="off"
  fi
  case "$FSTATE" in
    on)
      echo -e "\n  ${C_G}▸ ЧТО БУДЕТ С РУТОМ БЕЗ ЗАПИСИ (проверено на колонке): ЖИВ и защищён${C_0}"
      echo "   • маркеры /data/quasar/activate_adb_* в rw-разделе — переживают ребут,"
      echo "     onboot.sh запускает 'activate_adb.sh check' → adbd поднимается на :5555."
      echo "   • deactivate из конфига Яндекса (maind/AdbSwitcher) ЗАБЛОКИРОВАН обёрткой"
      echo "     (fortress: deactivate -> no-op) — легитный конфиг колонке не страшен."
      echo "   • DNS-запись и наш прокси НЕ нужны. Риск остаётся один: полный сброс кнопкой"
      echo "     (сносит /data: маркеры+keeper) → путь повторить: DNS  -> прокси -> ребут." ;;
    off)
      echo -e "\n  ${C_Y}▸ ЧТО БУДЕТ С РУТОМ БЕЗ ЗАПИСИ — ВНИМАНИЕ: fortress НЕ установлен${C_0}"
      echo "   • Рут ПЕРЕЖИВАЕТ ребут: маркеры /data/quasar/activate_adb_* в rw-разделе,"
      echo "     onboot.sh при старте запускает 'activate_adb.sh check' → adbd на :5555."
      echo "   • НО: если Яндекс пришлёт конфиг БЕЗ adbd.remoteActivation=true, maind"
      echo "     (AdbSwitcher) может запустить deactivate и снять маркер — OTA-щит это"
      echo "     НЕ закрывает (deactivate приходит с КОНФИГОМ, не с прошивкой)."
      echo "     Лечение: bash $0 --fortress (deactivate -> no-op) — и риск исчезнет;"
      echo "     либо держи DNS-запись+прокси, либо запрети колонке ($COLUMN_IP) доступ"
      echo "     к quasar.yandex.net на роутере."
      echo "   • Полный сброс кнопкой сносит /data → рут пропадёт (путь с нуля)." ;;
    *)
      echo -e "\n  ${C_Y}▸ ЧТО БУДЕТ С РУТОМ БЕЗ ЗАПИСИ — fortress не проверен (колонка offline?)${C_0}"
      echo "   • Ребут рут переживёт (маркеры в /data + onboot.sh -> check -> adbd)."
      echo "   • Риск: deactivate из конфига Яндекса (maind) снимет маркер, если конфиг"
      echo "     придёт без remoteActivation=true. OTA-щит это НЕ закрывает."
      echo "     Лечение: bash $0 --fortress на колонке (deactivate -> no-op);"
      echo "     либо держи DNS-запись+прокси; либо запрети колонке доступ к quasar.yandex.net."
      echo "   • Полный сброс кнопкой сносит /data → рут пропадёт." ;;
  esac
  exit 0; }


# ----------------------------------------------------------------------------
if [ "$MODE" = run ]; then            # DNS + прокси + ожидание нужны только полному пути
step "4/7 DNS-DNS-запись: $QUASAR_HOST -> $LOCAL_IP (на ТВОЁМ роутере)"
# Роутеры у всех разные — сами ничего не настраиваем: проверяем и печатаем команду.
dns_ip() {
  timeout 5 dig +short "$QUASAR_HOST" "@$ROUTER" 2>/dev/null | grep -P '^\d+\.' | head -1
  timeout 5 dig +short "$QUASAR_HOST" 2>/dev/null | grep -P '^\d+\.' | head -1
}
CUR=$(dns_ip | head -1)
if [ "$CUR" = "$LOCAL_IP" ]; then
  ok "уже настроено: $QUASAR_HOST -> $LOCAL_IP"
else
  bad "не настроено (сейчас: ${CUR:-пусто})"
  echo -e "  ${C_Y}▸ ГАЙД: на роутере нужна СТАТИЧЕСКАЯ DNS-запись: $QUASAR_HOST -> $LOCAL_IP${C_0}"
  echo -e "  ${C_DIM}Под твой роутер (любой ОДИН вариант):${C_0}"
  echo -e "   ${C_G}Keenetic${C_0} (telnet $ROUTER / ssh admin@$ROUTER или веб-CLI http://$ROUTER/a):"
  echo "       ip host $QUASAR_HOST $LOCAL_IP"
  echo "       system configuration save"
  echo -e "   ${C_G}MikroTik:${C_0}   /ip dns static add name=$QUASAR_HOST address=$LOCAL_IP"
  echo -e "   ${C_G}OpenWrt:${C_0}    uci add_list dhcp.@dnsmasq[0].address=/$QUASAR_HOST/$LOCAL_IP"
  echo "               uci commit dhcp && /etc/init.d/dnsmasq restart"
  echo -e "   ${C_G}pi-hole:${C_0}    Local DNS -> DNS Records: $QUASAR_HOST = $LOCAL_IP"
  echo -e "   ${C_G}dnsmasq:${C_0}    address=/$QUASAR_HOST/$LOCAL_IP"
  echo -e "  ${C_B}Проверка:${C_0} dig +short $QUASAR_HOST @$ROUTER   (должно дать $LOCAL_IP)"
  if [ "$INTERACTIVE" = 1 ]; then
    # МАСТЕР: не выходим — ждём, пока юзер поставит запись на роутере, и перепроверяем сами
    echo -e "  ${C_0}${C_B}МАСТЕР: поставь запись на роутере и вернись сюда — я перепроверю сам.${C_0}"
    while :; do
      echo -en "\n${C_B}[Enter — поставил, перепроверить / r — перепроверить / q — выход]:${C_0} "
      read -r ans || exit 0
      case "$ans" in
        q|Q) echo; info "выход из мастера (DNS-запись так и не появилась — гайд выше)"; exit 0 ;;
        *)   CUR=$(dns_ip | head -1)
             if [ "$CUR" = "$LOCAL_IP" ]; then
               echo; ok "готово: $QUASAR_HOST -> $LOCAL_IP — продолжаю"
               break
             fi
             bad "пока не настроено (сейчас: ${CUR:-пусто}) — гайд выше, жду Enter" ;;
      esac
    done
  else
    echo -e "  Затем запусти скрипт снова.${C_0}"
    exit 1
  fi
fi
# (режим --router-off обрабатывается раньше — сразу после поиска колонки)

# ----------------------------------------------------------------------------
step "5/7 прокси-сервер :443"
if timeout 1 bash -c "echo > /dev/tcp/$LOCAL_IP/443" 2>/dev/null; then
  ok "уже слушает :443 (не трогаю)"
else
  info "запускаю (sudo — может спросить пароль)..."
  (nohup sudo python3 "$BASE/server.py" >"$BASE/server.out" 2>&1 &)
  sleep 2; n=0
  while [ $n -lt 15 ]; do
    timeout 1 bash -c "echo > /dev/tcp/$LOCAL_IP/443" 2>/dev/null && break
    sleep 1; n=$((n+1))
  done
  timeout 1 bash -c "echo > /dev/tcp/$LOCAL_IP/443" 2>/dev/null \
    && ok "сервер на :443 (лог: server.out + mitm.log)" \
    || { bad "сервер не встал на :443 (занят порт? смотри: ss -tlnp | grep 443)"; exit 1; }
fi
probe=$(curl -sk --resolve "$QUASAR_HOST:443:$LOCAL_IP" -o /dev/null -w '%{http_code}' --max-time 10 "https://$QUASAR_HOST/get_sync_info?device_id=probe" 2>/dev/null || echo 000)
case "$probe" in
  000) bad "серт не отдаётся — смотри server.out" ;;
  *)   ok  "серты отвечают (HTTP $probe от пробника — норм, колонку ждём)" ;;
esac
python3 -c "
import json; r = json.load(open('$RULES'))
cu = r.get('check_updates_mode', 'passthrough')
print('        rules.json: adbd.remoteActivation =', json.dumps(r['rewrite'][0]))
print('        rules.json: check_updates_mode =', cu, '(block = колонке всегда hasUpdate:false)')" 2>/dev/null \
  || info "rules.json не читается — колонка получит ЛЕГИТНЫЙ конфиг без adbd (это безопасно, но рута не будет)"

# ----------------------------------------------------------------------------
step "6/7 Жду ADB (:5555) у колонки"
wait_port() { timeout 2 bash -c "echo > /dev/tcp/$COLUMN_IP/$1" 2>/dev/null; }
if wait_port $ADB_PORT; then
  ok ":5555 уже открыт"
else
  echo -e "  ${C_Y}ГАЙД: сейчас колонка ещё держит старый конфиг. ПЕРЕГРУЗИ ЕЁ ПИТАНИЕМ"
  echo "  (вынь/вставь USB-C или кнопку питания на ~5 сек). Жду до 10 минут, Ctrl+C — отмена.${C_0}"
  t0=$(date +%s); seen=""
  while true; do
    if wait_port $ADB_PORT; then echo; ok ":5555 ОТКРЫТ ($(($(date +%s)-t0)) c)"; break; fi
    now=$(( $(date +%s) - t0 ))
    [ $now -gt 600 ] && { echo; bad "10 минут прошло, :5555 нет."; echo "  ${C_Y}Проверь: колонка загрузилась? (попроси Алису что-нибудь) — она ходит через наш сервер?";
      echo "  смотри $LOG — есть ли GET /get_sync_info от $COLUMN_IP. Если есть, а adbd не поднялся —";
      echo "  значит adbd.remoteActivation не сработал на этой прошивке: дай вывод tail -50 $LOG агенту."; echo "  ${C_0}"; exit 1; }
    if [ $((now % 20)) -eq 0 ]; then
      new=$(grep -ac "get_sync_info.*from $COLUMN_IP" "$LOG" 2>/dev/null || echo 0)
      printf "\r        ждём... %3dс | конфиг-запросов от колонки: %s   " "$now" "$new"
    fi
    sleep 1
  done
fi
fi   # конец run-only блока (шаги 4-6: DNS, прокси, ожидание :5555)

# ----------------------------------------------------------------------------
if [ "$MODE" = run ] || [ "$MODE" = shell ] || [ "$MODE" = backup ]; then
step "7/7 Подключаюсь и проверяю рут"
"$ADB" start-server >/dev/null 2>&1
n=0; until "$ADB" connect "$COLUMN_IP:$ADB_PORT" >/dev/null 2>&1 || [ $n -ge 10 ]; do sleep 1; n=$((n+1)); done
out=$("$ADB" -s "$COLUMN_IP:$ADB_PORT" shell id 2>&1)
if echo "$out" | grep -q "uid=0"; then
  ROOT_OK=1
  echo -e "\n${C_G}  ██ РУТ: $out ██${C_0}"
else
  bad "подключился, но не root: $out — дай вывод агенту"
fi
"$ADB" -s "$COLUMN_IP:$ADB_PORT" shell 'uname -a; cat /etc/os-release 2>/dev/null | head -2; cat /system/vendor/quasar/VERSION 2>/dev/null; echo; ls /data/quasar/ 2>/dev/null' 2>/dev/null
if ! is_tested "${PLAT:-plum}" && [ "${PLAT:-plum}" != "?" ]; then
  echo -e "  ${C_Y}[!]${C_0} platform ${PLAT:-?} ($(plat_name "${PLAT:-?}")): НЕ из проверенных — путь тот же (quasar),"
  echo "      но раскладка не сверялась. 1) проверь глазами вывод выше (разделы, /data/quasar/маркеры);"
  echo "      2) пришли владельцу стенда recon: bash $0 --backup (архив tar.gz) — сверю и добавлю в проверенные."
fi
fi   # шаг 7 — для run|shell|backup (остальные режимы подключаются сами в своих блоках)

# ----------------------------------------------------------------------------
# МАСТЕР (только --run + терминал): закрепляем рут и доводим юзера до конца
if [ "$MODE" = run ] && [ "$INTERACTIVE" = 1 ] && [ "${ROOT_OK:-0}" = 1 ]; then
  step "МАСТЕР: закрепить рут (fortress + OTA-запрет)"
  echo "  Рут есть. Чтобы он пережил конфиг Яндекса и прошивку, ставят два тоггла:"
  echo "   • fortress (deactivate -> no-op + rootkeeper) — Яндекс не снимет adbd конфигом;"
  echo "   • OTA-запрет (--ota-off) — Яндекс вообще не сможет обновить колонку."
  read -rp "   Поставить оба сейчас? [Enter=да / n=пропустить]: " m || exit 0
  if [ "$m" = "n" ] || [ "$m" = "N" ]; then
    info "пропущено — позже: пункты меню 3 и 4 (или bash $0 --fortress / --ota-off)"
  else
    echo;  COLUMN_IP="$COLUMN_IP" ROUTER="$ROUTER" bash "$0" --fortress
    echo;  COLUMN_IP="$COLUMN_IP" ROUTER="$ROUTER" bash "$0" --ota-off
  fi
  echo;  COLUMN_IP="$COLUMN_IP" ROUTER="$ROUTER" bash "$0" --status
  step "МАСТЕР: финал — убери DNS-запись с роутера"
  echo -e "  ${C_G}Рут и защита теперь живут НА КОЛОНКЕ${C_0} (маркеры /data/quasar + fortress + OTA-запрет):"
  echo "  DNS-DNS-запись $QUASAR_HOST -> $LOCAL_IP и наш прокси больше НЕ нужны."
  echo
  echo -e "  ${C_W}▸ ГАЙД — удали статическую запись на роутере $ROUTER (любой ОДИН вариант):${C_0}"
  echo -e "   ${C_G}Keenetic:${C_0}  no ip host $QUASAR_HOST $LOCAL_IP   (потом: system configuration save)"
  echo -e "   ${C_G}MikroTik:${C_0}  /ip dns static remove [find name=$QUASAR_HOST]"
  echo -e "   ${C_G}OpenWrt:${C_0}   uci del_list dhcp.@dnsmasq[0].address=/$QUASAR_HOST/$LOCAL_IP ; uci commit dhcp"
  echo -e "   ${C_G}pi-hole:${C_0}   Local DNS -> DNS Records -> удали $QUASAR_HOST"
  echo -e "   ${C_G}dnsmasq:${C_0}   удали строку address=/$QUASAR_HOST/... и рестартни dnsmasq"
  echo
  echo -e "  ${C_B}Проверка после:${C_0} dig +short $QUASAR_HOST @$ROUTER — вернётся настоящий IP Яндекса."
  echo "  Что будет с рутом без записи — разбор: пункт меню 8 (bash $0 --router-off)."
  echo "  OTA-щит на роутере (пункт 5) можно оставить — не мешает; снимается так же."
  echo
  ! is_tested "${PLAT:-plum}" && [ "${PLAT:-plum}" != "?" ] && \
    echo -e "  ${C_Y}[!]${C_0} НЕпроверенная платформа (${PLAT}): обязательно пришли recon — bash $0 --backup"
fi

# ----------------------------------------------------------------------------
if [ "$MODE" = backup ]; then
  step "Бэкап/разведка (recon) колонки — для сверки незнакомых прошивок (Мини 3 Про и др.)"
  D="$Y/backup_${PLAT:-unknown}_$(date +%Y%m%d_%H%M%S)"; mkdir -p "$D"
  { echo "platform: ${PLAT:-?}   device_id: ${COL_ID:-?}   колонка: $COLUMN_IP";
    echo "дата: $(date)   ПК: $(hostname 2>/dev/null) $LOCAL_IP"; } > "$D/_info.txt"
  for c in "cat /proc/mtd" "cat /proc/cmdline" "cat /proc/partitions" "uname -a" \
           "cat /etc/os-release" "mount" \
           "cat /system/vendor/quasar/quasar.cfg" "cat /system/vendor/quasar/launch.json" \
           "cat /system/vendor/quasar/VERSION; cat /system/vendor/quasar/version_full" \
           "cat /system/vendor/quasar/adb_options.inc 2>/dev/null; cat /system/vendor/quasar/onboot.sh 2>/dev/null" \
           "ls -la /system/vendor/quasar/" "ls -la /data/quasar/" \
           "md5sum /system/vendor/quasar/activate_adb.sh /system/vendor/quasar/adb_options.inc /system/vendor/quasar/onboot.sh /system/vendor/quasar/device_ota_update.sh 2>/dev/null" \
           "fw_printenv 2>/dev/null" "busybox --list 2>/dev/null" \
           "ps -w 2>/dev/null | head -40" "netstat -tln 2>/dev/null | head -25" \
           "dmesg 2>/dev/null | tail -40"; do
    "$ADB" -s "$COLUMN_IP:$ADB_PORT" shell "$c" >"$D/$(echo "$c" | tr -cd 'a-zA-Z0-9._' | cut -c1-44).txt" 2>&1
  done
  tar -C "$Y" -czf "$D.tar.gz" "$(basename "$D")" && rm -rf "$D"
  echo -e "  ${C_G}Бэкап/recon: $D.tar.gz${C_0}"
  echo "  Внутри: quasar.cfg (весь), adb_options.inc, onboot.sh, mtd/cmdline, env, md5,"
  echo "  ps/netstat/dmesg — всё, что нужно для сверки раскладки чужой прошивки."
  ! is_tested "${PLAT:-plum}" && [ "${PLAT:-plum}" != "?" ] && echo -e "  ${C_Y}[!]${C_0} на этой колонке (${PLAT}) — пришли архив владельцу стенда."
fi

# ----------------------------------------------------------------------------
# FORTRESS: вшить самозащиту рута в прошивку (нужен уже работающий ADB-руТ)
# ----------------------------------------------------------------------------
A="$ADB -s $COLUMN_IP:$ADB_PORT"
QDIR=/system/vendor/quasar
MARK=/data/quasar/activate_adb_quasmodrom
MARKN=/data/quasar/activate_network_adb
adb_reconnect() {   # adbd колонки периодически уходит в offline — переподключаемся сами
  "$ADB" disconnect >/dev/null 2>&1
  sleep 1
  "$ADB" connect "$COLUMN_IP:$ADB_PORT" >/dev/null 2>&1
}

if [ "$MODE" = fortress ]; then
  step "FORTRESS: вшиваю самозащиту рута в прошивку"
  adb_reconnect
  $A shell id 2>/dev/null | grep -q uid=0 || { bad "нет ADB-рута — сначала прогони без флагов"; exit 1; }
  TMPD=$(mktemp -d)
  R="adb-remote-check"

  # проверка существования файла НА КОЛОНКЕ — только по выводу, exit-code adb не доверяем
  rexist() { [ "$($A shell "test -f $1 && echo Y || echo N" 2>/dev/null | tr -d '\r\n')" = "Y" ]; }
  ensure_keeper() {   # keeper должен КРУТИТЬСЯ всегда; adbd-разрыв его прибивает —
    $A shell 'pid=$(ps | grep rootkeeper | grep -v grep | awk "{print \$1}" | head -1); [ -n "$pid" ] && echo "keeper:RUNNING pid=$pid" || { setsid sh /data/quasar/rootkeeper.sh >/dev/null 2>&1 < /dev/null & sleep 1; ps | grep rootkeeper | grep -v grep | head -1 | awk "{print \"keeper:RESTORED pid \"\$1}"; }' 2>/dev/null | tr -d '\r' | head -1
  }

  # defensive: на НЕпроверенных прошивках (Мини 3 Про и др.) layout может отличаться —
  # не трогаем ничего, пока не убедились, что скрипт activate_adb.sh лежит там же.
  if ! rexist "$QDIR/activate_adb.sh"; then
    bad "нет $QDIR/activate_adb.sh — раскладка этой прошивки (${PLAT:-?}) отличается"
    echo -e "  ${C_Y}Ничего не трогаю. Пришли владельцу стенда recon: bash $0 --backup"
    echo "  (tar.gz с quasar.cfg, adb_options.inc, mtd, env) — сделаю адаптацию под эту модель.${C_0}"
    exit 1
  fi
  if ! is_tested "${PLAT:-plum}" && [ "${PLAT:-plum}" != "?" ]; then
    echo -e "  ${C_Y}[!]${C_0} platform ${PLAT}: activate_adb.sh+adb_options.inc общие у quasar (по реверсу),"
    echo "      но обёртка на этой модели НЕ проверялась — после шагов прогони --status."
  fi

  info "1) rootfs в rw"
  $A shell 'mount -o remount,rw / && mount | grep " / "'

  info "2) обёртка activate_adb.sh: deactivate -> no-op (maind больше не может убить adbd)"
  if rexist "$QDIR/activate_adb.sh.orig"; then
    $A shell "chmod 755 $QDIR/activate_adb.sh.orig"   # страховка: .orig обязан быть исполняемым
    ok "оригинал уже сохранён: activate_adb.sh.orig"
  else
    $A shell "cp $QDIR/activate_adb.sh $QDIR/activate_adb.sh.orig && chmod 755 $QDIR/activate_adb.sh.orig"
    rexist "$QDIR/activate_adb.sh.orig" && ok "оригинал сохранён: activate_adb.sh.orig (755)" \
      || { bad "не забэкапился — СТОП, ничего не трогаю"; exit 1; }
  fi
  if rexist "$QDIR/activate_adb.sh" && [ "$($A shell "head -2 $QDIR/activate_adb.sh" 2>/dev/null | grep -c FORTRESS)" != "0" ]; then
    ok "обёртка уже стоит"
  else
     cat > "$TMPD/wrap.sh" <<'WRAP'
#!/bin/sh
# FORTRESS-WRAPPER v2: 'deactivate' -> no-op (maind не может убить adbd),
# 'activate' -> всегда с --network (иначе оригинал снял бы network-маркер).
case " $* " in
  *" deactivate "*)
      echo "`date`: FORTRESS: deactivate blocked" >> /data/quasar/quasar_adb.log
      exit 0
      ;;
esac
case " $* " in
  *" activate "*)
      case " $* " in
        *" --network "*) : ;;
        *) set -- "$@" --network ;;
      esac
      ;;
esac
exec /system/vendor/quasar/activate_adb.sh.orig "$@"
WRAP
     $A push "$TMPD/wrap.sh" /data/local/tmp/wrap.sh >/dev/null 2>&1
     $A shell "cp /data/local/tmp/wrap.sh $QDIR/activate_adb.sh && chmod 755 $QDIR/activate_adb.sh"
     [ "$($A shell "head -3 $QDIR/activate_adb.sh" 2>/dev/null | grep -c FORTRESS-WRAPPER)" != "0" ] \
       && ok "обёртка v2 установлена (deactivate -> no-op; activate -> всегда --network)" \
       || { bad "обёртка не записалась"; exit 1; }
  fi

  info "3) rootkeeper.sh в /data/quasar (rw-раздел — переживает OTA)"
  cat > "$TMPD/keeper.sh" <<'KEEP'
#!/bin/sh
# FORTRESS-KEEPER v3: сторож в цикле — маркеры + adbd именно в СЕТЕВОМ режиме.
# v3: проверяем не pidof, а LISTEN на :5555 (0x15B3) — иначе USB-only adbd
# (маркер network снят -> check -> adbd без сети) остался бы незамеченным.
M=/data/quasar/activate_adb_quasmodrom
N=/data/quasar/activate_network_adb
listening() { awk '$2 ~ /:15B3$/ && $4=="0A" {f=1} END{exit !f}' /proc/net/tcp 2>/dev/null; }
while :; do
  [ -f "$M" ] || touch "$M"
  [ -f "$N" ] || touch "$N"
  if ! listening; then
    pidof adbd >/dev/null 2>&1 && kill $(pidof adbd) 2>/dev/null
    sleep 1
    start-stop-daemon -S -b -x /usr/bin/adbd 2>/dev/null
  fi
  sleep 60
done
KEEP
  $A push "$TMPD/keeper.sh" /data/quasar/rootkeeper.sh >/dev/null 2>&1 \
    || $A shell "mkdir -p /data/quasar" >/dev/null 2>&1
  $A shell "chmod 755 /data/quasar/rootkeeper.sh"    # adb push тянет права локального файла (644) — чиним
  [ "$($A shell "test -x /data/quasar/rootkeeper.sh && echo Y || echo N" 2>/dev/null | tr -d '\r\n')" = "Y" ] \
    && ok "rootkeeper.sh v3 стоит в /data/quasar (переживает OTA; чинит и USB-only adbd)" \
    || { bad "keeper не записался/не исполняемый в /data/quasar"; }

  info "4) автозапуск keeper из onboot.sh (переживёт ребут)"
  if [ "$($A shell "grep -c rootkeeper $QDIR/onboot.sh" 2>/dev/null | tr -d '\r\n')" != "0" ]; then
    ok "уже прописан в onboot.sh"
  else
    $A shell "echo '[ -x /data/quasar/rootkeeper.sh ] && /data/quasar/rootkeeper.sh >/dev/null 2>&1 &' >> $QDIR/onboot.sh"
    [ "$($A shell "grep -c rootkeeper $QDIR/onboot.sh" 2>/dev/null | tr -d '\r\n')" != "0" ] \
      && ok "строка автозапуска в конце onboot.sh" \
      || bad "не дописалось в onboot.sh"
  fi
  ok "$(ensure_keeper)"

  info "5) ТЕСТ: деактивация блокируется, маркеры и adbd живы"
  $A shell "$QDIR/activate_adb.sh --quasmodrom deactivate >/dev/null 2>&1; echo wrapper_exit=\$?"
  $A shell "test -f $MARK && test -f $MARKN && echo '        маркеры: ЖИВЫ' || echo '        маркеры: ПОТЕРЯНЫ!'"
  $A shell "pidof adbd >/dev/null && echo '        adbd: ЖИВ' || echo '        adbd: МЁРТВ'"
  $A shell "$QDIR/activate_adb.sh --quasmodrom activate --network >/dev/null 2>&1; echo '        activate через обёртку: exit='\$?"

  info "6) rootfs назад в ro + уборка"
  $A shell 'rm -f /data/local/tmp/wrap.sh /data/local/tmp/noota.sh; mount -o remount,ro / 2>/dev/null; mount | grep " / "'
  rm -rf "$TMPD"

  echo -e "\n  ${C_G}FORTRESS (adbd) стоит.${C_0}"
  echo "   • deactivate от конфига → no-op: adbd не убьётся, маркеры не удалятся."
  echo "   • при ребуте: onboot.sh → check → маркеры живы → adbd поднимется; keeper подстрахует."
  echo -e "  ${C_Y}Честные границы:"
  echo "   • патчи в /system живут до ближайшей OTA (см. --ota-off — блокирует и её)."
  echo "   • полный сброс кнопкой сносит /data — тогда весь путь заново (стенд готов).${C_0}"
fi

# ----------------------------------------------------------------------------
# ТОГГЛ OTA: --ota-off (заблокировать прошивку) / --ota-on (вернуть)
# ----------------------------------------------------------------------------
if [ "$MODE" = otaoff ] || [ "$MODE" = otaon ]; then
  [ "$MODE" = otaoff ] && step "OTA-OFF: запрещаю Яндексу обновлять колонку" \
                       || step "OTA-ON: разрешаю Яндексу обновлять колонку"
  adb_reconnect
  $A shell id 2>/dev/null | grep -q uid=0 || { bad "нет ADB-рута — сначала прогони без флагов"; exit 1; }
  TMPD=$(mktemp -d)
  rexist() { [ "$($A shell "test -f $1 && echo Y || echo N" 2>/dev/null | tr -d '\r\n')" = "Y" ]; }

  ensure_keeper() {   # keeper должен КРУТИТЬСЯ всегда; adbd-разрыв его прибивает —
    $A shell 'pid=$(ps | grep rootkeeper | grep -v grep | awk "{print \$1}" | head -1); [ -n "$pid" ] && echo RUNNING || { setsid sh /data/quasar/rootkeeper.sh >/dev/null 2>&1 < /dev/null & sleep 1; ps | grep rootkeeper | grep -v grep | head -1 | awk "{print \"RESTORED pid \"\$1}"; }' 2>/dev/null | tr -d '\r' | head -1
  }
  # --- приёмы yandex-micro-kit: подмена ключа подписи OTA (ota_script.pub) и
  #     вырезание updatesd из launch.json (апдейтер не стартует при загрузке).
  #     Проверки их же: баланс скобок JSON + sanity-флаги; при провале launch.json НЕ трогаем.
  cat > "$TMPD/kit_ota.sh" <<'KIT'
#!/bin/sh
Q=/system/vendor/quasar
LJ="$Q/launch.json"; PUB="$Q/ota_script.pub"
DUMMY_PUB='-----BEGIN RSA PUBLIC KEY-----
MIIBCgKCAQEAjQ29hAZE6u/Tly4UA7VIrzN+TweZM0nWT6L0HkYR2evczsdMPhZL
NZxzoZYt5ogstEMydG20xAs9m/k6qZ65sDiLkO9uNaGkR7JUn4+/0bOgJ7MWqgkU
hduLRgRsTQeOjuS16rjwnBxo5vwmb6P2cH+pof9Mri0IOHMuquQzF17YD7vxVIg8
w0c9FEyuoxc0DuedVVU2E9paPp5BkaUtkAqbz7hGzfij9aH8FBg/AkY5CKsFrdPo
gyVfA4qg84zQ7Y7PT7i1WSxPk4ShFtwYO5hYaEiZ+fCsf8Pp8Nd8RSa2kuObXE2X
Qt5QqXHjDoI9jyBgx7igBWAKHvQBwix/AwIDAQAB
-----END RSA PUBLIC KEY-----'
json_ok(){
  f="$1"; [ -s "$f" ] || return 1
  a=$(tr -cd '{' <"$f"|wc -c); b=$(tr -cd '}' <"$f"|wc -c)
  c=$(tr -cd '[' <"$f"|wc -c); d=$(tr -cd ']' <"$f"|wc -c)
  [ "$a" = "$b" ] && [ "$c" = "$d" ] || return 1
  grep -q -- '--mediad' "$f" && grep -q -- '--platformd' "$f" || return 1
  grep -q -- '--updatesd' "$f" && return 1
  return 0
}
strip_updatesd(){
  awk '
    { lines[NR]=$0 }
    /--updatesd/ { key=NR }
    END{
      if (key==0){ for(i=1;i<=NR;i++) print lines[i]; exit }
      s=0; for(i=key;i>=1;i--){ if(lines[i] ~ /^[[:space:]]*\{[[:space:]]*$/){ s=i; break } }
      e=0; for(i=key;i<=NR;i++){ if(lines[i] ~ /^[[:space:]]*\},?[[:space:]]*$/){ e=i; break } }
      if (s==0 || e==0){ exit }
      for(i=1;i<=NR;i++){ if(i>=s && i<=e) continue; print lines[i] }
    }' "$1"
}
case "$1" in
  off)
    [ -f "$LJ.orig" ] || cp "$LJ" "$LJ.orig"
    [ -f "$PUB.orig" ] || cp "$PUB" "$PUB.orig"
    printf '%s\n' "$DUMMY_PUB" > "$PUB" && echo "pub=REPLACED"
    if grep -q -- '--updatesd' "$LJ"; then
      TMP=/tmp/.kit.$$
      strip_updatesd "$LJ" > "$TMP" 2>/dev/null
      if json_ok "$TMP"; then cat "$TMP" > "$LJ"; echo "updatesd_strip=OK"; echo "launch_json=VALID"
      else echo "updatesd_strip=SKIP_JSON_INVALID"; fi
      rm -f "$TMP"
    else echo "updatesd_strip=ALREADY_ABSENT"; fi
    ;;
  on)
    [ -f "$LJ.orig" ] && { cp "$LJ.orig" "$LJ"; echo "launch_json=RESTORED"; }
    [ -f "$PUB.orig" ] && { cp "$PUB.orig" "$PUB"; echo "pub=RESTORED"; }
    ;;
  status)
    grep -q -- '--updatesd' "$LJ" && echo "updatesd=PRESENT" || echo "updatesd=ABSENT"
    [ -f "$PUB.orig" ] || { echo "pub=NO_ORIG"; exit 0; }
    cmp -s "$PUB" "$PUB.orig" && echo "pub=ORIGINAL" || echo "pub=REPLACED"
    ;;
esac
KIT
  $A push "$TMPD/kit_ota.sh" /data/local/tmp/kit_ota.sh >/dev/null 2>&1
  kit(){ $A shell "sh /data/local/tmp/kit_ota.sh $1" 2>/dev/null | tr -d '\r'; }
  # defensive: у НЕпроверенных прошивок (Мини 3 Про и др.) OTA-механика может отличаться
  if [ "$MODE" = otaoff ] && ! rexist "$QDIR/device_ota_update.sh"; then
    bad "нет $QDIR/device_ota_update.sh — OTA-механика этой прошивки (${PLAT:-?}) отличается"
    echo -e "  ${C_Y}Ничего не трогаю. Пришли владельцу стенда recon: bash $0 --backup${C_0}"
    exit 1
  fi

  $A shell 'mount -o remount,rw / && mount | grep " / "'
  if [ "$MODE" = otaoff ]; then
    # сохранить оригинал (один раз)
    if rexist "$QDIR/device_ota_update.sh.orig"; then
      $A shell "chmod 755 $QDIR/device_ota_update.sh.orig"
      ok "оригинал уже сохранён: device_ota_update.sh.orig"
    else
      $A shell "cp $QDIR/device_ota_update.sh $QDIR/device_ota_update.sh.orig && chmod 755 $QDIR/device_ota_update.sh.orig"
      rexist "$QDIR/device_ota_update.sh.orig" && ok "оригинал сохранён: device_ota_update.sh.orig" \
        || { bad "не забэкапился — СТОП, ничего не трогаю"; exit 1; }
    fi
    if [ "$($A shell "head -2 $QDIR/device_ota_update.sh" 2>/dev/null | grep -c FORTRESS)" != "0" ]; then
      ok "no-op уже стоит"
    else
      cat > "$TMPD/noota.sh" <<'NOOTA'
#!/bin/sh
# FORTRESS-NO-OTA: прошивка по OTA запрещена (защита рута от сноса).
# Единственный вход в fw_setenv boot_to_recovery -> recovery -> swupdate.
# Ничего не применяем, скачанный OTA удаляем, чтобы не копить на диске.
rm -f "$1" 2>/dev/null
echo "`date`: FORTRESS: OTA update blocked ($1)" >> /data/quasar/quasar_adb.log
exit 0
NOOTA
      $A push "$TMPD/noota.sh" /data/local/tmp/noota.sh >/dev/null 2>&1
      $A shell "cp /data/local/tmp/noota.sh $QDIR/device_ota_update.sh && chmod 755 $QDIR/device_ota_update.sh"
      [ "$($A shell "head -2 $QDIR/device_ota_update.sh" 2>/dev/null | grep -c FORTRESS)" != "0" ] \
        && ok "device_ota_update.sh -> no-op: OTA ЗАБЛОКИРОВАН" \
        || { bad "no-op не записался"; exit 1; }
    fi
    $A shell 'rm -f /data/local/tmp/noota.sh'
    info "ключ подписи OTA (ota_script.pub) + updatesd из launch.json [yandex-micro-kit]"
    KOUT=$(kit off)
    echo "$KOUT" | grep -q "pub=REPLACED" \
      && ok "ключ подписи OTA подменён: любой апдейт не пройдёт проверку подписи" \
      || bad "pub не подменён"
    case "$KOUT" in
      *updatesd_strip=OK*)        ok "updatesd ВЫРЕЗАН из launch.json — апдейтер не стартует при загрузке" ;;
      *updatesd_strip=ALREADY_ABSENT*) ok "updatesd в launch.json уже нет" ;;
      *updatesd_strip=SKIP_JSON_INVALID*) bad "launch.json не прошёл проверку — оставлен как был (защита на no-op+ключе)" ;;
    esac
    $A shell 'rm -f /data/local/tmp/kit_ota.sh'
    ensure_keeper
    echo -e "\n  ${C_Y}Последствия:"
    echo "   • updatesd при загрузке НЕ стартует (вырезан из launch.json) — не качает OTA;"
    echo "     если всё же что-то дотянет: применение -> no-op, файл удаляется,"
    echo "     ребута в обновление нет. И даже прямой запуск апдейтера не пройдёт"
    echo "     проверку подписи — ключ ota_script.pub подменён. Колонка не может обновиться."
    if [ "${PLAT:-plum}" = "yandexmidi" ] || [ "${PLAT:-plum}" = "cucumber" ]; then
      echo "   • (Миди: вход в OTA — apply_packed_ota из этого же скрипта, без fw_setenv —"
      echo "     обёртка перекрывает и его. Переменной boot_to_recovery у Миди нет — это норма.)"
    elif ! is_tested "${PLAT:-plum}" && [ "${PLAT:-plum}" != "?" ]; then
      echo "   • [${PLAT}] у Amlogic (Мини 3 Про) Secure Boot и u-boot ENV под БЕЛЫМ СПИСКОМ:"
      echo "     переменные кроме indication_brightness/silent не меняются — тумблер"
      echo "     boot_to_recovery может отсутствовать; обёртка device_ota_update.sh -> no-op"
      echo "     остаётся ГЛАВНОЙ защитой: вход в recovery заблокирован самим скриптом."
    fi
    echo "   • патчи в /system (обёртка, keeper-строка, сам no-op) становятся бессмертными:"
    echo "     снести их может только OTA, а OTA заблокирован."
    echo "   • если захочешь обновиться: bash $0 --ota-on — колонка вернётся в штатный режим."
    echo "     (сначала bash $0 --fortress-off, чтобы обёртки не посчитались модификацией).${C_0}"
  else
    info "возврат ключа OTA + launch.json [yandex-micro-kit]"
    kit on
    $A shell 'rm -f /data/local/tmp/kit_ota.sh'
    if rexist "$QDIR/device_ota_update.sh.orig"; then
      $A shell "mv -f $QDIR/device_ota_update.sh.orig $QDIR/device_ota_update.sh && chmod 755 $QDIR/device_ota_update.sh"
      rexist "$QDIR/device_ota_update.sh" && [ "$($A shell "head -2 $QDIR/device_ota_update.sh" 2>/dev/null | grep -c FORTRESS)" = "0" ] \
        && ok "OTA РАЗБЛОКИРОВАН: device_ota_update.sh — оригинал" \
        || bad "что-то не вернулось — проверь руками"
    else
      bad "нет device_ota_update.sh.orig — блокировки OTA не было (нечего включать)"
    fi
  fi
  $A shell 'mount -o remount,ro / 2>/dev/null; mount | grep " / "'
  rm -rf "$TMPD"
fi

# ----------------------------------------------------------------------------
# СТАТУС: показать все тогглы
# ----------------------------------------------------------------------------
if [ "$MODE" = status ]; then
  step "СТАТУС тогглов"
  adb_reconnect
  if $A shell id 2>/dev/null | grep -q uid=0; then
    ok "ADB-рут: есть ($($A shell id 2>/dev/null | head -1))"
    W="[$($A shell "head -2 $QDIR/activate_adb.sh" 2>/dev/null | grep -c FORTRESS)]"
    O="[$($A shell "head -2 $QDIR/device_ota_update.sh" 2>/dev/null | grep -c FORTRESS)]"
    [ "${W//[^0-9]/}" != "0" ] && echo -e "  ${C_G}[*]${C_0} adbd-fortress: ВКЛ (обёртка стоит)" || echo -e "  ${C_Y}[ ]${C_0} adbd-fortress: выкл"
    [ "${O//[^0-9]/}" != "0" ] && echo -e "  ${C_G}[*]${C_0} OTA-блокировка: ВКЛ (Яндекс не может обновить)" || echo -e "  ${C_Y}[ ]${C_0} OTA-блокировка: выкл (штатные обновления)"
    $A shell "test -x /data/quasar/rootkeeper.sh && echo Y" 2>/dev/null | grep -q Y \
      && echo -e "  ${C_G}[*]${C_0} rootkeeper в /data: стоит" || echo -e "  ${C_Y}[ ]${C_0} rootkeeper: нет"
    $A shell "grep -c rootkeeper $QDIR/onboot.sh" 2>/dev/null | tr -d '\r' | grep -qv "^0$" \
      && echo -e "  ${C_G}[*]${C_0} автозапуск keeper в onboot.sh: прописан" || echo -e "  ${C_Y}[ ]${C_0} автозапуск keeper: нет"
    KEEP=$($A shell 'pid=$(ps | grep rootkeeper | grep -v grep | awk "{print \$1}" | head -1); [ -n "$pid" ] && echo "$pid" || echo NONE' 2>/dev/null | tr -d '\r')
    [ "$KEEP" != "NONE" ] \
      && echo -e "  ${C_G}[*]${C_0} keeper-ПРОЦЕСС: крутится (pid $KEEP)" \
      || echo -e "  ${C_Y}[!]${C_0} keeper-ПРОЦЕСС: НЕ КРУТИТСЯ (рут жив, но сторожа нет — запусти --fortress)"
    btr=$($A shell "fw_printenv boot_to_recovery 2>&1" 2>/dev/null | tr -d '\r')
    case "$btr" in
      *boot_to_recovery=1*) echo -e "  ${C_R}!!${C_0} boot_to_recovery=1 — при ребуте уйдёт в recovery (см. логику OTA!)" ;;
      *boot_to_recovery=*)  echo -e "  ${C_G}[*]${C_0} boot_to_recovery=0 (в норме)" ;;
      *) echo -e "  ${C_B}[..]${C_0} boot_to_recovery: нет такой переменной (на части моделей, напр. Миди, вход в OTA иначе — норма)" ;;
    esac
    UPDS=$($A shell "grep -c -- '--updatesd' $QDIR/launch.json 2>/dev/null" | tr -d '\r')
    [ "${UPDS:-1}" = "0" ] \
      && echo -e "  ${C_G}[*]${C_0} updatesd в launch.json: ВЫРЕЗАН (апдейтер не стартует при загрузке)" \
      || echo -e "  ${C_Y}[ ]${C_0} updatesd в launch.json: жив (штатно)"
    PUBS=$($A shell "if [ ! -f $QDIR/ota_script.pub.orig ]; then echo NOORIG; elif cmp -s $QDIR/ota_script.pub $QDIR/ota_script.pub.orig; then echo EQ; else echo DIFF; fi" 2>/dev/null | tr -d '\r')
    case "$PUBS" in
      DIFF) echo -e "  ${C_G}[*]${C_0} ключ подписи OTA (ota_script.pub): подменён — апдейт не пройдёт проверку" ;;
      EQ)   echo -e "  ${C_Y}[ ]${C_0} ключ подписи OTA: штатный" ;;
      *)    echo -e "  ${C_DIM}[..]${C_0} ключ подписи OTA: нет .orig (не блокировался этой версией)" ;;
    esac
  else
    bad "ADB-рута нет — запусти скрипт без флагов (или проверь :5555)"
    timeout 2 bash -c "echo > /dev/tcp/$COLUMN_IP/$ADB_PORT" 2>/dev/null \
      && echo -e "  ${C_Y}порт :5555 открыт — просто adbd ушёл в offline, попробуй ещё раз${C_0}" \
      || echo -e "  ${C_Y}порт :5555 закрыт — нужен DNS-DNS-запись + прокси + ребут колонки (гайд выше)${C_0}"
  fi
fi

if [ "$MODE" = fortressoff ]; then
  step "FORTRESS-OFF: снимаю самозащиту adbd (OTA-тоггл не трогаю — он отдельный)"
  adb_reconnect
  $A shell id 2>/dev/null | grep -q uid=0 || { bad "нет ADB-рута"; exit 1; }
  $A shell 'mount -o remount,rw / && mount | grep " / "'
  $A shell "[ -f $QDIR/activate_adb.sh.orig ] && { mv -f $QDIR/activate_adb.sh.orig $QDIR/activate_adb.sh && echo 'activate_adb restored'; } || echo 'нет .orig — обёртки не было'"
  $A shell "pkill -f rootkeeper.sh 2>/dev/null; sed -i '/rootkeeper/d' $QDIR/onboot.sh && rm -f /data/quasar/rootkeeper.sh && echo 'keeper убран (процесс+автозапуск)'"
  $A shell 'mount -o remount,ro / && mount | grep " / "'
  ok "снято: deactivate снова работает как у Яндекса. OTA-тоггл: bash $0 --ota-on / --ota-off"
fi

# ----------------------------------------------------------------------------
# EXPORT: переносный tar.gz стенда — передать знакомому (например, с Мини 3 Про)
# ----------------------------------------------------------------------------
if [ "$MODE" = export ]; then
  step "EXPORT: собираю переносный комплект стенда (работает на чужом ПК без этой машины)"
  need=""
  for f in station-root.sh server.py rules.json chain.pem leaf.key; do
    [ -s "$BASE/$f" ] || { bad "нет $BASE/$f"; need="$need $f"; }
  done
  [ -n "$need" ] && { bad "стенд неполный:$need — сначала добей его здесь (bash $0)"; exit 1; }
  OUT="$HOME/yndx_export_$(date +%Y%m%d_%H%M%S).tar.gz"
  TMPD=$(mktemp -d)
  mkdir -p "$TMPD/yndx/mitm" "$TMPD/yndx/tools/platform-tools" "$TMPD/yndx/roots"
  cp "$BASE/station-root.sh" "$BASE/server.py" "$BASE/rules.json" "$BASE/chain.pem" "$BASE/leaf.key" "$TMPD/yndx/mitm/"
  [ -s "$Y/roots/aaa_root.pem" ] && cp "$Y/roots/aaa_root.pem" "$TMPD/yndx/roots/" \
    && ok "roots/aaa_root.pem приложен (на чужом ПК цепочка проверится офлайн)"
  if [ -x "$ADB" ]; then
    cp "$ADB" "$TMPD/yndx/tools/platform-tools/" && ok "adb приложен ($(du -h "$ADB" | awk '{print $1}')) — качать не придётся"
  else
    info "adb бинаря нет — на чужом ПК скрипт скачает platform-tools сам (нужен unzip/7z)"
  fi
  cat > "$TMPD/yndx/README-START.md" <<'RDM'
# Рут Яндекс Станции — стенд на ЭТОМ ПК (инструкция для проверяющего)

Что это: тот же автоматический стенд (серты quasar.yandex.net + прокси + меню).
Проверено живьём: Лайт 2 (plum) и Миди (YNDX-00054; в mDNS она platform=cucumber,
в quasar/OTA — yandexmidi). Миди отрутована на in-box 3.210.351.1.6: раскладка
совпала с разобранной OTA 0.369.1.50 (adbd-секция quasar.cfg та же,
adb_options.inc байт-в-байт) — путь тот же; fortress и OTA-запрет на ней стоят.
Пилот: Мини 3 Про (Amlogic A4, Secure Boot).

## Запуск
1. Пакеты (Debian/Ubuntu):   sudo apt install python3 openssl dnsutils curl avahi-utils unzip
   (curl/openssl обычно уже есть)
2. Распаковано: yndx/  →  запусти:   bash yndx/mitm/station-root.sh     (меню цифрами)
3. Если у ТЕБЯ другие адреса:  ROUTER=<ip роутера> COLUMN_IP=<ip колонки> bash yndx/mitm/station-root.sh
   (по умолчанию роутер 192.168.1.1, колонка ищется сама по mDNS)

## Порядок для чужой колонки (Мини 3 Про и др.) — шаги 0-8, мастер ведёт сам
0. ДО рута: пункт меню 5 (--ota-shield) — щит на РОУТЕРЕ от фикс-прошивки (команды
   печатает скрипт под Keenetic/MikroTik/OpenWrt/pi-hole; голос/синк не трогаем, стенд не нужен).
1. Пункт меню 2 (статус) — колонка видна? platform какая? (НЕ из проверенных — прошивка непроверенная)
2. Пункт меню 1 — МАСТЕР: рутирует шаг за шагом и ведёт до конца. Он печатает команды
   DNS-записи quasar.yandex.net -> <этот ПК> под ТВОЙ роутер (Keenetic/MikroTik/OpenWrt/
   pi-hole — ставить сам) и ЖДЁТ: поставь запись и нажми Enter (r — перепроверить, q — выход).
   Дальше мастер сам: поднимет прокси :443 -> попросит перегрузить колонку питанием -> ждёт :5555
   -> adb -> РУТ.
3. Если :5555 не открылся за 10 минут — пришли владельцу стенда:
   tail -100 yndx/mitm/mitm.log   (и файл yndx/mitm/server.out)
   → по логу видно, ходил ли syncd колонки в прокси и что он получил.
4. После рута мастер сам предложит fortress + OTA-запрет (по умолчанию ДА) и покажет статус.
   На НЕпроверенной прошивке они требуют, чтобы activate_adb.sh/device_ota_update.sh
   лежали там же (скрипт сам откажется и подскажет, если их нет на месте).
   Нюанс Миди (cucumber/yandexmidi): путь и файлы те же (проверено живьём); вход в OTA у неё —
   apply_packed_ota из device_ota_update.sh, БЕЗ fw_setenv boot_to_recovery — обёртка
   device_ota_update.sh -> no-op перекрывает и этот вход. Переменной boot_to_recovery
   у Миди нет — статус пишет «нет переменной», это норма.
   Нюанс Мини 3 Про: u-boot ENV под белым списком (только indication_brightness/silent) —
   тумблер boot_to_recovery может не сработать, но обёртка device_ota_update.sh -> no-op
   остаётся главной защитой.
5. СНАЧАЛА пришли владельцу стенда recon: пункт меню 6 (--backup, tar.gz) — для сверки
   раскладки чужой прошивки; fortress/OTA-запрет (пункты 3/4) — после его ответа, если
   мастер их не поставил.
6. Статус — пункт меню 2 (--status): fortress ВКЛ, OTA-блок ВКЛ, keeper стоит.
7. Убери DNS-запись с роутера (мастер напомнит команды): рут и защита теперь живут
   НА КОЛОНКЕ, DNS-DNS-запись и прокси не нужны. Разбор — пункт меню 8 (--router-off).
8. OTA-щит (шаг 0) можно оставить — не мешает; снимается удалением тех же записей.

## Защита от фикс-прошивки (важно для нерутованной колонки)
Пока колонка НЕ отрутована, Яндекс может сам обновить её по OTA и закрыть вектор.
Слои: (1) пункт меню 5 / --ota-shield — команды для ТВОЕГО роутера (Keenetic/MikroTik/
OpenWrt/pi-hole/dnsmasq): режем DNS-раздачу OTA-хостов (quasar.s3.yandex.net,
*.svc.cdn.yandex.net); голос/синк не страдают. (2) Пока наш прокси в цепи — он сам
подменяет /check_updates на {"hasUpdate":false} (rules.json: check_updates_mode=block;
но updatesd может ходить по прямым IP мимо прокси — главный слой всё равно --ota-off).
(3) После рута: --ota-off — самое сильное: no-op в device_ota_update.sh +
    подмена ключа подписи ota_script.pub + updatesd вырезается из launch.json.

## Приватность
- в комплекте СЕКРЕТНЫЙ ключ серта quasar.yandex.net (leaf.key): только для своей сети
  и своих устройств; по открытому каналу не пересылай без нужды;
- yndx/mitm/mitm.log может содержать OAuth-токен колонки — не публикуй.
RDM
  tar -C "$TMPD" -czf "$OUT" yndx
  rm -rf "$TMPD"
  ok "готово: $OUT ($(du -h "$OUT" | awk '{print $1}'))"
  echo "  Внутри: yndx/mitm (station-root.sh+server.py+серты+rules.json),"
  echo "         yndx/tools/platform-tools/adb (если был), yndx/roots/aaa_root.pem (если был),"
  echo "         yndx/README-START.md — инструкция для проверяющего."
  echo -e "  ${C_Y}Передай архив тому, кто будет проверять стенд на своём ПК:"
  echo "  распакует -> bash yndx/mitm/station-root.sh -> пункт меню 1 (МАСТЕР, ведёт до конца),"
  echo "  потом пришлёт recon (пункт 6).${C_0}"
fi

echo -e "\n${C_Y}Подсказка: справка по всем командам — пункт меню 9 или bash $0 --help${C_0}"
[ "$MODE" = shell ] && exec "$ADB" -s "$COLUMN_IP:$ADB_PORT" shell
exit 0