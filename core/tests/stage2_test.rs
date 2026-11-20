use friendlauncher_core::{api::Api, server};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-s2-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make_jar(path: &Path, files: &[(&str, &[u8])]) {
    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default();
    for (n, data) in files {
        z.start_file(*n, o).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

fn create(api: &Api, name: &str) -> Value {
    api.call("instances.create", &json!({"name": name, "mc_version": "1.20.1", "loader": {"type": "fabric", "version": "0.15"}}))
        .unwrap()["instance"]
        .clone()
}

#[test]
fn instances_crud() {
    let dir = temp_dir("crud");
    let api = Api::new(dir.clone());
    // проверки ввода
    assert!(api.call("instances.create", &json!({"name": " ", "mc_version": "1.20.1"})).is_err());
    assert!(api.call("instances.create", &json!({"name": "A"})).is_err());
    let e = api.call("instances.create", &json!({"name": "A", "mc_version": "1.20.1", "loader": {"type": "forge"}})).unwrap_err();
    assert!(e.0.contains("версию загрузчика"));
    assert!(api.call("instances.create", &json!({"name": "A", "mc_version": "1.20.1", "loader": {"type": "magic"}})).is_err());
    assert!(api.call("instances.create", &json!({"name": "A", "mc_version": "1.20.1", "ram_mb": 100})).is_err());

    let a = create(&api, "Моя сборка");
    let b = create(&api, "Alpha");
    assert_eq!(a["loader_label"], "Fabric 0.15");
    assert_eq!(a["mods"], 0);
    let id = a["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("pack-"), "кириллица даёт запасное имя: {}", id);
    // состояние: по алфавиту (Alpha раньше «Моя сборка»)
    let st = api.call("state", &json!({})).unwrap();
    let names: Vec<&str> = st["instances"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Alpha", "Моя сборка"]);
    // формат файла совместим с Python-версией
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("local").join(&id).join("instance.json")).unwrap()).unwrap();
    assert_eq!(raw["mc_version"], "1.20.1");
    assert!(raw.get("id").is_none());
    // правка
    let u = api.call("instances.update", &json!({"id": id, "name": " Новое ", "ram_mb": 6000, "loader": {"type": "quilt", "version": "0.2"}, "notes": " заметка "})).unwrap();
    assert_eq!(u["instance"]["name"], "Новое");
    assert_eq!(u["instance"]["loader_label"], "Quilt 0.2");
    assert_eq!(u["instance"]["notes"], "заметка");
    assert!(api.call("instances.update", &json!({"id": id, "name": "  "})).is_err());
    // копия: файлы копируются, журналы и бэкапы нет
    let key = format!("local:{}", id);
    let gdir = dir.join("local").join(&id).join("minecraft");
    std::fs::create_dir_all(gdir.join("config")).unwrap();
    std::fs::write(gdir.join("config").join("a.toml"), "x=1").unwrap();
    std::fs::create_dir_all(gdir.join("logs")).unwrap();
    std::fs::write(gdir.join("logs").join("latest.log"), "log").unwrap();
    let d = api.call("instances.duplicate", &json!({"key": key})).unwrap();
    assert_eq!(d["instance"]["name"], "Новое (копия)");
    let nid = d["instance"]["id"].as_str().unwrap();
    let ng = dir.join("local").join(nid).join("minecraft");
    assert!(ng.join("config").join("a.toml").is_file());
    assert!(!ng.join("logs").exists());
    // удаление, защита от чужих путей
    assert!(api.call("instances.delete", &json!({"id": "../.."})).is_err());
    api.call("instances.delete", &json!({"id": nid})).unwrap();
    assert!(!dir.join("local").join(nid).exists());
    assert!(api.call("instances.update", &json!({"id": "..", "name": "x"})).is_err());
    let _ = b;
}

#[test]
fn mods_list_toggle_delete() {
    let dir = temp_dir("mods");
    let api = Api::new(dir.clone());
    let inst = create(&api, "M");
    let key = format!("local:{}", inst["id"].as_str().unwrap());
    let mdir = dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft").join("mods");
    std::fs::create_dir_all(&mdir).unwrap();
    make_jar(
        &mdir.join("Fabulous.jar"),
        &[
            ("fabric.mod.json", "{\"id\":\"fab\",\"name\":\"Fabulous Mod\",\"version\":\"1.2.3\",\"description\":\"Тест\nмногострочно\",\"icon\":{\"16\":\"a.png\",\"128\":\"icon.png\"}}".as_bytes()),
            ("icon.png", b"PNGDATA"),
        ],
    );
    make_jar(
        &mdir.join("forgey.jar"),
        &[(
            "META-INF/mods.toml",
            b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"forgey\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Forgey\"\ndescription='''Long\ndescription'''\n",
        ), ("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\nImplementation-Version: 9.9\n")],
    );
    make_jar(&mdir.join("quilty.jar"), &[("quilt.mod.json", br#"{"quilt_loader":{"version":"3.0","metadata":{"name":"Quilty","description":"q"}}}"#)]);
    std::fs::write(mdir.join("broken.jar"), b"not a zip").unwrap();
    std::fs::write(mdir.join("readme.txt"), b"x").unwrap();

    let l = api.call("mods.list", &json!({"key": key})).unwrap();
    let mods = l["mods"].as_array().unwrap();
    assert_eq!(mods.len(), 4, "{:?}", mods);
    let by = |f: &str| mods.iter().find(|m| m["file"] == f).unwrap().clone();
    let fab = by("Fabulous.jar");
    assert_eq!((fab["name"].as_str(), fab["version"].as_str(), fab["icon"].as_bool()), (Some("Fabulous Mod"), Some("1.2.3"), Some(true)));
    assert!(fab["description"].as_str().unwrap().contains("многострочно"), "перевод строки внутри JSON допустим");
    let forge = by("forgey.jar");
    assert_eq!((forge["name"].as_str(), forge["version"].as_str()), (Some("Forgey"), Some("9.9")));
    assert_eq!(forge["description"], "Long\ndescription");
    assert_eq!(by("quilty.jar")["version"], "3.0");
    assert_eq!(by("broken.jar")["name"], "broken", "битый файл не ломает список");
    // иконка
    let out = api.call_out("modicon", &json!({"key": key, "file": "Fabulous.jar"})).unwrap();
    match out {
        friendlauncher_core::api::Out::Image(b, t) => assert_eq!((b.as_slice(), t), (&b"PNGDATA"[..], "image/png")),
        _ => panic!("ожидалась иконка"),
    }
    assert!(api.call_out("modicon", &json!({"key": key, "file": "../x.jar"})).is_err());
    // выключить / включить
    let t = api.call("mods.toggle", &json!({"key": key, "file": "Fabulous.jar"})).unwrap();
    assert_eq!(t["file"], "Fabulous.jar.disabled");
    assert!(mdir.join("Fabulous.jar.disabled").is_file());
    let l = api.call("mods.list", &json!({"key": key})).unwrap();
    let dis = l["mods"].as_array().unwrap().iter().find(|m| m["file"] == "Fabulous.jar.disabled").unwrap().clone();
    assert_eq!(dis["enabled"], false);
    assert_eq!(api.call("mods.toggle", &json!({"key": key, "file": "Fabulous.jar.disabled"})).unwrap()["file"], "Fabulous.jar");
    assert!(api.call("mods.toggle", &json!({"key": key, "file": "../evil.jar"})).is_err());
    // счётчик в состоянии
    let st = api.call("state", &json!({})).unwrap();
    assert_eq!(st["instances"][0]["mods"], 4);
    // удаление
    api.call("mods.delete", &json!({"key": key, "files": ["forgey.jar", "quilty.jar"]})).unwrap();
    assert!(!mdir.join("forgey.jar").exists() && mdir.join("Fabulous.jar").exists());
    assert!(api.call("mods.delete", &json!({"key": key, "files": ["../../instance.json"]})).is_err());
}

#[test]
fn worlds_and_backup() {
    let dir = temp_dir("worlds");
    let api = Api::new(dir.clone());
    let inst = create(&api, "W");
    let key = format!("local:{}", inst["id"].as_str().unwrap());
    let g = dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft");
    let w = g.join("saves").join("Мир 1");
    std::fs::create_dir_all(w.join("region")).unwrap();
    std::fs::write(w.join("level.dat"), b"lvl").unwrap();
    std::fs::write(w.join("region").join("r.0.0.mca"), vec![7u8; 5000]).unwrap();
    std::fs::write(w.join("session.lock"), b"lock").unwrap();
    std::fs::create_dir_all(g.join("saves").join("не мир")).unwrap();
    let l = api.call("worlds.list", &json!({"key": key})).unwrap();
    assert_eq!(l["worlds"].as_array().unwrap().len(), 1);
    assert_eq!(l["worlds"][0]["name"], "Мир 1");
    assert!(l["worlds"][0]["size"].as_u64().unwrap() >= 5000);
    let b = api.call("worlds.backup", &json!({"key": key, "name": "Мир 1"})).unwrap();
    let zp = PathBuf::from(b["path"].as_str().unwrap());
    assert!(zp.is_file() && zp.starts_with(g.join("backups")));
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zp).unwrap()).unwrap();
    let names: Vec<String> = z.file_names().map(String::from).collect();
    assert!(names.contains(&"Мир 1/level.dat".to_string()), "{:?}", names);
    assert!(names.contains(&"Мир 1/region/r.0.0.mca".to_string()));
    assert!(!names.iter().any(|n| n.contains("session.lock")));
    let mut data = Vec::new();
    z.by_name("Мир 1/region/r.0.0.mca").unwrap().read_to_end(&mut data).unwrap();
    assert_eq!(data.len(), 5000);
    assert!(api.call("worlds.backup", &json!({"key": key, "name": "../x"})).is_err());
    assert!(api.call("worlds.delete", &json!({"key": key, "name": "нет такого"})).is_err());
    api.call("worlds.delete", &json!({"key": key, "name": "Мир 1"})).unwrap();
    assert!(!w.exists());
}

#[test]
fn screenshots() {
    let dir = temp_dir("shots");
    let api = Api::new(dir.clone());
    let a = create(&api, "Alpha");
    let b = create(&api, "Beta");
    for (inst, files) in [(&a, vec!["a1.png", "a2.png"]), (&b, vec!["b1.png"])] {
        let sd = dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft").join("screenshots");
        std::fs::create_dir_all(&sd).unwrap();
        for f in files {
            std::fs::write(sd.join(f), format!("img-{}", f)).unwrap();
        }
        std::fs::write(sd.join("note.txt"), "x").unwrap();
    }
    let l = api.call("shots.list", &json!({})).unwrap();
    let shots = l["shots"].as_array().unwrap();
    assert_eq!(shots.len(), 3);
    assert!(shots.iter().all(|s| s["inst"] == "Alpha" || s["inst"] == "Beta"));
    let key = format!("local:{}", a["id"].as_str().unwrap());
    match api.call_out("shot", &json!({"key": key, "file": "a1.png"})).unwrap() {
        friendlauncher_core::api::Out::Image(bytes, t) => assert_eq!((bytes, t), (b"img-a1.png".to_vec(), "image/png")),
        _ => panic!("нет картинки"),
    }
    assert!(api.call_out("shot", &json!({"key": key, "file": "../../instance.json"})).is_err());
    assert!(api.call_out("shot", &json!({"key": key, "file": "note.txt"})).is_err());
    api.call("shots.delete", &json!({"key": key, "file": "a1.png"})).unwrap();
    assert_eq!(api.call("shots.list", &json!({})).unwrap()["shots"].as_array().unwrap().len(), 2);
}

fn http(port: u16, method: &str, path: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let head = format!("{} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", method, path, port, body.len());
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(body).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(raw.len());
    let code: u16 = String::from_utf8_lossy(&raw[..split]).split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, raw[(split + 4).min(raw.len())..].to_vec())
}

#[test]
fn http_upload_and_images() {
    let dir = temp_dir("http");
    let api = Arc::new(Api::new(dir.clone()));
    let inst = create(&api, "U");
    let key = format!("local:{}", inst["id"].as_str().unwrap());
    let run = server::start(api.clone()).unwrap();
    let t = &run.token;
    // загрузка мода
    let q = format!("/{}/upload/content?key={}&folder=mods&name=my%20mod.jar", t, key);
    let (c, b) = http(run.port, "POST", &q, b"JARDATA");
    assert_eq!(c, 200, "{}", String::from_utf8_lossy(&b));
    let saved = dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft").join("mods").join("my mod.jar");
    assert_eq!(std::fs::read(&saved).unwrap(), b"JARDATA");
    assert_eq!(std::fs::read_dir(dir.join("tmp")).unwrap().count(), 0, "временные файлы удаляются");
    // неверное расширение, неверный путь, пустой файл, неизвестный тип
    let (c, _) = http(run.port, "POST", &format!("/{}/upload/content?key={}&folder=mods&name=x.txt", t, key), b"1");
    assert_eq!(c, 400);
    let (c, _) = http(run.port, "POST", &format!("/{}/upload/content?key={}&folder=mods&name=..%2Fevil.jar", t, key), b"1");
    assert!(c == 400 || c == 200);
    assert!(!dir.join("local").join(inst["id"].as_str().unwrap()).join("evil.jar").exists());
    assert!(!dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft").join("evil.jar").exists());
    let (c, _) = http(run.port, "POST", &format!("/{}/upload/content?key={}&name=a.jar", t, key), b"");
    assert_eq!(c, 413);
    let (c, _) = http(run.port, "POST", &format!("/{}/upload/nope?name=a", t), b"1");
    assert_eq!(c, 404);
    let (c, _) = http(run.port, "GET", &format!("/{}/upload/content", t), b"");
    assert_eq!(c, 405);
    // картинка скриншота по HTTP, с русским именем сборки в ключе не нужна; имя файла с пробелом
    let sd = dir.join("local").join(inst["id"].as_str().unwrap()).join("minecraft").join("screenshots");
    std::fs::create_dir_all(&sd).unwrap();
    std::fs::write(sd.join("2024-01-01 12.00.00.png"), b"PNGBYTES").unwrap();
    let (c, body) = http(run.port, "GET", &format!("/{}/api/shot?key={}&file=2024-01-01%2012.00.00.png", t, key), b"");
    assert_eq!((c, body.as_slice()), (200, &b"PNGBYTES"[..]));
    let (c, _) = http(run.port, "GET", &format!("/{}/api/shot?key={}&file=nope.png", t, key), b"");
    assert_eq!(c, 404);
}
