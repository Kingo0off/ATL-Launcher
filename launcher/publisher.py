"""Публикация сборки для друзей: папка сборки → папка сайта (index.json + манифест + файлы)."""
import fnmatch
import hashlib
import json
import os
import re
import shutil

IGNORE_DIRS = {"saves", "logs", "crash-reports", "screenshots", "backups", ".git", ".fabric", "downloads",
               "versions", "libraries", "assets", "runtime", "natives"}
IGNORE_FILES = {".friend_state.json", ".fl_mods.json", "pack.json", ".DS_Store", "Thumbs.db", "usercache.json",
                "usernamecache.json", "launcher_profiles.json", "realms_persistence.json", "hotkeys.json"}
DEFAULT_ONCE = ["options.txt", "servers.dat", "optionsof.txt"]


def _sha1(path):
    h = hashlib.sha1()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def slugify(name):
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:32]


def bump_version(version):
    """1.0.3 → 1.0.4; если не число на конце — дописываем .1"""
    m = re.match(r"^(.*?)(\d+)$", version or "")
    return f"{m.group(1)}{int(m.group(2)) + 1}" if m else (version + ".1" if version else "1.0.0")


def publish_pack(folder, out, meta):
    """meta: id, name, description, version, mc_version, loader{type,version}, java_args, once, strict.
    Возвращает словарь с итогами. Ошибки ввода — ValueError."""
    pack_id = meta.get("id") or ""
    if not pack_id or not all(c.isalnum() or c in "-_" for c in pack_id) or not pack_id.isascii():
        raise ValueError("id должен состоять из латинских букв, цифр, - и _")
    if not meta.get("mc_version") or not meta.get("version"):
        raise ValueError("Нужны версия Minecraft и версия сборки")
    loader = meta.get("loader") or {"type": "vanilla", "version": ""}
    if loader.get("type") in ("forge", "neoforge") and not loader.get("version"):
        raise ValueError(f"Для {loader['type']} нужна версия загрузчика")
    if not os.path.isdir(folder):
        raise ValueError("Папка сборки не найдена")

    once = DEFAULT_ONCE if meta.get("once") is None else meta["once"]
    strict = ["mods"] if meta.get("strict") is None else meta["strict"]
    out = os.path.abspath(out)
    pack_out = os.path.join(out, "packs", pack_id)
    files_out = os.path.join(pack_out, "files")
    shutil.rmtree(pack_out, ignore_errors=True)
    os.makedirs(files_out)

    files = []
    for dirpath, dirs, names in os.walk(folder):
        dirs[:] = sorted(d for d in dirs if d not in IGNORE_DIRS and not d.startswith("."))
        for n in sorted(names):
            if n in IGNORE_FILES or n.endswith((".part", ".tmp", ".log", ".lock")):
                continue
            full = os.path.join(dirpath, n)
            rel = os.path.relpath(full, folder).replace(os.sep, "/")
            dest = os.path.join(files_out, *rel.split("/"))
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            shutil.copyfile(full, dest)
            entry = {"path": rel, "url": "files/" + rel.replace(" ", "%20"), "sha1": _sha1(full),
                     "size": os.path.getsize(full)}
            if any(fnmatch.fnmatch(rel, p) for p in once):
                entry["mode"] = "once"
            files.append(entry)

    manifest = {"id": pack_id, "name": meta.get("name") or pack_id, "version": meta["version"],
                "mc_version": meta["mc_version"], "loader": loader, "files": files, "strict": strict}
    if meta.get("java_args"):
        manifest["java_args"] = meta["java_args"]
    if meta.get("ram_mb"):
        manifest["ram_mb"] = meta["ram_mb"]
    with open(os.path.join(pack_out, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)

    index_path = os.path.join(out, "index.json")
    index = {"packs": []}
    if os.path.isfile(index_path):
        with open(index_path, encoding="utf-8") as f:
            index = json.load(f)
    entry = {"id": pack_id, "name": manifest["name"], "description": meta.get("description", ""),
             "version": meta["version"], "mc_version": meta["mc_version"], "loader": loader,
             "manifest": f"packs/{pack_id}/manifest.json"}
    index["packs"] = [p for p in index["packs"] if p["id"] != pack_id] + [entry]
    index["packs"].sort(key=lambda p: p.get("name", p["id"]).lower())
    os.makedirs(out, exist_ok=True)
    with open(index_path, "w", encoding="utf-8") as f:
        json.dump(index, f, ensure_ascii=False, indent=2)

    total = sum(f["size"] for f in files)
    return {"name": manifest["name"], "version": meta["version"], "files": len(files), "size": total,
            "out": out, "big": [f["path"] for f in files if f["size"] > 95 * 1048576]}


def remove_pack(out, pack_id):
    index_path = os.path.join(out, "index.json")
    with open(index_path, encoding="utf-8") as f:
        index = json.load(f)
    index["packs"] = [p for p in index["packs"] if p["id"] != pack_id]
    shutil.rmtree(os.path.join(out, "packs", pack_id), ignore_errors=True)
    with open(index_path, "w", encoding="utf-8") as f:
        json.dump(index, f, ensure_ascii=False, indent=2)
