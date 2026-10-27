# Развёртывание на этом Mac (тестовая Station Midi)

Состояние на момент развёртывания:

- прокси: `0.0.0.0:80` и `0.0.0.0:443`, **stage 1 (bootstrap)**, перехват
  `quasar.yandex.net` сертификатом из `certs/quasar.yandex.net.crt`
  (leaf + intermediate);
- DNS-резолвер: `0.0.0.0:53`, отвечает `quasar.yandex.net -> 192.0.2.11`,
  AAAA и прочие типы для этого имени — `NODATA`, остальное форвардит на
  `1.1.1.1`, `8.8.8.8`;
- адрес Mac в LAN: `192.0.2.11`, колонка: `192.0.2.13`;
- в `/etc/hosts` уже закреплён реальный `uniproxy.alice.yandex.net`
  (`213.180.193.76`) — это защита от петли, когда прокси сам ходит в upstream;
- роутер уже форвардит DNS на этот Mac (проверено: системный резолвер Mac
  получает `quasar.yandex.net -> 192.0.2.11`).

## Управление

```bash
cd ~/Desktop/yandex-station-lab/archive/legacy/alice-mitm-bootstrap

./scripts/proxy-ctl.sh status
./scripts/proxy-ctl.sh start
./scripts/proxy-ctl.sh stop
./scripts/proxy-ctl.sh restart

./scripts/proxy-ctl.sh stage1   # bootstrap: только quasar.yandex.net
./scripts/proxy-ctl.sh stage2   # локальная CA + uniproxy (после ADB и install-ca)
```

Логи: `logs/proxy.log`, `logs/dns.log`, `logs/requests.jsonl`.
Конфигурация этапов: `.env.stage1`, `.env.stage2` (активная копия — `.env`).

## Проверка

```bash
curl -s http://127.0.0.1/health
openssl s_client -connect 192.0.2.11:443 -servername quasar.yandex.net \
  -showcerts </dev/null | grep -E '^ *[0-9] s:|Verify return code'
curl -s --resolve 'quasar.yandex.net:443:192.0.2.11' \
  'https://quasar.yandex.net/get_sync_info?device_id=test'
```

Ожидаемо: в цепочке два сертификата, `Verify return code: 0`, а
`get_sync_info` возвращает JSON с `"adbd":{"remoteActivation":true,"network":true}`.

## Дальше

1. Перезагрузить/передёрнуть питание колонки, чтобы она заново разрешила
   `quasar.yandex.net` и запросила `/get_sync_info`.
2. Дождаться ADB:

   ```bash
   ./scripts/wait-for-adb.sh
   ```

3. Если локальная CA ещё не в прошивке (после обновления могла слететь):

   ```bash
   ./scripts/install-ca.sh certs/ca.crt
   ```

   Скрипт сам проверит, есть ли CA, и перезагрузит колонку только при установке.

4. Перейти на stage 2:

   ```bash
   ./scripts/proxy-ctl.sh stage2
   ```

   После этого DNS начнёт отдавать `uniproxy.alice.yandex.net -> 192.0.2.11`,
   а прокси — перехватывать uniproxy.

## Важно

- Пока роутер форвардит DNS на этот Mac, **выключение/сон Mac или остановка
  `dns` ломает разрешение имён во всей сети**. Останавливать — только вместе с
  возвратом DNS на роутере.
- Порт 53 на этом Mac биндится без `sudo`; после перезагрузки Mac процессы
  нужно поднять снова: `./scripts/proxy-ctl.sh start`.
- IP `192.0.2.11` лучше закрепить за Mac DHCP-reservation, иначе после смены
  адреса DNS-записи и сертификат перестанут совпадать.
- Комплект использует приватный ключ leaf-сертификата из утёкшего публичного
  CA-материала. Применять только к своей колонке в своей сети, не выставлять
  порты 80/443/53 в интернет.
