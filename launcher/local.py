"""Собственные (локальные) сборки: создание, правка, моды, миры, экспорт и импорт."""
import json
import os
import re
import shutil
import time
import uuid
import xml.etree.ElementTree as ET
import zipfile

from .mojang import MANIFEST_URL
from .util import ROOT, NetError, get_json, get_text, safe_join

LOCAL = os.path.join(ROOT, "local")
EXPORTS = os.path.join(ROOT, "exports")

LOADERS = ["vanilla", "fabric", "quilt", "forge", "neoforge"]
LOADER_TITLES = {"vanilla": "Vanilla", "fabric": "Fabric", "quilt": "Quilt", "forge": "Forge", "neoforge": "NeoForge"}

FORGE_META = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml"
NEOFORGE_META = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml"

CONTENT_DIRS = ("mods", "resourcepacks", "shaderpacks", "datapacks")
TRACK = ".fl_mods.json"


# ---------- сборки ----------

def _meta_path(iid):
    return os.path.join(LOCAL, iid, "instance.json")


def game_dir(inst):
    path = os.path.join(LOCAL, inst["id"], "minecraft")
    os.makedirs(path, exist_ok=True)
    return path


def list_instances():
    out = []
    if os.path.isdir(LOCAL):
        for iid in os.listdir(LOCAL):
            try:
                with open(_meta_path(iid), encoding="utf-8") as f:
                    inst = json.load(f)
                inst["id"] = iid
                out.append(inst)
            except (OSError, ValueError):
                continue
    return sorted(out, key=lambda i: i.get("name", "").lower())


def get_instance(iid):
    try:
        with open(_meta_path(iid), encoding="utf-8") as f:
            inst = json.load(f)
    except (OSError, ValueError):
        return None
    inst["id"] = iid
    return inst


def _slug(name):
    base = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")[:24] or "pack"
    return f"{base}-{uuid.uuid4().hex[:6]}"


def save_instance(inst):
    inst = dict(inst)
    if not inst.get("id"):
        inst["id"] = _slug(inst["name"])
    os.makedirs(os.path.join(LOCAL, inst["id"]), exist_ok=True)
    with open(_meta_path(inst["id"]), "w", encoding="utf-8") as f:
        json.dump({k: v for k, v in inst.items() if k != "id"}, f, ensure_ascii=False, indent=2)
    return inst


def delete_instance(inst):
    shutil.rmtree(os.path.join(LOCAL, inst["id"]), ignore_errors=True)


def loader_label(loader):
    loader = loader or {}
    kind = loader.get("type", "vanilla")
    text = LOADER_TITLES.get(kind, kind)
    if loader.get("version"):
        text += " " + loader["version"]
    return text


def describe(inst):
    return f"Minecraft {inst.get('mc_version', '?')}  •  {loader_label(inst.get('loader'))}"


def duplicate_instance(inst, new_name):
    new = {k: v for k, v in inst.items() if k not in ("id", "icon_seed", "publish")}
    new["name"] = new_name
    new = save_instance(new)
    shutil.copytree(game_dir(inst), game_dir(new), dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns("logs", "crash-reports", "backups"))
    return new


# ---------- списки версий ----------

def mc_versions(snapshots=False):
    data = get_json(MANIFEST_URL)
    return [v["id"] for v in data["versions"] if snapshots or v["type"] == "release"]


def _xml_versions(url):
    root = ET.fromstring(get_text(url))
    return [v.text for v in root.iter("version") if v.text]


def neoforge_prefix(mc):
    """NeoForge нумеруется как <minor>.<patch>.x, где MC = 1.<minor>.<patch>."""
    parts = mc.split(".")
    if len(parts) < 2 or parts[0] != "1":
        return None
    return f"{parts[1]}.{parts[2] if len(parts) > 2 else '0'}."


def filter_forge(versions, mc):
    pre = mc + "-"
    return [v[len(pre):] for v in versions if v.startswith(pre)][::-1]


def filter_neoforge(versions, mc):
    pre = neoforge_prefix(mc)
    if not pre:
        return []
    found = [v for v in versions if v.startswith(pre)][::-1]
    return [v for v in found if "beta" not in v] + [v for v in found if "beta" in v]


