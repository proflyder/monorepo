# OPERATIONS — ежедневные команды

Факты — `STATE.md`; K3s — `KUBERNETES-OPERATIONS.md`; сеть — `NETWORK.md`;
секреты и доступы — `SECRETS.md`, `WORKSTATION.md`. Подробные команды
модуля — в его README (таблица ниже); здесь только общие точки входа и то,
чего нет в README.

## Единая точка входа: `mise tasks`

`mise tasks` перечисляет локальные задачи. Деплоимые юниты и процедура отката
живут в `platform/ci/units.mjs` (`deploy` и `rollback`), список —
`mise run units`.

```bash
# пересчитать и запинить digest'ы (gh workflow run deploy-home-manual.yml
# --ref develop -f units=…); применяет Flux, в кластер из CI ничего не катится
mise run deploy scheduler --dry-run
mise run deploy scheduler,ha-bridge --yes --watch
mise run deploy all --yes

mise run deploy:status scheduler          # последние запуски deploy-home
mise run deploy:status scheduler --live   # + read-only kubectl get deploy

mise run rollback scheduler --dry-run     # план из units.mjs
mise run rollback scheduler --yes         # выполнить документированную команду
```

`with-kube-tunnel.sh` отказывается работать без явного `KUBECONFIG` и
проверяет, что его сервер — `https://127.0.0.1:16443`; поэтому голый
`kubectl` и `~/.kube/config` не используются. `--live` и `rollback --yes` берут admin kubeconfig из OpenBao через fnox
(профиль `kube`) и сами поднимают туннель к API. `rollback` для `forward-only`
завершается ненулевым кодом, для `pulsar` только печатает команду для
станции.

Бэкапы — реестр `platform/ops/backups.mjs` (раздел «Backup»):

```bash
mise run backup:list
mise run backup:status [<id>] [--dry-run]
mise run backup:now <id> --dry-run|--yes
mise run backup:verify [<id>] --dry-run|--yes
```

`backup:now` выполняет только документированную команду цели; цель только
через UI (HA backup) печатает `manual`. Restore не автоматизирован: вывод
содержит `restore_doc`.

App-токены (модули доступа) и service-token. Порядок раскатки:

```bash
# 0. до merge: консистентная копия БД scheduler (схема 7 -> 10 необратима)
mise run admin:scheduler-backup

# 1. до merge: init создаёт три новых ключа (service_token,
#    service_token_previous="", app_tokens пустым snapshot) и проверяет, что
#    остальные свойства scheduler-auth из ExternalSecret (token, vpn_sync_token,
#    vpn.json) уже есть; иначе падает, ничего не записав. Идемпотентно, ничего
#    не печатает, существующие значения не трогает
mise run admin:service-token-init

# 2. до merge: ESO обновляет scheduler-auth только раз в час, а init уже
#    записал service_token, service_token_previous и app_tokens в OpenBao.
#    Форсировать синк и убедиться, что ключи уже в Secret: иначе новый
#    scheduler стартует без них и падает в crash-loop до следующего refresh
mise run kubectl -- -n home-assistant annotate externalsecret scheduler-auth \
  force-sync="$(date +%s)" --overwrite
mise run kubectl -- -n home-assistant wait --for=condition=Ready \
  externalsecret/scheduler-auth --timeout=120s
mise run kubectl -- -n home-assistant get secret scheduler-auth \
  -o go-template='{{range $k, $v := .data}}{{$k}}{{"\n"}}{{end}}' |
  grep -Ex 'service_token|service_token_previous|app_tokens'

# 3. merge -> deploy-home: scheduler и сервисы стартуют на service_token,
#    телефоны/HA продолжают работать на legacy «Мама». Pins с digest'ами и
#    манифесты develop Flux применяет разными ревизиями, поэтому сначала
#    дождаться ревизии merge-коммита в app-ha-executors и app-intercom
#    (иначе новый образ работает со старым манифестом — так intercom падал
#    в crash-loop около 5 минут)
mise run kubectl -- -n flux-system get kustomization app-ha-executors app-intercom \
  -o jsonpath='{range .items[*]}{.metadata.name}{" "}{.status.lastAppliedRevision}{"\n"}{end}'
mise run kubectl -- -n home-assistant rollout status \
  deploy/scheduler deploy/intercom --timeout=300s

# 4. сразу после деплоя: в runtime-values (voice-proxy) HERMES_SCHEDULER_TOKEN
#    из scheduler-auth#service_token и рестарт hermes (см. ниже)

# 5. отдельный шаг: owner-токен (файл 0600, путь печатается, сам токен нет)
mise run admin:app-token-bootstrap
# ротация service-token (тот же шаг, но с окном previous и финализацией)
mise run admin:app-token-bootstrap -- --rotate-service
```

