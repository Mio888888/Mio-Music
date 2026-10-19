//! Encrypted S3 backup and restore (`s3_commands.rs` and `S3Backup.ts` of the original).
//!
//! A backup is one object `mio-backup-<UTC time>.enc` holding
//! `base64("MIOB" 0x01 salt(16) nonce(12) AES-256-GCM(JSON))`, the key derived from the password
//! with PBKDF2-HMAC-SHA256 (600 000 rounds). The JSON is
//! `{version: 3, timestamp, playlists, settings, plugins}`, the same document the original writes,
//! so a backup made by either application restores in the other as far as that application has
//! the data. Requests are signed with AWS Signature V4; nothing but the access key id, region,
//! endpoint, bucket and the backup count is kept on disk (never the secret or the password).
mod mobile;
use crate::storage::{Preferences, Storage};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, Mac};
pub use mobile::{MobileSaved, UploadOutcome, upload_prepared};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, time::Duration};

type HmacSha256 = Hmac<Sha256>;

const PBKDF2_ITERATIONS: u32 = 600_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const MAGIC: &[u8; 4] = b"MIOB";
const ENVELOPE_VERSION: u8 = 1;
const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const BACKUP_PREFIX: &str = "mio-backup-";
const MAX_LIST_BYTES: u64 = 8 * 1024 * 1024;
const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;
pub const SAVED_FILE: &str = "s3-backup.json";

/// What the original keeps in `localStorage` (`s3BackupConfig`, `maxBackups`, `lastBackupTime`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub bucket: String,
    pub max_backups: u32,
    pub last_backup_time: Option<String>,
    /// `https://host/bucket/key` (true, the original's only form) or `https://bucket.host/key`.
    pub path_style: bool,
}
impl Default for Saved {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            region: "auto".into(),
            access_key_id: String::new(),
            bucket: String::new(),
            max_backups: 10,
            last_backup_time: None,
            path_style: true,
        }
    }
}
impl Saved {
    pub fn load(store: Option<&Storage>) -> Self {
        let mut saved: Self = store
            .and_then(|s| s.load(SAVED_FILE).ok().flatten())
            .unwrap_or_default();
        if saved.region.trim().is_empty() {
            saved.region = "auto".into();
        }
        saved.max_backups = saved.max_backups.clamp(1, 100);
        saved
    }
    pub fn save(&self, store: &Storage) -> std::io::Result<()> {
        store.save(SAVED_FILE, self)
    }
}

#[derive(Clone, Default)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    /// Path-style addressing; otherwise the bucket is part of the host name.
    pub path_style: bool,
}

/// What a backup holds, collected on this machine.
pub struct Payload {
    pub settings: Value,
    pub plugins: Vec<Value>,
    pub playlists: Value,
}

pub struct BackupDone {
    pub timestamp: String,
    pub plugins: usize,
}

pub struct RestoreDone {
    pub playlists: usize,
    pub plugins: u32,
}

// ---------------------------------------------------------------- envelope

fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    key
}

pub fn encode_envelope(plain: &[u8], password: &str) -> Result<String, String> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let key = derive_key(password, &salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("创建加密器失败: {e}"))?;
    let sealed = cipher
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|e| format!("加密失败: {e}"))?;
    let mut envelope = Vec::with_capacity(MAGIC.len() + 1 + SALT_LEN + NONCE_LEN + sealed.len());
    envelope.extend_from_slice(MAGIC);
    envelope.push(ENVELOPE_VERSION);
    envelope.extend_from_slice(&salt);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&sealed);
    Ok(BASE64.encode(envelope))
}

pub fn decode_envelope(body: &str, password: &str) -> Result<Vec<u8>, String> {
    let envelope = BASE64
        .decode(body.trim())
        .map_err(|_| "备份格式错误：不是有效的加密备份".to_string())?;
    if envelope.len() <= MAGIC.len() || !envelope.starts_with(MAGIC) {
        return Err("备份格式错误：缺少加密信封".into());
    }
    if envelope[MAGIC.len()] != ENVELOPE_VERSION {
        return Err("备份格式错误：不支持的信封版本".into());
    }
    let sealed = &envelope[MAGIC.len() + 1..];
    if sealed.len() < SALT_LEN + NONCE_LEN + 16 {
        return Err("加密数据格式错误".into());
    }
    let (salt, rest) = sealed.split_at(SALT_LEN);
    let (nonce, cipher_text) = rest.split_at(NONCE_LEN);
    let key = derive_key(password, salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("创建解密器失败: {e}"))?;
    cipher
        .decrypt(Nonce::from_slice(nonce), cipher_text)
        .map_err(|_| "解密失败：密码错误或数据损坏".to_string())
}

