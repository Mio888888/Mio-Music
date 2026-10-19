//! Page-owned LAN receiver. The host supplies the existing install/import rules.
mod http;
mod net;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufReader, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const FILE_LIMIT: u64 = 256 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
const PLUGIN_LIMIT: u64 = 1024 * 1024;
const RESERVE: u64 = 64 * 1024 * 1024;
static ACTIVE: AtomicBool = AtomicBool::new(false);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}

pub fn addresses() -> Result<Vec<String>, String> {
    Ok(net::interfaces()?
        .iter()
        .map(|row| row.address.to_string())
        .collect())
}
#[derive(Clone)]
pub struct Event {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub success: bool,
    pub message: String,
}
#[derive(Clone)]
pub struct View {
    pub address: String,
    pub url: String,
    pub code: String,
    pub paired: bool,
    pub closed: bool,
    pub phase: String,
    pub received_bytes: u64,
    pub total_reserved: u64,
    pub events: Vec<Event>,
    pub error: Option<String>,
}
/// Created before an asynchronous start, so close also cancels pending bind.
#[derive(Default)]
pub struct Control {
    pub closed: Arc<AtomicBool>,
    listener: Mutex<Option<TcpListener>>,
    connection: Mutex<Option<TcpStream>>,
}
impl Control {
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.listener
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(stream) = self
            .connection
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
pub enum Upload {
    Audio { path: PathBuf, name: String },
    PluginCode(String),
    PluginUrl(String),
}
pub type Handler = dyn Fn(Upload, Arc<AtomicBool>) -> Result<String, String> + Send + Sync;
struct Pairing {
    invitation: Option<String>,
    session: String,
    peer: Option<Ipv4Addr>,
}
struct Progress {
    phase: String,
    received: u64,
    reserved: u64,
    count: u32,
    events: VecDeque<Event>,
    error: Option<String>,
}
struct State {
    control: Arc<Control>,
    interface: net::Interface,
    authority: String,
    destination: PathBuf,
    pairing: Mutex<Pairing>,
    progress: Mutex<Progress>,
    expires: Instant,
}
impl State {
    fn check(&self) -> Result<(), String> {
        if Instant::now() >= self.expires {
            self.control.close();
            return Err("lan_session_expired".into());
        }
        self.control.check()
    }
    fn event(&self, event: Event) {
        let mut state = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        if state.events.len() == 20 {
            state.events.pop_front();
        }
        state.events.push_back(event);
        state.phase = "ready".into();
    }
}
struct StartupGuard(Option<Arc<Control>>);
impl Drop for StartupGuard {
    fn drop(&mut self) {
        if let Some(control) = self.0.take() {
            control.close();
        }
    }
}
pub struct Server {
    state: Arc<State>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Server {
    pub fn start(
        control: Arc<Control>,
        address: &str,
        cache: &Path,
        destination: &Path,
        handler: Arc<Handler>,
    ) -> Result<Self, String> {
        let mut startup = StartupGuard(Some(control.clone()));
        control.check()?;
        let address: Ipv4Addr = address.parse().map_err(|_| "invalid_lan_address")?;
        let interface = net::interfaces()?
            .into_iter()
            .find(|row| row.address == address)
            .ok_or("lan_address_changed")?;
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "busy: LAN receiver")?;
        let permit = Permit;
        for directory in [cache, destination] {
            if !directory.is_absolute()
                || !fs::symlink_metadata(directory)
                    .map_err(|_| "lan_storage_unavailable")?
                    .is_dir()
            {
                return Err("lan_storage_unavailable".into());
            }
            net::space(directory, RESERVE)?;
        }
        let stage = tempfile::Builder::new()
            .prefix("mio-lan-")
            .tempdir_in(cache)
            .map_err(|_| "lan_storage_unavailable")?;
        let authority = {
            // Only bind/nonblocking setup under this gate. close returns after
            // any admitted listener is dropped; no disk or HTTP under the gate.
            let mut slot = control.listener.lock().map_err(|_| "lan_unavailable")?;
            control.check()?;
            if slot.is_some() {
                return Err("busy: LAN receiver".into());
            }
            let listener = TcpListener::bind((address, 0)).map_err(|_| "lan_bind_failed")?;
            listener
                .set_nonblocking(true)
                .map_err(|_| "lan_bind_failed")?;
            let authority = listener
                .local_addr()
                .map_err(|_| "lan_bind_failed")?
                .to_string();
            *slot = Some(listener);
            authority
        };
        let state = Arc::new(State {
            control: control.clone(),
            interface,
            authority,
            destination: destination.into(),
            pairing: Mutex::new(Pairing {
                invitation: Some(secret()),
                session: secret(),
                peer: None,
            }),
            progress: Mutex::new(Progress {
                phase: "ready".into(),
                received: 0,
                reserved: 0,
                count: 0,
                events: VecDeque::new(),
                error: None,
            }),
            expires: Instant::now() + Duration::from_secs(15 * 60),
        });
        let worker = state.clone();
        let thread = std::thread::Builder::new()
            .name("mio-lan-receiver".into())
            .spawn(move || {
                let _permit = permit;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    serve(&worker, stage.path(), &handler)
                }))
                .unwrap_or_else(|_| Err("lan_worker_failed".into()));
                if let Err(error) = result {
                    if error != "cancelled" {
                        worker
                            .progress
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .error = Some(error);
                    }
                }
                worker.control.close();
                // stage drops after the last callback and file handle, including errors/unwind.
                drop(stage);
            });
        match thread {
            Ok(thread) => {
                startup.0 = None;
                Ok(Self {
                    state,
                    thread: Mutex::new(Some(thread)),
                })
            }
            Err(_) => {
                control.close();
                Err("lan_worker_unavailable".into())
            }
        }
    }
    pub fn view(&self) -> View {
        let pairing = self.state.pairing.lock().unwrap_or_else(|e| e.into_inner());
        let progress = self
            .state
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let closed = self.state.control.closed.load(Ordering::Acquire);
        let code = if closed {
            String::new()
        } else {
            pairing.invitation.clone().unwrap_or_default()
        };
        let address = format!("http://{}/", self.state.authority);
        let url = if code.is_empty() {
            address.clone()
        } else {
            format!("{address}#{code}")
        };
        View {
            address,
            url,
            code,
            paired: pairing.peer.is_some(),
            closed,
            phase: progress.phase.clone(),
            received_bytes: progress.received,
            total_reserved: progress.reserved,
            events: progress.events.iter().cloned().collect(),
            error: progress.error.clone(),
        }
    }
    pub fn close(&self) {
        self.state.control.close();
    }
    pub fn finish_close(&self) {
        self.close();
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.finish_close();
    }
}
fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn same_secret(actual: &str, expected: &str) -> bool {
    actual.len() == expected.len()
        && actual
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |value, (a, b)| value | (a ^ b))
            == 0
}
fn serve(state: &State, stage: &Path, handler: &Arc<Handler>) -> Result<(), String> {
    let mut connections = 0u32;
    loop {
        state.check()?;
        let accepted = {
            let listener = state
                .control
                .listener
                .lock()
                .map_err(|_| "lan_unavailable")?;
            let Some(listener) = listener.as_ref() else {
                return Ok(());
            };
            listener.accept()
        };
        let (mut stream, peer) = match accepted {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(_) => return Err("lan_accept_failed".into()),
        };
        connections += 1;
        if connections > 256 {
            return Err("capacity_exceeded: reopen LAN page".into());
        }
        let std::net::IpAddr::V4(peer) = peer.ip() else {
            continue;
        };
        if !state.interface.accepts(peer) {
            continue;
        }
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| "lan_socket_failed")?;
        stream
            .set_write_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| "lan_socket_failed")?;
        {
            let mut current = state
                .control
                .connection
                .lock()
                .map_err(|_| "lan_unavailable")?;
            state.control.check()?;
            *current = Some(stream.try_clone().map_err(|_| "lan_socket_failed")?);
        }
        let result = receive(state, stage, &mut stream, peer, handler);
        if let Err(error) = result {
            json(
                &mut stream,
                400,
                serde_json::json!({"ok":false,"message":error}),
            );
        }
        let _ = stream.shutdown(Shutdown::Both);
        state
            .control
            .connection
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}
// Only public LAN wording is exposed; neither settings nor plugin configuration enter this script.
fn browser_script() -> &'static [u8] {
    static SCRIPT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    SCRIPT.get_or_init(|| {
        let mut languages = serde_json::Map::new();
        for (name, source) in [
            ("en-US", include_str!("../locales/en-US.json")),
            ("zh-CN", include_str!("../locales/zh-CN.json")),
        ] {
            let locale: serde_json::Value =
                serde_json::from_str(source).expect("bundled locale JSON");
            languages.insert(name.into(), locale["lanTransfer"].clone());
        }
        format!(
            "window.MIO_LAN={};\n{}",
            serde_json::Value::Object(languages),
            include_str!("lan_transfer/app.js")
        )
        .into_bytes()
    })
}
fn json(stream: &mut TcpStream, status: u16, value: serde_json::Value) {
    http::response(
        stream,
        status,
        "application/json; charset=utf-8",
        value.to_string().as_bytes(),
    );
}
fn receive(
    state: &State,
    stage: &Path,
    stream: &mut TcpStream,
    peer: Ipv4Addr,
    handler: &Arc<Handler>,
) -> Result<(), String> {
    let mut input = BufReader::with_capacity(
        16 * 1024,
        stream.try_clone().map_err(|_| "lan_socket_failed")?,
    );
    let request = http::request(
        &mut input,
        state.expires.min(Instant::now() + Duration::from_secs(3)),
    )?;
    state.check()?;
    if request.headers.get("host").map(String::as_str) != Some(&state.authority) {
        return Err("invalid_host".into());
    }
    if request.method == "GET" {
        match request.path.as_str() {
            "/" => http::response(
                stream,
                200,
                "text/html; charset=utf-8",
                include_bytes!("lan_transfer/index.html"),
            ),
            "/app.js" => http::response(
                stream,
                200,
                "text/javascript; charset=utf-8",
                browser_script(),
            ),
            _ => http::response(stream, 404, "text/plain", b"Not found"),
        }
        return Ok(());
    }
    let origin = format!("http://{}", state.authority);
    if request.headers.get("origin") != Some(&origin) {
        return Err("invalid_origin".into());
    }
    let key = request
        .headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or("unauthorized")?;
    if request.path == "/pair" {
        if request.length != 0 {
            return Err("invalid_pairing".into());
        }
        let mut pairing = state.pairing.lock().map_err(|_| "lan_unavailable")?;
        let matched = pairing
            .invitation
            .as_ref()
            .is_some_and(|expected| same_secret(key, expected));
        if !matched {
            return Err("pairing_expired_or_invalid".into());
        }
        pairing.invitation = None;
        pairing.peer = Some(peer);
        let session = pairing.session.clone();
        drop(pairing);
        json(
            stream,
            200,
            serde_json::json!({"ok":true,"session":session}),
        );
        return Ok(());
    }
    {
        let pairing = state.pairing.lock().map_err(|_| "lan_unavailable")?;
        if pairing.peer != Some(peer) || !same_secret(key, &pairing.session) {
            return Err("unauthorized".into());
        }
    }
    let id = {
        let mut progress = state.progress.lock().map_err(|_| "lan_unavailable")?;
        progress.count += 1; // Also count authenticated rejected requests; connections are capped.
        progress.received = 0;
        progress.count
    };
    let kind = match request.path.as_str() {
        "/audio" => "audio",
        "/plugin" => "plugin",
        "/plugin-url" => "plugin-url",
        _ => "upload",
    };
    let mut name = "Upload".to_string();
    let validation = (|| {
        let limit = match request.path.as_str() {
            "/audio" => FILE_LIMIT,
            "/plugin" => PLUGIN_LIMIT,
            "/plugin-url" => 4096,
            _ => return Err("invalid_path".to_string()),
        };
        if request.length == 0 || request.length > limit {
            return Err("capacity_exceeded: file size".into());
        }
        name = if request.path == "/plugin-url" {
            "Plugin URL".into()
        } else {
            http::name(
                request
                    .headers
                    .get("x-mio-name")
                    .ok_or("invalid_filename")?,
            )?
        };
        let cost = if request.path == "/plugin-url" {
            PLUGIN_LIMIT
        } else {
            request.length
        };
        let mut progress = state.progress.lock().map_err(|_| "lan_unavailable")?;
        if id > 32 || cost > TOTAL_LIMIT.saturating_sub(progress.reserved) {
            return Err("capacity_exceeded: reopen LAN page".into());
        }
        progress.reserved += cost;
        progress.phase = "receiving".into();
        Ok(limit)
    })();
    let result = (|| {
        let limit = validation?;
        let needed = request.length.saturating_mul(2).saturating_add(RESERVE);
        net::space(stage, needed)?;
        net::space(&state.destination, needed)?;
        let extension = Path::new(&name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if request.path == "/audio"
            && !matches!(
                extension.as_str(),
                "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "wma" | "opus" | "ape" | "alac"
            )
        {
            return Err("unsupported_audio".into());
        }
        // Browser names are metadata only; the staging filename is generated.
        let suffix = if request.path == "/audio" {
            format!(".{extension}")
        } else {
            ".upload".into()
        };
        let mut file = tempfile::Builder::new()
            .prefix("receive-")
            .suffix(&suffix)
            .tempfile_in(stage)
            .map_err(|_| "lan_storage_unavailable")?;
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut remaining = request.length;
        let mut bytes = [0u8; 64 * 1024];
        while remaining != 0 {
            state.check()?;
            if Instant::now() >= deadline {
                return Err("upload_timeout".into());
            }
            let size =
                usize::try_from(remaining.min(bytes.len() as u64)).map_err(|_| "invalid_upload")?;
            let count = match input.read(&mut bytes[..size]) {
                Ok(0) => return Err("upload_incomplete".into()),
                Ok(count) => count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(_) => return Err("upload_incomplete".into()),
            };
            file.write_all(&bytes[..count])
                .map_err(|_| "insufficient_space")?;
            remaining -= count as u64;
            state
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .received = request.length - remaining;
        }
        file.flush().map_err(|_| "insufficient_space")?;
        state.check()?;
        net::space(&state.destination, request.length.saturating_add(RESERVE))?;
        state
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .phase = "processing".into();
        let upload = if request.path == "/audio" {
            Upload::Audio {
                path: file.path().into(),
                name: name.clone(),
            }
        } else {
            let mut code = String::new();
            File::open(file.path())
                .map_err(|_| "lan_storage_unavailable")?
                .take(limit + 1)
                .read_to_string(&mut code)
                .map_err(|_| "invalid_text")?;
            if request.path == "/plugin-url" {
                let url = reqwest::Url::parse(code.trim()).map_err(|_| "invalid_plugin_url")?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                {
                    return Err("invalid_plugin_url".into());
                }
                Upload::PluginUrl(url.to_string())
            } else {
                Upload::PluginCode(code)
            }
        };
        handler(upload, state.control.closed.clone())
    })();
    let success = result.is_ok();
    let message = result.unwrap_or_else(|error| error);
    state.event(Event {
        id,
        name,
        kind: kind.into(),
        success,
        message: message.clone(),
    });
    json(
        stream,
        if success { 200 } else { 409 },
        serde_json::json!({"ok":success,"message":message}),
    );
    Ok(())
}
