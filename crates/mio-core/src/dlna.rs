//! DLNA casting (`dlna/` of the original): finding UPnP MediaRenderers on the local network with
//! SSDP and driving their AVTransport / RenderingControl services with SOAP.
//!
//! Everything here blocks and is meant to run on a background thread. The trust rules are the
//! original's: only private IPv4 devices that answer for themselves are accepted, their service
//! endpoints must be on the same address, and no redirects or credentials are followed.
use reqwest::Url;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const MEDIA_RENDERER_TARGET: &str = "urn:schemas-upnp-org:device:MediaRenderer:1";
const DEVICE_TYPE_PREFIX: &str = "urn:schemas-upnp-org:device:MediaRenderer:";
const AV_TRANSPORT_PREFIX: &str = "urn:schemas-upnp-org:service:AVTransport:";
const RENDERING_CONTROL_PREFIX: &str = "urn:schemas-upnp-org:service:RenderingControl:";
const SOAP_NAMESPACE: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const SSDP_TARGET: &str = "239.255.255.250:1900";
const SEARCH_ATTEMPTS: usize = 3;
const SEARCH_WINDOW: Duration = Duration::from_secs(3);
const DESCRIPTION_WINDOW: Duration = Duration::from_secs(3);
const SEARCH_COOLDOWN: Duration = Duration::from_secs(3);
const MAX_PACKET: usize = 8 * 1024;
const MAX_RESPONSES: usize = 64;
const MAX_INTERFACES: usize = 16;
const MAX_DESCRIPTION_BYTES: usize = 256 * 1024;
const MAX_SOAP_BYTES: usize = 128 * 1024;
const MAX_DESCRIPTION_FETCHES: usize = 6;

/// A renderer as the settings page lists it (`DlnaDevice`).
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub usn: String,
    pub name: String,
    pub location: String,
    pub address: String,
}

#[derive(Debug, Clone)]
struct Service {
    url: Url,
    service_type: String,
}

#[derive(Debug, Clone)]
struct Cached {
    public: Device,
    av_transport: Service,
    rendering_control: Service,
    ip: Ipv4Addr,
}

#[derive(Default)]
struct State {
    cache: HashMap<String, Cached>,
    /// Location of the renderer being cast to.
    active: Option<String>,
    last_search: Option<(Instant, Result<Vec<Device>, String>)>,
}

/// One owner's discovery cache and selected renderer. Network calls never hold the state lock.
#[derive(Default)]
pub struct Controller {
    state: Mutex<State>,
    operation: Mutex<()>,
}
static DEFAULT: std::sync::LazyLock<Controller> = std::sync::LazyLock::new(Controller::default);
impl Controller {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub position: f64,
    pub duration: f64,
}

// ----- discovery ---------------------------------------------------------------------------

fn is_local_unicast(address: Ipv4Addr) -> bool {
    address.is_private()
        && !address.is_loopback()
        && !address.is_link_local()
        && !address.is_multicast()
        && !address.is_broadcast()
        && !address.is_unspecified()
}

#[cfg(unix)]
fn local_interfaces() -> Result<Vec<Ipv4Addr>, String> {
    use std::collections::HashSet;
    let mut addresses = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut addresses) } != 0 {
        return Err(format!(
            "枚举本地 IPv4 接口失败: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut interfaces = HashSet::new();
    let mut current = addresses;
    while !current.is_null() {
        let interface = unsafe { &*current };
        if !interface.ifa_addr.is_null()
            && unsafe { (*interface.ifa_addr).sa_family as i32 } == libc::AF_INET
            && interface.ifa_flags & libc::IFF_UP as u32 != 0
            && interface.ifa_flags & libc::IFF_LOOPBACK as u32 == 0
        {
            let address = unsafe { &*(interface.ifa_addr as *const libc::sockaddr_in) };
            let address = Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr));
            if is_local_unicast(address) {
                interfaces.insert(address);
            }
        }
        current = interface.ifa_next;
    }
    unsafe { libc::freeifaddrs(addresses) };
    if interfaces.is_empty() {
        return Err("未找到可用的私有 IPv4 网络接口".into());
    }
    Ok(interfaces.into_iter().collect())
}