`lastAppliedRevision` в шаге 3 должен совпасть с merge-коммитом `develop`
(не с коммитом pins в `deploy/home-apps`); только после этого проверять поды.

Откат этого релиза: одного `mise run rollback scheduler` (предыдущий digest)
мало: старый бинарь отказывается открывать схему v10 (`NewerSchema`), а
манифесты требуют новых env и ключей. Порядок: `flux suspend` для
`app-ha-executors`, revert коммита merge (манифесты, ExternalSecret и pins
вместе), restore БД scheduler из копии шага 0 (`docs/REBUILD.md`, раздел
scheduler), resume. Записи, сделанные после merge (новые токены, привязки
устройств, расписания), при этом теряются.

Bootstrap повторяем. Owner-токен выпускается один раз; повторный запуск
сверяет файл с seed и ничего не меняет, потерянный файл — только явный
`--rotate-owner`. Новый токен пишется во временный 0600-файл, который
переименовывается в целевой только после успешной CAS-записи в OpenBao.
Дальше: force-sync ExternalSecret с ожиданием **нового** `refreshTime` и
фактических значений в Secret, рестарт scheduler, проверка Secret и
`-service-token-sha256` в поде scheduler, owner-хэша в `app_tokens`. Без
`--rotate-service` сервисы не перезапускаются: service-token не меняется.
С `--rotate-service` старый токен уходит в `service_token_previous`,
синхронизируются `scheduler-auth`, `phone-push`, `mcp-gateway-scheduler`,
`intercom`, перезапускаются существующие deployment `phone-push`,
`mcp-gateway`, `intercom`, `ha-control`, `ha-bridge`, `ha-scenario`, `hermes`
(voice-proxy — контейнер в `hermes`; отсутствующие пропускаются с сообщением),
затем `service_token_previous` становится `""`, синк и рестарт scheduler.
Прерванная ротация (previous не пуст) при следующем запуске доводится до
конца без второй ротации.

Preflight (до любых изменений) требует, чтобы `voice-proxy` в
`deployment/hermes` брал `HERMES_SCHEDULER_TOKEN` из
`scheduler-auth#service_token`, и ищет legacy-значение в Secrets/ConfigMaps
`home-assistant` кроме производных ESO-секретов. Обход — `--skip-preflight`.

Окно voice-queue: старый scheduler принимает только legacy-токен, новый —
только service-token, а env Hermes читается при старте, поэтому между
деплоем scheduler и рестартом `hermes` с новыми `runtime-values` voice-proxy
получает `401/403` на `ListDueEvents/AckEvent`. События остаются в очереди и
доставляются после рестарта; объявления старше TTL voice-proxy пропускает.
Разрешать legacy-токен как service-токен ради этого окна запрещено
(решение 69).

Потребители legacy app-токена «Мама», которые не меняются: HA
`secrets.yaml` → `scheduler_bearer` (`rest_command.scheduler_rpc`, только
app-RPC), старые APK, `secrets/home/scheduler.token` и `fnox`-профиль
`ingress`, seed сценариев и ручные e2e. `ha-control -healthcheck` ходит в
`/v1/catalog` legacy-токеном (`CONTROL_TOKEN`).

Скрипты с секретами — только `mise run admin:*` (`mise tasks | grep admin:`),
kubectl/helm — `mise run kubectl|helm -- …`. Разовая команда с kubeconfig:

```bash
mise run secrets:exec -- -P kube exec -- \
  platform/workstation/with-kube-tunnel.sh kubectl get pods -A
```

## secrets-gateway (owner-only секреты приложения)

Порядок первого включения (владелец; секреты и коды не печатать):

