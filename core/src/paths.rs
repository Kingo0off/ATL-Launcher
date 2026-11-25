use std::path::PathBuf;

/// Папка `ATL Launcher`. Если осталась папка под прежним названием (Voxel или FriendLauncher),
/// она переименовывается (сборки, миры и аккаунты сохраняются). Не вышло переименовать:
/// работаем со старой папкой.
fn pick(root: PathBuf) -> PathBuf {
    let new = root.join("ATL Launcher");
    if !new.exists() {
        for name in ["Voxel", "FriendLauncher"] {
            let old = root.join(name);
            if old.is_dir() {
                return if std::fs::rename(&old, &new).is_err() { old } else { new };
            }
        }
    }
    new
}

/// Папка данных. Формат тот же, что у Python-версии, поэтому сборки и настройки общие.
pub fn data_dir() -> PathBuf {
    let base = if let Ok(p) = std::env::var("ATL_HOME") {
        PathBuf::from(p)
    } else if cfg!(windows) {
        let root = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home());
        pick(root)
    } else if cfg!(target_os = "macos") {
        pick(home().join("Library/Application Support"))
    } else {
        let root = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home().join(".local/share"));
        pick(root)
    };
    let _ = std::fs::create_dir_all(&base);
    base
}

pub fn home() -> PathBuf {
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
