use friendlauncher_core::loaders;
use friendlauncher_core::mojang::{self, Mojang};
use friendlauncher_core::net::{download, no_progress, Endpoints};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
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
            let url = rq.url().to_string();
            match files.get(&url) {
                Some(b) => {
                    let _ = rq.respond(tiny_http::Response::from_data(b.clone()));
                }
                None => {
                    let _ = rq.respond(tiny_http::Response::from_string("no").with_status_code(404));
                }
            }
        }
    });
    base
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s3_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn version_lists() {
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/manifest".into(), json!({"versions":[{"id":"1.21","type":"release"},{"id":"24w01a","type":"snapshot"},{"id":"1.20.1","type":"release"}]}).to_string().into_bytes());
    f.insert("/fabric/versions/loader/1.20.1".into(), json!([{"loader":{"version":"0.15.0-beta.1"}},{"loader":{"version":"0.14.9"}}]).to_string().into_bytes());
    f.insert("/forge.xml".into(), b"<metadata><versions><version>1.20.1-47.0.1</version><version>1.20.1-47.1.0</version><version>1.19-41.0.0</version></versions></metadata>".to_vec());
    f.insert("/neo_legacy.xml".into(), b"<metadata><versions><version>1.20.1-47.1.0</version><version>1.20.1-47.1.5</version><version>1.19-41.0.0</version></versions></metadata>".to_vec());
    f.insert("/neo.xml".into(), b"<metadata><versions><version>20.4.1</version><version>20.4.2-beta</version><version>20.4.3</version><version>21.0.5</version><version>26.1.0.0-beta</version><version>26.1.0.1-beta</version><version>26.1.0.0-alpha.2+snapshot-1</version></versions></metadata>".to_vec());
    let base = mock(f);
    let ep = Endpoints {
        manifest: format!("{}/manifest", base),
        fabric: format!("{}/fabric", base),
        forge_meta: format!("{}/forge.xml", base),
        neoforge_meta: format!("{}/neo.xml", base),
        neoforge_legacy_meta: format!("{}/neo_legacy.xml", base),
        ..Endpoints::default()
    };
    assert_eq!(mojang::mc_versions(&ep, false).unwrap(), vec!["1.21", "1.20.1"]);
    assert_eq!(mojang::mc_versions(&ep, true).unwrap().len(), 3);
    assert_eq!(loaders::loader_versions(&ep, "fabric", "1.20.1").unwrap(), vec!["0.14.9", "0.15.0-beta.1"]);
    assert_eq!(loaders::loader_versions(&ep, "forge", "1.20.1").unwrap(), vec!["47.1.0", "47.0.1"]);
    assert_eq!(loaders::loader_versions(&ep, "neoforge", "1.20.1").unwrap(), vec!["47.1.5", "47.1.0"]);
    assert_eq!(loaders::loader_versions(&ep, "neoforge", "1.20.4").unwrap(), vec!["20.4.3", "20.4.1", "20.4.2-beta"]);
    assert_eq!(loaders::loader_versions(&ep, "neoforge", "1.21").unwrap(), vec!["21.0.5"]);
    assert_eq!(loaders::loader_versions(&ep, "neoforge", "26.1").unwrap(), vec!["26.1.0.1-beta", "26.1.0.0-beta"]);
    assert_eq!(loaders::loader_versions(&ep, "neoforge", "26.1-snapshot-1").unwrap(), vec!["26.1.0.0-alpha.2+snapshot-1"]);
    assert!(loaders::loader_versions(&ep, "vanilla", "1.20.1").unwrap().is_empty());
}

fn native_zip() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("lwjgl.dll", o).unwrap();
        z.write_all(b"dll").unwrap();
        z.start_file("META-INF/x.txt", o).unwrap();
        z.write_all(b"m").unwrap();
        z.start_file("../evil.dll", o).unwrap();
        z.write_all(b"evil").unwrap();
        z.finish().unwrap();
    }
    buf.into_inner()
}

