# Обучение «Рикардо» в WSL

Развёрнуто 2026-10-26, 23:10 Asia/Almaty.

## Подключение и расположение

- SSH: `ssh -p 2222 ml@192.0.2.19`
- Каталог WSL: `/home/trainer/projects/ricardo-wakeword`
- Ubuntu 24.04.5, Python 3.12.3.
- Ryzen 9 9950X3D, 23 GiB RAM доступны WSL, RTX 5070 12 GB.
- Отдельное окружение `.venv`: TensorFlow 2.21.0, NumPy 2.2.6, pymicro-wakeword 2.5.0, CUDA-библиотеки через pip.
- Окружение `~/venvs/ai` не изменялось. Драйвер NVIDIA из Windows; Linux-драйвер не устанавливался.

## Проверки

- `pip check`: No broken requirements found.
- TensorFlow GPU matmul: OK; MixedNet обучается на GPU, 3 пробных batch.
- Полная техническая цепочка на малом наборе: синтез, признаки с реверберацией/шумом, обучение, streaming-экспорт, TFLite inference — OK.
- Тестовая модель: 86 224 байта, вход int8 [1, 3, 40], выход uint8 [1, 1]. Это smoke-модель, качество обращения она не подтверждает.
- TensorFlow использует PTX JIT для Blackwell; первый шаг медленнее. XLA JIT выключен.

## Полный прогон

Запущен в tmux `ricardo-training` в 23:09 (+05).
Подтверждена генерация полного набора (50 000 train + 10 000 validation позитивов,
25 000 train + 5 000 validation похожих слов).
Четыре Piper RU голоса; 128 реальных RIR и 200 фоновых записей OpenSLR 28.
Дальше автоматически: features → train → export → проверка формата.
Прогон завершился 27 октября 2026 в 01:27 (+05). Модель, конфиг,
метрики и версии сохранены в `artifacts/wakeword/ricardo-v1/`;
отчёт результата — `artifacts/wakeword/ricardo-v1/README.md`.
Установка на станцию описана в `docs/WAKE-WORD.md`; текущий статус — `STATE.md`.

```bash
cd /home/trainer/projects/ricardo-wakeword
cat logs/training.status
# Логи этапов:
tail -f logs/full-generate.log
# Затем full-features.log, full-train.log, full-export.log
# Сессия:
tmux attach -t ricardo-training
# Отсоединиться, сохранив работу: Ctrl+B, затем D
```

Компьютер должен оставаться включённым; SSH можно закрыть. Колонка не изменялась.
Итоговые файлы: `ru_ricardo_mww.tflite`, `ru_ricardo_mww.json`, `model-sha256.txt`.

## Адаптации

Исходный trainer: https://github.com/interkelstar/microwakeword-trainer
Commit: 8fdbadc3b969a8c1993108c0d655910440dfb762.
MicroWakeWord: a70bd740d4e79ee8a8bb3db843fe862b88d5d6b0, editable-установка из vendor:
обычный wheel не включает layers из-за ошибки упаковки.

Устаревший pin NumPy <2 заменён на 2.2.6 для согласования зависимостей.
Извлечение признаков порциями с записью на диск; mmap для большого позитивного массива.
HF-негативы ограничены 100 000 окон для RAM WSL. GPU обязателен для обучения,
сохранение лучших весов и CSV метрик после эпохи.
Локальные изменения: `trainer-local.patch`; версии: `requirements.lock.txt`.
Повторно запускать исходный setup_mww.sh не следует: он вернёт старый pin NumPy.

## Проверка качества после обучения

Реальные обращения: `record/heldout/`, автоматически в обучение не добавляются.
Реальные ложные срабатывания для следующего обучения:
`training/output/ru_ricardo/user_negatives/` (WAV 16 kHz mono).
Нужны независимые измерения recall, ложных активаций в час и задержки на микрофонах Midi.
Синтетические метрики не подтверждают качество в комнате.
