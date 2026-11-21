use crate::instances::{count_files, load_track, loader_label, Store, LOADERS};
use crate::util::{open_path, open_url, read_json, valid_id};
use crate::net::Endpoints;
use crate::{accounts, loaders, mods, mojang, paths, settings, shots, worlds, APP_VERSION};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Запущенная (или готовящаяся) игра и её журнал.
pub struct RunTask {
    phase: Mutex<String>,
    error: Mutex<String>,
    code: Mutex<Option<i32>>,
    started: f64,
    lines: Mutex<(usize, VecDeque<String>)>,
    stop: AtomicBool,
    pid: std::sync::atomic::AtomicU32,
}

impl RunTask {
    fn new() -> RunTask {
        RunTask {
            phase: Mutex::new("preparing".into()),
            error: Mutex::new(String::new()),
            code: Mutex::new(None),
            started: now_secs(),
            lines: Mutex::new((0, VecDeque::new())),
            stop: AtomicBool::new(false),
            pid: std::sync::atomic::AtomicU32::new(0),
        }
    }
    fn add(&self, text: &str) {
        if let Ok(mut g) = self.lines.lock() {
            if g.1.len() >= 4000 {
                g.1.pop_front();
                g.0 += 1;
            }
            g.1.push_back(text.to_string());
        }
    }
    fn read(&self, since: usize) -> (Vec<String>, usize) {
        match self.lines.lock() {
            Ok(g) => {
                let start = since.max(g.0);
                (g.1.iter().skip(start - g.0).cloned().collect(), g.0 + g.1.len())
            }
            Err(_) => (Vec::new(), since),
        }
    }
    fn phase(&self) -> String {
        self.phase.lock().map(|p| p.clone()).unwrap_or_default()
    }
    fn set_phase(&self, p: &str) {
        if let Ok(mut g) = self.phase.lock() {
            *g = p.to_string();
        }
    }
}

/// Состояние входа через Microsoft (идёт в отдельном потоке).
pub struct MsFlow {
    state: Mutex<String>,
    code: Mutex<String>,
    url: Mutex<String>,
    error: Mutex<String>,
    cancel: AtomicBool,
}

impl MsFlow {
    fn get(m: &Mutex<String>) -> String {
        m.lock().map(|g| g.clone()).unwrap_or_default()
    }
    fn set(m: &Mutex<String>, v: &str) {
        if let Ok(mut g) = m.lock() {
            *g = v.to_string();
        }
    }
    fn public(&self) -> Value {
        json!({"state": Self::get(&self.state), "code": Self::get(&self.code), "url": Self::get(&self.url), "error": Self::get(&self.error)})
    }
}

#[derive(Default)]
pub struct RunState {
    tasks: Mutex<HashMap<String, Arc<RunTask>>>,
    progress: Mutex<Map<String, Value>>,
    notices: Mutex<(u64, Vec<Value>)>,
    prep: Mutex<()>,
    stats: Mutex<()>,
}

impl RunState {
    fn notice(&self, kind: &str, text: &str) {
        if let Ok(mut g) = self.notices.lock() {
            g.0 += 1;
            let id = g.0;
            g.1.push(json!({"id": id, "kind": kind, "text": text}));
            let n = g.1.len();
            if n > 30 {
                g.1.drain(..n - 30);
            }
        }
    }
    fn set_progress(&self, tag: &str, done: usize, total: usize, label: &str) {
        if let Ok(mut g) = self.progress.lock() {
            g.insert(tag.to_string(), json!({"label": label, "done": done, "total": total}));
        }
    }
    fn clear_progress(&self, tag: &str) {
        if let Ok(mut g) = self.progress.lock() {
            g.remove(tag);
        }
    }
}

/// Порт мира, открытого для сети, по журналам запущенных игр.
fn detect_world_port(run: &RunState) -> Option<u16> {
    let tasks: Vec<Arc<RunTask>> = run.tasks.lock().ok()?.values().cloned().collect();
    let running: Vec<&Arc<RunTask>> = tasks.iter().filter(|t| t.phase() == "running").collect();
    // Сначала по журналу игры, потом по портам, которые слушает процесс игры.
    running
        .iter()
        .filter_map(|t| crate::room::detect_port(&t.read(0).0))
        .last()
        .or_else(|| running.iter().filter_map(|t| crate::launch::listening_ports(t.pid.load(Ordering::Relaxed)).into_iter().max()).last())
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn record_play(dir: &Path, run: &RunState, key: &str, started: f64, seconds: f64) {
    let _g = run.stats.lock();
    let mut st = read_json(&dir.join("stats.json")).unwrap_or_else(|| json!({}));
    if !st.is_object() {
        st = json!({});
    }
    let prev = st[key]["playtime_s"].as_i64().unwrap_or(0);
    st[key] = json!({"last_played": started as i64, "playtime_s": prev + seconds as i64});
    let _ = crate::util::write_json(&dir.join("stats.json"), &st);
}

fn run_worker(dir: PathBuf, run: Arc<RunState>, task: Arc<RunTask>, key: String, name: String, spec: crate::launch::RunSpec) {
    let tag = key.clone();
    let log = |s: &str| task.add(s);
    let progress = |d: usize, t: usize, l: &str| run.set_progress(&tag, d, t, l);
    let result = (|| -> Result<(), String> {
        let mut cmd = {
            let _g = run.prep.lock();
            let mut spec = spec;
            spec.account = crate::auth::ensure_fresh(&dir, &spec.ep, &spec.account, &spec.ms_client_id)?;
            crate::launch::prepare_game(&spec, &progress, &log)?
        };
        run.clear_progress(&tag);
        let mut child = cmd.spawn().map_err(|e| format!("Не удалось запустить Java: {}", e))?;
        task.pid.store(child.id(), Ordering::Relaxed);
        task.set_phase("running");
        let started = now_secs();
        let code = crate::launch::pump(&mut child, &|l| task.add(&l), &task.stop);
        if let Ok(mut c) = task.code.lock() {
            *c = Some(code);
        }
        let stopped = task.stop.load(Ordering::Relaxed);
        let ok = stopped || code == 0 || code == 143 || code == -15;
        task.set_phase(if ok { "exited" } else { "crashed" });
        record_play(&dir, &run, &key, started, now_secs() - started);
        if !ok {
            run.notice("error", &format!("«{}» завершилась с ошибкой (код {}). Подробности в журнале сборки.", name, code));
        }
        Ok(())
    })();
    if let Err(e) = result {
        run.clear_progress(&tag);
        task.set_phase("error");
        if let Ok(mut g) = task.error.lock() {
            *g = e.clone();
        }
        task.add(&format!("ОШИБКА: {}", e));
        run.notice("error", &e);
    }
}

/// Ошибка, которую интерфейс покажет пользователю (HTTP 400).
#[derive(Debug)]
pub struct ApiError(pub String);

impl From<&str> for ApiError {
    fn from(s: &str) -> Self {
        ApiError(s.to_string())
    }
}

impl From<String> for ApiError {
    fn from(s: String) -> Self {
        ApiError(s)
    }
}

pub type ApiResult = Result<Value, ApiError>;

/// Ответ API: JSON или картинка.
pub enum Out {
    Json(Value),
    Image(Vec<u8>, &'static str),
    Missing,
}

/// Методы, которые можно вызывать запросом GET (остальные только POST).
pub const GET_OK: [&str; 10] = ["state", "poll", "log", "modicon", "ping", "latest_log", "shot", "skinimg", "skinurl", "insticon"];
pub const SUBFOLDERS: [&str; 9] = ["", "mods", "saves", "logs", "resourcepacks", "shaderpacks", "screenshots", "backups", "config"];
pub const MAX_UPLOAD: [(&str, u64); 4] = [("import", 4 << 30), ("content", 1 << 29), ("skin", 1 << 20), ("icon", 2 << 20)];

/// Значок сборки: пусто (пиксельный по зерну `icon_seed`) или `custom` (своя картинка).
pub fn valid_icon(s: &str) -> bool {
    s.is_empty() || s == "custom"
}

/// Ширина и высота PNG по заголовку (None, если это не PNG).
pub fn png_dims(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 24 || &d[..8] != b"\x89PNG\r\n\x1a\n" || &d[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([d[16], d[17], d[18], d[19]]);
    let h = u32::from_be_bytes([d[20], d[21], d[22], d[23]]);
    Some((w, h))
}

fn pstr<'a>(p: &'a Value, k: &str) -> Result<&'a str, ApiError> {
    p.get(k)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError(format!("Не хватает параметра запроса: {}", k)))
}

