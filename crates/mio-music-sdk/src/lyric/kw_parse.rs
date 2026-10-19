//! Kuwo lyric decoding and LRC post-processing, extracted from the original
//! `music_sdk/sources/kw/playback.rs` without behavioural changes.
#![allow(clippy::all)]
use base64::Engine;
use encoding_rs::GB18030;
use flate2::read::ZlibDecoder;
use std::io::Read;

const XOR_KEY: &[u8] = b"yeelion";

pub(super) fn decode_lyric(data: &[u8], is_lyricx: bool) -> Result<String, String> {
    if data.len() < 10 || std::str::from_utf8(&data[..10]).unwrap_or("") != "tp=content" {
        return Err("Invalid lyric response: missing tp=content header".into());
    }

    let sep = b"\r\n\r\n";
    let body_start = data
        .windows(sep.len())
        .position(|w| w == sep)
        .map(|pos| pos + sep.len())
        .ok_or("Invalid lyric response: missing header/body separator")?;

    let decoder = ZlibDecoder::new(&data[body_start..]);
    let mut inflated = Vec::new();
    decoder
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut inflated)
        .map_err(|e| format!("Zlib decompress error: {}", e))?;
    if inflated.len() > 8 * 1024 * 1024 {
        return Err("歌词解压结果过大".into());
    }

    if !is_lyricx {
        let (text, _, _) = GB18030.decode(&inflated);
        return Ok(text.into_owned());
    }

    let inflated_str = std::str::from_utf8(&inflated)
        .map_err(|e| format!("Inflated data is not valid UTF-8: {}", e))?;

    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(inflated_str.trim())
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    let mut decrypted = encrypted;
    for (i, b) in decrypted.iter_mut().enumerate() {
        *b ^= XOR_KEY[i % XOR_KEY.len()];
    }

    let (text, _, _) = GB18030.decode(&decrypted);
    Ok(text.into_owned())
}

struct LrcLine {
    time: String,
    text: String,
}

fn sort_lrc_arr(arr: Vec<LrcLine>) -> Result<(Vec<LrcLine>, Vec<LrcLine>), String> {
    let mut time_set = std::collections::HashSet::new();
    let mut lrc = Vec::new();
    let mut lrc_t = Vec::new();
    let mut is_lyricx = false;

    for item in arr {
        if time_set.contains(&item.time) {
            if lrc.len() < 2 {
                continue;
            }
            let mut t_item: LrcLine = lrc.pop().unwrap();
            t_item.time = lrc.last().unwrap().time.clone();
            lrc_t.push(t_item);
            lrc.push(item);
        } else {
            lrc.push(item);
        }
        time_set.insert(lrc.last().map(|l| l.time.clone()).unwrap_or_default());
        if !is_lyricx {
            let re = regex::Regex::new(r"<-?\d+,-?\d+>").unwrap();
            if let Some(last) = lrc.last() {
                if re.is_match(&last.text) {
                    is_lyricx = true;
                }
            }
        }
    }

    if !is_lyricx
        && !lrc_t.is_empty()
        && lrc_t.len() as f64 > lrc.len() as f64 * 0.3
        && lrc.len() > lrc_t.len() + 6
    {
        return Err("Translation count suspicious".into());
    }

    Ok((lrc, lrc_t))
}

fn parse_lrc(lrc_text: &str) -> Result<(String, String), String> {
    let time_re = regex::Regex::new(r"^\[([\d:.]*)\]").unwrap();
    let tag_re = regex::Regex::new(r"\[(ver|ti|ar|al|offset|by|kuwo):").unwrap();
    let mut tags = Vec::new();
    let mut lrc_arr = Vec::new();

    for line in lrc_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = time_re.captures(line) {
            let time = caps.get(1).unwrap().as_str();
            let text = time_re.replace(line, "").trim().to_string();
            let time = if let Some(dot_pos) = time.find('.') {
                let frac = &time[dot_pos + 1..];
                if frac.len() == 2 {
                    format!("{}{}0", &time[..dot_pos + 1], frac)
                } else {
                    time.to_string()
                }
            } else {
                time.to_string()
            };
            lrc_arr.push(LrcLine { time, text });
        } else if tag_re.is_match(line) {
            tags.push(line.to_string());
        }
    }

    let (lrc, lrc_t) = sort_lrc_arr(lrc_arr)?;
    let tags_str = tags.join("\n");
    let lyric = format_lyric(&tags_str, &lrc);
    let tlyric = if lrc_t.is_empty() {
        String::new()
    } else {
        format_lyric(&tags_str, &lrc_t)
    };
    Ok((lyric, tlyric))
}

