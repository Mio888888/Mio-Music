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

fn get_songmid(args: &serde_json::Value) -> String {
    let info = args.get("songInfo").unwrap_or(&serde_json::Value::Null);
    info.get("songmid").map(|v| match v {
        serde_json::Value::String(s) => s.clone(), serde_json::Value::Number(n) => n.to_string(), _ => String::new()
    }).unwrap_or_default()
}

// --- Search ---

async fn search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);
    if keyword.is_empty() { return empty_search("kw"); }

    let url = format!(
        "http://search.kuwo.cn/r.s?client=kt&all={}&pn={}&rn={}&uid=794762570&ver=kwplayer_ar_9.2.2.1&vipver=1&show_copyright_off=1&newver=1&ft=music&cluster=0&strategy=2012&encoding=utf8&rformat=json&vermerge=1&mobi=1&issubtitle=1",
        urlencoding::encode(keyword), page - 1, limit
    );
    let resp: serde_json::Value = get_http().get(&url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let total: i64 = resp.get("TOTAL").and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let abslist = resp.get("abslist").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<MusicItem> = abslist.iter().filter_map(|info| parse_music_item(info)).collect();

    Ok(serde_json::to_value(SearchResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64, limit: limit as i64, total, source: "kw".into(),
    }).unwrap())
}

fn parse_music_item(info: &serde_json::Value) -> Option<MusicItem> {
    let rid = info.get("MUSICRID")?.as_str()?;
    let songmid = rid.replace("MUSIC_", "");
    let name = decode_html(info.get("SONGNAME").and_then(|v| v.as_str()).unwrap_or(""));
    let singer = format_singer(decode_html(info.get("ARTIST").and_then(|v| v.as_str()).unwrap_or("")));
    let album = decode_html(info.get("ALBUM").and_then(|v| v.as_str()).unwrap_or(""));
    let album_id = info.get("ALBUMID").cloned().unwrap_or(serde_json::Value::String(String::new()));
    let duration: i64 = info.get("DURATION").and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let interval = format_play_time(duration);

    let minfo = info.get("N_MINFO").and_then(|v| v.as_str()).unwrap_or("");
    let types = parse_quality_types(minfo);

    Some(MusicItem {
        songmid: serde_json::Value::String(songmid),
        singer, name, album_name: album, album_id, source: "kw".into(), interval,
        img: String::new(), lrc: None, types: Some(types), type_url: Some(serde_json::json!({})), hash: None,
    })
}

fn parse_quality_types(minfo: &str) -> Vec<String> {
    let mut types = Vec::new();
    for part in minfo.split(';') {
        let fields: Vec<&str> = part.split(',').collect();
        if fields.len() < 3 { continue; }
        let bitrate = fields.get(1).unwrap_or(&"").parse::<u32>().unwrap_or(0);
        let quality = match bitrate {
            20900 => Some("master"), 4000 => Some("hires"), 2000 => Some("flac"),
            320 => Some("320k"), 128 => Some("128k"), _ => None,
        };
        if let Some(q) = quality { types.push(q.to_string()); }
    }
    types.sort_by(|a, b| quality_rank(b).cmp(&quality_rank(a)));
    types
}

fn quality_rank(q: &str) -> u8 { match q { "master" => 5, "hires" => 4, "flac" => 3, "320k" => 2, "128k" => 1, _ => 0 } }

// --- Tip Search ---

async fn tip_search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    if keyword.is_empty() { return Ok(serde_json::json!({ "list": [] })); }

    let url = format!("https://tips.kuwo.cn/t.s?corp=kuwo&newver=3&p2p=1&notrace=0&c=mbox&w={}&encoding=utf8&rformat=json", urlencoding::encode(keyword));
    let resp: serde_json::Value = get_http().get(&url)
        .header("Referer", "http://www.kuwo.cn/")
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let items = resp.get("WORDITEMS").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let songs: Vec<serde_json::Value> = items.iter().map(|item| {
        serde_json::json!({ "name": item.get("RELWORD").and_then(|v| v.as_str()).unwrap_or("") })
    }).collect();

    Ok(serde_json::json!({ "order": ["songs"], "songs": songs }))
}

// --- Hot Search ---

async fn hot_search(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "http://hotword.kuwo.cn/hotword.s?prod=kwplayer_ar_9.3.0.1&corp=kuwo&newver=2&vipver=9.3.0.1&source=kwplayer_ar_9.3.0.1_40.apk&p2p=1&notrace=0&uid=0&plat=kwplayer_ar&rformat=json&encoding=utf8&tabid=1";
    let resp: serde_json::Value = get_http().get(url)
        .header("User-Agent", "Dalvik/2.1.0 (Linux; U; Android 9;)")
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let list = resp.get("tagvalue").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let keywords: Vec<serde_json::Value> = list.iter()
        .filter_map(|item| item.get("key").and_then(|v| v.as_str()).map(|s| serde_json::json!(s)))
        .collect();

    Ok(serde_json::json!({ "source": "kw", "list": keywords }))
}

