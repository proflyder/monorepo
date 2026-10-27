#!/usr/bin/env python3
"""Локальный HTTPS-прокси для Яндекс.Станции (прозрачный режим проброса).

Что делает:
- HTTPS на :443 (или порт из argv[1]) с нашим листовым сертом (chain.pem/leaf.key,
  цепочка — своя, рядом со скриптом; подробности в README.md).
- хосты из списка (rules.json) (rules.json: quasar.yandex.net) пробрасывает в НАСТОЯЩИЙ
  бэкенд Яндекса и логирует всё: заголовки, тела, ответы. Это режим разведки.
- Ответы /get_sync_info можно ПРАВИТЬ на лету (rules.json: rewrite) или заменять
  целиком (sync_info_mode: static, файл sync_info.json).
- ЛЕГИТНЫЕ ответы бэкенда складывает в captures/ — там же лежит реальный формат
  конфига и его поля (adb/debug/develop ищем там).
- Ошибки TLS-рукопожатия (колонка НЕ приняла серт) пишутся в лог отдельной строкой.

Запуск:    sudo python3 server.py [порт]
Дальше:    правила — rules.json; лог — mitm.log; захват — captures/.
"""
import http.server, http.client, ssl, json, sys, datetime, os, socket, struct, urllib.parse

BASE    = os.path.dirname(os.path.abspath(__file__))
LOG     = os.path.join(BASE, "mitm.log")
CHAIN   = os.path.join(BASE, "chain.pem")
KEY     = os.path.join(BASE, "leaf.key")
SYNC    = os.path.join(BASE, "sync_info.json")
RULES_F = os.path.join(BASE, "rules.json")

# --- правила (перечитываются на каждый запрос — можно править на живом стенде) ---
def load_rules():
    try:
        with open(RULES_F, encoding="utf-8") as f:
            return json.load(f)
    except Exception as e:
        print("rules.json битый, работаю по умолчанию:", e)
        return {}

def log(msg):
    ts = datetime.datetime.now().isoformat(timespec="seconds")
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(f"{ts} {msg}\n")

# --- upstream (настоящий бэкенд; IP-пин, чтобы не зациклиться на себя) ---
# часть роутеров ПРОЗРАЧНО перехватывает 53/udp+tcp (запрос к любому DNS
# возвращает наш локальный IP), поэтому реальный IP достаём ЧЕРЕЗ DoH
# (dns.google), кэшируем, SNI/проверку серта оставляем по имени quasar.yandex.net.
REAL_IP_FALLBACK = "213.180.193.230"               # снято 21.10.2026 (DoH dns.google)
_real_ip_cache = {"ip": None, "ts": 0}

def real_ip():
    env = os.environ.get("QUASAR_REAL_IP")
    if env: return env
    now = datetime.datetime.now().timestamp()
    if _real_ip_cache["ip"] and now - _real_ip_cache["ts"] < 600:
        return _real_ip_cache["ip"]
    import urllib.request, urllib.parse
    for doh in ("https://dns.google/resolve", "https://cloudflare-dns.com/dns-query", "https://dns.quad9.net:5053/dns-query"):
        try:
            u = doh + "?" + urllib.parse.urlencode({"name": "quasar.yandex.net", "type": "A"})
            req = urllib.request.Request(u, headers={"Accept": "application/dns-json"})
            if "quad9" in doh: continue   # quad9 без JSON--api, не Morse
            with urllib.request.urlopen(req, timeout=10) as r:
                d = json.loads(r.read().decode())
            ips = [a.get("data") for a in d.get("Answer", []) if a.get("type") == 1]
            if ips:
                _real_ip_cache.update({"ip": ips[0], "ts": now})
                log(f"   [upstream] реальный IP quasar.yandex.net = {ips[0]} (DoH {doh})")
                return ips[0]
        except Exception as e:
            log(f"   [upstream] DoH {doh} не ответил: {e!r}")
    _real_ip_cache.update({"ip": REAL_IP_FALLBACK, "ts": now})
    log(f"   [upstream] DoH не вышел, фолбэк на {REAL_IP_FALLBACK}")
    return REAL_IP_FALLBACK

class PinnedHTTPS(http.client.HTTPSConnection):
    """HTTPS к РЕАЛЬНОМУ IP, но SNI и проверка серта — по имени quasar.yandex.net."""
    def __init__(self, host, real, **kw):
        super().__init__(host, **kw)
        self._real = real
    def connect(self):
        s = socket.create_connection((self._real, self.port if self.port else 443), timeout=self.timeout)
        self.sock = self._context.wrap_socket(s, server_hostname=self.host)

class Upstream:
    def __init__(self, host, tc=15, tr=30):
        self.host, self.tc, self.tr = host, tc, tr
    def request(self, method, path, headers, body=b""):
        ctx = ssl.create_default_context()          # системный CA -> настоящий серт Яндекса
        conn = PinnedHTTPS(self.host, real_ip(), timeout=self.tc, context=ctx)
        try:
            conn.request(method, path, body=body, headers=headers)
            r = conn.getresponse()
            data = r.read()
            hdrs = {k.lower(): v for k, v in r.getheaders()}
            return r.status, hdrs, data
        finally:
            conn.close()

# --- применение правил ---
def dig_set(obj, path, val):
    keys = path.split(".")
    for k in keys[:-1]:
        obj = obj.setdefault(k, {})
    obj[keys[-1]] = val

def apply_rules(obj, rules):
    for r in rules or []:
        if not isinstance(r, dict): continue
        op, path, val = r.get("op"), r.get("path"), r.get("value")
        if op == "set":
            dig_set(obj, path, val)
        elif op == "merge":
            tgt = obj
            for k in path.split("."): tgt = tgt.setdefault(k, {})
            if isinstance(val, dict): tgt.update(val)
    return obj

