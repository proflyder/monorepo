# Хронология и решения

## Предыдущая сессия (Codex, 21–22 октября 2026)

Полный транскрипт: `artifacts/prior-session/codex-session-01a15f1b-...md`.

1. Разобран архив `alice-mitm-bootstrap-2026-10-21.tar.gz` (eQuasar-подход:
   DNS-подмена `quasar.yandex.net`, MITM с сертификатом eMudhra).
2. Проверены сертификаты, поднят Bun-прокси и DNS, колонка
   (`192.0.2.13`, MAC `02:00:00:00:00:02`) получила fake-sync.
3. Получен ADB, установлена локальная CA, настроен stage 2 (uniproxy).
4. Затем Яндекс выпустил патч; после обновления прошивки ADB пропал.
   Причина по текущему разбору: патч убрал доверие к eMudhra-цепочке
   (и, возможно, изменил ADB-логику).
5. Попытки восстановления в той сессии остановились на обсуждении сброса.

## Текущая сессия (23 октября 2026)

1. Заново развёрнут `alice-mitm-bootstrap` (stage 1, Bun-прокси + DNS),
   проверена цепочка и fake-sync.
2. **Сброс колонки кнопкой Mute** (официальная процедура Миди).
   Восстановилась заводская прошивка `0.7.1.24...factory-7` (2024) и
   доверие к eMudhra. Получен root ADB.
3. Собран дамп `station.pcap`: до сброса колонка присылала
   `fatal alert 48 (unknown_ca)`; после сброса TLS проходит.
4. Найден **безопасный апдейт**: в `v3rt3p/quasar-server` файл
   `updates-config.yml` содержит «latest working versions with bad certificates».
   Для `platform=cucumber` — `0.369.1.46.4778262955.20261013.160`
   (13.10.2026, до патча). Колонка скачала официальный ZIP `cucumber` (CRC32 `3103154577`); после установки файл удалён из `/ota`.
5. Развёрнут стек `v3rt3p`: `quasar-server`, `https-proxy`, `processor`,
   `quasistreaming-gigam`, `supertonic-tts`, Postgres; скачаны модели.
6. Колонка переключена на стек (`switch-to-stack.sh`): DNS на все домены
   Яндекса, `https-proxy` на 80/443. Через штатный OTA колонка обновилась
   до `0.369.1.46...160`; ADB сохранился (конфиг стека:
   `adbd.remoteActivation=true`, `network=true`).
7. Установлен SSH (dropbear, ARM32 static, работает на aarch64 через compat),
   root-пароль из `secrets/CREDENTIALS.md`, init-скрипт `S99dropbear`, проверен ребут.
8. Диагностика «фиолетово-бирюзовой» анимации: это индикация обработки
   команды (Alice Plus), колонка ждёт ответа. Причины: `401` от LLM (нет ключа)
   и падение STT (`ten-vad` не поддерживает Linux aarch64). STT пропатчен,
   LLM-ключ ещё не задан.
9. **Поставлены OTA-запрет и fortress** (23.10, 20:15): обёртка
   `activate_adb.sh`, keeper v4 в `/data/quasar`, no-op
   `device_ota_update.sh`, dummy `ota_script.pub`, `updatesd` вырезан из
   `launch.json`. Проверено: deactivate блокируется, OTA не применяется,
   adbd восстанавливается keeper'ом после kill, SSH/ADB переживают ребут.
10. **Полный реальный Яндекс**: `DNS_NAMES` очищен, Bun-прокси и stack
    `https-proxy` остановлены; станция ходит в настоящий Яндекс. Проверено:
    Яндекс.Музыка и Яндекс.Лента работают, root/ADB/SSH/OTA-запрет живы.
    **(Исторический шаг; позже заменён карантином, шаг 11.)**
11. **Яндекс-карантин**: после сообщений о «пакете сброса вместо времени»
    поднят локальный DNS + `https-proxy`; на колонке `iptables` INPUT/OUTPUT
    режет Яндексовые сети, `unbound` → Mac, chrony → Google, `/etc/hosts` блок.
    Защита в `/data/quasar/yandex-block.sh`, вызывается из onboot/keeper;
    проверена после ребута. Реальных соединений с Яндексом нет.
12. **OTA/телеметрия**: `ALLOW_UPDATES=false`; s3/cdn в `https-proxy` → `403`;
    `telemetry` удалён из `launch.json`; `clck/report/startup/quasar_metrics` → `204`.
    Проверено после ребута: телеметрия не стартует, `/check_updates` →
    `hasUpdate:false`, ADB/SSH/OTA живы.
13. **API push найден**: `quasar-server` публикует `:31116` без auth;
    `POST /devices/:duid/push-directive` и `PATCH /devices/:duid` могут
    отправить станции директиву/конфиг. Предложено bind `127.0.0.1`; не сделано.
