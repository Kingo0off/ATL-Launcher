//! Скины: личная библиотека и смена скина и плаща у аккаунта Microsoft через Minecraft Services.
use crate::net::{get_bytes, get_json, http_json, http_multipart, Endpoints};
use crate::util::{read_json, urlencode, write_json};
use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const TEXTURE_HOST: &str = "textures.minecraft.net";
pub const MAX_SKIN: usize = 200_000;
const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

fn skin_dir(root: &Path) -> PathBuf {
    root.join("skins")
}

fn load(root: &Path) -> Vec<Value> {
    read_json(&skin_dir(root).join("index.json")).and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

fn save(root: &Path, items: &[Value]) -> Result<(), String> {
    std::fs::create_dir_all(skin_dir(root)).map_err(|e| e.to_string())?;
    write_json(&skin_dir(root).join("index.json"), &Value::Array(items.to_vec())).map_err(|e| e.to_string())
}

pub fn list(root: &Path) -> Vec<Value> {
    let mut v = load(root);
    v.sort_by_key(|s| std::cmp::Reverse(s["added"].as_i64().unwrap_or(0)));
    v
}

pub fn png_size(data: &[u8]) -> Result<(u32, u32), String> {
    if data.len() > MAX_SKIN || data.len() < 24 || !data.starts_with(PNG_SIG) || &data[12..16] != b"IHDR" {
        return Err("Это не PNG-файл скина".into());
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if (w, h) != (64, 64) && (w, h) != (64, 32) {
        return Err(format!("Размер скина должен быть 64×64 (или старый 64×32), а у файла {}×{}", w, h));
    }
    Ok((w, h))
}

fn file_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Неверный id скина".into());
    }
    Ok(skin_dir(root).join(format!("{}.png", id)))
}

fn variant_ok(v: &str) -> &'static str {
    if v == "slim" {
        "slim"
    } else {
        "classic"
    }
}

pub fn add(root: &Path, name: &str, data: &[u8], variant: &str) -> Result<Value, String> {
    png_size(data)?;
    let id = format!("{:010x}", rand::random::<u64>() & 0xff_ffff_ffff);
    std::fs::create_dir_all(skin_dir(root)).map_err(|e| e.to_string())?;
    std::fs::write(file_path(root, &id)?, data).map_err(|e| e.to_string())?;
    let nm: String = name.trim().chars().take(40).collect();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let item = json!({"id": id, "name": if nm.is_empty() { "Скин".to_string() } else { nm }, "variant": variant_ok(variant), "added": now});
    let mut items = load(root);
    items.push(item.clone());
    save(root, &items)?;
    Ok(item)
}

pub fn update(root: &Path, id: &str, name: Option<&str>, variant: Option<&str>) -> Result<Value, String> {
    let mut items = load(root);
    for it in items.iter_mut() {
        if it["id"] == id {
            if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
                it["name"] = json!(n.chars().take(40).collect::<String>());
            }
            if let Some(v) = variant.filter(|v| *v == "classic" || *v == "slim") {
                it["variant"] = json!(v);
            }
            let out = it.clone();
            save(root, &items)?;
            return Ok(out);
        }
    }
    Err("Скин не найден".into())
}

pub fn remove(root: &Path, id: &str) -> Result<(), String> {
    let p = file_path(root, id)?;
    let rest: Vec<Value> = load(root).into_iter().filter(|i| i["id"] != id).collect();
    save(root, &rest)?;
    let _ = std::fs::remove_file(p);
    Ok(())
}

pub fn read(root: &Path, id: &str) -> Option<Vec<u8>> {
    std::fs::read(file_path(root, id).ok()?).ok()
}

pub fn get(root: &Path, id: &str) -> Result<Value, String> {
    load(root).into_iter().find(|i| i["id"] == id).ok_or_else(|| "Скин не найден".to_string())
}

// ---------- Mojang ----------

fn auth(acc: &Value) -> String {
    format!("Bearer {}", acc["access_token"].as_str().unwrap_or(""))
}

fn check(st: u16, js: &Value, what: &str) -> Result<(), String> {
    match st {
        200 | 204 => Ok(()),
        401 | 403 => Err("Mojang не принял вход. Выйди из аккаунта Microsoft и войди снова.".into()),
        429 => Err("Слишком много запросов к Mojang. Подожди минуту и повтори.".into()),
        _ => {
            let msg = js["errorMessage"].as_str().or(js["error"].as_str()).or(js["_raw"].as_str()).unwrap_or("");
            Err(format!("{}: HTTP {} {}", what, st, msg).trim().to_string())
        }
    }
}

