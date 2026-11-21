//! Вход: Ely.by (Yggdrasil) и Microsoft (браузер с PKCE или код → Xbox → XSTS → Minecraft).
use crate::net::{http_json, Endpoints};
use crate::util::urlencode;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const SCOPE: &str = "XboxLive.signin offline_access";

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn ne(e: crate::net::NetError) -> String {
    e.0
}

pub fn upsert_account(root: &Path, acc: &Value) {
    let mut list: Vec<Value> = crate::accounts::load(root)
        .into_iter()
        .filter(|a| !(a["type"] == acc["type"] && a["uuid"] == acc["uuid"]))
        .collect();
    list.push(acc.clone());
    let _ = crate::accounts::save(root, &list);
}

fn rand_token(bytes: usize) -> String {
    let v: Vec<u8> = (0..bytes).map(|_| rand::random::<u8>()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v)
}

// ---------- Ely.by ----------

fn ely_error(js: &Value) -> String {
    js["errorMessage"]
        .as_str()
        .or(js["error"].as_str())
        .or(js["_raw"].as_str())
        .unwrap_or("неизвестная ошибка")
        .to_string()
}

/// totp — код двухфакторной аутентификации, если она включена.
pub fn ely_login(ep: &Endpoints, username: &str, password: &str, totp: &str) -> Result<Value, String> {
    let pw = if totp.is_empty() { password.to_string() } else { format!("{}:{}", password, totp) };
    let client_token = format!("{:032x}", rand::random::<u128>());
    let (st, js) = http_json(
        &format!("{}/auth/authenticate", ep.ely_auth),
        Some(&json!({"username": username, "password": pw, "clientToken": client_token, "requestUser": true})),
        None,
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        return Err(format!("Ely.by: {}", ely_error(&js)));
    }
    let prof = &js["selectedProfile"];
    if !prof.is_object() {
        return Err("Ely.by: у аккаунта нет профиля игрока".into());
    }
    Ok(json!({
        "type": "ely", "name": prof["name"], "uuid": prof["id"],
        "access_token": js["accessToken"], "client_token": js["clientToken"].as_str().unwrap_or(&client_token)
    }))
}

fn ely_ensure(root: &Path, ep: &Endpoints, acc: &Value) -> Result<Value, String> {
    let (st, _) = http_json(&format!("{}/auth/validate", ep.ely_auth), Some(&json!({"accessToken": acc["access_token"]})), None, &[], None).map_err(ne)?;
    if st == 200 {
        return Ok(acc.clone());
    }
    let (st, js) = http_json(
        &format!("{}/auth/refresh", ep.ely_auth),
        Some(&json!({"accessToken": acc["access_token"], "clientToken": acc["client_token"]})),
        None,
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        return Err(format!("Сессия Ely.by истекла, войди заново ({})", ely_error(&js)));
    }
    let mut a = acc.clone();
    a["access_token"] = js["accessToken"].clone();
    upsert_account(root, &a);
    Ok(a)
}

// ---------- Microsoft ----------

fn xsts_error(code: i64) -> Option<&'static str> {
    Some(match code {
        2148916233 => "У этой учётной записи Microsoft нет профиля Xbox. Зайди на xbox.com и создай его.",
        2148916235 => "Xbox Live недоступен в твоей стране.",
        2148916236 | 2148916237 => "Нужна проверка возраста/взрослого аккаунта Xbox.",
        2148916238 => "Детский аккаунт: добавь его в семейную группу Microsoft и разреши онлайн-игру.",
        _ => return None,
    })
}

fn need_client_id(cid: &str) -> Result<(), String> {
    if cid.trim().is_empty() {
        return Err("Не задан Client ID приложения Microsoft. Открой «Настройки» и вставь его (как получить — в README, раздел «Вход через Microsoft»).".into());
    }
    Ok(())
}

fn ms_error(js: &Value) -> String {
    js["error_description"].as_str().or(js["error"].as_str()).unwrap_or("неизвестная ошибка").to_string()
}

