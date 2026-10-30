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

## Проверено и отвергнуто (октябрь 2026)

Для замены Piper Puriso рассматривались варианты ниже; сэмплы и скрипты не
хранятся. Требования: русский, CPU без GPU (VM 8 vCPU), локально, Wyoming/HA.

| Кандидат | Результат |
|---|---|
| NVIDIA Nemotron Speech / Magpie TTS | Русского нет (12 языков: ar, de, en, es, fr, hi, it, ja, ko, pt, vi, zh); нужен GPU. Nemotron/Parakeet ASR знают русский, но это STT. |
| Chatterbox (NIM) | Русский есть, но слабый; сторонняя модель, в NIM 44 GiB VRAM. |
| ESpeech TTS-1 RL-V2 (Apache-2.0) | Клонирование тембра; RTF 1.4–2.1 на CPU — медленнее реального времени. |
| XTTS v2 (CPML, некоммерческая) | Русский есть, RTF ≈0.8–0.9 на M3 Pro, ударений нет; пользователю не подошёл. |
| Pocket-TTS (Kyutai) | Русского нет; веса gated. |
| xVASynth | Только Windows; русские голоса — community, слабые. |
| Qwen3-TTS (Apache-2.0) | Русский есть, но 1.7B на CPU ≈40 с на фразу; не запускался. |
| Supertonic 3 (OpenRAIL-M) | Русский есть, 99M, ONNX, RTF 0.18–0.29 на 4 потоках (M3 Pro). Голоса M1–M5/F1–F5; M3 понравился. Wyoming-обёртки нет; не внедрён. |

Итоговая оценка пользователя: сэмплы неудовлетворительны (M3 Supertonic был «неплох»). Puriso сделан из Piper
Ruslan дистилляцией Supertonic M1: тем же способом можно дообучить Piper на
другом голосе-учителе (лицензия результата останется некоммерческой, CC BY-NC-SA).

## Puriso

Текущий статус — `docs/STATE.md` (от корня репозитория); установка/откат — `projects/home-assistant/tts/puriso/README.md`. `install_puriso.py` скачивает закреплённые веса/config и проверяет SHA256, но не меняет HA/Compose.
