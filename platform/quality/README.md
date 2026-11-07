# Проверки репозитория

Здесь находятся общие syntax/security-проверки монорепозитория, аудит
публикации и агрегаторы `mise run test`, `mise run test:e2e`, `mise run coverage`.
Тесты отдельных компонентов находятся рядом с ними; E2E — внутри проектов.

Полные команды и ограничения — [TESTING.md](../../docs/TESTING.md).

`audit_repository.mjs` запускается также из pre-push hook.
`run-tests.sh` не включает live E2E. Проверки в `platform/ci/` и
`platform/quality/` — Node ESM без npm-зависимостей: `node --test platform/ci
platform/quality` (или `mise run test`).
