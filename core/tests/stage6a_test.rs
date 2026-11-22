use friendlauncher_core::catalog;
use friendlauncher_core::importer;
use friendlauncher_core::instances::{load_track, Store};
use friendlauncher_core::net::{no_progress, sha1_file, Endpoints};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

fn sha(b: &[u8]) -> String {
    Sha1::digest(b).iter().map(|x| format!("{:02x}", x)).collect()
}

fn mock(files: HashMap<String, Vec<u8>>) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", server.server_addr().to_ip().unwrap().port());
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for rq in server.incoming_requests() {
            let url = rq.url().split('?').next().unwrap_or("").to_string();
            let _ = match files.get(&url) {
                Some(b) => rq.respond(tiny_http::Response::from_data(b.clone())),
                None => rq.respond(tiny_http::Response::from_string("{}").with_status_code(404)),
            };
        }
    });
    base
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s6a_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn j(v: Value) -> Vec<u8> {
    v.to_string().into_bytes()
}

#[test]
fn modrinth_search_install_with_deps_and_updates() {
    let (a, b, a2) = (b"mod-a".to_vec(), b"mod-b".to_vec(), b"mod-a-new".to_vec());
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/files/a.jar".into(), a.clone());
    f.insert("/files/b.jar".into(), b.clone());
    f.insert("/files/a2.jar".into(), a2.clone());
    let base = mock(f.clone());
    let ver = |id: &str, pid: &str, file: &str, data: &[u8], deps: Value| {
        json!({"id": id, "project_id": pid, "version_type": "release", "version_number": "1.0",
          "files": [{"primary": true, "filename": file, "url": format!("{}/files/{}", base, file), "hashes": {"sha1": sha(data)}}], "dependencies": deps})
    };
    f.insert("/mr/search".into(), j(json!({"hits":[{"project_id":"PA","slug":"a","title":"A","description":"d","downloads":5,"follows":1,"author":"me","categories":["x","y"],"project_type":"mod"}],"total_hits":1})));
    f.insert("/mr/project/PA/version".into(), j(json!([ver("va", "PA", "a.jar", &a, json!([{"project_id":"PB","dependency_type":"required"},{"project_id":"PX","dependency_type":"optional"}]))])));
    f.insert("/mr/project/PB".into(), j(json!({"title":"B","icon_url":"i","project_type":"mod"})));
    f.insert("/mr/project/PB/version".into(), j(json!([ver("vb", "PB", "b.jar", &b, json!([]))])));
    let new_a = json!({"id":"va2","project_id":"PA","version_number":"2.0","files":[{"primary":true,"filename":"a2.jar","url":format!("{}/files/a2.jar", base),"hashes":{"sha1":sha(&a2)}}]});
    // ответ обновления ключуется старым хешем файла a.jar
    f.insert("/mr/version_files/update".into(), j(json!({ sha(&a): new_a })));
    let base2 = mock(f);
    let ep = Endpoints { modrinth: format!("{}/mr", base2), ..Endpoints::default() };

    let s = catalog::mr_search(&ep, "a", "mod", "1.20.1", "fabric", &["adventure".to_string()], "relevance", 0, 20).unwrap();
    assert_eq!(s["hits"][0]["title"], "A");
    assert_eq!(s["total"], 1);

    let gdir = tmp("mr");
    let installed = catalog::mr_install(&ep, &gdir, "1.20.1", "fabric", "PA", "mod", &json!({"title":"A"}), true, &mut HashSet::new(), 0).unwrap();
    assert_eq!(installed, vec!["mods/a.jar", "mods/b.jar"]);
    assert_eq!(std::fs::read(gdir.join("mods/b.jar")).unwrap(), b);
    let track = load_track(&gdir);
    assert_eq!(track["mods/a.jar"]["project_id"], "PA");
    assert_eq!(track["mods/b.jar"]["title"], "B");

    let ups = catalog::mr_check_updates(&ep, &gdir, "1.20.1", "fabric").unwrap();
    assert_eq!(ups.len(), 1);
    assert_eq!(ups[0]["file"], "a.jar");
    catalog::mr_apply_update(&gdir, &ups[0]).unwrap();
    assert!(!gdir.join("mods/a.jar").exists());
    assert_eq!(std::fs::read(gdir.join("mods/a2.jar")).unwrap(), a2);
    assert_eq!(load_track(&gdir)["mods/a2.jar"]["version_number"], "2.0");
    // подделка пути в обновлении
    let mut bad = ups[0].clone();
    bad["filename"] = json!("../evil.jar");
    assert!(catalog::mr_apply_update(&gdir, &bad).is_err() || !gdir.join("evil.jar").exists());
}

