//! Bounded native CR/LX runtime. No filesystem, process or module loader is exposed.
use base64::{Engine, engine::general_purpose::STANDARD};
use md5::{Digest, Md5};
use rquickjs::{Context, Function, Promise, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::Read,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const LOG_LINES: usize = 500;
const LOG_LINE_CHARS: usize = 2000;

/// One line of a plugin's log: when, and what the plugin (or the host) said.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub at_ms: u64,
    pub text: String,
}
pub(crate) type Logs = Arc<Mutex<VecDeque<LogLine>>>;

pub(crate) fn push_log(logs: &Logs, text: String) {
    let at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let text: String = text.chars().take(LOG_LINE_CHARS).collect();
    if let Ok(mut lines) = logs.lock() {
        if lines.len() >= LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(LogLine { at_ms, text });
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginSource {
    pub id: String,
    pub name: String,
    pub qualities: Vec<String>,
}
#[derive(Clone)]
pub struct Plugin {
    code: Arc<str>,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub sources: Vec<PluginSource>,
    logs: Logs,
}
impl Plugin {
    pub fn load(code: String) -> Result<Self, String> {
        if code.is_empty() || code.len() > 1024 * 1024 {
            return Err("插件文件为空或超过 1MiB".into());
        }
        let code = crate::converter::refresh_lx_conversion(code);
        let comment_name = regex_lite::Regex::new(r"@name\s+([^\r\n]+)")
            .ok()
            .and_then(|r| {
                r.captures(&code)
                    .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_owned()))
            });
        let logs: Logs = Default::default();
        let meta = execute(
            &code,
            "var i=__plugin.pluginInfo||{}; return {name: i.name || 'LX', version: i.version || '', author: i.author || '', description: i.description || '', sources: __sources};",
            Arc::new(AtomicBool::new(false)),
            &logs,
        )?;
        let comment = |tag: &str| {
            regex_lite::Regex::new(&format!(r"@{tag}\s+([^\r\n]+)"))
                .ok()
                .and_then(|r| {
                    r.captures(&code)
                        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_owned()))
                })
        };
        let text = |key: &str, tag: &str| {
            meta.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| comment(tag))
                .unwrap_or_default()
                .chars()
                .take(1000)
                .collect::<String>()
        };
        let version = text("version", "version");
        let author = text("author", "author");
        let description = text("description", "description");
        let name = meta
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .ok_or("插件缺少名称")?
            .to_owned();
        let name = if name == "LX" {
            comment_name.unwrap_or(name)
        } else {
            name
        };
        if name.len() > 256 {
            return Err("插件名称过长".into());
        }
        let sources = meta
            .get("sources")
            .and_then(Value::as_object)
            .ok_or("插件未声明来源")?
            .iter()
            .filter_map(|(id, data)| {
                if !matches!(id.as_str(), "kw" | "kg" | "tx" | "wy" | "mg" | "bd" | "git") {
                    return None;
                }
                let qualities: Vec<_> = data
                    .get("qualitys")
                    .or_else(|| data.get("qualities"))
                    .and_then(Value::as_array)?
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|q| {
                        matches!(
                            *q,
                            "128k"
                                | "320k"
                                | "flac"
                                | "flac24bit"
                                | "hires"
                                | "atmos"
                                | "atmos_plus"
                                | "master"
                        )
                    })
                    .map(str::to_owned)
                    .collect();
                if qualities.is_empty() {
                    return None;
                }
                Some(PluginSource {
                    id: id.clone(),
                    name: data
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| s.len() <= 128)
                        .unwrap_or(id)
                        .into(),
                    qualities,
                })
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Err("插件没有可用音乐来源和音质".into());
        }
        push_log(
            &logs,
            format!(
                "[info] 插件已加载：{name}{}，来源 {}",
                if version.is_empty() {
                    String::new()
                } else {
                    format!(" v{version}")
                },
                sources
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        );
        Ok(Self {
            code: code.into(),
            name,
            version,
            author,
            description,
            sources,
            logs,
        })
    }
    /// A client without a script: stands in for the playback plugin when music-list plugins
    /// provide playback themselves. It declares no source and resolves nothing.
    pub fn placeholder() -> Self {
        Self {
            code: Arc::from(""),
            name: "音乐列表插件".into(),
            version: String::new(),
            author: String::new(),
            description: String::new(),
            sources: Vec::new(),
            logs: Default::default(),
        }
    }
    /// What the plugin wrote with `console.*` and what the host noted about it, oldest first.
    pub fn logs(&self) -> Vec<LogLine> {
        self.logs
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }
    /// A line from the host (a failed request, a refused URL…).
    pub fn note(&self, text: impl Into<String>) {
        push_log(&self.logs, text.into());
    }
    pub fn resolve(
        &self,
        source: &str,
        song: &Value,
        quality: &str,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        if !self
            .sources
            .iter()
            .any(|s| s.id == source && s.qualities.iter().any(|q| q == quality))
        {
            return Err("插件不支持该来源或音质".into());
        }
        let args =
            serde_json::to_string(&json!([source, song, quality])).map_err(|_| "无效歌曲信息")?;
        let result = execute(
            &self.code,
            &format!("return await __plugin.musicUrl(...{args});"),
            cancelled,
            &self.logs,
        )
        .inspect_err(|error| {
            push_log(&self.logs, format!("[error] 解析 {source} 失败：{error}"))
        })?;
        let url = result
            .as_str()
            .or_else(|| result.get("url").and_then(Value::as_str))
            .or_else(|| result.get("musicUrl").and_then(Value::as_str))
            .ok_or("插件没有返回播放地址")?;
        validate_url(url)?;
        Ok(url.to_owned())
    }
}
pub fn validate_url(raw: &str) -> Result<reqwest::Url, String> {
    if raw.len() > 8192 {
        return Err("播放或请求地址过长".into());
    }
    let url = reqwest::Url::parse(raw).map_err(|_| "无效播放或请求地址")?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("地址只支持无用户信息的 HTTP/HTTPS".into());
    }
    Ok(url)
}
/// Which contract the script is run under: a playback (CR/LX) plugin, or a music-list plugin.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Playback,
    List,
}
fn execute(
    code: &str,
    action: &str,
    cancelled: Arc<AtomicBool>,
    logs: &Logs,
) -> Result<Value, String> {
    execute_as(Kind::Playback, code, action, cancelled, logs)
}
/// What a list plugin sees besides the network: the user's configuration values and a small
/// persistent key-value store (login state, caches). Writes go through `persist`.
#[derive(Clone)]
pub struct PluginEnv {
    pub config: Value,
    pub storage: Arc<Mutex<serde_json::Map<String, Value>>>,
    pub persist: Arc<dyn Fn(&serde_json::Map<String, Value>) + Send + Sync>,
}
const STORAGE_BYTES: usize = 256 * 1024;

