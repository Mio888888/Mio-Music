use crate::music_sdk::client::{self, MusicItem, PlaylistItem, PlaylistResult, SearchResult};

fn get_http() -> &'static reqwest::Client {
    client::get_client()
}

fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn get_u64(args: &serde_json::Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Parse Kugou count values which may be numbers or pre-formatted strings like "1681.7万", "2.3亿"
fn parse_kugou_count(v: &serde_json::Value) -> serde_json::Value {
    if let Some(n) = v.as_f64() {
        return serde_json::json!(n as i64);
    }
    if let Some(s) = v.as_str() {
        let s = s.trim();
        if s.ends_with('亿') {
            if let Ok(n) = s.trim_end_matches('亿').parse::<f64>() {
                return serde_json::json!((n * 100_000_000.0) as i64);
            }
        } else if s.ends_with('万') {
            if let Ok(n) = s.trim_end_matches('万').parse::<f64>() {
                return serde_json::json!((n * 10_000.0) as i64);
            }
        }
        if let Ok(n) = s.parse::<f64>() {
            return serde_json::json!(n as i64);
        }
    }
    serde_json::Value::Null
}

// --- Playlist Tags (no signing needed) ---

async fn get_playlist_tags(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "http://www2.kugou.kugou.com/yueku/v9/special/getSpecial?is_smarty=1";
    let resp: serde_json::Value = get_http().get(url)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let status = resp.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status != 1 {
        return Err("KuGou tags API error".into());
    }

    let data = resp.get("data").cloned().unwrap_or(serde_json::json!({}));

    // Hot tags — actual data is nested inside hotTag.data
    let hot_tag_raw = data.get("hotTag").and_then(|v| v.get("data")).cloned().unwrap_or(serde_json::json!({}));
    let hot_tag: Vec<serde_json::Value> = if let Some(obj) = hot_tag_raw.as_object() {
        obj.values().filter_map(|tag| {
            let id = tag.get("special_id")?.as_str()?;
            let name = tag.get("special_name")?.as_str()?;
            Some(serde_json::json!({
                "id": id.to_string(),
                "name": name,
                "source": "kg"
            }))
        }).collect()
    } else {
        vec![]
    };

    // All tags (grouped by category)
    let tagids_raw = data.get("tagids").cloned().unwrap_or(serde_json::json!({}));
    let tags: Vec<serde_json::Value> = if let Some(obj) = tagids_raw.as_object() {
        obj.iter().map(|(name, category_data)| {
            let tag_list: Vec<serde_json::Value> = category_data
                .get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default()
                .iter().filter_map(|tag| {
                    let id = tag.get("id")?.as_i64()?;
                    let tag_name = tag.get("name")?.as_str()?;
                    Some(serde_json::json!({
                        "id": id.to_string(),
                        "name": tag_name,
                        "source": "kg"
                    }))
                }).collect();
            serde_json::json!({
                "name": name,
                "list": tag_list,
                "source": "kg"
            })
        }).collect()
    } else {
        vec![]
    };

    Ok(serde_json::json!({ "tags": tags, "hotTag": hot_tag, "source": "kg" }))
}

// --- Category Playlists (no signing needed) ---

async fn get_category_playlists(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tag_id = get_str(&args, "tagId").to_string();
    let sort_id = get_str(&args, "sortId");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let sort = if sort_id.is_empty() { "5" } else { sort_id }; // default: 推荐 (sort=5)
    let url = format!(
        "http://www2.kugou.kugou.com/yueku/v9/special/getSpecial?is_ajax=1&cdn=cdn&t={}&c={}&p={}",
        sort, tag_id, page
    );

    let resp: serde_json::Value = get_http().get(&url)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let status = resp.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status != 1 {
        return Err("KuGou playlist list API error".into());
    }

    let raw_list = resp.get("special_db").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<PlaylistItem> = raw_list.iter().map(|item| {
        let special_id = item.get("specialid").cloned().unwrap_or(serde_json::Value::Null);
        // total_play_count may be a pre-formatted string like "1681.7万" — convert to number
        let play_count_val = item.get("total_play_count").or(item.get("play_count"))
            .map(|v| parse_kugou_count(v)).unwrap_or(serde_json::Value::Null);
        PlaylistItem {
            id: serde_json::json!(format!("id_{}", special_id.as_i64().unwrap_or(0))),
            name: item.get("specialname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            img: item.get("img").or(item.get("imgurl")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source: "kg".into(),
            desc: item.get("intro").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            play_count: play_count_val,
            author: item.get("nickname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            total: item.get("song_count").cloned().unwrap_or(serde_json::Value::Null),
        }
    }).collect();

    // Get total from listInfo or calculate from response
    let total = resp.get("data").and_then(|d| d.get("params")).and_then(|p| p.get("total"))
        .and_then(|v| v.as_i64()).unwrap_or(list.len() as i64);

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "kg".into(),
    }).unwrap())
}

