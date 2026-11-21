//! Собственные сборки: папка data/local/<id>/{instance.json, minecraft/}. Формат тот же, что у Python-версии.
use crate::util::{copy_dir, read_json, valid_id, write_json};
use rand::Rng;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const LOADERS: [&str; 5] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];
pub const TRACK: &str = ".fl_mods.json";

pub fn loader_title(kind: &str) -> String {
    match kind {
        "vanilla" => "Vanilla",
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        other => other,
    }
    .to_string()
}

pub fn loader_label(loader: &Value) -> String {
    let kind = loader.get("type").and_then(|v| v.as_str()).unwrap_or("vanilla");
    let mut text = loader_title(kind);
    if let Some(ver) = loader.get("version").and_then(|v| v.as_str()) {
        if !ver.is_empty() {
            text.push(' ');
            text.push_str(ver);
        }
    }
    text
}

pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    pub fn local_dir(&self) -> PathBuf {
        self.root.join("local")
    }

    pub fn game_dir(&self, id: &str) -> PathBuf {
        let p = self.local_dir().join(id).join("minecraft");
        let _ = std::fs::create_dir_all(&p);
        p
    }

    pub fn get(&self, id: &str) -> Option<Value> {
        if !valid_id(id) {
            return None;
        }
        let mut v = read_json(&self.local_dir().join(id).join("instance.json"))?;
        v.as_object_mut()?.insert("id".into(), json!(id));
        Some(v)
    }

    /// Все сборки, по алфавиту.
    pub fn list(&self) -> Vec<Value> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.local_dir()) {
            for e in rd.flatten() {
                if let Some(id) = e.file_name().to_str() {
                    if let Some(inst) = self.get(id) {
                        out.push(inst);
                    }
                }
            }
        }
        out.sort_by_key(|i| i["name"].as_str().unwrap_or("").to_lowercase());
        out
    }

    fn slug(name: &str) -> String {
        let mut base = String::new();
        let mut dash = false;
        for c in name.to_lowercase().chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                base.push(c);
                dash = false;
            } else if !dash && !base.is_empty() {
                base.push('-');
                dash = true;
            }
        }
        let base: String = base.trim_matches('-').chars().take(24).collect();
        let base = if base.is_empty() { "pack".to_string() } else { base };
        let tail: u32 = rand::thread_rng().gen_range(0..0x100_0000);
        format!("{}-{:06x}", base, tail)
    }

    /// Сохраняет сборку; если id нет — придумывает. Возвращает сборку с id.
    pub fn save(&self, inst: &Value) -> Result<Value, String> {
        let mut m: Map<String, Value> = inst.as_object().cloned().ok_or("Неверные данные сборки")?;
        let id = match m.get("id").and_then(|v| v.as_str()) {
            Some(i) if !i.is_empty() => i.to_string(),
            _ => Self::slug(m.get("name").and_then(|v| v.as_str()).unwrap_or("pack")),
        };
        if !valid_id(&id) {
            return Err("Неверный идентификатор сборки".into());
        }
        m.remove("id");
        write_json(&self.local_dir().join(&id).join("instance.json"), &Value::Object(m.clone()))
            .map_err(|e| format!("Не удалось сохранить сборку: {}", e))?;
        m.insert("id".into(), json!(id));
        Ok(Value::Object(m))
    }

    pub fn delete(&self, id: &str) {
        if valid_id(id) {
            let _ = std::fs::remove_dir_all(self.local_dir().join(id));
        }
    }

    pub fn duplicate(&self, inst: &Value, new_name: &str) -> Result<Value, String> {
        let mut m = inst.as_object().cloned().ok_or("Неверные данные сборки")?;
        let old_id = m.remove("id").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        m.remove("icon_seed");
        if m.get("icon").and_then(|v| v.as_str()) == Some("custom") {
            m.remove("icon");
        }
        m.remove("publish");
        m.insert("name".into(), json!(new_name));
        let new = self.save(&Value::Object(m))?;
        let new_id = new["id"].as_str().unwrap_or("").to_string();
        let src = self.game_dir(&old_id);
        let dst = self.game_dir(&new_id);
        copy_dir(&src, &dst, &["logs", "crash-reports", "backups"]).map_err(|e| format!("Не удалось скопировать файлы: {}", e))?;
        Ok(new)
    }
}

pub fn count_files(gdir: &Path, folder: &str, exts: &[&str]) -> usize {
    std::fs::read_dir(gdir.join(folder))
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    exts.iter().any(|x| n.ends_with(x))
                })
                .count()
        })
        .unwrap_or(0)
}

/// Учёт скачанного контента (откуда файл), ключи вида "mods/<файл>".
pub fn load_track(gdir: &Path) -> Map<String, Value> {
    read_json(&gdir.join(TRACK)).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

pub fn save_track(gdir: &Path, data: &Map<String, Value>) {
    let _ = write_json(&gdir.join(TRACK), &Value::Object(data.clone()));
}
