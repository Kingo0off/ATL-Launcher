"""Каталог CurseForge: поиск, установка модов/ресурспаков/шейдеров и импорт сборок CurseForge.

Нужен бесплатный ключ API (console.curseforge.com), он вводится в настройках лаунчера.
Если автор мода запретил скачивание сторонними программами, API не отдаёт ссылку на файл:
такой файл мы не обходим, а предлагаем скачать со страницы CurseForge.
"""
import json
import os
import re
import urllib.parse
import zipfile

from . import local
from .util import NetError, download, download_many, http_json, safe_join, sha1_file

API = "https://api.curseforge.com/v1"
GAME_ID = 432  # Minecraft
CLASS_IDS = {"mod": 6, "modpack": 4471, "resourcepack": 12, "shader": 6552, "datapack": 6945}
CLASS_DIRS = {6: "mods", 12: "resourcepacks", 6552: "shaderpacks", 6945: "datapacks"}
LOADER_IDS = {"forge": 1, "fabric": 4, "quilt": 5, "neoforge": 6}
SORTS = {"relevance": 1, "downloads": 6, "newest": 11, "updated": 3}
PREFIX = "cf:"

KEY = ""  # выставляет сервер из настроек


class NoKey(NetError):
    pass


class NoDownload(NetError):
    """Автор запретил раздачу файла сторонним лаунчерам."""


def _headers():
    if not (KEY or "").strip():
        raise NoKey("Нужен ключ API CurseForge: получи бесплатный на console.curseforge.com и вставь его в «Настройки».")
    return {"x-api-key": KEY.strip()}


def _call(path, params=None, body=None):
    url = API + path
    if params:
        url += "?" + urllib.parse.urlencode(params)
    st, js = http_json(url, body=body, headers=_headers())
    if st in (401, 403):
        raise NoKey("CurseForge не принял ключ API. Проверь его в «Настройках».")
    if st != 200:
        raise NetError(f"CurseForge: HTTP {st} {js.get('error') or js.get('_raw') or ''}".strip())
    return js


def _hit(m, ptype):
    authors = m.get("authors") or []
    return {
        "project_id": PREFIX + str(m["id"]), "slug": m.get("slug"), "title": m.get("name", ""),
        "description": m.get("summary", ""), "icon_url": (m.get("logo") or {}).get("thumbnailUrl") or (m.get("logo") or {}).get("url") or "",
        "downloads": int(m.get("downloadCount") or 0), "follows": m.get("thumbsUpCount", 0),
        "author": authors[0].get("name", "") if authors else "",
        "categories": [c.get("name", "") for c in (m.get("categories") or [])][:4],
        "project_type": ptype, "site_url": (m.get("links") or {}).get("websiteUrl") or "",
        "blocked": m.get("allowModDistribution") is False,
    }


def search(query="", project_type="mod", mc=None, loader=None, sort="relevance", offset=0, limit=20):
    params = {"gameId": GAME_ID, "classId": CLASS_IDS.get(project_type, 6), "pageSize": limit, "index": offset,
              "sortField": SORTS.get(sort, 1), "sortOrder": "desc"}
    if query:
        params["searchFilter"] = query
    if mc:
        params["gameVersion"] = mc
    if loader in LOADER_IDS and project_type in ("mod", "modpack"):
        params["modLoaderType"] = LOADER_IDS[loader]
    js = _call("/mods/search", params)
    pg = js.get("pagination") or {}
    hits = [_hit(m, project_type) for m in js.get("data", [])]
    return {"hits": hits, "total": min(pg.get("totalCount", len(hits)), 10000 - limit)}


def _cid(project_id):
    s = str(project_id)
    return int(s[len(PREFIX):] if s.startswith(PREFIX) else s)


def _files(mod_id, mc=None, loader=None, ptype="mod"):
    params = {"pageSize": 50}
    if mc and ptype != "modpack":
        params["gameVersion"] = mc
    if loader in LOADER_IDS and ptype == "mod":
        params["modLoaderType"] = LOADER_IDS[loader]
    files = _call(f"/mods/{mod_id}/files", params).get("data", [])
    return sorted(files, key=lambda f: f.get("fileDate", ""), reverse=True)


def _pick(files):
    """Свежий релиз; если релизов нет — свежая бета или альфа."""
    return next((f for f in files if f.get("releaseType") == 1), files[0] if files else None)


def _sha1(f):
    return next((h["value"] for h in f.get("hashes", []) if h.get("algo") == 1), None)


def _need_url(f, title, site=""):
    url = f.get("downloadUrl")
    if not url:
        raise NoDownload(f"Автор «{title}» запретил скачивание сторонними лаунчерами. "
                         f"Скачай файл на сайте CurseForge и добавь его вручную (вкладка «Моды» → «Добавить файл»)."
                         + (f" Страница: {site}" if site else ""))
    return url


