"""Локальный веб-сервер: отдаёт интерфейс и API.

Безопасность: слушает только 127.0.0.1, секретный токен входит в адрес страницы (/<токен>/...),
заголовок Host проверяется (защита от DNS rebinding). Без токена чужая веб-страница ничего не сможет.
"""
import json
import mimetypes
import os
import secrets
import subprocess
import sys
import threading
import time
import traceback
import urllib.parse
import webbrowser
import zipfile
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from . import APP_VERSION, auth, config, curseforge, launch, local, modrinth, packs, publisher, shots, skins
from .util import OS_NAME, ROOT, NetError, download

WEB_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "web")
STATS_FILE = os.path.join(ROOT, "stats.json")
LOG_FILE = os.path.join(ROOT, "launcher.log")
TMP_DIR = os.path.join(ROOT, "tmp")
SUBFOLDERS = ("", "mods", "saves", "logs", "resourcepacks", "shaderpacks", "screenshots", "backups", "config")
PUBLIC_SETTINGS = ("server_url", "ms_client_id", "cf_api_key", "ram_mb", "java_path", "site_dir")
MAX_UPLOAD = {"import": 4 << 30, "content": 1 << 29, "skin": 1 << 20}


class ApiError(Exception):
    pass


def app_log(text):
    try:
        if os.path.isfile(LOG_FILE) and os.path.getsize(LOG_FILE) > 1 << 20:
            os.remove(LOG_FILE)
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(time.strftime("%Y-%m-%d %H:%M:%S ") + text + "\n")
    except OSError:
        pass


