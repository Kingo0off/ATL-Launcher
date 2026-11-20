"""Раздача лаунчера по локальной сети: python tools/share.py [порт]

Друзья в той же сети (Wi-Fi, роутер, Radmin/Hamachi/ZeroTier) открывают адрес из консоли и скачивают
свежий архив. Архив собирается из этой папки сам, при каждом изменении файлов.
"""
import html
import io
import os
import socket
import sys
import threading
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
from launcher import APP_NAME, APP_VERSION  # noqa: E402

SKIP_DIRS = {"__pycache__", ".git", "site", "dist", "build"}
SKIP_EXT = (".pyc", ".log", ".zip")
_lock = threading.Lock()
_cache = {"stamp": None, "data": b""}


def _files():
    for d, dirs, names in os.walk(ROOT):
        dirs[:] = sorted(x for x in dirs if x not in SKIP_DIRS)
        for n in sorted(names):
            if not n.endswith(SKIP_EXT):
                yield os.path.join(d, n)


def build():
    """Возвращает (zip-байты, время последнего изменения)."""
    files = list(_files())
    stamp = tuple((f, int(os.path.getmtime(f)), os.path.getsize(f)) for f in files)
    with _lock:
        if _cache["stamp"] != stamp:
            buf = io.BytesIO()
            with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
                for f in files:
                    z.write(f, os.path.join(APP_NAME, os.path.relpath(f, ROOT)))
            _cache.update(stamp=stamp, data=buf.getvalue())
        return _cache["data"], max((s[1] for s in stamp), default=0)


PAGE = """<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{name}</title><style>
:root{{color-scheme:dark}}body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0f1117;color:#e9eaf2;font:16px/1.55 system-ui,Segoe UI,sans-serif}}
main{{width:min(520px,calc(100% - 32px));background:#161923;border:1px solid #2b3042;border-radius:20px;padding:34px 30px}}
h1{{margin:0 0 4px;font-size:30px;letter-spacing:-.02em}}.v{{color:#8c91a8;margin:0 0 22px}}
a.btn{{display:block;text-align:center;background:#9b7cff;color:#120b2b;font-weight:700;text-decoration:none;padding:14px;border-radius:12px}}
a.btn:hover{{background:#bba8ff}}ol{{color:#b4b8cc;padding-left:20px;margin:22px 0 0}}li{{margin:6px 0}}small{{color:#5f647b;display:block;margin-top:18px}}
</style><main><h1>{name}</h1><p class="v">Версия {version}, обновлено {date}, {size} МБ</p>
<a class="btn" href="/{fname}" download>Скачать для Windows, macOS, Linux</a>
<ol><li>Установи <b>Python 3.9+</b> (python.org, при установке отметь «Add to PATH»).</li><li>Распакуй архив.</li>
<li>Дважды кликни <b>FriendLauncher.pyw</b> (или <b>Запустить.vbs</b>).</li></ol><small>Нужен браузер на Chromium, например Edge или Chrome.</small></main></html>"""


class H(BaseHTTPRequestHandler):
    server_version = "FriendShare"

    def log_message(self, fmt, *a):
        print("  ", self.address_string(), fmt % a)

    def _send(self, code, body, ctype, extra=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?")[0]
        fname = f"{APP_NAME}-{APP_VERSION}.zip"
        if path in ("/", "/index.html"):
            import time
            data, mt = build()
            page = PAGE.format(name=html.escape(APP_NAME), version=html.escape(APP_VERSION), fname=fname,
                               date=time.strftime("%d.%m.%Y %H:%M", time.localtime(mt)), size=f"{len(data) / 1048576:.1f}")
            return self._send(200, page.encode("utf-8"), "text/html; charset=utf-8")
        if path in ("/" + fname, f"/{APP_NAME}.zip"):
            return self._send(200, build()[0], "application/zip", {"Content-Disposition": f'attachment; filename="{fname}"'})
        self._send(404, b"not found", "text/plain")

    do_HEAD = do_GET


def lan_ips():
    ips = set()
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("10.255.255.255", 1))  # пакеты не отправляются, нужен только выбор интерфейса
        ips.add(s.getsockname()[0])
        s.close()
    except OSError:
        pass
    try:
        ips.update(socket.gethostbyname_ex(socket.gethostname())[2])
    except OSError:
        pass
    return sorted(i for i in ips if not i.startswith("127."))


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    try:
        srv = ThreadingHTTPServer(("0.0.0.0", port), H)
    except OSError as e:
        sys.exit(f"Не удалось занять порт {port}: {e}. Попробуй другой: python tools/share.py 8090")
    data, _ = build()
    print(f"{APP_NAME} {APP_VERSION}, архив {len(data) / 1048576:.1f} МБ. Друзья открывают в браузере:")
    for ip in lan_ips() or ["<адрес этого компьютера>"]:
        print(f"   http://{ip}:{port}/")
    print("Если не открывается, разреши Python в брандмауэре Windows (частная сеть). Остановить: Ctrl+C.")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("Остановлено.")


if __name__ == "__main__":
    main()
