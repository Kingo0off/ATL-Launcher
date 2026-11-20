# Двойной клик по этому файлу запускает лаунчер БЕЗ чёрного окна консоли (.pyw открывается через pythonw).
import runpy
import os

runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), "main.py"), run_name="__main__")
