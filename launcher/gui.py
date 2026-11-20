"""Окно лаунчера на tkinter (входит в стандартную поставку Python)."""
import os
import queue
import shutil
import subprocess
import sys
import threading
import tkinter as tk
import webbrowser
from tkinter import filedialog, messagebox, ttk

from . import APP_NAME, auth, config, launch, local, packs
from .util import NetError

BG, PANEL, FG, MUTED, ACCENT, ERR = "#1e1f26", "#272833", "#e8e8ee", "#8b8d9a", "#4caf7a", "#e57373"
LOADER_KEY = {v: k for k, v in local.LOADER_TITLES.items()}


def open_folder(path):
    os.makedirs(path, exist_ok=True)
    if sys.platform == "win32":
        os.startfile(path)  # noqa: S606
    elif sys.platform == "darwin":
        subprocess.Popen(["open", path])
    else:
        subprocess.Popen(["xdg-open", path])


class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title(APP_NAME)
        self.geometry("960x660")
        self.minsize(860, 600)
        self.configure(bg=BG)
        self.settings = config.load()
        self.q = queue.Queue()
        self.index_url = None
        self.packs = []      # сборки с сервера
        self.locals = []     # свои сборки
        self.accounts = []
        self.busy = False
        self.ms_cancel = threading.Event()
        self._style()
        self._build()
        self._refresh_accounts()
        self._fill_locals()
        self.after(100, self._pump)
        self.after(200, self.reload_packs)

    # ---------- оформление ----------
    def _style(self):
        s = ttk.Style(self)
        s.theme_use("clam")
        s.configure(".", background=BG, foreground=FG, fieldbackground=PANEL, bordercolor=PANEL)
        s.configure("TButton", background=PANEL, foreground=FG, padding=6, borderwidth=0)
        s.map("TButton", background=[("active", "#33354a"), ("disabled", "#2a2b35")],
              foreground=[("disabled", "#666")])
        s.configure("Play.TButton", background=ACCENT, foreground="#08140d", font=("TkDefaultFont", 13, "bold"), padding=10)
        s.map("Play.TButton", background=[("active", "#5cc58d"), ("disabled", "#2f5a43")])
        s.configure("TCombobox", fieldbackground=PANEL, background=PANEL, foreground=FG, arrowcolor=FG)
        s.configure("TEntry", fieldbackground=PANEL, foreground=FG)
        s.configure("TCheckbutton", background=BG, foreground=FG)
        s.configure("TNotebook", background=BG, borderwidth=0)
        s.configure("TNotebook.Tab", background=PANEL, foreground=FG, padding=(14, 6))
        s.map("TNotebook.Tab", background=[("selected", "#33354a")])
        s.configure("Horizontal.TProgressbar", troughcolor=PANEL, background=ACCENT, bordercolor=PANEL)
        s.configure("TLabel", background=BG, foreground=FG)

    def _listbox(self, parent):
        lb = tk.Listbox(parent, width=28, bg=PANEL, fg=FG, selectbackground=ACCENT, selectforeground="#08140d",
                        highlightthickness=0, borderwidth=0, activestyle="none", font=("TkDefaultFont", 11),
                        exportselection=False)
        lb.bind("<<ListboxSelect>>", lambda e: self._show())
        return lb

    def _build(self):
        top = tk.Frame(self, bg=BG)
        top.pack(fill="x", padx=14, pady=(12, 6))
        tk.Label(top, text=APP_NAME, bg=BG, fg=FG, font=("TkDefaultFont", 16, "bold")).pack(side="left")
        ttk.Button(top, text="⚙", width=3, command=self.open_settings).pack(side="right")
        ttk.Button(top, text="Удалить", command=self.remove_account).pack(side="right", padx=4)
        ttk.Button(top, text="+ Microsoft", command=self.add_ms).pack(side="right", padx=4)
        ttk.Button(top, text="+ Ely.by", command=self.add_ely).pack(side="right", padx=4)
        self.acc_var = tk.StringVar()
        self.acc_box = ttk.Combobox(top, textvariable=self.acc_var, state="readonly", width=28)
        self.acc_box.pack(side="right", padx=8)
        self.acc_box.bind("<<ComboboxSelected>>", self._acc_selected)

        main = tk.Frame(self, bg=BG)
        main.pack(fill="both", expand=True, padx=14, pady=6)

        self.nb = ttk.Notebook(main)
        self.nb.pack(side="left", fill="y")
        tab_local = tk.Frame(self.nb, bg=BG)
        tab_srv = tk.Frame(self.nb, bg=BG)
        self.nb.add(tab_local, text="Мои сборки")
        self.nb.add(tab_srv, text="С сервера")
        self.nb.bind("<<NotebookTabChanged>>", lambda e: self._show())
        self.lb_local = self._listbox(tab_local)
        self.lb_local.pack(fill="y", expand=True, pady=(6, 0))
        ttk.Button(tab_local, text="+ Новая сборка", command=lambda: self.edit_instance(None)).pack(fill="x", pady=(6, 0))
        self.lb_srv = self._listbox(tab_srv)
        self.lb_srv.pack(fill="y", expand=True, pady=(6, 0))
        ttk.Button(tab_srv, text="Обновить список", command=self.reload_packs).pack(fill="x", pady=(6, 0))

        right = tk.Frame(main, bg=PANEL)
        right.pack(side="left", fill="both", expand=True, padx=(12, 0))
        self.title_lbl = tk.Label(right, text="", bg=PANEL, fg=FG, font=("TkDefaultFont", 18, "bold"),
                                  anchor="w", justify="left")
        self.title_lbl.pack(fill="x", padx=18, pady=(18, 4))
        self.info_lbl = tk.Label(right, text="", bg=PANEL, fg=MUTED, anchor="w", justify="left")
        self.info_lbl.pack(fill="x", padx=18)
        self.desc_lbl = tk.Label(right, text="", bg=PANEL, fg=FG, anchor="nw", justify="left", wraplength=560)
        self.desc_lbl.pack(fill="both", expand=True, padx=18, pady=12)
        tools = tk.Frame(right, bg=PANEL)
        tools.pack(fill="x", padx=18, pady=(0, 10))
        self.tool_btns = {}
        for key, text, cmd in [("mods", "Моды", self.open_mods), ("folder", "Папка", self.open_instance_folder),
                               ("edit", "Изменить", self.edit_selected), ("delete", "Удалить сборку", self.delete_selected)]:
            b = ttk.Button(tools, text=text, command=cmd)
            b.pack(side="left", padx=(0, 6))
            self.tool_btns[key] = b
        self.play_btn = ttk.Button(right, text="Играть", style="Play.TButton", command=self.play, state="disabled")
        self.play_btn.pack(fill="x", padx=18, pady=(0, 18))

        bottom = tk.Frame(self, bg=BG)
        bottom.pack(fill="x", padx=14, pady=(0, 12))
        self.status = tk.Label(bottom, text="", bg=BG, fg=MUTED, anchor="w")
        self.status.pack(fill="x")
        self.bar = ttk.Progressbar(bottom, mode="determinate")
        self.bar.pack(fill="x", pady=4)
        self.log_text = tk.Text(bottom, height=7, bg="#14151a", fg="#b8bac6", borderwidth=0, highlightthickness=0,
                                state="disabled", font=("TkFixedFont", 9))
        self.log_text.pack(fill="x")

    # ---------- очередь UI ----------
    def ui(self, fn):
        self.q.put(fn)

    def _pump(self):
        try:
            while True:
                self.q.get_nowait()()
        except queue.Empty:
            pass
        self.after(100, self._pump)

    def log(self, text):
        def f():
            self.log_text.configure(state="normal")
            self.log_text.insert("end", text + "\n")
            self.log_text.see("end")
            if int(self.log_text.index("end-1c").split(".")[0]) > 2000:
                self.log_text.delete("1.0", "500.0")
            self.log_text.configure(state="disabled")
        self.ui(f)

    def progress(self, done, total, label):
        def f():
            self.bar.configure(maximum=max(total, 1), value=done)
            self.status.configure(text=f"{label}: {done}/{total}")
        self.ui(f)

    def set_busy(self, busy):
        def f():
            self.busy = busy
            self._show()
            if not busy:
                self.bar.configure(value=0)
        self.ui(f)

    def set_status(self, text):
        self.ui(lambda: self.status.configure(text=text))

    # ---------- аккаунты ----------
    def _refresh_accounts(self, select=None):
        self.accounts = auth.load_accounts()
        labels = [auth.label(a) for a in self.accounts]
        self.acc_box["values"] = labels
        pick = select or self.settings.get("last_account")
        idx = next((i for i, a in enumerate(self.accounts) if a["uuid"] == pick), 0 if labels else -1)
        if idx >= 0:
            self.acc_box.current(idx)
        else:
            self.acc_var.set("")
        self._show()

    def current_account(self):
        i = self.acc_box.current()
        return self.accounts[i] if 0 <= i < len(self.accounts) else None

    def _acc_selected(self, _e=None):
        acc = self.current_account()
        if acc:
            self.settings["last_account"] = acc["uuid"]
            config.save(self.settings)

    def remove_account(self):
        acc = self.current_account()
        if acc and messagebox.askyesno(APP_NAME, f"Удалить аккаунт {acc['name']} из лаунчера?"):
            auth.remove_account(acc)
            self._refresh_accounts()

    def add_ely(self):
        d = tk.Toplevel(self)
        d.title("Вход через Ely.by")
        d.configure(bg=BG)
        d.resizable(False, False)
        d.transient(self)
        d.grab_set()
        entries = {}
        for i, (key, text, secret) in enumerate([("user", "Ник или e-mail", False), ("pw", "Пароль", True),
                                                 ("totp", "Код 2FA (если включён)", False)]):
            ttk.Label(d, text=text).grid(row=i, column=0, sticky="w", padx=12, pady=6)
            e = ttk.Entry(d, width=30, show="•" if secret else "")
            e.grid(row=i, column=1, padx=12, pady=6)
            entries[key] = e
        entries["user"].focus_set()
        msg = ttk.Label(d, text="", foreground=ERR, wraplength=300)
        msg.grid(row=3, column=0, columnspan=2, padx=12)

        def go():
            u, p, t = (entries[k].get().strip() for k in ("user", "pw", "totp"))
            if not u or not p:
                msg.configure(text="Введи ник и пароль")
                return
            btn.configure(state="disabled")
            msg.configure(text="Входим…", foreground=MUTED)

            def work():
                try:
                    acc = auth.ely_login(u, p, t)
                    auth.upsert_account(acc)
                    self.ui(lambda: (d.destroy(), self._refresh_accounts(acc["uuid"])))
                except (auth.AuthError, NetError) as ex:
                    text = str(ex)
                    if "two factor" in text.lower() or "2fa" in text.lower():
                        text += "\nВведи код из приложения-аутентификатора в поле 2FA."
                    self.ui(lambda: (msg.configure(text=text, foreground=ERR), btn.configure(state="normal")))
            threading.Thread(target=work, daemon=True).start()

        btn = ttk.Button(d, text="Войти", command=go)
        btn.grid(row=4, column=0, columnspan=2, pady=12)
        d.bind("<Return>", lambda e: go())

    def add_ms(self):
        client_id = self.settings.get("ms_client_id", "").strip()
        if not client_id:
            messagebox.showinfo(APP_NAME, "Сначала вставь Client ID приложения Microsoft в настройках (⚙).\n"
                                          "Как его получить — в README, раздел «Вход через Microsoft».")
            return
        self.ms_cancel.clear()
        d = tk.Toplevel(self)
        d.title("Вход через Microsoft")
        d.configure(bg=BG)
        d.resizable(False, False)
        d.transient(self)
        info = ttk.Label(d, text="Связываюсь с Microsoft…", wraplength=360, justify="center")
        info.pack(padx=20, pady=(18, 8))
        code_lbl = tk.Label(d, text="", bg=BG, fg=ACCENT, font=("TkFixedFont", 26, "bold"))
        code_lbl.pack(padx=20)
        btns = tk.Frame(d, bg=BG)
        btns.pack(pady=14)

        def close():
            self.ms_cancel.set()
            d.destroy()
        d.protocol("WM_DELETE_WINDOW", close)

        def on_code(code, url):
            def f():
                info.configure(text=f"Открой {url} и введи этот код (скопирован в буфер обмена):")
                code_lbl.configure(text=code)
                self.clipboard_clear()
                self.clipboard_append(code)
                ttk.Button(btns, text="Открыть страницу", command=lambda: webbrowser.open(url)).pack(side="left", padx=4)
                ttk.Button(btns, text="Отмена", command=close).pack(side="left", padx=4)
                webbrowser.open(url)
            self.ui(f)

        def work():
            try:
                acc = auth.ms_device_flow(client_id, on_code, self.ms_cancel.is_set)
                self.ui(lambda: (d.destroy() if d.winfo_exists() else None, self._refresh_accounts(acc["uuid"])))
            except (auth.AuthError, NetError) as ex:
                if not self.ms_cancel.is_set():
                    self.ui(lambda: (d.destroy() if d.winfo_exists() else None, messagebox.showerror(APP_NAME, str(ex))))
        threading.Thread(target=work, daemon=True).start()

    # ---------- списки сборок ----------
    def _fill_locals(self, select_id=None):
        self.locals = local.list_instances()
        self.lb_local.delete(0, "end")
        for i in self.locals:
            self.lb_local.insert("end", i["name"])
        if self.locals:
            idx = next((n for n, i in enumerate(self.locals) if i["id"] == select_id), 0)
            self.lb_local.selection_set(idx)
        self._show()

    def reload_packs(self):
        if not self.settings.get("server_url"):
            self.set_status("Адрес сервера сборок не задан — вкладка «С сервера» пуста (⚙ Настройки)")
            return
        self.set_status("Загружаю список сборок…")

        def work():
            try:
                url, plist = packs.fetch_index(self.settings.get("server_url"))
                self.index_url, self.packs = url, plist
                self.set_status(f"Сборок на сервере: {len(plist)}")
            except NetError as e:
                self.log(f"Список сборок: {e}")
                self.set_status("Не удалось загрузить список сборок (см. лог / настройки)")
            self.ui(self._fill_packs)
        threading.Thread(target=work, daemon=True).start()

    def _fill_packs(self):
        self.lb_srv.delete(0, "end")
        for p in self.packs:
            self.lb_srv.insert("end", p.get("name", p["id"]))
        if self.packs:
            self.lb_srv.selection_set(0)
        self._show()

    def current(self):
        """('local', inst) / ('server', pack) / (None, None)."""
        tab = self.nb.index(self.nb.select())
        lb, items, kind = (self.lb_local, self.locals, "local") if tab == 0 else (self.lb_srv, self.packs, "server")
        sel = lb.curselection()
        if sel and sel[0] < len(items):
            return kind, items[sel[0]]
        return kind, None

    def _show(self):
        kind, item = self.current()
        if item is None:
            if kind == "local":
                self.title_lbl.configure(text="Своих сборок пока нет")
                self.desc_lbl.configure(text="Нажми «+ Новая сборка», выбери версию Minecraft и загрузчик.")
            else:
                self.title_lbl.configure(text="Сборок с сервера нет")
                self.desc_lbl.configure(text="Проверь адрес сервера в настройках (⚙).")
            self.info_lbl.configure(text="")
        elif kind == "local":
            self.title_lbl.configure(text=item["name"])
            self.info_lbl.configure(text=local.describe(item))
            extra = []
            if item.get("ram_mb"):
                extra.append(f"Память: {item['ram_mb']} МБ")
            if item.get("java_args"):
                extra.append(f"JVM: {item['java_args']}")
            self.desc_lbl.configure(text="\n".join(extra))
        else:
            loader = item.get("loader") or {}
            ltxt = local.LOADER_TITLES.get(loader.get("type", "vanilla"), loader.get("type", ""))
            if loader.get("version"):
                ltxt += " " + loader["version"]
            self.title_lbl.configure(text=item.get("name", item["id"]))
            self.info_lbl.configure(text=f"Версия сборки {item.get('version', '?')}  •  Minecraft {item.get('mc_version', '?')}  •  {ltxt}")
            self.desc_lbl.configure(text=item.get("description", ""))

        has = item is not None
        is_local = kind == "local" and has
        installed = bool(kind == "server" and has and packs.installed_state(item["id"]))
        self.tool_btns["mods"].configure(state="normal" if is_local and not self.busy else "disabled")
        self.tool_btns["edit"].configure(state="normal" if is_local and not self.busy else "disabled")
        self.tool_btns["delete"].configure(state="normal" if is_local and not self.busy else "disabled")
        self.tool_btns["folder"].configure(state="normal" if (is_local or installed) else "disabled")

        if self.busy:
            self.play_btn.configure(state="disabled", text="Подожди…")
        elif not has:
            self.play_btn.configure(state="disabled", text="Играть")
        elif not self.current_account():
            self.play_btn.configure(state="disabled", text="Сначала добавь аккаунт (сверху)")
        else:
            text = "Играть"
            if kind == "server":
                st = packs.installed_state(item["id"])
                if not st:
                    text = "Установить и играть"
                elif st.get("version") != item.get("version"):
                    text = "Обновить и играть"
            self.play_btn.configure(state="normal", text=text)

    # ---------- действия над сборкой ----------
    def instance_path(self):
        kind, item = self.current()
        if kind == "local" and item:
            return local.game_dir(item)
        if kind == "server" and item:
            return packs.instance_dir(item["id"])
        return None

    def open_instance_folder(self):
        p = self.instance_path()
        if p:
            open_folder(p)

    def edit_selected(self):
        kind, item = self.current()
        if kind == "local" and item:
            self.edit_instance(item)

    def delete_selected(self):
        kind, item = self.current()
        if kind == "local" and item and messagebox.askyesno(
                APP_NAME, f"Удалить сборку «{item['name']}» вместе с её мирами, модами и настройками?\nЭто нельзя отменить."):
            local.delete_instance(item)
            self._fill_locals()

    # ---------- диалог создания / правки ----------
    def edit_instance(self, inst):
        editing = inst is not None
        inst = dict(inst) if editing else {"name": "", "mc_version": "", "loader": {"type": "fabric", "version": ""},
                                           "ram_mb": 0, "java_args": ""}
        d = tk.Toplevel(self)
        d.title("Изменить сборку" if editing else "Новая сборка")
        d.configure(bg=BG)
        d.resizable(False, False)
        d.transient(self)
        d.grab_set()

        loader0 = inst.get("loader") or {}
        name_v = tk.StringVar(value=inst.get("name", ""))
        mc_v = tk.StringVar(value=inst.get("mc_version", ""))
        snap_v = tk.BooleanVar(value=False)
        loader_v = tk.StringVar(value=local.LOADER_TITLES.get(loader0.get("type", "vanilla"), "Vanilla"))
        lver_v = tk.StringVar(value=loader0.get("version", ""))
        ram_v = tk.StringVar(value=str(inst.get("ram_mb") or ""))
        jargs_v = tk.StringVar(value=inst.get("java_args", ""))

        def row(i, text, widget):
            ttk.Label(d, text=text).grid(row=i, column=0, sticky="w", padx=14, pady=6)
            widget.grid(row=i, column=1, sticky="we", padx=14, pady=6)

        row(0, "Название", ttk.Entry(d, textvariable=name_v, width=34))
        mc_box = ttk.Combobox(d, textvariable=mc_v, state="readonly", width=32)
        row(1, "Версия Minecraft", mc_box)
        snap = ttk.Checkbutton(d, text="Показывать снапшоты и бета-версии", variable=snap_v)
        snap.grid(row=2, column=1, sticky="w", padx=14)
        loader_box = ttk.Combobox(d, textvariable=loader_v, state="readonly", width=32,
                                  values=[local.LOADER_TITLES[k] for k in local.LOADERS])
        row(3, "Загрузчик модов", loader_box)
        lv_box = ttk.Combobox(d, textvariable=lver_v, width=32)
        row(4, "Версия загрузчика", lv_box)
        row(5, "Память, МБ (пусто = общая)", ttk.Entry(d, textvariable=ram_v, width=34))
        row(6, "Доп. аргументы JVM", ttk.Entry(d, textvariable=jargs_v, width=34))
        status = ttk.Label(d, text="", foreground=MUTED, wraplength=380)
        status.grid(row=7, column=0, columnspan=2, padx=14, pady=(4, 0), sticky="w")

        def load_loader(*_):
            kind = LOADER_KEY.get(loader_v.get(), "vanilla")
            mc = mc_v.get()
            if kind == "vanilla":
                lv_box.configure(values=[], state="disabled")
                lver_v.set("")
                status.configure(text="", foreground=MUTED)
                return
            lv_box.configure(state="normal")
            if not mc:
                return
            status.configure(text="Загружаю версии загрузчика…", foreground=MUTED)

            def work():
                err = ""
                try:
                    vs = local.loader_versions(kind, mc)
                except Exception as e:
                    vs, err = [], str(e)

                def apply():
                    if not d.winfo_exists():
                        return
                    lv_box.configure(values=vs)
                    if vs and lver_v.get() not in vs:
                        lver_v.set(vs[0])
                    if err:
                        status.configure(text=f"Не удалось получить список: {err}. Можно ввести версию вручную.",
                                         foreground=ERR)
                    elif not vs:
                        status.configure(text=f"Для Minecraft {mc} нет версий {local.LOADER_TITLES[kind]}.",
                                         foreground=ERR)
                    else:
                        status.configure(text="", foreground=MUTED)
                self.ui(apply)
            threading.Thread(target=work, daemon=True).start()

        def load_mc(*_):
            status.configure(text="Загружаю версии Minecraft…", foreground=MUTED)

            def work():
                try:
                    vs = local.mc_versions(snap_v.get())
                except NetError as e:
                    self.ui(lambda: d.winfo_exists() and status.configure(text=str(e), foreground=ERR))
                    return

                def apply():
                    if not d.winfo_exists():
                        return
                    cur = mc_v.get()
                    mc_box.configure(values=vs if cur in vs or not cur else [cur] + vs)
                    if not cur and vs:
                        mc_v.set(vs[0])
                    load_loader()
                self.ui(apply)
            threading.Thread(target=work, daemon=True).start()

        mc_box.bind("<<ComboboxSelected>>", load_loader)
        loader_box.bind("<<ComboboxSelected>>", load_loader)
        snap.configure(command=load_mc)

        def save():
            name = name_v.get().strip()
            if not name:
                messagebox.showerror(APP_NAME, "Введи название сборки.", parent=d)
                return
            if not mc_v.get():
                messagebox.showerror(APP_NAME, "Выбери версию Minecraft.", parent=d)
                return
            kind = LOADER_KEY.get(loader_v.get(), "vanilla")
            lver = lver_v.get().strip() if kind != "vanilla" else ""
            if kind in ("forge", "neoforge") and not lver:
                messagebox.showerror(APP_NAME, f"Для {loader_v.get()} нужна версия загрузчика.", parent=d)
                return
            try:
                ram = int(ram_v.get()) if ram_v.get().strip() else 0
                if ram and ram < 512:
                    raise ValueError
            except ValueError:
                messagebox.showerror(APP_NAME, "Память — число от 512 МБ или пусто.", parent=d)
                return
            inst.update({"name": name, "mc_version": mc_v.get(), "loader": {"type": kind, "version": lver},
                         "ram_mb": ram, "java_args": jargs_v.get().strip()})
            saved = local.save_instance(inst)
            d.destroy()
            self.nb.select(0)
            self._fill_locals(saved["id"])

        ttk.Button(d, text="Сохранить" if editing else "Создать", command=save).grid(
            row=8, column=0, columnspan=2, pady=14)
        load_mc()

    # ---------- менеджер модов ----------
    def open_mods(self):
        kind, inst = self.current()
        if kind != "local" or not inst:
            return
        mods_dir = os.path.join(local.game_dir(inst), "mods")
        os.makedirs(mods_dir, exist_ok=True)
        d = tk.Toplevel(self)
        d.title(f"Моды — {inst['name']}")
        d.configure(bg=BG)
        d.geometry("560x440")
        d.transient(self)
        if (inst.get("loader") or {}).get("type", "vanilla") == "vanilla":
            ttk.Label(d, text="У этой сборки не выбран загрузчик — моды работать не будут. "
                              "Нажми «Изменить» и выбери Fabric/Quilt/Forge/NeoForge.",
                      foreground=ERR, wraplength=520).pack(padx=14, pady=(10, 0), anchor="w")
        frame = tk.Frame(d, bg=BG)
        frame.pack(fill="both", expand=True, padx=14, pady=10)
        lb = tk.Listbox(frame, bg=PANEL, fg=FG, selectbackground=ACCENT, selectforeground="#08140d",
                        highlightthickness=0, borderwidth=0, activestyle="none", selectmode="extended")
        sb = ttk.Scrollbar(frame, command=lb.yview)
        lb.configure(yscrollcommand=sb.set)
        sb.pack(side="right", fill="y")
        lb.pack(side="left", fill="both", expand=True)
        files = []

        def refresh():
            files.clear()
            lb.delete(0, "end")
            for f in sorted(os.listdir(mods_dir), key=str.lower):
                if f.endswith((".jar", ".jar.disabled")):
                    files.append(f)
                    if f.endswith(".disabled"):
                        lb.insert("end", "✗ " + f[:-len(".disabled")] + "   (выключен)")
                    else:
                        lb.insert("end", "✓ " + f)

        def selected():
            return [files[i] for i in lb.curselection()]

        def add():
            paths = filedialog.askopenfilenames(parent=d, title="Выбери моды", filetypes=[("Моды (.jar)", "*.jar")])
            for p in paths:
                shutil.copy(p, os.path.join(mods_dir, os.path.basename(p)))
            refresh()

        def toggle():
            for f in selected():
                src = os.path.join(mods_dir, f)
                dst = src[:-len(".disabled")] if f.endswith(".disabled") else src + ".disabled"
                os.replace(src, dst)
            refresh()

        def remove():
            sel = selected()
            if sel and messagebox.askyesno(APP_NAME, f"Удалить выбранные моды ({len(sel)})?", parent=d):
                for f in sel:
                    os.remove(os.path.join(mods_dir, f))
                refresh()

        btns = tk.Frame(d, bg=BG)
        btns.pack(fill="x", padx=14, pady=(0, 12))
        for text, cmd in [("Добавить…", add), ("Вкл / выкл", toggle), ("Удалить", remove),
                          ("Открыть папку", lambda: open_folder(mods_dir))]:
            ttk.Button(btns, text=text, command=cmd).pack(side="left", padx=(0, 6))
        refresh()

    # ---------- запуск ----------
    def play(self):
        kind, item = self.current()
        acc = self.current_account()
        if not item or not acc or self.busy:
            return
        self.set_busy(True)
        index_url = self.index_url

        def work():
            try:
                if kind == "local":
                    proc = launch.launch_local(item, local.game_dir(item), acc, self.settings, self.progress, self.log)
                else:
                    proc = launch.launch(index_url, item, acc, self.settings, self.progress, self.log)
                self.set_status("Игра запущена")
                for line in proc.stdout:
                    self.log(line.rstrip())
                code = proc.wait()
                self.log(f"Игра закрыта (код {code})")
                self.set_status("Игра закрыта" if code == 0 else f"Игра завершилась с ошибкой (код {code}) — смотри лог")
            except Exception as e:  # показываем любую ошибку, чтобы не молча падало
                self.log(f"ОШИБКА: {e}")
                self.set_status("Ошибка запуска")
                msg = str(e)
                self.ui(lambda: messagebox.showerror(APP_NAME, msg))
            finally:
                self.set_busy(False)
        threading.Thread(target=work, daemon=True).start()

    # ---------- настройки ----------
    def open_settings(self):
        d = tk.Toplevel(self)
        d.title("Настройки")
        d.configure(bg=BG)
        d.transient(self)
        d.grab_set()
        fields = [("server_url", "Адрес сервера сборок (index.json)"), ("ms_client_id", "Microsoft Client ID"),
                  ("ram_mb", "Память для игры по умолчанию, МБ"), ("java_path", "Путь к java (пусто = авто)")]
        vars_ = {}
        for i, (key, text) in enumerate(fields):
            ttk.Label(d, text=text).grid(row=i, column=0, sticky="w", padx=12, pady=6)
            v = tk.StringVar(value=str(self.settings.get(key, "")))
            ttk.Entry(d, textvariable=v, width=48).grid(row=i, column=1, padx=12, pady=6)
            vars_[key] = v

        def save():
            try:
                ram = int(vars_["ram_mb"].get())
                if ram < 512:
                    raise ValueError
            except ValueError:
                messagebox.showerror(APP_NAME, "Память — число от 512 МБ.", parent=d)
                return
            self.settings.update({k: v.get().strip() for k, v in vars_.items()})
            self.settings["ram_mb"] = ram
            config.save(self.settings)
            d.destroy()
            self.reload_packs()
        ttk.Button(d, text="Сохранить", command=save).grid(row=len(fields), column=0, columnspan=2, pady=12)


def run():
    App().mainloop()
