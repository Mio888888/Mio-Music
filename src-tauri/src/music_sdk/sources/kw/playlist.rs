use super::helpers::*;
use crate::music_sdk::client::{MusicItem, PlaylistItem, PlaylistResult};

fn filter_tag_info(raw_list: &[serde_json::Value]) -> Vec<serde_json::Value> {
    raw_list
        .iter()
        .map(|type_obj| {
            let name = type_obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data = type_obj
                .get("data")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let list: Vec<serde_json::Value> = data
                .iter()
                .map(|item| {
                    let item_id = item
                        .get("id")
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    let digest = item
                        .get("digest")
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": format!("{}-{}", item_id, digest),
                        "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "source": "kw"
                    })
                })
                .collect();
            serde_json::json!({ "name": name, "list": list })
        })
        .collect()
}

pub async fn get_hot_songlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let url = format!("http://wapi.kuwo.cn/api/pc/classify/playlist/getRcmPlayList?pn={}&rn={}&order=hot&vipver=1", page - 1, limit);
    let resp: serde_json::Value = get_http()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    parse_playlists(&resp, "kw", limit)
}

pub async fn get_playlist_tags(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tags_url = "http://wapi.kuwo.cn/api/pc/classify/playlist/getTagList?cmd=rcm_keyword_playlist&user=0&prod=kwplayer_pc_9.0.5.0&vipver=9.0.5.0&source=kwplayer_pc_9.0.5.0&loginUid=0&loginSid=0&appUid=76039576";
    let hot_tag_url = "http://wapi.kuwo.cn/api/pc/classify/playlist/getRcmTagList?loginUid=0&loginSid=0&appUid=76039576";

    let (tags_result, hot_result) = tokio::join!(
        async {
            let resp: serde_json::Value = get_http()
                .get(tags_url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let data = resp
                .get("data")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            Ok::<_, String>(filter_tag_info(&data))
        },
        async {
            let resp: serde_json::Value = get_http()
                .get(hot_tag_url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let raw_data = resp
                .get("data")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let hot_tags_raw = raw_data
                .first()
                .and_then(|g| g.get("data"))
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            let hot: Vec<serde_json::Value> = hot_tags_raw
                .iter()
                .map(|item| {
                    let item_id = item
                        .get("id")
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    let digest = item
                        .get("digest")
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": format!("{}-{}", item_id, digest),
                        "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        "source": "kw"
                    })
                })
                .collect();
            Ok::<_, String>(hot)
        }
    );

    let tags = tags_result.unwrap_or_default();
    let hot_tag = hot_result.unwrap_or_default();

    Ok(serde_json::json!({ "tags": tags, "hotTag": hot_tag, "source": "kw" }))
}

pub async fn get_category_playlists(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let tag_id = get_str(&args, "tagId").to_string();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 36);

    if tag_id.is_empty() {
        let url = format!("http://wapi.kuwo.cn/api/pc/classify/playlist/getRcmPlayList?loginUid=0&loginSid=0&appUid=76039576&pn={}&rn={}&order=hot", page - 1, limit);
        let resp: serde_json::Value = get_http()
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        return parse_playlists(&resp, "kw", limit);
    }

    let parts: Vec<&str> = tag_id.splitn(2, '-').collect();
    let numeric_id = parts[0];
    let digest = if parts.len() > 1 { parts[1] } else { "10000" };

    match digest {
        "43" => {
            let url = format!(
                "http://mobileinterfaces.kuwo.cn/er.s?type=get_pc_qz_data&f=web&id={}&prod=pc",
                numeric_id
            );
            let resp: serde_json::Value = get_http()
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            parse_playlists_mobile(&resp, "kw", limit)
        }
        _ => {
            let url = format!("http://wapi.kuwo.cn/api/pc/classify/playlist/getTagPlayList?loginUid=0&loginSid=0&appUid=76039576&pn={}&id={}&rn={}", page - 1, numeric_id, limit);
            let resp: serde_json::Value = get_http()
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            parse_playlists(&resp, "kw", limit)
        }
    }
}

pub async fn get_leaderboards(_args: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = "http://qukudata.kuwo.cn/q.k?op=query&cont=tree&node=2&pn=0&rn=1000&fmt=json&level=3";
    let resp: serde_json::Value = get_http()
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let raw_list = resp
        .get("child")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let list: Vec<serde_json::Value> = raw_list
        .iter()
        .map(|item| {
            let sourceid = item.get("sourceid").and_then(|v| v.as_str()).unwrap_or("");
            let listen = item
                .get("listen")
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
                })
                .unwrap_or(0);
            let listen_str = if listen >= 100_000_000 {
                format!("{:.1}亿", listen as f64 / 100_000_000.0)
            } else if listen >= 10_000 {
                format!("{:.1}万", listen as f64 / 10_000.0)
            } else {
                listen.to_string()
            };
            serde_json::json!({
                "id": format!("kw__{}", sourceid),
                "board_id": sourceid,
                "name": item.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "img": item.get("pic").and_then(|v| v.as_str()).unwrap_or(""),
                "pic": item.get("pic").and_then(|v| v.as_str()).unwrap_or(""),
                "listen": listen_str,
                "update_frequency": item.get("info").and_then(|v| v.as_str()).unwrap_or(""),
                "source": "kw"
            })
        })
        .collect();

    Ok(serde_json::json!({ "list": list, "source": "kw" }))
}

