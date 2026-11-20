use crate::instances::{load_track, save_track};
use crate::util::safe_name;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use zip::ZipArchive;

#[derive(Clone, Default, Debug)]
pub struct JarMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: String,
    pub icon: Option<String>,
}

type CacheKey = (std::path::PathBuf, u64, u64);

fn cache() -> &'static Mutex<HashMap<CacheKey, JarMeta>> {
    static C: OnceLock<Mutex<HashMap<CacheKey, JarMeta>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn read_text(z: &mut ZipArchive<File>, name: &str) -> Option<String> {
    let f = z.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.take(4 << 20).read_to_end(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf).into_owned();
    Some(s.trim_start_matches('\u{feff}').to_string())
}

/// JSON с допуском переводов строк внутри строк (так пишут некоторые моды).
fn parse_lenient(text: &str) -> Option<Value> {
    serde_json::from_str(text)
        .ok()
        .or_else(|| serde_json::from_str(&text.replace(['\n', '\r', '\t'], " ")).ok())
}

fn pick_icon(icon: &Value) -> Option<String> {
    match icon {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) if !m.is_empty() => {
            let best = m
                .iter()
                .filter_map(|(k, v)| Some((k.parse::<i64>().ok()?, v)))
                .max_by_key(|(k, _)| *k)
                .map(|(_, v)| v);
            best.or_else(|| m.values().next()).and_then(|v| v.as_str().map(String::from))
        }
        _ => None,
    }
}

fn sv(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

fn toml_value(t: &str, key: &str) -> Option<String> {
    for line in t.lines() {
        if let Some(rest) = line.trim_start().strip_prefix(key) {
            if let Some(v) = rest.trim_start().strip_prefix('=') {
                if let Some(q) = v.trim().strip_prefix('"') {
                    if let Some(end) = q.find('"') {
                        return Some(q[..end].to_string());
                    }
                }
            }
        }
    }
    None
}

fn toml_description(t: &str) -> String {
    for (idx, _) in t.match_indices("description") {
        let after = t[idx + "description".len()..].trim_start();
        if let Some(v) = after.strip_prefix('=') {
            let v = v.trim_start();
            if let Some(q) = v.strip_prefix("'''") {
                if let Some(end) = q.find("'''") {
                    return q[..end].trim().to_string();
                }
            } else if let Some(q) = v.strip_prefix('"') {
                if let Some(end) = q.find('"') {
                    return q[..end].trim().to_string();
                }
            }
        }
    }
    String::new()
}

/// Название, версия, описание и иконка из fabric / quilt / forge / neoforge мода.
pub fn read_jar_meta(path: &Path) -> JarMeta {
    let md = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return JarMeta::default(),
    };
    let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let key = (path.to_path_buf(), mt, md.len());
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let meta = read_jar_meta_uncached(path);
    if let Ok(mut c) = cache().lock() {
        if c.len() > 2000 {
            c.clear();
        }
        c.insert(key, meta.clone());
    }
    meta
}

fn read_jar_meta_uncached(path: &Path) -> JarMeta {
    let mut info = JarMeta::default();
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return info,
    };
    let mut z = match ZipArchive::new(file) {
        Ok(z) => z,
        Err(_) => return info,
    };
    let names: HashSet<String> = z.file_names().map(String::from).collect();
    if names.contains("fabric.mod.json") {
        if let Some(d) = read_text(&mut z, "fabric.mod.json").and_then(|t| parse_lenient(&t)) {
            info.name = sv(&d["name"]);
            info.version = sv(&d["version"]);
            info.description = d["description"].as_str().unwrap_or("").to_string();
            info.icon = pick_icon(&d["icon"]);
        }
    } else if names.contains("quilt.mod.json") {
        if let Some(d) = read_text(&mut z, "quilt.mod.json").and_then(|t| parse_lenient(&t)) {
            let q = &d["quilt_loader"];
            let m = &q["metadata"];
            info.name = sv(&m["name"]);
            info.version = sv(&q["version"]);
            info.description = m["description"].as_str().unwrap_or("").to_string();
            info.icon = pick_icon(&m["icon"]);
        }
    } else if let Some(tn) = ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"].iter().find(|n| names.contains(**n)) {
        if let Some(t) = read_text(&mut z, tn) {
            info.name = toml_value(&t, "displayName");
            info.version = toml_value(&t, "version");
            info.description = toml_description(&t);
            info.icon = toml_value(&t, "logoFile");
            if info.version.as_deref().map(|v| v.starts_with("${")).unwrap_or(false) && names.contains("META-INF/MANIFEST.MF") {
                info.version = read_text(&mut z, "META-INF/MANIFEST.MF").and_then(|mf| {
                    mf.lines()
                        .find_map(|l| l.strip_prefix("Implementation-Version:").map(|v| v.trim().to_string()))
                });
            }
        }
    }
    if let Some(icon) = &info.icon {
        if !names.contains(icon.trim_start_matches('/')) {
            info.icon = None;
        }
    }
    info
}

