//! «Комнаты»: игра по сети через туннель, без VPN-драйвера и без прав администратора.
//! Хозяин открывает мир для сети и создаёт комнату, друг вводит код: на его компьютере
//! появляется локальный адрес, а мир показывается в «Сетевой игре». Связь идёт напрямую
//! (QUIC, пробитие NAT), а если не выходит, через публичные реле iroh. Трафик шифруется.
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler, Router},
    Endpoint, EndpointId,
};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

pub const ALPN: &[u8] = b"friendlauncher/mc-tunnel/1";
pub const LAN_GROUP: &str = "224.0.2.60:4445";

/// Порт открытого для сети мира по строкам журнала игры (последнее упоминание).
pub fn detect_port(lines: &[String]) -> Option<u16> {
    for l in lines.iter().rev() {
        for key in ["Started serving on ", "Local game hosted on port ", "Started on "] {
            if let Some(i) = l.find(key) {
                let digits: String = l[i + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(p) = digits.parse::<u16>() {
                    if p >= 1024 {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

#[derive(Default)]
pub struct Info {
    status: Mutex<String>,
    error: Mutex<String>,
    pub listen_port: AtomicU16,
    pub peers: AtomicUsize,
    pub refused: AtomicUsize,
}

impl Info {
    fn set_status(&self, s: &str) {
        if let Ok(mut g) = self.status.lock() {
            *g = s.to_string();
        }
    }
    fn set_error(&self, s: &str) {
        if let Ok(mut g) = self.error.lock() {
            *g = s.to_string();
        }
    }
    pub fn status(&self) -> String {
        self.status.lock().map(|g| g.clone()).unwrap_or_default()
    }
    pub fn error(&self) -> String {
        self.error.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

pub struct Active {
    pub mode: &'static str,
    pub code: String,
    pub info: Arc<Info>,
    rt: Option<tokio::runtime::Runtime>,
}

impl Active {
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("Не удалось запустить сетевой модуль: {}", e))
}

/// Гоняет байты между TCP-соединением игры и потоком туннеля в обе стороны.
async fn pipe(tcp: TcpStream, mut send: SendStream, mut recv: RecvStream) {
    let _ = tcp.set_nodelay(true);
    let (mut tr, mut tw) = tcp.into_split();
    let down = async {
        let _ = tokio::io::copy(&mut recv, &mut tw).await;
        let _ = tw.shutdown().await;
    };
    let up = async {
        let _ = tokio::io::copy(&mut tr, &mut send).await;
        let _ = send.finish();
    };
    tokio::join!(down, up);
}

struct PeerGuard(Arc<Info>);

impl Drop for PeerGuard {
    fn drop(&mut self) {
        self.0.peers.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct Tunnel {
    target: Arc<dyn Fn() -> Option<u16> + Send + Sync>,
    info: Arc<Info>,
}

impl std::fmt::Debug for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tunnel")
    }
}

impl ProtocolHandler for Tunnel {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        self.info.peers.fetch_add(1, Ordering::Relaxed);
        let _guard = PeerGuard(self.info.clone());
        while let Ok((send, recv)) = conn.accept_bi().await {
            let port = (self.target)();
            let info = self.info.clone();
            tokio::spawn(async move {
                match port {
                    Some(p) => match TcpStream::connect(("127.0.0.1", p)).await {
                        Ok(tcp) => pipe(tcp, send, recv).await,
                        Err(_) => {
                            info.refused.fetch_add(1, Ordering::Relaxed);
                        }
                    },
                    None => {
                        info.refused.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
        Ok(())
    }
}

/// Хозяин: создаёт комнату. `target` возвращает порт мира, открытого для сети.
pub fn host(target: Arc<dyn Fn() -> Option<u16> + Send + Sync>) -> Result<Active, String> {
    let rt = runtime()?;
    let info = Arc::new(Info::default());
    let info2 = info.clone();
    let code = rt.block_on(async move {
        let ep = Endpoint::bind(presets::N0).await.map_err(|e| format!("Не удалось открыть сеть: {}", e))?;
        let router = Router::builder(ep.clone()).accept(ALPN, Tunnel { target, info: info2.clone() }).spawn();
        if tokio::time::timeout(Duration::from_secs(20), ep.online()).await.is_err() {
            return Err("Нет связи с серверами сети. Проверь интернет и попробуй ещё раз.".to_string());
        }
        let code = ep.id().to_string();
        info2.set_status("ready");
        tokio::spawn(async move {
            let _router = router;
            std::future::pending::<()>().await;
        });
        Ok(code)
    })?;
    Ok(Active { mode: "host", code, info, rt: Some(rt) })
}

/// Гость: подключается к комнате по коду и поднимает локальный адрес для игры.
pub fn join(code: &str) -> Result<Active, String> {
    let id: EndpointId = code.trim().parse().map_err(|_| "Неверный код комнаты. Скопируй его у хозяина целиком.".to_string())?;
    let rt = runtime()?;
    let info = Arc::new(Info::default());
    let info2 = info.clone();
    rt.block_on(async move {
        info2.set_status("connecting");
        let ep = Endpoint::bind(presets::N0).await.map_err(|e| format!("Не удалось открыть сеть: {}", e))?;
        let conn = match tokio::time::timeout(Duration::from_secs(30), ep.connect(id, ALPN)).await {
            Err(_) => return Err("Хозяин не отвечает. Комната должна быть открыта, а у хозяина запущен лаунчер.".to_string()),
            Ok(Err(e)) => return Err(format!("Не удалось подключиться к комнате: {}", e)),
            Ok(Ok(c)) => c,
        };
        let listener = match TcpListener::bind(("0.0.0.0", 25565)).await {
            Ok(l) => l,
            Err(_) => TcpListener::bind(("0.0.0.0", 0)).await.map_err(|e| format!("Не удалось открыть локальный порт: {}", e))?,
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        info2.listen_port.store(port, Ordering::Relaxed);
        info2.set_status("connected");

        let c2 = conn.clone();
        tokio::spawn(async move {
            loop {
                let (tcp, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let c = c2.clone();
                tokio::spawn(async move {
                    if let Ok((send, recv)) = c.open_bi().await {
                        pipe(tcp, send, recv).await;
                    }
                });
            }
        });
        // Объявление «мира в локальной сети»: Minecraft сам покажет его в «Сетевой игре».
        tokio::spawn(async move {
            let sock = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
                Ok(s) => s,
                Err(_) => return,
            };
            let msg = format!("[MOTD]Комната друзей[/MOTD][AD]{}[/AD]", port);
            loop {
                let _ = sock.send_to(msg.as_bytes(), LAN_GROUP).await;
                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        });
        let info3 = info2.clone();
        tokio::spawn(async move {
            let _ep = ep;
            conn.closed().await;
            info3.set_error("Связь с хозяином потеряна. Нажми «Выйти» и подключись снова.");
            info3.set_status("lost");
        });
        Ok(())
    })?;
    Ok(Active { mode: "guest", code: code.trim().to_string(), info, rt: Some(rt) })
}

