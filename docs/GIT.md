# GIT — ветки, pull request и выкладка

Как изменения попадают в `develop` и домой. Правила для агентов — `AGENTS.md`;
CI и маршрутизация юнитов — `TESTING.md`; деплой сервисов —
`projects/home/kubernetes/scheduler/README.md`.

## Ветки

| Ветка | Назначение | Публикуется |
|---|---|---|
| `develop` | основная, единственная долгоживущая; из неё деплоится дом | только через PR |
| `<type>/<name>` | рабочая ветка одного изменения | да, для PR |
| архивные refs, теги | локальные | никогда |

`<type>`: `feat`, `fix`, `chore`, `docs`, `ci`, `refactor`, `test`, `perf`;
`<name>` — строчные буквы, цифры, `.`, `_`, `-` (например `fix/deploy-digest`).

## Поток

```text
git fetch origin
git switch -c fix/<name> origin/develop     # или git worktree add … для параллельной работы
…правки, проверки, коммиты…
git push -u origin fix/<name>               # pre-push guard
gh pr create --base develop --fill          # PR в develop
# CI: ci-ok зелёный → mise run pr:merge <N> → ветка удаляется
# push в develop = merge PR → deploy-home пинует digest'ы затронутых юнитов,
#   Flux их применяет, warm-android/warm-rust прогревают кэши для PR
```

- Одно логическое изменение — одна ветка и один PR. Мелкие PR проще
  проверять и откатывать.
- Заголовок PR становится сообщением squash-коммита: повелительное
  наклонение, по-английски, как в истории (`Add …`, `Fix …`).
- Описание PR: что и зачем, какие проверки выполнены, влияние на live
  (деплой, режимы, устройства). Секреты, реальные адреса и логи не вставлять.
- Долго живущий PR перед merge обновить от `develop` (rebase или merge в
  ветку) и дождаться нового `ci-ok`: после merge код сразу уходит в деплой.
  `mise run pr:merge <N>` делает это сам (см. «Merge»).
- После merge ветку не переиспользовать; следующая работа — новая ветка от
  свежего `origin/develop`.

## Merge

Merge в `develop` — только `mise run pr:merge <N>`
(`platform/ci/safe-merge.mjs`, тесты — `safe-merge.test.mjs`). Скрипт:

- требует OPEN PR в `develop` без конфликтов;
- если база устарела, вливает `develop` в ветку (`gh pr update-branch`) и
  ждёт зелёный `ci-ok` на новом head (`--timeout <min>`, по умолчанию 90);
- обновление пропускается (новый прогон CI — 15–30 минут), если на текущем
  head зелёный `ci-ok`, PR `MERGEABLE`, файлы PR и новых коммитов `develop`
  не пересекаются (с учётом переименований), и новые коммиты `develop` не
  трогают `CI_GLOBAL_PATHS` из скрипта; `--always-refresh` отключает это
  исключение;
- перед merge снова проверяет базу (до трёх обновлений) и мержит
  `gh pr merge <N> --squash --admin --delete-branch --match-head-commit <head>`:
  сдвинутый head GitHub отклонит;
- печатает merge-sha и ссылку на запуск `deploy-home`.

`--dry-run` — план без изменений, `--no-update` — только проверка.

Почему не `gh pr merge` вручную: `ci-ok` проверяет tree PR, а squash на
уехавший `develop` даёт tree, которого CI не видел, и `deploy-home`
fail-closed отказывает затронутым юнитам. Ruleset не strict, поэтому гонку
закрывает только проверка базы непосредственно перед merge.

## Защита на GitHub

Ruleset ветки по умолчанию (`develop`):

- прямой push, удаление и force-push запрещены; история линейная;
- merge только squash, только через PR;
- обязательная проверка `ci-ok` (итог CI, публикуется GitHub Actions);
- администратор может обойти правила только при merge PR — осознанным
  действием в интерфейсе. Мержить с красным `ci-ok` не нужно: исправить и
  дождаться зелёного.

Ruleset машинной ветки `deploy/home-apps` (`Deploy pins branch`):

- удаление и force-push запрещены, поэтому история pins не переписывается и
  откат всегда делается revert-коммитом;
- обычный push не ограничен: в личном репозитории GitHub не позволяет
  добавить GitHub Actions в bypass, поэтому правило «пишет только CI» не
  выразить без отдельного deploy key. Писать в ветку может только владелец
  и `pins` job; руками коммитить туда только для аварийного отката.

## Локальная защита

`core.hooksPath=.githooks`. `pre-commit` проверяет staged-файлы на
комментарии в коде (`platform/quality/no_comments.mjs --staged`). `pre-push`:

- разрешён только push текущей рабочей ветки `<type>/<name>` в одноимённую
  удалённую; `develop`, архивные refs и теги заблокированы;
- удаление веток через push заблокировано (ветки удаляет GitHub после merge);
- рабочее дерево чистое, Gitleaks по исходящим коммитам без находок,
  `platform/quality/no_comments.mjs` и
  `platform/quality/audit_repository.mjs` без замечаний.

Не обходить guard (`--no-verify`, смена `hooksPath`, mirror push). Если в
основном checkout есть чужие незакоммиченные изменения, публиковать из
отдельного `git worktree` той же ветки.

## Учётная запись для push

Push и PR выполняются от аккаунта владельца репозитория. Если на машине
несколько GitHub-аккаунтов (например рабочий SSH-ключ), для этого
репозитория использовать HTTPS и `gh` как credential helper со своим
профилем, а не глобальный SSH-ключ:

```bash
git -c credential.helper= \
  -c 'credential.helper=!gh auth git-credential' \
  push https://github.com/<owner>/<repo>.git <type>/<name>
```

## Выкладка и откат

- Релиз — `.github/workflows/deploy-home.yml` на push в `develop`: job `pins`
  продвигает проверенные digest'ы затронутых юнитов в `deploy/home-apps`, а
  Flux применяет манифесты из `develop`. Прямого доступа CI в K3s нет.
- Режимы исполнения (`execute`, `shadow`, `control-*`) — Git-состояние в
  `scheduler/manifests/kustomization.yaml`; учения — `flux suspend` плюс
  admin-скрипт, затем `flux resume`.
- Откат приложения — revert pins-коммита или `flux suspend`, не
  `kubectl rollout undo`. Откат изменения целиком — новый PR с
  `git revert <squash-commit>`; историю `develop` не переписывать.

## Версии и публикация артефактов

Native-юниты (Pulsar, Windows logon provider) PR CI собирает и публикует как
`tree-<hash>` с provenance; push в `develop` только продвигает его на
релизные теги. Версию вычисляет CI: `VERSION` — только major.minor, patch —
число коммитов юнита (`platform/ci/version.mjs`, решение 60); отдельного PR с
бампом нет. Тег версии неизменяем по содержимому. Подробности —
`OPERATIONS.md` («Артефакты GHCR и CI-релизы»).

## Агенты

- Исполнитель работает в рабочей ветке или отдельном worktree, может
  коммитить в неё, но не пушит и не открывает PR.
- Primary проверяет diff и результаты проверок, публикует ветку, открывает
  PR, дожидается `ci-ok`, мержит и проверяет деплой.
- Вложенные upstream-репозитории в `resources/` живут по своим правилам из
  `AGENTS.md`; их ветки и remote этот поток не трогает.
