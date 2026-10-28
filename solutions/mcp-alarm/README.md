# mcp-alarm

Placeholder for schedule/alarm tools. Отдельного MCP runtime здесь нет и пока
не требуется.

## Границы

- Доменные данные и действия принадлежат canonical API
  `packages/proto/ricardo/v1/scheduler.proto` (`SchedulerService`). Мост и UI
  используют тот же контракт; собственная БД у адаптера отсутствует.
- Hermes и будущие клиенты должны получить один общий расширяемый MCP gateway
  с доменными группами инструментов, а не отдельный MCP server на проект.
  Группа «schedules/alarms» — одна из групп такого gateway.
- Gateway не получает project-specific endpoint/DB: инстанс задаётся
  конфигурацией развёртывания, как для остальных solutions.
- MCP — только поверхность для LLM-клиента. Локальный голос и Android ходят в
  scheduler напрямую через Connect/gRPC; MCP не является исполнителем и не
  владеет расписаниями.

## Статус

Runtime, transport, token и listen-настройки не реализованы и не
задеплоены; прежние `ALARM_MCP_*` из этого README удалены как
недействительные. Инструменты (`schedule_*`, `timer_*`, `task_*`) появятся в
общем gateway после стабилизации scheduler API. До этого проект не должен
подключать отдельный MCP server.
