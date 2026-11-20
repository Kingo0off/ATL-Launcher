use friendlauncher_core::{api::Api, server};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-test-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Простой HTTP-клиент: (код, заголовки в нижнем регистре, тело).
fn request(port: u16, method: &str, path: &str, host: Option<&str>, body: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let host = host.map(|h| h.to_string()).unwrap_or(format!("127.0.0.1:{}", port));
    let req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        method, path, host, body.len(), body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let code: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, head.to_lowercase(), body.to_string())
}

fn start(tag: &str) -> (server::Running, PathBuf) {
    let dir = temp_dir(tag);
    let run = server::start(Arc::new(Api::new(dir.clone()))).unwrap();
    (run, dir)
}

#[test]
fn security_token_and_host() {
    let (run, _d) = start("sec");
    let (c, _, _) = request(run.port, "GET", "/wrong/", None, "");
    assert_eq!(c, 404);
    let (c, _, _) = request(run.port, "GET", &format!("/{}/", run.token), Some("evil.com"), "");
    assert_eq!(c, 403, "чужой Host должен отклоняться");
    let (c, h, _) = request(run.port, "GET", &format!("/{}", run.token), None, "");
    assert_eq!(c, 301);
    assert!(h.contains(&format!("location: /{}/", run.token.to_lowercase())) || h.contains("location:"));
}

#[test]
fn static_files_and_csp() {
    let (run, _d) = start("static");
    let (c, h, b) = request(run.port, "GET", &run.url.replace(&format!("http://127.0.0.1:{}", run.port), ""), None, "");
    assert_eq!(c, 200);
    assert!(h.contains("content-security-policy"));
    assert!(h.contains("text/html"));
    assert!(b.contains("app.js"));
    for name in ["app.js", "app.css"] {
        let (c, _, b) = request(run.port, "GET", &format!("/{}/{}", run.token, name), None, "");
        assert_eq!(c, 200, "{}", name);
        assert!(!b.is_empty());
    }
    for bad in ["../Cargo.toml", "server.rs", "..%2f..%2fetc%2fpasswd", "api"] {
        let (c, _, _) = request(run.port, "GET", &format!("/{}/{}", run.token, bad), None, "");
        assert!(c == 404 || c == 405, "{} -> {}", bad, c);
    }
}

#[test]
fn api_state_poll_ping() {
    let (run, dir) = start("api");
    let (c, _, b) = request(run.port, "GET", &format!("/{}/api/state", run.token), None, "");
    assert_eq!(c, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["version"], friendlauncher_core::APP_VERSION);
    assert_eq!(v["settings"]["ram_mb"], 4096);
    assert!(v["instances"].is_array() && v["accounts"].is_array());
    assert_eq!(v["data_dir"], &*dir.to_string_lossy());
    let (c, _, b) = request(run.port, "POST", &format!("/{}/api/poll", run.token), None, "{}");
    assert_eq!(c, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["notices"].is_array() && v["tasks"].is_object());
    let (c, _, _) = request(run.port, "GET", &format!("/{}/api/ping", run.token), None, "");
    assert_eq!(c, 200);
}

#[test]
fn api_method_rules() {
    let (run, _d) = start("meth");
    let (c, _, _) = request(run.port, "GET", &format!("/{}/api/settings.set", run.token), None, "");
    assert_eq!(c, 405, "изменяющие методы только POST");
    let (c, _, b) = request(run.port, "POST", &format!("/{}/api/no.such", run.token), None, "{}");
    assert_eq!(c, 400);
    assert!(b.contains("error"));
    let (c, _, _) = request(run.port, "POST", &format!("/{}/api/state", run.token), None, "{не json");
    assert_eq!(c, 400);
}

#[test]
fn settings_save_and_validate() {
    let (run, dir) = start("set");
    let url = format!("/{}/api/settings.set", run.token);
    let (c, _, b) = request(run.port, "POST", &url, None, r#"{"ram_mb": 6144, "server_url": "  https://x.y/  ", "cf_api_key": "K"}"#);
    assert_eq!(c, 200, "{}", b);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["settings"]["ram_mb"], 6144);
    assert_eq!(v["settings"]["server_url"], "https://x.y/");
    let saved = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    assert!(saved.contains("6144") && saved.contains("https://x.y/"));
    let (c, _, b) = request(run.port, "POST", &url, None, r#"{"ram_mb": 100}"#);
    assert_eq!(c, 400);
    assert!(b.contains("512"));
    let (c, _, _) = request(run.port, "POST", &url, None, r#"{"ram_mb": "много"}"#);
    assert_eq!(c, 400);
    let (c, _, b) = request(run.port, "POST", &url, None, r#"{"ram_mb": 0}"#);
    assert_eq!(c, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["settings"]["ram_mb"], 4096, "0 означает значение по умолчанию");
    // после перезапуска настройки читаются с диска
    let api2 = Api::new(dir.clone());
    let s = api2.call("state", &serde_json::json!({})).unwrap();
    assert_eq!(s["settings"]["cf_api_key"], "K");
}

#[test]
fn percent_decoding() {
    assert_eq!(server::percent_decode("a%20b+c%D0%B0"), "a b cа");
    assert_eq!(server::percent_decode("100%"), "100%");
    assert_eq!(server::percent_decode("%zz"), "%zz");
}
