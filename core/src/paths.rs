use std::path::PathBuf;

/// Папка данных. Та же, что у Python-версии, поэтому сборки и настройки общие.
pub fn data_dir() -> PathBuf {
    let base = if let Ok(p) = std::env::var("FRIENDLAUNCHER_HOME") {
        PathBuf::from(p)
    } else if cfg!(windows) {
        let root = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home());
        root.join("FriendLauncher")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/FriendLauncher")
    } else {
        let root = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home().join(".local/share"));
        root.join("FriendLauncher")
    };
    let _ = std::fs::create_dir_all(&base);
    base
}

fn home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn os_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}