// --- Get Music URL ---

async fn get_music_url(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);
    let quality = get_str(&args, "quality");

    let url = format!("http://www.kuwo.cn/api/v1/www/music/playInfo?mid={}&type=music&httpsStatus=1", songmid);
    let resp: serde_json::Value = get_http().get(&url)
        .header("Referer", "http://www.kuwo.cn/")
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let play_url = resp.get("data").and_then(|d| d.get("url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Ok(serde_json::json!({ "url": play_url, "type": quality }))
}

// --- Get Pic ---

async fn get_pic(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);

    let url = format!("http://artistpicserver.kuwo.cn/pic.web?corp=kuwo&type=rid_pic&pictype=500&size=500&rid={}", songmid);
    let body = get_http().get(&url).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;

    let pic_url = if body.starts_with("http") { body } else { String::new() };
    Ok(serde_json::json!({ "url": pic_url }))
}

// --- Get Lyric ---

async fn get_lyric(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);

    let url = format!("http://m.kuwo.cn/newh5/singles/songinfoandlrc?mid={}&type=music&httpsStatus=1", songmid);
    let resp: serde_json::Value = get_http().get(&url)
        .header("Referer", "http://m.kuwo.cn/")
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let lrclist = resp.get("data").and_then(|d| d.get("lrclist")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let lrc_text: String = lrclist.iter().filter_map(|line| {
        let text = line.get("lineLyric").and_then(|v| v.as_str()).unwrap_or("");
        let t = line.get("time").and_then(|v| v.as_str()).unwrap_or("0");
        let ms: f64 = t.parse().ok()?;
        let total_ms = (ms * 1000.0) as u64;
        let min = total_ms / 60000;
        let sec = (total_ms % 60000) / 1000;
        let ms_rem = total_ms % 1000;
        Some(format!("[{:02}:{:02}.{:03}]{}", min, sec, ms_rem, text))
    }).collect::<Vec<_>>().join("\n");

    Ok(serde_json::json!({ "lrc": lrc_text, "source": "kw" }))
}

// --- Comments ---

async fn get_comment(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 20);

    let start = limit * (page - 1);
    let url = format!("http://ncomment.kuwo.cn/com.s?f=web&type=get_comment&aapiver=1&prod=kwplayer_ar_10.5.2.0&digest=15&sid={}&start={}&msgflag=1&count={}&newver=3&uid=0", songmid, start, limit);
    fetch_comments(url, page, limit).await
}

async fn get_hot_comment(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 100);

    let start = limit * (page - 1);
    let url = format!("http://ncomment.kuwo.cn/com.s?f=web&type=get_rec_comment&aapiver=1&prod=kwplayer_ar_10.5.2.0&digest=15&sid={}&start={}&msgflag=1&count={}&newver=3&uid=0", songmid, start, limit);
    fetch_comments(url, page, limit).await
}

async fn fetch_comments(url: String, page: u64, limit: u64) -> Result<serde_json::Value, String> {
    let resp: serde_json::Value = get_http().get(&url)
        .header("User-Agent", "Dalvik/2.1.0 (Linux; U; Android 9;)")
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let total = resp.get("comments_counts").and_then(|v| v.as_i64()).unwrap_or(0);
    let hot_total = resp.get("hot_comments_counts").and_then(|v| v.as_i64()).unwrap_or(0);
    let actual_total = if total > 0 { total } else { hot_total };

    let raw = resp.get("comments").or(resp.get("hot_comments")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let comments: Vec<serde_json::Value> = raw.iter().map(|item| {
        let child = item.get("child_comments").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let reply: Vec<serde_json::Value> = child.iter().map(|c| {
            serde_json::json!({
                "id": c.get("id"), "text": c.get("msg"), "time": c.get("time"),
                "userName": c.get("u_name"), "avatar": c.get("u_pic"), "userId": c.get("u_id"),
                "likedCount": c.get("like_num"), "images": c.get("mpic").map(|m| vec![m]).unwrap_or_default()
            })
        }).collect();
        serde_json::json!({
            "id": item.get("id"), "text": item.get("msg"), "time": item.get("time"),
            "userName": item.get("u_name"), "avatar": item.get("u_pic"), "userId": item.get("u_id"),
            "likedCount": item.get("like_num"),
            "images": item.get("mpic").and_then(|m| m.as_str()).map(|m| vec![serde_json::json!(m)]).unwrap_or_default(),
            "reply": reply
        })
    }).collect();

    Ok(serde_json::json!({
        "source": "kw", "comments": comments, "total": actual_total,
        "page": page, "limit": limit, "maxPage": ((actual_total as f64 / limit as f64).ceil() as i64).max(1)
    }))
}

// --- Hot Songlist ---

async fn get_hot_songlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let url = format!("http://wapi.kuwo.cn/api/pc/classify/playlist/getRcmPlayList?pn={}&rn={}&order=hot&vipver=1", page - 1, limit);
    let resp: serde_json::Value = get_http().get(&url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let total = resp.get("data").and_then(|d| d.get("total")).and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp.get("data").and_then(|d| d.get("data")).and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list.iter().map(|item| PlaylistItem {
        id: item.get("playlistid").cloned().unwrap_or(serde_json::Value::Null),
        name: item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        img: item.get("img").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        source: "kw".into(),
        desc: item.get("intro").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        play_count: item.get("playCount").cloned().unwrap_or(serde_json::Value::Null),
        author: String::new(),
    }).collect();

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64, limit: limit as i64, total, source: "kw".into(),
    }).unwrap())
}