#[cfg(not(unix))]
fn local_interfaces() -> Result<Vec<Ipv4Addr>, String> {
    Ok(vec![Ipv4Addr::UNSPECIFIED])
}

fn bind_socket(address: Ipv4Addr) -> Result<UdpSocket, String> {
    let socket = UdpSocket::bind((address, 0)).map_err(|e| format!("绑定 {address} 失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let interface = libc::in_addr {
            s_addr: u32::from_ne_bytes(address.octets()),
        };
        let result = unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                libc::IPPROTO_IP,
                libc::IP_MULTICAST_IF,
                &interface as *const libc::in_addr as *const libc::c_void,
                std::mem::size_of::<libc::in_addr>() as libc::socklen_t,
            )
        };
        if result != 0 {
            return Err(format!(
                "设置 DLNA 多播接口 {address} 失败: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    socket
        .set_write_timeout(Some(Duration::from_millis(250)))
        .map_err(|error| error.to_string())?;
    Ok(socket)
}

struct Response {
    usn: String,
    location: String,
    sender: SocketAddr,
}

fn parse_response(packet: &[u8], sender: SocketAddr) -> Option<Response> {
    let packet = std::str::from_utf8(packet).ok()?;
    let mut status = packet.lines().next()?.split_whitespace();
    if !matches!(status.next()?, "HTTP/1.0" | "HTTP/1.1") || status.next()? != "200" {
        return None;
    }
    let headers: HashMap<String, String> = packet
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let location = headers.get("location")?;
    let usn = headers.get("usn")?.clone();
    let suffix = format!("::{}", MEDIA_RENDERER_TARGET.to_ascii_lowercase());
    if !headers
        .get("st")?
        .eq_ignore_ascii_case(MEDIA_RENDERER_TARGET)
        || !usn.starts_with("uuid:")
        || !usn.to_ascii_lowercase().ends_with(&suffix)
        || !location_is_sender(location, sender.ip())
    {
        return None;
    }
    Some(Response {
        usn,
        location: location.clone(),
        sender,
    })
}

fn location_is_sender(location: &str, sender: IpAddr) -> bool {
    let Ok(url) = Url::parse(location) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    let (IpAddr::V4(sender), Some(host)) = (sender, url.host_str()) else {
        return false;
    };
    host.parse::<Ipv4Addr>().ok() == Some(sender) && is_local_unicast(sender)
}

/// Joins on all exits, including a later worker-spawn failure or unwinding.
struct Workers(Vec<std::thread::JoinHandle<()>>);
impl Drop for Workers {
    fn drop(&mut self) {
        for worker in self.0.drain(..) {
            let _ = worker.join();
        }
    }
}

/// Sends on each bounded interface concurrently; no worker outlives this call.
fn search_responses(cancelled: Arc<AtomicBool>) -> Result<HashMap<String, Response>, String> {
    const REQUEST: &str = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
    let interfaces = local_interfaces()?;
    if interfaces.len() > MAX_INTERFACES {
        return Err("DLNA 网络接口数量超过限制".into());
    }
    let sockets = interfaces
        .into_iter()
        .map(bind_socket)
        .collect::<Result<Vec<_>, _>>()?;
    let deadline = Instant::now() + SEARCH_WINDOW;
    let (tx, rx) = std::sync::mpsc::sync_channel(MAX_RESPONSES);
    let sent_any = Arc::new(AtomicBool::new(false));
    let mut workers = Workers(Vec::new());
    for socket in sockets {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let tx = tx.clone();
        let cancelled = cancelled.clone();
        let sent = sent_any.clone();
        workers.0.push(
            std::thread::Builder::new()
                .name("mio-ssdp".into())
                .spawn(move || {
                    for attempt in 0..SEARCH_ATTEMPTS {
                        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                            break;
                        }
                        if socket.send_to(REQUEST.as_bytes(), SSDP_TARGET).is_ok() {
                            sent.store(true, Ordering::Release);
                        }
                        if attempt + 1 < SEARCH_ATTEMPTS {
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                    let mut buffer = [0u8; MAX_PACKET + 1];
                    let mut accepted = 0;
                    loop {
                        let now = Instant::now();
                        if now >= deadline || cancelled.load(Ordering::Acquire) {
                            break;
                        }
                        if socket
                            .set_read_timeout(Some(
                                (deadline - now).min(Duration::from_millis(250)),
                            ))
                            .is_err()
                        {
                            break;
                        }
                        match socket.recv_from(&mut buffer) {
                            Ok((size, sender)) => {
                                if size > MAX_PACKET {
                                    continue;
                                }
                                if let Some(response) = parse_response(&buffer[..size], sender) {
                                    match tx.try_send(response) {
                                        Ok(()) => {
                                            accepted += 1;
                                        }
                                        Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                                        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                            break;
                                        }
                                    }
                                    if accepted >= MAX_RESPONSES {
                                        break;
                                    }
                                }
                            }
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|_| "dlna_worker_unavailable")?,
        );
    }
    drop(tx);
    let mut responses = HashMap::new();
    while let Ok(response) = rx.recv() {
        if responses.len() < MAX_RESPONSES {
            responses.entry(response.usn.clone()).or_insert(response);
        }
    }
    drop(workers);
    if cancelled.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    if !sent_any.load(Ordering::Acquire) {
        return Err("DLNA M-SEARCH 未成功发送".into());
    }
    Ok(responses)
}

// ----- device description and SOAP documents --------------------------------------------

fn http_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("DLNA 地址无效: {e}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("DLNA 地址必须是无凭据的 HTTP(S) 地址".into());
    }
    Ok(url)
}

fn resolve(base: &Url, raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw)
        .or_else(|_| base.join(raw))
        .map_err(|e| format!("DLNA 服务地址无效: {e}"))?;
    http_url(url.as_str())
}

fn is_named(node: roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name)
}
fn child<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children().find(|c| is_named(*c, name))
}
fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    child(node, name)
        .and_then(|c| c.text())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}
