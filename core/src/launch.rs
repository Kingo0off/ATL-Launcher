//! Запуск игры: Java от Mojang, установка загрузчика, аргументы, процесс и его вывод.
use crate::mojang::{self, Mojang};
use crate::net::{download, download_many, get_json, Endpoints, NetError, Progress, Task};
use crate::util::{read_json, safe_join, write_json};
use crate::{loaders, APP_NAME, APP_VERSION};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub type Log<'a> = &'a (dyn Fn(&str) + Sync);

pub const ELY_AGENT_ARG: &str = "ely.by";

fn ne(e: NetError) -> String {
    e.0
}

fn ioe(e: std::io::Error) -> String {
    e.to_string()
}

/// Команда без чёрного окна консоли.
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.stdin(Stdio::null());
    c
}

pub fn runtime_platform() -> &'static str {
    let arm = cfg!(target_arch = "aarch64");
    let x64 = cfg!(target_pointer_width = "64");
    match crate::paths::os_name() {
        "windows" => {
            if arm {
                "windows-arm64"
            } else if x64 {
                "windows-x64"
            } else {
                "windows-x86"
            }
        }
        "osx" => {
            if arm {
                "mac-os-arm64"
            } else {
                "mac-os"
            }
        }
        _ => {
            if x64 {
                "linux"
            } else {
                "linux-i386"
            }
        }
    }
}

fn find_java(folder: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    let mut stack = vec![folder.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().map(|n| n == name).unwrap_or(false) && p.parent().and_then(|x| x.file_name()).map(|n| n == "bin").unwrap_or(false) {
                return Some(p);
            }
        }
    }
    None
}

fn which_java() -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Java нужной версии: путь из настроек, либо скачанная от Mojang (один раз).
pub fn ensure_java(root: &Path, ep: &Endpoints, version: &Value, user_java: &str, progress: Progress, log: Log) -> Result<PathBuf, String> {
    if !user_java.is_empty() && Path::new(user_java).is_file() {
        return Ok(PathBuf::from(user_java));
    }
    let comp = version["javaVersion"]["component"].as_str().unwrap_or("jre-legacy").to_string();
    let major = version["javaVersion"]["majorVersion"].as_i64().unwrap_or(8);
    crate::util::safe_name(&comp)?;
    let dest = root.join("runtime").join(&comp);
    let marker = dest.join(".installed");
    let all = get_json(&ep.java_all).ok();
    let entry = all.as_ref().and_then(|a| a[runtime_platform()][&comp].as_array()).and_then(|a| a.first()).cloned();
    let exe = find_java(&dest);
    let entry = match entry {
        Some(e) => e,
        None => {
            return exe.or_else(which_java).ok_or_else(|| {
                format!("Не удалось получить Java {} от Mojang для этой платформы. Установи Java {} и укажи путь в настройках.", major, major)
            })
        }
    };
    let sha = entry["manifest"]["sha1"].as_str().unwrap_or("").to_string();
    if let Some(e) = &exe {
        if std::fs::read_to_string(&marker).map(|m| m.trim() == sha).unwrap_or(false) {
            return Ok(e.clone());
        }
    }
    log(&format!("Скачиваю Java {} (один раз)…", major));
    let mf = get_json(entry["manifest"]["url"].as_str().unwrap_or("")).map_err(ne)?;
    let (mut tasks, mut execs, mut links) = (Vec::new(), Vec::new(), Vec::new());
    for (rel, info) in mf["files"].as_object().cloned().unwrap_or_default() {
        let path = safe_join(&dest, &rel)?;
        match info["type"].as_str().unwrap_or("") {
            "directory" => {
                std::fs::create_dir_all(&path).map_err(ioe)?;
            }
            "file" => {
                let raw = &info["downloads"]["raw"];
                tasks.push(Task {
                    url: raw["url"].as_str().unwrap_or("").into(),
                    dest: path.clone(),
                    sha1: raw["sha1"].as_str().map(String::from),
                    size: raw["size"].as_u64(),
                });
                if info["executable"].as_bool().unwrap_or(false) {
                    execs.push(path);
                }
            }
            "link" => links.push((path, info["target"].as_str().unwrap_or("").to_string())),
            _ => {}
        }
    }
    download_many(&tasks, progress, &format!("Java {}", major), 8).map_err(ne)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in &execs {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
        }
        for (path, target) in &links {
            if !path.exists() {
                if let Some(d) = path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = std::os::unix::fs::symlink(target, path);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (&execs, &links);
    }
    std::fs::write(&marker, &sha).map_err(ioe)?;
    find_java(&dest).ok_or_else(|| "Java скачана, но файл java не найден внутри.".to_string())
}

// ---------- процессы ----------

/// Читает вывод процесса построчно (stdout и stderr вместе), ждёт завершения.
/// Если `stop` стал true, процесс убивается. Возвращает код выхода (-1, если убит).
pub fn pump(child: &mut Child, line: &(dyn Fn(String) + Sync), stop: &AtomicBool) -> i32 {
    fn reader<R: Read>(r: R, line: &(dyn Fn(String) + Sync)) {
        let mut br = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match br.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => line(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string()),
            }
        }
    }
    let out = child.stdout.take();
    let err = child.stderr.take();
    let mut code = -1;
    std::thread::scope(|s| {
        if let Some(o) = out {
            s.spawn(move || reader(o, line));
        }
        if let Some(e) = err {
            s.spawn(move || reader(e, line));
        }
        loop {
            if stop.load(Ordering::Relaxed) {
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(Some(st)) => {
                    code = st.code().unwrap_or(-1);
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
                Err(_) => break,
            }
        }
    });
    code
}

