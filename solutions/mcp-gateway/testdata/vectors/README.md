# Domain vectors

Языко-нейтральные векторы чистых функций домена. Go-тесты читают их через
`solutions/mcp-gateway/internal/golden`; Rust-реализация должна читать те же
файлы своим раннером.

Формат:

```json
{
  "name": "resolve",
  "description": "почему векторы важны",
  "catalog": "необязательный YAML каталога для всех кейсов",
  "cases": [{"name": "exact_ru", "input": {"name": "планировщик"}, "expected": {"service": "scheduler"}}]
}
```

`expected` — полный результат кейса. `-update` переписывает только
`expected`, не трогая имена, описания, входы и каталог.

| Файл | Что фиксирует |
|---|---|
| `speech.json` | нормализация spoken-строки (markdown, URL, символы, лимит 160 рун) |
| `redact.json` | удаление секретов; значения `{{token}}`/`{{jwt}}`/`{{hex}}`/`{{telegram}}` собирает тест, чтобы длинные токены не лежали в файле |
| `resolve.json` | резолв spoken-имени: регистр, ё/е, подстрока, один edit-distance, ошибки |
| `catalog_errors.json` | все правила валидации каталога и точные тексты ошибок |
| `report_inputs.json` | входы golden-отчётов: каталог, нагрузки, поды, проверки, часы в Asia/Almaty → `speech` и `details.text` |

Перегенерация: `go test ./solutions/mcp-gateway/internal/... -update`.
