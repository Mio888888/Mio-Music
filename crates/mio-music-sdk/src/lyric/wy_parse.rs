//! NetEase lyric post-processing, extracted from the original
//! `music_sdk/sources/wy/playback.rs` without behavioural changes.
#![allow(clippy::all)]

pub(super) fn fix_time_label(lrc: &str, tlrc: &str, romalrc: &str) -> (String, String, String) {
    if lrc.is_empty() {
        return (String::new(), tlrc.to_string(), romalrc.to_string());
    }
    let re = regex::Regex::new(r"\[(\d{2}:\d{2}):(\d{2})\]").unwrap();
    let new_lrc = re.replace_all(lrc, "[$1.$2]").to_string();
    let new_tlrc = re.replace_all(tlrc, "[$1.$2]").to_string();
    let has_changes = new_lrc != lrc || new_tlrc != tlrc;
    let result_lrc = new_lrc;
    let result_tlrc = new_tlrc;
    let mut result_romalrc = romalrc.to_string();
    if has_changes && !romalrc.is_empty() {
        let re3 = regex::Regex::new(r"\[(\d{2}:\d{2}):(\d{2,3})\]").unwrap();
        let re_trail = regex::Regex::new(r"\[(\d{2}:\d{2}\.\d{2})0\]").unwrap();
        result_romalrc = re_trail
            .replace_all(&re3.replace_all(&result_romalrc, "[$1.$2]"), "[$1]")
            .to_string();
    }
    (result_lrc, result_tlrc, result_romalrc)
}

pub(super) fn parse_lrc_with_header(str: &str) -> String {
    let str = str.replace('\r', "").trim().to_string();
    if str.is_empty() {
        return String::new();
    }

    let info_re = regex::Regex::new(r#"^\{"#).unwrap();
    let lines: Vec<String> = str
        .lines()
        .map(|line| {
            let line = line.trim();
            if info_re.is_match(line) {
                // JSON line with word-by-word timing info
                if let Ok(info) = serde_json::from_str::<serde_json::Value>(line) {
                    let t = info.get("t").and_then(|v| v.as_i64()).unwrap_or(0);
                    let ms = t % 1000;
                    let total_secs = t / 1000;
                    let m = total_secs / 60;
                    let s = total_secs % 60;
                    let time_str = format!("[{:02}:{:02}.{:03}]", m, s, ms);
                    if let Some(c) = info.get("c").and_then(|v| v.as_array()) {
                        let text: String = c
                            .iter()
                            .filter_map(|w| w.get("tx").and_then(|v| v.as_str()))
                            .collect();
                        return format!("{}{}", time_str, text);
                    }
                }
                String::new()
            } else {
                line.to_string()
            }
        })
        .filter(|l| !l.is_empty())
        .collect();

    lines.join("\n")
}

pub(super) fn parse_yrc_lyrics(
    yrc: &str,
    ytlrc: &str,
    yromalrc: &str,
    lrc: &str,
    _tlrc: &str,
    _rlrc: &str,
) -> serde_json::Value {
    let yrc_clean = yrc.replace('\r', "").trim().to_string();
    let line_time_re = regex::Regex::new(r"^\[(\d+),\d+\]").unwrap();
    let word_time_re = regex::Regex::new(r"\(\d+,\d+,\d+\)").unwrap();

    let mut lrc_lines = Vec::new();
    let mut crlyric_lines = Vec::new();

    for line in yrc_clean.lines() {
        let line = line.trim();
        if let Some(caps) = line_time_re.captures(line) {
            let start_ms: i64 = caps[1].parse().unwrap_or(0);
            let ms = start_ms % 1000;
            let total_secs = start_ms / 1000;
            let m = total_secs / 60;
            let s = total_secs % 60;
            let time_str = format!("[{:02}:{:02}.{:03}]", m, s, ms);

            let words = line_time_re.replace(line, "");
            // Plain text (remove word timing tags)
            let plain = word_time_re.replace_all(&words, "");
            lrc_lines.push(format!("{}{}", time_str, plain));
            // Keep original word-by-word for crlyric
            let full_match = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            crlyric_lines.push(format!("{}{}", full_match, words));
        }
    }

    let mut tlyric = String::new();
    if !ytlrc.is_empty() {
        let ytlrc_lines = parse_lrc_with_header(ytlrc);
        tlyric = fix_time_tags(&ytlrc_lines, &lrc_lines.join("\n"));
    }

    let mut rlyric = String::new();
    if !yromalrc.is_empty() {
        let yroma_lines = parse_lrc_with_header(yromalrc);
        rlyric = fix_time_tags(&yroma_lines, &lrc_lines.join("\n"));
    }

    // Build header lines from standard lrc
    let time_rxp = regex::Regex::new(r"^\[[\d:.]+\]").unwrap();
    let header_lines: Vec<&str> = lrc.lines().filter(|l| time_rxp.is_match(l)).collect();
    let lyric = format!("{}\n{}", header_lines.join("\n"), lrc_lines.join("\n"));

    serde_json::json!({
        "lyric": lyric, "tlyric": tlyric, "rlyric": rlyric,
        "crlyric": crlyric_lines.join("\n"), "source": "wy"
    })
}

fn get_interval_ms(interval: &str) -> i64 {
    if interval.is_empty() {
        return 0;
    }
    let interval = if !interval.contains('.') {
        format!("{}.0", interval)
    } else {
        interval.to_string()
    };
    let parts: Vec<&str> = interval.split([':', '.']).collect();
    let (m, s, ms) = match parts.len() {
        3 => (
            parts[0].parse::<i64>().unwrap_or(0),
            parts[1].parse::<i64>().unwrap_or(0),
            parts[2].parse::<i64>().unwrap_or(0),
        ),
        _ => return 0,
    };
    m * 3600000 + s * 1000 + ms
}

fn fix_time_tags(sub_lrc: &str, main_lrc: &str) -> String {
    let time_rxp = regex::Regex::new(r"^\[([\d:.]+)\]").unwrap();
    let sub_lines: Vec<&str> = sub_lrc.lines().collect();
    let main_lines: Vec<&str> = main_lrc.lines().collect();
    let mut result = Vec::new();
    let mut main_idx = 0;

    for sub_line in sub_lines {
        if let Some(sub_caps) = time_rxp.captures(sub_line) {
            let words = time_rxp.replace(sub_line, "");
            if words.trim().is_empty() {
                continue;
            }
            let t1 = get_interval_ms(&sub_caps[1]);

            while main_idx < main_lines.len() {
                if let Some(main_caps) = time_rxp.captures(main_lines[main_idx]) {
                    let t2 = get_interval_ms(&main_caps[1]);
                    if (t1 - t2).abs() < 100 {
                        result.push(time_rxp.replace(sub_line, &main_caps[0]).to_string());
                        main_idx += 1;
                        break;
                    }
                }
                main_idx += 1;
            }
        }
    }
    result.join("\n")
}