// ---------------------------------------------------------------- signing

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

struct Target {
    /// Virtual-hosted style: the bucket is in the host name and not in the path.
    virtual_hosted: bool,
    base: String,
    host: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
}

fn resolve_region(region: &str) -> String {
    let region = region.trim();
    if region.is_empty() || region == "auto" {
        "us-east-1".into()
    } else {
        region.into()
    }
}

/// `resolve_endpoint_and_bucket`: the bucket is the first path segment of the address, or the
/// separate bucket field.
fn target(config: &S3Config) -> Result<Target, String> {
    let endpoint = config.endpoint.trim().trim_end_matches('/');
    let (scheme, rest) = if let Some(rest) = endpoint.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err("服务地址格式错误，需要 http:// 或 https://".into());
    };
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or("").trim();
    if host.is_empty() {
        return Err("服务地址缺少 host".into());
    }
    let bucket = match parts.find(|s| !s.trim().is_empty()) {
        Some(bucket) => bucket.trim().to_owned(),
        None if !config.bucket.trim().is_empty() => config.bucket.trim().to_owned(),
        None => {
            return Err("缺少存储桶：请在 bucket 字段填写，或在 endpoint 后追加 /bucket".into());
        }
    };
    if config.access_key_id.is_empty() || config.secret_access_key.is_empty() {
        return Err("缺少访问密钥".into());
    }
    let virtual_hosted = !config.path_style;
    let host = if virtual_hosted {
        format!("{bucket}.{host}")
    } else {
        host.to_owned()
    };
    Ok(Target {
        virtual_hosted,
        base: format!("{scheme}://{host}"),
        host,
        bucket,
        region: resolve_region(&config.region),
        access_key: config.access_key_id.clone(),
        secret_key: config.secret_access_key.clone(),
    })
}

impl Target {
    /// `S3Signer::sign`: the `Authorization` value and the `x-amz-date` it was made for.
    fn sign(
        &self,
        method: &str,
        canonical_uri: &str,
        canonical_query: &str,
        payload_hash: &str,
        content_type: Option<&str>,
    ) -> (String, String) {
        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("host", self.host.clone());
        headers.insert("x-amz-content-sha256", payload_hash.to_owned());
        headers.insert("x-amz-date", amz_date.clone());
        if let Some(content_type) = content_type {
            headers.insert("content-type", content_type.to_owned());
        }
        let signed_headers = headers.keys().copied().collect::<Vec<_>>().join(";");
        let canonical_headers: String = headers
            .iter()
            .map(|(k, v)| format!("{k}:{}\n", v.trim()))
            .collect();
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let scope = format!("{date_stamp}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let k_date = hmac_sha256(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date_stamp.as_bytes(),
        );
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key
        );
        (authorization, amz_date)
    }

    fn request(
        &self,
        client: &reqwest::blocking::Client,
        method: reqwest::Method,
        key: Option<&str>,
        query: &str,
        body: Option<String>,
    ) -> Result<reqwest::blocking::Response, reqwest::Error> {
        let canonical_uri = match (key, self.virtual_hosted) {
            (Some(key), false) => {
                format!("/{}/{}", uri_component(&self.bucket), uri_component(key))
            }
            (None, false) => format!("/{}", uri_component(&self.bucket)),
            (Some(key), true) => format!("/{}", uri_component(key)),
            (None, true) => "/".to_owned(),
        };
        let payload_hash = body
            .as_ref()
            .map(|body| sha256_hex(body.as_bytes()))
            .unwrap_or_else(|| EMPTY_HASH.into());
        let content_type = body.as_ref().map(|_| "application/octet-stream");
        let (authorization, amz_date) = self.sign(
            method.as_str(),
            &canonical_uri,
            query,
            &payload_hash,
            content_type,
        );
        let mut url = format!("{}{canonical_uri}", self.base);
        if !query.is_empty() {
            url.push('?');
            url.push_str(query);
        }
        let mut request = client
            .request(method, url)
            .header("Authorization", authorization)
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", amz_date);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/octet-stream")
                .body(body);
        }
        request.send()
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        // A signed request is for one host; a redirect would only fail or leak the signature.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("连接失败: {e}"))
}

