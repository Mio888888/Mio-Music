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

// --- Playlist Tags ---

async fn get_playlist_tags(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tags_url = "https://u.y.qq.com/cgi-bin/musicu.fcg?loginUin=0&hostUin=0&format=json&inCharset=utf-8&outCharset=utf-8&notice=0&platform=wk_v15.json&needNewCode=0&data=%7B%22tags%22%3A%7B%22method%22%3A%22get_all_categories%22%2C%22param%22%3A%7B%22qq%22%3A%22%22%7D%2C%22module%22%3A%22playlist.PlaylistAllCategoriesServer%22%7D%7D";
    let hot_tag_url = "https://c.y.qq.com/node/pc/wk_v15/category_playlist.html";

    let (tags_result, hot_result) = tokio::join!(
        async {
            let resp: serde_json::Value = get_http().get(tags_url)
                .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;

            let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            if code != 0 {
                return Err(format!("TX tag API error: code={}", code));
            }

            let v_group = resp.get("tags").and_then(|t| t.get("data")).and_then(|d| d.get("v_group"))
                .and_then(|v| v.as_array()).cloned().unwrap_or_default();

            let tags: Vec<serde_json::Value> = v_group.iter().map(|group| {
                let name = group.get("group_name").and_then(|v| v.as_str()).unwrap_or("");
                let list: Vec<serde_json::Value> = group.get("v_item").and_then(|v| v.as_array())
                    .cloned().unwrap_or_default()
                    .iter().map(|item| {
                        let id = item.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        serde_json::json!({
                            "id": id,
                            "name": name,
                            "source": "tx"
                        })
                    }).collect();
                serde_json::json!({ "name": name, "list": list, "source": "tx" })
            }).collect();

            Ok::<_, String>(tags)
        },
        async {
            let resp = get_http().get(hot_tag_url)
                .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
                .send().await.map_err(|e| e.to_string())?;
            let html = resp.text().await.map_err(|e| e.to_string())?;

            // Parse hot tags from HTML: data-id="(\w+)">(.+?)</a>
            let re = regex_lite::Regex::new(r#"data-id="(\w+)">(.+?)</a>"#).map_err(|e| e.to_string())?;
            let hot: Vec<serde_json::Value> = re.captures_iter(&html).filter_map(|caps| {
                let id = caps.get(1)?.as_str();
                let name = caps.get(2)?.as_str();
                Some(serde_json::json!({ "id": id, "name": name, "source": "tx" }))
            }).collect();

            Ok::<_, String>(hot)
        }
    );

    let tags = tags_result.unwrap_or_default();
    let hot_tag = hot_result.unwrap_or_default();
    Ok(serde_json::json!({ "tags": tags, "hotTag": hot_tag, "source": "tx" }))
}

// --- Category Playlists ---

fn build_tx_playlist_url(sort_id: &str, tag_id: &str, page: u64) -> String {
    let base = "https://u.y.qq.com/cgi-bin/musicu.fcg?loginUin=0&hostUin=0&format=json&inCharset=utf-8&outCharset=utf-8&notice=0&platform=wk_v15.json&needNewCode=0";

    let data = if !tag_id.is_empty() {
        // Use category-specific API
        let id: i64 = tag_id.parse().unwrap_or(0);
        serde_json::json!({
            "comm": { "cv": 1602, "ct": 20 },
            "playlist": {
                "method": "get_category_content",
                "param": {
                    "titleid": id, "caller": "0", "category_id": id,
                    "size": 36, "page": (page as i64) - 1, "use_page": 1
                },
                "module": "playlist.PlayListCategoryServer"
            }
        })
    } else {
        let order = if sort_id.is_empty() || sort_id == "hot" { "5" } else { sort_id };
        serde_json::json!({
            "comm": { "cv": 1602, "ct": 20 },
            "playlist": {
                "method": "get_playlist_by_tag",
                "param": {
                    "id": 10000000_i64,
                    "sin": (36_i64) * ((page as i64) - 1),
                    "size": 36_i64,
                    "order": order.parse::<i64>().unwrap_or(5),
                    "cur_page": page as i64
                },
                "module": "playlist.PlayListPlazaServer"
            }
        })
    };

    format!("{}&data={}", base, urlencoding::encode(&serde_json::to_string(&data).unwrap_or_default()))
}

async fn get_category_playlists(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tag_id = get_str(&args, "tagId").to_string();
    let sort_id = get_str(&args, "sortId").to_string();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 36);

    let url = build_tx_playlist_url(&sort_id, &tag_id, page);
    let resp: serde_json::Value = get_http().get(&url)
        .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("TX playlist API error: code={}", code));
    }

    let playlist_data = resp.get("playlist").and_then(|p| p.get("data")).cloned().unwrap_or(serde_json::json!({}));

    let (list, total) = if tag_id.is_empty() {
        // filterList: data.v_playlist
        let raw_list = playlist_data.get("v_playlist").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let total = playlist_data.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
        let list: Vec<PlaylistItem> = raw_list.iter().map(|item| {
            let tid = item.get("tid").cloned().unwrap_or(serde_json::Value::Null);
            PlaylistItem {
                id: tid,
                name: item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                img: item.get("cover_url_medium").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                source: "tx".into(),
                desc: item.get("desc").and_then(|v| v.as_str()).unwrap_or("").replace("<br>", "\n"),
                play_count: item.get("access_num").cloned().unwrap_or(serde_json::Value::Null),
                author: item.get("creator_info").and_then(|c| c.get("nick")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                total: serde_json::Value::Null,
            }
        }).collect();
        (list, total)
    } else {
        // filterList2: content.v_item
        let content = playlist_data.get("content").cloned().unwrap_or(serde_json::json!({}));
        let raw_list = content.get("v_item").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let total = content.get("total_cnt").and_then(|v| v.as_i64()).unwrap_or(0);
        let list: Vec<PlaylistItem> = raw_list.iter().filter_map(|item| {
            let basic = item.get("basic")?;
            let tid = basic.get("tid").cloned().unwrap_or(serde_json::Value::Null);
            Some(PlaylistItem {
                id: tid,
                name: basic.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                img: basic.get("cover").and_then(|c| c.get("medium_url").or(c.get("default_url"))).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                source: "tx".into(),
                desc: basic.get("desc").and_then(|v| v.as_str()).unwrap_or("").replace("<br>", "\n"),
                play_count: basic.get("play_cnt").cloned().unwrap_or(serde_json::Value::Null),
                author: basic.get("creator").and_then(|c| c.get("nick")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                total: serde_json::Value::Null,
            })
        }).collect();
        (list, total)
    };

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "tx".into(),
    }).unwrap())
}

