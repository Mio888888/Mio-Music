use crate::music_sdk::client::{self, MusicItem};

pub fn get_http() -> &'static reqwest::Client {
    client::get_client()
}

pub fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

pub fn get_u64(args: &serde_json::Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub fn mg_headers() -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static("iOS@17.5.1(iPhone16,2)"));
    headers.insert("Referer", HeaderValue::from_static("https://m.music.migu.cn/"));
    headers
}

pub fn create_signature(time: &str, str: &str) -> (String, String) {
    use md5::{Digest, Md5};
    let device_id = "963B7AA0D21511ED807EE5846EC87D20";
    let signature_md5 = "6cdc72a439cef99a3418d2a78aa28c73";
    let input = format!("{}{}yyapp2d16148780a1dcc7408e06336b98cfd50{}{}", str, signature_md5, device_id, time);
    let digest = Md5::digest(input.as_bytes());
    (hex::encode(digest), device_id.to_string())
}

pub fn format_play_time(seconds: i64) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{:02}:{:02}", m, s)
}

pub fn format_file_size(size: &str) -> String {
    let size: f64 = size.parse().unwrap_or(0.0);
    if size <= 0.0 { return String::new(); }
    if size < 1024.0 { return format!("{} B", size); }
    if size < 1048576.0 { return format!("{:.2} KB", size / 1024.0); }
    if size < 1073741824.0 { return format!("{:.2} MB", size / 1048576.0); }
    format!("{:.2} GB", size / 1073741824.0)
}

pub fn format_singer_from_list(singers: &[serde_json::Value], key: &str) -> String {
    singers.iter()
        .filter_map(|a| a.get(key).and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("、")
}

pub fn get_song_img(item: &serde_json::Value) -> String {
    let img = item.get("img3").and_then(|v| v.as_str())
        .or_else(|| item.get("img2").and_then(|v| v.as_str()))
        .or_else(|| item.get("img1").and_then(|v| v.as_str()))
        .unwrap_or("");
    if img.is_empty() { return String::new(); }
    if img.starts_with("http") { return img.to_string(); }
    format!("https://d.musicapp.migu.cn{}", img)
}

pub fn parse_quality_types_from_info(item: &serde_json::Value) -> Vec<serde_json::Value> {
    let formats = item.get("newRateFormats")
        .or_else(|| item.get("audioFormats"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut types = Vec::new();
    for fmt in &formats {
        let ftype = fmt.get("formatType").and_then(|v| v.as_str()).unwrap_or("");
        let size_raw = fmt.get("size")
            .or_else(|| fmt.get("androidSize"))
            .or_else(|| fmt.get("asize"))
            .or_else(|| fmt.get("isize"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let size = format_file_size(size_raw);
        let quality = match ftype {
            "PQ" => "128k",
            "HQ" => "320k",
            "SQ" => "flac",
            "ZQ" | "ZQ24" => "hires",
            _ => continue,
        };
        types.push(serde_json::json!({ "type": quality, "size": size }));
    }
    types
}

pub fn mg_parse_music_item(item: &serde_json::Value) -> Option<MusicItem> {
    let id = item.get("id")?.as_i64()?;
    let title = item.get("title")?.as_str()?.to_string();
    let singer = item.get("artists")
        .and_then(|a| a.as_array())
        .map(|arr| format_singer_from_list(arr, "name"))
        .unwrap_or_default();
    let album_name = item.get("albumName").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let album_id = item.get("albumId").cloned().unwrap_or(serde_json::Value::Null);
    let cover = item.get("cover")
        .and_then(|c| c.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace("{size}", "400");
    let duration = item.get("duration")?.as_i64()?;
    let interval = format_play_time(duration / 1000);

    let resource = item.get("resource").and_then(|r| r.as_object());
    let mut types = Vec::new();
    if let Some(res) = resource {
        if res.contains_key("320k") { types.push("320k".to_string()); }
        if res.contains_key("flac") { types.push("flac".to_string()); }
        if res.contains_key("128k") { types.push("128k".to_string()); }
    }
    if types.is_empty() { types.push("128k".to_string()); }

    Some(MusicItem {
        songmid: serde_json::json!(id),
        singer, name: title, album_name, album_id,
        source: "mg".into(), interval,
        img: cover, lrc: None,
        types: Some(types), types_map: None, type_url: Some(serde_json::json!({})),
        hash: None, song_id: None, str_media_mid: None, album_mid: None,
    })
}
