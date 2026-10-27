# projects/mobile

Будущие мобильные клиенты. Пока не инициализируются, чтобы не фиксировать
фреймворк раньше API.

Кандидаты после стабилизации REST/gRPC/Connect:

- Kotlin Multiplatform / Android + iOS;
- Flutter;
- SwiftUI + Kotlin отдельно.

Bazel сможет оркестрировать Android (rules_android) и Apple (rules_apple),
но это отдельный этап. Основной клиент на первом этапе — PWA
`solutions/assistant-ui`.