def loader_versions(kind, mc):
    """Список версий загрузчика для данной версии MC, свежие первыми."""
    if kind == "vanilla":
        return []
    if kind in ("fabric", "quilt"):
        base = {"fabric": "https://meta.fabricmc.net/v2", "quilt": "https://meta.quiltmc.org/v3"}[kind]
        vs = [x["loader"]["version"] for x in get_json(f"{base}/versions/loader/{mc}")]
        pre = ("beta", "alpha", "pre")
        return [v for v in vs if not any(p in v for p in pre)] + [v for v in vs if any(p in v for p in pre)]
    if kind == "forge":
        return filter_forge(_xml_versions(FORGE_META), mc)
    if kind == "neoforge":
        return filter_neoforge(_xml_versions(NEOFORGE_META), mc)
    raise NetError(f"Неизвестный загрузчик {kind}")


# ---------- учёт установленного контента (откуда файл, чтобы показывать иконки и искать обновления) ----------

def load_track(gdir):
    try:
        with open(os.path.join(gdir, TRACK), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def save_track(gdir, data):
    os.makedirs(gdir, exist_ok=True)
    with open(os.path.join(gdir, TRACK), "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False)


# ---------- моды ----------

_info_cache = {}


def _pick_icon(icon):
    if isinstance(icon, dict) and icon:
        try:
            return icon[max(icon, key=lambda k: int(k))]
        except ValueError:
            return next(iter(icon.values()))
    return icon if isinstance(icon, str) else None


def read_jar_meta(path):
    """Достаёт название, версию, описание и путь к иконке из fabric/quilt/forge-мода."""
    try:
        key = (path, os.path.getmtime(path), os.path.getsize(path))
    except OSError:
        return {}
    if key in _info_cache:
        return _info_cache[key]
    info = {"name": None, "version": None, "description": "", "icon": None}
    try:
        with zipfile.ZipFile(path) as z:
            names = set(z.namelist())
            if "fabric.mod.json" in names:
                d = json.loads(z.read("fabric.mod.json").decode("utf-8-sig"), strict=False)
                info.update(name=d.get("name"), version=d.get("version"), description=d.get("description", ""),
                            icon=_pick_icon(d.get("icon")))
            elif "quilt.mod.json" in names:
                d = json.loads(z.read("quilt.mod.json").decode("utf-8-sig"), strict=False).get("quilt_loader", {})
                m = d.get("metadata", {})
                info.update(name=m.get("name"), version=d.get("version"), description=m.get("description", ""),
                            icon=_pick_icon(m.get("icon")))
            else:
                toml = next((n for n in ("META-INF/neoforge.mods.toml", "META-INF/mods.toml") if n in names), None)
                if toml:
                    t = z.read(toml).decode("utf-8", "replace")
                    g = lambda pat: (re.search(pat, t, re.M | re.S) or [None, None])[1]  # noqa: E731
                    info["name"] = g(r'displayName\s*=\s*"([^"]+)"')
                    info["version"] = g(r'^\s*version\s*=\s*"([^"]+)"')
                    info["description"] = (g(r"description\s*=\s*'''(.*?)'''") or g(r'description\s*=\s*"([^"]*)"') or "").strip()
                    info["icon"] = g(r'logoFile\s*=\s*"([^"]+)"')
                    if info["version"] and info["version"].startswith("${") and "META-INF/MANIFEST.MF" in names:
                        mf = z.read("META-INF/MANIFEST.MF").decode("utf-8", "replace")
                        m = re.search(r"^Implementation-Version:\s*(\S+)", mf, re.M)
                        info["version"] = m.group(1) if m else None
            if info["icon"] and info["icon"].lstrip("/") not in names:
                info["icon"] = None
    except (OSError, ValueError, KeyError, zipfile.BadZipFile):
        pass
    if len(_info_cache) > 2000:
        _info_cache.clear()
    _info_cache[key] = info
    return info


def _safe_name(name):
    """Допускаем только простое имя файла без путей."""
    if not name or name != os.path.basename(name) or name in (".", "..") or "/" in name or "\\" in name:
        raise ValueError(f"Недопустимое имя файла: {name!r}")
    return name


def list_mods(gdir):
    mdir = os.path.join(gdir, "mods")
    track = load_track(gdir)
    out = []
    if not os.path.isdir(mdir):
        return out
    for f in sorted(os.listdir(mdir), key=str.lower):
        if not f.endswith((".jar", ".jar.disabled")):
            continue
        path = os.path.join(mdir, f)
        enabled = not f.endswith(".disabled")
        base = f[:-len(".disabled")] if not enabled else f
        meta = read_jar_meta(path)
        tr = track.get("mods/" + base, {})
        out.append({
            "file": f, "enabled": enabled,
            "name": meta.get("name") or tr.get("title") or base[:-4],
            "version": meta.get("version") or tr.get("version_number") or "",
            "description": meta.get("description") or "",
            "size": os.path.getsize(path),
            "icon": bool(meta.get("icon")), "icon_url": tr.get("icon_url") or "",
            "project_id": tr.get("project_id") or "",
        })
    return out


def mod_icon(gdir, file):
    path = os.path.join(gdir, "mods", _safe_name(file))
    icon = read_jar_meta(path).get("icon")
    if not icon:
        return None
    try:
        with zipfile.ZipFile(path) as z:
            info = z.getinfo(icon.lstrip("/"))
            if info.file_size > 1 << 20:
                return None
            return z.read(info)
    except (OSError, KeyError, zipfile.BadZipFile):
        return None


def toggle_mod(gdir, file):
    _safe_name(file)
    src = os.path.join(gdir, "mods", file)
    if file.endswith(".disabled"):
        dst = src[:-len(".disabled")]
    elif file.endswith(".jar"):
        dst = src + ".disabled"
    else:
        raise ValueError("Это не мод")
    os.replace(src, dst)
    return os.path.basename(dst)


def delete_mods(gdir, files):
    track = load_track(gdir)
    for f in files:
        _safe_name(f)
        p = os.path.join(gdir, "mods", f)
        if os.path.isfile(p):
            os.remove(p)
        track.pop("mods/" + (f[:-len(".disabled")] if f.endswith(".disabled") else f), None)
    save_track(gdir, track)


def add_content_file(gdir, folder, name, src_path):
    if folder not in CONTENT_DIRS:
        raise ValueError("Неизвестная папка")
    _safe_name(name)
    dst_dir = os.path.join(gdir, folder)
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, name)
    shutil.move(src_path, dst)
    return dst


def count_files(gdir, folder, exts):
    d = os.path.join(gdir, folder)
    try:
        return sum(1 for f in os.listdir(d) if f.endswith(exts))
    except OSError:
        return 0


# ---------- миры ----------

def _dir_size(path):
    total = 0
    for dp, _d, files in os.walk(path):
        for f in files:
            try:
                total += os.path.getsize(os.path.join(dp, f))
            except OSError:
                pass
    return total


def list_worlds(gdir):
    saves = os.path.join(gdir, "saves")
    out = []
    if not os.path.isdir(saves):
        return out
    for d in os.listdir(saves):
        p = os.path.join(saves, d)
        lvl = os.path.join(p, "level.dat")
        if os.path.isfile(lvl):
            out.append({"name": d, "size": _dir_size(p), "modified": os.path.getmtime(lvl)})
    return sorted(out, key=lambda w: -w["modified"])


def _world_path(gdir, name):
    _safe_name(name)
    p = os.path.join(gdir, "saves", name)
    if not os.path.isfile(os.path.join(p, "level.dat")):
        raise ValueError("Мир не найден")
    return p


def backup_world(gdir, name):
    src = _world_path(gdir, name)
    dst_dir = os.path.join(gdir, "backups")
    os.makedirs(dst_dir, exist_ok=True)
    dst = os.path.join(dst_dir, f"{name}-{time.strftime('%Y%m%d-%H%M%S')}.zip")
    with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as z:
        for dp, _d, files in os.walk(src):
            for f in files:
                full = os.path.join(dp, f)
                if f == "session.lock":
                    continue
                z.write(full, os.path.join(name, os.path.relpath(full, src)))
    return dst


def delete_world(gdir, name):
    shutil.rmtree(_world_path(gdir, name))


# ---------- экспорт / импорт ----------

_EXPORT_SKIP = ("logs", "crash-reports", "backups", ".fl_", "usercache", "usernamecache", "launcher_profiles")


def export_zip(inst, include_worlds=False):
    os.makedirs(EXPORTS, exist_ok=True)
    safe = re.sub(r'[\\/:*?"<>|]+', "_", inst["name"]).strip() or "instance"
    dst = os.path.join(EXPORTS, f"{safe}-{time.strftime('%Y%m%d-%H%M')}.zip")
    gdir = game_dir(inst)
    meta = {k: v for k, v in inst.items() if k not in ("id", "publish")}
    with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("instance.json", json.dumps(meta, ensure_ascii=False, indent=2))
        for dp, dirs, files in os.walk(gdir):
            rel_dir = os.path.relpath(dp, gdir)
            top = rel_dir.split(os.sep)[0]
            if top == "saves" and not include_worlds:
                dirs[:] = []
                continue
            dirs[:] = [d for d in dirs if not (rel_dir == "." and d.startswith(_EXPORT_SKIP))]
            for f in files:
                if f.startswith(_EXPORT_SKIP) and rel_dir == ".":
                    continue
                full = os.path.join(dp, f)
                z.write(full, "minecraft/" + os.path.relpath(full, gdir).replace(os.sep, "/"))
    return dst


def extract_prefix(z, prefix, dest, skip=()):
    """Распаковывает записи архива с префиксом prefix в dest (с защитой от ../)."""
    for info in z.infolist():
        name = info.filename
        if info.is_dir() or not name.startswith(prefix):
            continue
        rel = name[len(prefix):]
        if not rel or any(rel.startswith(s) for s in skip):
            continue
        target = safe_join(dest, rel)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with z.open(info) as src, open(target, "wb") as dst:
            shutil.copyfileobj(src, dst)


_MMC_LOADERS = {"net.fabricmc.fabric-loader": "fabric", "org.quiltmc.quilt-loader": "quilt",
                "net.minecraftforge": "forge", "net.neoforged": "neoforge"}


def import_archive(path, progress=None, log=print):
    """Импорт .mrpack (Modrinth), нашего экспорта или zip из Prism/MultiMC. Возвращает сборку."""
    with zipfile.ZipFile(path) as z:
        names = set(z.namelist())
        if "modrinth.index.json" in names:
            from . import mrpack
            return mrpack.import_mrpack(path, progress, log)
        if "manifest.json" in names and "modrinth.index.json" not in names and "instance.json" not in names:
            from . import curseforge
            return curseforge.import_pack(path, progress, log)
        if "instance.json" in names:
            meta = json.loads(z.read("instance.json").decode("utf-8"))
            meta.pop("id", None)
            meta.setdefault("name", os.path.splitext(os.path.basename(path))[0])
            inst = save_instance(meta)
            try:
                extract_prefix(z, "minecraft/", game_dir(inst))
            except BaseException:
                delete_instance(inst)
                raise
            return inst
        # Prism / MultiMC: mmc-pack.json + instance.cfg + .minecraft/
        root = next((n[:-len("mmc-pack.json")] for n in names if n.endswith("mmc-pack.json")), None)
        if root is not None:
            pack = json.loads(z.read(root + "mmc-pack.json").decode("utf-8"))
            mc, loader = None, {"type": "vanilla", "version": ""}
            for c in pack.get("components", []):
                uid = c.get("uid")
                if uid == "net.minecraft":
                    mc = c.get("version")
                elif uid in _MMC_LOADERS:
                    loader = {"type": _MMC_LOADERS[uid], "version": c.get("version", "")}
            if not mc:
                raise ValueError("В mmc-pack.json не найдена версия Minecraft")
            name = os.path.splitext(os.path.basename(path))[0]
            if root + "instance.cfg" in names:
                m = re.search(r"^name=(.+)$", z.read(root + "instance.cfg").decode("utf-8", "replace"), re.M)
                if m:
                    name = m.group(1).strip()
            inst = save_instance({"name": name, "mc_version": mc, "loader": loader})
            try:
                for pre in (root + ".minecraft/", root + "minecraft/"):
                    extract_prefix(z, pre, game_dir(inst))
            except BaseException:
                delete_instance(inst)
                raise
            return inst
    raise ValueError("Не удалось распознать архив. Поддерживаются .mrpack, сборки CurseForge, экспорт FriendLauncher и zip из Prism/MultiMC.")
