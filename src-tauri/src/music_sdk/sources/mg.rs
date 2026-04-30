use crate::music_sdk::client::{self, MusicItem, PlaylistItem, PlaylistResult, SearchResult};
use reqwest::header::{HeaderMap, HeaderValue};

fn get_http() -> &'static reqwest::Client {
    client::get_client()
}

fn get_str<'a>(args: &'a serde_json::Value, key: &str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn get_u64(args: &serde_json::Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

fn mg_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", HeaderValue::from_static("iOS@17.5.1(iPhone16,2)"));
    headers.insert("Referer", HeaderValue::from_static("https://m.music.migu.cn/"));
    headers
}

// --- Playlist Tags ---

async fn get_playlist_tags(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "https://app.c.nf.migu.cn/pc/v1.0/template/musiclistplaza-taglist/release";
    let resp: serde_json::Value = get_http().get(url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let status = resp.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if status != "000000" {
        return Err("MG tags API error".into());
    }

    let data = resp.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // First element: hot tags
    let hot_tag: Vec<serde_json::Value> = data.first()
        .and_then(|item| item.get("content"))
        .and_then(|c| c.as_array()).cloned().unwrap_or_default()
        .iter().filter_map(|tag| {
            let texts = tag.get("texts").and_then(|t| t.as_array())?;
            let name = texts.get(0)?.as_str()?;
            let id = texts.get(1)?.as_str()?;
            Some(serde_json::json!({
                "id": id.to_string(),
                "name": name.to_string(),
                "source": "mg"
            }))
        }).collect();

    // Remaining elements: category groups
    let tags: Vec<serde_json::Value> = data.iter().skip(1).filter_map(|item| {
        let title = item.get("header")?.get("title")?.as_str()?;
        let content = item.get("content")?.as_array()?;
        let tag_list: Vec<serde_json::Value> = content.iter().filter_map(|tag| {
            let texts = tag.get("texts")?.as_array()?;
            let name = texts.get(0)?.as_str()?;
            let id = texts.get(1)?.as_str()?;
            Some(serde_json::json!({
                "id": id.to_string(),
                "name": name.to_string(),
                "source": "mg"
            }))
        }).collect();
        Some(serde_json::json!({
            "name": title.to_string(),
            "list": tag_list,
            "source": "mg"
        }))
    }).collect();

    Ok(serde_json::json!({ "tags": tags, "hotTag": hot_tag, "source": "mg" }))
}

// --- Category Playlists ---

async fn get_category_playlists(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tag_id = get_str(&args, "tagId");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let resp: serde_json::Value = if tag_id.is_empty() {
        // Without tagId: use recommend endpoint
        let url = format!(
            "https://app.c.nf.migu.cn/pc/bmw/page-data/playlist-square-recommend/v1.0?templateVersion={}&pageNo={}",
            page, page
        );
        get_http().get(&url)
            .headers(mg_headers())
            .send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?
    } else {
        // With tagId: use tag-specific endpoint
        let url = format!(
            "https://app.c.nf.migu.cn/pc/v1.0/template/musiclistplaza-listbytag/release?pageNumber={}&templateVersion=2&tagId={}",
            page, tag_id
        );
        get_http().get(&url)
            .headers(mg_headers())
            .send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?
    };

    let code = resp.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if code != "000000" {
        return Err("MG playlist list API error".into());
    }

    let data = resp.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // Extract playlists from the response structure
    let mut list: Vec<PlaylistItem> = Vec::new();
    for item in &data {
        if let Some(content) = item.get("content").and_then(|c| c.as_array()) {
            for pl in content {
                let texts = pl.get("texts").and_then(|t| t.as_array());
                let name = texts.and_then(|t| t.get(0)).and_then(|v| v.as_str()).unwrap_or("");
                let id = texts.and_then(|t| t.get(1)).and_then(|v| v.as_str()).unwrap_or("");
                let img = pl.get("img").and_then(|i| i.get("url")).and_then(|v| v.as_str()).unwrap_or("");

                if id.is_empty() { continue; }
                list.push(PlaylistItem {
                    id: serde_json::json!(id),
                    name: name.to_string(),
                    img: img.to_string(),
                    source: "mg".into(),
                    desc: String::new(),
                    play_count: serde_json::Value::Null,
                    author: String::new(),
                });
            }
        }
    }

    // Estimate total
    let total = list.len() as i64 * page as i64; // rough estimate

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "mg".into(),
    }).unwrap())
}

// --- Leaderboards (static list) ---

