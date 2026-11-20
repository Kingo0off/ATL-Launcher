"""Импорт сборок Modrinth (.mrpack)."""
import json
import os
import urllib.parse
import zipfile

from . import local
from .util import download_many, safe_join

# По спецификации .mrpack лаунчер обязан качать только с этих хостов.
TRUSTED_HOSTS = {"cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"}

_LOADER_KEYS = [("fabric-loader", "fabric"), ("quilt-loader", "quilt"), ("neoforge", "neoforge"), ("forge", "forge")]


def parse_index(z):
    idx = json.loads(z.read("modrinth.index.json").decode("utf-8"))
    if idx.get("game", "minecraft") != "minecraft":
        raise ValueError("Это сборка не для Minecraft")
    deps = idx.get("dependencies", {})
    mc = deps.get("minecraft")
    if not mc:
        raise ValueError("В сборке не указана версия Minecraft")
    loader = {"type": "vanilla", "version": ""}
    for key, kind in _LOADER_KEYS:
        if key in deps:
            loader = {"type": kind, "version": deps[key]}
            break
    return idx, mc, loader


def import_mrpack(path, progress=None, log=print):
    with zipfile.ZipFile(path) as z:
        idx, mc, loader = parse_index(z)
        tasks = []
        files = idx.get("files", [])
        for f in files:
            if (f.get("env") or {}).get("client") == "unsupported":
                continue
            url = (f.get("downloads") or [""])[0]
            host = urllib.parse.urlparse(url).hostname or ""
            if host not in TRUSTED_HOSTS:
                raise ValueError(f"Сборка ссылается на недоверенный сайт ({host or 'пусто'}). Импорт остановлен.")
        inst = local.save_instance({
            "name": idx.get("name") or os.path.splitext(os.path.basename(path))[0],
            "mc_version": mc, "loader": loader, "ram_mb": 0, "java_args": "",
            "notes": idx.get("summary", ""),
        })
        gdir = local.game_dir(inst)
        for f in files:
            if (f.get("env") or {}).get("client") == "unsupported":
                continue
            dest = safe_join(gdir, f["path"])
            tasks.append((f["downloads"][0], dest, (f.get("hashes") or {}).get("sha1"), None))
        log(f"Скачиваю файлов сборки: {len(tasks)}")
        try:
            download_many(tasks, progress, "Файлы сборки", workers=6)
            # overrides сначала, client-overrides поверх
            local.extract_prefix(z, "overrides/", gdir)
            local.extract_prefix(z, "client-overrides/", gdir)
        except BaseException:
            local.delete_instance(inst)
            raise
    return inst