14. **Ресурсы**: изучены pulsar, apkg, yaff, spotiya, ya-station-root-shell,
    AirPlay, Spotify-скрипт, yse. Клонированы в `resources/`; матрица —
    `docs/RESOURCES.md`; ничего не установлено. `spotify.sh` битый, `yse`
    без исходника, `spotiya` только Mini 2, AirPlay тестирован на Mini 3.
    (На момент аудита guard был привязан к `192.0.2.0/24`; позже расширен.)
15. **Factory/recovery**: factory-образа Midi `cucumber` нет; `yandexmidi`
    `.298` — Station 2, не Midi. Raw-дампа `recovery`/`aml_sysrecovery` нет;
    factory reset опасен (снесёт `/data`/root/OTA). Дамп не делали.
16. **Egress-guard**: на станции `EGRESS_GUARD` — LAN/loopback/DHCP/Google NTP,
    остальной WAN DROP. Закреплён в `onboot.sh` после `yio_iptables_init.sh` и в
    keeper; проверен reboot и выключением Mac DNS/proxy. ADB/SSH/LAN API живы,
    Яндекс и `1.1.1.1` блокируются. Откат — `egress-guard-off.sh`.

17. **Provisioning audit**: Wi-Fi AP поддерживается, но hostapd/DHCP нет;
    USB UDC/configfs есть (`rndis`/`ncm`/`ffs.adb`), data-порт не проверен;
    текущий guard привязан к `192.0.2.0/24`. План — `docs/PROVISIONING.md`.

18. **DPP проверен**: `wpa_supplicant 2.10` поддерживает Wi-Fi Easy Connect;
    `dpp_bootstrap_gen` с сохранённым DER-ключом даёт стабильный QR
    (`pkhash`/URI не меняются). Нужно `dpp_config_processing=2`, boot-скрипт,
    `dpp_listen` и тест с телефоном; это лучший аналог Yandex BLE-setup.

19. **Sound pairing проверен**: `quasar-connect-bot/pairing.py` — офлайн
    FSK-генератор, 15/15 тестов; Telegram не нужен. На живой Midi не проверен;
    нужен режим сопряжения, рандомный токен только для Wi-Fi. Резервный способ.

20. **DPP + sound внедрены (подготовка)**: guard расширен на private LAN;
    DPP DER-ключ/QR/`dpp_config_processing=2`/boot-скрипт установлены, reboot и
    стабильность QR проверены. Локальный sound-генератор WAV/OGG готов, тесты
    15/15. Осталось: скан QR телефоном и прогон звука в режиме сопряжения.

21. **DPP проверка**: DPP-QR распознаётся телефоном, но `wlan0` драйвер
    не даёт `remain_on_channel` (`vm_p2p_support==0`), поэтому DPP при активной
    сети не завершается. `p2p0` off-channel работает, но роняет `wlan0`;
    восстановили reboot. DPP отложен до unassociated + USB/fallback.

## Принятые решения

| Решение | Причина |
|---|---|
| Сброс на заводскую, а не борьба с патчем | Восстанавливает доверие к eMudhra и ADB без разборки |
| Обновление на `0.369.1.46...160` через OTA стека | Официальная подписанная прошивка из списка безопасных; не ручная накатка |
| Переход на стек v3rt3p | Полный эмулятор бэкенда + LLM/TTS/STT, а не только fake-sync |
| SSH по паролю | Быстрый доступ; ключи и отключение пароля — позже |
| OTA-запрет/fortress после обновления | Сначала нужно было обновиться; установлено 23.10, см. `docs/OTA-FORTRESS.md` |
| Полный реальный Яндекс | Яндекс.Музыка/Лента требуют настоящий `quasar`/`glagol`; root/OTA от DNS не зависят |
| Яндекс-карантин | После угрозы удалённого сброса/закрытия ADB: DNS+iptables, локальный прокси, без облака |
| Телеметрия/OTA вырезаны | `telemetry` удалён, s3/cdn `403`, `ALLOW_UPDATES=false`; проверено ребутом |
| Внешние ресурсы не ставим | Нет проверенной совместимости/HA/безопасности; только `resources/` для теста |
| Egress-guard вместо DNS-off | DNS можно выключить/вернуть, но блокировка по IP не зависит от резолвера; доступ по LAN сохранён |

## Что НЕ делали

- Не ставили модифицированную прошивку (подпись/шифрование/verified boot).
- OTA-запрет поставлен на самой колонке (23.10).
- Не настраивали LLM и не проверяли голосовые команды на кастомном бэкенде.
- `/etc/hosts` колонки трогали: блок `YANDEX-BLOCK` — часть карантина.