fn excerpt(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

fn read_limited(response: reqwest::blocking::Response, limit: u64) -> Result<String, String> {
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("读取响应失败: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err("响应数据超过上限".into());
    }
    String::from_utf8(bytes).map_err(|_| "响应不是有效文本".into())
}

fn failure(prefix: &str, response: reqwest::blocking::Response) -> String {
    let status = response.status();
    let body = read_limited(response, 64 * 1024).unwrap_or_default();
    format!("{prefix}: HTTP {status} - {}", excerpt(&body, 200))
}

// SigV4 uses UTF-8 bytes, uppercase hex and %20 rather than form encoding.
fn uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 15) as usize] as char);
        }
    }
    result
}

struct ListPage {
    objects: Vec<(String, chrono::DateTime<chrono::FixedOffset>)>,
    next: Option<String>,
}
fn list_field<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Result<Option<String>, String> {
    let mut matches = node
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == name);
    let Some(child) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some()
        || child.tag_name().namespace() != node.tag_name().namespace()
        || child.children().any(|n| !n.is_text())
    {
        return Err("invalid_data: S3 list field".into());
    }
    Ok(Some(child.children().filter_map(|n| n.text()).collect()))
}
fn list_page(xml: &str, expected_token: Option<&str>, bucket: &str) -> Result<ListPage, String> {
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 50_000,
            ..Default::default()
        },
    )
    .map_err(|_| "invalid_data: S3 list XML")?;
    let root = document.root_element();
    if root.tag_name().name() != "ListBucketResult"
        || !matches!(
            root.tag_name().namespace(),
            None | Some("http://s3.amazonaws.com/doc/2006-03-01/")
        )
    {
        return Err("invalid_data: S3 list root".into());
    }
    if list_field(root, "Name")?.as_deref() != Some(bucket) {
        return Err("invalid_data: S3 bucket mismatch".into());
    }
    if root
        .children()
        .any(|n| n.is_element() && n.tag_name().name() == "CommonPrefixes")
        || list_field(root, "Delimiter")?.is_some_and(|v| !v.is_empty())
        || list_field(root, "StartAfter")?.is_some_and(|v| !v.is_empty())
    {
        return Err("incomplete_data: grouped or skipped S3 list".into());
    }
    if list_field(root, "Prefix")?.as_deref() != Some(BACKUP_PREFIX) {
        return Err("invalid_data: S3 list prefix".into());
    }
    if list_field(root, "ContinuationToken")?.as_deref() != expected_token {
        return Err("invalid_data: S3 list token mismatch".into());
    }
    if list_field(root, "EncodingType")?.is_some_and(|v| !v.is_empty()) {
        return Err("invalid_data: unexpected S3 key encoding".into());
    }
    let truncated = match list_field(root, "IsTruncated")?.as_deref() {
        Some("true") => true,
        Some("false") => false,
        _ => return Err("invalid_data: S3 truncation marker".into()),
    };
    let key_count = list_field(root, "KeyCount")?
        .ok_or("invalid_data: S3 key count")?
        .parse::<usize>()
        .map_err(|_| "invalid_data: S3 key count")?;
    if key_count > 1000 {
        return Err("capacity_exceeded: S3 list page".into());
    }
    let mut objects = Vec::new();
    for child in root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "Contents")
    {
        if child.tag_name().namespace() != root.tag_name().namespace() {
            return Err("invalid_data: S3 contents namespace".into());
        }
        if objects.len() >= 1000 {
            return Err("capacity_exceeded: S3 list page".into());
        }
        let key = list_field(child, "Key")?.ok_or("invalid_data: S3 object key")?;
        let modified = list_field(child, "LastModified")?.ok_or("invalid_data: S3 object time")?;
        if key.len() > 1024 || !key.starts_with(BACKUP_PREFIX) || key.chars().any(char::is_control)
        {
            return Err("invalid_data: S3 object key".into());
        }
        let modified = chrono::DateTime::parse_from_rfc3339(&modified)
            .map_err(|_| "invalid_data: S3 object time")?;
        objects.push((key, modified));
    }
    if objects.len() != key_count {
        return Err("incomplete_data: S3 list count".into());
    }
    let next = list_field(root, "NextContinuationToken")?;
    let next = if truncated {
        let token = next
            .filter(|t| !t.is_empty() && t.len() <= 16 * 1024 && !t.chars().any(char::is_control))
            .ok_or("incomplete_data: S3 next token")?;
        if objects.is_empty() {
            return Err("incomplete_data: empty truncated S3 page".into());
        }
        Some(token)
    } else {
        if next.as_ref().is_some_and(|t| !t.is_empty()) {
            return Err("invalid_data: unexpected S3 next token".into());
        }
        None
    };
    Ok(ListPage { objects, next })
}

