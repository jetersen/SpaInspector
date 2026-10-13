//! Explicitly enabled localhost control endpoint and browser WebSocket bridge.
use crate::{Command, Envelope, Reply};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, Cursor, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub const MAX_BODY: usize = 180 * 1024 * 1024;
type Connection = Option<(u64, mpsc::Sender<Envelope>)>;
#[derive(Clone)]
pub struct Broker {
    inner: Arc<Mutex<Connection>>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Broker {
    pub fn native(sender: mpsc::Sender<Envelope>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some((0, sender)))),
            wake: Some(wake),
        }
    }
    pub fn browser() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            wake: None,
        }
    }
    pub fn call(&self, command: Command) -> Reply {
        let sender = self
            .inner
            .lock()
            .map_err(|_| "Control broker unavailable")?
            .as_ref()
            .map(|(_, s)| s.clone())
            .ok_or("No browser app is connected. Open the Rust app with ?control=8091.")?;
        let (tx, rx) = mpsc::channel();
        sender
            .send(Envelope { command, reply: tx })
            .map_err(|_| "Inspector disconnected")?;
        if let Some(wake) = &self.wake {
            wake();
        }
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| "Inspector did not answer within 30 seconds")?
    }
}
pub struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Server {
    pub fn start(port: u16, broker: Broker) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let active = Arc::new(AtomicUsize::new(0));
        thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if active.fetch_add(1, Ordering::Relaxed) >= 16 {
                            active.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let broker = broker.clone();
                        let active = Arc::clone(&active);
                        let stop = Arc::clone(&stopped);
                        thread::spawn(move || {
                            let _ = connection(stream, broker, stop);
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self { port, stop })
    }
}
struct PrefixStream {
    prefix: Cursor<Vec<u8>>,
    stream: TcpStream,
}
impl Read for PrefixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.prefix.position() < (self.prefix.get_ref().len() as u64) {
            self.prefix.read(buf)
        } else {
            self.stream.read(buf)
        }
    }
}
impl Write for PrefixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}
fn header(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut byte = [0];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 8192 {
            return Err(io::Error::other("HTTP headers exceed 8 KiB"));
        }
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
    }
    Ok(bytes)
}
fn authority(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "localhost") && port.parse::<u16>().is_ok_and(|p| p != 0)
}
fn origin(value: &str) -> bool {
    value.strip_prefix("http://").is_some_and(authority)
}
fn response(stream: &mut TcpStream, status: &str, value: Value) -> io::Result<()> {
    let body = serde_json::to_vec(&value)?;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}
fn connection(mut stream: TcpStream, broker: Broker, stop: Arc<AtomicBool>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let raw = header(&mut stream)?;
    let text = std::str::from_utf8(&raw).map_err(io::Error::other)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let headers = lines
        .filter_map(|l| {
            l.split_once(':')
                .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
        })
        .collect::<HashMap<_, _>>();
    if !headers.get("host").is_some_and(|h| authority(h))
        || headers.get("origin").is_some_and(|v| !origin(v))
    {
        return response(
            &mut stream,
            "403 Forbidden",
            json!({"ok":false,"error":"Only localhost control clients are accepted"}),
        );
    }
    if first == "GET /bridge HTTP/1.1"
        && headers
            .get("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    {
        return websocket(
            PrefixStream {
                prefix: Cursor::new(raw),
                stream,
            },
            broker,
            stop,
        );
    }
    if first != "POST /control HTTP/1.1" {
        return response(
            &mut stream,
            "405 Method Not Allowed",
            json!({"ok":false,"error":"Use POST /control"}),
        );
    }
    if headers
        .get("content-type")
        .is_none_or(|v| v.split(';').next() != Some("application/json"))
        || headers.contains_key("transfer-encoding")
    {
        return response(
            &mut stream,
            "415 Unsupported Media Type",
            json!({"ok":false,"error":"Use JSON with Content-Length"}),
        );
    }
    let Some(length) = headers
        .get("content-length")
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n <= MAX_BODY)
    else {
        return response(
            &mut stream,
            "413 Content Too Large",
            json!({"ok":false,"error":"Invalid or excessive Content-Length"}),
        );
    };
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    let command = match serde_json::from_slice::<Command>(&bytes) {
        Ok(c) => c,
        Err(e) => {
            return response(
                &mut stream,
                "400 Bad Request",
                json!({"ok":false,"error":e.to_string()}),
            );
        }
    };
    let result = match broker.call(command) {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    response(&mut stream, "200 OK", result)
}
static SESSION: AtomicU64 = AtomicU64::new(1);
fn websocket(stream: PrefixStream, broker: Broker, stop: Arc<AtomicBool>) -> io::Result<()> {
    let mut config = tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(MAX_BODY);
    config.max_frame_size = Some(MAX_BODY);
    let mut socket = tungstenite::accept_with_config(stream, Some(config))
        .map_err(|e| io::Error::other(e.to_string()))?;
    socket
        .get_mut()
        .stream
        .set_read_timeout(Some(Duration::from_millis(30)))?;
    let (tx, rx) = mpsc::channel::<Envelope>();
    let session = SESSION.fetch_add(1, Ordering::Relaxed);
    {
        let mut current = broker
            .inner
            .lock()
            .map_err(|_| io::Error::other("Broker unavailable"))?;
        if current.is_some() {
            return Err(io::Error::other("An inspector is already connected"));
        }
        *current = Some((session, tx));
    }
    let mut pending = HashMap::new();
    let mut next = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let mut disconnected = false;
        for envelope in rx.try_iter() {
            next += 1;
            let request = json!({"id":next,"method":envelope.command.method,"params":envelope.command.params});
            if socket
                .send(tungstenite::Message::Text(request.to_string().into()))
                .is_err()
            {
                let _ = envelope.reply.send(Err("Browser disconnected".into()));
                disconnected = true;
                break;
            }
            pending.insert(next, envelope.reply);
        }
        if disconnected {
            break;
        }
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text)
                    && let Some(id) = value["id"].as_u64()
                    && let Some(reply) = pending.remove(&id)
                {
                    let result = if value["ok"] == true {
                        Ok(value["result"].clone())
                    } else {
                        Err(value["error"]
                            .as_str()
                            .unwrap_or("Browser command failed")
                            .to_string())
                    };
                    let _ = reply.send(result);
                }
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    for (_, reply) in pending {
        let _ = reply.send(Err("Browser disconnected".into()));
    }
    if let Ok(mut current) = broker.inner.lock()
        && current.as_ref().is_some_and(|(id, _)| *id == session)
    {
        *current = None;
    }
    Ok(())
}
pub fn call(port: u16, command: Command) -> Reply {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
        .map_err(|e| format!("Cannot connect to inspector at 127.0.0.1:{port}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(35)))
        .map_err(|e| e.to_string())?;
    let body = serde_json::to_vec(&command).map_err(|e| e.to_string())?;
    write!(stream,"POST /control HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).map_err(|e|e.to_string())?;
    stream.write_all(&body).map_err(|e| e.to_string())?;
    let head = header(&mut stream).map_err(|e| e.to_string())?;
    let head = String::from_utf8(head).map_err(|e| e.to_string())?;
    let length = head
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        })
        .filter(|n| *n <= 512 * 1024 * 1024)
        .ok_or("Invalid control response length")?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if value["ok"] == true {
        Ok(value["result"].clone())
    } else {
        Err(value["error"]
            .as_str()
            .unwrap_or("Control request failed")
            .to_string())
    }
}