pub fn profile(ep: &Endpoints, acc: &Value) -> Result<Value, String> {
    let (st, js) = http_json(&format!("{}/minecraft/profile", ep.mc_api), None, None, &[("Authorization", &auth(acc))], None).map_err(|e| e.0)?;
    if st == 404 {
        return Err("У этого аккаунта нет профиля Minecraft Java Edition.".into());
    }
    check(st, &js, "Профиль")?;
    let skin = js["skins"].as_array().and_then(|a| a.iter().find(|s| s["state"] == "ACTIVE")).cloned();
    let capes: Vec<Value> = js["capes"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| json!({"id": c["id"], "alias": c["alias"].as_str().unwrap_or("Плащ"), "url": c["url"].as_str().unwrap_or(""), "active": c["state"] == "ACTIVE"}))
                .collect()
        })
        .unwrap_or_default();
    let variant = skin.as_ref().and_then(|s| s["variant"].as_str()).unwrap_or("CLASSIC").to_lowercase();
    Ok(json!({
        "name": js["name"], "id": js["id"],
        "skin_url": skin.as_ref().and_then(|s| s["url"].as_str()).unwrap_or(""),
        "variant": variant, "capes": capes
    }))
}

pub fn apply(ep: &Endpoints, acc: &Value, data: &[u8], variant: &str) -> Result<(), String> {
    png_size(data)?;
    let (st, js) = http_multipart(
        &format!("{}/minecraft/profile/skins", ep.mc_api),
        &[("variant", variant_ok(variant))],
        "file",
        "skin.png",
        data,
        &[("Authorization", &auth(acc))],
    )
    .map_err(|e| e.0)?;
    check(st, &js, "Загрузка скина")
}

pub fn reset(ep: &Endpoints, acc: &Value) -> Result<(), String> {
    let (st, js) = http_json(&format!("{}/minecraft/profile/skins/active", ep.mc_api), None, None, &[("Authorization", &auth(acc))], Some("DELETE")).map_err(|e| e.0)?;
    check(st, &js, "Сброс скина")
}

pub fn set_cape(ep: &Endpoints, acc: &Value, cape_id: &str) -> Result<(), String> {
    let url = format!("{}/minecraft/profile/capes/active", ep.mc_api);
    let (st, js) = if cape_id.is_empty() {
        http_json(&url, None, None, &[("Authorization", &auth(acc))], Some("DELETE"))
    } else {
        http_json(&url, Some(&json!({"capeId": cape_id})), None, &[("Authorization", &auth(acc))], Some("PUT"))
    }
    .map_err(|e| e.0)?;
    check(st, &js, "Смена плаща")
}

/// Скачивает PNG только с textures.minecraft.net.
pub fn fetch_texture(url: &str) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).ok_or("Неожиданный адрес скина")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if host != TEXTURE_HOST || path.contains("..") {
        return Err("Неожиданный адрес скина".into());
    }
    get_bytes(&format!("https://{}/{}", TEXTURE_HOST, path), MAX_SKIN as u64 + 1).map_err(|e| format!("Не удалось скачать скин: {}", e.0))
}

/// Скин любого лицензионного игрока по нику: (png, вариант, точный ник).
pub fn lookup(ep: &Endpoints, nick: &str) -> Result<(Vec<u8>, String, String), String> {
    let nick = nick.trim();
    if nick.is_empty() || nick.len() > 16 || !nick.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Ник: 3–16 символов, буквы, цифры и _".into());
    }
    let (st, js) = http_json(&format!("{}/users/profiles/minecraft/{}", ep.mojang_api, urlencode(nick)), None, None, &[], None).map_err(|e| e.0)?;
    let id = js["id"].as_str().unwrap_or("").to_string();
    if st == 204 || st == 404 || id.is_empty() {
        return Err(format!("Игрок «{}» не найден", nick));
    }
    check(st, &js, "Поиск игрока")?;
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Неожиданный ответ Mojang".into());
    }
    let prof = get_json(&format!("{}/session/minecraft/profile/{}", ep.session_api, id)).map_err(|e| e.0)?;
    for p in prof["properties"].as_array().cloned().unwrap_or_default() {
        if p["name"] == "textures" {
            let raw = base64::engine::general_purpose::STANDARD.decode(p["value"].as_str().unwrap_or("")).map_err(|_| "Неожиданный ответ Mojang")?;
            let tex: Value = serde_json::from_slice(&raw).map_err(|_| "Неожиданный ответ Mojang")?;
            let sk = &tex["textures"]["SKIN"];
            let url = sk["url"].as_str().ok_or_else(|| format!("У «{}» стандартный скин", nick))?;
            let variant = if sk["metadata"]["model"] == "slim" { "slim" } else { "classic" };
            return Ok((fetch_texture(url)?, variant.into(), js["name"].as_str().unwrap_or(nick).to_string()));
        }
    }
    Err(format!("У «{}» нет скина", nick))
}