```bash
# 1. policy + auth/kubernetes/role/secrets-gateway (audience secrets-gateway)
mise run admin:openbao-bootstrap

# 2. merge; если merge раньше шага 1 — под Pending и Kustomization
#    app-secrets-gateway NotReady, другие приложения не ломаются
# 3. после merge: force-sync ESO и проверка Secret
mise run kubectl -- -n home-assistant annotate externalsecret secrets-gateway \
  force-sync="$(date +%s)" --overwrite
mise run kubectl -- -n home-assistant wait --for=condition=Ready \
  externalsecret/secrets-gateway --timeout=120s
mise run kubectl -- -n home-assistant get secret secrets-gateway \
  -o go-template='{{range $k, $v := .data}}{{$k}}{{"\n"}}{{end}}'

# 4. привязать телефон: одноразовый код в файл 0600, действует 15 минут
mise run admin:secrets-device-enroll
# приложение: Система → Секреты → Привязать телефон
# повторная привязка — новый код; отзыв без телефона:
mise run admin:secrets-device-enroll -- --revoke
```

В Secret должен быть только `service_token`; в поде — SA `secrets-gateway` с
projected audience `secrets-gateway` (`automountServiceAccountToken: false`),
без root/operator token. PVC `secrets-gateway-state` хранит только
использованные step-up nonce (60 секунд). Код привязки — в
`secrets/local/secrets-gateway/device-enroll.txt` (0600), после проверки файл
удалить. `--revoke` удаляет ключ из OpenBao; сервис читает его на каждый
запрос, поэтому отзыв действует без рестарта (новые step-up и уже открытые
сессии перестают проходить). Маршрут `secrets.<HOME_DOMAIN>` — Traefik и
wildcard TLS, DNS-запись создаётся один раз вручную. Проверка и откат —
`projects/home/kubernetes/secrets-gateway/README.md`; API и step-up —
`solutions/secrets-gateway/README.md`.

## Где команды модулей

| Тема | Документ |
|---|---|
| VPN (ocserv, клиентские сертификаты, WireGuard, проброс портов) | `projects/home/proxmox/vpn/README.md` |
| BB server (CT 103), плагины, backup базы | `projects/home/proxmox/bb/README.md` |
| OpenBao | `projects/home/kubernetes/openbao/README.md` |
| External Secrets Operator | `projects/home/kubernetes/external-secrets/README.md` |
| Flux, pins, первая установка | `projects/home/kubernetes/flux/README.md` |
| HTTPS ingress, Cloudflare DNS | `projects/home/kubernetes/ingress/README.md` |
| Vaultwarden | `projects/home/kubernetes/vaultwarden/README.md` |
| ha-config-sync (доставка ha-voice) | `projects/home/kubernetes/ha-config-sync/README.md` |
| MCP-шлюз, голосовой маршрут, мост bb | `projects/home/kubernetes/mcp-gateway/README.md` |
| Планировщик, ha-bridge, ha-scenario, сид сценариев | `projects/home/kubernetes/scheduler/README.md` |
| Intercom | `projects/home/kubernetes/intercom/README.md` |
| secrets-gateway (OpenBao KV, device-key step-up, audit) | `solutions/secrets-gateway/README.md`, `projects/home/kubernetes/secrets-gateway/README.md` |
| wakegrant | `projects/home/kubernetes/wakegrant/README.md` |
| Currency bot | `projects/home/kubernetes/currency-bot/README.md` |
| Hermes / runtime | `projects/home-assistant/deploy/README.md`, `KUBERNETES-OPERATIONS.md` |
| Голос, фразы, профиль | `projects/home-assistant/ha-voice/README.md` |
| Комп (Windows-ПК) | `projects/home/windows-pc/README.md` |
| Модуль входа (logon provider) | `solutions/windows-logon-provider/README.md` |
| Android, App Distribution, ручной релиз | `projects/mobile/android/README.md` |
| BLE gateway | `projects/station/ble-gateway/README.md` |
| Wake-события и модель | `projects/station/tools/wakeword-personalize/README.md`, `WAKE-WORD.md` |
| Offsite backup | `projects/home/backup/offsite/README.md` |
| Тесты | `TESTING.md` |

## Доступ

```bash
adb connect 192.0.2.13:5555
ssh -i ~/.ssh/home-lab/station_ed25519 root@192.0.2.13   # пароль — fallback
mise run station:audit -- --ip 192.0.2.13                # read-only сверка

ssh -F ~/.ssh/home-lab/config pve|k3s|haos               # после secrets:pull
```

