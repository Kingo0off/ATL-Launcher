"""Запуск приложения: локальный сервер + окно без рамок браузера, выход после закрытия окна.

Консоль не нужна: запускай через FriendLauncher.pyw (pythonw). Любые ошибки пишутся в crash.log /
launcher.log в папке данных, а на Windows ещё показываются в обычном окне сообщения.
"""
import json
import os
import shutil
import sys
import threading
import time
import traceback
import urllib.request
import webbrowser

from . import APP_NAME
from .server import Api, app_log, make_server
from .util import OS_NAME, ROOT, popen

LOCK = os.path.join(ROOT, "app.lock")
PROFILE = os.path.join(ROOT, "webview")


def show_error(text):
    """Видимая ошибка даже без консоли."""
    try:
        if OS_NAME == "windows":
            import ctypes
            ctypes.windll.user32.MessageBoxW(0, text[-1500:], APP_NAME, 0x10)
        elif sys.stderr:
            sys.stderr.write(text + "\n")
    except Exception:  # noqa: BLE001
        pass


def find_browser():
    """Chromium-браузер, который умеет открывать страницу как отдельное приложение (--app)."""
    cands = []
    if OS_NAME == "windows":
        roots = [os.environ.get(v, "") for v in ("ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA")]
        rels = [r"Microsoft\Edge\Application\msedge.exe", r"Google\Chrome\Application\chrome.exe",
                r"BraveSoftware\Brave-Browser\Application\brave.exe", r"Vivaldi\Application\vivaldi.exe",
                r"Chromium\Application\chrome.exe"]
        cands = [os.path.join(r, rel) for rel in rels for r in roots if r]
    elif OS_NAME == "osx":
        cands = ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                 "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                 "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                 "/Applications/Chromium.app/Contents/MacOS/Chromium"]
    else:
        cands = [shutil.which(n) for n in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser",
                                           "microsoft-edge", "brave-browser", "vivaldi")]
    return next((c for c in cands if c and os.path.isfile(c)), None)


def open_window(url):
    """Возвращает Popen окна-приложения или None, если открыли в обычном браузере."""
    exe = find_browser()
    if exe:
        try:
            os.makedirs(PROFILE, exist_ok=True)
            return popen([exe, f"--app={url}", f"--user-data-dir={PROFILE}", "--window-size=1240,800",
                          "--no-first-run", "--no-default-browser-check", "--disable-features=Translate"])
        except OSError:
            app_log("browser start failed: " + traceback.format_exc())
    webbrowser.open(url)
    return None


def _ping(url):
    try:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))  # localhost мимо прокси
        with opener.open(url + "api/ping", timeout=1.5) as r:
            return r.status == 200
    except OSError:
        return False


def running_instance():
    try:
        with open(LOCK, encoding="utf-8") as f:
            url = json.load(f)["url"]
        return url if _ping(url) else None
    except (OSError, ValueError, KeyError):
        return None


def run():
    existing = running_instance()
    if existing:  # лаунчер уже запущен — просто показываем окно
        open_window(existing)
        return

    api = Api()
    httpd, url = make_server(api)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    threading.Thread(target=api.reload_packs, daemon=True).start()
    with open(LOCK, "w", encoding="utf-8") as f:
        json.dump({"url": url, "pid": os.getpid()}, f)
    app_log(f"started {url.rsplit('/', 2)[0]}")
    proc = open_window(url)
    started = time.time()
    try:
        while True:
            time.sleep(1)
            if api.any_running():
                continue  # пока идёт игра, лаунчер не закрываем (иначе потеряем её журнал)
            now = time.time()
            idle = now - api.last_ping
            if api.pings == 0:
                if now - started > 60:
                    break  # окно так и не открылось
                continue
            if api.bye_at and now - api.bye_at > 15:
                break  # страницу закрыли
            if proc is not None and proc.poll() is not None and idle > 15:
                break  # окно-приложение завершилось
            if idle > 600:
                break  # вкладку давно никто не трогал
    finally:
        try:
            os.remove(LOCK)
        except OSError:
            pass
        httpd.shutdown()
        app_log("stopped")


def main():
    try:
        run()
    except SystemExit:
        raise
    except BaseException:  # noqa: BLE001
        text = traceback.format_exc()
        try:
            with open(os.path.join(ROOT, "crash.log"), "a", encoding="utf-8") as f:
                f.write(time.strftime("%Y-%m-%d %H:%M:%S\n") + text + "\n")
        except OSError:
            pass
        show_error(f"{APP_NAME} не смог запуститься.\n\n{text}\n\nПодробности: {os.path.join(ROOT, 'crash.log')}")
        raise