/// Цепочка: токен Microsoft → Xbox Live → XSTS → токен Minecraft и профиль.
pub fn ms_chain(ep: &Endpoints, ms_token: &str) -> Result<(String, f64, Value), String> {
    let (st, xbl) = http_json(
        &ep.xbl,
        Some(&json!({
            "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", ms_token)},
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        })),
        None,
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        return Err(format!("Xbox Live: HTTP {}", st));
    }
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or("Xbox Live: нет uhs в ответе")?.to_string();
    let (st, xsts) = http_json(
        &ep.xsts,
        Some(&json!({
            "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl["Token"]]},
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"
        })),
        None,
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        let code = xsts["XErr"].as_i64().unwrap_or(0);
        return Err(xsts_error(code).map(String::from).unwrap_or_else(|| format!("XSTS: HTTP {}, код {}", st, code)));
    }
    let (st, mc) = http_json(
        &format!("{}/authentication/login_with_xbox", ep.mc_api),
        Some(&json!({"identityToken": format!("XBL3.0 x={};{}", uhs, xsts["Token"].as_str().unwrap_or(""))})),
        None,
        &[],
        None,
    )
    .map_err(ne)?;
    if st == 403 {
        return Err("Minecraft Services отклонил приложение (Invalid app registration). Client ID ещё не одобрен Mojang — см. README, раздел «Вход через Microsoft».".into());
    }
    if st != 200 {
        return Err(format!("Minecraft Services: HTTP {} {}", st, mc));
    }
    let token = mc["access_token"].as_str().ok_or("Minecraft Services: нет токена")?.to_string();
    let (st, prof) = http_json(&format!("{}/minecraft/profile", ep.mc_api), None, None, &[("Authorization", &format!("Bearer {}", token))], None).map_err(ne)?;
    if st == 404 {
        return Err("На этом аккаунте нет Minecraft: Java Edition.".into());
    }
    if st != 200 {
        return Err(format!("Не удалось получить профиль: HTTP {}", st));
    }
    Ok((token, mc["expires_in"].as_f64().unwrap_or(86400.0), prof))
}

fn ms_complete(root: &Path, ep: &Endpoints, tok: &Value) -> Result<Value, String> {
    let (mc_token, expires, prof) = ms_chain(ep, tok["access_token"].as_str().unwrap_or(""))?;
    let acc = json!({
        "type": "ms", "name": prof["name"], "uuid": prof["id"], "access_token": mc_token,
        "refresh_token": tok["refresh_token"].as_str().unwrap_or(""), "expires_at": now() + expires
    });
    upsert_account(root, &acc);
    Ok(acc)
}

pub fn ms_ensure(root: &Path, ep: &Endpoints, acc: &Value, client_id: &str) -> Result<Value, String> {
    if acc["expires_at"].as_f64().unwrap_or(0.0) > now() + 120.0 {
        return Ok(acc.clone());
    }
    need_client_id(client_id)?;
    let rt = acc["refresh_token"].as_str().unwrap_or("");
    if rt.is_empty() {
        return Err("Нужно войти в Microsoft заново.".into());
    }
    let (st, tok) = http_json(
        &format!("{}/token", ep.ms_base),
        None,
        Some(&[("grant_type", "refresh_token"), ("client_id", client_id), ("refresh_token", rt), ("scope", SCOPE)]),
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        return Err(format!("Сессия Microsoft истекла, войди заново ({})", ms_error(&tok)));
    }
    ms_complete(root, ep, &tok)
}

/// Освежает токен перед запуском или сменой скина.
pub fn ensure_fresh(root: &Path, ep: &Endpoints, acc: &Value, client_id: &str) -> Result<Value, String> {
    if acc["type"] == "ely" {
        ely_ensure(root, ep, acc)
    } else {
        ms_ensure(root, ep, acc, client_id)
    }
}