fn is_versioned(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|v| v.parse::<u32>().is_ok_and(|v| v > 0))
}

fn find_service(
    list: roxmltree::Node<'_, '_>,
    base: &Url,
    prefix: &str,
) -> Result<Service, String> {
    let service = list
        .children()
        .find(|s| {
            is_named(*s, "service")
                && child_text(*s, "serviceType").is_some_and(|t| is_versioned(&t, prefix))
        })
        .ok_or_else(|| format!("DLNA 设备不支持 {prefix} 服务"))?;
    let service_type = child_text(service, "serviceType").unwrap_or_default();
    let control = child_text(service, "controlURL")
        .ok_or_else(|| format!("DLNA {prefix} 服务缺少 controlURL"))?;
    Ok(Service {
        url: resolve(base, &control)?,
        service_type,
    })
}

fn parse_description(
    document: &str,
    description_url: &str,
) -> Result<(String, Service, Service), String> {
    let document =
        roxmltree::Document::parse(document).map_err(|e| format!("DLNA 设备描述 XML 无效: {e}"))?;
    let description_url = http_url(description_url)?;
    let root = document.root_element();
    let device = child(root, "device").ok_or("DLNA 设备描述缺少根设备")?;
    let device_type = child_text(device, "deviceType").ok_or("DLNA 设备描述缺少 deviceType")?;
    if !is_versioned(&device_type, DEVICE_TYPE_PREFIX) {
        return Err("DLNA 设备不是 MediaRenderer".into());
    }
    let base = match child_text(root, "URLBase") {
        Some(value) => resolve(&description_url, &value)?,
        None => description_url,
    };
    let list = child(device, "serviceList").ok_or("DLNA 设备描述缺少 renderer serviceList")?;
    let av = find_service(list, &base, AV_TRANSPORT_PREFIX)?;
    let rc = find_service(list, &base, RENDERING_CONTROL_PREFIX)?;
    let name = child_text(device, "friendlyName").unwrap_or_else(|| "DLNA Media Renderer".into());
    Ok((name, av, rc))
}

fn parse_duration(value: &str) -> Result<f64, String> {
    let parts: Vec<&str> = value.trim().split(':').collect();
    if parts.len() != 3 {
        return Err(format!("DLNA 时间格式无效: {value}"));
    }
    let mut total = 0.;
    for (index, part) in parts.iter().enumerate() {
        let number: f64 = part
            .parse()
            .map_err(|_| format!("DLNA 时间格式无效: {value}"))?;
        if !number.is_finite() {
            return Err(format!("DLNA 时间格式无效: {value}"));
        }
        total += number * [3600., 60., 1.][index];
    }
    Ok(total)
}

