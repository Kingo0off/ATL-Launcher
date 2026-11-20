#!/usr/bin/env python3
"""Публикация сборки для FriendLauncher из командной строки.

То же самое умеет кнопка «Для друзей» в самом лаунчере; этот скрипт нужен, если удобнее из терминала.

    python tools/publish.py my-pack --id friends-smp --name "Наш сервер" \
        --mc 1.20.1 --loader fabric --loader-version 0.15.11 --version 1.0.0 --out site

Параметры можно положить в my-pack/pack.json, тогда достаточно:  python tools/publish.py my-pack --out site
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from launcher import publisher  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("folder", help="папка со сборкой")
    ap.add_argument("--out", default="site", help="куда складывать результат (по умолчанию ./site)")
    ap.add_argument("--id")
    ap.add_argument("--name")
    ap.add_argument("--description")
    ap.add_argument("--version", help="версия сборки (меняй при каждом обновлении)")
    ap.add_argument("--mc", help="версия Minecraft, например 1.20.1")
    ap.add_argument("--loader", choices=["vanilla", "fabric", "quilt", "forge", "neoforge"])
    ap.add_argument("--loader-version")
    ap.add_argument("--java-args")
    ap.add_argument("--once", action="append", default=None, help="файл, который копируется только если его ещё нет")
    ap.add_argument("--strict", action="append", default=None, help="папка, где лишние файлы у игрока удаляются")
    ap.add_argument("--remove", action="store_true", help="убрать сборку с этим --id из index.json")
    a = ap.parse_args()

    folder = os.path.abspath(a.folder)
    meta = {}
    meta_path = os.path.join(folder, "pack.json")
    if os.path.isfile(meta_path):
        with open(meta_path, encoding="utf-8") as f:
            meta = json.load(f)

    def opt(cli, key, default=None):
        return cli if cli is not None else meta.get(key, default)

    pack_id = opt(a.id, "id")
    if a.remove:
        if not pack_id:
            sys.exit("Для --remove нужен --id")
        publisher.remove_pack(os.path.abspath(a.out), pack_id)
        print(f"Сборка {pack_id} убрана из index.json")
        return

    try:
        res = publisher.publish_pack(folder, a.out, {
            "id": pack_id, "name": opt(a.name, "name", pack_id), "description": opt(a.description, "description", ""),
            "version": opt(a.version, "version"), "mc_version": opt(a.mc, "mc_version"),
            "loader": {"type": opt(a.loader, "loader", "vanilla"), "version": opt(a.loader_version, "loader_version", "")},
            "java_args": opt(a.java_args, "java_args"),
            "once": a.once if a.once is not None else meta.get("once"),
            "strict": a.strict if a.strict is not None else meta.get("strict"),
        })
    except ValueError as e:
        sys.exit(str(e))
    print(f"Готово: «{res['name']}» {res['version']} — {res['files']} файлов, {res['size'] / 1048576:.1f} МБ")
    print(f"Залей папку {res['out']} на хостинг. Адрес для лаунчера — ссылка на index.json (или на папку).")
    if res["big"]:
        print("ВНИМАНИЕ: файлы больше 95 МБ не примет GitHub (лимит 100 МБ):", ", ".join(res["big"]))


if __name__ == "__main__":
    main()
