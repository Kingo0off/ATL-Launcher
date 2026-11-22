use friendlauncher_core::net::Endpoints;
use friendlauncher_core::selfupdate::{self, newer};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

fn mock(files: HashMap<String, Vec<u8>>) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for rq in server.incoming_requests() {
            let url = rq.url().to_string();
            match files.get(&url) {
                Some(b) => {
                    let _ = rq.respond(tiny_http::Response::from_data(b.clone()));
                }
                None => {
                    let _ = rq.respond(tiny_http::Response::from_string("no").with_status_code(404));
                }
            }
        }
    });
    base
}

#[test]
fn version_compare_is_numeric() {
    assert!(newer("3.10.0", "3.9.0"));
    assert!(newer("v3.1.5", "3.1.4"));
    assert!(newer("3.1", "3.0.9"));
    assert!(!newer("3.1.4", "3.1.4"));
    assert!(!newer("3.0.9", "3.1.0"));
    assert!(!newer("3.1.0-beta", "3.1.0"));
}

#[test]
fn check_and_download() {
    let exe: Vec<u8> = b"MZ\x90\x00 fake exe body".to_vec();
    let sha: String = Sha256::digest(&exe).iter().map(|b| format!("{:02x}", b)).collect();
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/dl/Voxel.exe".into(), exe.clone());
    let base = mock(f.clone());
    let release = |digest: &str| {
        json!({"tag_name": "v3.9.1", "body": "Новое", "assets": [
            {"name": "other.zip", "size": 1, "browser_download_url": format!("{}/x", base)},
            {"name": "Voxel.exe", "size": exe.len(), "digest": digest, "browser_download_url": format!("{}/dl/Voxel.exe", base)}]})
    };
    f.insert("/rel_ok".into(), release(&format!("sha256:{}", sha)).to_string().into_bytes());
    f.insert("/rel_bad".into(), release("sha256:00").to_string().into_bytes());
    let base2 = mock(f);
    // относительные адреса скачивания смотрят на первый сервер, где лежит exe
    let ep = |p: &str| Endpoints { self_release: format!("{}/{}", base2, p), ..Endpoints::default() };

    assert!(selfupdate::check(&ep("rel_ok"), "3.9.1").unwrap().is_none());
    assert!(selfupdate::check(&ep("rel_ok"), "3.10.0").unwrap().is_none());
    let info = selfupdate::check(&ep("rel_ok"), "3.0.0").unwrap().expect("есть обновление");
    assert_eq!(info.version, "3.9.1");
    assert_eq!(info.notes, "Новое");

    let d = std::env::temp_dir().join(format!("fl_upd_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let dest = d.join("new.exe");
    selfupdate::download(&info, &dest).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), exe);

    let bad = selfupdate::check(&ep("rel_bad"), "3.0.0").unwrap().unwrap();
    let dest2 = d.join("bad.exe");
    assert!(selfupdate::download(&bad, &dest2).is_err());
    assert!(!dest2.exists());
}