fn format_lyric(tags: &str, lines: &[LrcLine]) -> String {
    let mut result = if tags.is_empty() {
        String::new()
    } else {
        format!("{}\n", tags)
    };
    if lines.is_empty() {
        result.push_str("暂无歌词");
    } else {
        for line in lines {
            result.push_str(&format!("[{}]{}\n", line.time, line.text));
        }
    }
    result
}

fn parse_crlyric(lyric: &str) -> String {
    let word_line_re = regex::Regex::new(r"^(\[\d{1,2}:[\d.]+\])\s*(\S+(?:\s+\S+)*)?\s*").unwrap();
    let word_time_re = regex::Regex::new(r"<(-?\d+),(-?\d+)(?:,-?\d+)?>").unwrap();
    let kuwo_tag_re = regex::Regex::new(r"\[kuwo:\s*(\S+)\]").unwrap();
    let tag_line_re =
        regex::Regex::new(r"\[(ver|ti|ar|al|offset|by):\s*(\S+(?:\s+\S+)*)\s*\]").unwrap();

    let mut offset: i64 = 1;
    let mut offset2: i64 = 1;
    let mut lines: Vec<String> = Vec::new();
    let mut cr_tags: Vec<String> = Vec::new();

    for raw_line in lyric.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(caps) = kuwo_tag_re.captures(line) {
            let content = caps.get(1).unwrap().as_str();
            let content = if let Some(pos) = content.find("][") {
                &content[..pos]
            } else {
                content
            };
            let value = i64::from_str_radix(content, 8).unwrap_or(0);
            offset = value / 10;
            offset2 = value % 10;
            if offset == 0 || offset2 == 0 {
                return String::new();
            }
            continue;
        }

        if tag_line_re.is_match(line) {
            cr_tags.push(line.to_string());
            continue;
        }

        let Some(caps) = word_line_re.captures(line) else {
            continue;
        };
        let time_tag = caps.get(1).unwrap().as_str();
        let words = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();

        let word_time_data: Vec<(String, i64, i64)> = word_time_re
            .captures_iter(&words)
            .filter_map(|cap| {
                let full = cap.get(0)?.as_str().to_string();
                let a: i64 = cap.get(1)?.as_str().parse().ok()?;
                let b: i64 = cap.get(2)?.as_str().parse().ok()?;
                Some((full, a, b))
            })
            .collect();

        if word_time_data.is_empty() {
            continue;
        }

        let time_match = regex::Regex::new(r"\[(\d+):(\d+)\.(\d+)\]")
            .unwrap()
            .captures(time_tag);
        let line_start_ms: u64 = if let Some(tc) = time_match {
            let min: u64 = tc.get(1).unwrap().as_str().parse().unwrap_or(0);
            let sec: u64 = tc.get(2).unwrap().as_str().parse().unwrap_or(0);
            let ms: u64 = tc.get(3).unwrap().as_str().parse().unwrap_or(0);
            min * 60_000 + sec * 1_000 + ms
        } else {
            0
        };

        let mut result_words = words;
        for (original, a, b) in &word_time_data {
            let start = ((a + b).unsigned_abs() as u64)
                .checked_div(offset.unsigned_abs() as u64 * 2)
                .unwrap_or(0);
            let dur = ((a - b).unsigned_abs() as u64)
                .checked_div(offset2.unsigned_abs() as u64 * 2)
                .unwrap_or(0);
            let abs_start = line_start_ms + start;
            let replacement = format!("({},{},0)", abs_start, dur);
            result_words = result_words.replacen(original.as_str(), &replacement, 1);
        }

        lines.push(format!("[{},5000]{}", line_start_ms, result_words));
    }

    if lines.is_empty() {
        return String::new();
    }

    let mut result = lines.join("\n");
    if !cr_tags.is_empty() {
        result = format!("{}\n{}", cr_tags.join("\n"), result);
    }
    result
}

pub(super) fn build_lyric_result(lrc_text: &str) -> Result<serde_json::Value, String> {
    let (lyric, tlyric) = parse_lrc(lrc_text).unwrap_or((String::new(), String::new()));

    let word_time_re = regex::Regex::new(r"<(-?\d+),(-?\d+)(?:,-?\d+)?>").unwrap();
    let clean_lyric = word_time_re.replace_all(&lyric, "").to_string();
    let clean_tlyric = word_time_re.replace_all(&tlyric, "").to_string();

    let exist_time_re = regex::Regex::new(r"\[\d{1,2}:.*\d{1,4}\]").unwrap();
    if !exist_time_re.is_match(&clean_lyric) {
        return Ok(serde_json::json!({
            "lyric": "", "tlyric": "", "crlyric": "", "source": "kw"
        }));
    }

    let crlyric = parse_crlyric(&lyric);

    Ok(serde_json::json!({
        "lyric": clean_lyric,
        "tlyric": clean_tlyric,
        "crlyric": crlyric,
        "source": "kw"
    }))
}
