"""Скины: личная библиотека + смена скина и плаща у аккаунта Microsoft через Minecraft Services API."""
import base64
import json
import os
import secrets
import struct
import time
import urllib.error
import urllib.parse
import urllib.request

from .util import ROOT, UA, NetError, http_json

SKIN_DIR = os.path.join(ROOT, "skins")
INDEX = os.path.join(SKIN_DIR, "index.json")
API = "https://api.minecraftservices.com"
TEXTURE_HOST = "textures.minecraft.net"
PNG_SIG = b"\x89PNG\r\n\x1a\n"
MAX_SKIN = 200_000


class SkinError(ValueError):
    pass


# ---------- библиотека ----------

def _load():
    try:
        with open(INDEX, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return []


def _save(items):
    os.makedirs(SKIN_DIR, exist_ok=True)
    with open(INDEX, "w", encoding="utf-8") as f:
        json.dump(items, f, ensure_ascii=False, indent=2)


def list_skins():
    return sorted(_load(), key=lambda s: s.get("added", 0), reverse=True)


def png_size(data):
    if len(data) > MAX_SKIN or not data.startswith(PNG_SIG) or data[12:16] != b"IHDR":
        raise SkinError("Это не PNG-файл скина")
    w, h = struct.unpack(">II", data[16:24])
    if (w, h) not in ((64, 64), (64, 32)):
        raise SkinError(f"Размер скина должен быть 64×64 (или старый 64×32), а у файла {w}×{h}")
    return w, h


def add(name, data, variant="classic"):
    png_size(data)
    if variant not in ("classic", "slim"):
        variant = "classic"
    sid = secrets.token_hex(5)
    os.makedirs(SKIN_DIR, exist_ok=True)
    with open(os.path.join(SKIN_DIR, sid + ".png"), "wb") as f:
        f.write(data)
    items = _load()
    item = {"id": sid, "name": (name or "Скин").strip()[:40] or "Скин", "variant": variant, "added": int(time.time())}
    items.append(item)
    _save(items)
    return item


def update(sid, name=None, variant=None):
    items = _load()
    for it in items:
        if it["id"] == sid:
            if name is not None and name.strip():
                it["name"] = name.strip()[:40]
            if variant in ("classic", "slim"):
                it["variant"] = variant
            _save(items)
            return it
    raise SkinError("Скин не найден")


def remove(sid):
    items = _load()
    _save([i for i in items if i["id"] != sid])
    p = _file(sid)
    if os.path.isfile(p):
        os.remove(p)


def _file(sid):
    if not sid.isalnum():
        raise SkinError("Неверный id скина")
    return os.path.join(SKIN_DIR, sid + ".png")


def read(sid):
    try:
        with open(_file(sid), "rb") as f:
            return f.read()
    except OSError:
        return None


def get(sid):
    it = next((i for i in _load() if i["id"] == sid), None)
    if not it:
        raise SkinError("Скин не найден")
    return it


# ---------- Mojang ----------

def _auth(acc):
    return {"Authorization": "Bearer " + acc["access_token"]}


def _check(st, js, what):
    if st in (401, 403):
        raise SkinError("Mojang не принял вход. Выйди из аккаунта Microsoft и войди снова.")
    if st == 429:
        raise SkinError("Слишком много запросов к Mojang. Подожди минуту и повтори.")
    if st not in (200, 204):
        msg = js.get("errorMessage") or js.get("error") or js.get("_raw") or ""
        raise SkinError(f"{what}: HTTP {st} {msg}".strip())


def profile(acc):
    st, js = http_json(API + "/minecraft/profile", headers=_auth(acc))
    if st == 404:
        raise SkinError("У этого аккаунта нет профиля Minecraft Java Edition.")
    _check(st, js, "Профиль")
    skin = next((s for s in js.get("skins", []) if s.get("state") == "ACTIVE"), None)
    return {
        "name": js.get("name"), "id": js.get("id"),
        "skin_url": skin.get("url") if skin else "", "variant": (skin or {}).get("variant", "CLASSIC").lower(),
        "capes": [{"id": c["id"], "alias": c.get("alias") or "Плащ", "url": c.get("url", ""), "active": c.get("state") == "ACTIVE"}
                  for c in js.get("capes", [])],
    }


def _multipart(fields, file_field, filename, data):
    b = "----fl" + secrets.token_hex(12)
    out = b""
    for k, v in fields.items():
        out += f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
    out += (f'--{b}\r\nContent-Disposition: form-data; name="{file_field}"; filename="{filename}"\r\n'
            f'Content-Type: image/png\r\n\r\n').encode() + data + f"\r\n--{b}--\r\n".encode()
    return out, "multipart/form-data; boundary=" + b


def apply(acc, data, variant):
    png_size(data)
    body, ctype = _multipart({"variant": "slim" if variant == "slim" else "classic"}, "file", "skin.png", data)
    req = urllib.request.Request(API + "/minecraft/profile/skins", data=body, method="POST",
                                 headers={**_auth(acc), "Content-Type": ctype, "User-Agent": UA, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=40) as r:
            st, raw = r.status, r.read()
    except urllib.error.HTTPError as e:
        st, raw = e.code, e.read()
    except OSError as e:
        raise NetError(f"Нет соединения с Mojang: {getattr(e, 'reason', e)}")
    try:
        js = json.loads(raw.decode("utf-8")) if raw.strip() else {}
    except ValueError:
        js = {}
    _check(st, js, "Загрузка скина")


def reset(acc):
    st, js = http_json(API + "/minecraft/profile/skins/active", headers=_auth(acc), method="DELETE")
    _check(st, js, "Сброс скина")


def set_cape(acc, cape_id):
    if cape_id:
        st, js = http_json(API + "/minecraft/profile/capes/active", body={"capeId": cape_id}, headers=_auth(acc), method="PUT")
    else:
        st, js = http_json(API + "/minecraft/profile/capes/active", headers=_auth(acc), method="DELETE")
    _check(st, js, "Смена плаща")


def fetch_texture(url):
    """Скачивает PNG только с textures.minecraft.net."""
    u = urllib.parse.urlparse(url)
    if u.scheme not in ("http", "https") or u.hostname != TEXTURE_HOST:
        raise SkinError("Неожиданный адрес скина")
    req = urllib.request.Request("https://" + TEXTURE_HOST + u.path, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.read(MAX_SKIN + 1)
    except OSError as e:
        raise NetError(f"Не удалось скачать скин: {getattr(e, 'reason', e)}")


def lookup(nick):
    """Скин любого лицензионного игрока по нику: (png, variant)."""
    nick = (nick or "").strip()
    if not nick or not all(c.isalnum() or c == "_" for c in nick) or len(nick) > 16:
        raise SkinError("Ник: 3–16 символов, буквы, цифры и _")
    st, js = http_json("https://api.mojang.com/users/profiles/minecraft/" + urllib.parse.quote(nick))
    if st in (204, 404) or not js.get("id"):
        raise SkinError(f"Игрок «{nick}» не найден")
    _check(st, js, "Поиск игрока")
    st, prof = http_json("https://sessionserver.mojang.com/session/minecraft/profile/" + js["id"])
    _check(st, prof, "Профиль игрока")
    for p in prof.get("properties", []):
        if p.get("name") == "textures":
            tex = json.loads(base64.b64decode(p["value"]).decode("utf-8")).get("textures", {})
            sk = tex.get("SKIN")
            if not sk:
                raise SkinError(f"У «{nick}» стандартный скин")
            variant = "slim" if (sk.get("metadata") or {}).get("model") == "slim" else "classic"
            return fetch_texture(sk["url"]), variant, js.get("name", nick)
    raise SkinError(f"У «{nick}» нет скина")
