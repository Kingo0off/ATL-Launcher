//! Ванильный Minecraft: список версий, json версии, библиотеки, ассеты, нативы, аргументы запуска.
use crate::net::{download, download_many, get_json, Endpoints, NetError, Progress, Task};
use crate::paths::os_name;
use crate::util::safe_join;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_pointer_width = "64") {
        "x86_64"
    } else {
        "x86"
    }
}

pub fn classpath_sep() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

pub struct Mojang {
    pub root: PathBuf,
    pub ep: Endpoints,
}

pub struct GameFiles {
    pub version: Value,
    pub classpath: String,
    pub natives_dir: PathBuf,
    pub jar: PathBuf,
    pub game_assets: PathBuf,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

pub fn mc_versions(ep: &Endpoints, snapshots: bool) -> Result<Vec<String>, NetError> {
    let data = get_json(&ep.manifest)?;
    Ok(data["versions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|v| snapshots || v["type"] == "release")
                .filter_map(|v| v["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

fn lib_key(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = base.split(':').collect();
    let mut key = vec![parts.first().copied().unwrap_or(""), parts.get(1).copied().unwrap_or("")];
    if let Some(c) = parts.get(3) {
        key.push(c);
    }
    key.join(":")
}

/// Объединяет версию с родительской (inheritsFrom): библиотеки и аргументы складываются.
pub fn merge(parent: &Value, child: &Value) -> Value {
    let mut r: Map<String, Value> = parent.as_object().cloned().unwrap_or_default();
    if let Some(c) = child.as_object() {
        for (k, v) in c {
            if !["libraries", "arguments", "inheritsFrom"].contains(&k.as_str()) {
                r.insert(k.clone(), v.clone());
            }
        }
    }
    let child_libs: Vec<Value> = child["libraries"].as_array().cloned().unwrap_or_default();
    let seen: HashSet<String> = child_libs.iter().map(|l| lib_key(s(l, "name"))).collect();
    let mut libs = child_libs;
    for l in parent["libraries"].as_array().cloned().unwrap_or_default() {
        if !seen.contains(&lib_key(s(&l, "name"))) {
            libs.push(l);
        }
    }
    r.insert("libraries".into(), Value::Array(libs));
    if parent.get("arguments").is_some() || child.get("arguments").is_some() {
        let cat = |k: &str| -> Value {
            let mut a = parent["arguments"][k].as_array().cloned().unwrap_or_default();
            a.extend(child["arguments"][k].as_array().cloned().unwrap_or_default());
            Value::Array(a)
        };
        r.insert("arguments".into(), json!({"game": cat("game"), "jvm": cat("jvm")}));
    }
    r.insert("_jar_id".into(), parent["_jar_id"].clone());
    Value::Object(r)
}

impl Mojang {
    pub fn new(root: PathBuf, ep: Endpoints) -> Mojang {
        Mojang { root, ep }
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn libs(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn natives(&self) -> PathBuf {
        self.root.join("natives")
    }

    /// Скачивает json версии из манифеста Mojang, если его ещё нет.
    pub fn install_vanilla_json(&self, mc: &str) -> Result<(), NetError> {
        crate::util::safe_name(mc).map_err(NetError)?;
        let path = self.versions().join(mc).join(format!("{}.json", mc));
        if path.is_file() {
            return Ok(());
        }
        let manifest = get_json(&self.ep.manifest)?;
        for v in manifest["versions"].as_array().cloned().unwrap_or_default() {
            if v["id"] == mc {
                download(s(&v, "url"), &path, v["sha1"].as_str(), None, 3)?;
                return Ok(());
            }
        }
        Err(NetError(format!("Версия Minecraft {} не найдена в манифесте Mojang", mc)))
    }

    pub fn load_version(&self, vid: &str) -> Result<Value, String> {
        self.load_version_depth(vid, 0)
    }

    fn load_version_depth(&self, vid: &str, depth: u32) -> Result<Value, String> {
        if depth > 8 {
            return Err("Слишком длинная цепочка inheritsFrom".into());
        }
        crate::util::safe_name(vid)?;
        let path = self.versions().join(vid).join(format!("{}.json", vid));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("Не удалось прочитать версию {}: {}", vid, e))?;
        let mut v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("Файл версии {} повреждён: {}", vid, e))?;
        if let Some(parent) = v["inheritsFrom"].as_str().map(String::from) {
            let p = self.load_version_depth(&parent, depth + 1)?;
            return Ok(merge(&p, &v));
        }
        let jar = v["jar"].as_str().or_else(|| v["id"].as_str()).unwrap_or(vid).to_string();
        v["_jar_id"] = json!(jar);
        Ok(v)
    }

    /// (задачи скачивания, classpath, нативные архивы с исключениями)
    #[allow(clippy::type_complexity)]
    pub fn collect_libraries(&self, v: &Value) -> Result<(Vec<Task>, Vec<PathBuf>, Vec<(PathBuf, Vec<String>)>), String> {
        let (mut tasks, mut cp, mut natives) = (Vec::new(), Vec::new(), Vec::new());
        for lib in v["libraries"].as_array().cloned().unwrap_or_default() {
            if !rules_ok(&lib["rules"]) {
                continue;
            }
            // (путь, url, sha1, размер, нативная?)
            let mut arts: Vec<(String, String, Option<String>, Option<u64>, bool)> = Vec::new();
            if lib["downloads"].is_object() {
                let dl = &lib["downloads"];
                let a = &dl["artifact"];
                if let Some(path) = a["path"].as_str().filter(|p| !p.is_empty()) {
                    arts.push((path.into(), s(a, "url").into(), a["sha1"].as_str().map(String::from), a["size"].as_u64(), false));
                }
                if let Some(cls) = lib["natives"][os_name()].as_str() {
                    let cls = cls.replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" });
                    let n = &dl["classifiers"][cls.as_str()];
                    if let Some(path) = n["path"].as_str() {
                        arts.push((path.into(), s(n, "url").into(), n["sha1"].as_str().map(String::from), n["size"].as_u64(), true));
                    }
                }
            } else {
                let path = maven_path(s(&lib, "name"));
                let base = lib["url"].as_str().unwrap_or("https://libraries.minecraft.net/").trim_end_matches('/').to_string();
                arts.push((path.clone(), format!("{}/{}", base, path), lib["sha1"].as_str().map(String::from), lib["size"].as_u64(), false));
            }
            for (path, url, sha1, size, is_native) in arts {
                let dest = safe_join(&self.libs(), &path)?;
                if !url.is_empty() {
                    tasks.push(Task { url, dest: dest.clone(), sha1, size });
                }
                if is_native {
                    let excl: Vec<String> = lib["extract"]["exclude"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    natives.push((dest, excl));
                } else if !cp.contains(&dest) {
                    cp.push(dest);
                }
            }
        }
        Ok((tasks, cp, natives))
    }

    /// Распаковывает нативные библиотеки в natives/<версия>.
    pub fn extract_natives(&self, natives: &[(PathBuf, Vec<String>)], vid: &str) -> Result<PathBuf, String> {
        crate::util::safe_name(vid)?;
        let out = self.natives().join(vid);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        for (jar, exclude) in natives {
            let f = std::fs::File::open(jar).map_err(|e| format!("Нет файла {}: {}", jar.display(), e))?;
            let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Повреждён архив {}: {}", jar.display(), e))?;
            for i in 0..z.len() {
                let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
                let name = entry.name().to_string();
                if entry.is_dir() || name.starts_with("META-INF") || exclude.iter().any(|e| name.starts_with(e.as_str())) {
                    continue;
                }
                let rel = match entry.enclosed_name() {
                    Some(p) => p,
                    None => continue, // путь пытается выйти за папку
                };
                let target = out.join(rel);
                if let Some(dir) = target.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let mut dst = std::fs::File::create(&target).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut dst).map_err(|e| e.to_string())?;
            }
        }
        Ok(out)
    }

    pub fn client_task(&self, v: &Value) -> Result<(Task, PathBuf), String> {
        let d = &v["downloads"]["client"];
        let id = s(v, "_jar_id");
        crate::util::safe_name(id)?;
        let jar = self.versions().join(id).join(format!("{}.jar", id));
        Ok((
            Task { url: s(d, "url").into(), dest: jar.clone(), sha1: d["sha1"].as_str().map(String::from), size: d["size"].as_u64() },
            jar,
        ))
    }

    pub fn asset_tasks(&self, v: &Value) -> Result<(Vec<Task>, Value), String> {
        let ai = &v["assetIndex"];
        let id = s(ai, "id");
        crate::util::safe_name(id)?;
        let idx_path = self.assets().join("indexes").join(format!("{}.json", id));
        download(s(ai, "url"), &idx_path, ai["sha1"].as_str(), None, 3).map_err(|e| e.0)?;
        let text = std::fs::read_to_string(&idx_path).map_err(|e| e.to_string())?;
        let idx: Value = serde_json::from_str(&text).map_err(|e| format!("Индекс ресурсов повреждён: {}", e))?;
        let mut tasks = Vec::new();
        if let Some(objs) = idx["objects"].as_object() {
            for o in objs.values() {
                let h = s(o, "hash");
                if h.len() < 2 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err(format!("Неверный хеш в индексе ресурсов: {}", h));
                }
                tasks.push(Task {
                    url: format!("{}/{}/{}", self.ep.asset, &h[..2], h),
                    dest: self.assets().join("objects").join(&h[..2]).join(h),
                    sha1: None,
                    size: o["size"].as_u64(),
                });
            }
        }
        Ok((tasks, idx))
    }

    /// Для очень старых версий (≤1.7) ресурсы нужны по обычным путям. Возвращает корень ассетов.
    pub fn finish_legacy_assets(&self, v: &Value, idx: &Value, game_dir: &Path) -> Result<PathBuf, String> {
        let target = if idx["map_to_resources"].as_bool().unwrap_or(false) {
            Some(game_dir.join("resources"))
        } else if idx["virtual"].as_bool().unwrap_or(false) {
            Some(self.assets().join("virtual").join(s(&v["assetIndex"], "id")))
        } else {
            None
        };
        let target = match target {
            Some(t) => t,
            None => return Ok(self.assets()),
        };
        for (name, o) in idx["objects"].as_object().cloned().unwrap_or_default() {
            let h = s(&o, "hash");
            let src = self.assets().join("objects").join(&h[..2.min(h.len())]).join(h);
            let dst = safe_join(&target, &name)?;
            if !dst.exists() {
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            }
        }
        Ok(target)
    }

    /// Докачивает клиент, библиотеки и ассеты; распаковывает нативы.
    pub fn install_game_files(&self, vid: &str, game_dir: &Path, progress: Progress) -> Result<GameFiles, String> {
        let v = self.load_version(vid)?;
        let (lib_tasks, mut cp, natives) = self.collect_libraries(&v)?;
        let (ctask, jar) = self.client_task(&v)?;
        let mut first = vec![ctask];
        first.extend(lib_tasks);
        download_many(&first, progress, "Библиотеки", 8).map_err(|e| e.0)?;
        let (a_tasks, idx) = self.asset_tasks(&v)?;
        download_many(&a_tasks, progress, "Ресурсы игры", 8).map_err(|e| e.0)?;
        let game_assets = self.finish_legacy_assets(&v, &idx, game_dir)?;
        let natives_dir = self.extract_natives(&natives, vid)?;
        cp.push(jar.clone());
        let classpath = cp.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(classpath_sep());
        Ok(GameFiles { version: v, classpath, natives_dir, jar, game_assets })
    }
}

// ---------- правила и аргументы ----------

fn rule_match(rule: &Value) -> bool {
    if rule.get("features").is_some() {
        return false; // демо-режим и своё разрешение мы не включаем
    }
    let o = &rule["os"];
    if o.is_object() {
        if let Some(n) = o["name"].as_str() {
            if n != os_name() {
                return false;
            }
        }
        if let Some(a) = o["arch"].as_str() {
            if a != arch() {
                return false;
            }
        }
    }
    true
}

pub fn rules_ok(rules: &Value) -> bool {
    let arr = match rules.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return true,
    };
    let mut allowed = false;
    for r in arr {
        if rule_match(r) {
            allowed = r["action"] == "allow";
        }
    }
    allowed
}

fn sub(text: &str, vars: &Map<String, Value>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) if after[..end].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                if let Some(v) = vars.get(&after[..end]) {
                    out.push_str(v.as_str().unwrap_or(""));
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str("${");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn expand_args(args: &Value, vars: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for a in args.as_array().cloned().unwrap_or_default() {
        if let Some(t) = a.as_str() {
            out.push(sub(t, vars));
        } else if rules_ok(&a["rules"]) {
            match &a["value"] {
                Value::String(t) => out.push(sub(t, vars)),
                Value::Array(items) => out.extend(items.iter().filter_map(|x| x.as_str()).map(|x| sub(x, vars))),
                _ => {}
            }
        }
    }
    out
}

/// (аргументы JVM, аргументы игры) для новых и старых версий.
pub fn build_arguments(v: &Value, vars: &Map<String, Value>) -> (Vec<String>, Vec<String>) {
    let (mut jvm, game);
    if v.get("arguments").is_some() {
        jvm = expand_args(&v["arguments"]["jvm"], vars);
        game = expand_args(&v["arguments"]["game"], vars);
    } else {
        jvm = expand_args(&json!(["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]), vars);
        game = s(v, "minecraftArguments").split_whitespace().map(|x| sub(x, vars)).collect();
    }
    if !jvm.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm.push("-cp".into());
        jvm.push(vars.get("classpath").and_then(|x| x.as_str()).unwrap_or("").to_string());
    }
    if !jvm.iter().any(|a| a.starts_with("-Djava.library.path=")) {
        jvm.push(format!("-Djava.library.path={}", vars.get("natives_directory").and_then(|x| x.as_str()).unwrap_or("")));
    }
    (jvm, game)
}

pub fn maven_path(name: &str) -> String {
    let (name, ext) = match name.split_once('@') {
        Some((n, e)) => (n, e),
        None => (name, "jar"),
    };
    let p: Vec<&str> = name.split(':').collect();
    let (group, art, ver) = (p.first().copied().unwrap_or(""), p.get(1).copied().unwrap_or(""), p.get(2).copied().unwrap_or(""));
    let cls = p.get(3).map(|c| format!("-{}", c)).unwrap_or_default();
    format!("{}/{}/{}/{}-{}{}.{}", group.replace('.', "/"), art, ver, art, ver, cls, ext)
}