pub fn list_mods(gdir: &Path) -> Vec<Value> {
    let track = load_track(gdir);
    let mut files: Vec<String> = std::fs::read_dir(gdir.join("mods"))
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().map(String::from)).collect())
        .unwrap_or_default();
    files.sort_by_key(|f| f.to_lowercase());
    let mut out = Vec::new();
    for f in files {
        if !(f.ends_with(".jar") || f.ends_with(".jar.disabled")) {
            continue;
        }
        let path = gdir.join("mods").join(&f);
        let enabled = !f.ends_with(".disabled");
        let base = if enabled { f.clone() } else { f[..f.len() - ".disabled".len()].to_string() };
        let meta = read_jar_meta(&path);
        let tr = track.get(&format!("mods/{}", base)).cloned().unwrap_or(Value::Null);
        let name = meta
            .name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| tr["title"].as_str().map(String::from))
            .unwrap_or_else(|| base[..base.len() - 4].to_string());
        let version = meta
            .version
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| tr["version_number"].as_str().map(String::from))
            .unwrap_or_default();
        out.push(json!({
            "file": f, "enabled": enabled, "name": name, "version": version,
            "description": meta.description,
            "size": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            "icon": meta.icon.is_some(),
            "icon_url": tr["icon_url"].as_str().unwrap_or(""),
            "project_id": tr["project_id"].as_str().unwrap_or("")
        }));
    }
    out
}

pub fn mod_icon(gdir: &Path, file: &str) -> Result<Option<Vec<u8>>, String> {
    let path = gdir.join("mods").join(safe_name(file)?);
    let icon = match read_jar_meta(&path).icon {
        Some(i) => i,
        None => return Ok(None),
    };
    let mut z = match File::open(&path).ok().and_then(|f| ZipArchive::new(f).ok()) {
        Some(z) => z,
        None => return Ok(None),
    };
    let entry = match z.by_name(icon.trim_start_matches('/')) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    if entry.size() > 1 << 20 {
        return Ok(None);
    }
    let mut buf = Vec::new();
    entry.take(1 << 20).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(Some(buf))
}

pub fn toggle_mod(gdir: &Path, file: &str) -> Result<String, String> {
    safe_name(file)?;
    let dir = gdir.join("mods");
    let new_name = if let Some(base) = file.strip_suffix(".disabled") {
        base.to_string()
    } else if file.ends_with(".jar") {
        format!("{}.disabled", file)
    } else {
        return Err("Это не мод".into());
    };
    std::fs::rename(dir.join(file), dir.join(&new_name)).map_err(|e| format!("Не удалось переименовать: {}", e))?;
    Ok(new_name)
}

pub fn delete_mods(gdir: &Path, files: &[String]) -> Result<(), String> {
    let mut track = load_track(gdir);
    for f in files {
        safe_name(f)?;
        let p = gdir.join("mods").join(f);
        if p.is_file() {
            std::fs::remove_file(&p).map_err(|e| format!("Не удалось удалить {}: {}", f, e))?;
        }
        track.shift_remove(&format!("mods/{}", f.strip_suffix(".disabled").unwrap_or(f)));
    }
    save_track(gdir, &track);
    Ok(())
}

pub const CONTENT_DIRS: [&str; 4] = ["mods", "resourcepacks", "shaderpacks", "datapacks"];

pub fn add_content_file(gdir: &Path, folder: &str, name: &str, src: &Path) -> Result<(), String> {
    if !CONTENT_DIRS.contains(&folder) {
        return Err("Неизвестная папка".into());
    }
    safe_name(name)?;
    let dir = gdir.join(folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dst = dir.join(name);
    if std::fs::rename(src, &dst).is_err() {
        std::fs::copy(src, &dst).map_err(|e| format!("Не удалось сохранить файл: {}", e))?;
        let _ = std::fs::remove_file(src);
    }
    Ok(())
}