fn parse_position(document: &str, service_type: &str) -> Result<Position, String> {
    let document =
        roxmltree::Document::parse(document).map_err(|e| format!("DLNA 位置响应 XML 无效: {e}"))?;
    let envelope = document.root_element();
    if !is_named(envelope, "Envelope") || envelope.tag_name().namespace() != Some(SOAP_NAMESPACE) {
        return Err("DLNA 位置响应不是 SOAP Envelope".into());
    }
    let body = child(envelope, "Body").ok_or("DLNA 位置响应缺少 SOAP Body")?;
    let response = body
        .children()
        .find(|n| {
            is_named(*n, "GetPositionInfoResponse")
                && n.tag_name().namespace() == Some(service_type)
        })
        .ok_or("DLNA 位置响应不是 GetPositionInfoResponse")?;
    let position = child_text(response, "RelTime").ok_or("DLNA 位置响应缺少 RelTime")?;
    let duration =
        child_text(response, "TrackDuration").ok_or("DLNA 位置响应缺少 TrackDuration")?;
    Ok(Position {
        position: parse_duration(&position)?,
        duration: parse_duration(&duration)?,
    })
}

fn parse_fault(document: &str) -> Option<String> {
    let document = roxmltree::Document::parse(document).ok()?;
    let envelope = document.root_element();
    if !is_named(envelope, "Envelope") || envelope.tag_name().namespace() != Some(SOAP_NAMESPACE) {
        return None;
    }
    let body = child(envelope, "Body")?;
    let fault = body
        .children()
        .find(|n| is_named(*n, "Fault") && n.tag_name().namespace() == Some(SOAP_NAMESPACE))?;
    let find = |name: &str| {
        fault
            .descendants()
            .find(|n| is_named(*n, name))
            .and_then(|n| n.text())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let code = find("errorCode")
        .or_else(|| find("faultcode"))
        .unwrap_or_else(|| "unknown".into());
    let message = find("errorDescription")
        .or_else(|| find("faultstring"))
        .unwrap_or_else(|| "Unknown DLNA SOAP fault".into());
    Some(format!("DLNA SOAP Fault {code}: {message}"))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn didl(title: &str, resource: &str) -> String {
    let title = escape(if title.trim().is_empty() {
        "Mio Music"
    } else {
        title
    });
    let resource = escape(resource);
    let path = resource
        .split('?')
        .next()
        .unwrap_or(&resource)
        .to_ascii_lowercase();
    let mime = if path.ends_with(".mp3") {
        "audio/mpeg"
    } else if path.ends_with(".flac") {
        "audio/flac"
    } else if path.ends_with(".aac") {
        "audio/aac"
    } else if path.ends_with(".m4a") {
        "audio/mp4"
    } else if path.ends_with(".ogg") || path.ends_with(".opus") {
        "audio/ogg"
    } else if path.ends_with(".wav") {
        "audio/wav"
    } else {
        "audio/*"
    };
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"><item id=\"0\" parentID=\"0\" restricted=\"1\"><dc:title>{title}</dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo=\"http-get:*:{mime}:*\">{resource}</res></item></DIDL-Lite>"
    )
}

fn envelope(service: &str, action: &str, values: &[(&str, String)]) -> String {
    let values: String = values
        .iter()
        .map(|(name, value)| format!("<{name}>{}</{name}>", escape(value)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{service}\">{values}</u:{action}></s:Body></s:Envelope>"
    )
}

#[allow(dead_code)]
fn clock(seconds: f64) -> String {
    let seconds = if seconds.is_finite() && seconds > 0. {
        seconds.floor() as u64
    } else {
        0
    };
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

// ----- network ---------------------------------------------------------------------------

/// One pooled client for every description and SOAP call (clones share the pool).
fn client() -> Result<reqwest::blocking::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::blocking::Client, String>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .map_err(|e| e.to_string())
        })
        .clone()
}

fn read_limited(response: reqwest::blocking::Response, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("DLNA 响应读取失败: {e}"))?;
    if bytes.len() > limit {
        return Err("DLNA 响应过大".into());
    }
    Ok(bytes)
}

