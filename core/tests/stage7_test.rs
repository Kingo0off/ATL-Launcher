use friendlauncher_core::{applog, share};
use std::time::Duration;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s7_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn share_serves_only_the_launcher_file() {
    let d = tmp("share");
    let f = d.join("secret-name.bin");
    std::fs::write(&f, b"launcher-bytes").unwrap();
    std::fs::write(d.join("other.txt"), b"nope").unwrap();
    let s = share::start(f, "FriendLauncher.exe").unwrap();
    let base = format!("http://127.0.0.1:{}", s.port);
    assert!(s.url.ends_with(&format!(":{}/", s.port)));
    let page = ureq::get(&format!("{}/", base)).call().unwrap().into_string().unwrap();
    assert!(page.contains("FriendLauncher.exe"));
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut ureq::get(&format!("{}/FriendLauncher.exe", base)).call().unwrap().into_reader(), &mut body).unwrap();
    assert_eq!(body, b"launcher-bytes");
    assert!(matches!(ureq::get(&format!("{}/other.txt", base)).call(), Err(ureq::Error::Status(404, _))));
    assert!(matches!(ureq::get(&format!("{}/../other.txt", base)).call(), Err(_)));
    s.stop();
    std::thread::sleep(Duration::from_millis(900));
    assert!(ureq::get(&format!("{}/", base)).timeout(Duration::from_secs(2)).call().is_err());
}

#[test]
fn applog_writes_files() {
    let d = tmp("log");
    applog::log(&d, "привет");
    let t = std::fs::read_to_string(d.join("launcher.log")).unwrap();
    assert!(t.contains("привет"));
}
