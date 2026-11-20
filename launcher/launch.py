"""Полный цикл запуска: сборка → Java → игра → загрузчик → авторизация → процесс."""
import os
import shlex
import subprocess

from . import APP_NAME, APP_VERSION, auth, loaders, mojang, packs
from .java_rt import ensure_java
from .util import ROOT, NetError, download, get_json, popen

AUTHLIB_PATH = os.path.join(ROOT, "authlib-injector.jar")


def ensure_authlib(log):
    if os.path.isfile(AUTHLIB_PATH):
        return AUTHLIB_PATH
    log("Скачиваю authlib-injector (нужен для Ely.by)…")
    url = None
    try:
        url = get_json("https://authlib-injector.yushi.moe/artifact/latest.json")["download_url"]
    except (NetError, KeyError):
        rel = get_json("https://api.github.com/repos/yushijinhun/authlib-injector/releases/latest")
        for a in rel.get("assets", []):
            if a["name"].endswith(".jar"):
                url = a["browser_download_url"]
    if not url:
        raise RuntimeError("Не удалось найти authlib-injector для загрузки.")
    download(url, AUTHLIB_PATH)
    return AUTHLIB_PATH


def prepare_manifest(index_url, pack, log):
    """Берём свежий манифест; если сервер недоступен — используем сохранённый."""
    try:
        return packs.fetch_manifest(index_url, pack)
    except NetError as e:
        cached = packs.cached_manifest(pack["id"])
        if cached:
            log(f"Сервер сборок недоступен ({e}). Запускаю последнюю установленную версию.")
            return cached
        raise


def launch(index_url, pack, account, settings, progress, log):
    """Готовит всё необходимое и запускает игру. Возвращает subprocess.Popen."""
    manifest = prepare_manifest(index_url, pack, log)

    log(f"Синхронизирую сборку «{manifest.get('name', pack['id'])}» {manifest.get('version', '')}…")
    try:
        game_dir = packs.sync_pack(manifest, progress, log)
    except NetError as e:
        if packs.installed_state(pack["id"]):
            log(f"Не удалось обновить файлы ({e}). Пробую запустить как есть.")
            game_dir = packs.instance_dir(pack["id"])
        else:
            raise
    return run_game(manifest, game_dir, account, settings, progress, log)


def launch_local(inst, game_dir, account, settings, progress, log):
    """Запуск собственной сборки (без синхронизации с сервером)."""
    return run_game(inst, game_dir, account, settings, progress, log)


def run_game(manifest, game_dir, account, settings, progress, log):
    """manifest: нужны mc_version, loader; необязательно java_args, ram_mb."""
    mc = manifest["mc_version"]
    log(f"Готовлю Minecraft {mc}…")
    mojang.install_vanilla_json(mc)
    java = ensure_java(mojang.load_version(mc), settings.get("java_path", ""), progress, log)
    vid = loaders.install_loader(manifest.get("loader"), mc, java, log)

    version, classpath, natdir, jar, game_assets = mojang.install_game_files(vid, game_dir, progress)
    java = ensure_java(version, settings.get("java_path", ""), progress, log)

    log("Проверяю аккаунт…")
    account = auth.ensure_fresh(account, settings.get("ms_client_id", ""))

    variables = {
        "auth_player_name": account["name"],
        "auth_uuid": account["uuid"],
        "auth_access_token": account["access_token"],
        "auth_session": f"token:{account['access_token']}:{account['uuid']}",
        "user_type": "msa" if account["type"] == "ms" else "mojang",
        "user_properties": "{}",
        "clientid": "", "auth_xuid": "",
        "version_name": vid,
        "version_type": version.get("type", "release"),
        "game_directory": game_dir,
        "assets_root": mojang.ASSETS,
        "game_assets": game_assets,
        "assets_index_name": version["assetIndex"]["id"],
        "natives_directory": natdir,
        "launcher_name": APP_NAME,
        "launcher_version": APP_VERSION,
        "classpath": classpath,
        "classpath_separator": os.pathsep,
        "library_directory": mojang.LIBS,
        "primary_jar": jar,
    }
    jvm, game = mojang.build_arguments(version, variables)

    pre = []
    if account["type"] == "ely":
        pre.append(f"-javaagent:{ensure_authlib(log)}={auth.ELY_AGENT_ARG}")
    ram = int(manifest.get("ram_mb") or settings.get("ram_mb", 4096))
    pre += [f"-Xmx{ram}M", f"-Xms{min(ram, 1024)}M"]
    if manifest.get("java_args"):
        pre += shlex.split(manifest["java_args"])

    cmd = [java] + pre + jvm + [version["mainClass"]] + game
    log(f"Запуск игры под ником {account['name']}…")
    os.makedirs(game_dir, exist_ok=True)
    return popen(cmd, cwd=game_dir, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                 text=True, encoding="utf-8", errors="replace")