/// The endpoint must be on the renderer's own address.
fn trusted_endpoint(raw: &str, ip: Ipv4Addr) -> Result<(), String> {
    let url = Url::parse(raw).map_err(|e| format!("DLNA 端点无效: {e}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().and_then(|h| h.parse::<Ipv4Addr>().ok()) != Some(ip)
    {
        return Err("DLNA 端点必须使用已发现设备的私有 IPv4 地址".into());
    }
    Ok(())
}

fn describe(response: &Response) -> Result<Cached, String> {
    let ip = match response.sender.ip() {
        IpAddr::V4(ip) if is_local_unicast(ip) => ip,
        _ => return Err("DLNA 响应未来自受信任的私有 IPv4 地址".into()),
    };
    trusted_endpoint(&response.location, ip)?;
    let reply = client()?
        .get(&response.location)
        .send()
        .map_err(|e| format!("DLNA 设备描述请求失败: {e}"))?;
    if !reply.status().is_success() {
        return Err(format!(
            "DLNA 设备描述返回 HTTP {}",
            reply.status().as_u16()
        ));
    }
    let bytes = read_limited(reply, MAX_DESCRIPTION_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "DLNA 设备描述不是 UTF-8 文本".to_owned())?;
    let (name, av, rc) = parse_description(&text, &response.location)?;
    trusted_endpoint(av.url.as_str(), ip)?;
    trusted_endpoint(rc.url.as_str(), ip)?;
    Ok(Cached {
        public: Device {
            usn: response.usn.clone(),
            name,
            location: response.location.clone(),
            address: ip.to_string(),
        },
        av_transport: av,
        rendering_control: rc,
        ip,
    })
}

/// `start_search`: the devices answering within the window, sorted by name. A search finished
/// moments ago is answered from its result.
pub fn search() -> Result<Vec<Device>, String> {
    DEFAULT.search()
}

fn soap(
    device: &Cached,
    service: &Service,
    action: &str,
    values: &[(&str, String)],
) -> Result<String, String> {
    trusted_endpoint(service.url.as_str(), device.ip)?;
    let body = envelope(&service.service_type, action, values);
    let reply = client()?
        .post(service.url.clone())
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header(
            "SOAPACTION",
            format!("\"{}#{action}\"", service.service_type),
        )
        .body(body)
        .send()
        .map_err(|e| format!("DLNA 控制请求失败: {e}"))?;
    let status = reply.status();
    let bytes = read_limited(reply, MAX_SOAP_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "DLNA SOAP 响应不是 UTF-8 文本".to_owned())?;
    if let Some(fault) = parse_fault(&text) {
        return Err(fault);
    }
    if !status.is_success() {
        return Err(format!("DLNA 控制请求返回 HTTP {}", status.as_u16()));
    }
    Ok(text)
}

fn av(device: &Cached, action: &str, values: &[(&str, String)]) -> Result<String, String> {
    soap(device, &device.av_transport, action, values)
}

/// `play`: sets the media and starts it. `location` must be a device of the last search.
pub fn play(location: &str, media_url: &str, title: &str) -> Result<(), String> {
    DEFAULT.play(location, media_url, title)
}

pub fn pause() -> Result<(), String> {
    DEFAULT.pause()
}

pub fn resume() -> Result<(), String> {
    DEFAULT.resume()
}

/// Stops the renderer and forgets it as the cast target (also when stopping fails).
pub fn stop() -> Result<(), String> {
    DEFAULT.stop()
}

/// `dlna__seek` exists in the original but nothing in its player calls it.
#[allow(dead_code)]
pub fn seek(seconds: f64) -> Result<(), String> {
    DEFAULT.seek(seconds)
}

/// Volume 0 to 100.
pub fn set_volume(volume: f64) -> Result<(), String> {
    DEFAULT.set_volume(volume)
}

pub fn position() -> Result<Position, String> {
    DEFAULT.position()
}

impl Controller {
    pub fn search(&self) -> Result<Vec<Device>, String> {
        self.search_with_cancel(Arc::new(AtomicBool::new(false)))
    }
    pub fn search_with_cancel(&self, cancelled: Arc<AtomicBool>) -> Result<Vec<Device>, String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if let Some((at, result)) = &self.state().last_search {
            if at.elapsed() < SEARCH_COOLDOWN {
                return result.clone();
            }
        }
        let result = (|| {
            let responses = search_responses(cancelled.clone())?;
            // One entry per description URL.
            let mut unique: HashMap<String, Response> = HashMap::new();
            for response in responses.into_values() {
                unique.entry(response.location.clone()).or_insert(response);
            }
            let queue = std::sync::Arc::new(Mutex::new(unique.into_values().collect::<Vec<_>>()));
            let found = std::sync::Arc::new(Mutex::new(Vec::new()));
            let deadline = Instant::now() + DESCRIPTION_WINDOW;
            let mut workers = Workers(Vec::new());
            for _ in 0..MAX_DESCRIPTION_FETCHES {
                let queue = queue.clone();
                let found = found.clone();
                let cancelled = cancelled.clone();
                workers.0.push(
                    std::thread::Builder::new()
                        .name("mio-dlna-description".into())
                        .spawn(move || {
                            loop {
                                if Instant::now() >= deadline || cancelled.load(Ordering::Acquire) {
                                    break;
                                }
                                let next = queue.lock().ok().and_then(|mut queue| queue.pop());
                                let Some(response) = next else {
                                    break;
                                };
                                if let Ok(device) = describe(&response) {
                                    if let Ok(mut found) = found.lock() {
                                        found.push(device);
                                    }
                                }
                            }
                        })
                        .map_err(|_| "dlna_worker_unavailable")?,
                );
            }
            drop(workers);
            if cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            let devices = found.lock().map(|f| f.clone()).unwrap_or_default();
            let mut state = self.state();
            let mut replacement: HashMap<String, Cached> = devices
                .into_iter()
                .map(|d| (d.public.location.clone(), d))
                .collect();
            // The renderer being cast to stays even when it did not answer this time.
            if let Some(active) = state.active.clone() {
                if !replacement.contains_key(&active) {
                    if let Some(old) = state.cache.get(&active).cloned() {
                        replacement.retain(|_, d| d.public.usn != old.public.usn);
                        replacement.insert(active, old);
                    }
                }
            }
            state.cache = replacement;
            let mut list: Vec<Device> = state.cache.values().map(|d| d.public.clone()).collect();
            list.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(list)
        })();
        self.state().last_search = Some((Instant::now(), result.clone()));
        result
    }

    fn active_device(&self) -> Result<Cached, String> {
        let state = self.state();
        let location = state.active.clone().ok_or("尚未开始 DLNA 投放")?;
        state
            .cache
            .get(&location)
            .cloned()
            .ok_or_else(|| "DLNA 设备未在当前发现缓存中".to_owned())
    }

    pub fn play(&self, location: &str, media_url: &str, title: &str) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        let media = Url::parse(media_url).map_err(|e| format!("媒体地址无效: {e}"))?;
        if !matches!(media.scheme(), "http" | "https")
            || !media.username().is_empty()
            || media.password().is_some()
        {
            return Err("媒体地址必须是无凭据的 HTTP(S) 地址".into());
        }
        let device = self
            .state()
            .cache
            .get(location)
            .cloned()
            .ok_or("DLNA 设备未在当前发现缓存中")?;
        let result = (|| {
            av(
                &device,
                "SetAVTransportURI",
                &[
                    ("InstanceID", "0".into()),
                    ("CurrentURI", media_url.to_owned()),
                    ("CurrentURIMetaData", didl(title, media_url)),
                ],
            )?;
            av(
                &device,
                "Play",
                &[("InstanceID", "0".into()), ("Speed", "1".into())],
            )?;
            Ok(())
        })();
        self.state().active = result.is_ok().then(|| location.to_owned());
        result
    }

    pub fn pause(&self) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        av(
            &self.active_device()?,
            "Pause",
            &[("InstanceID", "0".into())],
        )
        .map(|_| ())
    }

    pub fn resume(&self) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        av(
            &self.active_device()?,
            "Play",
            &[("InstanceID", "0".into()), ("Speed", "1".into())],
        )
        .map(|_| ())
    }

    pub fn stop(&self) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        let device = self.active_device();
        self.state().active = None;
        av(&device?, "Stop", &[("InstanceID", "0".into())]).map(|_| ())
    }

    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        if !seconds.is_finite() || seconds < 0. {
            return Err("播放位置必须是非负有限数字".into());
        }
        av(
            &self.active_device()?,
            "Seek",
            &[
                ("InstanceID", "0".into()),
                ("Unit", "REL_TIME".into()),
                ("Target", clock(seconds)),
            ],
        )
        .map(|_| ())
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        if !volume.is_finite() {
            return Err("音量必须是有限数字".into());
        }
        let device = self.active_device()?;
        soap(
            &device,
            &device.rendering_control,
            "SetVolume",
            &[
                ("InstanceID", "0".into()),
                ("Channel", "Master".into()),
                (
                    "DesiredVolume",
                    (volume.clamp(0., 100.).round() as u8).to_string(),
                ),
            ],
        )
        .map(|_| ())
    }

    pub fn position(&self) -> Result<Position, String> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| "dlna_controller_unavailable")?;
        let device = self.active_device()?;
        let response = av(&device, "GetPositionInfo", &[("InstanceID", "0".into())])?;
        parse_position(&response, &device.av_transport.service_type)
    }
}