const BOARD_LIST: &[(&str, &str, &str, &str)] = &[
    ("24960262", "尖叫新歌榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960262/1200x1200.jpg"),
    ("24960284", "尖叫原创榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960284/1200x1200.jpg"),
    ("24960287", "尖叫热歌榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960287/1200x1200.jpg"),
    ("24960285", "尖叫飙升榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960285/1200x1200.jpg"),
    ("24960259", "内地榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960259/1200x1200.jpg"),
    ("24960260", "港台榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960260/1200x1200.jpg"),
    ("24960261", "欧美榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960261/1200x1200.jpg"),
    ("24960257", "日韩榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960257/1200x1200.jpg"),
    ("24960258", "彩铃榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960258/1200x1200.jpg"),
    ("24960255", "KTV榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960255/1200x1200.jpg"),
    ("24960256", "网络榜", "7.1 更新", "https://cdnmusic.migu.cn/v3/music/songlist/24960256/1200x1200.jpg"),
];

async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let list: Vec<serde_json::Value> = BOARD_LIST.iter().map(|(id, name, freq, img)| {
        serde_json::json!({
            "id": format!("mg__{}", id),
            "name": name,
            "bangid": id,
            "img": img,
            "listen": 0,
            "update_frequency": freq,
            "source": "mg"
        })
    }).collect();

    Ok(serde_json::json!({ "list": list, "source": "mg" }))
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

    let column_id = if raw_id.starts_with("mg__") {
        raw_id.replace("mg__", "")
    } else {
        raw_id
    };

    let url = format!(
        "https://app.c.nf.migu.cn/MIGUM2.0/v1.0/content/querycontentbyId.do?columnId={}&needAll=0",
        column_id
    );

    let resp: serde_json::Value = get_http().get(&url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if code != "000000" {
        return Err("MG leaderboard detail API error".into());
    }

    let column_info = resp.get("columnInfo").cloned().unwrap_or(serde_json::json!({}));
    let contents = column_info.get("contents").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let total = contents.len() as i64;

    // Paginate
    let start = ((page - 1) * limit) as usize;
    let end = (start + limit as usize).min(contents.len());
    let page_contents = if start < contents.len() && start < end {
        &contents[start..end]
    } else {
        &[]
    };

    let list: Vec<MusicItem> = page_contents.iter().filter_map(|item| {
        let id = item.get("id")?.as_i64()?.to_string();
        let title = item.get("title")?.as_str()?.to_string();
        let singer = item.get("artists").and_then(|a| a.get(0)).and_then(|a| a.get("name"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_name = item.get("albumName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_id = item.get("albumId").cloned().unwrap_or(serde_json::Value::Null);
        let cover = item.get("cover").and_then(|c| c.get("url")).and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("{size}", "400");
        let duration = item.get("duration")?.as_i64()?;
        let minutes = duration / 60000;
        let seconds = (duration % 60000) / 1000;
        let interval = format!("{:02}:{:02}", minutes, seconds);

        // Determine quality types
        let resource = item.get("resource").and_then(|r| r.as_object())?;
        let mut types = Vec::new();
        if resource.get("320k").is_some() { types.push("320k".to_string()); }
        if resource.get("flac").is_some() { types.push("flac".to_string()); }
        if resource.get("128k").is_some() { types.push("128k".to_string()); }
        if types.is_empty() { types.push("128k".to_string()); }

        Some(MusicItem {
            songmid: serde_json::json!(id),
            singer, name: title, album_name, album_id,
            source: "mg".into(), interval,
            img: cover, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})),
            hash: None,
        })
    }).collect();

    Ok(serde_json::json!({
        "list": list, "info": serde_json::json!({}),
        "allPage": (total as f64 / limit as f64).ceil() as i64,
        "limit": limit as i64, "total": total, "source": "mg"
    }))
}

// --- Playlist Detail ---

