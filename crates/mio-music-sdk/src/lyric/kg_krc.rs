//! KuGou KRC decoding and parsing, extracted from the original
//! `music_sdk/sources/kg/playback.rs` without behavioural changes.
#![allow(clippy::all)]

/// Decode KRC lyric format (XOR + zlib inflate)
pub(super) fn decode_krc(content: &str) -> Result<serde_json::Value, String> {
    use base64::Engine;
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let enc_key: [u8; 16] = [
        0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e,
        0x69,
    ];

    let raw = base64::engine::general_purpose::STANDARD
        .decode(content)
        .map_err(|e| format!("KRC base64 decode error: {}", e))?;

    if raw.len() < 4 {
        return Err("KRC data too short".into());
    }

    // Skip first 4 bytes (header), XOR with key
    let mut decoded = raw[4..].to_vec();
    for i in 0..decoded.len() {
        decoded[i] ^= enc_key[i % 16];
    }

    // Zlib inflate
    let decoder = ZlibDecoder::new(&decoded[..]);
    let mut decompressed = String::new();
    decoder
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut decompressed)
        .map_err(|e| format!("KRC zlib decompress error: {}", e))?;
    if decompressed.len() > 8 * 1024 * 1024 {
        return Err("歌词解压结果过大".into());
    }

    // Parse the KRC format into standard LRC
    parse_krc_lyric(&decompressed)
}

fn parse_krc_lyric(str: &str) -> Result<serde_json::Value, String> {
    let mut content = str.replace('\r', "");

    // Remove header line [id:$xxx]
    if let Some(pos) = content.find('\n') {
        if content[..pos].starts_with("[id:$") {
            content = content[pos + 1..].to_string();
        }
    }

    // Extract translated lyrics if present
    let mut tlyric = String::new();
    let mut crlyric_lines: Vec<String> = Vec::new();
    let mut lyric_lines: Vec<String> = Vec::new();

    // Check for [language:...] tag
    let lang_regex = regex::Regex::new(r"\[language:([\w=\\/+]+)\]").ok();
    let lang_content = if let Some(re) = &lang_regex {
        re.captures(&content).map(|caps| caps[1].to_string())
    } else {
        None
    };

    if let Some(lang_data) = &lang_content {
        content = content.replace(&format!("[language:{}]", lang_data), "");
        // The language data is base64-encoded JSON with translated/romanized lyrics
        use base64::Engine;
        if let Ok(json_bytes) = base64::engine::general_purpose::STANDARD.decode(lang_data) {
            if let Ok(json_str) = String::from_utf8(json_bytes) {
                if let Ok(lang_json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(items) = lang_json.get("content").and_then(|v| v.as_array()) {
                        for item in items {
                            let item_type = item.get("type").and_then(|v| v.as_i64()).unwrap_or(-1);
                            let lyric_content = item.get("lyricContent").and_then(|v| v.as_array());
                            if item_type == 1 {
                                // Translation lyrics
                                if let Some(lines) = lyric_content {
                                    tlyric = lines
                                        .iter()
                                        .filter_map(|line| line.as_array())
                                        .map(|words| {
                                            words
                                                .iter()
                                                .filter_map(|w| w.as_str())
                                                .collect::<Vec<_>>()
                                                .join("")
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Parse main lyric with [time,duration] word-by-word format.
    // Emit LYS for word-level karaoke rendering and standard LRC as a fallback.
    let word_regex = regex::Regex::new(r"\[(\d+),(\d+)\](.*)").unwrap();
    let word_detail_regex = regex::Regex::new(r"<(\d+),(\d+),\d+>([^<]*)").unwrap();

    for line in content.lines() {
        if let Some(caps) = word_regex.captures(line) {
            let line_start_ms: i64 = caps[1].parse().unwrap_or(0);
            let word_content = &caps[3];

            let time_tag = ms_to_lrc_time(line_start_ms);

            // Extract plain text (remove word timing tags)
            let plain_text = word_detail_regex.replace_all(word_content, "$3");
            lyric_lines.push(format!("[{}]{}", time_tag, plain_text));

            // Build crlyric with absolute timestamps
            let mut cr_words = String::new();
            for wc in word_detail_regex.captures_iter(word_content) {
                let offset: i64 = wc[1].parse().unwrap_or(0);
                let duration: i64 = wc[2].parse().unwrap_or(0);
                let word_text = &wc[3];
                let abs_time = line_start_ms + offset;
                cr_words.push_str(&format!("{}({},{})", word_text, abs_time, duration));
            }
            if !cr_words.is_empty() {
                crlyric_lines.push(format!("[0]{}", cr_words));
            }
        }
    }

    let lyric = lyric_lines.join("\n");
    let crlyric = crlyric_lines.join("\n");

    let final_lyric = if lyric.trim().is_empty() {
        let standard_lrc_regex = regex::Regex::new(r"^\[\d{1,2}:\d{2}(?:\.\d{1,3})?\].+").unwrap();
        let line_tag_regex = regex::Regex::new(r"^\[\d+,\d+\]").unwrap();
        let word_tag_regex = regex::Regex::new(r"<\d+,\d+,\d+>").unwrap();

        let standard_lrc_lines: Vec<String> = content
            .lines()
            .map(str::trim)
            .filter(|line| standard_lrc_regex.is_match(line))
            .map(|line| line.to_string())
            .collect();

        if !standard_lrc_lines.is_empty() {
            standard_lrc_lines.join("\n")
        } else {
            let fallback_lines: Vec<String> = content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter(|line| {
                    !line.starts_with("[ti:")
                        && !line.starts_with("[ar:")
                        && !line.starts_with("[al:")
                        && !line.starts_with("[by:")
                        && !line.starts_with("[offset:")
                })
                .map(|line| line_tag_regex.replace(line, "").to_string())
                .map(|line| word_tag_regex.replace_all(&line, "").to_string())
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty() && !line.starts_with('['))
                .collect();

            fallback_lines
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    let time_tag = ms_to_lrc_time((idx as i64) * 2000);
                    format!("[{}]{}", time_tag, line)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    } else {
        lyric
    };

    Ok(serde_json::json!({
        "lyric": final_lyric,
        "tlyric": tlyric,
        "crlyric": crlyric,
        "source": "kg"
    }))
}

fn ms_to_lrc_time(ms: i64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    let ms_rem = ms % 1000;
    format!("{:02}:{:02}.{:03}", mins, secs, ms_rem)
}