pub async fn get_playlist_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args
        .get("id")
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default();
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let id = if raw_id.starts_with("digest-") {
        if let Some(pos) = raw_id.find("__") {
            raw_id[pos + 2..].to_string()
        } else {
            raw_id.clone()
        }
    } else {
        raw_id.clone()
    };

    let url = format!("http://nplserver.kuwo.cn/pl.svc?op=getlistinfo&pid={}&pn={}&rn={}&encode=utf8&keyset=pl2012&identity=kuwo&pcmp4=1&vipver=MUSIC_9.0.5.0_W1&newver=1", id, page - 1, limit);
    let resp: serde_json::Value = get_http()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let total = resp.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp
        .get("musiclist")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let list: Vec<MusicItem> = raw_list
        .iter()
        .filter_map(parse_music_item)
        .collect();

    let title = resp.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let pic = resp.get("pic").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let info_desc = resp.get("info").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let author = resp.get("uname").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(serde_json::json!({
        "list": list,
        "info": { "name": title, "img": pic, "desc": info_desc, "author": author },
        "allPage": (total as f64 / limit as f64).ceil() as i64, "limit": limit as i64, "total": total, "source": "kw"
    }))
}

pub async fn get_leaderboard_detail(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let raw_id = args
        .get("id")
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default();

    let id = raw_id.strip_prefix("kw__").unwrap_or(&raw_id).to_string();

    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    let url = format!(
        "http://kbangserver.kuwo.cn/ksong.s?from=pc&fmt=json&pn={}&rn={}&type=bang&data=content&id={}&show_copyright_off=0&pcmp4=1&isbang=1",
        page - 1, limit, id
    );
    let resp: serde_json::Value = get_http()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let total = resp.get("num").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = resp
        .get("musiclist")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let list: Vec<MusicItem> = raw_list
        .iter()
        .filter_map(parse_music_item)
        .collect();

    let title = resp.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let pic = resp.get("pic").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let info_desc = resp.get("info").and_then(|v| v.as_str()).unwrap_or("").to_string();

    Ok(serde_json::json!({
        "list": list,
        "info": { "name": title, "img": pic, "desc": info_desc },
        "allPage": (total as f64 / limit as f64).ceil() as i64,
        "limit": limit as i64, "total": total, "source": "kw"
    }))
}

pub async fn search_playlist(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let keyword = get_str(&args, "keyword");
    let page = get_u64(&args, "page", 1);
    let limit = get_u64(&args, "limit", 30);

    if keyword.is_empty() {
        return Ok(serde_json::to_value(PlaylistResult {
            list: vec![],
            all_page: 0,
            limit: limit as i64,
            total: 0,
            source: "kw".into(),
        })
        .unwrap());
    }

    let url = format!("http://search.kuwo.cn/r.s?client=kt&all={}&pn={}&rn={}&ft=playlist&cluster=0&strategy=2012&encoding=utf8&rformat=json", urlencoding::encode(keyword), page - 1, limit);
    let resp: serde_json::Value = get_http()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let total: i64 = resp
        .get("TOTAL")
        .and_then(|v| v.as_str())
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let raw_list = resp
        .get("abslist")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list
        .iter()
        .map(|item| PlaylistItem {
            id: item
                .get("playlistid")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            name: decode_html(item.get("name").and_then(|v| v.as_str()).unwrap_or("")),
            img: item
                .get("img")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: "kw".into(),
            desc: String::new(),
            play_count: item
                .get("playCount")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            author: String::new(),
            total: serde_json::Value::Null,
        })
        .collect();

    Ok(serde_json::to_value(PlaylistResult {
        list,
        all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64,
        total,
        source: "kw".into(),
    })
    .unwrap())
}

// --- Internal helpers ---

fn parse_playlists(
    resp: &serde_json::Value,
    source: &str,
    limit: u64,
) -> Result<serde_json::Value, String> {
    let data = resp.get("data").cloned().unwrap_or(serde_json::json!({}));
    let total = data.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    let raw_list = data
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list
        .iter()
        .map(|item| {
            let digest = item
                .get("digest")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let raw_id = item
                .get("id")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();

            PlaylistItem {
                id: serde_json::json!(format!("digest-{}__{}", digest, raw_id)),
                name: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                img: item
                    .get("img")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                source: source.to_string(),
                desc: item
                    .get("desc")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                play_count: item
                    .get("listencnt")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                author: item
                    .get("uname")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                total: item
                    .get("total")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            }
        })
        .collect();

    Ok(serde_json::to_value(PlaylistResult {
        list,
        all_page: (total as f64 / limit as f64).ceil() as i64,
        limit: limit as i64,
        total,
        source: source.to_string(),
    })
    .unwrap())
}

fn parse_playlists_mobile(
    resp: &serde_json::Value,
    source: &str,
    limit: u64,
) -> Result<serde_json::Value, String> {
    let raw_list = resp
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let list: Vec<PlaylistItem> = raw_list
        .iter()
        .filter_map(|item| {
            item.get("label")?;
            let inner_list = item
                .get("list")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            Some(inner_list)
        })
        .flatten()
        .map(|item| {
            let digest = item
                .get("digest")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            let raw_id = item
                .get("id")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            PlaylistItem {
                id: serde_json::json!(format!("digest-{}__{}", digest, raw_id)),
                name: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                img: item
                    .get("img")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                source: source.to_string(),
                desc: item
                    .get("desc")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                play_count: item
                    .get("listencnt")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                author: item
                    .get("uname")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                total: item
                    .get("total")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            }
        })
        .collect();
    let count = list.len() as i64;

    Ok(serde_json::to_value(PlaylistResult {
        list,
        all_page: (count as f64 / limit as f64).ceil() as i64,
        limit: limit as i64,
        total: count,
        source: source.to_string(),
    })
    .unwrap())
}
