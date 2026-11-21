use serde_json::{json, Map, Value};
use std::path::Path;

/// Настройки, которые видит интерфейс.
pub const PUBLIC: [&str; 6] = ["server_url", "ms_client_id", "cf_api_key", "ram_mb", "java_path", "site_dir"];

pub fn defaults() -> Map<String, Value> {
    let v = json!({
        "server_url": "",
        "ms_client_id": option_env!("FL_MS_CLIENT_ID").unwrap_or(""),
        "cf_api_key": "",
        "ram_mb": 4096,
        "java_path": "",
        "last_account": "",
        "site_dir": ""
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
