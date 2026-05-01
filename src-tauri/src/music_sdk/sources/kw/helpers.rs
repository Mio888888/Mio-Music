use crate::music_sdk::client::{self, MusicItem, SearchResult};

pub fn get_http() -> &'static reqwest::Client {
    client::get_client()
}

pub fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

pub fn get_u64(args: &serde_json::Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub fn get_songmid(args: &serde_json::Value) -> String {
    let info = args.get("songInfo").unwrap_or(&serde_json::Value::Null);
    info.get("songmid")
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

pub fn empty_search(source: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::to_value(SearchResult {
        list: vec![],
        all_page: 0,
        limit: 30,
        total: 0,
        source: source.into(),
    })
    .unwrap())
}

pub fn decode_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn format_singer(raw: String) -> String {
    raw.replace("&", "、")
}

pub fn format_play_time(seconds: i64) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{:02}:{:02}", m, s)
}

pub fn parse_quality_types(minfo: &str) -> Vec<String> {
    let mut types = Vec::new();
    for part in minfo.split(';') {
        let fields: Vec<&str> = part.split(',').collect();
        if fields.len() < 3 {
            continue;
        }
        let bitrate = fields.get(1).unwrap_or(&"").parse::<u32>().unwrap_or(0);
        let quality = match bitrate {
            20900 => Some("master"),
            4000 => Some("hires"),
            2000 => Some("flac"),
            320 => Some("320k"),
            128 => Some("128k"),
            _ => None,
        };
        if let Some(q) = quality {
            types.push(q.to_string());
        }
    }
    types.sort_by_key(|a| std::cmp::Reverse(quality_rank(a)));
    types
}

pub fn quality_rank(q: &str) -> u8 {
    match q {
        "master" => 5,
        "hires" => 4,
        "flac" => 3,
        "320k" => 2,
        "128k" => 1,
        _ => 0,
    }
}

pub fn parse_music_item(info: &serde_json::Value) -> Option<MusicItem> {
    let rid = info
        .get("MUSICRID")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            info.get("rid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            info.get("rid")
                .and_then(|v| v.as_i64())
                .map(|n| n.to_string())
        })
        .or_else(|| {
            info.get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            info.get("id")
                .and_then(|v| v.as_i64())
                .map(|n| n.to_string())
        })?;
    let songmid = rid.replace("MUSIC_", "");
    let name = decode_html(
        info.get("SONGNAME")
            .or_else(|| info.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let singer = format_singer(decode_html(
        info.get("ARTIST")
            .or_else(|| info.get("artist"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    ));
    let album = decode_html(
        info.get("ALBUM")
            .or_else(|| info.get("album"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let album_id = info
        .get("ALBUMID")
        .or_else(|| info.get("albumid"))
        .cloned()
        .unwrap_or(serde_json::Value::String(String::new()));
    let duration: i64 = info
        .get("DURATION")
        .or_else(|| info.get("duration"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let interval = format_play_time(duration);

    let minfo = info
        .get("N_MINFO")
        .or_else(|| info.get("formats"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let types = parse_quality_types(minfo);
    let img = info
        .get("albumpic")
        .or_else(|| info.get("albpic"))
        .or_else(|| info.get("pic"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Some(MusicItem {
        songmid: serde_json::Value::String(songmid),
        singer,
        name,
        album_name: album,
        album_id,
        source: "kw".into(),
        interval,
        img,
        lrc: None,
        types: Some(types),
        types_map: None,
        type_url: Some(serde_json::json!({})),
        hash: None,
        song_id: None,
        str_media_mid: None,
        album_mid: None,
    })
}
