# Проверки репозитория

Здесь находятся общие syntax/security-проверки монорепозитория, аудит
публикации и агрегаторы `make test`, `make test-e2e`, `make coverage`.
Тесты отдельных компонентов находятся рядом с ними; E2E — внутри проектов.

Полные команды и ограничения — [TESTING.md](../../docs/TESTING.md).

`audit_repository.py` запускается также из pre-push hook.
`run-tests.sh` не включает live E2E.
