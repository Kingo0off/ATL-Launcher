// В релизе окно консоли не открывается.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use friendlauncher_core::{api::Api, paths, server, APP_NAME};
use std::sync::Arc;
use tauri::{WebviewUrl, WebviewWindowBuilder};

fn main() {
    let api = Arc::new(Api::new(paths::data_dir()));
    let api_keep = Arc::clone(&api);
    let running = match server::start(api) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {}", APP_NAME, e);
            std::process::exit(1);
        }
    };
    let url = running.url.clone();
    tauri::Builder::default()
        .setup(move |app| {
            let parsed = url.parse().expect("адрес интерфейса");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(parsed))
                .title(APP_NAME)
                .inner_size(1280.0, 820.0)
                .min_inner_size(900.0, 600.0)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("не удалось создать окно")
        .run(move |app, event| {
            // Пока идёт игра, лаунчер не закрываем: иначе потеряется её журнал.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() && api_keep.any_running() {
                    api.prevent_exit();
                    let (handle, a) = (app.clone(), Arc::clone(&api_keep));
                    std::thread::spawn(move || {
                        while a.any_running() {
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        handle.exit(0);
                    });
                }
            }
        });
}