fn popt<'a>(p: &'a Value, k: &str) -> &'a str {
    p.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

#[allow(dead_code)]
struct Ref {
    id: String,
    gdir: PathBuf,
    inst: Value,
    mc: String,
    loader: Value,
    name: String,
}

pub struct Api {
    pub dir: PathBuf,
    pub store: Store,
    settings: Arc<Mutex<Map<String, Value>>>,
    ms_flow: Mutex<Option<Arc<MsFlow>>>,
    updates: Mutex<HashMap<String, Vec<Value>>>,
    self_update: Mutex<Option<crate::selfupdate::Info>>,
    room: Mutex<Option<crate::room::Active>>,
    room_port: Arc<std::sync::atomic::AtomicU16>,
    pub endpoints: Endpoints,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
    run: Arc<RunState>,
}

impl Api {
    pub fn new(dir: PathBuf) -> Api {
        let s = settings::load(&dir);
        let _ = std::fs::create_dir_all(dir.join("tmp"));
        crate::selfupdate::cleanup();
        Api { store: Store::new(dir.clone()), dir, settings: Arc::new(Mutex::new(s)), ms_flow: Mutex::new(None), updates: Mutex::new(HashMap::new()), self_update: Mutex::new(None), room: Mutex::new(None), room_port: Arc::new(std::sync::atomic::AtomicU16::new(0)), endpoints: Endpoints::default(), cache: Mutex::new(HashMap::new()), run: Arc::new(RunState::default()) }
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.dir.join("tmp")
    }

    fn public_settings(&self, s: &Map<String, Value>) -> Value {
        let mut out = Map::new();
        for k in settings::PUBLIC {
            out.insert(k.to_string(), s.get(k).cloned().unwrap_or_else(|| json!("")));
        }
        Value::Object(out)
    }

    fn stats(&self) -> Value {
        read_json(&self.dir.join("stats.json")).unwrap_or_else(|| json!({}))
    }

    fn item_local(&self, inst: &Value, stats: &Value) -> Value {
        let id = inst["id"].as_str().unwrap_or("");
        let key = format!("local:{}", id);
        let st = &stats[&key];
        let loader = if inst["loader"].is_object() { inst["loader"].clone() } else { json!({}) };
        let gdir = self.dir.join("local").join(id).join("minecraft");
        let seed = match inst["icon_seed"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => id.to_string(),
        };
        let ipath = self.icon_path(id);
        let custom = inst["icon"].as_str() == Some("custom");
        let (icon, icon_v) = if custom && ipath.is_file() {
            let v = std::fs::metadata(&ipath)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(1);
            ("custom".to_string(), v)
        } else if custom {
            (String::new(), 0)
        } else {
            (String::new(), 0)
        };
        json!({
            "key": key, "kind": "local", "id": id, "name": inst["name"], "mc_version": inst["mc_version"],
            "loader_label": loader_label(&loader), "loader": loader,
            "icon": icon, "icon_v": icon_v, "icon_seed": seed, "ram_mb": inst["ram_mb"].as_i64().unwrap_or(0),
            "java_args": inst["java_args"].as_str().unwrap_or(""), "notes": inst["notes"].as_str().unwrap_or(""),
            "last_played": st["last_played"].as_i64().unwrap_or(0), "playtime_s": st["playtime_s"].as_i64().unwrap_or(0),
            "mods": count_files(&gdir, "mods", &[".jar", ".jar.disabled"]),
            "phase": self.run.tasks.lock().ok().and_then(|t| t.get(&key).map(|x| json!(x.phase()))).unwrap_or(Value::Null)
        })
    }

    fn items(&self) -> Vec<Value> {
        let stats = self.stats();
        self.store.list().iter().map(|i| self.item_local(i, &stats)).collect()
    }

    fn icon_path(&self, id: &str) -> PathBuf {
        self.dir.join("local").join(id).join("icon.png")
    }

