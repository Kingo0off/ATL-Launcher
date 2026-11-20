"""Установка загрузчиков модов: Fabric, Quilt (через meta API), Forge и NeoForge (через установщик)."""
import json
import os
import subprocess

from . import mojang
from .util import ROOT, NetError, download, get_json, popen

_CACHE = os.path.join(ROOT, "loaders.json")


def _cache_get(key):
    try:
        with open(_CACHE, encoding="utf-8") as f:
            return json.load(f).get(key)
    except (OSError, ValueError):
        return None


def _cache_set(key, value):
    try:
        with open(_CACHE, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        data = {}
    data[key] = value
    with open(_CACHE, "w", encoding="utf-8") as f:
        json.dump(data, f)


def _profile_loader(kind, mc, version, log):
    base = {"fabric": "https://meta.fabricmc.net/v2", "quilt": "https://meta.quiltmc.org/v3"}[kind]
    if not version or version == "latest":
        versions = get_json(f"{base}/versions/loader/{mc}")
        if not versions:
            raise NetError(f"{kind} не поддерживает Minecraft {mc}")
        stable = [x for x in versions if "beta" not in x["loader"]["version"] and "alpha" not in x["loader"]["version"]]
        version = (stable or versions)[0]["loader"]["version"]
    vid = f"{kind}-loader-{version}-{mc}"
    path = os.path.join(mojang.VERSIONS, vid, vid + ".json")
    if not os.path.isfile(path):
        log(f"Устанавливаю {kind} {version}…")
        profile = get_json(f"{base}/versions/loader/{mc}/{version}/profile/json")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(profile, f)
        vid = profile.get("id", vid)
    return vid


def _installer_loader(kind, mc, version, java, log):
    key = f"{kind}:{mc}:{version}"
    cached = _cache_get(key)
    if cached and os.path.isfile(os.path.join(mojang.VERSIONS, cached, cached + ".json")):
        return cached
    if kind == "forge":
        url = (f"https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{version}/"
               f"forge-{mc}-{version}-installer.jar")
    else:
        url = (f"https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/"
               f"neoforge-{version}-installer.jar")
    jar = os.path.join(ROOT, "installers", os.path.basename(url))
    log(f"Скачиваю установщик {kind}…")
    download(url, jar)
    profiles = os.path.join(ROOT, "launcher_profiles.json")
    if not os.path.isfile(profiles):
        with open(profiles, "w") as f:
            json.dump({"profiles": {}, "version": 3}, f)
    before = set(os.listdir(mojang.VERSIONS)) if os.path.isdir(mojang.VERSIONS) else set()
    log(f"Запускаю установщик {kind} (может занять пару минут)…")
    p = popen([java, "-jar", jar, "--installClient", ROOT], cwd=ROOT, stdout=subprocess.PIPE,
              stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
    for line in p.stdout:
        line = line.rstrip()
        if line:
            log("  " + line)
    if p.wait() != 0:
        raise RuntimeError(f"Установщик {kind} завершился с ошибкой (см. лог выше).")
    new = [d for d in set(os.listdir(mojang.VERSIONS)) - before if "forge" in d.lower()]
    if not new:
        # установщик мог ничего не добавить, если версия уже стояла — ищем по подстроке
        new = [d for d in os.listdir(mojang.VERSIONS) if "forge" in d.lower() and version in d]
    if not new:
        raise RuntimeError(f"После установки {kind} не нашёл новую версию в {mojang.VERSIONS}")
    vid = sorted(new, key=lambda d: os.path.getmtime(os.path.join(mojang.VERSIONS, d)))[-1]
    _cache_set(key, vid)
    return vid


def install_loader(loader, mc, java, log):
    """Возвращает id версии, которую надо запускать (для vanilla — просто mc)."""
    kind = (loader or {}).get("type", "vanilla").lower()
    version = (loader or {}).get("version", "")
    if kind in ("", "vanilla", "none"):
        return mc
    if kind in ("fabric", "quilt"):
        return _profile_loader(kind, mc, version, log)
    if kind in ("forge", "neoforge"):
        if not version:
            raise RuntimeError(f"Для {kind} нужно указать точную версию загрузчика в манифесте сборки.")
        return _installer_loader(kind, mc, version, java, log)
    raise RuntimeError(f"Неизвестный загрузчик: {kind}")
