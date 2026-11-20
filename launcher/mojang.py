"""Установка ванильного Minecraft: версия, библиотеки, ассеты, нативы, аргументы запуска."""
import json
import os
import re
import shutil
import zipfile

from .util import ARCH, IS_64, OS_NAME, ROOT, NetError, download, get_json

MANIFEST_URL = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
ASSET_URL = "https://resources.download.minecraft.net"

VERSIONS = os.path.join(ROOT, "versions")
LIBS = os.path.join(ROOT, "libraries")
ASSETS = os.path.join(ROOT, "assets")
NATIVES = os.path.join(ROOT, "natives")


# ---------- версии ----------

def install_vanilla_json(mc):
    path = os.path.join(VERSIONS, mc, mc + ".json")
    if os.path.isfile(path):
        return
    manifest = get_json(MANIFEST_URL)
    for v in manifest["versions"]:
        if v["id"] == mc:
            download(v["url"], path, sha1=v.get("sha1"))
            return
    raise NetError(f"Версия Minecraft {mc} не найдена в манифесте Mojang")


def _lib_key(name):
    parts = name.split("@")[0].split(":")
    return ":".join([parts[0], parts[1]] + parts[3:4])


def merge(parent, child):
    r = dict(parent)
    r.update({k: v for k, v in child.items() if k not in ("libraries", "arguments", "inheritsFrom")})
    libs = list(child.get("libraries", []))
    seen = {_lib_key(l["name"]) for l in libs}
    libs += [l for l in parent.get("libraries", []) if _lib_key(l["name"]) not in seen]
    r["libraries"] = libs
    if "arguments" in parent or "arguments" in child:
        pa, ca = parent.get("arguments", {}), child.get("arguments", {})
        r["arguments"] = {
            "game": pa.get("game", []) + ca.get("game", []),
            "jvm": pa.get("jvm", []) + ca.get("jvm", []),
        }
    r["_jar_id"] = parent["_jar_id"]
    return r


def load_version(vid):
    with open(os.path.join(VERSIONS, vid, vid + ".json"), encoding="utf-8") as f:
        v = json.load(f)
    if "inheritsFrom" in v:
        return merge(load_version(v["inheritsFrom"]), v)
    v["_jar_id"] = v.get("jar", v["id"])
    return v


# ---------- правила и аргументы ----------

def _rule_match(rule):
    if "features" in rule:
        return False  # демо-режим, кастомное разрешение и т.п. мы не включаем
    os_ = rule.get("os")
    if os_:
        if "name" in os_ and os_["name"] != OS_NAME:
            return False
        if "arch" in os_ and os_["arch"] != ARCH:
            return False
    return True


def rules_ok(rules):
    if not rules:
        return True
    allowed = False
    for r in rules:
        if _rule_match(r):
            allowed = r.get("action") == "allow"
    return allowed


def _sub(s, variables):
    return re.sub(r"\$\{(\w+)\}", lambda m: str(variables.get(m.group(1), "")), s)


def expand_args(args, variables):
    out = []
    for a in args:
        if isinstance(a, str):
            out.append(_sub(a, variables))
        elif rules_ok(a.get("rules")):
            val = a["value"]
            for x in ([val] if isinstance(val, str) else val):
                out.append(_sub(x, variables))
    return out


def build_arguments(v, variables):
    """Возвращает (jvm_args, game_args) для новых и старых версий."""
    if "arguments" in v:
        jvm = expand_args(v["arguments"].get("jvm", []), variables)
        game = expand_args(v["arguments"].get("game", []), variables)
    else:
        jvm = expand_args(["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"], variables)
        game = [_sub(x, variables) for x in v.get("minecraftArguments", "").split()]
    if "-cp" not in jvm and "-classpath" not in jvm:
        jvm += ["-cp", variables["classpath"]]
    if not any(a.startswith("-Djava.library.path=") for a in jvm):
        jvm.append("-Djava.library.path=" + variables["natives_directory"])
    return jvm, game


# ---------- библиотеки ----------

def maven_path(name):
    ext = "jar"
    if "@" in name:
        name, ext = name.split("@", 1)
    p = name.split(":")
    group, art, ver = p[0], p[1], p[2]
    cls = "-" + p[3] if len(p) > 3 else ""
    return f"{group.replace('.', '/')}/{art}/{ver}/{art}-{ver}{cls}.{ext}"


