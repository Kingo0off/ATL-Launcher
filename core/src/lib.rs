//! Ядро Voxel. Интерфейс (HTML/JS) общается с ним по тем же запросам, что и в Python-версии.
pub mod accounts;
pub mod api;
pub mod applog;
pub mod catalog;
pub mod importer;
pub mod auth;
pub mod instances;
pub mod launch;
pub mod loaders;
pub mod mojang;
pub mod mods;
pub mod net;
pub mod paths;
pub mod room;
pub mod server;
pub mod selfupdate;
pub mod settings;
pub mod shots;
pub mod skins;
pub mod util;
pub mod worlds;

pub const APP_NAME: &str = "Voxel";
pub const APP_VERSION: &str = match option_env!("FL_VERSION") {
    Some(v) if !v.is_empty() => v,
    _ => "3.0.0",
};
