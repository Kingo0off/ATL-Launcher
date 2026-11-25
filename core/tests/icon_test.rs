use friendlauncher_core::api::{valid_icon, Api, Out};
use serde_json::{json, Value};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-icon-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Минимальный заголовок PNG нужного размера (для проверки загрузки).
fn fake_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    v
}

fn create(api: &Api) -> Value {
    api.call("instances.create", &json!({"name": "Икона", "mc_version": "1.20.1"})).unwrap()["instance"].clone()
}

#[test]
fn icon_values_are_checked() {
    for ok in ["", "custom"] {
        assert!(valid_icon(ok), "{}", ok);
    }
    for bad in ["../x", "g:cube:0", "custom2", " custom"] {
        assert!(!valid_icon(bad), "{}", bad);
    }
}

#[test]
fn custom_icon_upload_select_and_reset() {
    let dir = temp_dir("flow");
    let api = Api::new(dir.clone());
    let inst = create(&api);
    let id = inst["id"].as_str().unwrap().to_string();
    let key = format!("local:{}", id);
    assert_eq!(inst["icon"], "");

    // выбор пиксельного значка из списка (меняется зерно)
    let r = api.call("instances.update", &json!({"id": id, "icon": "", "icon_seed": "v7"})).unwrap();
    assert_eq!(r["instance"]["icon"], "");
    assert_eq!(r["instance"]["icon_seed"], "v7");
    // чужие значения и «custom» без файла отклоняются
    assert!(api.call("instances.update", &json!({"id": id, "icon": "../x"})).is_err());
    assert!(api.call("instances.update", &json!({"id": id, "icon": "custom"})).is_err());

    // своя картинка: не PNG и слишком большая отклоняются
    let tmp = dir.join("up.tmp");
    std::fs::write(&tmp, b"not a png at all, just text").unwrap();
    assert!(api.upload("icon", &json!({"key": key}), &tmp).is_err());
    std::fs::write(&tmp, fake_png(4000, 4000)).unwrap();
    assert!(api.upload("icon", &json!({"key": key}), &tmp).is_err());

    // нормальная
    let png = fake_png(256, 256);
    std::fs::write(&tmp, &png).unwrap();
    let r = api.upload("icon", &json!({"key": key}), &tmp).unwrap();
    assert_eq!(r["instance"]["icon"], "custom");
    assert!(r["instance"]["icon_v"].as_u64().unwrap() > 0);
    match api.call_out("insticon", &json!({"key": key})).unwrap() {
        Out::Image(b, t) => assert_eq!((b, t), (png.clone(), "image/png")),
        _ => panic!("нет картинки"),
    }
    let items = api.call("state", &json!({})).unwrap();
    let got = items["instances"].as_array().unwrap().iter().find(|i| i["id"] == id.as_str()).unwrap().clone();
    assert_eq!(got["icon"], "custom");

    // возврат к пиксельному значку удаляет файл
    let r = api.call("instances.update", &json!({"id": id, "icon": "", "icon_seed": "v9"})).unwrap();
    assert_eq!(r["instance"]["icon"], "");
    assert!(matches!(api.call_out("insticon", &json!({"key": key})).unwrap(), Out::Missing));

    // копия не наследует свою картинку
    api.upload("icon", &json!({"key": key}), &tmp).unwrap();
    let copy = api.call("instances.duplicate", &json!({"key": key, "name": "Копия"})).unwrap();
    assert_eq!(copy["instance"]["icon"], "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn catalog_icon_hosts_and_formats_are_restricted() {
    use friendlauncher_core::api::{catalog_icon_type, catalog_icon_url_ok};
    assert!(catalog_icon_url_ok("https://cdn.modrinth.com/data/a.png"));
    assert!(catalog_icon_url_ok("https://media.forgecdn.net/avatars/a.jpg"));
    for bad in ["http://cdn.modrinth.com/a", "https://cdn.modrinth.com.evil.test/a", "https://cdn.modrinth.com@evil.test/a", "https://127.0.0.1/a"] {
        assert!(!catalog_icon_url_ok(bad), "{}", bad);
    }
    assert_eq!(catalog_icon_type(&fake_png(32, 32)), Some("image/png"));
    assert_eq!(catalog_icon_type(&[0xff, 0xd8, 0xff, 0]), Some("image/jpeg"));
    assert_eq!(catalog_icon_type(b"RIFF1234WEBP"), Some("image/webp"));
    assert_eq!(catalog_icon_type(b"<html>"), None);
}

#[test]
fn downloaded_jpeg_icon_is_served_as_jpeg() {
    let dir = temp_dir("catalog-jpeg");
    let api = Api::new(dir.clone());
    let inst = create(&api);
    let id = inst["id"].as_str().unwrap();
    let key = format!("local:{}", id);
    let jpeg = vec![0xff, 0xd8, 0xff, 0xd9];
    std::fs::write(dir.join("local").join(id).join("icon.png"), &jpeg).unwrap();
    api.call("instances.update", &json!({"id": id, "icon": "custom"})).unwrap();
    match api.call_out("insticon", &json!({"key": key})).unwrap() {
        Out::Image(bytes, mime) => assert_eq!((bytes, mime), (jpeg, "image/jpeg")),
        _ => panic!("нет картинки"),
    }
}