// --- Leaderboards ---

async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "https://c.y.qq.com/v8/fcg-bin/fcg_myqq_toplist.fcg?g_tk=1928093487&inCharset=utf-8&outCharset=utf-8&notice=0&format=json&uin=0&needNewCode=1&platform=h5";
    let resp: serde_json::Value = get_http().get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("TX toplist API error: code={}", code));
    }

    let raw_list = resp.get("data").and_then(|d| d.get("topList")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let list: Vec<serde_json::Value> = raw_list.iter().filter_map(|board| {
        let id = board.get("id")?.as_i64()?;
        if id == 201 { return None; } // Exclude MV榜

        let mut name = board.get("topTitle")?.as_str()?.to_string();
        // Normalize name to match reference
        if name.starts_with("巅峰榜·") {
            name = name.replace("巅峰榜·", "");
        }
        if !name.ends_with('榜') {
            name.push('榜');
        }

        let pic = board.get("picUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let listen = board.get("listenCount").cloned().unwrap_or(serde_json::Value::Null);

        Some(serde_json::json!({
            "id": format!("tx__{}", id),
            "name": name,
            "bangid": id.to_string(),
            "img": pic,
                "pic": pic,
            "listen": listen,
            "source": "tx"
        }))
    }).collect();

    Ok(serde_json::json!({ "list": list, "source": "tx" }))
}

// --- Leaderboard Detail (songs in a board) ---

async fn get_leaderboard_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let _page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 300);

    // Extract numeric ID from "tx__XX" format
    let bang_id: i64 = if raw_id.starts_with("tx__") {
        raw_id.replace("tx__", "").parse().unwrap_or(0)
    } else {
        raw_id.parse().unwrap_or(0)
    };

    let body = serde_json::json!({
        "toplist": {
            "module": "musicToplist.ToplistInfoServer",
            "method": "GetDetail",
            "param": { "topid": bang_id, "num": limit, "period": "" }
        },
        "comm": { "uin": 0, "format": "json", "ct": 20, "cv": 1859 }
    });

    let resp: serde_json::Value = get_http()
        .post("https://u.y.qq.com/cgi-bin/musicu.fcg")
        .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
        .json(&body)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("TX toplist detail API error: code={}", code));
    }

    let toplist_data = resp.get("toplist").and_then(|t| t.get("data")).cloned().unwrap_or(serde_json::json!({}));
    let song_info_list = toplist_data.get("songInfoList").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let total = song_info_list.len() as i64;

    let list: Vec<MusicItem> = song_info_list.iter().map(|item| {
        let singer_list = item.get("singer").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let singer = singer_list.iter().filter_map(|s| s.get("name")).filter_map(|n| n.as_str()).collect::<Vec<_>>().join("、");
        let name = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let album_name = item.get("album").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("");
        let album_mid = item.get("album").and_then(|a| a.get("mid")).and_then(|v| v.as_str()).unwrap_or("");
        let songmid = item.get("mid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let interval = item.get("interval").and_then(|v| v.as_i64()).unwrap_or(0);

        let img = if album_name.is_empty() || album_name == "空" {
            singer_list.first().and_then(|s| s.get("mid")).and_then(|v| v.as_str())
                .map(|m| format!("https://y.gtimg.cn/music/photo_new/T001R500x500M000{}.jpg", m))
                .unwrap_or_default()
        } else {
            format!("https://y.gtimg.cn/music/photo_new/T002R500x500M000{}.jpg", album_mid)
        };

        // Quality types
        let file = item.get("file").cloned().unwrap_or(serde_json::json!({}));
        let mut types = Vec::new();
        if file.get("size_new").and_then(|v| v.get(0)).and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("master".to_string()); }
        if file.get("size_hires").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("hires".to_string()); }
        if file.get("size_flac").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("flac".to_string()); }
        if file.get("size_320mp3").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("320k".to_string()); }
        if file.get("size_128mp3").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("128k".to_string()); }

        MusicItem {
            songmid: serde_json::Value::String(songmid), singer, name: name.to_string(),
            album_name: album_name.to_string(), album_id: serde_json::Value::String(album_mid.to_string()),
            source: "tx".into(), interval: format_play_time(interval), img, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})), hash: None,
        }
    }).collect();

    Ok(serde_json::json!({
        "list": list, "info": serde_json::json!({}),
        "allPage": 1, "limit": limit as i64, "total": total, "source": "tx"
    }))
}

