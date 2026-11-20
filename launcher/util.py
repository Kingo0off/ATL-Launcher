"""Общие помощники: пути, HTTP, скачивание с проверкой хеша."""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

UA = "FriendLauncher/1.0"

OS_NAME = "windows" if sys.platform == "win32" else "osx" if sys.platform == "darwin" else "linux"
IS_64 = sys.maxsize > 2 ** 32
_machine = (os.uname().machine if hasattr(os, "uname") else os.environ.get("PROCESSOR_ARCHITECTURE", "")).lower()
IS_ARM = _machine in ("arm64", "aarch64")
ARCH = "arm64" if IS_ARM else ("x86_64" if IS_64 else "x86")

NO_WINDOW = 0x08000000 if sys.platform == "win32" else 0  # CREATE_NO_WINDOW


def popen(cmd, **kw):
    """subprocess.Popen без чёрного окна консоли.

    stdin обязательно задаём явно: у pythonw нет консоли, и без этого дочерний процесс падает
    с «invalid handle». STARTUPINFO с SW_HIDE не используем — он спрятал бы и окно самой игры.
    """
    kw.setdefault("stdin", subprocess.DEVNULL)
    kw["creationflags"] = kw.get("creationflags", 0) | NO_WINDOW
    return subprocess.Popen(cmd, **kw)


def _data_dir():
    override = os.environ.get("FRIENDLAUNCHER_HOME")
    if override:
        base = override
    elif sys.platform == "win32":
        base = os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), "FriendLauncher")
    elif sys.platform == "darwin":
        base = os.path.expanduser("~/Library/Application Support/FriendLauncher")
    else:
        base = os.path.join(os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")), "FriendLauncher")
    os.makedirs(base, exist_ok=True)
    return base


ROOT = _data_dir()


class NetError(Exception):
    pass


def http_json(url, body=None, form=None, headers=None, method=None, timeout=30):
    """Возвращает (status, json). HTTP-ошибки не бросают исключение — смотри status."""
    h = {"User-Agent": UA, "Accept": "application/json"}
    data = None
    if body is not None:
        data = json.dumps(body).encode()
        h["Content-Type"] = "application/json"
    elif form is not None:
        data = urllib.parse.urlencode(form).encode()
        h["Content-Type"] = "application/x-www-form-urlencoded"
    if headers:
        h.update(headers)
    req = urllib.request.Request(url, data=data, headers=h, method=method or ("POST" if data is not None else "GET"))
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            status, raw = r.status, r.read()
    except urllib.error.HTTPError as e:
        status, raw = e.code, e.read()
    except OSError as e:
        reason = getattr(e, "reason", e)
        raise NetError(f"Нет соединения с {urllib.parse.urlparse(url).netloc}: {reason}")
    try:
        js = json.loads(raw.decode("utf-8")) if raw.strip() else {}
    except ValueError:
        js = {"_raw": raw[:300].decode("utf-8", "replace")}
    return status, js


def get_json(url, timeout=30):
    status, js = http_json(url, timeout=timeout)
    if status != 200:
        raise NetError(f"{url} → HTTP {status}")
    return js


def get_text(url, timeout=30):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        raise NetError(f"{url} → HTTP {e.code}")
    except OSError as e:
        raise NetError(f"Нет соединения с {urllib.parse.urlparse(url).netloc}: {getattr(e, 'reason', e)}")


def sha1_file(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _up_to_date(dest, sha1, size):
    if not os.path.isfile(dest):
        return False
    if size is not None:
        return os.path.getsize(dest) == size
    if sha1:
        return sha1_file(dest) == sha1.lower()
    return True


def download(url, dest, sha1=None, size=None, retries=3):
    """Скачивает файл, если его нет или он не совпадает. Возвращает True, если скачал."""
    if _up_to_date(dest, sha1, size):
        return False
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    part = dest + ".part"
    last = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as r, open(part, "wb") as f:
                shutil.copyfileobj(r, f, 1 << 16)
            if sha1 and sha1_file(part) != sha1.lower():
                raise NetError("не совпала контрольная сумма")
            os.replace(part, dest)
            return True
        except (OSError, NetError) as e:
            last = e
            time.sleep(1 + attempt)
    if os.path.exists(part):
        try:
            os.remove(part)
        except OSError:
            pass
    raise NetError(f"Не удалось скачать {url}: {last}")


def download_many(tasks, progress=None, label="Загрузка", workers=8):
    """tasks: список (url, dest, sha1, size). progress(done, total, label)."""
    total = len(tasks)
    if progress:
        progress(0, total, label)
    if not total:
        return
    done = 0

    def one(t):
        download(*t)

    with ThreadPoolExecutor(max_workers=workers) as ex:
        futures = [ex.submit(one, t) for t in tasks]
        try:
            for fut in futures:
                fut.result()
                done += 1
                if progress and (done % 5 == 0 or done == total):
                    progress(done, total, label)
        except BaseException:
            for f in futures:
                f.cancel()
            raise


def safe_join(base, rel):
    """Склеивает путь и не даёт выйти за пределы base (защита от ../ в манифесте)."""
    base = os.path.abspath(base)
    path = os.path.abspath(os.path.join(base, *rel.replace("\\", "/").split("/")))
    if path != base and not path.startswith(base + os.sep):
        raise ValueError(f"Недопустимый путь в манифесте: {rel}")
    return path
