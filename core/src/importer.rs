//! Импорт сборок (.mrpack, CurseForge, экспорт ATL Launcher, Prism/MultiMC) и экспорт в .zip.
use crate::catalog::{cf_dir_for, cf_lookup_files, cf_lookup_mods, cf_sha1, CF_PREFIX};
use crate::instances::{save_track, Store};
use crate::net::{download_many, Endpoints, Progress, Task};
use crate::util::safe_join;
use serde_json::{json, Map, Value};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// По спецификации .mrpack лаунчер обязан качать только с этих хостов.
pub const TRUSTED_HOSTS: [&str; 4] = ["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"];

type Zip = zip::ZipArchive<File>;

fn jerr(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Распаковывает записи архива с префиксом в dest (с защитой от «../»).
pub fn extract_prefix(z: &mut Zip, prefix: &str, dest: &Path) -> Result<(), String> {
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(jerr)?;
        let name = e.name().to_string();
        if e.is_dir() || !name.starts_with(prefix) {
            continue;
        }
        let rel = &name[prefix.len()..];
        if rel.is_empty() {
            continue;
        }
        let target = safe_join(dest, rel)?;
        if let Some(d) = target.parent() {
            std::fs::create_dir_all(d).map_err(jerr)?;
        }
        let mut out = File::create(&target).map_err(jerr)?;
        std::io::copy(&mut e, &mut out).map_err(jerr)?;
    }
    Ok(())
}

fn read_text(z: &mut Zip, name: &str) -> Result<String, String> {
    let mut e = z.by_name(name).map_err(|_| format!("В архиве нет {}", name))?;
    let mut s = String::new();
    e.read_to_string(&mut s).map_err(jerr)?;
    Ok(s.trim_start_matches('\u{feff}').to_string())
}

fn has(z: &mut Zip, name: &str) -> bool {
    z.by_name(name).is_ok()
}

fn stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("Сборка").to_string()
}

/// Удаляет сборку, если импорт не удался.
fn rollback<T>(store: &Store, inst: &Value, r: Result<T, String>) -> Result<T, String> {
    if r.is_err() {
        store.delete(inst["id"].as_str().unwrap_or(""));
    }
    r
}

pub fn import_archive(store: &Store, ep: &Endpoints, cf_key: &str, path: &Path, progress: Progress, log: &(dyn Fn(&str) + Sync)) -> Result<Value, String> {
    let f = File::open(path).map_err(jerr)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Файл повреждён или не является архивом: {}", e))?;
    if has(&mut z, "modrinth.index.json") {
        return import_mrpack(store, ep, &mut z, path, progress, log);
    }
    if has(&mut z, "manifest.json") && !has(&mut z, "instance.json") {
        return import_curseforge(store, ep, cf_key, &mut z, path, progress, log);
    }
    if has(&mut z, "instance.json") {
        let mut meta: Value = serde_json::from_str(&read_text(&mut z, "instance.json")?).map_err(|e| format!("instance.json повреждён: {}", e))?;
        if let Some(m) = meta.as_object_mut() {
            m.remove("id");
            if !m.contains_key("name") {
                m.insert("name".into(), json!(stem(path)));
            }
        }
        let inst = store.save(&meta)?;
        let r = extract_prefix(&mut z, "minecraft/", &store.game_dir(inst["id"].as_str().unwrap_or("")));
        rollback(store, &inst, r)?;
        return Ok(inst);
    }
    // Prism / MultiMC: mmc-pack.json + instance.cfg + .minecraft/
    let names: Vec<String> = z.file_names().map(String::from).collect();
    if let Some(root) = names.iter().find(|n| n.ends_with("mmc-pack.json")).map(|n| n[..n.len() - "mmc-pack.json".len()].to_string()) {
        let pack: Value = serde_json::from_str(&read_text(&mut z, &format!("{}mmc-pack.json", root))?).map_err(jerr)?;
        let (mut mc, mut loader) = (None::<String>, json!({"type": "vanilla", "version": ""}));
        for c in pack["components"].as_array().cloned().unwrap_or_default() {
            let kind = match c["uid"].as_str().unwrap_or("") {
                "net.minecraft" => {
                    mc = c["version"].as_str().map(String::from);
                    continue;
                }
                "net.fabricmc.fabric-loader" => "fabric",
                "org.quiltmc.quilt-loader" => "quilt",
                "net.minecraftforge" => "forge",
                "net.neoforged" => "neoforge",
                _ => continue,
            };
            loader = json!({"type": kind, "version": c["version"].as_str().unwrap_or("")});
        }
        let mc = mc.ok_or("В mmc-pack.json не найдена версия Minecraft")?;
        let mut name = stem(path);
        if let Ok(cfg) = read_text(&mut z, &format!("{}instance.cfg", root)) {
            if let Some(l) = cfg.lines().find_map(|l| l.strip_prefix("name=")) {
                if !l.trim().is_empty() {
                    name = l.trim().to_string();
                }
            }
        }
        let inst = store.save(&json!({"name": name, "mc_version": mc, "loader": loader}))?;
        let gdir = store.game_dir(inst["id"].as_str().unwrap_or(""));
        let r = (|| -> Result<(), String> {
            for pre in [format!("{}.minecraft/", root), format!("{}minecraft/", root)] {
                extract_prefix(&mut z, &pre, &gdir)?;
            }
            Ok(())
        })();
        rollback(store, &inst, r)?;
        return Ok(inst);
    }
    Err("Не удалось распознать архив. Поддерживаются .mrpack, сборки CurseForge, экспорт ATL Launcher и zip из Prism/MultiMC.".into())
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("").split(':').next().unwrap_or("").to_string()
}