// --- Leaderboards ---

async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "http://kbangserver.kuwo.cn/ksong.s?from=pc&fmt=json&pn=0&rn=100&method=bd.getTopList&isbang=1&pc=1&m=&n=&v=&num=&show_copyright_off=0&pc=1&isbang=1&isweb=1&data=";
    let resp: serde_json::Value = get_http().get(url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let raw_list = resp.get("musiclist").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = raw_list.iter().map(|item| {
        serde_json::json!({
            "id": item.get("sourceid"), "name": item.get("name"),
            "img": item.get("img5s").or(item.get("img")), "source": "kw"
        })
    }).collect();

    Ok(serde_json::json!({ "list": list, "source": "kw" }))
}

// --- Playlist Detail ---

async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(), serde_json::Value::Number(n) => n.to_string(), _ => String::new()
    }).unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let url = format!("http://nplserver.kuwo.cn/pl.svc?op=getlistinfo&pid={}&pn={}&rn={}&encode=utf8&vipver=1&isbang=1", id, page - 1, limit);
    let resp: serde_json::Value = get_http().get(&url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let total = resp.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp.get("musiclist").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<MusicItem> = raw_list.iter().filter_map(|info| parse_music_item(info)).collect();

    Ok(serde_json::json!({
        "list": list, "info": resp.get("info").cloned().unwrap_or(serde_json::json!({})),
        "allPage": (total as f64 / limit as f64).ceil() as i64, "limit": limit as i64, "total": total, "source": "kw"
    }))
}

// --- Search Playlist ---

async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    if keyword.is_empty() {
        return Ok(serde_json::to_value(PlaylistResult {
            list: vec![], all_page: 0, limit: limit as i64, total: 0, source: "kw".into(),
        }).unwrap());
    }

    let url = format!("http://search.kuwo.cn/r.s?client=kt&all={}&pn={}&rn={}&ft=playlist&cluster=0&strategy=2012&encoding=utf8&rformat=json", urlencoding::encode(keyword), page - 1, limit);
    let resp: serde_json::Value = get_http().get(&url).send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;

    let total: i64 = resp.get("TOTAL").and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0);
    let raw_list = resp.get("abslist").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list.iter().map(|item| PlaylistItem {
        id: item.get("playlistid").cloned().unwrap_or(serde_json::Value::Null),
        name: decode_html(item.get("name").and_then(|v| v.as_str()).unwrap_or("")),
        img: item.get("img").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        source: "kw".into(),
        desc: String::new(), play_count: item.get("playCount").cloned().unwrap_or(serde_json::Value::Null), author: String::new(),
    }).collect();

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64, limit: limit as i64, total, source: "kw".into(),
    }).unwrap())
}

// --- Helpers ---

fn empty_search(source: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::to_value(SearchResult { list: vec![], all_page: 0, limit: 30, total: 0, source: source.into() }).unwrap())
}

fn decode_html(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
}

fn format_singer(raw: String) -> String { raw.replace("&", "、") }

fn format_play_time(seconds: i64) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{:02}:{:02}", m, s)
}

// --- Router ---

pub async fn handle(method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    match method {
        "search" => search(args).await,
        "tipSearch" => tip_search(args).await,
        "hotSearch" => hot_search(args).await,
        "getMusicUrl" => get_music_url(args).await,
        "getPic" => get_pic(args).await,
        "getLyric" => get_lyric(args).await,
        "getComment" => get_comment(args).await,
        "getHotComment" => get_hot_comment(args).await,
        "getHotSonglist" | "getHotPlaylists" => get_hot_songlist(args).await,
        "getLeaderboards" => get_leaderboards(args).await,
        "getPlaylistDetail" | "getPlaylistDetailById" => get_playlist_detail(args).await,
        "searchPlaylist" => search_playlist(args).await,
        _ => Err(format!("Unknown SDK method for kw: {}", method)),
    }
}