// --- Playlist Detail ---

async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args.get("id").map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let limit = get_u64(&args, "limit", 300);

    let id = raw_id.replace("tx_", "").trim().to_string();

    let url = format!(
        "https://c.y.qq.com/qzone/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg?type=1&json=1&utf8=1&onlysong=0&new_format=1&disstid={}&loginUin=0&hostUin=0&format=json&inCharset=utf8&outCharset=utf-8&notice=0&platform=yqq.json&needNewCode=0",
        id
    );

    let resp: serde_json::Value = get_http().get(&url)
        .header("Origin", "https://y.qq.com")
        .header("Referer", format!("https://y.qq.com/n/yqq/playsquare/{}.html", id))
        .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("TX playlist detail API error: code={}", code));
    }

    let cdlist = resp.get("cdlist").and_then(|v| v.as_array()).and_then(|a| a.first()).cloned().unwrap_or(serde_json::json!({}));
    let songlist = cdlist.get("songlist").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let total = songlist.len() as i64;

    let list: Vec<MusicItem> = songlist.iter().map(|item| {
        let singer_list = item.get("singer").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let singer = singer_list.iter().filter_map(|s| s.get("name")).filter_map(|n| n.as_str()).collect::<Vec<_>>().join("、");
        let name = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let album_name = item.get("album").and_then(|a| a.get("name")).and_then(|v| v.as_str()).unwrap_or("");
        let album_mid = item.get("album").and_then(|a| a.get("mid")).and_then(|v| v.as_str()).unwrap_or("");
        let songmid = item.get("mid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let interval = item.get("interval").and_then(|v| v.as_i64()).unwrap_or(0);

        let img = if album_name.is_empty() || album_name == "空" {
            singer_list.first().and_then(|s| s.get("mid")).and_then(|v| v.as_str())
                .map(|m| format!("https://y.gtimg.cn/music/photo_new/T001R500x500M000{}.jpg", m))
                .unwrap_or_default()
        } else {
            format!("https://y.gtimg.cn/music/photo_new/T002R500x500M000{}.jpg", album_mid)
        };

        let file = item.get("file").cloned().unwrap_or(serde_json::json!({}));
        let mut types = Vec::new();
        if file.get("size_new").and_then(|v| v.get(0)).and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("master".to_string()); }
        if file.get("size_hires").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("hires".to_string()); }
        if file.get("size_flac").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("flac".to_string()); }
        if file.get("size_320mp3").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("320k".to_string()); }
        if file.get("size_128mp3").and_then(|v| v.as_i64()).unwrap_or(0) > 0 { types.push("128k".to_string()); }

        MusicItem {
            songmid: serde_json::Value::String(songmid), singer, name: name.to_string(),
            album_name: album_name.to_string(), album_id: serde_json::Value::String(album_mid.to_string()),
            source: "tx".into(), interval: format_play_time(interval), img, lrc: None,
            types: Some(types), type_url: Some(serde_json::json!({})), hash: None,
        }
    }).collect();

    Ok(serde_json::json!({
        "list": list,
        "info": {
            "name": cdlist.get("dissname").and_then(|v| v.as_str()).unwrap_or(""),
            "img": cdlist.get("logo").and_then(|v| v.as_str()).unwrap_or(""),
            "desc": cdlist.get("desc").and_then(|v| v.as_str()).unwrap_or("").replace("<br>", "\n"),
            "author": cdlist.get("nickname").and_then(|v| v.as_str()).unwrap_or("")
        },
        "allPage": 1, "limit": limit as i64, "total": total, "source": "tx"
    }))
}

