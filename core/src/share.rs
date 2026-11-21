//! Раздача файла лаунчера друзьям по локальной сети: страница с кнопкой «Скачать».
use std::fs::File;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Share {
    stop: Arc<AtomicBool>,
    pub url: String,
    pub port: u16,
}

impl Share {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Адрес этого компьютера в локальной сети (пакеты не отправляются).
pub fn lan_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn page(name: &str, size: u64) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>FriendLauncher</title>\
<body style=\"font:18px system-ui;background:#0f1117;color:#e9eaf2;display:grid;place-items:center;min-height:100vh;margin:0\">\
<div style=\"text-align:center;padding:24px\"><h1>FriendLauncher</h1><p>Лаунчер Minecraft для игры с друзьями</p>\
<p><a href=\"/{0}\" style=\"display:inline-block;background:#3ddc97;color:#06150d;padding:14px 28px;border-radius:12px;text-decoration:none;font-weight:600\">Скачать для Windows ({1:.1} МБ)</a></p>\
<p style=\"opacity:.7;font-size:15px\">Windows может показать предупреждение SmartScreen: «Подробнее» → «Выполнить в любом случае».</p></div>",
        name,
        size as f64 / 1_048_576.0
    )
}

/// Запускает раздачу файла `file` под именем `name`. Порт 8080–8099, первый свободный.
pub fn start(file: PathBuf, name: &str) -> Result<Share, String> {
    let mut found = None;
    for port in 8080u16..8100 {
        if let Ok(s) = tiny_http::Server::http(("0.0.0.0", port)) {
            found = Some((s, port));
            break;
        }
    }
    let (server, port) = found.ok_or("Не нашлось свободного порта (8080–8099) для раздачи")?;
    let size = std::fs::metadata(&file).map(|m| m.len()).map_err(|e| format!("Не удалось прочитать файл лаунчера: {}", e))?;
    let stop = Arc::new(AtomicBool::new(false));
    let (st, name_owned) = (stop.clone(), name.to_string());
    std::thread::spawn(move || {
        while !st.load(Ordering::Relaxed) {
            let rq = match server.recv_timeout(Duration::from_millis(300)) {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(_) => break,
            };
            let (file, name) = (file.clone(), name_owned.clone());
            std::thread::spawn(move || {
                let path = rq.url().split('?').next().unwrap_or("").to_string();
                if path == "/" {
                    let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
                    let _ = rq.respond(tiny_http::Response::from_string(page(&name, size)).with_header(h));
                } else if path == format!("/{}", name) {
                    match File::open(&file) {
                        Ok(f) => {
                            let h = tiny_http::Header::from_bytes(&b"Content-Disposition"[..], format!("attachment; filename=\"{}\"", name).as_bytes()).unwrap();
                            let t = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/octet-stream"[..]).unwrap();
                            let _ = rq.respond(tiny_http::Response::from_file(f).with_header(h).with_header(t));
                        }
                        Err(_) => {
                            let _ = rq.respond(tiny_http::Response::from_string("Файл недоступен").with_status_code(500));
                        }
                    }
                } else {
                    let _ = rq.respond(tiny_http::Response::from_string("Не найдено").with_status_code(404));
                }
            });
        }
    });
    Ok(Share { stop, url: format!("http://{}:{}/", lan_ip(), port), port })
}