def open_path(path):
    os.makedirs(path, exist_ok=True)
    if OS_NAME == "windows":
        os.startfile(path)  # noqa: S606
    else:
        subprocess.Popen(["open" if OS_NAME == "osx" else "xdg-open", path], stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class Task:
    """Запущенная (или готовящаяся к запуску) игра и её журнал."""

    def __init__(self, key):
        self.key = key
        self.phase = "preparing"   # preparing → running → exited | crashed | error
        self.proc = None
        self.error = ""
        self.code = None
        self.started = time.time()
        self._lines = deque(maxlen=4000)
        self._base = 0
        self._lock = threading.Lock()

    def add(self, text):
        with self._lock:
            if len(self._lines) == self._lines.maxlen:
                self._base += 1
            self._lines.append(str(text))

    def read(self, since):
        with self._lock:
            start = max(since, self._base)
            return list(self._lines)[start - self._base:], self._base + len(self._lines)


class Api:
    def __init__(self):
        self.settings = config.load()
        curseforge.KEY = self.settings.get("cf_api_key", "")
        self.lock = threading.RLock()
        self.tasks = {}
        self.prep_lock = threading.Lock()
        self.packs, self.index_url, self.packs_error = [], None, ""
        self.progress = {}
        self.notices, self.notice_id = [], 0
        self.ms_flow = None
        self.updates = {}
        self._cache = {}
        self.last_ping, self.pings, self.bye_at = time.time(), 0, None
        os.makedirs(TMP_DIR, exist_ok=True)

    # ---------- служебное ----------
    def notice(self, kind, text):
        with self.lock:
            self.notice_id += 1
            self.notices = (self.notices + [{"id": self.notice_id, "kind": kind, "text": text}])[-30:]

    def progress_cb(self, tag):
        def cb(done, total, label):
            self.progress[tag] = {"label": label, "done": done, "total": total}
        return cb

    def cached(self, key, ttl, fn):
        hit = self._cache.get(key)
        if hit and time.time() - hit[0] < ttl:
            return hit[1]
        val = fn()
        self._cache[key] = (time.time(), val)
        return val

    def any_running(self):
        return any(t.phase in ("preparing", "running") for t in self.tasks.values())

    def _stats(self):
        try:
            with open(STATS_FILE, encoding="utf-8") as f:
                return json.load(f)
        except (OSError, ValueError):
            return {}

    def _record_play(self, key, started, seconds):
        with self.lock:
            st = self._stats()
            e = st.setdefault(key, {"last_played": 0, "playtime_s": 0})
            e["last_played"] = started
            e["playtime_s"] += int(seconds)
            with open(STATS_FILE, "w", encoding="utf-8") as f:
                json.dump(st, f)

    def ref(self, key):
        kind, _, iid = (key or "").partition(":")
        if kind == "local":
            inst = local.get_instance(iid)
            if not inst:
                raise ApiError("Сборка не найдена")
            return {"kind": "local", "id": iid, "inst": inst, "gdir": local.game_dir(inst), "mc": inst["mc_version"],
                    "loader": inst.get("loader") or {}, "name": inst["name"]}
        if kind == "srv":
            pack = next((p for p in self.packs if p["id"] == iid), None)
            st = packs.installed_state(iid)
            if not pack and not st:
                raise ApiError("Сборка не найдена на сервере")
            meta = pack or st["manifest"]
            return {"kind": "server", "id": iid, "pack": pack, "gdir": packs.instance_dir(iid),
                    "mc": meta.get("mc_version"), "loader": meta.get("loader") or {}, "name": meta.get("name", iid)}
        raise ApiError("Неизвестная сборка")

    def local_ref(self, key):
        r = self.ref(key)
        if r["kind"] != "local":
            raise ApiError("Сборку с сервера нельзя менять — она обновляется с сервера целиком. "
                           "Сделай копию, если хочешь свои моды.")
        return r

    def _phase(self, key):
        t = self.tasks.get(key)
        return t.phase if t else None

    # ---------- состояние ----------
    def item_local(self, i):
        key = "local:" + i["id"]
        st = self._stats().get(key, {})
        gdir = os.path.join(local.LOCAL, i["id"], "minecraft")
        return {"key": key, "kind": "local", "id": i["id"], "name": i["name"], "mc_version": i.get("mc_version"),
                "loader": i.get("loader") or {}, "loader_label": local.loader_label(i.get("loader")),
                "icon_seed": i.get("icon_seed") or i["id"], "ram_mb": i.get("ram_mb") or 0,
                "java_args": i.get("java_args", ""), "notes": i.get("notes", ""),
                "last_played": st.get("last_played", 0), "playtime_s": st.get("playtime_s", 0),
                "mods": local.count_files(gdir, "mods", (".jar", ".jar.disabled")),
                "phase": self._phase(key)}

    def item_server(self, p):
        key = "srv:" + p["id"]
        st = self._stats().get(key, {})
        installed = packs.installed_state(p["id"])
        status = "new" if not installed else ("update" if installed.get("version") != p.get("version") else "ready")
        return {"key": key, "kind": "server", "id": p["id"], "name": p.get("name", p["id"]),
                "mc_version": p.get("mc_version"), "loader": p.get("loader") or {},
                "loader_label": local.loader_label(p.get("loader")), "icon_seed": p["id"],
                "description": p.get("description", ""), "version": p.get("version", ""),
                "installed_version": installed.get("version") if installed else None, "status": status,
                "last_played": st.get("last_played", 0), "playtime_s": st.get("playtime_s", 0),
                "phase": self._phase(key)}

    def items(self):
        return [self.item_local(i) for i in local.list_instances()] + [self.item_server(p) for p in self.packs]

    def public_settings(self):
        return {k: self.settings.get(k, "") for k in PUBLIC_SETTINGS}

    def m_state(self, p):
        return {"version": APP_VERSION, "os": OS_NAME, "data_dir": ROOT, "settings": self.public_settings(),
                "accounts": [{"type": a["type"], "name": a["name"], "uuid": a["uuid"]} for a in auth.load_accounts()],
                "selected": self.settings.get("last_account", ""), "instances": self.items(),
                "packs_error": self.packs_error}

    def m_ping(self, p):
        self.last_ping = time.time()
        self.pings += 1
        self.bye_at = None
        return {"ok": True}

    def m_bye(self, p):
        self.bye_at = time.time()
        return {"ok": True}

    def m_poll(self, p):
        self.m_ping(p)
        since = int(p.get("notice") or 0)
        tasks = {k: {"phase": t.phase, "error": t.error, "code": t.code, "started": t.started}
                 for k, t in self.tasks.items()}
        return {"tasks": tasks, "progress": dict(self.progress), "notice": self.notice_id,
                "notices": [n for n in self.notices if n["id"] > since]}

    # ---------- аккаунты ----------
    def m_accounts_select(self, p):
        self.settings["last_account"] = p["uuid"]
        config.save(self.settings)
        return {}

    def m_accounts_remove(self, p):
        acc = next((a for a in auth.load_accounts() if a["uuid"] == p["uuid"] and a["type"] == p.get("type")), None)
        if acc:
            auth.remove_account(acc)
        return {}

    def m_accounts_ely_login(self, p):
        try:
            acc = auth.ely_login((p.get("username") or "").strip(), p.get("password") or "", (p.get("totp") or "").strip())
        except auth.AuthError as e:
            raise ApiError(str(e))
        auth.upsert_account(acc)
        self.settings["last_account"] = acc["uuid"]
        config.save(self.settings)
        return {"name": acc["name"]}

    @staticmethod
    def _flow_public(f):
        return {"state": f["state"], "code": f["code"], "url": f["url"], "error": f["error"]}

    def m_accounts_ms_start(self, p):
        cid = (self.settings.get("ms_client_id") or "").strip()
        if not cid:
            raise ApiError("Не задан Client ID приложения Microsoft. Вставь его в Настройках, раздел «Вход через Microsoft».")
        if self.ms_flow and self.ms_flow["state"] in ("starting", "waiting"):
            self.ms_flow["cancel"].set()
        flow = {"state": "starting", "code": "", "url": "", "error": "", "cancel": threading.Event()}
        self.ms_flow = flow

        def on_code(code, url):
            flow.update(code=code, url=url, state="waiting")

        def on_url(url):
            flow.update(code="", url=url, state="waiting")

        def run():
            try:
                if p.get("mode") == "code":
                    acc = auth.ms_device_flow(cid, on_code, flow["cancel"].is_set)
                else:
                    acc = auth.ms_browser_flow(cid, on_url, flow["cancel"].is_set)
                self.settings["last_account"] = acc["uuid"]
                config.save(self.settings)
                flow["state"] = "done"
            except (auth.AuthError, NetError) as e:
                flow.update(state="error", error=str(e))
            except Exception as e:  # noqa: BLE001
                app_log("ms flow: " + traceback.format_exc())
                flow.update(state="error", error=str(e))
        threading.Thread(target=run, daemon=True).start()
        for _ in range(80):
            if flow["state"] != "starting":
                break
            time.sleep(0.25)
        return self._flow_public(flow)

    def m_accounts_ms_status(self, p):
        return self._flow_public(self.ms_flow) if self.ms_flow else {"state": "none", "code": "", "url": "", "error": ""}

    def m_accounts_ms_cancel(self, p):
        if self.ms_flow:
            self.ms_flow["cancel"].set()
        return {}

    def current_account(self):
        accs = auth.load_accounts()
        sel = self.settings.get("last_account")
        return next((a for a in accs if a["uuid"] == sel), accs[0] if accs else None)

    # ---------- версии ----------
    def m_meta_mc_versions(self, p):
        snap = bool(p.get("snapshots"))
        return {"versions": self.cached(("mc", snap), 600, lambda: local.mc_versions(snap))}

    def m_meta_loader_versions(self, p):
        kind, mc = p["kind"], p["mc"]
        return {"versions": self.cached(("lv", kind, mc), 600, lambda: local.loader_versions(kind, mc))}

    # ---------- сборки ----------
    def _clean_loader(self, loader):
        kind = (loader or {}).get("type", "vanilla")
        if kind not in local.LOADERS:
            raise ApiError("Неизвестный загрузчик")
        ver = (loader or {}).get("version", "").strip() if kind != "vanilla" else ""
        if kind in ("forge", "neoforge") and not ver:
            raise ApiError(f"Для {local.LOADER_TITLES[kind]} нужно выбрать версию загрузчика")
        return {"type": kind, "version": ver}

    @staticmethod
    def _clean_ram(v):
        try:
            ram = int(v or 0)
        except (TypeError, ValueError):
            raise ApiError("Память — число в мегабайтах")
        if ram and not 512 <= ram <= 65536:
            raise ApiError("Память должна быть от 512 до 65536 МБ (или 0 — общая настройка)")
        return ram

    def m_instances_create(self, p):
        name = (p.get("name") or "").strip()
        if not name:
            raise ApiError("Введи название сборки")
        if not p.get("mc_version"):
            raise ApiError("Выбери версию Minecraft")
        inst = local.save_instance({
            "name": name, "mc_version": p["mc_version"], "loader": self._clean_loader(p.get("loader")),
            "ram_mb": self._clean_ram(p.get("ram_mb")), "java_args": (p.get("java_args") or "").strip(),
            "icon_seed": secrets.token_hex(4)})
        return {"instance": self.item_local(inst)}

    def m_instances_update(self, p):
        inst = local.get_instance(p["id"])
        if not inst:
            raise ApiError("Сборка не найдена")
        if self._phase("local:" + p["id"]) in ("preparing", "running"):
            raise ApiError("Сначала закрой игру: сборка сейчас запущена")
        if "name" in p:
            if not (p["name"] or "").strip():
                raise ApiError("Название не может быть пустым")
            inst["name"] = p["name"].strip()
        if "mc_version" in p and p["mc_version"]:
            inst["mc_version"] = p["mc_version"]
        if "loader" in p:
            inst["loader"] = self._clean_loader(p["loader"])
        if "ram_mb" in p:
            inst["ram_mb"] = self._clean_ram(p["ram_mb"])
        for k in ("java_args", "icon_seed", "notes"):
            if k in p:
                inst[k] = (p[k] or "").strip() if k != "icon_seed" else p[k]
        local.save_instance(inst)
        return {"instance": self.item_local(inst)}

    def m_instances_delete(self, p):
        r = self.local_ref("local:" + p["id"])
        if self._phase("local:" + p["id"]) in ("preparing", "running"):
            raise ApiError("Сначала закрой игру")
        local.delete_instance(r["inst"])
        self.tasks.pop("local:" + p["id"], None)
        return {}

    def m_instances_duplicate(self, p):
        r = self.local_ref(p["key"])
        new = local.duplicate_instance(r["inst"], (p.get("name") or "").strip() or r["name"] + " (копия)")
        return {"instance": self.item_local(new)}

    def m_instances_export(self, p):
        r = self.local_ref(p["key"])
        path = local.export_zip(r["inst"], include_worlds=bool(p.get("include_worlds")))
        return {"path": path, "size": os.path.getsize(path)}

    def m_instances_open_folder(self, p):
        r = self.ref(p["key"])
        sub = p.get("sub", "")
        if sub not in SUBFOLDERS:
            raise ApiError("Неизвестная папка")
        open_path(os.path.join(r["gdir"], sub) if sub else r["gdir"])
        return {}

    def m_app_open_url(self, p):
        """Открывает ссылку в обычном браузере пользователя. Только разрешённые адреса."""
        url = p.get("url", "")
        flow_url = self.ms_flow.get("url") if self.ms_flow else None
        if not (url == flow_url or url.startswith(("https://modrinth.com/", "https://ely.by/", "https://www.curseforge.com/", "https://console.curseforge.com/"))):
            raise ApiError("Этот адрес открывать нельзя")
        webbrowser.open(url)
        return {}

    def m_app_open(self, p):
        what = p.get("what")
        target = {"data": ROOT, "exports": local.EXPORTS, "site": self.settings.get("site_dir")}.get(what)
        if not target:
            raise ApiError("Папка не задана")
        open_path(target)
        return {}

    # ---------- содержимое сборки ----------
    def m_mods_list(self, p):
        r = self.ref(p["key"])
        return {"mods": local.list_mods(r["gdir"]), "readonly": r["kind"] != "local",
                "updates": [u["file"] for u in self.updates.get(p["key"], [])]}

    def m_modicon(self, p):
        r = self.ref(p["key"])
        data = local.mod_icon(r["gdir"], p["file"])
        if not data:
            return None
        return (data, "image/png")

    def m_mods_toggle(self, p):
        r = self.local_ref(p["key"])
        return {"file": local.toggle_mod(r["gdir"], p["file"])}

    def m_mods_delete(self, p):
        r = self.local_ref(p["key"])
        local.delete_mods(r["gdir"], p.get("files") or [])
        self.updates.pop(p["key"], None)
        return {}

    def m_mods_check_updates(self, p):
        r = self.local_ref(p["key"])
        if (r["loader"] or {}).get("type", "vanilla") == "vanilla":
            raise ApiError("У сборки не выбран загрузчик, моды не используются")
        ups = modrinth.check_updates(r["gdir"], r["mc"], r["loader"]["type"])
        self.updates[p["key"]] = ups
        return {"updates": [{"file": u["file"], "version": u["version_number"]} for u in ups]}

    def m_mods_update(self, p):
        r = self.local_ref(p["key"])
        wanted = set(p.get("files") or [])
        done = 0
        keep = []
        for u in self.updates.get(p["key"], []):
            if u["file"] in wanted:
                modrinth.apply_update(r["gdir"], u)
                done += 1
            else:
                keep.append(u)
        self.updates[p["key"]] = keep
        return {"updated": done}

    def m_content_installed(self, p):
        r = self.ref(p["key"])
        return {"project_ids": sorted({t.get("project_id") for t in local.load_track(r["gdir"]).values()
                                       if t.get("project_id")})}

    def m_worlds_list(self, p):
        return {"worlds": local.list_worlds(self.ref(p["key"])["gdir"])}

    def m_worlds_backup(self, p):
        path = local.backup_world(self.ref(p["key"])["gdir"], p["name"])
        return {"path": path, "size": os.path.getsize(path)}

    def m_worlds_delete(self, p):
        local.delete_world(self.local_ref(p["key"])["gdir"], p["name"])
        return {}

    def m_latest_log(self, p):
        path = os.path.join(self.ref(p["key"])["gdir"], "logs", "latest.log")
        if not os.path.isfile(path):
            return {"lines": []}
        with open(path, "rb") as f:
            f.seek(0, 2)
            f.seek(max(0, f.tell() - 200_000))
            text = f.read().decode("utf-8", "replace")
        return {"lines": text.splitlines()[-600:]}

    # ---------- скриншоты ----------
    def _shot_refs(self):
        refs = [("local:" + i["id"], i["name"], local.game_dir(i)) for i in local.list_instances()]
        for p in self.packs:
            if packs.installed_state(p["id"]):
                refs.append(("srv:" + p["id"], p.get("name") or p["id"], packs.instance_dir(p["id"])))
        return refs

    def m_shots_list(self, p):
        return {"shots": shots.list_all(self._shot_refs())}

    def m_shot(self, p):
        return shots.read(self.ref(p["key"])["gdir"], p["file"])

    def m_shots_delete(self, p):
        shots.delete(self.ref(p["key"])["gdir"], p["file"])
        return {}

    def m_shots_save_copy(self, p):
        """Копия в папку «Изображения/FriendLauncher» пользователя."""
        pics = os.path.join(os.path.expanduser("~"), "Pictures")
        dest = shots.copy_to(self.ref(p["key"])["gdir"], p["file"], os.path.join(pics, "FriendLauncher"))
        return {"path": dest}

    # ---------- скины ----------
    def _ms_account(self, uuid):
        acc = next((a for a in auth.load_accounts() if a["uuid"] == uuid and a["type"] == "ms"), None)
        if not acc:
            raise ApiError("Скины можно менять только у аккаунта Microsoft. Войди через Microsoft в меню аккаунта.")
        try:
            return auth.ensure_fresh(acc, self.settings.get("ms_client_id", ""))
        except auth.AuthError as e:
            raise ApiError(str(e))

    def m_skins_list(self, p):
        return {"skins": skins.list_skins()}

    def m_skinimg(self, p):
        data = skins.read(p.get("id", ""))
        return (data, "image/png") if data else None

    def m_skinurl(self, p):
        """Текущий скин аккаунта (через лаунчер, чтобы страница не ходила на чужие сайты)."""
        acc = next((a for a in auth.load_accounts() if a["uuid"] == p.get("uuid")), None)
        if not acc:
            return None
        try:
            if acc["type"] == "ms":
                prof = skins.profile(self._ms_account(acc["uuid"]))
                return (skins.fetch_texture(prof["skin_url"]), "image/png") if prof["skin_url"] else None
        except (skins.SkinError, ApiError, NetError):
            return None
        return None

    def m_skins_profile(self, p):
        try:
            return skins.profile(self._ms_account(p["uuid"]))
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_apply(self, p):
        acc = self._ms_account(p["uuid"])
        it = skins.get(p["id"])
        variant = p.get("variant") or it["variant"]
        if variant != it["variant"]:
            skins.update(it["id"], variant=variant)
        try:
            skins.apply(acc, skins.read(it["id"]) or b"", variant)
            return skins.profile(acc)
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_reset(self, p):
        acc = self._ms_account(p["uuid"])
        try:
            skins.reset(acc)
            return skins.profile(acc)
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_cape(self, p):
        acc = self._ms_account(p["uuid"])
        try:
            skins.set_cape(acc, p.get("cape_id") or "")
            return skins.profile(acc)
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_import_current(self, p):
        try:
            prof = skins.profile(self._ms_account(p["uuid"]))
            if not prof["skin_url"]:
                raise ApiError("У аккаунта стандартный скин, сохранять нечего")
            return {"skin": skins.add("Мой скин " + (prof["name"] or ""), skins.fetch_texture(prof["skin_url"]), prof["variant"])}
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_lookup(self, p):
        try:
            data, variant, name = skins.lookup(p.get("nick", ""))
            return {"skin": skins.add("Скин " + name, data, variant)}
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_update(self, p):
        try:
            return {"skin": skins.update(p["id"], p.get("name"), p.get("variant"))}
        except skins.SkinError as e:
            raise ApiError(str(e))

    def m_skins_delete(self, p):
        try:
            skins.remove(p["id"])
        except skins.SkinError as e:
            raise ApiError(str(e))
        return {}

    # ---------- Modrinth ----------
    def m_modrinth_search(self, p):
        return modrinth.search(p.get("query", ""), p.get("type", "mod"), p.get("mc") or None, p.get("loader") or None,
                               p.get("sort", "relevance"), int(p.get("offset") or 0), 20)

    def m_modrinth_install(self, p):
        meta = {"title": p.get("title"), "icon_url": p.get("icon_url")}
        ptype = p.get("type", "mod")
        tag = p.get("key") or "global"
        cb = self.progress_cb(tag)
        try:
            if ptype == "modpack":
                url, fname, sha1, _ = modrinth.modpack_file(p["project_id"])
                tmp = os.path.join(TMP_DIR, secrets.token_hex(4) + "-" + os.path.basename(fname))
                try:
                    download(url, tmp, sha1=sha1)
                    inst = local.import_archive(tmp, cb, lambda s: None)
                finally:
                    if os.path.isfile(tmp):
                        os.remove(tmp)
                return {"instance": self.item_local(inst)}
            r = self.local_ref(p["key"])
            if ptype == "mod" and (r["loader"] or {}).get("type", "vanilla") == "vanilla":
                raise ApiError("У этой сборки не выбран загрузчик — моды не заработают. Выбери Fabric, Quilt, Forge или NeoForge в настройках сборки.")
            files = modrinth.install(r["gdir"], r["mc"], (r["loader"] or {}).get("type"), p["project_id"], ptype, meta)
            return {"installed": files}
        finally:
            self.progress.pop(tag, None)

    # ---------- CurseForge ----------
    def m_curseforge_search(self, p):
        return curseforge.search(p.get("query", ""), p.get("type", "mod"), p.get("mc") or None, p.get("loader") or None,
                                 p.get("sort", "relevance"), int(p.get("offset") or 0), 20)

    def m_curseforge_install(self, p):
        meta = {"title": p.get("title"), "icon_url": p.get("icon_url"), "site_url": p.get("site_url", "")}
        ptype = p.get("type", "mod")
        tag = p.get("key") or "global"
        cb = self.progress_cb(tag)
        try:
            if ptype == "modpack":
                url, fname, sha1 = curseforge.modpack_file(p["project_id"], meta["title"] or "", meta["site_url"])
                tmp = os.path.join(TMP_DIR, secrets.token_hex(4) + "-" + os.path.basename(fname))
                try:
                    download(url, tmp, sha1=sha1)
                    inst = local.import_archive(tmp, cb, lambda s: None)
                finally:
                    if os.path.isfile(tmp):
                        os.remove(tmp)
                skipped = inst.pop("skipped", [])
                return {"instance": self.item_local(inst), "skipped": skipped}
            r = self.local_ref(p["key"])
            if ptype == "mod" and (r["loader"] or {}).get("type", "vanilla") == "vanilla":
                raise ApiError("У этой сборки не выбран загрузчик — моды не заработают. Выбери Fabric, Quilt, Forge или NeoForge в настройках сборки.")
            files = curseforge.install(r["gdir"], r["mc"], (r["loader"] or {}).get("type"), p["project_id"], ptype, meta)
            return {"installed": files}
        finally:
            self.progress.pop(tag, None)

    # ---------- сборки с сервера ----------
    def reload_packs(self):
        if not self.settings.get("server_url"):
            self.packs, self.index_url, self.packs_error = [], None, ""
            return
        try:
            self.index_url, self.packs = packs.fetch_index(self.settings["server_url"])
            self.packs_error = ""
        except (NetError, ValueError) as e:
            self.packs_error = str(e)

    def m_packs_reload(self, p):
        self.reload_packs()
        return {"instances": self.items(), "error": self.packs_error}

    # ---------- публикация ----------
    def m_publish_defaults(self, p):
        r = self.local_ref(p["key"])
        prev = r["inst"].get("publish") or {}
        pid = prev.get("id") or publisher.slugify(r["name"]) or "pack-" + r["id"][-6:]
        return {"pack_id": pid, "version": publisher.bump_version(prev.get("version", "")), "name": r["name"],
                "description": r["inst"].get("notes", ""), "site_dir": self.settings.get("site_dir", ""),
                "first": not prev}

    def m_publish(self, p):
        r = self.local_ref(p["key"])
        site = (self.settings.get("site_dir") or "").strip()
        if not site:
            raise ApiError("Не указана папка сайта. Задай её в Настройках, раздел «Публикация для друзей».")
        res = publisher.publish_pack(r["gdir"], site, {
            "id": (p.get("pack_id") or "").strip(), "name": (p.get("name") or r["name"]).strip(),
            "description": p.get("description", ""), "version": (p.get("version") or "").strip(),
            "mc_version": r["mc"], "loader": r["loader"], "java_args": r["inst"].get("java_args") or None,
            "ram_mb": r["inst"].get("ram_mb") or None})
        inst = r["inst"]
        inst["publish"] = {"id": p["pack_id"].strip(), "version": p["version"].strip()}
        local.save_instance(inst)
        return res

    # ---------- настройки ----------
    def m_settings_set(self, p):
        new = dict(self.settings)
        for k in PUBLIC_SETTINGS:
            if k in p:
                new[k] = (str(p[k]) if k != "ram_mb" else p[k])
                if isinstance(new[k], str):
                    new[k] = new[k].strip()
        ram = self._clean_ram(new.get("ram_mb"))
        if ram == 0:
            ram = 4096
        new["ram_mb"] = ram
        changed = new.get("server_url") != self.settings.get("server_url")
        self.settings.update(new)
        curseforge.KEY = self.settings.get("cf_api_key", "")
        config.save(self.settings)
        if changed:
            threading.Thread(target=self.reload_packs, daemon=True).start()
        return {"settings": self.public_settings()}

    # ---------- запуск игры ----------
    def m_play(self, p):
        key = p["key"]
        with self.lock:
            t = self.tasks.get(key)
            if t and t.phase in ("preparing", "running"):
                raise ApiError("Эта сборка уже запущена")
            acc = self.current_account()
            if not acc:
                raise ApiError("Сначала добавь аккаунт: Ely.by или Microsoft (значок внизу слева)")
            ref = self.ref(key)
            t = self.tasks[key] = Task(key)
        threading.Thread(target=self._run_game, args=(t, ref, acc), daemon=True).start()
        return {}

    def _run_game(self, t, ref, acc):
        cb = self.progress_cb(t.key)
        try:
            with self.prep_lock:
                if ref["kind"] == "local":
                    proc = launch.launch_local(ref["inst"], ref["gdir"], acc, self.settings, cb, t.add)
                elif ref["pack"] is not None:
                    proc = launch.launch(self.index_url, ref["pack"], acc, self.settings, cb, t.add)
                else:  # сервер недоступен — запускаем то, что уже установлено
                    t.add("Сервер сборок недоступен, запускаю установленную версию.")
                    proc = launch.run_game(packs.cached_manifest(ref["id"]), ref["gdir"], acc, self.settings, cb, t.add)
            self.progress.pop(t.key, None)
            t.proc, t.phase = proc, "running"
            started = time.time()
            for line in proc.stdout:
                t.add(line.rstrip("\r\n"))
            t.code = proc.wait()
            t.phase = "exited" if t.code in (0, 143, -15) else "crashed"
            self._record_play(t.key, started, time.time() - started)
            if t.phase == "crashed":
                self.notice("error", f"«{ref['name']}» завершилась с ошибкой (код {t.code}). Подробности в журнале сборки.")
        except Exception as e:  # noqa: BLE001
            self.progress.pop(t.key, None)
            t.phase, t.error = "error", str(e)
            t.add("ОШИБКА: " + str(e))
            app_log(f"play {t.key}: {traceback.format_exc()}")
            self.notice("error", str(e))

    def m_stop(self, p):
        t = self.tasks.get(p["key"])
        if t and t.proc and t.phase == "running":
            t.proc.terminate()
        return {}

    def m_log(self, p):
        t = self.tasks.get(p["key"])
        if not t:
            return {"lines": [], "next": 0, "phase": None}
        lines, nxt = t.read(int(p.get("since") or 0))
        return {"lines": lines, "next": nxt, "phase": t.phase}

    # ---------- загрузка файлов из браузера ----------
    def upload(self, kind, q, tmp_path):
        name = os.path.basename(q.get("name", "file"))
        if kind == "import":
            if not name.lower().endswith((".mrpack", ".zip")):
                raise ApiError("Нужен файл .mrpack или .zip")
            cb = self.progress_cb("global")
            try:
                inst = local.import_archive(tmp_path, cb, lambda s: None)
            except zipfile.BadZipFile as e:
                raise ApiError("Файл повреждён или не является архивом: " + str(e))
            finally:
                self.progress.pop("global", None)
            return {"instance": self.item_local(inst)}
        if kind == "skin":
            if not name.lower().endswith(".png"):
                raise ApiError("Нужен файл скина в формате .png")
            with open(tmp_path, "rb") as f:
                data = f.read()
            try:
                item = skins.add(os.path.splitext(name)[0], data, q.get("variant", "classic"))
            except skins.SkinError as e:
                raise ApiError(str(e))
            return {"skin": item}
        if kind == "content":
            r = self.local_ref(q["key"])
            folder = q.get("folder", "mods")
            ok = (".jar",) if folder == "mods" else (".zip",)
            if not name.lower().endswith(ok):
                raise ApiError("Для этой папки нужен файл " + "/".join(ok))
            local.add_content_file(r["gdir"], folder, name, tmp_path)
            return {"name": name}
        raise ApiError("Неизвестный тип загрузки")


GET_OK = {"state", "poll", "log", "modicon", "ping", "latest_log", "shot", "skinimg", "skinurl"}
IMAGE_API = ("modicon", "shot", "skinimg", "skinurl")


class Handler(BaseHTTPRequestHandler):
    server_version = "FriendLauncher"

    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="application/json; charset=utf-8", extra=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def _json(self, code, obj):
        self._send(code, json.dumps(obj, ensure_ascii=False).encode("utf-8"))

    def _handle(self, method):
        srv = self.server
        if self.headers.get("Host", "") not in srv.allowed_hosts:
            return self._json(403, {"error": "forbidden"})
        parsed = urllib.parse.urlparse(self.path)
        parts = parsed.path.split("/", 2)
        if len(parts) < 2 or not secrets.compare_digest(parts[1], srv.token):
            return self._json(404, {"error": "not found"})
        if len(parts) == 2:  # /<токен> без слэша
            return self._send(301, b"", extra={"Location": f"/{srv.token}/"})
        rest = parts[2]
        q = dict(urllib.parse.parse_qsl(parsed.query))

        if method == "GET" and not rest.startswith(("api/", "upload/")):
            return self._static(rest or "index.html")
        try:
            if rest.startswith("upload/") and method == "POST":
                return self._upload(rest[len("upload/"):], q)
            if rest.startswith("api/"):
                name = rest[4:]
                if method == "GET" and name not in GET_OK:
                    return self._json(405, {"error": "method not allowed"})
                params = q
                if method == "POST":
                    n = int(self.headers.get("Content-Length") or 0)
                    if n > 1 << 20:
                        return self._json(413, {"error": "too large"})
                    raw = self.rfile.read(n) if n else b""
                    params = json.loads(raw.decode("utf-8")) if raw.strip() else {}
                fn = getattr(srv.api, "m_" + name.replace(".", "_"), None)
                if not fn or name.startswith("_"):
                    return self._json(404, {"error": "unknown method"})
                res = fn(params)
                if name in IMAGE_API:
                    return self._send(200, res[0], res[1]) if res else self._send(404, b"", "text/plain")
                return self._json(200, res)
            return self._json(404, {"error": "not found"})
        except (ApiError, NetError, ValueError, auth.AuthError, OSError, KeyError) as e:
            if isinstance(e, KeyError):
                app_log("bad request: " + traceback.format_exc())
                return self._json(400, {"error": f"Не хватает параметра запроса или неожиданный ответ сервера: {e}"})
            return self._json(400, {"error": str(e)})
        except Exception as e:  # noqa: BLE001
            app_log(traceback.format_exc())
            return self._json(500, {"error": f"Внутренняя ошибка: {e}"})

    def _static(self, rel):
        if rel not in ("index.html", "app.js", "app.css"):
            return self._json(404, {"error": "not found"})
        path = os.path.join(WEB_DIR, rel)
        try:
            with open(path, "rb") as f:
                body = f.read()
        except OSError:
            return self._json(404, {"error": "missing " + rel})
        ctype = (mimetypes.guess_type(rel)[0] or "text/plain") + "; charset=utf-8"
        csp = ("default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; "
               "script-src 'self'; connect-src 'self'; frame-ancestors 'none'")
        self._send(200, body, ctype, {"Content-Security-Policy": csp})

    def _upload(self, kind, q):
        limit = MAX_UPLOAD.get(kind)
        if not limit:
            return self._json(404, {"error": "unknown upload"})
        n = int(self.headers.get("Content-Length") or 0)
        if n <= 0 or n > limit:
            return self._json(413, {"error": "Файл пустой или слишком большой"})
        tmp = os.path.join(TMP_DIR, secrets.token_hex(6) + ".upload")
        try:
            with open(tmp, "wb") as f:
                left = n
                while left:
                    chunk = self.rfile.read(min(1 << 20, left))
                    if not chunk:
                        raise OSError("Загрузка прервана")
                    f.write(chunk)
                    left -= len(chunk)
            res = self.server.api.upload(kind, q, tmp)
            self._json(200, res)
        finally:
            if os.path.isfile(tmp):
                os.remove(tmp)

    def do_GET(self):
        self._handle("GET")

    def do_HEAD(self):
        self._handle("GET")

    def do_POST(self):
        self._handle("POST")


class Server(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True


def make_server(api, port=0):
    srv = Server(("127.0.0.1", port), Handler)
    srv.api = api
    srv.token = secrets.token_urlsafe(24)
    p = srv.server_address[1]
    srv.allowed_hosts = {f"127.0.0.1:{p}", f"localhost:{p}"}
    return srv, f"http://127.0.0.1:{p}/{srv.token}/"
