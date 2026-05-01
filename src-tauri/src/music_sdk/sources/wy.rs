use crate::music_sdk::client::{self, MusicItem, PlaylistItem, PlaylistResult, SearchResult};
use aes::Aes128;
use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyInit};

fn aes_ecb_encrypt(data: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let cipher = Aes128::new(key.into());
    let mut buf = vec![0u8; data.len() + 16];
    buf[..data.len()].copy_from_slice(data);
    cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, data.len()).unwrap().to_vec()
}

// NetEase linuxapi encryption: AES-128-ECB
fn linuxapi_encrypt(payload: &str) -> (String,) {
    let linux_key = *b"rFgB&h#%2?^eDg:Q";
    let encrypted = aes_ecb_encrypt(payload.as_bytes(), &linux_key);
    let eparams = hex::encode(&encrypted).to_uppercase();
    (eparams,)
}

// NetEase eapi encryption: AES-128-ECB with MD5 digest
fn eapi_encrypt(url: &str, data: &str) -> String {
    use md5::{Digest, Md5};
    let eapi_key = *b"e82ckenh8dichen8";
    let message = format!("nobody{}use{}md5forencrypt", url, data);
    let digest = Md5::digest(message.as_bytes());
    let digest_hex = hex::encode(digest);
    let plaintext = format!("{}-36cd479b6b5-{}-36cd479b6b5-{}", url, data, digest_hex);

    let encrypted = aes_ecb_encrypt(plaintext.as_bytes(), &eapi_key);
    hex::encode(&encrypted).to_uppercase()
}

fn get_http() -> &'static reqwest::Client {
    client::get_client()
}

fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn get_u64(args: &serde_json::Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

fn get_wy_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/60.0.3112.90 Safari/537.36"),
        ("Referer", "https://music.163.com/"),
        ("Origin", "https://music.163.com"),
        ("Content-Type", "application/x-www-form-urlencoded"),
    ]
}

// --- Playlist Tags (linuxapi) ---

async fn get_playlist_tags(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let (tags_result, hot_result) = tokio::join!(
        async {
            let inner = serde_json::json!({
                "method": "POST",
                "url": "https://music.163.com/api/playlist/catalogue",
                "params": {}
            });
            let (eparams,) = linuxapi_encrypt(&serde_json::to_string(&inner).unwrap());
            let resp: serde_json::Value = get_http()
                .post("https://music.163.com/api/linux/forward")
                .headers(reqwest::header::HeaderMap::from_iter(
                    get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
                ))
                .body(format!("eparams={}", eparams))
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;

            let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            if code != 200 {
                return Err(format!("NetEase tag API error: code={}", code));
            }

            let sub = resp.get("sub").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let categories_raw = resp.get("categories").cloned().unwrap_or(serde_json::json!({}));

            let mut cat_tags: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
            for item in &sub {
                let cat_idx = item.get("category").and_then(|v| v.as_i64()).unwrap_or(0).to_string();
                let tag_name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                cat_tags.entry(cat_idx).or_default().push(serde_json::json!({
                    "id": tag_name,
                    "name": tag_name,
                    "source": "wy"
                }));
            }

            let list: Vec<serde_json::Value> = categories_raw.as_object().map(|cats| {
                cats.iter().map(|(key, val)| {
                    let cat_name = val.as_str().unwrap_or("");
                    serde_json::json!({
                        "name": cat_name,
                        "list": cat_tags.get(key).cloned().unwrap_or_default(),
                        "source": "wy"
                    })
                }).collect::<Vec<_>>()
            }).unwrap_or_default();

            Ok::<_, String>(list)
        },
        async {
            let inner = serde_json::json!({
                "method": "POST",
                "url": "https://music.163.com/api/playlist/hottags",
                "params": {}
            });
            let (eparams,) = linuxapi_encrypt(&serde_json::to_string(&inner).unwrap());
            let resp: serde_json::Value = get_http()
                .post("https://music.163.com/api/linux/forward")
                .headers(reqwest::header::HeaderMap::from_iter(
                    get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
                ))
                .body(format!("eparams={}", eparams))
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;

            let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            if code != 200 {
                return Err(format!("NetEase hot tag API error: code={}", code));
            }

            let tags = resp.get("tags").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let hot: Vec<serde_json::Value> = tags.iter().filter_map(|item| {
                item.get("playlistTag").and_then(|pt| {
                    let name = pt.get("name")?.as_str()?.to_string();
                    Some(serde_json::json!({ "id": name.clone(), "name": name, "source": "wy" }))
                })
            }).collect();
            Ok::<_, String>(hot)
        }
    );

    let tags = tags_result.unwrap_or_default();
    let hot_tag = hot_result.unwrap_or_default();
    Ok(serde_json::json!({ "tags": tags, "hotTag": hot_tag, "source": "wy" }))
}