// ---------------------------------------------------------------- operations

pub fn test_connection(config: &S3Config) -> Result<(), String> {
    let target = target(config)?;
    let client = client()?;
    let response = target
        .request(
            &client,
            reqwest::Method::GET,
            None,
            "list-type=2&max-keys=1",
            None,
        )
        .map_err(|e| format!("连接失败: {e}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(failure("连接失败", response))
    }
}

/// Read every bounded page before returning any object for fetch or retention.
fn list_backups(
    target: &Target,
    client: &reqwest::blocking::Client,
) -> Result<Vec<(String, chrono::DateTime<chrono::FixedOffset>)>, String> {
    list_backups_checked(target, client, &|| Ok(()))
}
fn list_backups_checked(
    target: &Target,
    client: &reqwest::blocking::Client,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<Vec<(String, chrono::DateTime<chrono::FixedOffset>)>, String> {
    let mut objects = Vec::new();
    let mut keys = std::collections::HashSet::new();
    let mut tokens = std::collections::HashSet::new();
    let mut token: Option<String> = None;
    let mut bytes = 0usize;
    for _ in 0..100 {
        check()?;
        // Canonical query fields are sorted by encoded name before signing.
        let mut query = format!("list-type=2&max-keys=1000&prefix={BACKUP_PREFIX}");
        if let Some(token) = &token {
            query = format!("continuation-token={}&{query}", uri_component(token));
        }
        let response = target
            .request(client, reqwest::Method::GET, None, &query, None)
            .map_err(|_| "network_failed: S3 list request")?;
        if !response.status().is_success() {
            return Err(format!("列出备份失败: HTTP {}", response.status()));
        }
        check()?;
        let xml = read_limited_checked(response, MAX_LIST_BYTES, check)?;
        bytes = bytes
            .checked_add(xml.len())
            .filter(|n| *n <= 32 * 1024 * 1024)
            .ok_or("capacity_exceeded: S3 list bytes")?;
        let page = list_page(&xml, token.as_deref(), &target.bucket)?;
        for object in page.objects {
            check()?;
            if keys.len() >= 20_000 {
                return Err("capacity_exceeded: S3 objects".into());
            }
            if !keys.insert(object.0.clone()) {
                return Err("incomplete_data: duplicate S3 key".into());
            }
            // Only this application's flat encrypted backup objects are eligible.
            if object.0.ends_with(".enc")
                && !object.0.contains('/')
                && object.0.len() > BACKUP_PREFIX.len() + 4
            {
                objects.push(object);
            }
        }
        match page.next {
            Some(next) => {
                if !tokens.insert(next.clone()) {
                    return Err("incomplete_data: repeated S3 token".into());
                }
                token = Some(next);
            }
            None => {
                objects.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
                check()?;
                return Ok(objects);
            }
        }
    }
    Err("capacity_exceeded: S3 list pages".into())
}

/// Uploads one backup and removes the oldest ones beyond `max_backups`.
pub fn backup(
    config: &S3Config,
    password: &str,
    max_backups: u32,
    payload: &Payload,
) -> Result<BackupDone, String> {
    if !payload.settings.is_object() {
        return Err("备份设置格式错误".into());
    }
    let target = target(config)?;
    let now = chrono::Utc::now();
    let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let key = format!("{BACKUP_PREFIX}{}.enc", now.format("%Y-%m-%dT%H-%M-%S-%fZ"));
    let document = json!({
        "version": 3,
        "timestamp": timestamp,
        "playlists": payload.playlists,
        "settings": payload.settings,
        "plugins": payload.plugins,
    });
    let plain = serde_json::to_vec(&document).map_err(|e| format!("序列化失败: {e}"))?;
    let body = encode_envelope(&plain, password)?;
    let client = client()?;
    let response = target
        .request(&client, reqwest::Method::PUT, Some(&key), "", Some(body))
        .map_err(|e| format!("上传失败: {e}"))?;
    if !response.status().is_success() {
        return Err(failure("上传失败", response));
    }
    // Removing old backups is best effort: a failure here never fails the backup.
    if let Ok(objects) = list_backups(&target, &client) {
        for (old, _) in objects.iter().skip(max_backups.max(1) as usize) {
            if old != &key {
                let _ = target.request(&client, reqwest::Method::DELETE, Some(old), "", None);
            }
        }
    }
    Ok(BackupDone {
        timestamp,
        plugins: payload.plugins.len(),
    })
}

/// Downloads and decrypts the newest backup and checks its document.
pub fn fetch_latest(config: &S3Config, password: &str) -> Result<Value, String> {
    let target = target(config)?;
    let client = client()?;
    let objects = list_backups(&target, &client)?;
    let (latest, _) = objects.first().ok_or("未找到备份数据")?;
    let response = target
        .request(&client, reqwest::Method::GET, Some(latest), "", None)
        .map_err(|e| format!("下载备份失败: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("下载备份失败: HTTP {}", response.status()));
    }
    let body = read_limited(response, MAX_BACKUP_BYTES)?;
    let plain = decode_envelope(&body, password)?;
    let data: Value =
        serde_json::from_slice(&plain).map_err(|e| format!("解析备份数据失败: {e}"))?;
    validate(&data)?;
    Ok(data)
}

pub fn validate(data: &Value) -> Result<(), String> {
    if data.get("version").and_then(Value::as_u64) != Some(3) {
        return Err("不支持的备份数据版本".into());
    }
    if !data.get("settings").is_some_and(Value::is_object) {
        return Err("备份设置格式错误".into());
    }
    let playlists = data.get("playlists").ok_or("备份缺少歌单数据")?;
    crate::library::check_backup(playlists).ok_or("歌单备份格式错误")?;
    Ok(())
}

// Host-selected directories are metadata only; backup settings never restore them.
pub struct Directories<'a> {
    pub cache_dir: &'a str,
    pub download_dir: &'a str,
}

/// This application's side of a backup: playlists from the library database, the plugin that
/// is installed, and the settings in the original's `appSettings` terms.
pub fn collect(
    store: &Storage,
    preferences: &Preferences,
    directories: &Directories<'_>,
) -> Result<Payload, String> {
    let playlists = crate::library::export_backup(&store.library_path())
        .map_err(|_| "导出歌单失败".to_string())?;
    Ok(Payload {
        settings: settings_json(preferences, directories),
        plugins: installed_plugins(store)?,
        playlists,
    })
}

/// Installed registry entries in the original's `{id, name, code, config}` backup format.
fn installed_plugins(store: &Storage) -> Result<Vec<Value>, String> {
    crate::online::Online::backup_plugins(store)
}

/// `appSettings` of the original.
pub fn settings_json(preferences: &Preferences, directories: &Directories<'_>) -> Value {
    let app = &preferences.app;
    let background = &preferences.background;
    json!({
        "language": preferences.language.clone().unwrap_or_else(|| "system".into()),
        "autoCacheMusic": app.auto_cache_music,
        "cacheSizeLimit": app.cache_size_limit,
        "directories": {
            "cacheDir": directories.cache_dir,
            "downloadDir": directories.download_dir,
        },
        "filenameTemplate": app.filename_template,
        "tagWriteOptions": preferences.tag_write_options,
        "autoUpdate": app.auto_update,
        "autoImportPlaylistOnOpen": false,
        "suppressImportPrompt": false,
        "lyricFontFamily": app.lyric_font_family,
        "lyricFontSize": 36,
        "FullPlayLyricFontRate": app.lyric_font_rate,
        "lyricFontWeight": app.lyric_font_weight,
        "closeToTray": preferences.close_to_tray,
        "hasConfiguredCloseBehavior": preferences.close_behavior_configured,
        "routePreloadEnabled": app.route_preload,
        "backgroundRender": {
            "fullPlay": {
                "preset": match background.preset {
                    crate::background_config::Preset::Auto => "auto",
                    crate::background_config::Preset::Performance => "performance",
                    crate::background_config::Preset::Quality => "quality",
                    crate::background_config::Preset::Custom => "custom",
                },
                "enabled": background.enabled,
                "audioResponse": background.audio_response,
                "renderScale": background.render_scale,
                "flowSpeed": background.flow_speed,
                "staticMode": background.static_mode,
                "fps": background.fps,
            }
        },
        "skipHiddenFiles": app.skip_hidden_files,
    })
}

/// How a settings document meets this machine's settings.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// The document's values, the defaults for what it lacks (`mode: overwrite`).
    Overwrite,
    /// Every value this machine already has wins (the original's `{...backup, ...local}`).
    Merge,
    /// The document's values win; what it lacks stays as it is (first-run import).
    Import,
}

/// Puts a backup's `settings` into `preferences` (see [`Mode`]). Directories are not applied:
/// the original never reads them back from `appSettings` either.
pub fn apply_settings(
    preferences: &mut Preferences,
    directories: &Directories<'_>,
    backup: &Value,
    mode: Mode,
) {
    let overwrite = mode == Mode::Overwrite;
    let merge = mode == Mode::Merge;
    let defaults = settings_json(&Preferences::default(), directories);
    let mut effective = if overwrite {
        defaults.as_object().cloned().unwrap_or_default()
    } else {
        serde_json::Map::new()
    };
    if let Some(backup) = backup.as_object() {
        for (key, value) in backup {
            effective.insert(key.clone(), value.clone());
        }
    }
    if merge {
        if let Some(local) = settings_json(preferences, directories).as_object() {
            for (key, value) in local {
                effective.insert(key.clone(), value.clone());
            }
        }
    }
    let get = |key: &str| effective.get(key);
    let app = &mut preferences.app;
    if let Some(v) = get("autoCacheMusic").and_then(Value::as_bool) {
        app.auto_cache_music = v;
    }
    if let Some(v) = get("cacheSizeLimit")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.)
    {
        app.cache_size_limit = v as u64;
    }
    if let Some(v) = get("filenameTemplate")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
    {
        app.filename_template = v.to_owned();
    }
    if let Some(v) = get("autoUpdate").and_then(Value::as_bool) {
        app.auto_update = v;
    }
    if let Some(v) = get("lyricFontFamily")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
    {
        app.lyric_font_family = v.to_owned();
    }
    if let Some(v) = get("FullPlayLyricFontRate").and_then(Value::as_f64) {
        // The original resets a rate outside 0.1–2 to 1.
        app.lyric_font_rate = if (0.1..=2.).contains(&v) {
            v as f32
        } else {
            1.
        };
    }
    if let Some(v) = get("lyricFontWeight").and_then(Value::as_u64) {
        app.lyric_font_weight = (v as u32).clamp(100, 900);
    }
    if let Some(v) = get("routePreloadEnabled").and_then(Value::as_bool) {
        app.route_preload = v;
    }
    if let Some(v) = get("skipHiddenFiles").and_then(Value::as_bool) {
        app.skip_hidden_files = v;
    }
    if let Some(v) = get("closeToTray").and_then(Value::as_bool) {
        preferences.close_to_tray = v;
    }
    if let Some(v) = get("hasConfiguredCloseBehavior").and_then(Value::as_bool) {
        preferences.close_behavior_configured = v;
    }
    if let Some(language) = get("language")
        .and_then(Value::as_str)
        .filter(|v| matches!(*v, "system" | "zh-CN" | "en-US"))
    {
        preferences.language = Some(language.to_owned());
        preferences.english = preferences.english();
    }
    if let Some(options) = get("tagWriteOptions").filter(|v| v.is_object()) {
        let mut tags = preferences.tag_write_options.clone();
        let flag =
            |key: &str, current: bool| options.get(key).and_then(Value::as_bool).unwrap_or(current);
        tags.basic_info = flag("basicInfo", tags.basic_info);
        tags.cover = flag("cover", tags.cover);
        tags.lyrics = flag("lyrics", tags.lyrics);
        tags.download_lyrics = flag("downloadLyrics", tags.download_lyrics);
        if let Some(format) = options
            .get("lyricFormat")
            .and_then(Value::as_str)
            .filter(|v| matches!(*v, "lrc" | "word-by-word"))
        {
            tags.lyric_format = format.to_owned();
        }
        preferences.tag_write_options = tags;
    }
    if let Some(full) = get("backgroundRender")
        .and_then(|v| v.get("fullPlay"))
        .filter(|v| v.is_object())
    {
        use crate::background_config::Preset;
        let mut config = preferences.background;
        if let Some(preset) = full.get("preset").and_then(Value::as_str) {
            config.preset = match preset {
                "performance" => Preset::Performance,
                "quality" => Preset::Quality,
                "custom" => Preset::Custom,
                "auto" => Preset::Auto,
                _ => config.preset,
            };
        }
        if let Some(v) = full.get("enabled").and_then(Value::as_bool) {
            config.enabled = v;
        }
        if let Some(v) = full.get("audioResponse").and_then(Value::as_bool) {
            config.audio_response = v;
        }
        if let Some(v) = full.get("renderScale").and_then(Value::as_f64) {
            config.render_scale = v as f32;
        }
        if let Some(v) = full.get("flowSpeed").and_then(Value::as_f64) {
            config.flow_speed = v as f32;
        }
        if let Some(v) = full.get("staticMode").and_then(Value::as_bool) {
            config.static_mode = v;
        }
        if let Some(v) = full.get("fps").and_then(Value::as_u64) {
            config.fps = v.min(u32::MAX as u64) as u32;
        }
        preferences.background = config.validated();
    }
}

