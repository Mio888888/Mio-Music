use super::helpers::*;

pub async fn get_music_url(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);
    let quality = get_str(&args, "quality");

    let url = format!(
        "http://www.kuwo.cn/api/v1/www/music/playInfo?mid={}&type=music&httpsStatus=1",
        songmid
    );
    let resp: serde_json::Value = get_http()
        .get(&url)
        .header("Referer", "http://www.kuwo.cn/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let play_url = resp
        .get("data")
        .and_then(|d| d.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(serde_json::json!({ "url": play_url, "type": quality }))
}

pub async fn get_pic(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);

    let url = format!(
        "http://artistpicserver.kuwo.cn/pic.web?corp=kuwo&type=rid_pic&pictype=500&size=500&rid={}",
        songmid
    );
    let body = get_http()
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let pic_url = if body.starts_with("http") {
        body
    } else {
        String::new()
    };
    Ok(serde_json::json!({ "url": pic_url }))
}

pub async fn get_lyric(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let songmid = get_songmid(&args);

    let url = format!(
        "http://m.kuwo.cn/newh5/singles/songinfoandlrc?mid={}&type=music&httpsStatus=1",
        songmid
    );
    let resp: serde_json::Value = get_http()
        .get(&url)
        .header("Referer", "http://m.kuwo.cn/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let lrclist = resp
        .get("data")
        .and_then(|d| d.get("lrclist"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let lrc_text: String = lrclist
        .iter()
        .filter_map(|line| {
            let text = line.get("lineLyric").and_then(|v| v.as_str()).unwrap_or("");
            let t = line.get("time").and_then(|v| v.as_str()).unwrap_or("0");
            let ms: f64 = t.parse().ok()?;
            let total_ms = (ms * 1000.0) as u64;
            let min = total_ms / 60000;
            let sec = (total_ms % 60000) / 1000;
            let ms_rem = total_ms % 1000;
            Some(format!("[{:02}:{:02}.{:03}]{}", min, sec, ms_rem, text))
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(serde_json::json!({ "lrc": lrc_text, "source": "kw" }))
}
