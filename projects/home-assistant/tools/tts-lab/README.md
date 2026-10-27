# tts-lab — прослушивание голосов

Лабораторный инструмент для сравнения TTS-голосов до замены Piper в живом
контуре. Ничего не устанавливает на станцию и не меняет HA.

## Сэмплы

Готовые сэмплы лежат в
`artifacts/tts-samples/` (локальная приватная папка от корня репозитория):

- `index.html` — плеер (открыть в браузере);
- `piper/` — прежний `ru_RU-irina-medium` и мужские Denis/Dmitri/Ruslan;
- `silero-v5-cis/` — 10 мужских голосов Silero v5 CIS base, короткий и
  длинный текст, плюс варианты ударения в «Рикардо».

Слушать лучше на самой колонке через HA `tts.speak` и в наушниках.

## Генерация

```bash
cd projects/home-assistant/tools/tts-lab
python3 -m venv .venv
. .venv/bin/activate
pip install torch silero soundfile
python generate_samples.py
```

Модель `v5_cis_base_nostress.pt` (87 МБ, MIT) скачивается в
`~/.cache/tts-lab/`; можно указать свой путь через `--model` или
`TTS_LAB_CACHE`.

## Лицензия

Модель `v5_cis_base_nostress` — MIT (см. `LICENSE_CIS` в репозитории
Silero). Модели `v5_5_ru`/`v5_6_ru` — CC BY-NC; для домашнего использования
это допустимо, для коммерческого — нет.

## Puriso

Текущий статус — `docs/STATE.md` (от корня репозитория); установка/откат — `projects/home-assistant/tts/puriso/README.md`. `install_puriso.py` скачивает закреплённые веса/config и проверяет SHA256, но не меняет HA/Compose.
