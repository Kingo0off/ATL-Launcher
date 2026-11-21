use friendlauncher_core::applog;
use std::time::Duration;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s7_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn applog_writes_files() {
    let d = tmp("log");
    applog::log(&d, "привет");
    let t = std::fs::read_to_string(d.join("launcher.log")).unwrap();
    assert!(t.contains("привет"));
}
