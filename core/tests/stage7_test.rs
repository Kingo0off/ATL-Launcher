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

#[test]
fn theme_is_validated() {
    use friendlauncher_core::settings::clean_theme;
    use serde_json::json;
    let ok = clean_theme(&json!({"accent": "#AABBCC", "bg": "#000000", "evil": "x"})).unwrap();
    assert_eq!(ok, json!({"accent": "#aabbcc", "bg": "#000000"}));
    assert!(clean_theme(&json!({"accent": "red"})).is_err());
    assert!(clean_theme(&json!({"play": "#12345"})).is_err());
    assert!(clean_theme(&json!({"preset": "a b"})).is_err());
    assert_eq!(clean_theme(&json!({"preset": "ocean"})).unwrap(), json!({"preset": "ocean"}));
    assert_eq!(clean_theme(&json!(null)).unwrap(), json!({}));
    assert_eq!(clean_theme(&json!({"event": "halloween"})).unwrap(), json!({"event": "halloween"}));
    assert!(clean_theme(&json!({"event": "easter"})).is_err());
})).is_err());
}
