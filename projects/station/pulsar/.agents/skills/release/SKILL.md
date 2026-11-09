---
name: release
description: >-
  Выпуск релиза pulsar (изменение VERSION, OCI-артефакт в GHCR). Только по явной просьбе
  пользователя. Ключевые слова: release, tag, version bump, VERSION, GHCR, publish, deploy,
  SHA256SUMS, release assets.
---

# Релиз

Только по явной просьбе пользователя. Контракты (имена OCI-слоёв, версия, обновления из HA) —
`docs/development.md`, «CI и релизы» и «Обновления из HA»; правила git — корневой
`../../../docs/GIT.md`.

Релизы Pulsar публикует монорепозиторий: GitHub Releases и теги запрещены. Источник обновлений
станции — `ghcr.io/proflyder/pulsar-firmware`.

## 1. Предусловия

- Ветка `<type>/<name>` от свежего `origin/develop`, чистое дерево.
- CI-юнит `pulsar` (`_native.yml`) зелёный в PR.

## 2. Проверки

Skill `pre-publish-review` по диапазону PR.

## 3. Версия

Версия — содержимое `projects/station/pulsar/VERSION` (`vMAJOR.MINOR.PATCH`, продолжение
нумерации upstream: `v<major.minor>.<1000+N>`). Изменить файл в рабочей ветке; сравнить с прошлым
значением и предложить версию пользователю.

## 4. PR и публикация

1. Обычный PR в `develop`; merge — squash после зелёного `ci-ok`.
2. PR CI (`_native.yml`, `promote: false`) собирает `aarch64` и `armhf` с этой версией, radio
   и `SHA256SUMS`, проверяет `--version` под qemu и делает `oras push` в
   `ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с provenance. Push в `develop` с изменённым
   `VERSION` (`native.publish_when`) запускает `native-release`: он проверяет `tree-<hash>` и
   provenance и ставит на тот же digest теги `<version>` и `:latest`; повторной сборки нет.
   Прочие изменения Pulsar релиз не запускают.
3. Тег версии неизменяем по содержимому: повторная публикация того же артефакта — no-op, другого
   содержимого под тем же тегом — ошибка. Уже опубликованную версию не перезаписывать, а
   выпускать следующую.

## 5. Контроль

1. `gh run list --workflow deploy-home --branch develop --limit 1` и `gh run watch <id>`.
2. Упал — `gh run view <id> --log-failed`, сообщить пользователю; релиз не править молча.
3. Проверить артефакт: `oras manifest fetch ghcr.io/proflyder/pulsar-firmware:<version>` —
   слои `pulsar-aarch64.bin`, `pulsar-armhf.bin`, `SHA256SUMS`, `zigbee-*.ybl`; аннотация
   `org.opencontainers.image.version`.
4. Деплой на станцию — job `deploy-station` того же workflow после approve в Environment
   `Station` (только для прошивки, которая уже читает `pulsar-firmware`). Повторный деплой
   уже опубликованной версии — `workflow_dispatch` deploy-home с `units=pulsar`.
