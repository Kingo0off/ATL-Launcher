"""Скриншоты всех сборок в одном месте."""
import os
import shutil

from .util import safe_join

EXTS = (".png", ".jpg", ".jpeg")
LIMIT = 3000


def list_all(refs):
    """refs: [(key, name, game_dir)]. Возвращает скриншоты от новых к старым."""
    out = []
    for key, name, gdir in refs:
        d = os.path.join(gdir, "screenshots")
        try:
            names = os.listdir(d)
        except OSError:
            continue
        for f in names:
            if not f.lower().endswith(EXTS):
                continue
            try:
                st = os.stat(os.path.join(d, f))
            except OSError:
                continue
            out.append({"key": key, "inst": name, "file": f, "mtime": int(st.st_mtime), "size": st.st_size})
    out.sort(key=lambda s: s["mtime"], reverse=True)
    return out[:LIMIT]


def _path(gdir, file):
    if os.path.basename(file) != file or not file.lower().endswith(EXTS):
        raise ValueError("Неверное имя файла")
    return safe_join(os.path.join(gdir, "screenshots"), file)


def read(gdir, file):
    try:
        with open(_path(gdir, file), "rb") as f:
            return f.read(), ("image/png" if file.lower().endswith(".png") else "image/jpeg")
    except OSError:
        return None


def delete(gdir, file):
    p = _path(gdir, file)
    if os.path.isfile(p):
        os.remove(p)


def copy_to(gdir, file, dest_dir):
    """Копирует скриншот в папку (например, «Изображения»). Возвращает путь."""
    p = _path(gdir, file)
    os.makedirs(dest_dir, exist_ok=True)
    dest, n = os.path.join(dest_dir, file), 1
    while os.path.exists(dest):
        base, ext = os.path.splitext(file)
        dest = os.path.join(dest_dir, f"{base} ({n}){ext}")
        n += 1
    shutil.copyfile(p, dest)
    return dest