    fn local_ref(&self, key: &str) -> Result<Ref, ApiError> {
        let (kind, id) = key.split_once(':').unwrap_or(("", ""));
        match kind {
            "local" => {
                let inst = self.store.get(id).ok_or("Сборка не найдена")?;
                let loader = if inst["loader"].is_object() { inst["loader"].clone() } else { json!({}) };
                Ok(Ref {
                    id: id.to_string(),
                    gdir: self.store.game_dir(id),
                    mc: inst["mc_version"].as_str().unwrap_or("").to_string(),
                    name: inst["name"].as_str().unwrap_or("").to_string(),
                    loader,
                    inst,
                })
            }
            "srv" => Err("Сборки с сервера друзей появятся на одном из следующих этапов переноса на Rust.".into()),
            _ => Err("Неизвестная сборка".into()),
        }
    }

    /// Кэш ответов сети на 10 минут.
    fn cached(&self, key: &str, fetch: impl FnOnce() -> ApiResult) -> ApiResult {
        if let Ok(c) = self.cache.lock() {
            if let Some((t, v)) = c.get(key) {
                if t.elapsed() < Duration::from_secs(600) {
                    return Ok(v.clone());
                }
            }
        }
        let v = fetch()?;
        if let Ok(mut c) = self.cache.lock() {
            c.insert(key.to_string(), (Instant::now(), v.clone()));
        }
        Ok(v)
    }

    fn current_account(&self) -> Option<Value> {
        let accs = accounts::load(&self.dir);
        let sel = self.settings.lock().ok().and_then(|s| s.get("last_account").and_then(|v| v.as_str().map(String::from))).unwrap_or_default();
        accs.iter().find(|a| a["uuid"].as_str() == Some(sel.as_str())).or_else(|| accs.first()).cloned()
    }

    fn room_leave(&self) {
        let old = self.room.lock().ok().and_then(|mut g| g.take());
        if let Some(a) = old {
            a.stop();
        }
    }

    fn room_status(&self) -> Value {
        let manual = self.room_port.load(Ordering::Relaxed);
        let world = if manual != 0 { Some(manual) } else { detect_world_port(&self.run) };
        match self.room.lock().ok().and_then(|g| {
            g.as_ref().map(|a| {
                json!({
                    "mode": a.mode, "status": a.info.status(), "code": a.code, "error": a.info.error(),
                    "peers": a.info.peers.load(Ordering::Relaxed), "refused": a.info.refused.load(Ordering::Relaxed),
                    "listen_port": a.info.listen_port.load(Ordering::Relaxed)
                })
            })
        }) {
            Some(mut v) => {
                v["world_port"] = json!(world);
                v["manual_port"] = json!(manual);
                v
            }
            None => json!({"mode": "", "status": "", "code": "", "error": "", "peers": 0, "refused": 0, "listen_port": 0, "world_port": world, "manual_port": manual}),
        }
    }

    fn cf_key(&self) -> String {
        self.settings.lock().ok().and_then(|s| s.get("cf_api_key").and_then(|v| v.as_str().map(|x| x.trim().to_string()))).unwrap_or_default()
    }

    fn import_file(&self, path: &Path, tag: &str) -> Result<Value, String> {
        let run = self.run.clone();
        let r = crate::importer::import_archive(&self.store, &self.endpoints, &self.cf_key(), path, &|d, t, l| run.set_progress(tag, d, t, l), &|_| {});
        run.clear_progress(tag);
        r
    }

    fn catalog_install(&self, p: &Value, cf: bool) -> ApiResult {
        let ptype = if popt(p, "type").is_empty() { "mod" } else { popt(p, "type") };
        let tag = if popt(p, "key").is_empty() { "global".to_string() } else { popt(p, "key").to_string() };
        let pid = pstr(p, "project_id")?;
        let ckey = self.cf_key();
        let meta = json!({"title": p["title"], "icon_url": p["icon_url"], "site_url": p["site_url"]});
        let title = popt(p, "title");
        let site = popt(p, "site_url");
        let ep = &self.endpoints;
        if ptype == "modpack" {
            let (url, fname, sha) = if cf { crate::catalog::cf_modpack_file(ep, &ckey, pid, title, site)? } else { crate::catalog::mr_modpack_file(ep, pid)? };
            let tmp = self.tmp_dir().join(format!("{:08x}-{}", rand::random::<u32>(), fname));
            let dl = crate::net::download(&url, &tmp, sha.as_deref(), None, 3).map_err(|e| e.0);
            let imported = dl.and_then(|_| self.import_file(&tmp, &tag));
            let _ = std::fs::remove_file(&tmp);
            let mut inst = imported?;
            let skipped = inst.as_object_mut().and_then(|m| m.remove("skipped")).unwrap_or_else(|| json!([]));
            let mut out = json!({"instance": self.item_local(&inst, &self.stats())});
            if cf {
                out["skipped"] = skipped;
            }
            return Ok(out);
        }
        let r = self.local_ref(pstr(p, "key")?)?;
        let kind = r.loader["type"].as_str().unwrap_or("vanilla");
        if ptype == "mod" && kind == "vanilla" {
            return Err("У этой сборки не выбран загрузчик — моды не заработают. Выбери Fabric, Quilt, Forge или NeoForge в настройках сборки.".into());
        }
        let files = if cf {
            crate::catalog::cf_install(ep, &ckey, &r.gdir, &r.mc, kind, pid, ptype, &meta, true, &mut Default::default(), 0)?
        } else {
            crate::catalog::mr_install(ep, &r.gdir, &r.mc, kind, pid, ptype, &meta, true, &mut Default::default(), 0)?
        };
        Ok(json!({"installed": files}))
    }

    fn select_account(&self, uuid: &str) {
        if let Ok(mut s) = self.settings.lock() {
            s.insert("last_account".into(), json!(uuid));
            let _ = settings::save(&self.dir, &s);
        }
    }

    fn ms_client_id(&self) -> String {
        self.settings.lock().ok().and_then(|s| s.get("ms_client_id").and_then(|v| v.as_str().map(|x| x.trim().to_string()))).unwrap_or_default()
    }

