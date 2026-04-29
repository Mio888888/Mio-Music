use std::collections::HashMap;
use std::sync::Mutex;
use base64::Engine;

static COVER_CACHE: once_cell::sync::Lazy<Mutex<HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

const MAX_CACHE_SIZE: usize = 200;

pub fn get_cover_base64(songmid: &str, path: &str) -> Option<String> {
    // Check memory cache first
    {
        let cache = COVER_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(songmid) {
            return Some(cached.clone());
        }
    }

    // Read from file
    let path = std::path::Path::new(path);
    let data = crate::local_music::scanner::get_cover_data(path)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

    // Detect mime type from extension
    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        _ => "image/jpeg",
    };
    let data_uri = format!("data:{};base64,{}", mime, b64);

    // Store in cache
    {
        let mut cache = COVER_CACHE.lock().unwrap();
        if cache.len() >= MAX_CACHE_SIZE {
            // Simple eviction: remove first entry
            if let Some(key) = cache.keys().next().cloned() {
                cache.remove(&key);
            }
        }
        cache.insert(songmid.to_string(), data_uri.clone());
    }

    Some(data_uri)
}

pub fn batch_get_covers(songmids: &[String], tracks: &[(String, String)]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (songmid, path) in tracks {
        if songmids.contains(&songmid.clone()) {
            if let Some(cover) = get_cover_base64(songmid, path) {
                result.insert(songmid.clone(), cover);
            }
        }
    }
    result
}

pub fn clear_cache() {
    COVER_CACHE.lock().unwrap().clear();
}