async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    if raw_id.is_empty() {
        return Ok(serde_json::json!({
            "list": [], "info": {}, "allPage": 0, "limit": limit as i64, "total": 0, "source": "mg"
        }));
    }

    // Get playlist info
    let info_url = format!(
        "https://c.musicapp.migu.cn/MIGUM3.0/resource/playlist/v2.0?playlistId={}",
        raw_id
    );
    let info_resp: serde_json::Value = get_http().get(&info_url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let playlist_info = info_resp.get("playlist").cloned().unwrap_or(serde_json::json!({}));
    let name = playlist_info.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let img = playlist_info.get("coverImgUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let desc = playlist_info.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let _play_count = playlist_info.get("playCount").cloned().unwrap_or(serde_json::Value::Null);

    // Get playlist songs
    let url = format!(
        "https://app.c.nf.migu.cn/MIGUM3.0/resource/playlist/song/v2.0?pageNo={}&pageSize={}&playlistId={}",
        page, limit, raw_id
    );

    let resp: serde_json::Value = get_http().get(&url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_str()).unwrap_or("");
    if code != "000000" {
        return Err("MG playlist detail API error".into());
    }

    let total = resp.get("totalCount").and_then(|v| v.as_i64()).unwrap_or(0);
    let contents = resp.get("list").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<MusicItem> = contents.iter().filter_map(|item| {
        let id = item.get("id")?.as_i64()?.to_string();
        let title = item.get("title")?.as_str()?.to_string();
        let singer = item.get("artists").and_then(|a| a.get(0)).and_then(|a| a.get("name"))
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_name = item.get("albumName").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let album_id = item.get("albumId").cloned().unwrap_or(serde_json::Value::Null);
        let cover = item.get("cover").and_then(|c| c.get("url")).and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("{size}", "400");
        let duration = item.get("duration")?.as_i64()?;
        let minutes = duration / 60000;
        let seconds = (duration % 60000) / 1000;
        let interval = format!("{:02}:{:02}", minutes, seconds);

        let resource = item.get("resource").and_then(|r| r.as_object())?;
        let mut types = Vec::new();
        if resource.get("320k").is_some() { types.push("320k".to_string()); }
        if resource.get("flac").is_some() { types.push("flac".to_string()); }
        if resource.get("128k").is_some() { types.push("128k".to_string()); }
        if types.is_empty() { types.push("128k".to_string()); }

        Some(MusicItem {
            songmid: serde_json::json!(id),
            singer, name: title, album_name, album_id,
            source: "mg".into(), interval,
            img: cover, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})),
            hash: None,
        })
    }).collect();

    Ok(serde_json::json!({
        "list": list,
        "info": { "name": name, "img": img, "desc": desc },
        "allPage": (total as f64 / limit as f64).ceil() as i64,
        "limit": limit as i64, "total": total, "source": "mg"
    }))
}

// --- Search ---

async fn search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 20);
    if keyword.is_empty() {
        return Ok(serde_json::to_value(SearchResult {
            list: vec![], all_page: 0, limit: limit as i64, total: 0, source: "mg".into(),
        }).unwrap());
    }

    // MG search uses a different endpoint; simplified implementation
    let url = format!(
        "https://m.music.migu.cn/migu/remoting/scr_search_tag?rows={}&type=2&keyword={}&pgc={}&pg={}",
        limit, urlencoding::encode(keyword), page, page
    );

    let resp: serde_json::Value = get_http().get(&url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let total = resp.get("totalCount").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp.get("musics").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<MusicItem> = raw_list.iter().filter_map(|item| {
        let id = item.get("id")?.as_str().unwrap_or("");
        let title = item.get("songName")?.as_str().unwrap_or("").to_string();
        let singer = item.get("singerName")?.as_str().unwrap_or("").to_string();
        let album_name = item.get("albumName")?.as_str().unwrap_or("").to_string();
        let album_id = item.get("albumId").cloned().unwrap_or(serde_json::Value::Null);
        let img = item.get("albumImgs").and_then(|imgs| imgs.get(0)).and_then(|img| img.get("img"))
            .and_then(|v| v.as_str()).unwrap_or("")
            .replace("{size}", "400");
        let duration = item.get("duration")?.as_i64()?;
        let minutes = duration / 60000;
        let seconds = (duration % 60000) / 1000;
        let interval = format!("{:02}:{:02}", minutes, seconds);

        let mut types = vec!["128k".to_string()];
        if let Some(mp3) = item.get("mp3").and_then(|v| v.as_str()) {
            if !mp3.is_empty() { types.push("320k".to_string()); }
        }

        Some(MusicItem {
            songmid: serde_json::json!(id),
            singer, name: title, album_name, album_id,
            source: "mg".into(), interval,
            img, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})),
            hash: None,
        })
    }).collect();

    Ok(serde_json::to_value(SearchResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "mg".into(),
    }).unwrap())
}

async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let url = format!(
        "https://m.music.migu.cn/migu/remoting/scr_search_tag?rows={}&type=12&keyword={}&pgc={}&pg={}",
        limit, urlencoding::encode(keyword), page, page
    );

    let resp: serde_json::Value = get_http().get(&url)
        .headers(mg_headers())
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let total = resp.get("totalCount").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp.get("musics").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list.iter().filter_map(|item| {
        let id = item.get("id")?.as_str()?;
        let name = item.get("title")?.as_str()?.to_string();
        let img = item.get("img")?.as_str()?.to_string();

        Some(PlaylistItem {
            id: serde_json::json!(id),
            name, img,
            source: "mg".into(),
            desc: String::new(),
            play_count: serde_json::Value::Null,
            author: String::new(),
        })
    }).collect();

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "mg".into(),
    }).unwrap())
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
        _ => Err(format!("Unknown SDK method for mg: {}", method)),
    }
}