pub(crate) fn execute_as(
    kind: Kind,
    code: &str,
    action: &str,
    cancelled: Arc<AtomicBool>,
    logs: &Logs,
) -> Result<Value, String> {
    execute_env(kind, code, action, cancelled, logs, None)
}
/// Shared by every plugin request: one connection pool and TLS session cache instead of a
/// client and runtime thread per `fetch`. Each request carries its own total timeout.
fn plugin_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: std::sync::OnceLock<Option<reqwest::blocking::Client>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::limited(5))
                .gzip(true)
                .brotli(true)
                .build()
                .ok()
        })
        .as_ref()
}
pub(crate) fn execute_env(
    kind: Kind,
    code: &str,
    action: &str,
    cancelled: Arc<AtomicBool>,
    logs: &Logs,
    env: Option<PluginEnv>,
) -> Result<Value, String> {
    let request_limit = if kind == Kind::List { 24 } else { 16 };
    // A list plugin may read a whole web page (a playlist's song list): allow larger bodies.
    let body_limit: usize = if kind == Kind::List { 16 } else { 4 } * 1024 * 1024;
    let deadline = Instant::now() + Duration::from_secs(30);
    let rt = Runtime::new().map_err(|_| "无法创建插件运行时")?;
    rt.set_memory_limit(if kind == Kind::List { 96 } else { 32 } * 1024 * 1024);
    rt.set_max_stack_size(512 * 1024);
    let interrupt_cancel = cancelled.clone();
    rt.set_interrupt_handler(Some(Box::new(move || {
        Instant::now() >= deadline || interrupt_cancel.load(Ordering::Relaxed)
    })));
    let context = Context::full(&rt).map_err(|_| "无法创建插件上下文")?;
    context.with(|ctx| -> Result<Value, String> {
        let count = Arc::new(AtomicUsize::new(0));
        // Preserve a safe transport-error category when playback scripts reject their
        // promise. Their raw exception may contain signed URLs or credentials.
        let network_failed = Arc::new(AtomicBool::new(false));
        let request_failed = network_failed.clone();
        let request_cancel = cancelled.clone();
        let http_call: Arc<dyn Fn(&str, &str) -> Result<Value, String> + Send + Sync> = Arc::new(move |raw: &str, options: &str| -> Result<Value, String> {
                if request_cancel.load(Ordering::Relaxed) || count.fetch_add(1, Ordering::Relaxed) >= request_limit { return Err("插件请求已取消或次数超限".into()); }
                let url = validate_url(raw)?;
                let opts: Value = serde_json::from_str(options).map_err(|_| "无效请求参数")?;
                let method = opts.get("method").and_then(Value::as_str).unwrap_or("GET");
                let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| "无效请求方法")?;
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() { return Err("插件执行超时".into()); }
                let timeout = Duration::from_millis(opts.get("timeout").and_then(Value::as_u64).unwrap_or(15000).clamp(1,15000)).min(remaining);
                let client = plugin_client().ok_or("无法创建插件网络客户端")?;
                let mut request = client.request(method,url).timeout(timeout);
                if let Some(headers) = opts.get("headers").and_then(Value::as_object) {
                    if headers.len() > 64 { return Err("请求头过多".into()); }
                    for (key,value) in headers { if let Some(value) = value.as_str() { request = request.header(key,value); } }
                }
                if let Some(body) = opts.get("body").filter(|v| !v.is_null()) {
                    let body = body.as_str().map(str::to_owned).unwrap_or_else(|| body.to_string());
                    if body.len() > 1024 * 1024 { return Err("插件请求体过大".into()); }
                    request = request.body(body);
                }
                let response = request.send().map_err(|error| {
                    if !error.is_builder() && !error.is_redirect() {
                        request_failed.store(true, Ordering::Relaxed);
                    }
                    "插件网络请求失败或超时"
                })?;
                let status = response.status().as_u16();
                let headers: serde_json::Map<String, Value> = response.headers().iter().filter_map(|(k,v)| v.to_str().ok().map(|s|(k.to_string(),Value::String(s.into())))).collect();
                let mut bytes = Vec::new(); response.take(body_limit as u64 + 1).read_to_end(&mut bytes).map_err(|_| {
                    request_failed.store(true, Ordering::Relaxed);
                    "无法读取插件响应"
                })?;
                if bytes.len() > body_limit { return Err("插件响应过大".into()); }
                if Instant::now() >= deadline || request_cancel.load(Ordering::Relaxed) { return Err("插件执行超时或取消".into()); }
                // Lossy, like the built-in adapters: a source may cut a multi-byte character in a description.
                let body = if opts.get("responseType").and_then(Value::as_str) == Some("base64") {
                    Value::String(STANDARD.encode(&bytes))
                } else {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text))
                };
                Ok(json!({"statusCode":status,"headers":headers,"body":body}))
        });
        let single = http_call.clone();
        ctx.globals().set("__http", Function::new(ctx.clone(), move |raw: String, options: String| -> rquickjs::Result<String> {
            Ok(match single(&raw, &options) { Ok(v) => json!({"result":v}), Err(e) => json!({"error":e}) }.to_string())
        }).map_err(|_| "无法创建插件网络桥")?).map_err(|_| "无法安装插件网络桥")?;
        // Several requests at once (at most 8 in flight), answered in input order.
        let many = http_call.clone();
        ctx.globals().set("__httpMany", Function::new(ctx.clone(), move |batch: String| -> rquickjs::Result<String> {
            let jobs: Vec<Value> = serde_json::from_str(&batch).unwrap_or_default();
            if jobs.len() > 24 { return Ok(json!([{"error":"批量请求过多"}]).to_string()); }
            let results = Mutex::new(vec![json!({"error":"未执行"}); jobs.len()]);
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..jobs.len().min(8) {
                    scope.spawn(|| loop {
                        let index = next.fetch_add(1, Ordering::SeqCst);
                        let Some(job) = jobs.get(index) else { break };
                        let url = job.get("url").and_then(Value::as_str).unwrap_or("");
                        let options = job.get("options").map(Value::to_string).unwrap_or_else(|| "{}".into());
                        let value = match many(url, &options) { Ok(v) => json!({"result":v}), Err(e) => json!({"error":e}) };
                        if let Ok(mut out) = results.lock() { out[index] = value; }
                    });
                }
            });
            Ok(Value::Array(results.into_inner().unwrap_or_default()).to_string())
        }).map_err(|_| "无法创建插件网络桥")?).map_err(|_| "无法安装插件网络桥")?;
        ctx.globals().set("__aes128ecb", Function::new(ctx.clone(), |plain: String, key: String| -> rquickjs::Result<String> {
            use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
            let key: [u8; 16] = key.as_bytes().try_into().map_err(|_| rquickjs::Error::Unknown)?;
            let cipher = aes::Aes128::new(GenericArray::from_slice(&key));
            let pad = 16 - plain.len() % 16;
            let mut buf = plain.into_bytes();
            buf.extend(std::iter::repeat_n(pad as u8, pad));
            for block in buf.as_chunks_mut::<16>().0 { cipher.encrypt_block(GenericArray::from_mut_slice(block)); }
            Ok(buf.iter().map(|b| format!("{b:02X}")).collect::<String>())
        }).map_err(|_| "插件加密失败")?).map_err(|_| "插件加密失败")?;
        // AES-128-ECB, PKCS7, raw key given as hex: `encrypt` takes text and answers base64,
        // `decrypt` takes base64 and answers the (valid UTF-8) text. A bad input throws.
        ctx.globals().set("__aes128ecbB64", Function::new(ctx.clone(), |op: String, data: String, key_hex: String| -> rquickjs::Result<String> {
            use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
            let fail = || rquickjs::Error::Unknown;
            if key_hex.len() != 32 || !key_hex.is_ascii() { return Err(fail()); }
            let mut key = [0u8; 16];
            for (i, byte) in key.iter_mut().enumerate() { *byte = u8::from_str_radix(&key_hex[i * 2..i * 2 + 2], 16).map_err(|_| fail())?; }
            let cipher = aes::Aes128::new(GenericArray::from_slice(&key));
            match op.as_str() {
                "encrypt" => {
                    let pad = 16 - data.len() % 16;
                    let mut buf = data.into_bytes();
                    buf.extend(std::iter::repeat_n(pad as u8, pad));
                    for block in buf.as_chunks_mut::<16>().0 { cipher.encrypt_block(GenericArray::from_mut_slice(block)); }
                    Ok(STANDARD.encode(buf))
                }
                "decrypt" => {
                    let mut buf = STANDARD.decode(data.trim()).map_err(|_| fail())?;
                    if buf.is_empty() || buf.len() % 16 != 0 { return Err(fail()); }
                    for block in buf.as_chunks_mut::<16>().0 { cipher.decrypt_block(GenericArray::from_mut_slice(block)); }
                    let pad = *buf.last().ok_or_else(fail)? as usize;
                    if pad == 0 || pad > 16 || !buf[buf.len() - pad..].iter().all(|b| *b as usize == pad) { return Err(fail()); }
                    buf.truncate(buf.len() - pad);
                    String::from_utf8(buf).map_err(|_| fail())
                }
                _ => Err(fail()),
            }
        }).map_err(|_| "插件加密失败")?).map_err(|_| "插件加密失败")?;
        if let Some(env) = env {
            let config = env.config.to_string();
            ctx.globals().set("__configJson", config).map_err(|_| "无法安装插件配置")?;
            let store = env.storage.clone();
            ctx.globals().set("__storeGet", Function::new(ctx.clone(), move |key: String| -> String {
                store.lock().ok().and_then(|m| m.get(&key).map(Value::to_string)).unwrap_or_default()
            }).map_err(|_| "无法安装插件存储")?).map_err(|_| "无法安装插件存储")?;
            let (store, persist) = (env.storage.clone(), env.persist.clone());
            ctx.globals().set("__storeSet", Function::new(ctx.clone(), move |key: String, json: String| -> rquickjs::Result<()> {
                if key.is_empty() || key.len() > 128 { return Err(rquickjs::Error::Unknown); }
                let value: Value = serde_json::from_str(&json).map_err(|_| rquickjs::Error::Unknown)?;
                let mut map = store.lock().map_err(|_| rquickjs::Error::Unknown)?;
                map.insert(key, value);
                if serde_json::to_vec(&*map).map(|b| b.len()).unwrap_or(usize::MAX) > STORAGE_BYTES { return Err(rquickjs::Error::Unknown); }
                persist(&map);
                Ok(())
            }).map_err(|_| "无法安装插件存储")?).map_err(|_| "无法安装插件存储")?;
            let (store, persist) = (env.storage.clone(), env.persist.clone());
            ctx.globals().set("__storeDel", Function::new(ctx.clone(), move |key: String| {
                if let Ok(mut map) = store.lock() && map.remove(&key).is_some() { persist(&map); }
            }).map_err(|_| "无法安装插件存储")?).map_err(|_| "无法安装插件存储")?;
        }
        let sink = logs.clone();
        ctx.globals().set("__log", Function::new(ctx.clone(), move |level: String, text: String| {
            push_log(&sink, format!("[{}] {text}", level.chars().take(8).collect::<String>()));
        }).map_err(|_| "无法创建插件日志桥")?).map_err(|_| "无法安装插件日志桥")?;
        ctx.globals().set("__now", Function::new(ctx.clone(), || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as f64).map_err(|_| "插件时钟失败")?).map_err(|_| "插件时钟失败")?;
        ctx.globals().set("atob", Function::new(ctx.clone(), |s: String| STANDARD.decode(s).map(|b| b.iter().map(|v|char::from(*v)).collect::<String>()).map_err(|_|rquickjs::Error::Unknown)).map_err(|_| "插件编码失败")?).map_err(|_| "插件编码失败")?;
        ctx.globals().set("btoa", Function::new(ctx.clone(), |s: String| -> rquickjs::Result<String> { let bytes=s.chars().map(|c|u8::try_from(c as u32)).collect::<Result<Vec<_>,_>>().map_err(|_|rquickjs::Error::Unknown)?; Ok(STANDARD.encode(bytes)) }).map_err(|_| "插件编码失败")?).map_err(|_| "插件编码失败")?;
        ctx.globals().set("__validTtml", Function::new(ctx.clone(), |text: String| -> bool {
            text.len()<=2*1024*1024 && mio_music_sdk::parse_ttml(&text).is_ok_and(|lines| lines.iter().any(|line| line.words.iter().any(|word| !word.text.trim().is_empty())))
        }).map_err(|_| "TTML 解析桥初始化失败")?).map_err(|_| "TTML 解析桥初始化失败")?;
        ctx.globals().set("__decodeKuwo", Function::new(ctx.clone(), |body: String, lyricx: bool| -> rquickjs::Result<String> {
            mio_music_sdk::decode_lyric_payload(&json!({"format":"kw","data":{"bytes":body,"isLyricx":lyricx}}), "kw")
                .and_then(|lyric| serde_json::to_string(&lyric).map_err(|_| "歌词格式无效".to_string()))
                .map_err(|_| rquickjs::Error::Unknown)
        }).map_err(|_| "歌词解析桥初始化失败")?).map_err(|_| "歌词解析桥初始化失败")?;
        ctx.globals().set("__sha1", Function::new(ctx.clone(), |s: String| sha1_smol::Sha1::from(s.as_bytes()).digest().to_string()).map_err(|_| "插件摘要失败")?).map_err(|_| "插件摘要失败")?;
        ctx.globals().set("__md5", Function::new(ctx.clone(), |s: String| format!("{:x}", Md5::digest(s.as_bytes()))).map_err(|_| "插件摘要失败")?).map_err(|_| "插件摘要失败")?;
        // Set the host platform before author code and the LX conversion handshake run.
        let runtime = if cfg!(any(target_os = "android", target_os = "ios")) {
            format!("{}\ncerumusic.platform='mobile';lx.env='mobile';", include_str!("runtime.js"))
        } else {
            include_str!("runtime.js").to_owned()
        };
        let script = if kind == Kind::List {
            // A list plugin exports `listPlugin`; there is no LX request handshake.
            format!("{}\n{}\nvar __plugin = (function(){{ var module={{exports:{{}}}}; var exports=module.exports; var global=globalThis; {code}\nreturn module.exports; }})();\n(async function(){{ await __plugin.ready; var __list=__plugin.listPlugin; if(!__list||typeof __list!=='object')throw Error('缺少 listPlugin'); {action} }})().then(v=>JSON.stringify(v===undefined?null:v));", runtime, include_str!("service_adapter.js"))
        } else {
            format!("{}\nvar __plugin = (function(){{ var module={{exports:{{}}}}; var exports=module.exports; var global=globalThis; {code}\nreturn module.exports; }})();\n(async function(){{ await __plugin.ready; if(__lxUsed){{while(!__inited||!__handler) await new Promise(r=>setTimeout(r,5)); __plugin.musicUrl=async(s,info,q)=>__handler({{source:s,action:'musicUrl',info:{{musicInfo:info,type:q}}}}); }} var __sources=__lxUsed?__lxSources:__plugin.sources; if(typeof __plugin.musicUrl!=='function')throw Error('缺少 musicUrl'); {action} }})().then(v=>JSON.stringify(v));", runtime)
        };
        let promise: Promise = ctx.eval(script).map_err(|_| "插件初始化失败或不兼容当前宿主")?;
        let run_timers: Function = ctx.globals().get("__runTimers").map_err(|_| "插件定时回调失败")?;
        loop {
            if let Some(result) = promise.result::<String>() { let text = result.map_err(|error| {
                    if kind != Kind::List && network_failed.load(Ordering::Relaxed) {
                        return "插件网络请求失败或超时".to_owned();
                    }
                    if kind == Kind::List {
                        if let rquickjs::Error::Exception = error {
                            let caught = ctx.catch();
                            let message = caught.as_object().and_then(|o| o.get::<_, String>("message").ok()).or_else(|| caught.as_string().and_then(|s| s.to_string().ok()));
                            if let Some(message) = message { return message.chars().take(300).collect::<String>(); }
                        }
                        return "列表插件执行失败".to_owned();
                    }
                    "插件初始化或播放解析失败".to_owned()
                })?; return serde_json::from_str(&text).map_err(|_| "无效插件返回值".to_owned()); }
            if Instant::now() >= deadline || cancelled.load(Ordering::Relaxed) { return Err("插件执行超时或取消".into()); }
            run_timers.call::<_, ()>(()).map_err(|_| "插件定时回调失败")?;
            // Wait only while nothing is runnable: a resolved request continues at once.
            let mut ran = false;
            for _ in 0..64 { if !ctx.execute_pending_job() { break; } ran = true; }
            if !ran { std::thread::sleep(Duration::from_millis(2)); }
        }
    })
}
