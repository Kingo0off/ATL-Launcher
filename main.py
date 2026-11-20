"""Точка входа. Обычный запуск — окно лаунчера. `--classic` — старый интерфейс на tkinter."""
import os
import sys

os.chdir(os.path.dirname(os.path.abspath(__file__)))  # чтобы запуск работал из любой папки
sys.path.insert(0, os.getcwd())

if sys.version_info < (3, 9):
    msg = "Нужен Python 3.9 или новее. У тебя: " + sys.version.split()[0]
    if sys.stderr:
        sys.exit(msg)
    raise SystemExit(msg)

if __name__ == "__main__":
    if "--classic" in sys.argv:
        from launcher.gui import run
        run()
    else:
        from launcher.app import main
        main()
