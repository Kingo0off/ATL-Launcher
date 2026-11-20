//! Ядро FriendLauncher. Интерфейс (HTML/JS) общается с ним по тем же запросам, что и в Python-версии.
pub mod accounts;
pub mod api;
pub mod instances;
pub mod launch;
pub mod loaders;
pub mod mojang;
pub mod mods;
pub mod net;
pub mod paths;
pub mod server;
pub mod settings;
pub mod shots;
pub mod util;
pub mod worlds;

pub const APP_NAME: &str = "FriendLauncher";
pub const APP_VERSION: &str = "3.0.0";
