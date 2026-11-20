//! Ядро FriendLauncher. Интерфейс (HTML/JS) общается с ним по тем же запросам, что и в Python-версии.
pub mod api;
pub mod paths;
pub mod server;
pub mod settings;

pub const APP_NAME: &str = "FriendLauncher";
pub const APP_VERSION: &str = "3.0.0";