def collect_libraries(v):
    """Возвращает (tasks, classpath, natives). tasks — список для download_many."""
    tasks, cp, natives = [], [], []
    for lib in v.get("libraries", []):
        if not rules_ok(lib.get("rules")):
            continue
        arts = []  # (path, url, sha1, size, is_native)
        dl = lib.get("downloads")
        if dl:
            a = dl.get("artifact")
            if a and a.get("path"):
                arts.append((a["path"], a.get("url", ""), a.get("sha1"), a.get("size"), False))
            cls = (lib.get("natives") or {}).get(OS_NAME)
            if cls:
                cls = cls.replace("${arch}", "64" if IS_64 else "32")
                n = dl.get("classifiers", {}).get(cls)
                if n:
                    arts.append((n["path"], n.get("url", ""), n.get("sha1"), n.get("size"), True))
        else:
            path = maven_path(lib["name"])
            base = lib.get("url", "https://libraries.minecraft.net/").rstrip("/")
            arts.append((path, f"{base}/{path}", lib.get("sha1"), lib.get("size"), False))
        for path, url, sha1, size, is_native in arts:
            dest = os.path.join(LIBS, *path.split("/"))
            if url:
                tasks.append((url, dest, sha1, size))
            if is_native:
                natives.append((dest, (lib.get("extract") or {}).get("exclude", [])))
            elif dest not in cp:
                cp.append(dest)
    return tasks, cp, natives


def extract_natives(natives, vid):
    out = os.path.join(NATIVES, vid)
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out, exist_ok=True)
    for jar, exclude in natives:
        with zipfile.ZipFile(jar) as z:
            for info in z.infolist():
                if info.is_dir() or info.filename.startswith("META-INF"):
                    continue
                if any(info.filename.startswith(e) for e in exclude):
                    continue
                target = os.path.abspath(os.path.join(out, info.filename))
                if not target.startswith(os.path.abspath(out) + os.sep):
                    continue
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with z.open(info) as src, open(target, "wb") as dst:
                    shutil.copyfileobj(src, dst)
    return out


# ---------- клиент и ассеты ----------

def client_task(v):
    d = v["downloads"]["client"]
    jar = os.path.join(VERSIONS, v["_jar_id"], v["_jar_id"] + ".jar")
    return (d["url"], jar, d.get("sha1"), d.get("size")), jar


def asset_tasks(v):
    ai = v["assetIndex"]
    idx_path = os.path.join(ASSETS, "indexes", ai["id"] + ".json")
    download(ai["url"], idx_path, sha1=ai.get("sha1"))
    with open(idx_path, encoding="utf-8") as f:
        idx = json.load(f)
    tasks = []
    for o in idx["objects"].values():
        h = o["hash"]
        tasks.append((f"{ASSET_URL}/{h[:2]}/{h}", os.path.join(ASSETS, "objects", h[:2], h), None, o["size"]))
    return tasks, idx


def finish_legacy_assets(v, idx, game_dir):
    """Для очень старых версий (≤1.7) ассеты нужны по «человеческим» путям. Возвращает game_assets."""
    root = ASSETS
    target = None
    if idx.get("map_to_resources"):
        target = os.path.join(game_dir, "resources")
    elif idx.get("virtual"):
        target = os.path.join(ASSETS, "virtual", v["assetIndex"]["id"])
    if target:
        for name, o in idx["objects"].items():
            src = os.path.join(ASSETS, "objects", o["hash"][:2], o["hash"])
            dst = os.path.join(target, *name.split("/"))
            if not os.path.exists(dst):
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.copyfile(src, dst)
        root = target
    return root


def install_game_files(vid, game_dir, progress=None):
    """Докачивает клиент, библиотеки и ассеты. Возвращает (version, classpath_str, natives_dir, jar, game_assets)."""
    from .util import download_many
    v = load_version(vid)
    lib_tasks, cp, natives = collect_libraries(v)
    ctask, jar = client_task(v)
    download_many([ctask] + lib_tasks, progress, "Библиотеки")
    a_tasks, idx = asset_tasks(v)
    download_many(a_tasks, progress, "Ресурсы игры")
    game_assets = finish_legacy_assets(v, idx, game_dir)
    natdir = extract_natives(natives, vid)
    sep = os.pathsep
    classpath = sep.join(cp + [jar])
    return v, classpath, natdir, jar, game_assets