HA `http://192.0.2.28:8123`, MA `:8095`, Grafana LAN
`http://192.0.2.27:30300/`. HTTPS (LAN/VPN):
`https://{ha,grafana,alarms,control,currency,intercom,secrets,vault,passwords,nexus}.home.example.com/`.
Реальные адреса — `secrets/NETWORK.md`. Не пробрасывать на роутере
`8123/8095/8099/6053/6638` и `80/443`.

CT 102 (VPN): команды, которые перезапускают ocserv/nftables/ssh/сеть, —
только отсоединённо (`pct exec 102 -- systemd-run --unit=<name> <команда>`,
затем `journalctl -u <name>`): синхронный запуск рвёт связь владельцу,
сидящему через ocserv `full`.

## Android: обновление через App Distribution

Приложение обновляется само из Firebase App Distribution, но установку
подтверждает человек. Первый запуск на телефоне:

1. Открыть **Настройки → Система → Проверить обновления**, убедиться, что
   показана текущая версия.
2. Нажать **Войти** и войти в тот Google-аккаунт, который приглашён в группу
   тестировщиков `admins` (Firebase App Distribution хранит вход сам, пароль
   приложение не видит).
3. Когда появится доступная версия с релизными заметками, нажать
   **Обновить** и разрешить установку из этого источника в системном окне.

Дальше проверка идёт при выходе приложения на передний план не чаще раза в
6 часов; строка в настройках проверяет сразу. Автоматических установок нет.
Детали — `projects/mobile/android/README.md` (In-app updates).

## Проверка

```bash
mise run test                # hermetic; стенд не трогает
mise run test:e2e            # live HA/BLE read-only; нужны HA_URL, HA_TOKEN
STATION_E2E_MUTATE=1 mise run test:e2e
STATION_E2E_AUDIO=1 mise run test:e2e
```

```bash
mise run kubectl -- get kustomizations -A
mise run kubectl -- -n flux-system get configmap home-app-images -o yaml   # pins
mise run kubectl -- -n flux-system annotate kustomization/<name> \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run admin:ingress-verify
```

Откат приложения — не `kubectl rollout undo`: `flux suspend` Kustomization,
revert pins-коммита в `deploy/home-apps`, `resume` (`mise run rollback <unit>`
печатает план). `app-ha-executors` — один Kustomization на scheduler,
`ha-bridge`, `ha-control`, `ha-scenario`; режимы и реплики — в Git, учения —
через `flux suspend` + `deploy.sh` (README scheduler). `app-hermes` —
`HelmRelease/runtime` с `remediation.strategy: rollback`; аварийный путь —
`projects/home-assistant/deploy/rollout.sh`.

Не запускать второй writer для HA/MA/SQLite: один Deployment — одна replica,
Recreate; Helm rollback state не откатывает.

## Внешний heartbeat (dead-man switch)

`Watchdog` Alertmanager пингует Cloudflare Worker `solutions/heartbeat`
(контракт — его README). Приватные значения —
`secrets/home/heartbeat.env` (`CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN`,
`KV_NAMESPACE_ID`, `PING_TOKEN`, `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`);
Alertmanager читает URL и токен из OpenBao `secret/k8s/monitoring/heartbeat`.

```bash
node --test solutions/heartbeat/src/*.test.js
mise run admin:heartbeat                 # деплой Worker
mise run admin:heartbeat -- --secrets    # + секреты Worker
mise run kubectl -- -n monitoring annotate externalsecret alertmanager-telegram \
  force-sync="$(date +%s)" --overwrite
```

Ротация `PING_TOKEN`: env → `admin:heartbeat -- --secrets` → запись в
OpenBao. Смена Telegram-бота — то же плюс
`secret/k8s/home-assistant/hermes-environment`, чтобы Alertmanager и Hermes
остались на одном боте. Outage-тест — silence только на `Watchdog`:

```bash
mise run kubectl -- -n monitoring exec alertmanager-monitoring-alertmanager-0 -c alertmanager -- \
  amtool silence add alertname=Watchdog --duration=25m \
  --comment="heartbeat outage test" --alertmanager.url=http://127.0.0.1:9093
```