// ---------- загрузчики ----------

fn profile_loader(root: &Path, ep: &Endpoints, kind: &str, mc: &str, version: &str, log: Log) -> Result<String, String> {
    let base = if kind == "fabric" { &ep.fabric } else { &ep.quilt };
    let mut version = version.to_string();
    if version.is_empty() || version == "latest" {
        let all = loaders::loader_versions(ep, kind, mc).map_err(ne)?;
        version = all.first().cloned().ok_or_else(|| format!("{} не поддерживает Minecraft {}", kind, mc))?;
    }
    crate::util::safe_name(&version)?;
    let vid = format!("{}-loader-{}-{}", kind, version, mc);
    let path = root.join("versions").join(&vid).join(format!("{}.json", vid));
    if !path.is_file() {
        log(&format!("Устанавливаю {} {}…", kind, version));
        let profile = get_json(&format!("{}/versions/loader/{}/{}/profile/json", base, mc, version)).map_err(ne)?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(ioe)?;
        }
        write_json(&path, &profile).map_err(ioe)?;
        if let Some(id) = profile["id"].as_str() {
            if id != vid {
                // id внутри файла должен совпасть с папкой, иначе запуск не найдёт версию
                let alt = root.join("versions").join(id);
                crate::util::safe_name(id)?;
                std::fs::create_dir_all(&alt).map_err(ioe)?;
                write_json(&alt.join(format!("{}.json", id)), &profile).map_err(ioe)?;
                return Ok(id.to_string());
            }
        }
    }
    Ok(vid)
}

