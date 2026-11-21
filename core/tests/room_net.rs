//! Сетевой тест через настоящие серверы iroh (нужен интернет), поэтому по умолчанию отключён:
//! cargo test -p friendlauncher-core --test room_net -- --ignored
use friendlauncher_core::room;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn detect_port_from_log() {
    let lines: Vec<String> = ["[Server thread/INFO]: Starting", "[Server thread/INFO]: Started serving on 54321", "chat: Local game hosted on port 61000"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(room::detect_port(&lines), Some(61000));
    assert_eq!(room::detect_port(&lines[..2]), Some(54321));
    assert_eq!(room::detect_port(&["nothing".to_string()]), None);
    assert_eq!(room::detect_port(&["Started serving on 80".to_string()]), None);
}

#[test]
fn parse_netstat_listeners() {
    let t = "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       4\n  TCP    0.0.0.0:54321          0.0.0.0:0              LISTENING       777\n  TCP    [::]:54321             [::]:0                 LISTENING       777\n  TCP    127.0.0.1:50000        127.0.0.1:50001        ESTABLISHED     777\n  TCP    0.0.0.0:25565          0.0.0.0:0              LISTENING       888\n";
    assert_eq!(friendlauncher_core::launch::parse_netstat(t, 777), vec![54321]);
    assert!(friendlauncher_core::launch::parse_netstat(t, 1).is_empty());
}

#[test]
#[ignore]
fn tunnel_end_to_end() {
    // «Мир Minecraft»: простой эхо-сервер на локальном порту
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let world_port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let mut buf = [0u8; 64];
                while let Ok(n) = s.read(&mut buf) {
                    if n == 0 || s.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    let host = room::host(Arc::new(move || Some(world_port))).expect("комната");
    let guest = room::join(&host.code).expect("подключение");
    let port = guest.info.listen_port.load(std::sync::atomic::Ordering::Relaxed);
    assert!(port > 0);
    let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    c.write_all(b"hello minecraft").unwrap();
    let mut got = [0u8; 15];
    c.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"hello minecraft");
    assert_eq!(host.info.peers.load(std::sync::atomic::Ordering::Relaxed), 1);
    guest.stop();
    host.stop();
}
