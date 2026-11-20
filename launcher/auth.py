"""Авторизация: Ely.by (Yggdrasil) и Microsoft (device code → Xbox → XSTS → Minecraft)."""
import base64
import hashlib
import html
import http.server
import json
import os
import secrets
import threading
import time
import urllib.parse
import uuid

from .util import ROOT, NetError, http_json

_ACCOUNTS = os.path.join(ROOT, "accounts.json")


class AuthError(Exception):
    pass


# ---------- хранилище аккаунтов ----------

def load_accounts():
    try:
        with open(_ACCOUNTS, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return []


def save_accounts(accounts):
    with open(_ACCOUNTS, "w", encoding="utf-8") as f:
        json.dump(accounts, f, ensure_ascii=False, indent=2)
    try:
        os.chmod(_ACCOUNTS, 0o600)
    except OSError:
        pass


def upsert_account(acc):
    accounts = [a for a in load_accounts() if not (a["type"] == acc["type"] and a["uuid"] == acc["uuid"])]
    accounts.append(acc)
    save_accounts(accounts)


def remove_account(acc):
    save_accounts([a for a in load_accounts() if not (a["type"] == acc["type"] and a["uuid"] == acc["uuid"])])


def label(acc):
    return f"{acc['name']}  ({'Ely.by' if acc['type'] == 'ely' else 'Microsoft'})"


# ---------- Ely.by ----------

ELY_AUTH = "https://authserver.ely.by"
ELY_AGENT_ARG = "ely.by"  # значение для -javaagent:authlib-injector.jar=ely.by


def _ely_error(js):
    return js.get("errorMessage") or js.get("error") or js.get("_raw") or "неизвестная ошибка"


def ely_login(username, password, totp=""):
    """totp — код двухфакторной аутентификации, если она включена на аккаунте."""
    pw = f"{password}:{totp}" if totp else password
    client_token = uuid.uuid4().hex
    st, js = http_json(ELY_AUTH + "/auth/authenticate", body={
        "username": username, "password": pw, "clientToken": client_token, "requestUser": True})
    if st != 200:
        raise AuthError(f"Ely.by: {_ely_error(js)}")
    prof = js.get("selectedProfile") or {}
    if not prof:
        raise AuthError("Ely.by: у аккаунта нет профиля игрока")
    return {"type": "ely", "name": prof["name"], "uuid": prof["id"],
            "access_token": js["accessToken"], "client_token": js.get("clientToken", client_token)}


def ely_ensure(acc):
    st, _ = http_json(ELY_AUTH + "/auth/validate", body={"accessToken": acc["access_token"]})
    if st == 200:
        return acc
    st, js = http_json(ELY_AUTH + "/auth/refresh", body={
        "accessToken": acc["access_token"], "clientToken": acc["client_token"]})
    if st != 200:
        raise AuthError(f"Сессия Ely.by истекла, войди заново ({_ely_error(js)})")
    acc["access_token"] = js["accessToken"]
    upsert_account(acc)
    return acc


# ---------- Microsoft ----------

_MS = "https://login.microsoftonline.com/consumers/oauth2/v2.0"
_SCOPE = "XboxLive.signin offline_access"

_XSTS_ERRORS = {
    2148916233: "У этой учётной записи Microsoft нет профиля Xbox. Зайди на xbox.com и создай его.",
    2148916235: "Xbox Live недоступен в твоей стране.",
    2148916236: "Нужна проверка возраста/взрослого аккаунта Xbox.",
    2148916237: "Нужна проверка возраста/взрослого аккаунта Xbox.",
    2148916238: "Детский аккаунт: добавь его в семейную группу Microsoft и разреши онлайн-игру.",
}


def _need_client_id(client_id):
    if not client_id:
        raise AuthError("Не задан Client ID приложения Microsoft. Открой «Настройки» и вставь его "
                        "(как его получить — в README, раздел «Вход через Microsoft»).")


def ms_device_flow(client_id, on_code, cancelled=lambda: False):
    """on_code(user_code, verification_uri) — показать код пользователю."""
    _need_client_id(client_id)
    st, js = http_json(_MS + "/devicecode", form={"client_id": client_id, "scope": _SCOPE})
    if st != 200:
        raise AuthError(f"Microsoft: {js.get('error_description', js)}")
    on_code(js["user_code"], js["verification_uri"])
    interval = int(js.get("interval", 5))
    deadline = time.time() + int(js.get("expires_in", 900))
    while time.time() < deadline:
        for _ in range(interval * 2):
            if cancelled():
                raise AuthError("Вход отменён")
            time.sleep(0.5)
        st, tok = http_json(_MS + "/token", form={
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            "client_id": client_id, "device_code": js["device_code"]})
        if st == 200:
            return _ms_complete(tok)
        err = tok.get("error")
        if err == "authorization_pending":
            continue
        if err == "slow_down":
            interval += 5
            continue
        raise AuthError(f"Microsoft: {tok.get('error_description', err)}")
    raise AuthError("Время ввода кода истекло")


def ms_browser_flow(client_id, on_url, cancelled=lambda: False, timeout=300):
    """Вход как на обычных сайтах: открывается страница Microsoft, пароль вводится там, а не в лаунчере.
    OAuth 2.0 с PKCE и возвратом на локальный адрес http://localhost:порт (виден только этому компьютеру).
    on_url(url) — показать/открыть адрес входа."""
    _need_client_id(client_id)
    verifier = secrets.token_urlsafe(64)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
    state = secrets.token_urlsafe(24)
    got = {}

    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_GET(self):
            u = urllib.parse.urlparse(self.path)
            q = dict(urllib.parse.parse_qsl(u.query))
            if u.path != "/" or not ("code" in q or "error" in q):
                self.send_response(404)
                self.end_headers()
                return
            if not secrets.compare_digest(q.get("state", ""), state):
                msg, ok = "Неверный ответ входа. Закрой вкладку и начни вход в лаунчере заново.", False
            elif "error" in q:
                msg, ok = "Вход не выполнен: " + (q.get("error_description") or q["error"]), False
                got["error"] = msg
            else:
                got["code"] = q["code"]
                msg, ok = "Готово! Можно закрыть эту вкладку и вернуться в лаунчер.", True
            body = ("<!doctype html><meta charset=utf-8><title>FriendLauncher</title><body style=\"font:18px system-ui;background:#0f1117;"
                    "color:#e9eaf2;display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center;max-width:420px;padding:20px\">"
                    f"<h1 style=\"color:{'#7be0a8' if ok else '#ff8a8a'}\">{'Вход выполнен' if ok else 'Ошибка входа'}</h1><p>{html.escape(msg)}</p></div>").encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            if got.get("code") or got.get("error"):  # чужие запросы с неверным state вход не завершают
                got["done"] = True

    srv = http.server.HTTPServer(("127.0.0.1", 0), H)
    srv.timeout = 0.5
    redirect = f"http://localhost:{srv.server_address[1]}"
    try:
        on_url(_MS + "/authorize?" + urllib.parse.urlencode({
            "client_id": client_id, "response_type": "code", "redirect_uri": redirect, "scope": _SCOPE,
            "code_challenge": challenge, "code_challenge_method": "S256", "state": state,
            "prompt": "select_account", "response_mode": "query"}))
        deadline = time.time() + timeout
        while not got.get("done") and time.time() < deadline:
            if cancelled():
                raise AuthError("Вход отменён")
            srv.handle_request()
    finally:
        srv.server_close()
    if got.get("error"):
        raise AuthError(got["error"])
    if "code" not in got:
        raise AuthError("Время входа истекло. Нажми «Войти через Microsoft» ещё раз.")
    st, tok = http_json(_MS + "/token", form={
        "grant_type": "authorization_code", "client_id": client_id, "code": got["code"],
        "redirect_uri": redirect, "code_verifier": verifier, "scope": _SCOPE})
    if st != 200:
        raise AuthError(f"Microsoft: {tok.get('error_description', tok.get('error', st))}")
    return _ms_complete(tok)


def _ms_complete(tok):
    mc_token, expires, profile = _ms_chain(tok["access_token"])
    acc = {"type": "ms", "name": profile["name"], "uuid": profile["id"],
           "access_token": mc_token, "refresh_token": tok.get("refresh_token", ""),
           "expires_at": time.time() + expires}
    upsert_account(acc)
    return acc


def _ms_chain(ms_access_token):
    st, xbl = http_json("https://user.auth.xboxlive.com/user/authenticate", body={
        "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com",
                       "RpsTicket": "d=" + ms_access_token},
        "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"})
    if st != 200:
        raise AuthError(f"Xbox Live: HTTP {st}")
    uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]

    st, xsts = http_json("https://xsts.auth.xboxlive.com/xsts/authorize", body={
        "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl["Token"]]},
        "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"})
    if st != 200:
        code = xsts.get("XErr")
        raise AuthError(_XSTS_ERRORS.get(code, f"XSTS: HTTP {st}, код {code}"))

    st, mc = http_json("https://api.minecraftservices.com/authentication/login_with_xbox", body={
        "identityToken": f"XBL3.0 x={uhs};{xsts['Token']}"})
    if st == 403:
        raise AuthError("Minecraft Services отклонил приложение (Invalid app registration). Client ID "
                        "ещё не одобрен Mojang — см. README, раздел «Вход через Microsoft».")
    if st != 200:
        raise AuthError(f"Minecraft Services: HTTP {st} {mc}")

    st, prof = http_json("https://api.minecraftservices.com/minecraft/profile",
                         headers={"Authorization": "Bearer " + mc["access_token"]})
    if st == 404:
        raise AuthError("На этом аккаунте нет Minecraft: Java Edition.")
    if st != 200:
        raise AuthError(f"Не удалось получить профиль: HTTP {st}")
    return mc["access_token"], int(mc.get("expires_in", 86400)), prof


def ms_ensure(acc, client_id):
    if acc.get("expires_at", 0) > time.time() + 120:
        return acc
    _need_client_id(client_id)
    if not acc.get("refresh_token"):
        raise AuthError("Нужно войти в Microsoft заново.")
    st, tok = http_json(_MS + "/token", form={
        "grant_type": "refresh_token", "client_id": client_id,
        "refresh_token": acc["refresh_token"], "scope": _SCOPE})
    if st != 200:
        raise AuthError(f"Сессия Microsoft истекла, войди заново ({tok.get('error_description', tok.get('error'))})")
    return _ms_complete(tok)


def ensure_fresh(acc, client_id):
    try:
        return ely_ensure(acc) if acc["type"] == "ely" else ms_ensure(acc, client_id)
    except NetError as e:
        raise AuthError(str(e))
