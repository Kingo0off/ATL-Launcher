use friendlauncher_core::net::Endpoints;
use friendlauncher_core::{accounts, auth, server, skins};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::Duration;

fn mock(files: HashMap<String, Vec<u8>>) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for rq in server.incoming_requests() {
            let url = rq.url().split('?').next().unwrap_or("").to_string();
            let _ = match files.get(&url) {
                Some(b) => rq.respond(tiny_http::Response::from_data(b.clone())),
                None => rq.respond(tiny_http::Response::from_string("{}").with_status_code(404)),
            };
        }
    });
    base
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s5_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn j(v: Value) -> Vec<u8> {
    v.to_string().into_bytes()
}

fn ms_mock() -> Endpoints {
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/ms/token".into(), j(json!({"access_token":"msat","refresh_token":"rt1"})));
    f.insert("/xbl".into(), j(json!({"Token":"xt","DisplayClaims":{"xui":[{"uhs":"uh"}]}})));
    f.insert("/xsts".into(), j(json!({"Token":"st","DisplayClaims":{"xui":[{"uhs":"uh"}]}})));
    f.insert("/mc/authentication/login_with_xbox".into(), j(json!({"access_token":"mct","expires_in":86400})));
    f.insert("/mc/minecraft/profile".into(), j(json!({"name":"Steve","id":"abc123","skins":[{"state":"ACTIVE","url":"https://textures.minecraft.net/texture/x","variant":"SLIM"}],"capes":[{"id":"c1","alias":"Cape","state":"INACTIVE"}]})));
    f.insert("/ely/auth/authenticate".into(), j(json!({"accessToken":"eat","clientToken":"ect","selectedProfile":{"name":"Alex","id":"e1"}})));
    let base = mock(f);
    Endpoints {
        ms_base: format!("{}/ms", base),
        xbl: format!("{}/xbl", base),
        xsts: format!("{}/xsts", base),
        mc_api: format!("{}/mc", base),
        ely_auth: format!("{}/ely", base),
        ..Endpoints::default()
    }
}

#[test]
fn browser_login_with_pkce_and_wrong_state() {
    let ep = ms_mock();
    let root = tmp("browser");
    let (tx, rx) = mpsc::channel::<String>();
    let (root2, ep2) = (root.clone(), ep.clone());
    let h = std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        auth::ms_browser_flow(&root2, &ep2, "client-id", &|u| tx.send(u.to_string()).unwrap(), &cancel, Duration::from_secs(20))
    });
    let url = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(url.contains("code_challenge_method=S256") && url.contains("client_id=client-id"));
    let q = server::parse_query(url.split_once('?').unwrap().1);
    let redirect = q["redirect_uri"].as_str().unwrap().replace("localhost", "127.0.0.1");
    let state = q["state"].as_str().unwrap().to_string();
    // чужой ответ с неверным state вход не завершает
    let _ = ureq::get(&format!("{}/?code=evil&state=wrong", redirect)).call();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!h.is_finished());
    let page = ureq::get(&format!("{}/?code=goodcode&state={}", redirect, state)).call().unwrap().into_string().unwrap();
    assert!(page.contains("Вход выполнен"));
    let acc = h.join().unwrap().unwrap();
    assert_eq!(acc["name"], "Steve");
    assert_eq!(acc["refresh_token"], "rt1");
    let saved = accounts::load(&root);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0]["access_token"], "mct");
}

#[test]
fn browser_login_error_and_cancel() {
    let ep = ms_mock();
    let root = tmp("browser2");
    let (tx, rx) = mpsc::channel::<String>();
    let (root2, ep2) = (root.clone(), ep.clone());
    let h = std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        auth::ms_browser_flow(&root2, &ep2, "cid", &|u| tx.send(u.to_string()).unwrap(), &cancel, Duration::from_secs(20))
    });
    let url = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let q = server::parse_query(url.split_once('?').unwrap().1);
    let redirect = q["redirect_uri"].as_str().unwrap().replace("localhost", "127.0.0.1");
    let state = q["state"].as_str().unwrap().to_string();
    let _ = ureq::get(&format!("{}/?error=access_denied&error_description=nope&state={}", redirect, state)).call();
    let err = h.join().unwrap().unwrap_err();
    assert!(err.contains("nope"), "{}", err);
    assert!(accounts::load(&root).is_empty());
    // отмена
    let cancel = AtomicBool::new(true);
    let e = auth::ms_browser_flow(&root, &ep, "cid", &|_| {}, &cancel, Duration::from_secs(5)).unwrap_err();
    assert!(e.contains("отменён"));
    assert!(auth::ms_browser_flow(&root, &ep, "", &|_| {}, &cancel, Duration::from_secs(5)).is_err());
}

#[test]
fn refresh_and_ely() {
    let ep = ms_mock();
    let root = tmp("refresh");
    let old = json!({"type":"ms","name":"Steve","uuid":"abc123","access_token":"old","refresh_token":"rt0","expires_at":1.0});
    let fresh = auth::ensure_fresh(&root, &ep, &old, "cid").unwrap();
    assert_eq!(fresh["access_token"], "mct");
    assert_eq!(fresh["refresh_token"], "rt1");
    assert!(auth::ensure_fresh(&root, &ep, &old, "").is_err());
    let ely = auth::ely_login(&ep, "user", "pw", "123456").unwrap();
    assert_eq!(ely["name"], "Alex");
    assert_eq!(ely["client_token"], "ect");
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

#[test]
fn skin_library_and_profile() {
    let root = tmp("skins");
    assert!(skins::add(&root, "bad", b"hello", "classic").is_err());
    assert!(skins::add(&root, "big", &png(128, 128), "classic").unwrap_err().contains("128"));
    let a = skins::add(&root, "  Мой скин  ", &png(64, 64), "slim").unwrap();
    let b = skins::add(&root, "", &png(64, 32), "weird").unwrap();
    assert_eq!(a["name"], "Мой скин");
    assert_eq!(a["variant"], "slim");
    assert_eq!(b["name"], "Скин");
    assert_eq!(b["variant"], "classic");
    assert_eq!(skins::list(&root).len(), 2);
    let id = a["id"].as_str().unwrap();
    assert_eq!(skins::update(&root, id, Some("Новый"), Some("classic")).unwrap()["name"], "Новый");
    assert!(skins::read(&root, id).is_some());
    assert!(skins::read(&root, "../x").is_none());
    skins::remove(&root, id).unwrap();
    assert_eq!(skins::list(&root).len(), 1);
    assert!(skins::get(&root, id).is_err());

    let ep = ms_mock();
    let acc = json!({"access_token":"mct"});
    let p = skins::profile(&ep, &acc).unwrap();
    assert_eq!(p["name"], "Steve");
    assert_eq!(p["variant"], "slim");
    assert_eq!(p["capes"][0]["id"], "c1");
    assert!(skins::fetch_texture("https://evil.example.com/texture/x").is_err());
    assert!(skins::fetch_texture("https://textures.minecraft.net/../x").is_err());
    assert!(skins::lookup(&ep, "bad nick!").is_err());
}