fn import_mrpack(store: &Store, ep: &Endpoints, z: &mut Zip, path: &Path, progress: Progress, log: &(dyn Fn(&str) + Sync)) -> Result<Value, String> {
    let idx: Value = serde_json::from_str(&read_text(z, "modrinth.index.json")?).map_err(|e| format!("modrinth.index.json повреждён: {}", e))?;
    if idx["game"].as_str().unwrap_or("minecraft") != "minecraft" {
        return Err("Это сборка не для Minecraft".into());
    }
    let deps = &idx["dependencies"];
    let mc = deps["minecraft"].as_str().ok_or("В сборке не указана версия Minecraft")?.to_string();
    let mut loader = json!({"type": "vanilla", "version": ""});
    for (key, kind) in [("fabric-loader", "fabric"), ("quilt-loader", "quilt"), ("neoforge", "neoforge"), ("forge", "forge")] {
        if let Some(v) = deps[key].as_str() {
            loader = json!({"type": kind, "version": v});
            break;
        }
    }
    let files: Vec<Value> = idx["files"].as_array().cloned().unwrap_or_default().into_iter().filter(|f| f["env"]["client"] != "unsupported").collect();
    for f in &files {
        let url = f["downloads"][0].as_str().unwrap_or("");
        let host = host_of(url);
        if !TRUSTED_HOSTS.contains(&host.as_str()) && !ep.extra_trusted_hosts.contains(&host) {
            return Err(format!("Сборка ссылается на недоверенный сайт ({}). Импорт остановлен.", if host.is_empty() { "пусто" } else { &host }));
        }
    }
    let name = idx["name"].as_str().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| stem(path));
    let inst = store.save(&json!({"name": name, "mc_version": mc, "loader": loader, "ram_mb": 0, "java_args": "", "notes": idx["summary"].as_str().unwrap_or("")}))?;
    let gdir = store.game_dir(inst["id"].as_str().unwrap_or(""));
    let r = (|| -> Result<(), String> {
        let mut tasks = Vec::new();
        for f in &files {
            tasks.push(Task {
                url: f["downloads"][0].as_str().unwrap_or("").to_string(),
                dest: safe_join(&gdir, f["path"].as_str().unwrap_or(""))?,
                sha1: f["hashes"]["sha1"].as_str().map(String::from),
                size: None,
            });
        }
        log(&format!("Скачиваю файлов сборки: {}", tasks.len()));
        download_many(&tasks, progress, "Файлы сборки", 6).map_err(|e| e.0)?;
        extract_prefix(z, "overrides/", &gdir)?;
        extract_prefix(z, "client-overrides/", &gdir)
    })();
    rollback(store, &inst, r)?;
    Ok(inst)
}

