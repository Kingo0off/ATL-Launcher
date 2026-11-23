//! Самообновление: ищем новый релиз на GitHub, качаем .exe и подменяем им текущий.
use crate::net::{self, Endpoints};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const ASSET: &str = "Voxel.exe";

#[derive(Clone, Debug)]
pub struct Info {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub notes: String,
}

fn parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .split(|c| c == '-' || c == '+')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|x| x.parse::<u64>().unwrap_or(0))
        .collect()
}

/// `a` новее, чем `b` (сравнение по числам: 3.10.0 новее 3.9.0).
pub fn newer(a: &str, b: &str) -> bool {
    let (x, y) = (parts(a), parts(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

/// Разбор ответа GitHub `releases/latest`.
pub fn parse_release(js: &Value) -> Option<Info> {
    let tag = js.get("tag_name")?.as_str()?;
    let assets = js.get("assets")?.as_array()?;
    let asset = assets.iter().find(|a| a.get("name").and_then(|n| n.as_str()) == Some(ASSET))?;
    let sha = asset
        .get("digest")
        .and_then(|d| d.as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|d| d.to_lowercase());
    Some(Info {
        version: tag.trim_start_matches(|c| c == 'v' || c == 'V').to_string(),
        url: asset.get("browser_download_url")?.as_str()?.to_string(),
        size: asset.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
        sha256: sha,
        notes: clean_body(js.get("body").and_then(|b| b.as_str()).unwrap_or("")).chars().take(1500).collect(),
    })
}

/// Есть ли версия новее текущей.
pub fn check(ep: &Endpoints, current: &str) -> Result<Option<Info>, String> {
    let js = net::get_json(&ep.self_release).map_err(|e| format!("Не удалось проверить обновления: {}", e.0))?;
    match parse_release(&js) {
        Some(info) if newer(&info.version, current) => Ok(Some(info)),
        _ => Ok(None),
    }
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

/// Скачивает новый .exe в `dest` и проверяет его.
pub fn download(info: &Info, dest: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(dest);
    net::download(&info.url, dest, None, None, 3).map_err(|e| e.0)?;
    let bad = |m: &str| {
        let _ = std::fs::remove_file(dest);
        Err(m.to_string())
    };
    let len = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if len == 0 || (info.size > 0 && len != info.size) {
        return bad("Файл обновления скачался не полностью. Попробуй ещё раз.");
    }
    let head = std::fs::read(dest).map(|b| b.iter().take(2).copied().collect::<Vec<u8>>()).unwrap_or_default();
    if head != b"MZ" {
        return bad("Скачанный файл не похож на программу.");
    }
    if let Some(want) = &info.sha256 {
        if sha256_file(dest).map_err(|e| e.to_string())? != *want {
            return bad("Контрольная сумма обновления не совпала.");
        }
    }
    Ok(())
}

/// Удаляет файл прошлой версии, оставшийся после обновления.
pub fn cleanup() {
    if let Ok(cur) = std::env::current_exe() {
        let _ = std::fs::remove_file(cur.with_extension("old"));
        let _ = std::fs::remove_file(cur.with_file_name("Voxel.old"));
    }
}

/// Подменяет работающий .exe новым и запускает его через пару секунд, когда старый закроется.
#[cfg(windows)]
pub fn apply(new: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Если файл лаунчера переименовали или переложили, пока он работал, прежнего пути уже нет.
    // Тогда кладём новую версию рядом под именем Voxel.exe.
    let cur = if exe.exists() { exe } else { exe.with_file_name("Voxel.exe") };
    let old = cur.with_extension("old");
    let _ = std::fs::remove_file(&old);
    if cur.exists() {
        std::fs::rename(&cur, &old).map_err(|e| format!("Не удалось заменить файл лаунчера: {}", e))?;
    }
    if let Err(e) = std::fs::copy(new, &cur) {
        if old.exists() {
            let _ = std::fs::rename(&old, &cur);
        }
        return Err(format!("Не удалось записать новую версию: {}", e));
    }
    let cmd = format!("/C ping -n 4 127.0.0.1 >nul & start \"\" \"{}\"", cur.display());
    std::process::Command::new("cmd")
        .raw_arg(cmd)
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|e| format!("Не удалось перезапустить лаунчер: {}", e))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn apply(_new: &Path) -> Result<(), String> {
    Err("Самообновление работает только в Windows.".into())
}

/// Убирает из описания релиза служебные строки коммита (соавторство, ссылка на сессию).
pub fn clean_body(body: &str) -> String {
    body.lines()
        .filter(|l| {
            let t = l.trim_start().to_lowercase();
            !(t.starts_with("co-authored-by:") || t.starts_with("claude-session:") || t.contains("generated with [claude code]"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Превращает список релизов GitHub в записи для вкладки «Новости».
/// Служебные строки коммита (Co-Authored-By, Claude-Session) вырезаются.
pub fn parse_news(js: &Value) -> Vec<Value> {
    js.as_array()
        .map(|a| {
            a.iter()
                .filter(|r| !r["draft"].as_bool().unwrap_or(false))
                .filter_map(|r| {
                    let tag = r["tag_name"].as_str()?;
                    let body = clean_body(r["body"].as_str().unwrap_or(""));
                    if body.is_empty() {
                        return None; // релизы без описания в новостях не нужны
                    }
                    Some(json!({
                        "version": tag.trim_start_matches('v'),
                        "title": r["name"].as_str().unwrap_or(tag),
                        "date": r["published_at"].as_str().unwrap_or("").chars().take(10).collect::<String>(),
                        "body": body.chars().take(8000).collect::<String>(),
                        "url": r["html_url"].as_str().unwrap_or(""),
                        "prerelease": r["prerelease"].as_bool().unwrap_or(false)
                    }))
                })
                .take(15)
                .collect()
        })
        .unwrap_or_default()
}

/// Последние релизы с описанием изменений.
pub fn news(ep: &Endpoints) -> Result<Vec<Value>, String> {
    let base = ep.self_release.trim_end_matches("/latest");
    let js = net::get_json(&format!("{}?per_page=15", base)).map_err(|e| format!("Не удалось загрузить новости: {}", e.0))?;
    Ok(parse_news(&js))
}