// --- Leaderboards (static list + API fetch) ---

async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "http://mobilecdnbj.kugou.com/api/v5/rank/list?version=9108&plat=0&showtype=2&parentid=0&apiver=6&area_code=1&withsong=1";
    let resp: serde_json::Value = get_http().get(url)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let errcode = resp.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if errcode != 0 {
        return Err("KuGou rank list API error".into());
    }

    let raw_list = resp.get("data").and_then(|d| d.get("info")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = raw_list.iter().filter_map(|board| {
        // Only include boards that are "vol" (isvol=1) like the reference
        let isvol = board.get("isvol").and_then(|v| v.as_i64()).unwrap_or(0);
        if isvol != 1 { return None; }

        let rankid = board.get("rankid")?.as_i64()?;
        let name = board.get("rankname")?.as_str()?;
        let imgurl = board.get("imgurl").and_then(|v| v.as_str()).unwrap_or("")
            .replace("{size}", "512");
        let play_times = board.get("play_times").and_then(|v| v.as_i64()).unwrap_or(0);
        let update_frequency = board.get("update_frequency").and_then(|v| v.as_str()).unwrap_or("");

        Some(serde_json::json!({
            "id": format!("kg__{}", rankid),
            "name": name,
            "bangid": rankid.to_string(),
            "img": imgurl,
                "pic": imgurl,
            "listen": play_times,
            "update_frequency": update_frequency,
            "source": "kg"
        }))
    }).collect();

    Ok(serde_json::json!({ "list": list, "source": "kg" }))
}

// --- Leaderboard Detail (songs in a board) ---

async fn get_leaderboard_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 100);

    // Extract numeric rank ID from "kg__XXXX" format
    let rank_id = if raw_id.starts_with("kg__") {
        raw_id.replace("kg__", "")
    } else {
        raw_id.clone()
    };

    let url = format!(
        "http://mobilecdnbj.kugou.com/api/v3/rank/song?version=9108&ranktype=1&plat=0&pagesize={}&area_code=1&page={}&rankid={}&with_res_tag=0&show_portrait_mv=1",
        limit, page, rank_id
    );

    let resp: serde_json::Value = get_http().get(&url)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let errcode = resp.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if errcode != 0 {
        return Err("KuGou rank song API error".into());
    }

    let data = resp.get("data").cloned().unwrap_or(serde_json::json!({}));
    let total = data.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = data.get("info").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Extract hashes and fetch song details via gateway API
    let hashes: Vec<String> = raw_list.iter().filter_map(|item| {
        item.get("hash").and_then(|v| v.as_str()).map(|s| s.to_string())
    }).collect();

    let list = fetch_kg_song_details(&hashes).await.unwrap_or_default();

    Ok(serde_json::json!({
        "list": list, "info": serde_json::json!({}),
        "allPage": (total as f64 / limit as f64).ceil() as i64, "limit": limit as i64, "total": total, "source": "kg"
    }))
}

