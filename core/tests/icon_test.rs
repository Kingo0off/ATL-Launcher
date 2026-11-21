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
    for ok in ["", "custom", "g:cube:0", "g:pick-axe:12", "g:a_b:3"] {
        assert!(valid_icon(ok), "{}", ok);
    }
    for bad in ["../x", "g:cube", "g:Cube:1", "g::1", "g:cube:x", "g:cube:123", "custom2", "g:cube:1:2", "g:aaaaaaaaaaaaaaaaa:1"] {
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

    // выбор значка из набора
    let r = api.call("instances.update", &json!({"id": id, "icon": "g:gem:4"})).unwrap();
    assert_eq!(r["instance"]["icon"], "g:gem:4");
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

    // возврат к значку из набора удаляет файл
    let r = api.call("instances.update", &json!({"id": id, "icon": "g:cube:0"})).unwrap();
    assert_eq!(r["instance"]["icon"], "g:cube:0");
    assert!(matches!(api.call_out("insticon", &json!({"key": key})).unwrap(), Out::Missing));

    // копия не наследует свою картинку
    api.upload("icon", &json!({"key": key}), &tmp).unwrap();
    let copy = api.call("instances.duplicate", &json!({"key": key, "name": "Копия"})).unwrap();
    assert_eq!(copy["instance"]["icon"], "");
    let _ = std::fs::remove_dir_all(&dir);
}
