"""Автоматическая загрузка Java от Mojang, чтобы друзьям не пришлось ничего ставить."""
import os
import shutil

from .util import IS_64, IS_ARM, OS_NAME, ROOT, NetError, download_many, get_json, safe_join

RUNTIME_ALL = ("https://launchermeta.mojang.com/v1/products/java-runtime/"
               "2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json")


def runtime_platform():
    if OS_NAME == "windows":
        return "windows-arm64" if IS_ARM else ("windows-x64" if IS_64 else "windows-x86")
    if OS_NAME == "osx":
        return "mac-os-arm64" if IS_ARM else "mac-os"
    return "linux" if IS_64 else "linux-i386"


def find_java(folder):
    name = "java.exe" if OS_NAME == "windows" else "java"
    for dirpath, _dirs, files in os.walk(folder):
        if name in files and os.path.basename(dirpath) == "bin":
            return os.path.join(dirpath, name)
    return None


def ensure_java(version, user_java="", progress=None, log=print):
    if user_java and os.path.isfile(user_java):
        return user_java
    jv = version.get("javaVersion") or {}
    comp = jv.get("component", "jre-legacy")
    major = jv.get("majorVersion", 8)
    dest = os.path.join(ROOT, "runtime", comp)
    marker = os.path.join(dest, ".installed")

    try:
        allj = get_json(RUNTIME_ALL)
        entries = allj.get(runtime_platform(), {}).get(comp) or []
    except NetError:
        entries = []
        allj = None

    exe = find_java(dest)
    if not entries:
        exe = exe or shutil.which("java")
        if exe:
            return exe
        raise RuntimeError(f"Не удалось получить Java {major} от Mojang для этой платформы. "
                           f"Установи Java {major} вручную и укажи путь в настройках.")

    sha = entries[0]["manifest"]["sha1"]
    if exe and os.path.isfile(marker) and open(marker).read().strip() == sha:
        return exe

    log(f"Скачиваю Java {major} (один раз)…")
    mf = get_json(entries[0]["manifest"]["url"])
    tasks, execs, links = [], [], []
    for rel, info in mf["files"].items():
        path = safe_join(dest, rel)
        t = info["type"]
        if t == "directory":
            os.makedirs(path, exist_ok=True)
        elif t == "file":
            raw = info["downloads"]["raw"]
            tasks.append((raw["url"], path, raw["sha1"], raw["size"]))
            if info.get("executable"):
                execs.append(path)
        elif t == "link":
            links.append((path, info["target"]))
    download_many(tasks, progress, f"Java {major}")
    for p in execs:
        os.chmod(p, 0o755)
    for path, target in links:
        try:
            if not os.path.lexists(path):
                os.makedirs(os.path.dirname(path), exist_ok=True)
                os.symlink(target, path)
        except OSError:
            pass
    with open(marker, "w") as f:
        f.write(sha)
    exe = find_java(dest)
    if not exe:
        raise RuntimeError("Java скачана, но java не найдена внутри.")
    return exe