// --- Search ---

async fn search(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 20);
    if keyword.is_empty() {
        return Ok(serde_json::to_value(SearchResult {
            list: vec![], all_page: 0, limit: limit as i64, total: 0, source: "tx".into(),
        }).unwrap());
    }

    let url = format!(
        "http://c.y.qq.com/soso/fcgi-bin/client_music_search_songlist?page_no={}&num_per_page={}&format=json&query={}&remoteplace=txt.yqq.playlist&inCharset=utf8&outCharset=utf-8",
        page - 1, limit, urlencoding::encode(keyword)
    );

    let resp: serde_json::Value = get_http().get(&url)
        .header("User-Agent", "Mozilla/5.0 (compatible; MSIE 9.0; Windows NT 6.1; WOW64; Trident/5.0)")
        .header("Referer", "http://y.qq.com/portal/search.html")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let code = resp.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        return Err(format!("TX search API error: code={}", code));
    }

    let data = resp.get("data").cloned().unwrap_or(serde_json::json!({}));
    let total = data.get("sum").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = data.get("list").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list.iter().map(|item| {
        let dissid = item.get("dissid").cloned().unwrap_or(serde_json::Value::Null);
        PlaylistItem {
            id: dissid,
            name: item.get("dissname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            img: item.get("imgurl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source: "tx".into(),
            desc: item.get("introduction").and_then(|v| v.as_str()).unwrap_or("").replace("<br>", "\n"),
            play_count: item.get("listennum").cloned().unwrap_or(serde_json::Value::Null),
            author: item.get("creator").and_then(|c| c.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
            total: serde_json::Value::Null,
        }
    }).collect();

    Ok(serde_json::to_value(PlaylistResult {
        list, all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64, total, source: "tx".into(),
    }).unwrap())
}

async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    search(args).await
}

// --- Helpers ---

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
        _ => Err(format!("Unknown SDK method for tx: {}", method)),
    }
}