fn installer_loader(root: &Path, ep: &Endpoints, kind: &str, mc: &str, version: &str, java: &Path, log: Log) -> Result<String, String> {
    crate::util::safe_name(version)?;
    let cache_path = root.join("loaders.json");
    let key = format!("{}:{}:{}", kind, mc, version);
    let mut cache = read_json(&cache_path).unwrap_or_else(|| json!({}));
    if let Some(c) = cache[&key].as_str() {
        if root.join("versions").join(c).join(format!("{}.json", c)).is_file() {
            return Ok(c.to_string());
        }
    }
    let url = if kind == "forge" {
        format!("{0}/{1}-{2}/forge-{1}-{2}-installer.jar", ep.forge_maven, mc, version)
    } else {
        format!("{0}/{1}/neoforge-{1}-installer.jar", ep.neoforge_maven, version)
    };
    let jar = root.join("installers").join(url.rsplit('/').next().unwrap_or("installer.jar"));
    log(&format!("Скачиваю установщик {}…", kind));
    download(&url, &jar, None, None, 3).map_err(ne)?;
    let profiles = root.join("launcher_profiles.json");
    if !profiles.is_file() {
        write_json(&profiles, &json!({"profiles": {}, "version": 3})).map_err(ioe)?;
    }
    let versions = root.join("versions");
    let list = |d: &Path| -> Vec<String> {
        std::fs::read_dir(d).map(|r| r.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default()
    };
    let before = list(&versions);
    log(&format!("Запускаю установщик {} (может занять пару минут)…", kind));
    let mut cmd = command(java);
    cmd.arg("-jar").arg(&jar).arg("--installClient").arg(root).current_dir(root).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Не удалось запустить Java для установщика: {}", e))?;
    let stop = AtomicBool::new(false);
    let code = pump(&mut child, &|l| {
        if !l.trim().is_empty() {
            log(&format!("  {}", l));
        }
    }, &stop);
    if code != 0 {
        return Err(format!("Установщик {} завершился с ошибкой (код {}). Подробности выше в журнале.", kind, code));
    }
    let mut found: Vec<String> = list(&versions).into_iter().filter(|d| !before.contains(d) && d.to_lowercase().contains("forge")).collect();
    if found.is_empty() {
        found = list(&versions).into_iter().filter(|d| d.to_lowercase().contains("forge") && d.contains(version)).collect();
    }
    let mtime = |d: &String| crate::util::mtime_secs(&versions.join(d));
    found.sort_by(|a, b| mtime(a).partial_cmp(&mtime(b)).unwrap_or(std::cmp::Ordering::Equal));
    let vid = found.pop().ok_or_else(|| format!("После установки {} не нашлась новая версия в {}", kind, versions.display()))?;
    cache[&key] = json!(vid);
    let _ = write_json(&cache_path, &cache);
    Ok(vid)
}

/// Возвращает id версии, которую надо запускать (для vanilla это просто mc).
pub fn install_loader(root: &Path, ep: &Endpoints, loader: &Value, mc: &str, java: &Path, log: Log) -> Result<String, String> {
    let kind = loader["type"].as_str().unwrap_or("vanilla").to_lowercase();
    let version = loader["version"].as_str().unwrap_or("");
    match kind.as_str() {
        "" | "vanilla" | "none" => Ok(mc.to_string()),
        "fabric" | "quilt" => profile_loader(root, ep, &kind, mc, version, log),
        "forge" | "neoforge" => {
            if version.is_empty() {
                return Err(format!("Для {} нужно выбрать версию загрузчика.", kind));
            }
            installer_loader(root, ep, &kind, mc, version, java, log)
        }
        other => Err(format!("Неизвестный загрузчик: {}", other)),
    }
}

// ---------- Ely.by ----------

pub fn ensure_authlib(root: &Path, ep: &Endpoints, log: Log) -> Result<PathBuf, String> {
    let path = root.join("authlib-injector.jar");
    if path.is_file() {
        return Ok(path);
    }
    log("Скачиваю authlib-injector (нужен для Ely.by)…");
    let url = get_json(&ep.authlib).ok().and_then(|j| j["download_url"].as_str().map(String::from)).or_else(|| {
        let rel = get_json("https://api.github.com/repos/yushijinhun/authlib-injector/releases/latest").ok()?;
        rel["assets"].as_array()?.iter().find(|a| a["name"].as_str().map(|n| n.ends_with(".jar")).unwrap_or(false)).and_then(|a| a["browser_download_url"].as_str().map(String::from))
    });
    let url = url.ok_or("Не удалось найти authlib-injector для загрузки.")?;
    download(&url, &path, None, None, 3).map_err(ne)?;
    Ok(path)
}

/// Разбор строки аргументов JVM с учётом кавычек.
pub fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q, mut any) = (Vec::new(), String::new(), None::<char>, false);
    for c in s.chars() {
        match q {
            Some(qc) if c == qc => q = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                q = Some(c);
                any = true;
            }
            None if c.is_whitespace() => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            None => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub struct RunSpec {
    pub root: PathBuf,
    pub ep: Endpoints,
    pub mc: String,
    pub loader: Value,
    pub game_dir: PathBuf,
    pub account: Value,
    pub user_java: String,
    pub ram_mb: i64,
    pub java_args: String,
}

/// Готовит всё и возвращает Command, готовую к запуску (stdout и stderr уже перехвачены).
pub fn prepare_game(spec: &RunSpec, progress: Progress, log: Log) -> Result<Command, String> {
    let moj = Mojang::new(spec.root.clone(), spec.ep.clone());
    log(&format!("Готовлю Minecraft {}…", spec.mc));
    moj.install_vanilla_json(&spec.mc).map_err(ne)?;
    let base = moj.load_version(&spec.mc)?;
    let java0 = ensure_java(&spec.root, &spec.ep, &base, &spec.user_java, progress, log)?;
    let vid = install_loader(&spec.root, &spec.ep, &spec.loader, &spec.mc, &java0, log)?;
    let gf = moj.install_game_files(&vid, &spec.game_dir, progress)?;
    let java = ensure_java(&spec.root, &spec.ep, &gf.version, &spec.user_java, progress, log)?;

    let acc = &spec.account;
    let token = acc["access_token"].as_str().unwrap_or("");
    let uuid = acc["uuid"].as_str().unwrap_or("");
    let mut vars = Map::new();
    let mut set = |k: &str, v: String| {
        vars.insert(k.to_string(), Value::String(v));
    };
    set("auth_player_name", acc["name"].as_str().unwrap_or("Player").to_string());
    set("auth_uuid", uuid.to_string());
    set("auth_access_token", token.to_string());
    set("auth_session", format!("token:{}:{}", token, uuid));
    set("user_type", if acc["type"] == "ms" { "msa" } else { "mojang" }.to_string());
    set("user_properties", "{}".into());
    set("clientid", String::new());
    set("auth_xuid", String::new());
    set("version_name", vid.clone());
    set("version_type", gf.version["type"].as_str().unwrap_or("release").to_string());
    set("game_directory", spec.game_dir.to_string_lossy().to_string());
    set("assets_root", moj.assets().to_string_lossy().to_string());
    set("game_assets", gf.game_assets.to_string_lossy().to_string());
    set("assets_index_name", gf.version["assetIndex"]["id"].as_str().unwrap_or("").to_string());
    set("natives_directory", gf.natives_dir.to_string_lossy().to_string());
    set("launcher_name", APP_NAME.to_string());
    set("launcher_version", APP_VERSION.to_string());
    set("classpath", gf.classpath.clone());
    set("classpath_separator", mojang::classpath_sep().to_string());
    set("library_directory", moj.libs().to_string_lossy().to_string());
    set("primary_jar", gf.jar.to_string_lossy().to_string());
    let (jvm, game) = mojang::build_arguments(&gf.version, &vars);

    let mut pre: Vec<String> = Vec::new();
    if acc["type"] == "ely" {
        let a = ensure_authlib(&spec.root, &spec.ep, log)?;
        pre.push(format!("-javaagent:{}={}", a.display(), ELY_AGENT_ARG));
    }
    let ram = if spec.ram_mb > 0 { spec.ram_mb } else { 4096 };
    pre.push(format!("-Xmx{}M", ram));
    pre.push(format!("-Xms{}M", ram.min(1024)));
    pre.extend(split_args(&spec.java_args));

    let main = gf.version["mainClass"].as_str().ok_or("В версии нет mainClass")?;
    std::fs::create_dir_all(&spec.game_dir).map_err(ioe)?;
    let mut cmd = command(&java);
    cmd.args(&pre).args(&jvm).arg(main).args(&game).current_dir(&spec.game_dir).stdout(Stdio::piped()).stderr(Stdio::piped());
    log(&format!("Запуск игры под ником {}…", acc["name"].as_str().unwrap_or("")));
    Ok(cmd)
}

// ---------- проверка аккаунта ----------

pub const ELY_AUTH: &str = "https://authserver.ely.by";

fn upsert_account(root: &Path, acc: &Value) {
    let mut list: Vec<Value> = crate::accounts::load(root)
        .into_iter()
        .filter(|a| !(a["type"] == acc["type"] && a["uuid"] == acc["uuid"]))
        .collect();
    list.push(acc.clone());
    let _ = crate::accounts::save(root, &list);
}

/// Освежает токен перед запуском. Ely.by: проверка и refresh. Microsoft: пока только проверка срока.
pub fn ensure_fresh(root: &Path, acc: &Value) -> Result<Value, String> {
    if acc["type"] == "ely" {
        let (st, _) = crate::net::http_json(&format!("{}/auth/validate", ELY_AUTH), Some(&json!({"accessToken": acc["access_token"]})), None, &[], None).map_err(ne)?;
        if st == 200 {
            return Ok(acc.clone());
        }
        let (st, js) = crate::net::http_json(
            &format!("{}/auth/refresh", ELY_AUTH),
            Some(&json!({"accessToken": acc["access_token"], "clientToken": acc["client_token"]})),
            None,
            &[],
            None,
        )
        .map_err(ne)?;
        if st != 200 {
            let why = js["errorMessage"].as_str().or(js["error"].as_str()).unwrap_or("неизвестная ошибка");
            return Err(format!("Сессия Ely.by истекла, войди заново ({})", why));
        }
        let mut a = acc.clone();
        a["access_token"] = js["accessToken"].clone();
        upsert_account(root, &a);
        return Ok(a);
    }
    let exp = acc["expires_at"].as_f64().unwrap_or(0.0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    if exp > now + 120.0 {
        return Ok(acc.clone());
    }
    Err("Вход Microsoft устарел. Обновление входа появится на следующем этапе — пока войди через Microsoft заново.".into())
}
