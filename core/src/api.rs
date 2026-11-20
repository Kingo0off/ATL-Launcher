use crate::instances::{count_files, load_track, loader_label, Store, LOADERS};
use crate::util::{open_path, open_url, read_json, valid_id};
use crate::net::Endpoints;
use crate::{accounts, loaders, mods, mojang, paths, settings, shots, worlds, APP_VERSION};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
pub const GET_OK: [&str; 9] = ["state", "poll", "log", "modicon", "ping", "latest_log", "shot", "skinimg", "skinurl"];
pub const SUBFOLDERS: [&str; 9] = ["", "mods", "saves", "logs", "resourcepacks", "shaderpacks", "screenshots", "backups", "config"];
pub const MAX_UPLOAD: [(&str, u64); 3] = [("import", 4 << 30), ("content", 1 << 29), ("skin", 1 << 20)];

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
    settings: Mutex<Map<String, Value>>,
    pub endpoints: Endpoints,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
}

impl Api {
    pub fn new(dir: PathBuf) -> Api {
        let s = settings::load(&dir);
        let _ = std::fs::create_dir_all(dir.join("tmp"));
        Api { store: Store::new(dir.clone()), dir, settings: Mutex::new(s), endpoints: Endpoints::default(), cache: Mutex::new(HashMap::new()) }
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
        json!({
            "key": key, "kind": "local", "id": id, "name": inst["name"], "mc_version": inst["mc_version"],
            "loader_label": loader_label(&loader), "loader": loader,
            "icon_seed": seed, "ram_mb": inst["ram_mb"].as_i64().unwrap_or(0),
            "java_args": inst["java_args"].as_str().unwrap_or(""), "notes": inst["notes"].as_str().unwrap_or(""),
            "last_played": st["last_played"].as_i64().unwrap_or(0), "playtime_s": st["playtime_s"].as_i64().unwrap_or(0),
            "mods": count_files(&gdir, "mods", &[".jar", ".jar.disabled"]),
            "phase": Value::Null
        })
    }

    fn items(&self) -> Vec<Value> {
        let stats = self.stats();
        self.store.list().iter().map(|i| self.item_local(i, &stats)).collect()
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
            "shot" => {
                let r = self.local_ref(pstr(p, "key")?)?;
                Ok(match shots::read(&r.gdir, pstr(p, "file")?)? {
                    Some((b, t)) => Out::Image(b, t),
                    None => Out::Missing,
                })
            }
            "skinimg" | "skinurl" => Ok(Out::Missing),
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
            "poll" => Ok(json!({"tasks": {}, "progress": {}, "notice": 0, "notices": []})),
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
                if !OK.iter().any(|pre| url.starts_with(pre)) {
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
                let dest = paths::home().join("Pictures").join("FriendLauncher");
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
            "import" | "skin" => Err("Эта загрузка появится на одном из следующих этапов переноса на Rust.".into()),
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