#[test]
fn game_files_full_install() {
    let jar = b"clientjar".to_vec();
    let lib = b"libjar".to_vec();
    let nat = native_zip();
    let asset = b"assetdata".to_vec();
    let ah = sha(&asset);
    let idx = json!({"objects":{"icons/a.png":{"hash":ah,"size":asset.len()}},"virtual":true}).to_string().into_bytes();
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/client.jar".into(), jar.clone());
    f.insert("/lib.jar".into(), lib.clone());
    f.insert("/nat.jar".into(), nat.clone());
    f.insert("/idx.json".into(), idx.clone());
    f.insert(format!("/assets/{}/{}", &ah[..2], ah), asset.clone());
    let base = mock(f);
    let ver = json!({
        "id":"9.9","assetIndex":{"id":"9","url":format!("{}/idx.json",base),"sha1":sha(&idx)},
        "downloads":{"client":{"url":format!("{}/client.jar",base),"sha1":sha(&jar),"size":jar.len()}},
        "libraries":[
            {"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar","url":format!("{}/lib.jar",base),"sha1":sha(&lib),"size":lib.len()}}},
            {"name":"skip:me:1","rules":[{"action":"allow","os":{"name":"nonexistent-os"}}],"downloads":{"artifact":{"path":"s/m.jar","url":format!("{}/none",base)}}},
            {"name":"n:n:1","natives":{"windows":"nat","linux":"nat","osx":"nat"},"downloads":{"classifiers":{"nat":{"path":"n/n-nat.jar","url":format!("{}/nat.jar",base),"sha1":sha(&nat),"size":nat.len()}}}}
        ],
        "mainClass":"M","minecraftArguments":"--username ${auth_player_name} --gameDir ${game_directory}"
    });
    let root = tmp("root");
    let m = Mojang::new(root.clone(), Endpoints { asset: format!("{}/assets", base), ..Endpoints::default() });
    std::fs::create_dir_all(root.join("versions/9.9")).unwrap();
    std::fs::write(root.join("versions/9.9/9.9.json"), ver.to_string()).unwrap();
    let game = tmp("game");
    let g = m.install_game_files("9.9", &game, &no_progress).unwrap();
    assert!(g.jar.is_file());
    assert!(g.classpath.contains("b-1.jar") && g.classpath.contains("9.9.jar"));
    assert!(!g.classpath.contains("m.jar"));
    assert!(g.natives_dir.join("lwjgl.dll").is_file());
    assert!(!g.natives_dir.join("META-INF").exists());
    assert!(!root.join("natives/evil.dll").exists() && !g.natives_dir.join("evil.dll").exists());
    assert!(root.join("assets/objects").join(&ah[..2]).join(&ah).is_file());
    assert!(root.join("assets/virtual/9/icons/a.png").is_file());
    // повторная установка ничего не ломает
    m.install_game_files("9.9", &game, &no_progress).unwrap();
}

#[test]
fn sha_mismatch_fails_and_leaves_no_file() {
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/x".into(), b"hello".to_vec());
    let base = mock(f);
    let d = tmp("sha");
    let dest = d.join("x.bin");
    let bad = "0".repeat(40);
    let r = download(&format!("{}/x", base), &dest, Some(&bad), None, 1);
    assert!(r.is_err());
    assert!(!dest.exists());
    assert!(!d.join("x.bin.part").exists());
    assert_eq!(download(&format!("{}/x", base), &dest, Some(&sha(b"hello")), None, 1).unwrap(), true);
    assert_eq!(download(&format!("{}/x", base), &dest, Some(&sha(b"hello")), None, 1).unwrap(), false);
    assert!(download(&format!("{}/missing", base), &d.join("y"), None, None, 1).is_err());
}

#[test]
fn inherits_and_args() {
    let m = Mojang::new(tmp("inh"), Endpoints::default());
    let vd = m.versions();
    std::fs::create_dir_all(vd.join("1.20")).unwrap();
    std::fs::create_dir_all(vd.join("fab")).unwrap();
    std::fs::write(vd.join("1.20/1.20.json"), json!({
        "id":"1.20","mainClass":"vanilla",
        "libraries":[{"name":"x:y:1"},{"name":"x:z:1"}],
        "arguments":{"game":["--a",{"rules":[{"action":"allow","features":{"demo":true}}],"value":"--demo"}],"jvm":["-Dx=${natives_directory}"]}
    }).to_string()).unwrap();
    std::fs::write(vd.join("fab/fab.json"), json!({
        "id":"fab","inheritsFrom":"1.20","mainClass":"knot",
        "libraries":[{"name":"x:y:2"}],
        "arguments":{"game":["--b"],"jvm":[]}
    }).to_string()).unwrap();
    let v: Value = m.load_version("fab").unwrap();
    assert_eq!(v["mainClass"], "knot");
    assert_eq!(v["_jar_id"], "1.20");
    let names: Vec<&str> = v["libraries"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["x:y:2", "x:z:1"]);
    let mut vars = serde_json::Map::new();
    vars.insert("natives_directory".into(), json!("/n"));
    vars.insert("classpath".into(), json!("cp"));
    let (jvm, game) = mojang::build_arguments(&v, &vars);
    assert_eq!(game, vec!["--a", "--b"]);
    assert!(jvm.contains(&"-Dx=/n".to_string()));
    assert!(jvm.contains(&"-cp".to_string()) && jvm.contains(&"-Djava.library.path=/n".to_string()));
    assert!(m.load_version("../evil").is_err());
    assert_eq!(mojang::maven_path("net.fabricmc:fabric-loader:0.14.9"), "net/fabricmc/fabric-loader/0.14.9/fabric-loader-0.14.9.jar");
}
