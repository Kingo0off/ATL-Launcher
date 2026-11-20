//! Скриншоты всех сборок.
use crate::util::safe_name;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const LIMIT: usize = 3000;

fn is_image(n: &str) -> bool {
    let l = n.to_lowercase();
    l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg")
}

/// refs: (ключ сборки, название, папка игры). От новых к старым.
pub fn list_all(refs: &[(String, String, PathBuf)]) -> Vec<Value> {
    let mut out: Vec<(u64, Value)> = Vec::new();
    for (key, name, gdir) in refs {
        if let Ok(rd) = std::fs::read_dir(gdir.join("screenshots")) {
            for e in rd.flatten() {
                let f = e.file_name().to_string_lossy().to_string();
                if !is_image(&f) {
                    continue;
                }
                if let Ok(md) = e.metadata() {
                    let mt = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                    out.push((mt, json!({"key": key, "inst": name, "file": f, "mtime": mt, "size": md.len()})));
                }
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.truncate(LIMIT);
    out.into_iter().map(|(_, v)| v).collect()
}

fn path(gdir: &Path, file: &str) -> Result<PathBuf, String> {
    safe_name(file).map_err(|_| "Неверное имя файла".to_string())?;
    if !is_image(file) {
        return Err("Неверное имя файла".into());
    }
    Ok(gdir.join("screenshots").join(file))
}

pub fn read(gdir: &Path, file: &str) -> Result<Option<(Vec<u8>, &'static str)>, String> {
    let p = path(gdir, file)?;
    let ctype = if file.to_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    Ok(std::fs::read(p).ok().map(|b| (b, ctype)))
}

pub fn delete(gdir: &Path, file: &str) -> Result<(), String> {
    let p = path(gdir, file)?;
    if p.is_file() {
        std::fs::remove_file(p).map_err(|e| format!("Не удалось удалить: {}", e))?;
    }
    Ok(())
}

/// Копия в папку dest_dir; если файл уже есть, добавляется « (1)».
pub fn copy_to(gdir: &Path, file: &str, dest_dir: &Path) -> Result<PathBuf, String> {
    let p = path(gdir, file)?;
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut dest = dest_dir.join(file);
    let mut n = 1;
    while dest.exists() {
        let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("shot");
        let ext = Path::new(file).extension().and_then(|s| s.to_str()).unwrap_or("png");
        dest = dest_dir.join(format!("{} ({}).{}", stem, n, ext));
        n += 1;
    }
    std::fs::copy(p, &dest).map_err(|e| format!("Не удалось сохранить: {}", e))?;
    Ok(dest)
}