// --- Fetch song details via KG gateway API (batch by hash) ---

async fn fetch_kg_song_details(hashes: &[String]) -> Result<Vec<MusicItem>, String> {
    if hashes.is_empty() {
        return Ok(vec![]);
    }

    // KG gateway API: batch lookup by hash
    let data_hashes: Vec<serde_json::Value> = hashes.iter().map(|h| serde_json::json!({ "hash": h })).collect();
    let body = serde_json::json!({
        "appid": 1005,
        "clientver": 11451,
        "mid": "1",
        "dfid": "-",
        "clienttime": chrono::Utc::now().timestamp_millis(),
        "key": "OIlwieks28dk2k092lksi2UIkp",
        "fields": "album_info,author_name,audio_info,ori_audio_name,base,songname",
        "data": data_hashes,
        "show_privilege": 1,
        "show_album_info": "1",
        "is_publish": "",
        "area_code": "1"
    });

    let resp: serde_json::Value = get_http()
        .post("http://gateway.kugou.com/v2/album_audio/audio")
        .header("KG-THash", "13a3164")
        .header("KG-RC", "1")
        .header("KG-Fake", "0")
        .header("KG-RF", "00869891")
        .header("User-Agent", "Android712-AndroidPhone-11451-376-0-FeeCacheUpdate-wifi")
        .header("x-router", "kmr.service.kugou.com")
        .json(&body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let data = resp.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<MusicItem> = data.iter().filter_map(|item| {
        // Each item is an array with one element
        let info = item.as_array()?.first()?;
        let audio_info = info.get("audio_info")?;
        let album_info = info.get("album_info");

        let hash = audio_info.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if hash.is_empty() { return None; }
        let songmid = hash.clone();
        let name = info.get("songname").or_else(|| info.get("ori_audio_name"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let singer = info.get("author_name").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let album_name = album_info.and_then(|a| a.get("album_name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_id = album_info.and_then(|a| a.get("album_id")).cloned().unwrap_or(serde_json::Value::Null);
        let img = album_info.and_then(|a| a.get("imgurl")).or_else(|| audio_info.get("img"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let duration_ms = audio_info.get("timelength").and_then(|v| v.as_i64()).unwrap_or(0);
        let interval = format_play_time(duration_ms / 1000);

        let mut types = Vec::new();
        let filesize_flac = audio_info.get("filesize_flac").and_then(|v| v.as_i64()).unwrap_or(0);
        let filesize_320 = audio_info.get("filesize_320").and_then(|v| v.as_i64()).unwrap_or(0);
        let filesize_128 = audio_info.get("filesize").and_then(|v| v.as_i64()).unwrap_or(0);
        if filesize_flac > 0 { types.push("flac".to_string()); }
        if filesize_320 > 0 { types.push("320k".to_string()); }
        if filesize_128 > 0 { types.push("128k".to_string()); }
        types.reverse();

        Some(MusicItem {
            songmid: serde_json::Value::String(songmid), singer, name, album_name, album_id,
            source: "kg".into(), interval, img, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})),
            hash: Some(hash),
        })
    }).collect();

    Ok(list)
}

// --- Playlist Detail (simplified - uses HTML parsing) ---

async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    // Extract special ID from various formats
    let id = if raw_id.starts_with("id_") {
        raw_id.replace("id_", "")
    } else {
        raw_id
    };

    // Fetch the playlist page and extract data from embedded JSON
    let url = format!("http://www2.kugou.kugou.com/yueku/v9/special/single/{}-5-9999.html", id);
    let resp = get_http().get(&url)
        .send().await.map_err(|e| e.to_string())?;
    let html = resp.text().await.map_err(|e| e.to_string())?;

    // Extract playlist info from HTML
    let name = extract_regex(&html, r#"name: "([^"]+)""#).unwrap_or_default();
    let img = extract_regex(&html, r#"pic: "([^"]+)""#).unwrap_or_default();

    // Extract global.data from HTML
    let data_str = extract_regex(&html, r"global\.data = (\[.+?\]);").unwrap_or_default();
    let raw_data: Vec<serde_json::Value> = serde_json::from_str(&data_str).unwrap_or_default();

    let hashes: Vec<String> = raw_data.iter().filter_map(|item| {
        item.get("hash").and_then(|v| v.as_str()).map(|s| s.to_string())
    }).collect();

    // Paginate
    let total = hashes.len() as i64;
    let start = ((page - 1) * limit) as usize;
    let end = (start + limit as usize).min(hashes.len());
    let page_hashes = if start < hashes.len() { &hashes[start..end] } else { &[] };

    let list = fetch_kg_song_details(page_hashes).await.unwrap_or_default();

    Ok(serde_json::json!({
        "list": list, "info": { "name": name, "img": img, "desc": "" },
        "allPage": (total as f64 / limit as f64).ceil() as i64, "limit": limit as i64, "total": total, "source": "kg"
    }))
}

// --- Search ---

async fn search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 20);
    if keyword.is_empty() {
        return Ok(serde_json::to_value(SearchResult {
            list: vec![], all_page: 0, limit: limit as i64, total: 0, source: "kg".into(),
        }).unwrap());
    }

    let url = format!(
        "http://msearchretry.kugou.com/api/v3/search/special?keyword={}&page={}&pagesize={}&showtype=10&filter=0&version=7910&sver=2",
        urlencoding::encode(keyword), page, limit
    );

    let resp: serde_json::Value = get_http().get(&url)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let errcode = resp.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if errcode != 0 {
        return Err("KuGou search API error".into());
    }

    let data = resp.get("data").cloned().unwrap_or(serde_json::json!({}));
    let total = data.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = data.get("info").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list.iter().map(|item| {
        let special_id = item.get("specialid").cloned().unwrap_or(serde_json::Value::Null);
        PlaylistItem {
            id: serde_json::json!(format!("id_{}", special_id.as_i64().unwrap_or(0))),
            name: item.get("specialname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            img: item.get("imgurl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source: "kg".into(),
            desc: item.get("intro").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            play_count: item.get("playcount").cloned().unwrap_or(serde_json::Value::Null),
            author: item.get("nickname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            total: serde_json::Value::Null,
        }
    }).collect();

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "kg".into(),
    }).unwrap())
}

// --- Search Playlist (same as search) ---

async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    search(args).await
}

// --- Helpers ---

fn extract_regex(html: &str, pattern: &str) -> Option<String> {
    let re = regex_lite::Regex::new(pattern).ok()?;
    let caps = re.captures(html)?;
    Some(caps.get(1)?.as_str().to_string())
}

fn format_play_time(seconds: i64) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{:02}:{:02}", m, s)
}

// --- Router ---

pub async fn handle(method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    match method {
        "search" => search(args).await,
        "tipSearch" | "hotSearch" => Ok(serde_json::json!({ "list": [] })),
        "getMusicUrl" => Ok(serde_json::json!({ "url": "" })),
        "getPic" => Ok(serde_json::json!({ "url": "" })),
        "getLyric" => Ok(serde_json::json!({ "lrc": "" })),
        "getComment" | "getHotComment" => Ok(serde_json::json!({ "comments": [], "total": 0 })),
        "getHotSonglist" | "getHotPlaylists" => get_category_playlists(args).await,
        "getPlaylistTags" | "getSongboardTags" => get_playlist_tags(args).await,
        "getCategoryPlaylists" => get_category_playlists(args).await,
        "getLeaderboards" => get_leaderboards(args).await,
        "getPlaylistDetail" | "getPlaylistDetailById" => get_playlist_detail(args).await,
        "getLeaderboardDetail" => get_leaderboard_detail(args).await,
        "searchPlaylist" => search_playlist(args).await,
        _ => Err(format!("Unknown SDK method for kg: {}", method)),
    }
}
