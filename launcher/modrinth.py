"""Каталог Modrinth: поиск, установка модов/ресурспаков/шейдеров с зависимостями, поиск обновлений."""
import json
import os
import urllib.parse

from . import local
from .util import NetError, download, http_json, sha1_file

API = "https://api.modrinth.com/v2"
TYPE_DIRS = {"mod": "mods", "resourcepack": "resourcepacks", "shader": "shaderpacks", "datapack": "datapacks"}


def _get(path, params=None):
    url = API + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    st, js = http_json(url)
    if st != 200:
        raise NetError(f"Modrinth: HTTP {st} {js.get('description') or js.get('error') or ''}".strip())
    return js


def loader_list(kind):
    """Какие загрузчики Modrinth считать подходящими."""
    return {"fabric": ["fabric"], "quilt": ["quilt", "fabric"], "forge": ["forge"], "neoforge": ["neoforge"]}.get(kind, [])


def search(query="", project_type="mod", mc=None, loader=None, sort="relevance", offset=0, limit=20):
    facets = [[f"project_type:{project_type}"]]
    if mc:
        facets.append([f"versions:{mc}"])
    if loader and project_type in ("mod", "modpack"):
        ls = loader_list(loader) or [loader]
        facets.append([f"categories:{x}" for x in ls])
    data = _get("/search", {"query": query, "facets": json.dumps(facets), "limit": limit, "offset": offset,
                            "index": sort})
    hits = [{
        "project_id": h["project_id"], "slug": h.get("slug"), "title": h["title"],
        "description": h.get("description", ""), "icon_url": h.get("icon_url") or "",
        "downloads": h.get("downloads", 0), "follows": h.get("follows", 0), "author": h.get("author", ""),
        "categories": [c for c in h.get("display_categories") or h.get("categories", [])][:4],
        "project_type": h.get("project_type", project_type),
    } for h in data.get("hits", [])]
    return {"hits": hits, "total": data.get("total_hits", len(hits))}


def project_versions(project_id, mc=None, loader=None, project_type="mod"):
    params = {}
    if mc and project_type != "modpack":
        params["game_versions"] = json.dumps([mc])
    if loader and project_type in ("mod",):
        params["loaders"] = json.dumps(loader_list(loader) or [loader])
    return _get(f"/project/{project_id}/version", params)


def pick_version(versions):
    """Свежая стабильная версия, а если стабильных нет — просто свежая."""
    for v in versions:
        if v.get("version_type") == "release":
            return v
    return versions[0] if versions else None


def primary_file(version):
    files = version.get("files", [])
    return next((f for f in files if f.get("primary")), files[0] if files else None)


def _install_version(gdir, version, project_type, meta, installed, track):
    f = primary_file(version)
    if not f:
        raise NetError(f"У версии {version.get('version_number')} нет файлов")
    folder = TYPE_DIRS.get(project_type, "mods")
    name = local._safe_name(os.path.basename(f["filename"]))
    dest = os.path.join(gdir, folder, name)
    download(f["url"], dest, sha1=(f.get("hashes") or {}).get("sha1"))
    track[f"{folder}/{name}"] = {
        "project_id": version["project_id"], "version_id": version["id"], "version_number": version.get("version_number"),
        "title": meta.get("title") or name, "icon_url": meta.get("icon_url") or "",
        "sha1": (f.get("hashes") or {}).get("sha1") or sha1_file(dest)}
    installed.append(f"{folder}/{name}")


def install(gdir, mc, loader, project_id, project_type="mod", meta=None, with_deps=True, _seen=None, _depth=0):
    """Ставит проект (и обязательные зависимости) в папку сборки. Возвращает список установленных файлов."""
    meta = meta or {}
    seen = _seen if _seen is not None else set()
    seen.add(project_id)
    versions = project_versions(project_id, mc, loader, project_type)
    version = pick_version(versions)
    if not version:
        raise NetError(f"Для Minecraft {mc} ({local.LOADER_TITLES.get(loader, loader)}) у «{meta.get('title') or project_id}» нет подходящей версии")
    track = local.load_track(gdir)
    installed = []
    _install_version(gdir, version, project_type, meta, installed, track)
    local.save_track(gdir, track)
    if with_deps and _depth < 4:
        for dep in version.get("dependencies", []):
            pid = dep.get("project_id")
            if dep.get("dependency_type") != "required" or not pid or pid in seen:
                continue
            if any(t.get("project_id") == pid for t in local.load_track(gdir).values()):
                seen.add(pid)
                continue
            info = _get(f"/project/{pid}")
            installed += install(gdir, mc, loader, pid, info.get("project_type", "mod"),
                                 {"title": info.get("title"), "icon_url": info.get("icon_url")}, True, seen, _depth + 1)
    return installed


def modpack_file(project_id):
    """Скачиваемый .mrpack последней версии модпака. Возвращает (url, filename, sha1, title)."""
    v = pick_version(_get(f"/project/{project_id}/version"))
    if not v:
        raise NetError("У модпака нет версий")
    f = primary_file(v)
    return f["url"], os.path.basename(f["filename"]), (f.get("hashes") or {}).get("sha1"), v.get("name")


# ---------- обновления ----------

def check_updates(gdir, mc, loader):
    """Ищет по хешам более новые версии для всех включённых модов. Возвращает список обновлений."""
    mdir = os.path.join(gdir, "mods")
    hashes = {}
    if os.path.isdir(mdir):
        for f in os.listdir(mdir):
            if f.endswith(".jar"):
                hashes[sha1_file(os.path.join(mdir, f))] = f
    if not hashes:
        return []
    body = {"hashes": list(hashes), "algorithm": "sha1", "game_versions": [mc]}
    ls = loader_list(loader)
    if ls:
        body["loaders"] = ls
    st, js = http_json(API + "/version_files/update", body=body)
    if st != 200:
        raise NetError(f"Modrinth: HTTP {st}")
    out = []
    for h, ver in js.items():
        f = primary_file(ver)
        new_hash = (f.get("hashes") or {}).get("sha1") if f else None
        if not f or not new_hash or new_hash == h or h not in hashes:
            continue
        out.append({"file": hashes[h], "project_id": ver["project_id"], "version_id": ver["id"],
                    "version_number": ver.get("version_number"), "url": f["url"],
                    "filename": os.path.basename(f["filename"]), "sha1": new_hash})
    return out


def apply_update(gdir, item):
    mdir = os.path.join(gdir, "mods")
    new_name = local._safe_name(item["filename"])
    download(item["url"], os.path.join(mdir, new_name), sha1=item["sha1"])
    old = os.path.join(mdir, local._safe_name(item["file"]))
    if new_name != item["file"] and os.path.isfile(old):
        os.remove(old)
    track = local.load_track(gdir)
    old_entry = track.pop("mods/" + item["file"], {})
    track["mods/" + new_name] = {**old_entry, "project_id": item["project_id"], "version_id": item["version_id"],
                                 "version_number": item["version_number"], "sha1": item["sha1"]}
    local.save_track(gdir, track)