// --- Category Playlists (weapi) ---

fn wy_filter_playlist(raw: &[serde_json::Value]) -> Vec<PlaylistItem> {
    raw.iter().map(|item| PlaylistItem {
        id: item.get("id").cloned().unwrap_or(serde_json::Value::Null),
        name: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        img: item.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        source: "wy".into(),
        desc: item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        play_count: item.get("playCount").cloned().unwrap_or(serde_json::Value::Null),
        author: item.get("creator").and_then(|c| c.get("nickname")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
        total: serde_json::Value::Null,
    }).collect()
}

async fn get_category_playlists(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tag_id = get_str(&args, "tagId").to_string();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let cat = if tag_id.is_empty() { "全部".to_string() } else { tag_id };
    let body = format!(
        "cat={}&order=hot&limit={}&offset={}&total=true",
        urlencoding::encode(&cat),
        limit,
        limit * (page - 1)
    );

    let resp: serde_json::Value = get_http()
        .post("https://music.163.com/api/playlist/list")
        .headers(reqwest::header::HeaderMap::from_iter(
            get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
        ))
        .body(body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        return Err(format!("NetEase playlist list API error: code={}", code));
    }

    let total = resp.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp.get("playlists").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list = wy_filter_playlist(&raw_list);

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "wy".into(),
    }).unwrap())
}

// --- Leaderboards (linuxapi) ---

async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let inner = serde_json::json!({
        "method": "POST",
        "url": "https://music.163.com/api/toplist",
        "params": {}
    });
    let (eparams,) = linuxapi_encrypt(&serde_json::to_string(&inner).unwrap());

    let resp: serde_json::Value = get_http()
        .post("https://music.163.com/api/linux/forward")
        .headers(reqwest::header::HeaderMap::from_iter(
            get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
        ))
        .body(format!("eparams={}", eparams))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        return Err(format!("NetEase toplist API error: code={}", code));
    }

    let raw_list = resp.get("list").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = raw_list.iter().map(|item| {
        serde_json::json!({
            "id": format!("wy__{}", item.get("id").and_then(|v| v.as_i64()).unwrap_or(0)),
            "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "img": item.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or(""),
            "pic": item.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or(""),
            "listen": item.get("playCount").cloned().unwrap_or(serde_json::Value::Null),
            "update_frequency": item.get("updateFrequency").and_then(|v| v.as_str()).unwrap_or(""),
            "source": "wy"
        })
    }).collect();

    Ok(serde_json::json!({ "list": list, "source": "wy" }))
}

// --- Playlist/Leaderboard Detail (linuxapi for detail, weapi for fallback) ---

async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit: i64 = 1000;
    let start = (limit * (page as i64 - 1)) as usize;

    // Parse ID from URL if needed
    let id = parse_wy_id(&raw_id);

    // Use linuxapi for playlist detail
    let inner_payload = serde_json::json!({
        "method": "POST",
        "url": "https://music.163.com/api/v3/playlist/detail",
        "params": { "id": id, "n": 100000, "s": 8 }
    });
    let inner_str = serde_json::to_string(&inner_payload).unwrap_or_else(|_| "{}".to_string());
    let (eparams,) = linuxapi_encrypt(&inner_str);
    let body = format!("eparams={}", eparams);

    let resp: serde_json::Value = get_http()
        .post("https://music.163.com/api/linux/forward")
        .headers(reqwest::header::HeaderMap::from_iter(
            get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
        ))
        .body(body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        return Err(format!("NetEase playlist detail API error: code={}", code));
    }

    let playlist = resp.get("playlist").cloned().unwrap_or(serde_json::json!({}));
    let track_ids: Vec<serde_json::Value> = playlist.get("trackIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let total = track_ids.len() as i64;
    let privileges = resp.get("privileges").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let tracks = playlist.get("tracks").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<MusicItem> = if tracks.len() == privileges.len() {
        parse_wy_tracks(&tracks, &privileges)
    } else if !track_ids.is_empty() {
        // Fetch song details via eapi
        let song_ids: Vec<i64> = track_ids.iter().skip(start).take(limit as usize)
            .filter_map(|tid| tid.get("id").and_then(|v| v.as_i64()))
            .collect();
        fetch_wy_song_details(&song_ids).await.unwrap_or_default()
    } else {
        vec![]
    };

    Ok(serde_json::json!({
        "list": list, "info": {
            "play_count": playlist.get("playCount").cloned().unwrap_or(serde_json::Value::Null),
            "name": playlist.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "img": playlist.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or(""),
            "pic": playlist.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or(""),
            "desc": playlist.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            "author": playlist.get("creator").and_then(|c| c.get("nickname")).and_then(|v| v.as_str()).unwrap_or("")
        },
        "allPage": (total as f64 / limit as f64).ceil() as i64, "limit": limit, "total": total, "source": "wy"
    }))
}

