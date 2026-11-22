# platform

Общая платформенная обвязка.

- `bazel/` — toolchains, платформы, downloader-конфиг CI ([README](bazel/README.md)).
- `ci/` — карта юнитов `units.mjs`, маршрутизация, `ci-ok` gate, promotion,
  pins, уборка GHCR/Firebase, `safe-merge.mjs`.
- `quality/` — аудит репозитория, статические проверки, запрет комментариев
  ([README](quality/README.md)).
- `deploy-rs/` — CLI `home-deploy` (Rust) ([README](deploy-rs/README.md));
  `deploy/` — `resolve.sh` и общие манифесты ([README](deploy/README.md)).
- `android/` — Bazel-правила, lint, detekt, подпись и инструменты Android-сборки.
- `mise-tasks/` — код задач `mise run ...`; `ops/` — реестр бэкапов;
  `workstation/` — доступ рабочего места к OpenBao.

CI и правила workflow — [`docs/TESTING.md`](../docs/TESTING.md), Git —
[`docs/GIT.md`](../docs/GIT.md).

## Заметки к коду

- `android/BUILD.bazel`: Firebase-конфиг не коммитится; без gitignored
  симлинка приложение собирается, Firebase не инициализируется. Без
  `--android_platforms` нативные библиотеки (libwebrtc) собираются под CPU
  хоста, и APK не ставится на телефон — это ловит `apk_abi_test`.
- `android/apk_manifest_test.sh`: манифест библиотеки мержится, только если
  она выставляет `exports_manifest`.
- `android/sign-release.sh`: ожидаемый подписант обязателен — его отсутствие
  ошибка, а не предупреждение.
- `ci/affected.mjs`: инвалидации с `deploy: false` перезапускают проверки, но
  не деплой; Markdown внутри дерева приложения не публикует APK.
- `ci/check_units.mjs`: не больше четырёх образов на юнит — по числу
  статических attestation-шагов в `_unit.yml`.
- `ci/fad_cleanup.mjs`: все релизы с максимальным `createTime` считаются
  новейшими — гонка загрузок не удаляет одну из двух свежих сборок.
- `ci/pins.mjs`: пишет только в ветку `deploy/home-apps`, никогда в `develop`;
  гонку с pins другого юнита ретраит.
- `ci/promote.mjs`: дельта для PR-head fallback — `git diff <head> <commit>`
  по дереву, не диапазон от merge base; native-путь никогда не ищет по
  истории — старое дерево нельзя перетегировать как текущий релиз.
- `ci/safe-merge.mjs`: compare-ответ без массива `files` ничего не доказывает
  и не читается как «нет изменений»; после `update-branch` compare может
  несколько секунд отдавать старый `behind_by`.
- `mise-tasks/test/bazel.sh`: `//...` включает Android-цели, поэтому
  передаётся `--config=android`.
- `workstation/run.sh`: интерпретатор берётся `mise which`, чтобы shim'ы
  asdf/pyenv раньше в `PATH` не подменили `python3`.
- `workstation/with-kube-tunnel.sh`: API K3s закрыт из LAN, поэтому на время
  команды поднимается SSH-туннель на `127.0.0.1:16443`; уже открытый туннель
  переиспользуется.
- `workstation/bin/vault`: fnox вызывает `vault`, а `bao` совместим по
  используемым KV-операциям.
