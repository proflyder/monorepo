# Проверки репозитория

Здесь находятся общие syntax/security-проверки монорепозитория, аудит
публикации и агрегаторы `mise run test`, `mise run test:e2e`, `mise run coverage`.
Тесты отдельных компонентов находятся рядом с ними; E2E — внутри проектов.

Полные команды и ограничения — [TESTING.md](../../docs/TESTING.md).

`audit_repository.mjs` запускается также из pre-push hook.
`run-tests.sh` не включает live E2E. Проверки в `platform/ci/` и
`platform/quality/` — Node ESM без npm-зависимостей: `node --test platform/ci
platform/quality` (или `mise run test`).

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `audit_repository.mjs` — Read-only publication checks; print paths/line numbers, never matched values.
- `audit_repository.test.mjs` — Boundary tests for the reviewed public-name allowances in the audit scanner.
- `coverage.sh` — Run the hermetic suite, collect machine-measured coverage where possible, and regenerate reports/TEST-COVERAGE.md (generated, not tracked).
- `run-tests.sh` — Hermetic tests live beside their owners; this file only orchestrates them.
  - The control-mode regression drives deploy.sh --skip-build with pre-resolved image digests, so it needs neither Bazel nor release binaries in dist/. The Rust crate's converge/state/preflight are covered by its Bazel unit tests.
- `test_static.py`
  - Docs and code must not contain obvious live credentials.
