//! In-app updates (`useAppUpdater.ts` on top of Tauri's updater plugin): ask a manifest for a
//! newer version, download its package for this platform, check its minisign signature against
//! the project's key, install it over the running application and start it again.
//!
//! The manifest is the plugin's `latest.json`: `version`, `notes`, and per target
//! (`darwin-aarch64`, `windows-x86_64`, `linux-x86_64`…) an `url` and the `signature` text.
//! The address comes from `MIO_GPUI_UPDATE_URL`, read at run time first and otherwise the value the
//! binary was compiled with (release CI bakes in the repository's `latest.json`); without either
//! nothing is ever fetched.
use std::io::Read;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

/// The updater public key of the project (`plugins.updater.pubkey` of the original), the
/// minisign key file text in base64.
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDdEQzM2NUNGNjI1RUY2MEQKUldRTjlsNWl6MlhEZlcrVDM0V2YzWWUxSTNXYXprbmNzdUZQK2xYY1hKWUhQZFdBOTZVR2w3eTMK";
pub const ENDPOINT_ENV: &str = "MIO_GPUI_UPDATE_URL";
/// The channel fixed when the binary is built, used when the run-time variable is not set.
const BUILT_IN_ENDPOINT: Option<&str> = option_env!("MIO_GPUI_UPDATE_URL");
/// A channel signed with another key names its public key (the key file's text, base64).
const PUBKEY_ENV: &str = "MIO_GPUI_UPDATE_PUBKEY";
const MAX_PACKAGE_BYTES: u64 = 1024 * 1024 * 1024;

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A newer version the manifest offers for this platform.
#[derive(Debug, Clone)]
pub struct Update {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub signature: String,
}

fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn arch() -> &'static str {
    if cfg!(target_arch = "x86") {
        "i686"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "arm") {
        "armv7"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "riscv64"
    }
}

/// The manifest keys to look for, most specific first.
fn target_keys() -> Vec<String> {
    let base = format!("{}-{}", os(), arch());
    let bundles: &[&str] = if cfg!(target_os = "macos") {
        &["app"]
    } else if cfg!(target_os = "windows") {
        &["nsis", "msi"]
    } else {
        &["appimage", "deb", "rpm"]
    };
    bundles
        .iter()
        .map(|b| format!("{base}-{b}"))
        .chain(std::iter::once(base.clone()))
        .collect()
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(1800))
        .user_agent(format!("mio-music-gpui-updater/{}", current_version()))
        .build()
        .map_err(|e| e.to_string())
}

/// The manifest address: the run-time variable, else the one compiled in.
pub fn endpoint() -> Option<String> {
    let configured = std::env::var(ENDPOINT_ENV).unwrap_or_default();
    let configured = configured.trim();
    if !configured.is_empty() {
        return Some(configured.to_owned());
    }
    BUILT_IN_ENDPOINT
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Asks the manifest; `Ok(None)` means this version is current.
pub fn check() -> Result<Option<Update>, String> {
    let endpoint = endpoint().ok_or("更新通道尚未配置")?;
    let url = endpoint
        .replace("{{current_version}}", current_version())
        .replace("{{target}}", os())
        .replace("{{arch}}", arch());
    let url = reqwest::Url::parse(&url).map_err(|e| format!("更新地址无效: {e}"))?;
    if url.scheme() != "https" && !cfg!(debug_assertions) {
        return Err("更新地址必须使用 HTTPS".into());
    }
    let response = client()?
        .get(url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("检查更新失败: {e}"))?;
    if response.status().as_u16() == 204 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("更新服务返回 HTTP {}", response.status().as_u16()));
    }
    let mut body = Vec::new();
    response
        .take(2 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "更新清单格式无效".to_owned())?;
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("更新清单缺少 version")?
        .trim_start_matches('v')
        .to_owned();
    let remote = semver::Version::parse(&version).map_err(|_| "更新清单的版本号无效".to_owned())?;
    let local = semver::Version::parse(current_version()).map_err(|e| e.to_string())?;
    if remote <= local {
        return Ok(None);
    }
    let platform = match manifest.get("platforms").and_then(|p| p.as_object()) {
        Some(platforms) => target_keys()
            .iter()
            .find_map(|key| platforms.get(key))
            .cloned()
            .ok_or("更新清单没有适用于当前平台的安装包")?,
        None => manifest.clone(),
    };
    let field = |name: &str| {
        platform
            .get(name)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    };
    Ok(Some(Update {
        version,
        notes: manifest
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        url: field("url").ok_or("更新清单缺少下载地址")?,
        signature: field("signature").ok_or("更新清单缺少签名")?,
    }))
}