async fn get_leaderboard_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    get_playlist_detail(args).await
}

// --- Parse tracks from playlist detail response ---
fn parse_wy_tracks(tracks: &[serde_json::Value], privileges: &[serde_json::Value]) -> Vec<MusicItem> {
    tracks.iter().enumerate().filter_map(|(idx, item)| {
        let privilege = privileges.get(idx)
            .or_else(|| privileges.iter().find(|p| p.get("id") == item.get("id")))
            .cloned().unwrap_or(serde_json::json!({}));

        let id = item.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let singers = item.get("ar").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let singer = singers.iter().filter_map(|s| s.get("name")).filter_map(|n| n.as_str()).collect::<Vec<_>>().join("、");
        let album_name = item.get("al").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_id = item.get("al").and_then(|a| a.get("id")).cloned().unwrap_or(serde_json::Value::Null);
        let img = item.get("al").and_then(|a| a.get("picUrl")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let dt = item.get("dt").and_then(|v| v.as_i64()).unwrap_or(0);
        let interval = format_play_time(dt / 1000);

        let mut types = Vec::new();
        let maxbr = privilege.get("maxbr").and_then(|v| v.as_i64()).unwrap_or(128000);
        if privilege.get("maxBrLevel").and_then(|v| v.as_str()) == Some("hires") {
            types.push("hires".to_string());
        }
        if maxbr >= 999000 {
            types.push("flac".to_string());
        }
        if maxbr >= 320000 {
            types.push("320k".to_string());
        }
        if maxbr >= 128000 {
            types.push("128k".to_string());
        }
        types.dedup();
        types.reverse();

        Some(MusicItem {
            songmid: id, singer, name, album_name, album_id,
            source: "wy".into(), interval, img, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})), hash: None,
        })
    }).collect()
}

// --- Fetch song details via eapi (used when tracks not included in playlist detail) ---
async fn fetch_wy_song_details(song_ids: &[i64]) -> Result<Vec<MusicItem>, String> {
    if song_ids.is_empty() {
        return Ok(vec![]);
    }

    let _ids_str = song_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let data = serde_json::json!({ "ids": song_ids });
    let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let eparams = eapi_encrypt("/api/v3/song/detail", &data_str);
    let body = format!("eparams={}", eparams);

    let resp: serde_json::Value = get_http()
        .post("http://interface.music.163.com/eapi/batch")
        .headers(reqwest::header::HeaderMap::from_iter(
            get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
        ))
        .body(body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let songs = resp.get("songs").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let privileges = resp.get("privileges").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    Ok(parse_wy_tracks(&songs, &privileges))
}

// --- Search (eapi) ---

async fn search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 20);
    if keyword.is_empty() {
        return Ok(serde_json::to_value(SearchResult {
            list: vec![], all_page: 0, limit: limit as i64, total: 0, source: "wy".into(),
        }).unwrap());
    }

    let data = serde_json::json!({
        "s": keyword,
        "type": 1000, // 1000 = playlist
        "limit": limit,
        "total": page == 1,
        "offset": limit * (page - 1)
    });
    let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    let eparams = eapi_encrypt("/api/cloudsearch/pc", &data_str);
    let body = format!("eparams={}", eparams);

    let resp: serde_json::Value = get_http()
        .post("http://interface.music.163.com/eapi/batch")
        .headers(reqwest::header::HeaderMap::from_iter(
            get_wy_headers().into_iter().map(|(k, v)| (k.parse().unwrap(), v.parse().unwrap())),
        ))
        .body(body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        return Err(format!("NetEase search API error: code={}", code));
    }

    let result = resp.get("result").cloned().unwrap_or(serde_json::json!({}));
    let total = result.get("playlistCount").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = result.get("playlists").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list = wy_filter_playlist(&raw_list);

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "wy".into(),
    }).unwrap())
}

// --- Search Playlist (reuses search with type 1000) ---

async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    search(args).await
}

// --- Helpers ---

fn parse_wy_id(raw_id: &str) -> String {
    if raw_id.contains("wy__") {
        return raw_id.replace("wy__", "");
    }
    if let Some(pos) = raw_id.rfind('/') {
        return raw_id[pos + 1..].to_string();
    }
    raw_id.to_string()
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
        _ => Err(format!("Unknown SDK method for wy: {}", method)),
    }
}