#[test]
fn curseforge_key_search_install_and_blocked() {
    let ok = b"cf-mod".to_vec();
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/dl/ok.jar".into(), ok.clone());
    let base = mock(f.clone());
    f.insert("/cf/mods/search".into(), j(json!({"data":[{"id":7,"name":"Seven","slug":"seven","summary":"s","downloadCount":10.0,"authors":[{"name":"au"}],"categories":[{"name":"c"}],"links":{"websiteUrl":"https://www.curseforge.com/x"},"allowModDistribution":false}],"pagination":{"totalCount":1}})));
    f.insert("/cf/mods/7/files".into(), j(json!({"data":[{"id":70,"fileName":"ok.jar","displayName":"Ok 1","releaseType":1,"fileDate":"2024-01-01","downloadUrl":format!("{}/dl/ok.jar", base),"hashes":[{"algo":1,"value":sha(&ok)}],"dependencies":[]}]})));
    f.insert("/cf/mods/8/files".into(), j(json!({"data":[{"id":80,"fileName":"no.jar","releaseType":1,"fileDate":"2024-01-01","downloadUrl":null,"hashes":[],"dependencies":[]}]})));
    let base2 = mock(f);
    let ep = Endpoints { curseforge: format!("{}/cf", base2), ..Endpoints::default() };
    assert!(catalog::cf_search(&ep, "", "x", "mod", "", "", "relevance", 0, 20).unwrap_err().contains("ключ API"));
    let s = catalog::cf_search(&ep, "key", "x", "mod", "1.20.1", "fabric", "downloads", 0, 20).unwrap();
    assert_eq!(s["hits"][0]["project_id"], "cf:7");
    assert_eq!(s["hits"][0]["blocked"], true);
    let gdir = tmp("cf");
    let got = catalog::cf_install(&ep, "key", &gdir, "1.20.1", "fabric", "cf:7", "mod", &json!({"title":"Seven"}), true, &mut HashSet::new(), 0).unwrap();
    assert_eq!(got, vec!["mods/ok.jar"]);
    assert_eq!(load_track(&gdir)["mods/ok.jar"]["project_id"], "cf:7");
    let err = catalog::cf_install(&ep, "key", &gdir, "1.20.1", "fabric", "cf:8", "mod", &json!({"title":"Eight"}), true, &mut HashSet::new(), 0).unwrap_err();
    assert!(err.contains("запретил"), "{}", err);
}

fn make_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default();
    for (n, d) in entries {
        z.start_file(*n, o).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap();
}

#[test]
fn import_mrpack_trusted_hosts_and_rollback() {
    let data = b"mod-file".to_vec();
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/m.jar".into(), data.clone());
    let base = mock(f);
    let root = tmp("imp");
    let store = Store::new(root.clone());
    let ep = Endpoints { extra_trusted_hosts: vec!["127.0.0.1".into()], ..Endpoints::default() };
    let idx = json!({"game":"minecraft","name":"Pack","summary":"sum","dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0"},
        "files":[{"path":"mods/m.jar","hashes":{"sha1":sha(&data)},"downloads":[format!("{}/m.jar", base)]},
                 {"path":"mods/skip.jar","env":{"client":"unsupported"},"downloads":["https://evil.example.com/x.jar"]}]});
    let zp = root.join("p.mrpack");
    make_zip(&zp, &[("modrinth.index.json", idx.to_string().as_bytes()), ("overrides/config/a.txt", b"A"), ("client-overrides/config/b.txt", b"B")]);
    let inst = importer::import_archive(&store, &ep, "", &zp, &no_progress, &|_| {}).unwrap();
    assert_eq!(inst["name"], "Pack");
    assert_eq!(inst["loader"]["type"], "fabric");
    let g = store.game_dir(inst["id"].as_str().unwrap());
    assert_eq!(std::fs::read(g.join("mods/m.jar")).unwrap(), data);
    assert!(g.join("config/a.txt").is_file() && g.join("config/b.txt").is_file());
    assert!(!g.join("mods/skip.jar").exists());

    // недоверенный хост (без extra_trusted)
    let plain = Endpoints::default();
    let before = store.list().len();
    let e = importer::import_archive(&store, &plain, "", &zp, &no_progress, &|_| {}).unwrap_err();
    assert!(e.contains("недоверенный"), "{}", e);
    assert_eq!(store.list().len(), before);

    // выход за папку в overrides: откат
    let idx2 = json!({"dependencies":{"minecraft":"1.20.1"},"files":[]});
    let zp2 = root.join("evil.mrpack");
    make_zip(&zp2, &[("modrinth.index.json", idx2.to_string().as_bytes()), ("overrides/../../evil.txt", b"x")]);
    assert!(importer::import_archive(&store, &ep, "", &zp2, &no_progress, &|_| {}).is_err());
    assert_eq!(store.list().len(), before);
    assert!(!root.join("local").join("evil.txt").exists());
}

