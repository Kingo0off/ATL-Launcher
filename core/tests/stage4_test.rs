use friendlauncher_core::launch::{self, RunSpec};
use friendlauncher_core::net::{no_progress, Endpoints};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

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
            let _ = match files.get(&url) {
                Some(b) => rq.respond(tiny_http::Response::from_data(b.clone())),
                None => rq.respond(tiny_http::Response::from_string("no").with_status_code(404)),
            };
        }
    });
    base
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("fl_s4_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn split_args_quotes() {
    assert_eq!(launch::split_args("-Da=1  \"-Db=x y\" '-Dc=z'"), vec!["-Da=1", "-Db=x y", "-Dc=z"]);
    assert!(launch::split_args("   ").is_empty());
}

#[test]
fn pump_collects_output_and_code() {
    let exe = std::env::current_exe().unwrap();
    let mut c = launch::command(&exe);
    c.arg("--list").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    let lines = Mutex::new(Vec::new());
    let stop = AtomicBool::new(false);
    let code = launch::pump(&mut child, &|l| lines.lock().unwrap().push(l), &stop);
    assert_eq!(code, 0);
    assert!(lines.lock().unwrap().iter().any(|l| l.contains("pump_collects_output_and_code")));
    let mut c = launch::command(&exe);
    c.arg("--definitely-not-a-flag").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = c.spawn().unwrap();
    assert_ne!(launch::pump(&mut child, &|_| {}, &stop), 0);
}

#[test]
fn java_runtime_download() {
    let java = b"fake java".to_vec();
    let exe_name = if cfg!(windows) { "bin/java.exe" } else { "bin/java" };
    let mut files = serde_json::Map::new();
    files.insert("bin".into(), json!({"type":"directory"}));
    files.insert(exe_name.into(), json!({"type":"file","executable":true,"downloads":{"raw":{"url":"@@/java","sha1":sha(&java),"size":java.len()}}}));
    let manifest = json!({ "files": files });
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/java".into(), java);
    let base = mock(f.clone());
    // манифест содержит адрес сервера, поэтому поднимаем второй сервер после подстановки
    let mf = manifest.to_string().replace("@@", &base);
    f.insert("/mf.json".into(), mf.clone().into_bytes());
    let base2 = mock(f.clone());
    let mut all = serde_json::Map::new();
    all.insert(launch::runtime_platform().into(), json!({"jre-test": [{"manifest":{"sha1":sha(mf.as_bytes()),"url":format!("{}/mf.json",base2)}}]}));
    let all = Value::Object(all);
    f.insert("/all.json".into(), all.to_string().into_bytes());
    let base3 = mock(f);
    let ep = Endpoints { java_all: format!("{}/all.json", base3), ..Endpoints::default() };
    let root = tmp("java");
    let v = json!({"javaVersion":{"component":"jre-test","majorVersion":21}});
    let p = launch::ensure_java(&root, &ep, &v, "", &no_progress, &|_| {}).unwrap();
    assert!(p.is_file());
    // второй раз ничего не качает и возвращает тот же путь
    let p2 = launch::ensure_java(&root, &ep, &v, "", &no_progress, &|_| {}).unwrap();
    assert_eq!(p, p2);
}

#[test]
fn prepare_game_fabric_and_ely() {
    let jar = b"clientjar".to_vec();
    let authlib = b"authlib".to_vec();
    let ah = sha(b"a");
    let idx = json!({"objects":{"x":{"hash":ah,"size":1}}}).to_string().into_bytes();
    let profile = json!({"id":"fabric-loader-0.1-9.9","inheritsFrom":"9.9","mainClass":"knot.Main","libraries":[],"arguments":{"game":["--fab"],"jvm":[]}});
    let mut f: HashMap<String, Vec<u8>> = HashMap::new();
    f.insert("/client.jar".into(), jar.clone());
    f.insert("/idx.json".into(), idx.clone());
    f.insert(format!("/assets/{}/{}", &ah[..2], ah), b"a".to_vec());
    f.insert("/fabric/versions/loader/9.9/0.1/profile/json".into(), profile.to_string().into_bytes());
    f.insert("/authlib.jar".into(), authlib);
    let base = mock(f.clone());
    f.insert("/authlib.json".into(), json!({"download_url": format!("{}/authlib.jar", base)}).to_string().into_bytes());
    let base = mock(f);
    let root = tmp("prep");
    let ver = json!({
        "id":"9.9","type":"release","mainClass":"vanilla.Main",
        "assetIndex":{"id":"9","url":format!("{}/idx.json",base),"sha1":sha(&idx)},
        "downloads":{"client":{"url":format!("{}/client.jar",base),"sha1":sha(&jar),"size":jar.len()}},
        "libraries":[],
        "arguments":{"game":["--username","${auth_player_name}","--version","${version_name}"],"jvm":["-Dnat=${natives_directory}","-cp","${classpath}"]}
    });
    std::fs::create_dir_all(root.join("versions/9.9")).unwrap();
    std::fs::write(root.join("versions/9.9/9.9.json"), ver.to_string()).unwrap();
    let ep = Endpoints {
        fabric: format!("{}/fabric", base),
        asset: format!("{}/assets", base),
        authlib: format!("{}/authlib.json", base),
        ..Endpoints::default()
    };
    let exe = std::env::current_exe().unwrap();
    let spec = RunSpec {
        root: root.clone(),
        ep,
        mc: "9.9".into(),
        loader: json!({"type":"fabric","version":"0.1"}),
        game_dir: tmp("game"),
        account: json!({"type":"ely","name":"Steve","uuid":"abc","access_token":"tok"}),
        user_java: exe.to_string_lossy().to_string(),
        ram_mb: 2048,
        java_args: "-Dextra=\"a b\"".into(),
    };
    let logs = Mutex::new(Vec::<String>::new());
    let cmd = launch::prepare_game(&spec, &no_progress, &|s| logs.lock().unwrap().push(s.to_string())).unwrap();
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    assert!(args.iter().any(|a| a.starts_with("-javaagent:") && a.ends_with("=ely.by")), "{:?}", args);
    assert!(args.contains(&"-Xmx2048M".to_string()));
    assert!(args.contains(&"-Dextra=a b".to_string()));
    let i = args.iter().position(|a| a == "knot.Main").expect("mainClass загрузчика");
    assert_eq!(&args[i + 1..], ["--username", "Steve", "--version", "fabric-loader-0.1-9.9", "--fab"]);
    assert!(root.join("authlib-injector.jar").is_file());
    assert!(root.join("versions/fabric-loader-0.1-9.9/fabric-loader-0.1-9.9.json").is_file());
}

#[test]
fn ensure_fresh_microsoft_expiry() {
    let root = tmp("fresh");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    let good = json!({"type":"ms","name":"A","uuid":"1","access_token":"t","expires_at":now + 3600.0});
    assert_eq!(launch::ensure_fresh(&root, &good).unwrap(), good);
    let old: Value = json!({"type":"ms","name":"A","uuid":"1","access_token":"t","expires_at":1.0});
    assert!(launch::ensure_fresh(&root, &old).is_err());
}