/// Mobile preparation is read-only and uses bounded/cancellable IO and stricter
/// document validation. Desktop legacy fetch/apply semantics remain separate.
pub fn fetch_latest_prepared(
    config: S3Config,
    password: &str,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<crate::backup_restore::PreparedRestore, String> {
    check()?;
    if password.is_empty() || password.len() > 4096 {
        return Err("invalid_input: backup password".into());
    }
    let config = mobile_config(config)?;
    let target = target(&config)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "unavailable: S3 client")?;
    let objects = list_backups_checked(&target, &client, check)?;
    let (key, _) = objects.first().ok_or("not_found: backup")?;
    check()?;
    let response = target
        .request(&client, reqwest::Method::GET, Some(key), "", None)
        .map_err(|_| "network_failed: backup download")?;
    check()?;
    if !response.status().is_success() {
        return Err(format!("network_failed: backup HTTP {}", response.status()));
    }
    let body = read_limited_checked(
        response,
        crate::backup_restore::MAX_ENCRYPTED_BYTES as u64,
        check,
    )?;
    crate::backup_restore::PreparedRestore::from_encrypted(body, password, check)
}
fn read_limited_checked(
    mut response: reqwest::blocking::Response,
    limit: u64,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<String, String> {
    check()?;
    if response.content_length().is_some_and(|n| n > limit) {
        return Err("capacity_exceeded: S3 response".into());
    }
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check()?;
        let remaining =
            ((limit + 1).saturating_sub(bytes.len() as u64)).min(buffer.len() as u64) as usize;
        let read = response
            .read(&mut buffer[..remaining])
            .map_err(|_| "network_failed: S3 response")?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() as u64 > limit {
            return Err("capacity_exceeded: S3 response".into());
        }
    }
    check()?;
    String::from_utf8(bytes).map_err(|_| "invalid_data: S3 response text".into())
}
/// Validate and canonicalize only explicitly supplied endpoints; credentials
/// stay in memory and errors contain no endpoint, key, password or provider body.
fn mobile_config(mut config: S3Config) -> Result<S3Config, String> {
    let invalid = || "invalid_input: S3 configuration".to_owned();
    for value in [
        &config.endpoint,
        &config.region,
        &config.access_key_id,
        &config.secret_access_key,
        &config.bucket,
    ] {
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(invalid());
        }
    }
    let url = reqwest::Url::parse(config.endpoint.trim()).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let path_bucket = url.path().trim_matches('/');
    let bucket = if path_bucket.is_empty() {
        config.bucket.trim()
    } else {
        if !config.bucket.trim().is_empty() && config.bucket.trim() != path_bucket {
            return Err(invalid());
        }
        path_bucket
    };
    if bucket.is_empty()
        || bucket.len() > 255
        || matches!(bucket, "." | "..")
        || !bucket
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(invalid());
    }
    if !config.path_style
        && url
            .host_str()
            .is_some_and(|h| h.contains(':') || h.contains('['))
    {
        return Err(invalid());
    }
    let region = config.region.trim();
    if region.len() > 128
        || !region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || config.access_key_id.is_empty()
        || config.access_key_id.len() > 256
        || !config
            .access_key_id
            .bytes()
            .all(|b| b.is_ascii_graphic() && !b"/ ,".contains(&b))
        || config.secret_access_key.is_empty()
    {
        return Err(invalid());
    }
    config.bucket = bucket.to_owned();
    config.endpoint = url.origin().ascii_serialization();
    config.region = region.to_owned();
    Ok(config)
}
