//! Локальный HTTP-сервер: отдаёт интерфейс и принимает запросы /<токен>/api/<метод>.
//! Защита как в Python-версии: секретный токен в адресе, проверка Host, CSP.
use crate::api::{Api, ApiError, Out, GET_OK, MAX_UPLOAD};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("../../launcher/web/index.html");
const APP_JS: &str = include_str!("../../launcher/web/app.js");
const APP_CSS: &str = include_str!("../../launcher/web/app.css");

const CSP: &str = "default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; \
                   script-src 'self'; connect-src 'self'; frame-ancestors 'none'";
const MAX_JSON: usize = 1 << 20;

pub struct Running {
    pub url: String,
    pub token: String,
    pub port: u16,
}

/// Запускает сервер в фоновом потоке и возвращает адрес интерфейса.
pub fn start(api: Arc<Api>) -> Result<Running, String> {
    let server = Server::http("127.0.0.1:0").map_err(|e| format!("не удалось открыть порт: {}", e))?;
    let port: u16 = server
        .server_addr()
        .to_string()
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or("не удалось определить порт")?;
    let token: String = rand::thread_rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect();
    let state = Arc::new(Shared { api, token: token.clone(), port });
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let st = Arc::clone(&state);
            std::thread::spawn(move || handle(&st, request));
        }
    });
    Ok(Running { url: format!("http://127.0.0.1:{}/{}/", port, token), token, port })
}

struct Shared {
    api: Arc<Api>,
    token: String,
    port: u16,
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("header")
}

fn send(request: Request, code: u16, body: Vec<u8>, ctype: &str, extra: &[(&str, &str)]) {
    let head_only = *request.method() == Method::Head;
    let mut resp = Response::from_data(if head_only { Vec::new() } else { body })
        .with_status_code(code)
        .with_header(header("Content-Type", ctype))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"));
    for (k, v) in extra {
        resp = resp.with_header(header(k, v));
    }
    let _ = request.respond(resp);
}

fn send_json(request: Request, code: u16, v: Value) {
    send(request, code, v.to_string().into_bytes(), "application/json; charset=utf-8", &[]);
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|x| u8::from_str_radix(x, 16).ok());
                match h {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_query(q: &str) -> Value {
    let mut m = serde_json::Map::new();
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        m.insert(percent_decode(k), Value::String(percent_decode(v)));
    }
    Value::Object(m)
}

fn handle(st: &Shared, mut request: Request) {
    // Защита от DNS-rebinding: принимаем только свой адрес.
    let host = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    if host != format!("127.0.0.1:{}", st.port) && host != format!("localhost:{}", st.port) {
        return send_json(request, 403, json!({"error": "forbidden"}));
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let mut parts = path.splitn(3, '/');
    let _ = parts.next(); // пусто перед первым «/»
    let tok = parts.next().unwrap_or("");
    if !same(tok, &st.token) {
        return send_json(request, 404, json!({"error": "not found"}));
    }
    let rest = match parts.next() {
        Some(r) => r.to_string(),
        None => {
            let loc = format!("/{}/", st.token);
            return send(request, 301, Vec::new(), "text/plain", &[("Location", loc.as_str())]);
        }
    };
    let is_post = *request.method() == Method::Post;
    let is_get = matches!(*request.method(), Method::Get | Method::Head);

    if is_get && !rest.starts_with("api/") && !rest.starts_with("upload/") {
        let name = if rest.is_empty() { "index.html" } else { rest.as_str() };
        let (body, ctype) = match name {
            "index.html" => (INDEX_HTML, "text/html; charset=utf-8"),
            "app.js" => (APP_JS, "text/javascript; charset=utf-8"),
            "app.css" => (APP_CSS, "text/css; charset=utf-8"),
            _ => return send_json(request, 404, json!({"error": "not found"})),
        };
        return send(request, 200, body.as_bytes().to_vec(), ctype, &[("Content-Security-Policy", CSP)]);
    }

    if let Some(name) = rest.strip_prefix("api/") {
        if !is_post && !(is_get && GET_OK.contains(&name)) {
            return send_json(request, 405, json!({"error": "method not allowed"}));
        }
        let params = if is_post {
            let mut raw = Vec::new();
            let _ = request.as_reader().take(MAX_JSON as u64 + 1).read_to_end(&mut raw);
            if raw.len() > MAX_JSON {
                return send_json(request, 413, json!({"error": "too large"}));
            }
            if raw.iter().all(|b| b.is_ascii_whitespace()) {
                json!({})
            } else {
                match serde_json::from_slice::<Value>(&raw) {
                    Ok(v) => v,
                    Err(e) => return send_json(request, 400, json!({"error": format!("Неверный запрос: {}", e)})),
                }
            }
        } else {
            parse_query(query)
        };
        return match st.api.call_out(name, &params) {
            Ok(Out::Json(v)) => send_json(request, 200, v),
            Ok(Out::Image(bytes, ctype)) => send(request, 200, bytes, ctype, &[]),
            Ok(Out::Missing) => send(request, 404, Vec::new(), "text/plain", &[]),
            Err(ApiError(msg)) => send_json(request, 400, json!({"error": msg})),
        };
    }

    if let Some(kind) = rest.strip_prefix("upload/") {
        if !is_post {
            return send_json(request, 405, json!({"error": "method not allowed"}));
        }
        let limit = match MAX_UPLOAD.iter().find(|(k, _)| *k == kind) {
            Some((_, l)) => *l,
            None => return send_json(request, 404, json!({"error": "unknown upload"})),
        };
        let len = request.body_length().unwrap_or(0) as u64;
        if len == 0 || len > limit {
            return send_json(request, 413, json!({"error": "Файл пустой или слишком большой"}));
        }
        let tmp = st.api.tmp_dir().join(format!("{:016x}.upload", rand::random::<u64>()));
        let saved = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create(&tmp)?;
            let mut left = len;
            let mut buf = vec![0u8; 1 << 20];
            let reader = request.as_reader();
            while left > 0 {
                let want = left.min(buf.len() as u64) as usize;
                let n = reader.read(&mut buf[..want])?;
                if n == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Загрузка прервана"));
                }
                f.write_all(&buf[..n])?;
                left -= n as u64;
            }
            Ok(())
        })();
        let result = match saved {
            Ok(()) => st.api.upload(kind, &parse_query(query), &tmp),
            Err(e) => Err(ApiError(format!("Не удалось принять файл: {}", e))),
        };
        let _ = std::fs::remove_file(&tmp);
        return match result {
            Ok(v) => send_json(request, 200, v),
            Err(ApiError(msg)) => send_json(request, 400, json!({"error": msg})),
        };
    }
    send_json(request, 404, json!({"error": "not found"}))
}