fn import_curseforge(store: &Store, ep: &Endpoints, key: &str, z: &mut Zip, path: &Path, progress: Progress, log: &(dyn Fn(&str) + Sync)) -> Result<Value, String> {
    let man: Value = serde_json::from_str(&read_text(z, "manifest.json")?).map_err(|e| format!("manifest.json повреждён: {}", e))?;
    let mc = man["minecraft"]["version"].as_str().ok_or("В сборке CurseForge не указана версия Minecraft")?.to_string();
    let mut loader = json!({"type": "vanilla", "version": ""});
    let mls = man["minecraft"]["modLoaders"].as_array().cloned().unwrap_or_default();
    if let Some(ml) = mls.iter().find(|m| m["primary"] == true).or(mls.first()) {
        let id = ml["id"].as_str().unwrap_or("");
        for kind in ["neoforge", "forge", "fabric", "quilt"] {
            if let Some(v) = id.strip_prefix(&format!("{}-", kind)) {
                loader = json!({"type": kind, "version": v});
                break;
            }
        }
    }
    let entries: Vec<Value> = man["files"].as_array().cloned().unwrap_or_default().into_iter().filter(|f| f["required"] != false).collect();
    let mut ids: Vec<i64> = entries.iter().filter_map(|f| f["fileID"].as_i64()).collect();
    ids.sort();
    ids.dedup();
    let mut mod_ids: Vec<i64> = entries.iter().filter_map(|f| f["projectID"].as_i64()).collect();
    mod_ids.sort();
    mod_ids.dedup();
    let by_file = cf_lookup_files(ep, key, &ids)?;
    let by_mod = cf_lookup_mods(ep, key, &mod_ids)?;
    let author = man["author"].as_str().filter(|a| !a.is_empty()).map(|a| format!("Автор: {}", a)).unwrap_or_default();
    let name = man["name"].as_str().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| stem(path));
    let mut inst = store.save(&json!({"name": name, "mc_version": mc, "loader": loader, "ram_mb": 0, "java_args": "", "notes": author}))?;
    let gdir = store.game_dir(inst["id"].as_str().unwrap_or(""));
    let mut skipped: Vec<Value> = Vec::new();
    let r = (|| -> Result<(), String> {
        let (mut tasks, mut track) = (Vec::new(), Map::new());
        for e in &entries {
            let f = by_file.get(&e["fileID"].to_string());
            let m = by_mod.get(&e["projectID"].to_string()).cloned().unwrap_or_else(|| json!({}));
            let title = m["name"].as_str().map(String::from).unwrap_or_else(|| e["projectID"].to_string());
            let url = f.and_then(|f| f["downloadUrl"].as_str()).filter(|u| !u.is_empty());
            let (f, url) = match (f, url) {
                (Some(f), Some(u)) => (f, u),
                _ => {
                    skipped.push(json!({"title": title, "url": m["links"]["websiteUrl"].as_str().unwrap_or("")}));
                    continue;
                }
            };
            let folder = cf_dir_for(m["classId"].as_i64().unwrap_or(6));
            let fname = crate::util::safe_name(f["fileName"].as_str().unwrap_or("").rsplit('/').next().unwrap_or(""))?.to_string();
            tasks.push(Task { url: url.to_string(), dest: gdir.join(folder).join(&fname), sha1: cf_sha1(f), size: None });
            track.insert(
                format!("{}/{}", folder, fname),
                json!({
                    "project_id": format!("{}{}", CF_PREFIX, e["projectID"]), "source": "curseforge", "version_id": f["id"].to_string(),
                    "title": title, "icon_url": m["logo"]["thumbnailUrl"].as_str().unwrap_or(""), "sha1": cf_sha1(f)
                }),
            );
        }
        log(&format!("Скачиваю файлов сборки: {}", tasks.len()));
        download_many(&tasks, progress, "Файлы сборки", 6).map_err(|e| e.0)?;
        let over = man["overrides"].as_str().filter(|o| !o.is_empty()).unwrap_or("overrides").trim_matches('/').to_string();
        extract_prefix(z, &format!("{}/", over), &gdir)?;
        save_track(&gdir, &track);
        Ok(())
    })();
    rollback(store, &inst, r)?;
    if !skipped.is_empty() {
        log(&format!("Не скачано из-за запрета авторов: {}", skipped.len()));
        inst["skipped"] = Value::Array(skipped);
    }
    Ok(inst)
}

// ---------- экспорт ----------

const EXPORT_SKIP: [&str; 7] = ["logs", "crash-reports", "backups", ".fl_", "usercache", "usernamecache", "launcher_profiles"];

fn skipped_top(name: &str) -> bool {
    EXPORT_SKIP.iter().any(|s| name.starts_with(s))
}

fn walk(dir: &Path, rel: &str, include_worlds: bool, out: &mut Vec<(PathBuf, String)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let relp = if rel.is_empty() { name.clone() } else { format!("{}/{}", rel, name) };
        let p = e.path();
        if rel.is_empty() && skipped_top(&name) {
            continue;
        }
        if p.is_dir() {
            if rel.is_empty() && name == "saves" && !include_worlds {
                continue;
            }
            walk(&p, &relp, include_worlds, out);
        } else if p.is_file() {
            out.push((p, relp));
        }
    }
}

pub fn export_zip(root: &Path, inst: &Value, gdir: &Path, include_worlds: bool) -> Result<PathBuf, String> {
    let dir = root.join("exports");
    std::fs::create_dir_all(&dir).map_err(jerr)?;
    let raw = inst["name"].as_str().unwrap_or("instance");
    let safe: String = raw.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let safe = if safe.trim().is_empty() { "instance".to_string() } else { safe.trim().to_string() };
    let dst = dir.join(format!("{}-{}.zip", safe, chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let mut meta = inst.as_object().cloned().unwrap_or_default();
    meta.remove("id");
    meta.remove("publish");
    let f = File::create(&dst).map_err(jerr)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    z.start_file("instance.json", opts).map_err(jerr)?;
    z.write_all(serde_json::to_string_pretty(&Value::Object(meta)).map_err(jerr)?.as_bytes()).map_err(jerr)?;
    let mut files = Vec::new();
    walk(gdir, "", include_worlds, &mut files);
    for (p, rel) in files {
        z.start_file(format!("minecraft/{}", rel), opts).map_err(jerr)?;
        let mut src = File::open(&p).map_err(jerr)?;
        std::io::copy(&mut src, &mut z).map_err(jerr)?;
    }
    z.finish().map_err(jerr)?;
    Ok(dst)
}
