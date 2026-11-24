# Проверки репозитория

Общие syntax/security-проверки, аудит публикации, запрет комментариев
(`no_comments.mjs`) и агрегаторы `mise run test`, `mise run coverage`. Тесты
компонентов — рядом с ними, E2E — внутри проектов. Команды и правила —
[TESTING.md](../../docs/TESTING.md).

- `run-tests.sh` — оркестратор герметичных наборов, live E2E не включает.
- `audit_repository.mjs` — read-only, печатает пути и строки, но не найденные
  значения; запускается и из pre-push.
- `test_static.py` — syntax/security-гейты и `kustomize build` Flux-каталогов:
  каждый namespaced ресурс в `projects/home/kubernetes/*/manifests` без
  `targetNamespace` обязан иметь `metadata.namespace`; фикстуры —
  `testdata/kustomize/`.
- `coverage.sh` — пересобирает `reports/TEST-COVERAGE.md` (не в Git).
- Проверки здесь и в `platform/ci/` — Node ESM без npm (`mise run test:ci`).