class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    # ---------- служебное ----------
    def _log(self, tag, body=b""):
        log(f"== {self.command} {self.path} from {self.client_address[0]} [{tag}]")
        for k, v in self.headers.items():
            log(f"H: {k}: {v}")
        if body:
            log(f"BODY({len(body)}): {body[:8192].decode('utf-8','replace')}")

    def _read_body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    def _reply(self, code, data, ctype="application/json", extra=None):
        if isinstance(data, (dict, list)): data = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        for k, v in (extra or {}).items(): self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
        log(f"-> {code} ({len(data)} байт) клиенту")

    def _save_capture(self, q, name, data):
        r = load_rules(); cap = r.get("capture"), r.get("capture_dir", "captures")
        if not cap[0]: return
        d = os.path.join(BASE, cap[1]); os.makedirs(d, exist_ok=True)
        dev = (urllib.parse.parse_qs(q).get("device_id") or ["x"])[0][:40]
        ts = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        fn = os.path.join(d, f"{ts}_{name}_{dev}.json")
        open(fn, "wb").write(data)
        log(f"-> capture: {fn}")

    # ---------- проброс ----------
    def _forward(self, method, body=b""):
        r = load_rules()
        up_host = (r.get("upstream") or {}).get("host", "quasar.yandex.net")
        my_host = self.headers.get("Host", "").split(":")[0]
        # перекладываем заголовки колонки; то, что мешает, убираем
        h = {}
        for k, v in self.headers.items():
            kl = k.lower()
            if kl in ("host", "accept-encoding", "content-length", "connection", "keep-alive"):
                continue
            h[k] = v
        h["Host"] = up_host
        h["Accept-Encoding"] = "identity"
        if body: h["Content-Length"] = str(len(body))
        mode = r.get("sync_info_mode", "rewrite")
        is_sync = "/get_sync_info" in self.path
        is_check = "/check_updates" in self.path
        check_mode = r.get("check_updates_mode", "passthrough")
        # --- static: вообще не ходим в интернет, отдаём файл ---
        if is_sync and mode == "static":
            try: data = open(SYNC, "rb").read()
            except FileNotFoundError: data = b"{}"
            self._log(f"static -> sync_info.json", body)
            self._reply(200, data)
            return
        # --- проброс в настоящий бэкенд ---
        try:
            up = Upstream(up_host,
                         (r.get("upstream") or {}).get("timeout_connect", 15),
                         (r.get("upstream") or {}).get("timeout_read", 30))
            st, uh, udata = up.request(method, self.path, h, body)
            log(f"   upstream {my_host} -> {st}, {len(udata)} байт")
        except Exception as e:
            log(f"   upstream ОШИБКА: {e!r}")
            self._reply(502, b"{}")
            return
        # --- OTA-щит: /check_updates -> всегда {"hasUpdate":false} (rules.json: check_updates_mode) ---
        # Препятствует прилетанию фикс-прошивки ДО того, как колонка отрутована и
        # закрыта --ota-off (no-op в device_ota_update.sh). Что предлагал Яндекс — видно в captures/.
        if is_check and check_mode == "block":
            offered = "?"
            try:
                offered = json.loads(udata).get("version", "нет")
            except Exception:
                offered = "не-JSON"
            log(f"   OTA-ЩИТ: Яндекс предлагал: {offered} -> отдаю колонке hasUpdate:false")
            self._save_capture(urllib.parse.urlparse(self.path).query, "legit", udata)
            self._reply(200, b'{"hasUpdate":false}')
            return
        if is_sync and mode == "rewrite":
            try:
                q = urllib.parse.urlparse(self.path).query
                obj = json.loads(udata)
                self._save_capture(q, "legit", udata)          # оригинал — на сохранение
                apply_rules(obj, r.get("rewrite"))
                udata = json.dumps(obj, separators=(",", ":")).encode()
                self._save_capture(q, "patched", udata)
            except Exception as e:
                log(f"   rewrite пропущен (не JSON?): {e!r}")
        elif is_sync and mode == "passthrough":
            q = urllib.parse.urlparse(self.path).query
            self._save_capture(q, "legit", udata)
        # --- ответ колонке ---
        self.send_response(st)
        for k, v in uh.items():
            if k in ("transfer-encoding", "content-length", "connection", "keep-alive", "content-encoding"):
                continue
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(udata)))
        self.end_headers()
        self.wfile.write(udata)
        log(f"-> {st} ({len(udata)} байт) клиенту")

    # ---------- методы ----------
    def do_GET(self):
        self._log("GET"); self._forward("GET")

    def do_POST(self):
        body = self._read_body()
        self._log("POST", body); self._forward("POST", body)

    def log_message(self, *a): pass

# --- сервер: TLS вручную, чтобы видеть отказы рукопожатия ---
CTX = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
CTX.load_cert_chain(CHAIN, KEY)
CTX.minimum_version = ssl.TLSVersion.TLSv1_2

class S(http.server.ThreadingHTTPServer):
    daemon_threads = True
    def finish_request(self, request, client_address):
        try:
            tls = CTX.wrap_socket(request, server_side=True)
        except ssl.SSLError as e:
            log(f"!! TLS-рукопожатие ОТКЛОНЕНО {client_address[0]}: {e!r} — серт колонкой НЕ принят (в её бандле нет AAA?)")
            try: request.close()
            except Exception: pass
            return
        self.RequestHandlerClass(tls, client_address, self)

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 443
    httpd = S(("0.0.0.0", port), H)
    print(f"HTTPS-прокси на :{port}; серт: {CHAIN}")
    print(f"правила: {RULES_F} (можно править на ходу); лог: {LOG}; захват: captures/")
    httpd.serve_forever()