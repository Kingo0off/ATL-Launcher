use crate::{paths, settings, APP_VERSION};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Mutex;

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

/// Методы, которые можно вызывать запросом GET (остальные только POST).
pub const GET_OK: [&str; 6] = ["state", "poll", "log", "modicon", "ping", "latest_log"];

pub struct Api {
    pub dir: PathBuf,
    settings: Mutex<Map<String, Value>>,
}

impl Api {
    pub fn new(dir: PathBuf) -> Api {
        let s = settings::load(&dir);
        Api { dir, settings: Mutex::new(s) }
    }

    fn public_settings(&self, s: &Map<String, Value>) -> Value {
        let mut out = Map::new();
        for k in settings::PUBLIC {
            out.insert(k.to_string(), s.get(k).cloned().unwrap_or_else(|| json!("")));
        }
        Value::Object(out)
    }

    pub fn call(&self, name: &str, p: &Value) -> ApiResult {
        match name {
            "state" => {
                let s = self.settings.lock().map_err(|_| "внутренняя ошибка")?;
                Ok(json!({
                    "version": APP_VERSION,
                    "os": paths::os_name(),
                    "data_dir": self.dir.to_string_lossy(),
                    "settings": self.public_settings(&s),
                    "accounts": [],
                    "selected": s.get("last_account").cloned().unwrap_or_else(|| json!("")),
                    "instances": [],
                    "packs_error": ""
                }))
            }
            "ping" | "bye" => Ok(json!({"ok": true})),
            "poll" => Ok(json!({"tasks": {}, "progress": {}, "notice": 0, "notices": []})),
            "settings.set" => self.settings_set(p),
            other => Err(ApiError(format!(
                "Функция «{}» ещё переносится на Rust и появится в следующих этапах.",
                other
            ))),
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
