# Hermes voice: low reasoning и потоковый TTS

Внедрение: 2026-10-27, lab-host. Текущее состояние — [STATE.md](STATE.md),
исходный анализ — [RESEARCH-HERMES-LATENCY.md](RESEARCH-HERMES-LATENCY.md).

## Изменения

- `projects/home-assistant/hermes/voice_proxy.py` v2 развёрнут в
  `/home/labuser/.local/lib/hermes-voice-proxy/voice_proxy.py`.
- Каждый voice chat request получает
  `model_options: {reasoning_effort: low}`. Именно этот API-параметр читает
  установленный Hermes v0.22.5 (`_request_agent_overrides` →
  `_request_reasoning_config` → `_create_agent`). Значение можно переопределить
  env `HERMES_VOICE_REASONING_EFFORT` в proxy service. Глобальный Hermes
  `agent.reasoning_effort=medium` сохранён для остальных клиентов.
- Proxy больше не принуждает `stream=false`. Для streaming он читает SSE
  по событиям, очищает законченные предложения, убирает reasoning и служебные
  Hermes events. Ссылки/URL через границы chunks удерживаются до безопасной
  границы. Ответы обычным JSON продолжают работать.
- После законченного предложения добавляется `\n\n`: sentence detector
  Piper иначе ждёт начало следующего предложения. Это разделитель текста,
  который не произносится; код Piper не менялся.
- Последний текстовый fragment отправляется **до** отдельного finish frame:
  HA 2026.10.4 прекращает чтение сразу при `finish_reason`.
- При обрыве/ошибке upstream выдаётся SSE error без второго HTTP response
  и без выдачи незавершённого хвоста. Upstream закрывается при обнаружении
  отключения клиента; keepalive передаются для проверки живости соединения.
- HA entry `01M5W7PPAR84C51486PX6Q79YF`, domain `llama_cpp`:
  `data.streaming` изменён с false на true. Subentry, prompt, модель,
  HA tool API и остальные записи не менялись. После restart проверено
  семантическое равенство полного JSON backup/current с поправкой только
  на этот флаг.
- Перезапущены proxy и один раз HA. Hermes gateway, Piper, MA, станция и
  Pulsar не перезапускались. Бинарники/исходники Pulsar не менялись.

## Проверки

Локально: `python3 -m unittest discover -s tests -t . -p 'test_*.py' -q`.
Профиль: `python3 -m unittest discover -s projects/home-assistant/hermes -q`.

Результат: **91 Python test passed**, из них 25 в Hermes-профиле.
HA config check RC=0; локальный «какой будильник» по-прежнему отвечает
«Будильник выключен»; после проверок satellite idle, alarm_ringing off.

Новые проверки включают все разрезы ссылок/цитат, посимвольную выдачу,
раннюю доставку при намеренно удержанном upstream, правильный finish-order
для HA, фильтрацию reasoning/tools, HTTP 429 и обрыв посреди SSE.

Живые измерения (единичные примеры, разные ответы, без статистических выводов):

| Запрос / уровень | Результат |
|---|---|
| «Почему небо голубое», direct Hermes medium, JSON | полный текст 6.180 с |
| Тот же вопрос, direct Hermes low, SSE | первый delta 2.059 с, завершение 3.269 с |
| «Что такое фотосинтез», новый proxy low, SSE | первое очищенное предложение 2.597 с, завершение 2.966 с |
| HA → Hermes → Puriso, обычный короткий вопрос | `stream_response=true`; первые аудиобайты 3.497 с от text-run |
| HA → Hermes → Puriso, два предложения, до разделителя | ранний TTS 2.846 с, intent-end 3.787 с, аудио 4.044 с: Piper ещё ждал следующую фразу |
| Тот же сценарий после разделителя | ранний TTS 2.061 с; первые **1024 байта** аудио 2.328 с; intent-end 2.592 с |

Последний запрос явно просил начать ответ предложением о фотосинтезе и
затем объяснить его пользу. Проверка подключалась к штатному pipeline
`Рикардо`, начиная с intent stage и заканчивая TTS; HTTP audio скачивалось
при `tts_start_streaming`, а не после run-end. Аудиофайлы не сохранялись,
на media player не отправлялись. Это доказывает сквозную доступность
раннего аудио, но не измеряет микрофон/VAD/STT или начало звука динамика.

Piper `:10200` через Wyoming describe сообщил
`supports_synthesize_streaming=true`. HA 2026.10.4 запускает ранний TTS
после **более 60 символов**; порог не менялся. Для одной короткой фразы
streaming может не дать заметного выигрыша — reasoning low остаётся полезным
независимо от него.

Запрос со свежей новостью через streaming proxy завершился за 11.963 с;
ответ 93 символа, URL и reasoning/tool deltas в тексте отсутствовали.
Это отдельный класс задержки, не скорость обычного вопроса.

Proxy пишет только времена `first_text`, `total` и reasoning level,
без текста вопроса/ответа и без bearer key. Это время до очищенного текста,
а не до аудио. Постоянное debug recording не включалось.

## Проверка runtime и развёртывание proxy

На Mac:

```bash
ssh -i ~/.ssh/labuser_ha labuser@192.0.2.25
```

На lab-host:

```bash
systemctl --user is-active hermes-voice-proxy
journalctl --user -u hermes-voice-proxy -n 20 --no-pager
```

Для следующей версии proxy: сравнить актуальный deployed файл с репозиторием,
сохранить отдельный backup, загрузить `.new`, выполнить `python3 -m py_compile`,
затем atomic rename и restart только `hermes-voice-proxy`.

Флаг HA находится в entry data, не в conversation subentry options.
В установленном HA flow нет отдельного переключателя этого флага, а
`config_entries/update` не разрешает менять data. В этом внедрении HA
остановлен штатным `docker stop`, после остановки сохранён полный storage
backup и изменено только `data.streaming`; HA поднят в `finally`.
Не править `.storage` работающего HA: он может перезаписать изменения.
Перед обслуживанием проверить отсутствие активного voice-запроса, звонящего
будильника и активных Assist-таймеров (они не переживают restart HA).

## Откат

Private backup на lab-host:
`/home/labuser/nettop/backups/hermes-streaming-20261027/`:

- `voice_proxy.py.before` — исходный deployed proxy;
- `core.config_entries.before` — конфигурация после штатной остановки HA,
  mode 600; содержит секреты, не печатать и не коммитить.

Для отката **не заменять весь текущий storage старым backup**: с тех пор
могли измениться другие integrations. Остановить HA, сделать новый backup
актуального файла и поменять только `data.streaming=false` у указанного
entry, сохраняя все остальные поля. Затем запустить HA. После выключения
streaming можно вернуть proxy из backup, если сравнение подтвердило, что
поверх него ещё нет нужных более новых изменений; restart proxy.

Если нужен только прежний reasoning, достаточно задать proxy env
`HERMES_VOICE_REASONING_EFFORT=medium` и перезапустить proxy; HA и Hermes
gateway перезапускать не требуется. Для возврата low удалить это env
переопределение или явно задать low.

Следующая ручная приёмка: обычный вопрос и вопрос с поиском через микрофон;
проверить начало речи, паузы, отмену и возвращение музыки. Полную акустическую
статистику p50/p95 этот deploy не заменяет.
