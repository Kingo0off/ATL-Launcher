use crate::util::{dir_size, mtime_secs, safe_name};
use serde_json::{json, Value};
use std::fs::File;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

pub fn list_worlds(gdir: &Path) -> Vec<Value> {
    let mut out: Vec<(f64, Value)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(gdir.join("saves")) {
        for e in rd.flatten() {
            let p = e.path();
            let lvl = p.join("level.dat");
            if lvl.is_file() {
                let modified = mtime_secs(&lvl);
                out.push((
                    modified,
                    json!({"name": e.file_name().to_string_lossy(), "size": dir_size(&p), "modified": modified}),
                ));
            }
        }
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter().map(|(_, v)| v).collect()
}

fn world_path(gdir: &Path, name: &str) -> Result<PathBuf, String> {
    safe_name(name)?;
    let p = gdir.join("saves").join(name);
    if !p.join("level.dat").is_file() {
        return Err("Мир не найден".into());
    }
    Ok(p)
}

fn add_dir(z: &mut zip::ZipWriter<File>, base: &Path, dir: &Path, prefix: &str, opts: SimpleFileOptions) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        if path.is_dir() {
            add_dir(z, base, &path, prefix, opts)?;
            continue;
        }
        if name == "session.lock" {
            continue;
        }
        let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
        let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        let entry = format!("{}/{}", prefix, parts.join("/"));
        z.start_file(entry, opts).map_err(|e| e.to_string())?;
        let mut f = File::open(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, z).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Резервная копия мира в gdir/backups/<мир>-<дата>.zip. Возвращает путь.
pub fn backup_world(gdir: &Path, name: &str) -> Result<PathBuf, String> {
    let src = world_path(gdir, name)?;
    let dst_dir = gdir.join("backups");
    std::fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dst = dst_dir.join(format!("{}-{}.zip", name, stamp));
    let file = File::create(&dst).map_err(|e| format!("Не удалось создать архив: {}", e))?;
    let mut z = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    if let Err(e) = add_dir(&mut z, &src, &src, name, opts).and_then(|_| z.finish().map(|_| ()).map_err(|e| e.to_string())) {
        let _ = std::fs::remove_file(&dst);
        return Err(format!("Не удалось сделать копию мира: {}", e));
    }
    Ok(dst)
}

pub fn delete_world(gdir: &Path, name: &str) -> Result<(), String> {
    let p = world_path(gdir, name)?;
    std::fs::remove_dir_all(p).map_err(|e| format!("Не удалось удалить мир: {}", e))
}
