# Domain vectors (frozen)

Языко-нейтральные векторы чистых функций домена. Их читает Rust-раннер
`crates/mcp-gateway/src/domain/vectors.rs`; исходный Go-раннер удалён вместе с
Go-реализацией, поэтому `-update` больше нет.

Формат:

```json
{
  "name": "resolve",
  "description": "почему векторы важны",
  "catalog": "необязательный YAML каталога для всех кейсов",
  "cases": [{"name": "exact_ru", "input": {"name": "планировщик"}, "expected": {"service": "scheduler"}}]
}
```

`expected` — полный результат кейса. Это замороженные данные: изменение
поведения — осознанная правка вектора с причиной в PR, а не перегенерация.

| Файл | Что фиксирует |
|---|---|
| `speech.json` | нормализация spoken-строки (markdown, URL, символы, лимит 160 рун) |
| `redact.json` | удаление секретов; значения `{{token}}`/`{{jwt}}`/`{{hex}}`/`{{telegram}}` собирает тест, чтобы длинные токены не лежали в файле |
| `resolve.json` | резолв spoken-имени: регистр, ё/е, подстрока, один edit-distance, ошибки |
| `catalog_errors.json` | все правила валидации каталога и точные тексты ошибок |
| `report_inputs.json` | входы golden-отчётов: каталог, нагрузки, поды, проверки, кластерный список workload'ов и Flux-ресурсы, часы в Asia/Almaty → `speech` и `details.text` |
