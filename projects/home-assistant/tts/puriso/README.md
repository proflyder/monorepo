# Puriso через существующий Piper

Фактический текущий статус — `../../../docs/STATE.md`; команды — `../../../docs/OPERATIONS.md`.

Источник: https://huggingface.co/dhammagift/piper-ru-puriso

Checkpoint revision: `4f826cf39157118a2854f0b825917c1f83a342e7`. Файлы ONNX + ONNX JSON неизменны относительно этого revision. Voice ID в Wyoming — **`ru_puriso2-medium`**, язык **`ru`**. Sample rate 22050 Hz, mono. Используется штатный Piper phonemizer; RUAccent не добавлялся и не нужен для воспроизведения принятого пользователем сэмпла.

Лицензии checkpoint по автору: CC BY-NC-SA 4.0 и OpenRAIL-M. Attribution: Puriso voice by Dhamma.Gift, based on Piper ruslan and Supertonic M1.

## Установка весов

На том узле, где находится Piper data-dir:

```bash
python3 projects/home-assistant/tools/tts-lab/install_puriso.py --data-dir /srv/k3s/volumes/piper
```

Скрипт скачивает только два файла с закреплённого revision, проверяет SHA256, не перезапускает сервисы. Если путь скрипта другой, передать его фактический путь. Проверка уже загруженных файлов без сети:

```bash
python3 projects/home-assistant/tools/tts-lab/install_puriso.py --data-dir /srv/k3s/volumes/piper --verify-only
```

В private values Piper (K3s runtime):

```text
PIPER_VOICE=ru_puriso2-medium
```

После сохранения файлов и backup перезапустить только Deployment:

```bash
kubectl -n home-assistant rollout restart deploy/piper
```

В HA pipeline «Рикардо»: engine `tts.piper`, language `ru`, voice `ru_puriso2-medium`. Применять через UI/WebSocket API, не править работающую `.storage` вручную.

Для явного `tts.speak`: `language: ru`, `options: {voice: ru_puriso2-medium}`. В будильнике сохранять ESPHome endpoint `media_player.iandeks_stantsiia_midi_media_player`.

## Откат к Irina

Вернуть в pipeline `tts_language: ru_RU`, `tts_voice: ru_RU-irina-medium`. В private values вернуть `PIPER_VOICE=ru_RU-irina-medium`, перезапустить Piper. В `alarm_announce` убрать новые `language/options` либо явно задать Irina с `ru_RU`; reload scripts. Веса Irina не удалять. Проверить local intent → TTS и конечное состояние спутника.

Откат не требует перезагрузки HA, станции или STT. Не восстанавливать целиком устаревшую `.env`/scripts.yaml из backup поверх чужих последующих изменений: применить точечные изменения либо сначала сравнить файлы.
