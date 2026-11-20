"""Настройки лаунчера.

Две константы ниже можно заранее прописать перед тем, как отдавать лаунчер друзьям —
тогда им ничего не придётся настраивать. Остальное меняется в окне «Настройки».
"""
import json
import os

from .util import ROOT

# Адрес index.json со списком твоих сборок (см. README, раздел «Публикация сборок»).
# Можно указать и просто папку — тогда в конец добавится /index.json
DEFAULT_SERVER_URL = ""

# Application (client) ID приложения из Azure для входа через Microsoft (см. README).
DEFAULT_MS_CLIENT_ID = ""

_SETTINGS = os.path.join(ROOT, "settings.json")

DEFAULTS = {
    "server_url": DEFAULT_SERVER_URL,
    "ms_client_id": DEFAULT_MS_CLIENT_ID,
    "cf_api_key": "",
    "ram_mb": 4096,
    "java_path": "",
    "last_account": "",
    "site_dir": "",
}


def load():
    data = dict(DEFAULTS)
    try:
        with open(_SETTINGS, encoding="utf-8") as f:
            data.update(json.load(f))
    except (OSError, ValueError):
        pass
    return data


def save(data):
    with open(_SETTINGS, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