/// Immutable renderer selected from one controller's verified discovery cache.
/// Later discovery or a different controller cannot redirect this handle.
#[derive(Clone)]
pub struct Renderer {
    device: Cached,
}
impl Controller {
    pub fn renderer(&self, location: &str) -> Result<Renderer, String> {
        let device = self
            .state()
            .cache
            .get(location)
            .cloned()
            .ok_or("DLNA 设备未在当前发现缓存中")?;
        Ok(Renderer { device })
    }
}
impl Renderer {
    pub fn device(&self) -> &Device {
        &self.device.public
    }
    /// Caller revalidates the original song/intent before each mutating request.
    /// A request already sent cannot be rolled back; errors may have changed the renderer.
    pub fn play_checked(
        &self,
        media_url: &str,
        title: &str,
        playing: bool,
        check: &dyn Fn() -> Result<(), String>,
        before_send: &mut dyn FnMut(),
    ) -> Result<(), String> {
        if media_url.len() > 16 * 1024 || title.len() > 4096 {
            return Err("capacity_exceeded: DLNA media".into());
        }
        let media = mio_plugin::runtime_url(media_url)?.to_string();
        let media_url = media.as_str();
        http_url(media_url)?;
        check()?;
        before_send();
        av(
            &self.device,
            "SetAVTransportURI",
            &[
                ("InstanceID", "0".into()),
                ("CurrentURI", media_url.into()),
                ("CurrentURIMetaData", didl(title, media_url)),
            ],
        )?;
        check()?;
        if playing {
            self.resume()?;
        }
        Ok(())
    }
    pub fn pause(&self) -> Result<(), String> {
        av(&self.device, "Pause", &[("InstanceID", "0".into())]).map(|_| ())
    }
    pub fn resume(&self) -> Result<(), String> {
        av(
            &self.device,
            "Play",
            &[("InstanceID", "0".into()), ("Speed", "1".into())],
        )
        .map(|_| ())
    }
    pub fn stop(&self) -> Result<(), String> {
        av(&self.device, "Stop", &[("InstanceID", "0".into())]).map(|_| ())
    }
    pub fn set_volume(&self, volume: f64) -> Result<(), String> {
        if !volume.is_finite() {
            return Err("音量必须是有限数字".into());
        }
        soap(
            &self.device,
            &self.device.rendering_control,
            "SetVolume",
            &[
                ("InstanceID", "0".into()),
                ("Channel", "Master".into()),
                (
                    "DesiredVolume",
                    (volume.clamp(0., 100.).round() as u8).to_string(),
                ),
            ],
        )
        .map(|_| ())
    }
    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0. {
            return Err("invalid_input: DLNA seek".into());
        }
        av(
            &self.device,
            "Seek",
            &[
                ("InstanceID", "0".into()),
                ("Unit", "REL_TIME".into()),
                ("Target", clock(seconds)),
            ],
        )
        .map(|_| ())
    }
    pub fn position(&self) -> Result<Position, String> {
        let response = av(
            &self.device,
            "GetPositionInfo",
            &[("InstanceID", "0".into())],
        )?;
        parse_position(&response, &self.device.av_transport.service_type)
    }
}