Через `GRACE_SECONDS` + cron-тик приходит «дом молчит»; после истечения
silence первый ping даёт recovery.

## Станция / Pulsar

```bash
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh log 100'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh disable'  # вернуть maind
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh enable'
adb -s 192.0.2.13:5555 shell '/data/pulsar/pulsar --version'
```

Откат на `pulsar.prev` — только на станции, не через CI (updater предлагает
лишь более новые версии); атомарно, не поверх работающего файла:

```bash
adb -s 192.0.2.13:5555 shell 'cd /data/pulsar && cp -p pulsar.prev pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'
```

`mise run rollback pulsar` печатает эту команду, станцию не трогает.

Локальный endpointing (экспериментальный) включается только переменной
процесса: `PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`; reload без
переменной выключает. Не boot-persistent; `deploy.py` модели перезапускает
Pulsar без неё.

При возврате на stock (`ctl.sh disable` или crash-loop) снова откроется
`quasar-server :31116` без auth — перед возвратом закрыть порт правилом INPUT.
StationCancel без речи — HA-конфиг, не бинарник.

Модель wake word меняется отдельно от бинарника; текущая и профили отката —
`STATE.md`. Откат на профиль (подставить LAN-адрес HA):

```bash
mise run secrets:exec -- -P station exec -- \
  python3 projects/station/tools/wakeword-personalize/deploy.py \
  --ha-url http://192.0.2.28:8123 \
  --profile projects/station/configs/wakeword-v27.json
```

После рестарта Pulsar MA переподключается сам (~3 с). Если
`media_player.iandeks_stantsiia_midi` остаётся `unavailable` — один рестарт
add-on MA (`hassio.addon_restart`, `addon: d5369777_music_assistant`), без
рестарта HA.

```bash
projects/station/scripts/station-status.sh
projects/station/scripts/restore-ota-fortress.sh   # осторожно, откат
```

### Релиз и деплой Pulsar

Полная процедура — `projects/station/pulsar/RICARDO.md`, «Релизы и деплой»;
версия — решение 60. Кратко:

