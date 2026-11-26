//! Сеть: запросы JSON, скачивание файлов с проверкой sha1, параллельная загрузка.
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const UA: &str = "ATLLauncher/3.0";

#[derive(Debug, Clone)]
pub struct NetError(pub String);

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn io(e: std::io::Error) -> NetError {
    NetError(e.to_string())
}

/// Адреса сервисов. В тестах подменяются на локальный сервер.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub manifest: String,
    pub asset: String,
    pub fabric: String,
    pub quilt: String,
    pub forge_meta: String,
    pub neoforge_meta: String,
    pub neoforge_prism: String,
    pub java_all: String,
    pub authlib: String,
    pub forge_maven: String,
    pub neoforge_maven: String,
    pub neoforge_legacy_meta: String,
    pub neoforge_legacy_maven: String,
    pub ms_base: String,
    pub self_release: String,
    pub xbl: String,
    pub xsts: String,
    pub mc_api: String,
    pub ely_auth: String,
    pub mojang_api: String,
    pub session_api: String,
    pub modrinth: String,
    pub curseforge: String,
    pub extra_trusted_hosts: Vec<String>,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            manifest: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json".into(),
            asset: "https://resources.download.minecraft.net".into(),
            fabric: "https://meta.fabricmc.net/v2".into(),
            quilt: "https://meta.quiltmc.org/v3".into(),
            forge_meta: "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml".into(),
            neoforge_meta: "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml".into(),
            neoforge_prism: "https://raw.githubusercontent.com/PrismLauncher/meta-launcher/master/net.neoforged/index.json".into(),
            java_all: "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json".into(),
            authlib: "https://authlib-injector.yushi.moe/artifact/latest.json".into(),
            forge_maven: "https://maven.minecraftforge.net/net/minecraftforge/forge".into(),
            neoforge_maven: "https://maven.neoforged.net/releases/net/neoforged/neoforge".into(),
            neoforge_legacy_meta: "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml".into(),
            neoforge_legacy_maven: "https://maven.neoforged.net/releases/net/neoforged/forge".into(),
            ms_base: "https://login.microsoftonline.com/consumers/oauth2/v2.0".into(),
            self_release: "https://api.github.com/repos/Kingo0off/ATL-Launcher/releases/latest".into(),
            xbl: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            mc_api: "https://api.minecraftservices.com".into(),
            ely_auth: "https://authserver.ely.by".into(),
            mojang_api: "https://api.mojang.com".into(),
            session_api: "https://sessionserver.mojang.com".into(),
            modrinth: "https://api.modrinth.com/v2".into(),
            curseforge: "https://api.curseforge.com/v1".into(),
            extra_trusted_hosts: Vec::new(),
        }
    }
}

pub type Progress<'a> = &'a (dyn Fn(usize, usize, &str) + Sync);

pub fn no_progress(_: usize, _: usize, _: &str) {}

fn agent() -> &'static ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| {
        ureq::AgentBuilder::new()
            .user_agent(UA)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build()
    })
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).to_string()
}

fn transport_err(url: &str, t: impl std::fmt::Display) -> NetError {
    NetError(format!("Нет соединения с {}: {}", host_of(url), t))
}

fn read_all(resp: ureq::Response, limit: u64) -> Result<Vec<u8>, NetError> {
    let mut buf = Vec::new();
    resp.into_reader().take(limit).read_to_end(&mut buf).map_err(io)?;
    Ok(buf)
}

/// Возвращает (код ответа, JSON). HTTP-ошибки не считаются ошибкой, смотри код.
pub fn http_json(
    url: &str,
    body: Option<&Value>,
    form: Option<&[(&str, &str)]>,
    headers: &[(&str, &str)],
    method: Option<&str>,
) -> Result<(u16, Value), NetError> {
    let m = method.unwrap_or(if body.is_some() || form.is_some() { "POST" } else { "GET" });
    let mut req = agent().request(m, url).set("Accept", "application/json");
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let result = if let Some(b) = body {
        req.set("Content-Type", "application/json").send_string(&b.to_string())
    } else if let Some(f) = form {
        req.send_form(f)
    } else {
        req.call()
    };
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(t)) => return Err(transport_err(url, t)),
    };
    let status = resp.status();
    let raw = read_all(resp, 64 << 20)?;
    let js = if raw.iter().all(|b| b.is_ascii_whitespace()) {
        json!({})
    } else {
        serde_json::from_slice(&raw)
            .unwrap_or_else(|_| json!({"_raw": String::from_utf8_lossy(&raw[..raw.len().min(300)])}))
    };
    Ok((status, js))
}

fn fetch(url: &str) -> Result<ureq::Response, NetError> {
    match agent().get(url).call() {
        Ok(r) => Ok(r),
        Err(ureq::Error::Status(code, _)) => Err(NetError(format!("{} → HTTP {}", url, code))),
        Err(ureq::Error::Transport(t)) => Err(transport_err(url, t)),
    }
}