/// Downloads the package and checks its signature; `progress(received, total)` is called per
/// chunk.
pub fn download(
    update: &Update,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, String> {
    let url = reqwest::Url::parse(&update.url).map_err(|e| format!("下载地址无效: {e}"))?;
    if url.scheme() != "https" && !cfg!(debug_assertions) {
        return Err("下载地址必须使用 HTTPS".into());
    }
    let mut response = client()?
        .get(url)
        .header("Accept", "application/octet-stream")
        .send()
        .map_err(|e| format!("下载更新失败: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("下载更新失败: HTTP {}", response.status().as_u16()));
    }
    let total = response.content_length();
    if total.is_some_and(|n| n > MAX_PACKAGE_BYTES) {
        return Err("更新包过大".into());
    }
    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(MAX_PACKAGE_BYTES) as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut chunk)
            .map_err(|e| format!("更新下载中断: {e}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() as u64 > MAX_PACKAGE_BYTES {
            return Err("更新包过大".into());
        }
        progress(bytes.len() as u64, total);
    }
    verify(&bytes, &update.signature)?;
    Ok(bytes)
}

fn base64_text(value: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| "签名格式无效".to_owned())?;
    String::from_utf8(bytes).map_err(|_| "签名格式无效".to_owned())
}

/// The package must be signed with the project's key.
fn verify(data: &[u8], signature: &str) -> Result<(), String> {
    let configured = std::env::var(PUBKEY_ENV).unwrap_or_default();
    let pubkey = if configured.trim().is_empty() {
        PUBKEY
    } else {
        configured.trim()
    };
    let key = minisign_verify::PublicKey::decode(&base64_text(pubkey)?)
        .map_err(|_| "更新公钥无效".to_owned())?;
    let signature = minisign_verify::Signature::decode(&base64_text(signature)?)
        .map_err(|_| "签名格式无效".to_owned())?;
    key.verify(data, &signature, true)
        .map_err(|_| "更新包签名校验失败".to_owned())
}

/// Puts the verified package in place of the running application.
pub fn install(bytes: &[u8]) -> Result<(), String> {
    install_inner(bytes)
}

#[cfg(target_os = "macos")]
fn install_inner(bytes: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // `<Name>.app/Contents/MacOS/<exe>`
    let bundle: PathBuf = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or("当前不是从应用包运行，无法自动安装更新")?;
    let extract = tempfile::Builder::new()
        .prefix("mio_updated_app")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let backup = tempfile::Builder::new()
        .prefix("mio_current_app")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        // The first component is the bundle's own folder name.
        let relative: PathBuf = entry
            .path()
            .map_err(|e| e.to_string())?
            .iter()
            .skip(1)
            .collect();
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        let target = extract.path().join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        entry.unpack(&target).map_err(|e| e.to_string())?;
    }
    // Old bundle aside, new one in; the old one comes back if the move fails.
    let aside = backup.path().join("current_app");
    std::fs::rename(&bundle, &aside).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "没有权限替换应用，请把应用放在可写目录或手动更新".to_owned()
        } else {
            e.to_string()
        }
    })?;
    if let Err(error) = std::fs::rename(extract.path(), &bundle).or_else(|_| {
        // Different volumes: copy then remove.
        copy_dir(extract.path(), &bundle)
    }) {
        let _ = std::fs::remove_dir_all(&bundle);
        let _ = std::fs::rename(&aside, &bundle);
        return Err(error.to_string());
    }
    let _ = std::process::Command::new("touch").arg(&bundle).status();
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let link = std::fs::read_link(entry.path())?;
            std::os::unix::fs::symlink(link, target)?;
        } else if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_inner(bytes: &[u8]) -> Result<(), String> {
    let (extension, args): (&str, Vec<&str>) = if bytes.starts_with(b"MZ") {
        ("exe", vec!["/P", "/UPDATE"])
    } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        ("msi", vec!["/passive", "/promptrestart"])
    } else {
        return Err("不支持的更新包格式".into());
    };
    let directory = tempfile::Builder::new()
        .prefix("mio-music-updater-")
        .tempdir()
        .map_err(|e| e.to_string())?
        .keep();
    let path = directory.join(format!("mio-music-installer.{extension}"));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let mut command = if extension == "msi" {
        let mut c = std::process::Command::new("msiexec.exe");
        c.arg("/i").arg(&path);
        c
    } else {
        std::process::Command::new(&path)
    };
    command.args(args).spawn().map_err(|e| e.to_string())?;
    // The installer replaces files this process holds.
    mio_core::storage::flush_session_writes();
    std::process::exit(0);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_inner(bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let target = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .ok_or("仅支持 AppImage 的自动更新，请手动安装更新包")?;
    let permissions = std::fs::metadata(&target)
        .map_err(|e| e.to_string())?
        .permissions();
    let staged = target.with_extension("update");
    let image: Vec<u8> = if bytes.starts_with(&[0x1f, 0x8b]) {
        let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = tar::Archive::new(decoder);
        let mut found = None;
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let is_image = entry
                .path()
                .ok()
                .is_some_and(|p| p.extension().is_some_and(|e| e == "AppImage"));
            if is_image {
                let mut data = Vec::new();
                entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
                found = Some(data);
                break;
            }
        }
        found.ok_or("更新包中没有 AppImage")?
    } else {
        bytes.to_vec()
    };
    std::fs::write(&staged, &image).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(permissions.mode()))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&staged, &target).map_err(|e| e.to_string())?;
    Ok(())
}

/// Starts the application again and ends this process's role; the caller then quits.
pub fn relaunch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = if cfg!(all(unix, not(target_os = "macos"))) {
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .unwrap_or(exe)
    } else {
        exe
    };
    std::process::Command::new(target)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
