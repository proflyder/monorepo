# AGENTS.md

pulsar — нативный Rust-демон вместо стека Яндекса (`maind`) на рутованных Яндекс Станциях: Home
Assistant через ESPHome Native API, Music Assistant через Sendspin, на Миди — Zigbee-радио.
Проприетарные библиотеки прошивки грузятся `dlopen` со станции и никогда не копируются в
репозиторий. Документация (`README.md`, `docs/`) — на русском.

## Харнесс

- Канон для агентов — этот файл и `.agents/skills/<name>/SKILL.md` (формат
  [agentskills.io](https://agentskills.io): frontmatter `name`, `description`).
- Конфигов под конкретные агенты разработки в репозитории нет: не создавать `CLAUDE.md`,
  `.claude/`, `.codex/`, вложенные `AGENTS.md`. Симлинки под конкретные инструменты не коммитятся:
  читать `.agents/skills/` напрямую.
- CI и релиз Pulsar — не в этом каталоге, а в корневых `.github/workflows/` монорепозитория
  (`_native.yml` и `release-pulsar.yml`).
- `AGENTS.md` держать ≤ 16 KiB (у части агентов лимит чтения 32 KiB). Здесь только правила для любой
  задачи; новые факты идут в `docs/`.

| skill | когда |
|---|---|
| `station-test` | любое действие на живой станции: lock, `stop`, тестовый экземпляр, живая подмена, `cleanup` |
| `add-station-model` | новая модель станции: сбор данных read-only, измерения, `Model`, установщик, `boot.sh`, docs |
| `zigbee-radio` | сборка и прошивка образов радио Миди, `zigbee_probe`, чтение через `tools/deconz_read.py` |
| `crash-decode` | расшифровка `=== CRASH` из лога через `addr2line` и `pulsar.debug` |
| `release` | изменение `VERSION`, публикация OCI-артефакта в GHCR — только по явной просьбе пользователя |
| `pre-publish-review` | обязательно перед `git push`, `gh pr create` и тегом `v*` |

## Куда смотреть

| задача | doc | skill |
|---|---|---|
| код демона: крейты, модель исполнения, FMA, `state.conf`, порты | [docs/architecture.md](docs/architecture.md) | — |
| голос: HA voice assistant, стоковый maind, арбитраж, пре-ролл | [docs/voice.md](docs/voice.md) | — |
| железо: указатель моделей и вики, библиотеки Яндекса, новая модель; факты модели — `docs/<модель>.md` | [docs/hardware.md](docs/hardware.md) | `add-station-model` |
| Zigbee (Миди), прошивки `radio/` | [docs/zigbee.md](docs/zigbee.md) | `zigbee-radio` |
| сборка, CI, установщик, супервизор, обновления, релиз, падение | [docs/development.md](docs/development.md) | `release`, `crash-decode` |
| тесты на станциях | [docs/stations.md](docs/stations.md) | `station-test` |
| поведение для пользователя | [README.md](README.md) | — |

Каталоги: `satellite/` — [docs/architecture.md, «Workspace»](docs/architecture.md#workspace);
`installer/`, `satellite/deploy/`, `ssh/` — [docs/development.md](docs/development.md); `radio/` —
[docs/zigbee.md](docs/zigbee.md); `tools/` — [docs/stations.md](docs/stations.md) и skill
`station-test`.

## Сборка и проверки

```sh
make -C satellite -j"$(nproc)"                        # обе архитектуры
make -C satellite ARCH=armhf                          # одна архитектура
make -C satellite ARCH=armhf tools                    # все утилиты src/bin
make -C satellite ARCH=armhf build/armhf/tools/<name> # одна утилита
make -C satellite check                               # fmt, clippy -D warnings, тесты
go -C installer test ./...
radio/build.sh
ssh/build.sh
```

- Версии инструментов, выходы, версия бинаря и zig для `make check` —
  [docs/development.md, «Сборка»](docs/development.md#сборка).
- `make -C satellite check` должен проходить. Хостовые тесты покрывают Rust-крейты и установщик;
  поведение железа проверяется на станции (skill `station-test`). Те же проверки (обе архитектуры,
  installer, radio, ssh) выполняет CI-юнит `pulsar` в корневом `_native.yml`.
- Перед `git push` и PR пройти skill `pre-publish-review` до чистого раунда в рамках правил
  монорепозитория.

## Правила кода

- **Акторы и неблокирующие handle** — [docs/architecture.md, «Модель
  исполнения»](docs/architecture.md#модель-исполнения).
- **`unsafe` и libc** — только в модулях из
  [docs/architecture.md, «Модель исполнения»](docs/architecture.md#модель-исполнения), п. 8.
- **`mul_add` не добавлять и не убирать** — [«Плавающая точка
  (FMA)»](docs/architecture.md#плавающая-точка-fma).
- **`state.conf` совместим с `.prev`** — [«state.conf и откат на
  .prev»](docs/architecture.md#stateconf-и-откат-на-prev).
- **Зависимости** пинятся точной версией в `[workspace.dependencies]` (`satellite/Cargo.toml`);
  новая внешняя зависимость — только с обоснованием.
- **Код под архитектуру** целиком, со всеми хелперами, под `#[cfg(target_arch = …)]`: мёртвого кода
  на других целях нет (`clippy -D warnings` на хосте и обеих станционных целях).
- **Комментарии** объясняют факты устройства, ограничения и «почему», а не «что». Новые — на
  русском; комментарий, чей смысл меняется, переписывается на русском целиком; нетронутые не
  переводятся. Если обоснование — факт из docs, у кода одна строка `// см. docs/<файл>.md,
  «<раздел>»`.

## Документация

- Поведение кода в docs не пересказывать; обоснование решения — комментарием у кода.
- В docs только железо, внешние системы (HA, ESPHome, maind, библиотеки Яндекса, Zigbee-стек,
  GitHub), межкомпонентные контракты, правила и навигация.
- Каждый факт — в одном месте, остальные ссылаются (`docs/<файл>.md, «<раздел>»`; якоря должны
  разрешаться). Новый факт — в doc своей подсистемы; в `AGENTS.md` — только правила для любой
  задачи.
- Факты железа (SoC, ядро, чипы и шины, имена sysfs, коды кнопок) берутся из вики hackalice.me
  (страницы моделей — [docs/hardware.md, «Модели»](docs/hardware.md#модели)) и в docs не
  повторяются. В doc модели — только то, чего в вики нет: замеры, контракты и особенности, на
  которых стоит код, статус проверок. При расхождении с вики свой факт остаётся, только если за ним
  замер мейнтейнера (Миди, Лайт 2) или сырой вывод контрибьютора, с пометкой «расходится с вики»;
  иначе удаляется.

## Git

Код Pulsar живёт в монорепозитории (`projects/station/pulsar/`), поэтому Git-процесс — общий:
корневой [`AGENTS.md`](../../../AGENTS.md), раздел «Git и pull request», и
[`docs/GIT.md`](../../../docs/GIT.md). Правила в этом файле и в `.agents/skills/` относятся к
самому Pulsar; Git-процесс, ветки, PR и релизы определяет монорепозиторий.

- Рабочая ветка `<type>/<name>` от свежего `origin/develop`; PR в `develop` только squash-merge
  с зелёным `ci-ok`. Прямой push в `develop` и push тегов запрещены (pre-push hook).
- Монорепозиторий не публикует GitHub Releases и не пушит теги. Релиз Pulsar — изменение
  `projects/station/pulsar/VERSION`: merge в `develop` запускает `release-pulsar.yml`, который
  собирает обе архитектуры и публикует OCI-артефакт в `ghcr.io/proflyder/pulsar-firmware`.
- Upstream `Morvex885/pulsar` вливается вручную патчем (`git diff vA..vB` → `git apply -3
  --directory=projects/station/pulsar`), затем обычный PR. Вложенного checkout'а и snapshot-скрипта
  больше нет.
- Персональные данные не попадают в репозиторий: никаких реальных IP, MAC, серийников, имён Wi-Fi,
  токенов, логов станций. Плейсхолдер адреса — `192.0.2.10` (TEST-NET-1).
- `backup/`, `dump/`, `re/`, `ref/`, `satellite/build/`, `satellite/target/`,
  `satellite/third_party/src/` — в `.gitignore`, не публиковать. Проприетарные библиотеки, модели и
  конфиги прошивки — тоже.
- Авторство и сообщения коммитов — по корневому `docs/GIT.md`; отдельного скрипта и job `commits`
  в монорепозитории нет.

## Безопасность станций

- **Root adb не ломать.** Не трогать adbd, маркеры `/data/quasar/activate_*` и
  `/data/quasar/rootkeeper.sh` (принадлежит root-методу): установщик только убирает из него
  строки-хуки, которые добавляли его старые версии, и никогда не создаёт, не удаляет и иначе не
  меняет файл.
- **Rootfs (`/system`, `/etc`) не менять**, всё постоянное живёт в `/data`. Единственное исключение
  — `/etc/init.d/S99zpulsar`: его пишет установщик и только после проверки, что rootfs не защищена
  (`rootfsGuard` в `installer/autostart.go`; [docs/development.md, «Установка и
  станция»](docs/development.md#установка-и-станция)).
- Защита станции от OTA прошивки Яндекса — не задача pulsar (владелец станции / root-метод). OTA,
  заменившая system-раздел, сносит `S99zpulsar`; повторный запуск установщика его восстанавливает,
  если проверка rootfs пропускает запись.
- Несколько устройств могут делить один adb-сервер: всегда `adb -s <serial>`.
- Через adb бинарные данные — только `adb push` / `pull`; коду выхода `adb shell` не верить
  ([docs/development.md, «Правила installer/»](docs/development.md#правила-installer)).
- Перед заменой бинаря сохранить рабочий как `.prev` — на нём держится автоматический откат. Ручной
  выход: `.prev` или `ctl.sh disable` (запускает стек Яндекса).
- Не открывать ESPHome API к станции, которую использует Home Assistant: тест — на отдельном бинаре
  на другом порту при остановленном сервисе (`ctl.sh disable` или остановка супервизора).
- Без согласия владельца — никаких изменений громкости и тестовых звуков на живой колонке; тесты со
  звуком ограничивают громкость: ≤ 0.2 на шкале `Mixer`, raw ≤ 82; выше — только с явного
  разрешения владельца.
- `/tmp` — небольшой tmpfs (размеры — [docs/hardware.md, «RAM и
  tmpfs»](docs/hardware.md#ram-и-tmpfs)): тестовые файлы удалять.
- Живые тесты — только по [docs/stations.md](docs/stations.md) (lock, тестовый экземпляр, очистка),
  процедура — skill `station-test`.
