# Snapshots

Локальные снапшоты проекта лежат в:

```text
snapshots/
```

Физическое хранение: `secrets/local/snapshots/`; `snapshots` — локальная
игнорируемая ссылка для совместимости. Папка **целиком исключена из Git**. В Git остаются только правила и
документация, а сами снапшоты — нет.

## Формат

```text
snapshots/
  20261027T211500Z/
    repo.tar.gz
    manifest.json
    SHA256SUMS
```

- имя каталога — UTC timestamp;
- `repo.tar.gz` — снимок tracked-файлов на момент коммита;
- `manifest.json` — commit, branch, время, размер, список исключений;
- `SHA256SUMS` — контрольная сумма архива.

## Что не попадает в снапшот

- `secrets/`;
- `.env` и любые ключи;
- `backups/`;
- `logs/`;
- `artifacts/`;
- `resources/` и `projects/station/upstream/` — внешние checkout'ы;
- `snapshots/` — сами снапшоты;
- бинарники сборки.

## Пример

```bash
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "snapshots/$STAMP"
git archive --format=tar.gz HEAD > "snapshots/$STAMP/repo.tar.gz"
(cd "snapshots/$STAMP" && shasum -a 256 repo.tar.gz > SHA256SUMS)
```

Снапшоты в Git не коммитить. При переносе на внешний носитель копировать
`snapshots/` целиком и проверять `SHA256SUMS`.