def install(gdir, mc, loader, project_id, project_type="mod", meta=None, with_deps=True, _seen=None, _depth=0):
    meta = meta or {}
    mod_id = _cid(project_id)
    seen = _seen if _seen is not None else set()
    seen.add(mod_id)
    title = meta.get("title") or str(mod_id)
    files = _files(mod_id, mc, loader, project_type)
    f = _pick(files)
    if not f:
        raise NetError(f"Для Minecraft {mc} ({local.LOADER_TITLES.get(loader, loader)}) у «{title}» нет подходящего файла")
    url = _need_url(f, title, meta.get("site_url", ""))
    folder = CLASS_DIRS.get(CLASS_IDS.get(project_type, 6), "mods")
    name = local._safe_name(os.path.basename(f["fileName"]))
    dest = os.path.join(gdir, folder, name)
    download(url, dest, sha1=_sha1(f))
    track = local.load_track(gdir)
    track[f"{folder}/{name}"] = {"project_id": PREFIX + str(mod_id), "source": "curseforge", "version_id": str(f["id"]),
                                 "version_number": f.get("displayName") or name, "title": title,
                                 "icon_url": meta.get("icon_url") or "", "sha1": _sha1(f) or sha1_file(dest)}
    local.save_track(gdir, track)
    installed = [f"{folder}/{name}"]
    if with_deps and _depth < 4:
        for dep in f.get("dependencies", []):
            did = dep.get("modId")
            if dep.get("relationType") != 3 or not did or did in seen:  # 3 = обязательная зависимость
                continue
            if any(t.get("project_id") == PREFIX + str(did) for t in local.load_track(gdir).values()):
                seen.add(did)
                continue
            info = _call(f"/mods/{did}").get("data", {})
            ptype = next((k for k, v in CLASS_IDS.items() if v == info.get("classId")), "mod")
            installed += install(gdir, mc, loader, PREFIX + str(did), ptype,
                                 {"title": info.get("name"), "icon_url": (info.get("logo") or {}).get("thumbnailUrl"),
                                  "site_url": (info.get("links") or {}).get("websiteUrl", "")}, True, seen, _depth + 1)
    return installed


def modpack_file(project_id, title="", site_url=""):
    """Скачивает не здесь: возвращает (url, filename, sha1)."""
    f = _pick(_files(_cid(project_id), ptype="modpack"))
    if not f:
        raise NetError("У сборки нет файлов")
    return _need_url(f, title or "сборки", site_url), local._safe_name(os.path.basename(f["fileName"])), _sha1(f)


# ---------- импорт сборки CurseForge (manifest.json) ----------

_LOADER_RE = re.compile(r"^(forge|fabric|quilt|neoforge)-(.+)$")


def is_pack(names):
    return "manifest.json" in names


def import_pack(path, progress=None, log=print):
    with zipfile.ZipFile(path) as z:
        man = json.loads(z.read("manifest.json").decode("utf-8-sig"))
        mc = (man.get("minecraft") or {}).get("version")
        if not mc:
            raise ValueError("В сборке CurseForge не указана версия Minecraft")
        loader = {"type": "vanilla", "version": ""}
        mls = (man.get("minecraft") or {}).get("modLoaders") or []
        ml = next((m for m in mls if m.get("primary")), mls[0] if mls else None)
        m = _LOADER_RE.match((ml or {}).get("id", ""))
        if m:
            loader = {"type": m.group(1), "version": m.group(2)}
        entries = [f for f in man.get("files", []) if f.get("required", True)]
        ids = sorted({f["fileID"] for f in entries})
        mod_ids = sorted({f["projectID"] for f in entries})
        by_file, by_mod = {}, {}
        for i in range(0, len(ids), 200):
            for f in _call("/mods/files", body={"fileIds": ids[i:i + 200]}).get("data", []):
                by_file[f["id"]] = f
        for i in range(0, len(mod_ids), 200):
            for mod in _call("/mods", body={"modIds": mod_ids[i:i + 200]}).get("data", []):
                by_mod[mod["id"]] = mod
        tasks, skipped, track = [], [], {}
        inst = local.save_instance({"name": man.get("name") or os.path.splitext(os.path.basename(path))[0],
                                    "mc_version": mc, "loader": loader, "ram_mb": 0, "java_args": "",
                                    "notes": (man.get("author") and f"Автор: {man['author']}") or ""})
        gdir = local.game_dir(inst)
        try:
            for e in entries:
                f, mod = by_file.get(e["fileID"]), by_mod.get(e["projectID"], {})
                title = mod.get("name") or str(e["projectID"])
                if not f or not f.get("downloadUrl"):
                    skipped.append({"title": title, "url": (mod.get("links") or {}).get("websiteUrl", "")})
                    continue
                folder = CLASS_DIRS.get(mod.get("classId"), "mods")
                name = local._safe_name(os.path.basename(f["fileName"]))
                tasks.append((f["downloadUrl"], os.path.join(gdir, folder, name), _sha1(f), None))
                track[f"{folder}/{name}"] = {"project_id": PREFIX + str(e["projectID"]), "source": "curseforge",
                                             "version_id": str(f["id"]), "title": title,
                                             "icon_url": (mod.get("logo") or {}).get("thumbnailUrl") or "", "sha1": _sha1(f)}
            log(f"Скачиваю файлов сборки: {len(tasks)}")
            download_many(tasks, progress, "Файлы сборки", workers=6)
            local.extract_prefix(z, (man.get("overrides") or "overrides").strip("/") + "/", gdir)
            local.save_track(gdir, track)
        except BaseException:
            local.delete_instance(inst)
            raise
    if skipped:
        inst["skipped"] = skipped
        log(f"Не скачано из-за запрета авторов: {len(skipped)}")
    return inst