1. Обычный PR, squash после зелёного `ci-ok`; `VERSION` руками не правится.
   PR публикует `ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с
   provenance; merge продвигает этот digest на `<version>` и `latest`.
2. Approve в Environment `Station` → `deploy-station` (VPN группа `ci`,
   сертификат `ci-pulsar`, OIDC-роль OpenBao `deploy-station`, секрет
   `secret/ci/deploy-station`) → HA `update_entity` + `update.install`.
   Job ждёт `installed_version == version` и 90 с стабильности.
3. Повтор уже опубликованного `latest` — `mise run deploy pulsar --yes`.
4. Сломанный релиз — «Пересобрать и задеплоить сервисы» с галочкой `pulsar`.

Sync upstream — патчем, см. `AGENTS.md` («Вложенные upstream-репозитории»).

## Egress / карантин станции

```bash
adb -s 192.0.2.13:5555 shell 'iptables -S EGRESS_GUARD; ip6tables -S EGRESS_GUARD6'
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh
adb -s 192.0.2.13:5555 shell /data/quasar/yandex-block.sh
```

Guard разрешает LAN/DHCP/NTP и при `ALLOW_GITHUB=1` (по умолчанию) `tcp/443`
к диапазонам GitHub из `GITHUB_NETS` в скрипте (для updater'а); остальной
WAN и Яндекс — DROP. Снятие guard не снимает Яндекс-блок/OTA.

## Артефакты GHCR и CI-релизы

Сборка один раз на PR, после merge только promotion (решение 36); механика,
PR-head fallback и карта воркфлоу — `DECISIONS.md` и `TESTING.md` (CI).
Профилактика уехавшей базы — `mise run pr:merge <N>` (`GIT.md`).

PR CI проверяет версию до merge: `native / version guard` сравнивает содержимое
опубликованного `tree-<hash>` с уже существующим тегом вычисленной версии
(`platform/ci/promote.mjs`, `verifyReleaseVersion`). Если тег занят другим
содержимым, PR красный: нужен коммит по путям юнита или подъём `VERSION`
floor. Уже опубликованный сломанный тег чинится только явной пересборкой
(`replace_artifacts`), не повторным promotion.

Релиз native-юнита запускает только изменение его собственных путей
(`publish_when`, иначе `paths`). Общие входы (`MODULE.bazel`, `Cargo.lock`,
`platform/mise-tasks/**`, `_native.yml`) пересобирают и тестируют юнит, но не
публикуют и не деплоят его: иначе версия не растёт, а байты меняются.

Честная пересборка: Actions → «Пересобрать и задеплоить сервисы»
(`deploy-services.yml`) на `develop`. Галочки групп — `platform/ci/rebuild.mjs`,
поле `units` — точные id. `replace_artifacts` позволяет новым native-байтам
заменить опубликованный `tree-<hash>` и version-тег.

Что приехало: summary job'а `pins <unit>` в прогоне `deploy-home`, configmap
`home-app-images`, для Hermes — `mise run helm -- -n home-assistant history runtime`.

Воспроизводимость Hermes (две сборки без кэша → один digest):

```bash
projects/home-assistant/deploy/images/hermes/verify-reproducible.sh
```

Обновление Hermes — bump `projects/home-assistant/deploy/images/hermes/revision`.

Уборка — `_cleanup.yml` (джобы `ghcr` и `fad`), триггеры `cleanup.yml`
(ежедневно) и `cleanup-manual.yml` (`dry_run` по умолчанию `true`). Политика —
в коде скриптов; локальный dry-run на фикстурах:

```bash
node platform/ci/ghcr_cleanup.mjs \
  --fixture platform/ci/testdata/ghcr-versions.json --now 2026-11-04T00:00:00Z
node platform/ci/fad_cleanup.mjs \
  --fixture platform/ci/testdata/fad-releases.json --now 2026-11-04T12:00:00Z
```

`ghcr` без права удаления требует секрет `GHCR_CLEANUP_TOKEN`; `fad` — роль
Firebase App Distribution Admin у сервис-аккаунта и OIDC-роль OpenBao
`android-cleanup` (`configure.py`, применяется `mise run admin:openbao-bootstrap`).

## Reboot

```bash
ssh -F ~/.ssh/home-lab/config pve 'qm reboot 100'   # HAOS
ssh -F ~/.ssh/home-lab/config pve 'qm reboot 101'   # K3s
```

После reboot проверить HA, K3s pods, Grafana, HTTPS ingress, Sendspin и
Pulsar. Аварийный доступ к PVE — LAN/Wi-Fi.

## Backup

Все цели — `mise run backup:list` (реестр `platform/ops/backups.mjs`).
Proxmox: ночной vzdump `nightly` (04:00), storage `backup`.

```bash
ssh -F ~/.ssh/home-lab/config pve 'pvesh get /cluster/backup; pvesm list backup | tail'
ssh -F ~/.ssh/home-lab/config pve 'vzdump 101 --storage backup --mode snapshot --compress zstd'
ssh -F ~/.ssh/home-lab/config pve 'qmrestore /mnt/pve/backup/dump/<archive> <new-vmid> --storage local-lvm'
```

Проверочный restore — в новый VMID без сети (`--unique`, отключить `net0`),
не поверх живой VM. HA full backup — HA UI/Supervisor (automatic encrypted
backup 03:30, 3 копии). K3s — `KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не
удалять: `/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.

### Offsite backup (Google Drive + restic)

K3s-узел, `offsite-backup.timer` 04:45 → restic `rclone:gdrive:home-offsite-v2`
(scope `drive.file`: rclone видит только файлы своего OAuth client).
Устройство, лимиты, retention и restore —
`projects/home/backup/offsite/README.md`.

```bash
mise run admin:offsite-install
mise run admin:pve-backup-install
mise run backup:status offsite-restic
mise run backup:verify offsite-restic --yes
```

Приватные входы — OpenBao `secret/infra/backup` (`offsite/*`, fnox-профиль
`offsite`); файлы `home/offsite-rclone.conf` и
`home/offsite-gdrive-client.env` — через `mise run secrets:file`
(каталог — `SECRETS.md`). Новый
OAuth-токен: `rclone authorize drive <base64 JSON {"client_id","client_secret","scope":"drive.file"}>`,
собрать `[gdrive]` в `offsite-rclone.conf`, переустановить. Смена client =
новый репозиторий (drive.file не видит файлы другого client).
