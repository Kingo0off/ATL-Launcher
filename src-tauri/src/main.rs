// В релизе окно консоли не открывается.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use friendlauncher_core::{api::Api, applog, paths, server, APP_NAME, APP_VERSION};
use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const WEBVIEW2_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

fn main() {
    let dir = paths::data_dir();
    applog::install_panic_hook(dir.clone());

    // Окно строится на WebView2 (в Windows 11 он уже есть). Если его нет, открываем страницу загрузки.
    if tauri::webview_version().is_err() {
        applog::log(&dir, "WebView2 не найден, открываю страницу загрузки");
        friendlauncher_core::util::open_url(WEBVIEW2_URL);
        std::process::exit(1);
    }

    let api = Arc::new(Api::new(dir.clone()));
    let api_keep = Arc::clone(&api);
    let running = match server::start(api) {
        Ok(r) => r,
        Err(e) => {
            applog::log(&dir, &format!("Не удалось запустить сервер: {}", e));
            eprintln!("{}: {}", APP_NAME, e);
            std::process::exit(1);
        }
    };
    applog::log(&dir, &format!("{} {} запущен, порт {}", APP_NAME, APP_VERSION, running.port));
    let url = running.url.clone();
    tauri::Builder::default()
        // Второй запуск не открывает вторую копию, а показывает уже открытое окно.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
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
