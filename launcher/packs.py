"""Сборки: список с твоего сервера, синхронизация файлов в папку экземпляра."""
import fnmatch
import json
import os
import urllib.parse

from .util import ROOT, NetError, download_many, get_json, safe_join

INSTANCES = os.path.join(ROOT, "instances")


def normalize_index_url(url):
    url = (url or "").strip()
    if not url:
        raise NetError("Не указан адрес сервера сборок (Настройки → Адрес сервера).")
    if "://" not in url:
        url = "https://" + url
    if not url.lower().split("?")[0].endswith(".json"):
        url = url.rstrip("/") + "/index.json"
    return url


def fetch_index(server_url):
    """Возвращает (index_url, список сборок)."""
    index_url = normalize_index_url(server_url)
    data = get_json(index_url)
    return index_url, data.get("packs", [])


def fetch_manifest(index_url, pack):
    url = urllib.parse.urljoin(index_url, pack["manifest"])
    manifest = get_json(url, timeout=60)
    manifest["_url"] = url
    return manifest


def instance_dir(pack_id):
    return safe_join(INSTANCES, pack_id)


def _state_path(pack_id):
    return os.path.join(instance_dir(pack_id), ".friend_state.json")


def installed_state(pack_id):
    try:
        with open(_state_path(pack_id), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def cached_manifest(pack_id):
    st = installed_state(pack_id)
    return st.get("manifest") if st else None


def sync_pack(manifest, progress=None, log=print):
    """Приводит папку экземпляра в соответствие с манифестом."""
    pid = manifest["id"]
    inst = instance_dir(pid)
    os.makedirs(inst, exist_ok=True)
    old = installed_state(pid) or {}
    old_managed = set(old.get("managed", []))

    tasks, managed = [], []
    for f in manifest.get("files", []):
        rel = f["path"]
        dest = safe_join(inst, rel)
        if f.get("mode") == "once":
            if not os.path.exists(dest):
                tasks.append((urllib.parse.urljoin(manifest["_url"], f["url"]), dest, f.get("sha1"), None))
            continue
        managed.append(rel)
        tasks.append((urllib.parse.urljoin(manifest["_url"], f["url"]), dest, f.get("sha1"), None))

    # удаляем то, что раньше раздавали, а теперь убрали из сборки
    removed = 0
    for rel in old_managed - set(managed):
        try:
            p = safe_join(inst, rel)
            if os.path.isfile(p):
                os.remove(p)
                removed += 1
        except (ValueError, OSError):
            pass

    # strict-папки: всё лишнее внутри (например, чужие моды) удаляется
    keep = set(managed)
    for folder in manifest.get("strict", []):
        base = safe_join(inst, folder)
        if not os.path.isdir(base):
            continue
        for dirpath, _d, files in os.walk(base):
            for name in files:
                full = os.path.join(dirpath, name)
                rel = os.path.relpath(full, inst).replace(os.sep, "/")
                if rel not in keep:
                    os.remove(full)
                    removed += 1
                    log(f"Удалён лишний файл: {rel}")

    download_many(tasks, progress, "Файлы сборки", workers=6)
    if removed:
        log(f"Убрано устаревших файлов: {removed}")

    with open(_state_path(pid), "w", encoding="utf-8") as fh:
        json.dump({"version": manifest.get("version", ""), "managed": managed,
                   "manifest": {k: v for k, v in manifest.items() if k != "_url"} | {"_url": manifest["_url"]}},
                  fh, ensure_ascii=False)
    return inst


def should_keep(rel, patterns):
    return any(fnmatch.fnmatch(rel, p) for p in patterns)