    /// Аккаунт Microsoft с свежим токеном (для смены скина).
    fn ms_account(&self, uuid: &str) -> Result<Value, ApiError> {
        let acc = accounts::load(&self.dir)
            .into_iter()
            .find(|a| a["uuid"] == uuid && a["type"] == "ms")
            .ok_or("Скины можно менять только у аккаунта Microsoft. Войди через Microsoft в меню аккаунта.")?;
        Ok(crate::auth::ensure_fresh(&self.dir, &self.endpoints, &acc, &self.ms_client_id())?)
    }

    fn ms_start(&self, mode: &str) -> ApiResult {
        let cid = self.ms_client_id();
        if cid.is_empty() {
            return Err("Не задан Client ID приложения Microsoft. Вставь его в Настройках, раздел «Вход через Microsoft».".into());
        }
        let flow = Arc::new(MsFlow {
            state: Mutex::new("starting".into()),
            code: Mutex::new(String::new()),
            url: Mutex::new(String::new()),
            error: Mutex::new(String::new()),
            cancel: AtomicBool::new(false),
        });
        {
            let mut g = self.ms_flow.lock().map_err(|_| "внутренняя ошибка")?;
            if let Some(old) = g.as_ref() {
                if matches!(MsFlow::get(&old.state).as_str(), "starting" | "waiting") {
                    old.cancel.store(true, Ordering::Relaxed);
                }
            }
            *g = Some(flow.clone());
        }
        let (dir, ep, settings, f, code_mode) = (self.dir.clone(), self.endpoints.clone(), self.settings.clone(), flow.clone(), mode == "code");
        std::thread::spawn(move || {
            let res = if code_mode {
                crate::auth::ms_device_flow(&dir, &ep, &cid, &|code, url| {
                    MsFlow::set(&f.code, code);
                    MsFlow::set(&f.url, url);
                    MsFlow::set(&f.state, "waiting");
                }, &f.cancel)
            } else {
                crate::auth::ms_browser_flow(&dir, &ep, &cid, &|url| {
                    MsFlow::set(&f.url, url);
                    MsFlow::set(&f.state, "waiting");
                }, &f.cancel, Duration::from_secs(300))
            };
            match res {
                Ok(acc) => {
                    if let Ok(mut s) = settings.lock() {
                        s.insert("last_account".into(), acc["uuid"].clone());
                        let _ = settings::save(&dir, &s);
                    }
                    MsFlow::set(&f.state, "done");
                }
                Err(e) => {
                    MsFlow::set(&f.error, &e);
                    MsFlow::set(&f.state, "error");
                }
            }
        });
        for _ in 0..80 {
            if MsFlow::get(&flow.state) != "starting" {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(flow.public())
    }

    fn play(&self, key: &str) -> ApiResult {
        let r = self.local_ref(key)?;
        let task = {
            let mut tasks = self.run.tasks.lock().map_err(|_| "внутренняя ошибка")?;
            if let Some(t) = tasks.get(key) {
                if matches!(t.phase().as_str(), "preparing" | "running") {
                    return Err("Эта сборка уже запущена".into());
                }
            }
            let acc = self.current_account().ok_or("Сначала добавь аккаунт: Ely.by или Microsoft (значок внизу слева)")?;
            let t = Arc::new(RunTask::new());
            tasks.insert(key.to_string(), t.clone());
            (t, acc)
        };
        let (task, account) = task;
        let s = self.settings.lock().map_err(|_| "внутренняя ошибка")?.clone();
        let ram = r.inst["ram_mb"].as_i64().filter(|x| *x > 0).or(s.get("ram_mb").and_then(|v| v.as_i64())).unwrap_or(4096);
        let spec = crate::launch::RunSpec {
            root: self.dir.clone(),
            ep: self.endpoints.clone(),
            mc: r.mc.clone(),
            loader: r.loader.clone(),
            game_dir: r.gdir.clone(),
            account,
            user_java: s.get("java_path").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            ram_mb: ram,
            java_args: r.inst["java_args"].as_str().unwrap_or("").to_string(),
            ms_client_id: s.get("ms_client_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string(),
        };
        let (dir, run, k, name) = (self.dir.clone(), self.run.clone(), key.to_string(), r.name.clone());
        std::thread::spawn(move || run_worker(dir, run, task, k, name, spec));
        Ok(json!({}))
    }

    /// Идёт ли сейчас игра или подготовка к ней.
    pub fn any_running(&self) -> bool {
        self.run.tasks.lock().map(|t| t.values().any(|x| matches!(x.phase().as_str(), "preparing" | "running"))).unwrap_or(false)
    }

    fn clean_loader(v: &Value) -> Result<Value, ApiError> {
        let kind = v.get("type").and_then(|x| x.as_str()).unwrap_or("vanilla");
        if !LOADERS.contains(&kind) {
            return Err("Неизвестный загрузчик".into());
        }
        let ver = if kind == "vanilla" { "" } else { v.get("version").and_then(|x| x.as_str()).unwrap_or("").trim() };
        if (kind == "forge" || kind == "neoforge") && ver.is_empty() {
            return Err(format!("Для {} нужно выбрать версию загрузчика", crate::instances::loader_title(kind)).into());
        }
        Ok(json!({"type": kind, "version": ver}))
    }

    /// Запросы с картинкой в ответе.
    pub fn call_out(&self, name: &str, p: &Value) -> Result<Out, ApiError> {
        match name {
            "modicon" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(match mods::mod_icon(&r.gdir, pstr(p, "file")?)? {
                    Some(b) => Out::Image(b, "image/png"),
                    None => Out::Missing,
                })
            }
            "insticon" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(match std::fs::read(self.icon_path(&r.id)) {
                    Ok(b) => Out::Image(b, "image/png"),
                    Err(_) => Out::Missing,
                })
            }
            "shot" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(match shots::read(&r.gdir, pstr(p, "file")?)? {
                    Some((b, t)) => Out::Image(b, t),
                    None => Out::Missing,
                })
            }
            "skinimg" => Ok(match crate::skins::read(&self.dir, popt(p, "id")) {
                Some(b) => Out::Image(b, "image/png"),
                None => Out::Missing,
            }),
            "skinurl" => {
                // Текущий скин аккаунта через лаунчер, чтобы страница не ходила на чужие сайты.
                let png = (|| -> Option<Vec<u8>> {
                    let acc = self.ms_account(p["uuid"].as_str()?).ok()?;
                    let prof = crate::skins::profile(&self.endpoints, &acc).ok()?;
                    let url = prof["skin_url"].as_str().filter(|u| !u.is_empty())?;
                    crate::skins::fetch_texture(url).ok()
                })();
                Ok(match png {
                    Some(b) => Out::Image(b, "image/png"),
                    None => Out::Missing,
                })
            }
            _ => self.call(name, p).map(Out::Json),
        }
    }

