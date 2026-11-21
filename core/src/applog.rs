//! Журналы приложения: launcher.log (ход работы) и crash.log (сбои) в папке данных.
use std::io::Write;
use std::path::{Path, PathBuf};

fn append(path: &Path, line: &str) {
    if std::fs::metadata(path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("old"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), line);
    }
}

pub fn log(dir: &Path, msg: &str) {
    let _ = std::fs::create_dir_all(dir);
    append(&dir.join("launcher.log"), msg);
}

/// Любая паника пишется в crash.log (окна консоли у программы нет).
pub fn install_panic_hook(dir: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::create_dir_all(&dir);
        append(&dir.join("crash.log"), &format!("{}\n{}", info, std::backtrace::Backtrace::force_capture()));
    }));
}
