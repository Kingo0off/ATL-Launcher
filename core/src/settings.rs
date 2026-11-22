use serde_json::{json, Map, Value};
use std::path::Path;

/// Настройки, которые видит интерфейс.
pub const PUBLIC: [&str; 7] = ["theme", "server_url", "ms_client_id", "cf_api_key", "ram_mb", "java_path", "site_dir"];

/// Ключ API CurseForge, вшитый при сборке (переменная FL_CF_API_KEY). Свой ключ из настроек важнее.
pub const BUILTIN_CF_KEY: &str = match option_env!("FL_CF_API_KEY") {
    Some(k) => k,
    None => "",
};

pub fn defaults() -> Map<String, Value> {
    let v = json!({
        "server_url": "",
        "ms_client_id": option_env!("FL_MS_CLIENT_ID").unwrap_or(""),
        "cf_api_key": "",
        "ram_mb": 4096,
        "java_path": "",
        "last_account": "",
        "site_dir": "",
        "theme": {}
    });
    v.as_object().cloned().unwrap_or_default()
}

pub fn load(dir: &Path) -> Map<String, Value> {
    let mut data = defaults();
    if let Ok(text) = std::fs::read_to_string(dir.join("settings.json")) {
        if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            for (k, v) in m {
                data.insert(k, v);
            }
        }
    }
    // Пустое значение в файле не должно затирать Client ID, зашитый при сборке.
    if data.get("ms_client_id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        data.insert("ms_client_id".into(), json!(option_env!("FL_MS_CLIENT_ID").unwrap_or("")));
    }
    data
}

pub fn save(dir: &Path, data: &Map<String, Value>) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(&Value::Object(data.clone()))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(dir.join("settings.json"), text)
}

/// Проверка темы оформления: только известные ключи и цвета вида #rrggbb.
pub fn clean_theme(v: &Value) -> Result<Value, String> {
    let obj = match v {
        Value::Object(m) => m,
        Value::Null => return Ok(json!({})),
        _ => return Err("Тема должна быть объектом".into()),
    };
    let mut out = Map::new();
    for (k, val) in obj {
        match k.as_str() {
            "accent" | "play" | "bg" | "text" => {
                let t = val.as_str().ok_or("Цвет должен быть строкой")?;
                let ok = t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit());
                if !ok {
                    return Err(format!("Неверный цвет: {}", t));
                }
                out.insert(k.clone(), json!(t.to_lowercase()));
            }
            "event" => {
                let t = val.as_str().ok_or("Событие должно быть строкой")?;
                if !matches!(t, "" | "auto" | "newyear" | "halloween" | "valentine") {
                    return Err("Неизвестное событие".into());
                }
                out.insert(k.clone(), json!(t));
            }
            "preset" => {
                let t = val.as_str().ok_or("Название темы должно быть строкой")?;
                if t.len() > 32 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Err("Неверное название темы".into());
                }
                out.insert(k.clone(), json!(t));
            }
            _ => {}
        }
    }
    Ok(Value::Object(out))
}