/// Вход по коду: on_code(код, адрес) показывает код пользователю.
pub fn ms_device_flow(root: &Path, ep: &Endpoints, client_id: &str, on_code: &dyn Fn(&str, &str), cancel: &AtomicBool) -> Result<Value, String> {
    need_client_id(client_id)?;
    let (st, js) = http_json(&format!("{}/devicecode", ep.ms_base), None, Some(&[("client_id", client_id), ("scope", SCOPE)]), &[], None).map_err(ne)?;
    if st != 200 {
        return Err(format!("Microsoft: {}", ms_error(&js)));
    }
    on_code(js["user_code"].as_str().unwrap_or(""), js["verification_uri"].as_str().unwrap_or(""));
    let mut interval = js["interval"].as_u64().unwrap_or(5);
    let deadline = Instant::now() + Duration::from_secs(js["expires_in"].as_u64().unwrap_or(900));
    let device = js["device_code"].as_str().unwrap_or("").to_string();
    while Instant::now() < deadline {
        for _ in 0..interval * 2 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Вход отменён".into());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let (st, tok) = http_json(
            &format!("{}/token", ep.ms_base),
            None,
            Some(&[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("client_id", client_id), ("device_code", &device)]),
            &[],
            None,
        )
        .map_err(ne)?;
        if st == 200 {
            return ms_complete(root, ep, &tok);
        }
        match tok["error"].as_str().unwrap_or("") {
            "authorization_pending" => continue,
            "slow_down" => interval += 5,
            _ => return Err(format!("Microsoft: {}", ms_error(&tok))),
        }
    }
    Err("Время ввода кода истекло".into())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn done_page(ok: bool, msg: &str) -> String {
    format!(
        "<!doctype html><meta charset=utf-8><title>Voxel</title><body style=\"font:18px system-ui;background:#0f1117;color:#e9eaf2;display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center;max-width:420px;padding:20px\"><h1 style=\"color:{}\">{}</h1><p>{}</p></div>",
        if ok { "#7be0a8" } else { "#ff8a8a" },
        if ok { "Вход выполнен" } else { "Ошибка входа" },
        html_escape(msg)
    )
}

/// Вход как на обычных сайтах: страница Microsoft в браузере, пароль вводится там.
/// OAuth 2.0 с PKCE, возврат на http://localhost:порт (виден только этому компьютеру).
pub fn ms_browser_flow(root: &Path, ep: &Endpoints, client_id: &str, on_url: &dyn Fn(&str), cancel: &AtomicBool, timeout: Duration) -> Result<Value, String> {
    need_client_id(client_id)?;
    let verifier = rand_token(48);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = rand_token(18);
    let srv = tiny_http::Server::http("127.0.0.1:0").map_err(|e| format!("Не удалось открыть локальный порт для входа: {}", e))?;
    let port = srv.server_addr().to_ip().map(|a| a.port()).ok_or("Не удалось определить порт")?;
    let redirect = format!("http://localhost:{}", port);
    let url = format!(
        "{}/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}&prompt=select_account&response_mode=query",
        ep.ms_base,
        urlencode(client_id),
        urlencode(&redirect),
        urlencode(SCOPE),
        challenge,
        urlencode(&state)
    );
    on_url(&url);
    let deadline = Instant::now() + timeout;
    let (mut code, mut error) = (String::new(), String::new());
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return Err("Вход отменён".into());
        }
        let rq = match srv.recv_timeout(Duration::from_millis(500)) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(_) => break,
        };
        let full = rq.url().to_string();
        let (path, query) = full.split_once('?').unwrap_or((full.as_str(), ""));
        let q = crate::server::parse_query(query);
        let has = q.get("code").is_some() || q.get("error").is_some();
        let html_header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
        let reply = |rq: tiny_http::Request, status: u16, body: String| {
            let _ = rq.respond(tiny_http::Response::from_string(body).with_status_code(status).with_header(html_header.clone()));
        };
        if path != "/" || !has {
            reply(rq, 404, String::new());
            continue;
        }
        if q["state"].as_str().unwrap_or("") != state {
            // чужой запрос с неверным state вход не завершает
            reply(rq, 200, done_page(false, "Неверный ответ входа. Закрой вкладку и начни вход в лаунчере заново."));
            continue;
        }
        if let Some(e) = q["error"].as_str() {
            error = format!("Вход не выполнен: {}", q["error_description"].as_str().unwrap_or(e));
            reply(rq, 200, done_page(false, &error));
        } else {
            code = q["code"].as_str().unwrap_or("").to_string();
            reply(rq, 200, done_page(true, "Готово! Можно закрыть эту вкладку и вернуться в лаунчер."));
        }
        break;
    }
    drop(srv);
    if !error.is_empty() {
        return Err(error);
    }
    if code.is_empty() {
        return Err("Время входа истекло. Нажми «Войти через Microsoft» ещё раз.".into());
    }
    let (st, tok) = http_json(
        &format!("{}/token", ep.ms_base),
        None,
        Some(&[("grant_type", "authorization_code"), ("client_id", client_id), ("code", &code), ("redirect_uri", &redirect), ("code_verifier", &verifier), ("scope", SCOPE)]),
        &[],
        None,
    )
    .map_err(ne)?;
    if st != 200 {
        return Err(format!("Microsoft: {}", ms_error(&tok)));
    }
    ms_complete(root, ep, &tok)
}