#[test]
fn export_import_roundtrip_and_prism() {
    let root = tmp("exp");
    let store = Store::new(root.clone());
    let inst = store.save(&json!({"name":"Моя: сборка","mc_version":"1.20.1","loader":{"type":"quilt","version":"0.20"},"publish":{"id":"x"}})).unwrap();
    let g = store.game_dir(inst["id"].as_str().unwrap());
    std::fs::create_dir_all(g.join("mods")).unwrap();
    std::fs::create_dir_all(g.join("saves/W")).unwrap();
    std::fs::create_dir_all(g.join("logs")).unwrap();
    std::fs::write(g.join("mods/a.jar"), b"A").unwrap();
    std::fs::write(g.join("saves/W/level.dat"), b"L").unwrap();
    std::fs::write(g.join("logs/latest.log"), b"log").unwrap();
    std::fs::write(g.join("options.txt"), b"o").unwrap();
    let ep = Endpoints::default();
    let z1 = importer::export_zip(&root, &inst, &g, false).unwrap();
    assert!(z1.file_name().unwrap().to_string_lossy().starts_with("Моя_ сборка-"));
    let a = importer::import_archive(&store, &ep, "", &z1, &no_progress, &|_| {}).unwrap();
    let ga = store.game_dir(a["id"].as_str().unwrap());
    assert!(ga.join("mods/a.jar").is_file() && ga.join("options.txt").is_file());
    assert!(!ga.join("saves").exists() && !ga.join("logs").exists());
    assert_eq!(a["loader"]["version"], "0.20");
    assert!(a.get("publish").is_none());
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let z2 = importer::export_zip(&root, &inst, &g, true).unwrap();
    let b = importer::import_archive(&store, &ep, "", &z2, &no_progress, &|_| {}).unwrap();
    assert!(store.game_dir(b["id"].as_str().unwrap()).join("saves/W/level.dat").is_file());

    // Prism
    let pz = root.join("Prism Pack.zip");
    make_zip(&pz, &[
        ("Prism/mmc-pack.json", json!({"components":[{"uid":"net.minecraft","version":"1.19.2"},{"uid":"net.minecraftforge","version":"43.2.0"}]}).to_string().as_bytes()),
        ("Prism/instance.cfg", "[General]\nname=Из Призма\n".as_bytes()),
        ("Prism/.minecraft/mods/p.jar", b"P"),
    ]);
    let p = importer::import_archive(&store, &ep, "", &pz, &no_progress, &|_| {}).unwrap();
    assert_eq!(p["name"], "Из Призма");
    assert_eq!(p["mc_version"], "1.19.2");
    assert_eq!(p["loader"]["type"], "forge");
    assert!(store.game_dir(p["id"].as_str().unwrap()).join("mods/p.jar").is_file());
    let junk = root.join("junk.zip");
    make_zip(&junk, &[("hello.txt", b"x")]);
    assert!(importer::import_archive(&store, &ep, "", &junk, &no_progress, &|_| {}).unwrap_err().contains("распознать"));
    let _ = sha1_file(&z1);
}

#[test]
fn import_curseforge_pack_with_blocked_file() {
    let ok = b"cf-ok".to_vec();
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/dl/ok.jar".into(), ok.clone());
    let base = mock(f.clone());
    f.insert("/cf/mods/files".into(), j(json!({"data":[
        {"id":1,"fileName":"ok.jar","downloadUrl":format!("{}/dl/ok.jar", base),"hashes":[{"algo":1,"value":sha(&ok)}]},
        {"id":2,"fileName":"blocked.jar","downloadUrl":null,"hashes":[]}]})));
    f.insert("/cf/mods".into(), j(json!({"data":[
        {"id":10,"name":"Ok mod","classId":6,"logo":{"thumbnailUrl":"t"},"links":{"websiteUrl":"u1"}},
        {"id":20,"name":"Blocked mod","classId":6,"links":{"websiteUrl":"https://www.curseforge.com/b"}}]})));
    let base2 = mock(f);
    let ep = Endpoints { curseforge: format!("{}/cf", base2), ..Endpoints::default() };
    let root = tmp("cfimp");
    let store = Store::new(root.clone());
    let man = json!({"name":"CF Pack","author":"me","minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.1.0","primary":true}]},
        "files":[{"projectID":10,"fileID":1,"required":true},{"projectID":20,"fileID":2,"required":true}],"overrides":"overrides"});
    let zp = root.join("cf.zip");
    make_zip(&zp, &[("manifest.json", man.to_string().as_bytes()), ("overrides/options.txt", b"o")]);
    let inst = importer::import_archive(&store, &ep, "key", &zp, &no_progress, &|_| {}).unwrap();
    assert_eq!(inst["loader"]["type"], "forge");
    assert_eq!(inst["loader"]["version"], "47.1.0");
    assert_eq!(inst["skipped"][0]["title"], "Blocked mod");
    let g = store.game_dir(inst["id"].as_str().unwrap());
    assert_eq!(std::fs::read(g.join("mods/ok.jar")).unwrap(), ok);
    assert!(g.join("options.txt").is_file());
    assert_eq!(load_track(&g)["mods/ok.jar"]["project_id"], "cf:10");
}