    pub fn call(&self, name: &str, p: &Value) -> ApiResult {
        match name {
            "state" => {
                let s = self.settings.lock().map_err(|_| "внутренняя ошибка")?;
                let accs: Vec<Value> = accounts::load(&self.dir)
                    .iter()
                    .map(|a| json!({"type": a["type"], "name": a["name"], "uuid": a["uuid"]}))
                    .collect();
                Ok(json!({
                    "version": APP_VERSION, "os": paths::os_name(), "data_dir": self.dir.to_string_lossy(),
                    "settings": self.public_settings(&s), "accounts": accs,
                    "selected": s.get("last_account").cloned().unwrap_or_else(|| json!("")),
                    "instances": self.items(), "packs_error": ""
                }))
            }
            "ping" | "bye" => Ok(json!({"ok": true})),
            "app.update_check" => {
                let found = crate::selfupdate::check(&self.endpoints, APP_VERSION)?;
                let out = match &found {
                    Some(i) => json!({"available": true, "current": APP_VERSION, "latest": i.version, "size": i.size, "notes": i.notes}),
                    None => json!({"available": false, "current": APP_VERSION, "latest": APP_VERSION}),
                };
                *self.self_update.lock().map_err(|_| "внутренняя ошибка")? = found;
                Ok(out)
            }
            "app.update_install" => {
                if self.any_running() {
                    return Err("Сначала закрой игру: лаунчер перезапустится.".into());
                }
                let info = self.self_update.lock().map_err(|_| "внутренняя ошибка")?.clone();
                let info = match info {
                    Some(i) => i,
                    None => crate::selfupdate::check(&self.endpoints, APP_VERSION)?.ok_or("Обновлений нет")?,
                };
                let dest = self.dir.join("update").join(format!("Voxel-{}.exe", info.version));
                crate::selfupdate::download(&info, &dest)?;
                crate::selfupdate::apply(&dest)?;
                crate::applog::log(&self.dir, &format!("Обновление {} → {}", APP_VERSION, info.version));
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(700));
                    std::process::exit(0);
                });
                Ok(json!({"restarting": true, "version": info.version}))
            }
            "poll" => {
                let since = p["notice"].as_u64().unwrap_or(0);
                let mut tasks = Map::new();
                if let Ok(t) = self.run.tasks.lock() {
                    for (k, v) in t.iter() {
                        tasks.insert(k.clone(), json!({
                            "phase": v.phase(), "error": v.error.lock().map(|e| e.clone()).unwrap_or_default(),
                            "code": v.code.lock().ok().and_then(|c| *c), "started": v.started
                        }));
                    }
                }
                let progress = self.run.progress.lock().map(|g| g.clone()).unwrap_or_default();
                let (nid, notes) = match self.run.notices.lock() {
                    Ok(g) => (g.0, g.1.iter().filter(|n| n["id"].as_u64().unwrap_or(0) > since).cloned().collect::<Vec<_>>()),
                    Err(_) => (0, Vec::new()),
                };
                Ok(json!({"tasks": tasks, "progress": progress, "notice": nid, "notices": notes}))
            }
            "play" => self.play(pstr(p, "key")?),
            "stop" => {
                if let Some(t) = self.run.tasks.lock().ok().and_then(|t| t.get(pstr(p, "key").unwrap_or("")).cloned()) {
                    if t.phase() == "running" {
                        t.stop.store(true, Ordering::Relaxed);
                    }
                }
                Ok(json!({}))
            }
            "log" => {
                let t = self.run.tasks.lock().ok().and_then(|t| t.get(pstr(p, "key").unwrap_or("")).cloned());
                match t {
                    None => Ok(json!({"lines": [], "next": 0, "phase": null})),
                    Some(t) => {
                        let (lines, next) = t.read(p["since"].as_u64().unwrap_or(0) as usize);
                        Ok(json!({"lines": lines, "next": next, "phase": t.phase()}))
                    }
                }
            }
            "settings.set" => self.settings_set(p),
            "meta.mc_versions" => {
                let snap = p["snapshots"].as_bool().unwrap_or(false);
                self.cached(&format!("mc:{}", snap), || {
                    let v = mojang::mc_versions(&self.endpoints, snap).map_err(|e| e.0)?;
                    Ok(json!({"versions": v}))
                })
            }
            "meta.loader_versions" => {
                let (kind, mc) = (pstr(p, "kind")?, pstr(p, "mc")?);
                self.cached(&format!("lv:{}:{}", kind, mc), || {
                    let v = loaders::loader_versions(&self.endpoints, kind, mc).map_err(|e| e.0)?;
                    Ok(json!({"versions": v}))
                })
            }
            "accounts.select" => {
                let mut s = self.settings.lock().map_err(|_| "внутренняя ошибка")?;
                s.insert("last_account".into(), json!(pstr(p, "uuid")?));
                settings::save(&self.dir, &s).map_err(|e| e.to_string())?;
                Ok(json!({}))
            }
            "accounts.remove" => {
                accounts::remove(&self.dir, pstr(p, "uuid")?, popt(p, "type")).map_err(|e| e.to_string())?;
                Ok(json!({}))
            }
            "accounts.ely_login" => {
                let acc = crate::auth::ely_login(&self.endpoints, popt(p, "username").trim(), popt(p, "password"), popt(p, "totp").trim())?;
                crate::auth::upsert_account(&self.dir, &acc);
                self.select_account(acc["uuid"].as_str().unwrap_or(""));
                Ok(json!({"name": acc["name"]}))
            }
            "accounts.ms_start" => self.ms_start(popt(p, "mode")),
            "accounts.ms_status" => Ok(self
                .ms_flow
                .lock()
                .ok()
                .and_then(|f| f.as_ref().map(|f| f.public()))
                .unwrap_or_else(|| json!({"state": "none", "code": "", "url": "", "error": ""}))),
            "accounts.ms_cancel" => {
                if let Some(f) = self.ms_flow.lock().ok().and_then(|f| f.clone()) {
                    f.cancel.store(true, Ordering::Relaxed);
                }
                Ok(json!({}))
            }
            "skins.list" => Ok(json!({"skins": crate::skins::list(&self.dir)})),
            "skins.profile" => {
                let acc = self.ms_account(pstr(p, "uuid")?)?;
                Ok(crate::skins::profile(&self.endpoints, &acc)?)
            }
            "skins.apply" => {
                let acc = self.ms_account(pstr(p, "uuid")?)?;
                let it = crate::skins::get(&self.dir, pstr(p, "id")?)?;
                let variant = if popt(p, "variant").is_empty() { it["variant"].as_str().unwrap_or("classic") } else { popt(p, "variant") };
                if Some(variant) != it["variant"].as_str() {
                    crate::skins::update(&self.dir, pstr(p, "id")?, None, Some(variant))?;
                }
                let data = crate::skins::read(&self.dir, pstr(p, "id")?).unwrap_or_default();
                crate::skins::apply(&self.endpoints, &acc, &data, variant)?;
                Ok(crate::skins::profile(&self.endpoints, &acc)?)
            }
            "skins.reset" => {
                let acc = self.ms_account(pstr(p, "uuid")?)?;
                crate::skins::reset(&self.endpoints, &acc)?;
                Ok(crate::skins::profile(&self.endpoints, &acc)?)
            }
            "skins.cape" => {
                let acc = self.ms_account(pstr(p, "uuid")?)?;
                crate::skins::set_cape(&self.endpoints, &acc, popt(p, "cape_id"))?;
                Ok(crate::skins::profile(&self.endpoints, &acc)?)
            }
            "skins.import_current" => {
                let acc = self.ms_account(pstr(p, "uuid")?)?;
                let prof = crate::skins::profile(&self.endpoints, &acc)?;
                let url = prof["skin_url"].as_str().unwrap_or("");
                if url.is_empty() {
                    return Err("У аккаунта стандартный скин, сохранять нечего".into());
                }
                let data = crate::skins::fetch_texture(url)?;
                let item = crate::skins::add(&self.dir, &format!("Мой скин {}", prof["name"].as_str().unwrap_or("")), &data, prof["variant"].as_str().unwrap_or("classic"))?;
                Ok(json!({"skin": item}))
            }
            "skins.lookup" => {
                let (data, variant, name) = crate::skins::lookup(&self.endpoints, popt(p, "nick"))?;
                Ok(json!({"skin": crate::skins::add(&self.dir, &format!("Скин {}", name), &data, &variant)?}))
            }
            "skins.update" => {
                let name = p["name"].as_str();
                let variant = p["variant"].as_str();
                Ok(json!({"skin": crate::skins::update(&self.dir, pstr(p, "id")?, name, variant)?}))
            }
            "skins.delete" => {
                crate::skins::remove(&self.dir, pstr(p, "id")?)?;
                Ok(json!({}))
            }
            "room.status" => Ok(self.room_status()),
            "room.host" => {
                let manual = p["port"].as_u64().filter(|x| (1024..=65535).contains(x)).unwrap_or(0) as u16;
                self.room_port.store(manual, Ordering::Relaxed);
                self.room_leave();
                let (run, port) = (self.run.clone(), self.room_port.clone());
                let target: Arc<dyn Fn() -> Option<u16> + Send + Sync> = Arc::new(move || match port.load(Ordering::Relaxed) {
                    0 => detect_world_port(&run),
                    n => Some(n),
                });
                let act = crate::room::host(target)?;
                *self.room.lock().map_err(|_| "внутренняя ошибка")? = Some(act);
                Ok(self.room_status())
            }
            "room.join" => {
                self.room_leave();
                let act = crate::room::join(popt(p, "code"))?;
                *self.room.lock().map_err(|_| "внутренняя ошибка")? = Some(act);
                Ok(self.room_status())
            }
            "room.set_port" => {
                let manual = p["port"].as_u64().filter(|x| (1024..=65535).contains(x)).unwrap_or(0) as u16;
                self.room_port.store(manual, Ordering::Relaxed);
                Ok(self.room_status())
            }
            "room.leave" => {
                self.room_leave();
                Ok(self.room_status())
            }
            "modrinth.search" | "curseforge.search" => {
                let ptype = if popt(p, "type").is_empty() { "mod" } else { popt(p, "type") };
                let sort = if popt(p, "sort").is_empty() { "relevance" } else { popt(p, "sort") };
                let offset = p["offset"].as_u64().unwrap_or(0);
                Ok(if name == "modrinth.search" {
                    crate::catalog::mr_search(&self.endpoints, popt(p, "query"), ptype, popt(p, "mc"), popt(p, "loader"), sort, offset, 20)?
                } else {
                    crate::catalog::cf_search(&self.endpoints, &self.cf_key(), popt(p, "query"), ptype, popt(p, "mc"), popt(p, "loader"), sort, offset, 20)?
                })
            }
            "modrinth.install" => self.catalog_install(p, false),
            "curseforge.install" => self.catalog_install(p, true),
            "mods.check_updates" => {
                let key = pstr(p, "key")?;
                let r = self.local_ref(key)?;
                let kind = r.loader["type"].as_str().unwrap_or("vanilla");
                if kind == "vanilla" {
                    return Err("У сборки не выбран загрузчик, моды не используются".into());
                }
                let ups = crate::catalog::mr_check_updates(&self.endpoints, &r.gdir, &r.mc, kind)?;
                let out: Vec<Value> = ups.iter().map(|u| json!({"file": u["file"], "version": u["version_number"]})).collect();
                self.updates.lock().map_err(|_| "внутренняя ошибка")?.insert(key.to_string(), ups);
                Ok(json!({"updates": out}))
            }
            "mods.update" => {
                let key = pstr(p, "key")?;
                let r = self.local_ref(key)?;
                let wanted: Vec<String> = p["files"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let pending = self.updates.lock().map_err(|_| "внутренняя ошибка")?.get(key).cloned().unwrap_or_default();
                let (mut done, mut keep) = (0, Vec::new());
                for u in pending {
                    if wanted.iter().any(|w| u["file"].as_str() == Some(w.as_str())) {
                        crate::catalog::mr_apply_update(&r.gdir, &u)?;
                        done += 1;
                    } else {
                        keep.push(u);
                    }
                }
                self.updates.lock().map_err(|_| "внутренняя ошибка")?.insert(key.to_string(), keep);
                Ok(json!({"updated": done}))
            }
            "instances.export" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let path = crate::importer::export_zip(&self.dir, &r.inst, &r.gdir, p["include_worlds"].as_bool().unwrap_or(false))?;
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                Ok(json!({"path": path.to_string_lossy(), "size": size}))
            }
            "instances.create" => self.instances_create(p),
            "instances.update" => self.instances_update(p),
            "instances.delete" => {
                let id = pstr(p, "id")?;
                self.local_ref(&format!("local:{}", id))?;
                self.store.delete(id);
                Ok(json!({}))
            }
            "instances.duplicate" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let wanted = popt(p, "name").trim();
                let new_name = if wanted.is_empty() { format!("{} (копия)", r.name) } else { wanted.to_string() };
                let new = self.store.duplicate(&r.inst, &new_name)?;
                Ok(json!({"instance": self.item_local(&new, &self.stats())}))
            }
            "instances.open_folder" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let sub = popt(p, "sub");
                if !SUBFOLDERS.contains(&sub) {
                    return Err("Неизвестная папка".into());
                }
                open_path(&if sub.is_empty() { r.gdir.clone() } else { r.gdir.join(sub) });
                Ok(json!({}))
            }
            "app.open" => {
                let target: Option<PathBuf> = match popt(p, "what") {
                    "data" => Some(self.dir.clone()),
                    "exports" => Some(self.dir.join("exports")),
                    "site" => {
                        let s = self.settings.lock().map_err(|_| "внутренняя ошибка")?;
                        s.get("site_dir").and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(PathBuf::from)
                    }
                    _ => None,
                };
                open_path(&target.ok_or("Папка не задана")?);
                Ok(json!({}))
            }
            "app.open_url" => {
                let url = pstr(p, "url")?;
                const OK: [&str; 4] = [
                    "https://modrinth.com/",
                    "https://ely.by/",
                    "https://www.curseforge.com/",
                    "https://console.curseforge.com/",
                ];
                let flow_url = self.ms_flow.lock().ok().and_then(|f| f.as_ref().map(|f| MsFlow::get(&f.url))).unwrap_or_default();
                if !(url == flow_url && !flow_url.is_empty()) && !OK.iter().any(|pre| url.starts_with(pre)) {
                    return Err("Этот адрес открывать нельзя".into());
                }
                open_url(url);
                Ok(json!({}))
            }
            "mods.list" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(json!({"mods": mods::list_mods(&r.gdir), "readonly": false, "updates": []}))
            }
            "mods.toggle" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(json!({"file": mods::toggle_mod(&r.gdir, pstr(p, "file")?)?}))
            }
            "mods.delete" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let files: Vec<String> = p["files"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                mods::delete_mods(&r.gdir, &files)?;
                Ok(json!({}))
            }
            "content.installed" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let mut ids: Vec<String> = load_track(&r.gdir)
                    .values()
                    .filter_map(|t| t["project_id"].as_str().filter(|s| !s.is_empty()).map(String::from))
                    .collect();
                ids.sort();
                ids.dedup();
                Ok(json!({"project_ids": ids}))
            }
            "worlds.list" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(json!({"worlds": worlds::list_worlds(&r.gdir)}))
            }
            "worlds.backup" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let path = worlds::backup_world(&r.gdir, pstr(p, "name")?)?;
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                Ok(json!({"path": path.to_string_lossy(), "size": size}))
            }
            "worlds.delete" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                worlds::delete_world(&r.gdir, pstr(p, "name")?)?;
                Ok(json!({}))
            }
            "latest_log" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let path = r.gdir.join("logs").join("latest.log");
                let bytes = std::fs::read(&path).unwrap_or_default();
                let tail = &bytes[bytes.len().saturating_sub(200_000)..];
                let text = String::from_utf8_lossy(tail);
                let lines: Vec<&str> = text.lines().collect();
                let from = lines.len().saturating_sub(600);
                Ok(json!({"lines": lines[from..]}))
            }
            "shots.list" => {
                let stats_refs: Vec<(String, String, PathBuf)> = self
                    .store
                    .list()
                    .iter()
                    .map(|i| {
                        let id = i["id"].as_str().unwrap_or("").to_string();
                        (format!("local:{}", id), i["name"].as_str().unwrap_or("").to_string(), self.store.game_dir(&id))
                    })
                    .collect();
                Ok(json!({"shots": shots::list_all(&stats_refs)}))
            }
            "shots.delete" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                shots::delete(&r.gdir, pstr(p, "file")?)?;
                Ok(json!({}))
            }
            "shots.save_copy" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                let dest = paths::home().join("Pictures").join("Voxel");
                let path = shots::copy_to(&r.gdir, pstr(p, "file")?, &dest)?;
                Ok(json!({"path": path.to_string_lossy()}))
            }
            other => Err(ApiError(format!(
                "Функция «{}» ещё переносится на Rust и появится в следующих этапах.",
                other
            ))),
        }
    }

    fn instances_create(&self, p: &Value) -> ApiResult {
        let name = popt(p, "name").trim();
        if name.is_empty() {
            return Err("Введи название сборки".into());
        }
        let mc = popt(p, "mc_version");
        if mc.is_empty() {
            return Err("Выбери версию Minecraft".into());
        }
        let loader = Self::clean_loader(&p["loader"])?;
        let ram = clean_ram(&p["ram_mb"])?;
        let seed: u32 = rand::random();
        let inst = self.store.save(&json!({
            "name": name, "mc_version": mc, "loader": loader, "ram_mb": ram,
            "java_args": popt(p, "java_args").trim(), "icon_seed": format!("{:08x}", seed)
        }))?;
        Ok(json!({"instance": self.item_local(&inst, &self.stats())}))
    }

    fn instances_update(&self, p: &Value) -> ApiResult {
        let id = pstr(p, "id")?;
        let mut inst = self.store.get(id).ok_or("Сборка не найдена")?;
        let m = inst.as_object_mut().ok_or("Неверные данные сборки")?;
        if let Some(n) = p.get("name") {
            let n = n.as_str().unwrap_or("").trim();
            if n.is_empty() {
                return Err("Название не может быть пустым".into());
            }
            m.insert("name".into(), json!(n));
        }
        if let Some(v) = p.get("mc_version").and_then(|v| v.as_str()).filter(|v| !v.is_empty()) {
            m.insert("mc_version".into(), json!(v));
        }
        if let Some(l) = p.get("loader") {
            m.insert("loader".into(), Self::clean_loader(l)?);
        }
        if let Some(r) = p.get("ram_mb") {
            m.insert("ram_mb".into(), json!(clean_ram(r)?));
        }
        if let Some(v) = p.get("icon") {
            let t = v.as_str().unwrap_or("");
            if !valid_icon(t) {
                return Err("Неверный значок".into());
            }
            if t == "custom" && !self.icon_path(id).is_file() {
                return Err("Своя картинка не загружена".into());
            }
            if t != "custom" {
                let _ = std::fs::remove_file(self.icon_path(id));
            }
            m.insert("icon".into(), json!(t));
        }
        for k in ["java_args", "icon_seed", "notes"] {
            if let Some(v) = p.get(k) {
                let text = v.as_str().unwrap_or("");
                m.insert(k.into(), json!(if k == "icon_seed" { text } else { text.trim() }));
            }
        }
        let saved = self.store.save(&inst)?;
        Ok(json!({"instance": self.item_local(&saved, &self.stats())}))
    }

    /// Принимает файл, загруженный из окна (уже сохранён во временный файл).
    pub fn upload(&self, kind: &str, q: &Value, tmp: &Path) -> ApiResult {
        let name = Path::new(popt(q, "name")).file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let lower = name.to_lowercase();
        match kind {
            "content" => {
                let r = self.local_ref(pstr(q, "key")?)?;
                let folder = if popt(q, "folder").is_empty() { "mods" } else { popt(q, "folder") };
                let ext = if folder == "mods" { ".jar" } else { ".zip" };
                if !lower.ends_with(ext) {
                    return Err(format!("Для этой папки нужен файл {}", ext).into());
                }
                mods::add_content_file(&r.gdir, folder, &name, tmp)?;
                Ok(json!({"name": name}))
            }
            "skin" => {
                if !lower.ends_with(".png") {
                    return Err("Нужен файл скина в формате .png".into());
                }
                let data = std::fs::read(tmp).map_err(|e| e.to_string())?;
                let stem = Path::new(&name).file_stem().and_then(|n| n.to_str()).unwrap_or("Скин");
                let variant = if popt(q, "variant").is_empty() { "classic" } else { popt(q, "variant") };
                Ok(json!({"skin": crate::skins::add(&self.dir, stem, &data, variant)?}))
            }
            "icon" => {
                let r = self.local_ref(pstr(q, "key")?)?;
                let data = std::fs::read(tmp).map_err(|e| e.to_string())?;
                match png_dims(&data) {
                    Some((w, h)) if w > 0 && h > 0 && w <= 1024 && h <= 1024 => {}
                    _ => return Err("Нужна картинка PNG не больше 1024×1024".into()),
                }
                std::fs::write(self.icon_path(&r.id), &data).map_err(|e| format!("Не удалось сохранить значок: {}", e))?;
                let mut inst = self.store.get(&r.id).ok_or("Сборка не найдена")?;
                if let Some(m) = inst.as_object_mut() {
                    m.insert("icon".into(), json!("custom"));
                }
                let saved = self.store.save(&inst)?;
                Ok(json!({"instance": self.item_local(&saved, &self.stats())}))
            }
            "import" => {
                if !lower.ends_with(".mrpack") && !lower.ends_with(".zip") {
                    return Err("Нужен файл .mrpack или .zip".into());
                }
                let mut inst = self.import_file(tmp, "global")?;
                if let Some(m) = inst.as_object_mut() {
                    m.remove("skipped");
                }
                Ok(json!({"instance": self.item_local(&inst, &self.stats())}))
            }
            _ => Err("Неизвестный тип загрузки".into()),
        }
    }

    fn settings_set(&self, p: &Value) -> ApiResult {
        let mut s = self.settings.lock().map_err(|_| "внутренняя ошибка")?;
        let mut new = s.clone();
        for k in settings::PUBLIC {
            if let Some(v) = p.get(k) {
                if k == "ram_mb" {
                    new.insert(k.to_string(), v.clone());
                } else if k == "theme" {
                    new.insert(k.to_string(), settings::clean_theme(v)?);
                } else {
                    let text = match v {
                        Value::String(t) => t.trim().to_string(),
                        other => other.to_string(),
                    };
                    new.insert(k.to_string(), Value::String(text));
                }
            }
        }
        let ram = clean_ram(new.get("ram_mb").unwrap_or(&Value::Null))?;
        new.insert("ram_mb".into(), json!(if ram == 0 { 4096 } else { ram }));
        settings::save(&self.dir, &new).map_err(|e| format!("Не удалось сохранить настройки: {}", e))?;
        *s = new;
        Ok(json!({"settings": self.public_settings(&s)}))
    }
}

/// Память: число МБ, 0 (общая настройка) или от 512 до 65536.
pub fn clean_ram(v: &Value) -> Result<i64, ApiError> {
    let n = match v {
        Value::Null => 0,
        Value::Number(n) => n.as_i64().ok_or("Память — число в мегабайтах")?,
        Value::String(t) if t.trim().is_empty() => 0,
        Value::String(t) => t.trim().parse::<i64>().map_err(|_| "Память — число в мегабайтах")?,
        _ => return Err("Память — число в мегабайтах".into()),
    };
    if n != 0 && !(512..=65536).contains(&n) {
        return Err("Память должна быть от 512 до 65536 МБ (или 0 — общая настройка)".into());
    }
    Ok(n)
}

#[allow(dead_code)]
fn _id_ok(id: &str) -> bool {
    valid_id(id)
}
