# Проверки репозитория

Общие syntax/security-проверки, аудит публикации, запрет комментариев
(`no_comments.mjs`) и агрегаторы `mise run test`, `mise run coverage`. Тесты
компонентов — рядом с ними, E2E — внутри проектов. Команды и правила —
[TESTING.md](../../docs/TESTING.md).

- `run-tests.sh` — оркестратор герметичных наборов, live E2E не включает.
- `audit_repository.mjs` — read-only, печатает пути и строки, но не найденные
  значения; запускается и из pre-push.
- `coverage.sh` — пересобирает `reports/TEST-COVERAGE.md` (не в Git).
- Проверки здесь и в `platform/ci/` — Node ESM без npm (`mise run test:ci`).
