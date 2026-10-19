//! The play-queue export format of the original (`utils/playlist/playlistExportImport.ts`): the
//! queue as a JSON array of songs, encrypted with CryptoJS's passphrase AES (OpenSSL
//! `Salted__` container, MD5 key derivation, AES-256-CBC), as base64 text. A `.cpl` file holds
//! that text; a `.cmpl` file holds it gzipped. The clipboard form is the same text.
use aes::Aes256;
use base64::Engine;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use md5::{Digest, Md5};
use std::io::{Read, Write};

const SECRET: &[u8] = b"CeruMusic-PlaylistSecretKey";
const MAGIC: &[u8; 8] = b"Salted__";
const MAX_TEXT: usize = 64 * 1024 * 1024;

/// OpenSSL's `EVP_BytesToKey` with MD5 and one round: 32 key bytes and 16 IV bytes.
fn derive(salt: &[u8]) -> ([u8; 32], [u8; 16]) {
    let mut material = Vec::with_capacity(48);
    let mut previous: Vec<u8> = Vec::new();
    while material.len() < 48 {
        let mut hash = Md5::new();
        hash.update(&previous);
        hash.update(SECRET);
        hash.update(salt);
        previous = hash.finalize().to_vec();
        material.extend_from_slice(&previous);
    }
    let mut key = [0u8; 32];
    let mut iv = [0u8; 16];
    key.copy_from_slice(&material[..32]);
    iv.copy_from_slice(&material[32..48]);
    (key, iv)
}

/// `encryptPlaylist`: the songs as base64 text.
pub fn encrypt(songs: &[serde_json::Value]) -> Result<String, String> {
    let json = serde_json::to_vec(songs).map_err(|e| e.to_string())?;
    let mut salt = [0u8; 8];
    // The salt only has to differ between files; the uuid generator is random.
    salt.copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..8]);
    let (key, iv) = derive(&salt);
    let encrypted = cbc::Encryptor::<Aes256>::new(&key.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(&json);
    let mut container = Vec::with_capacity(16 + encrypted.len());
    container.extend_from_slice(MAGIC);
    container.extend_from_slice(&salt);
    container.extend_from_slice(&encrypted);
    Ok(base64::engine::general_purpose::STANDARD.encode(container))
}

/// `decryptPlaylist`: base64 text back to the songs array.
pub fn decrypt(text: &str) -> Result<Vec<serde_json::Value>, String> {
    let compact: String = text.split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|_| "无法解密播放列表".to_owned())?;
    if bytes.len() < 32 || &bytes[..8] != MAGIC {
        return Err("无法解密播放列表".into());
    }
    let (key, iv) = derive(&bytes[8..16]);
    let plain = cbc::Decryptor::<Aes256>::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&bytes[16..])
        .map_err(|_| "无法解密播放列表".to_owned())?;
    let value: serde_json::Value =
        serde_json::from_slice(&plain).map_err(|_| "无法解密播放列表".to_owned())?;
    match value {
        serde_json::Value::Array(list) => Ok(list),
        _ => Err("无法解密播放列表".into()),
    }
}

/// `.cmpl`: the encrypted text, gzipped.
pub fn gzip(text: &str) -> Result<Vec<u8>, String> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

pub fn gunzip(bytes: &[u8]) -> Result<String, String> {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_TEXT as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|_| "无法读取播放列表文件".to_owned())?;
    if text.len() > MAX_TEXT {
        return Err("播放列表文件过大".into());
    }
    Ok(text)
}

/// The songs of a `.cmpl` or `.cpl` file's bytes; the kind comes from the extension.
pub fn read_file(bytes: &[u8], extension: &str) -> Result<Vec<serde_json::Value>, String> {
    match extension.to_ascii_lowercase().as_str() {
        "cpl" => decrypt(&String::from_utf8_lossy(bytes)),
        "cmpl" => decrypt(&gunzip(bytes)?),
        _ => Err("不支持的文件类型".into()),
    }
}

/// `cerumusic-playlist-YYYY-MM-DD.cmpl`, dated in UTC like `toISOString()`.
pub fn default_file_name() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("cerumusic-playlist-{year:04}-{month:02}-{day:02}.cmpl")
}