/// Ограниченное по времени получение метаданных загрузчика.
pub fn loader_metadata(url: &str) -> Result<String, NetError> {
    let agent = ureq::AgentBuilder::new().user_agent(UA).timeout(Duration::from_secs(12)).build();
    let response = agent.get(url).call().map_err(|e| NetError(format!("Не удалось загрузить список версий: {}", e)))?;
    let raw = read_all(response, 8 << 20)?;
    String::from_utf8(raw).map_err(|e| NetError(format!("Некорректный ответ сервера: {}", e)))
}

pub fn get_json(url: &str) -> Result<Value, NetError> {
    let (st, js) = http_json(url, None, None, &[], None)?;
    if st != 200 {
        return Err(NetError(format!("{} → HTTP {}", url, st)));
    }
    Ok(js)
}

pub fn get_text(url: &str) -> Result<String, NetError> {
    let raw = read_all(fetch(url)?, 64 << 20)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

fn up_to_date(dest: &Path, sha1: Option<&str>, size: Option<u64>) -> bool {
    let md = match std::fs::metadata(dest) {
        Ok(m) if m.is_file() => m,
        _ => return false,
    };
    if let Some(s) = size {
        return md.len() == s;
    }
    if let Some(h) = sha1 {
        return sha1_file(dest).map(|x| x == h.to_lowercase()).unwrap_or(false);
    }
    true
}

fn download_once(url: &str, part: &Path, sha1: Option<&str>) -> Result<(), NetError> {
    let mut r = fetch(url)?.into_reader();
    let mut f = File::create(part).map_err(io)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = r.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        f.write_all(&buf[..n]).map_err(io)?;
    }
    f.flush().map_err(io)?;
    drop(f);
    if let Some(want) = sha1 {
        if hex(&h.finalize()) != want.to_lowercase() {
            return Err(NetError("не совпала контрольная сумма".into()));
        }
    }
    Ok(())
}

/// Скачивает файл, если его нет или он не совпадает. Возвращает true, если скачал.
pub fn download(url: &str, dest: &Path, sha1: Option<&str>, size: Option<u64>, retries: u32) -> Result<bool, NetError> {
    if up_to_date(dest, sha1, size) {
        return Ok(false);
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let mut part = dest.as_os_str().to_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let mut last = NetError("неизвестная ошибка".into());
    for attempt in 0..retries.max(1) {
        match download_once(url, &part, sha1) {
            Ok(()) => {
                std::fs::rename(&part, dest).map_err(io)?;
                return Ok(true);
            }
            Err(e) => {
                last = e;
                if attempt + 1 < retries {
                    std::thread::sleep(Duration::from_secs(1 + attempt as u64));
                }
            }
        }
    }
    let _ = std::fs::remove_file(&part);
    Err(NetError(format!("Не удалось скачать {}: {}", url, last)))
}

#[derive(Clone, Debug)]
pub struct Task {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Параллельная загрузка. Первая ошибка останавливает остальных.
pub fn download_many(tasks: &[Task], progress: Progress, label: &str, workers: usize) -> Result<(), NetError> {
    let total = tasks.len();
    progress(0, total, label);
    if total == 0 {
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let err: Mutex<Option<NetError>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..workers.clamp(1, total) {
            s.spawn(|| loop {
                if failed.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                let t = &tasks[i];
                match download(&t.url, &t.dest, t.sha1.as_deref(), t.size, 3) {
                    Ok(_) => {
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if d % 5 == 0 || d == total {
                            progress(d, total, label);
                        }
                    }
                    Err(e) => {
                        failed.store(true, Ordering::Relaxed);
                        if let Ok(mut g) = err.lock() {
                            if g.is_none() {
                                *g = Some(e);
                            }
                        }
                        break;
                    }
                }
            });
        }
    });
    match err.into_inner() {
        Ok(Some(e)) => Err(e),
        _ => Ok(()),
    }
}

/// Загрузка PNG-файла формой multipart (для смены скина). Возвращает (код, JSON).
pub fn http_multipart(url: &str, fields: &[(&str, &str)], file_field: &str, filename: &str, data: &[u8], headers: &[(&str, &str)]) -> Result<(u16, Value), NetError> {
    let boundary = format!("----fl{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>());
    let mut body: Vec<u8> = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n", boundary, k, v).as_bytes());
    }
    body.extend_from_slice(
        format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: image/png\r\n\r\n", boundary, file_field, filename).as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    let mut req = agent().post(url).set("Accept", "application/json").set("Content-Type", &format!("multipart/form-data; boundary={}", boundary));
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let resp = match req.send_bytes(&body) {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(t)) => return Err(transport_err(url, t)),
    };
    let status = resp.status();
    let raw = read_all(resp, 1 << 20)?;
    let js = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    Ok((status, js))
}

/// Скачивает файл целиком в память (с ограничением размера).
pub fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, NetError> {
    read_all(fetch(url)?, limit)
}
