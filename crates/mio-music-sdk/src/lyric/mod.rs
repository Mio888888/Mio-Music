//! Lyric retrieval per source, ported from each original `getLyric`. Decoding and parsing are
//! the original code unchanged (`*_parse.rs`, `*_qrc.rs`, ...); only the requests are rewritten
//! as bounded blocking calls.
//!
//! The result keeps the original shape: `lyric` is line-level LRC (`[mm:ss.xxx]text`), `tlyric`
//! a translation, `rlyric` a romanization, and `crlyric` the word-timed form of the source.
mod kg_krc;
mod kw_parse;
mod mg_mrc;
pub mod ttml;
mod tx_qrc;
mod wy_parse;

use crate::MusicItem;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyric {
    pub lyric: String,
    pub tlyric: String,
    pub rlyric: String,
    pub crlyric: String,
    pub source: String,
}

impl Lyric {
    fn from_value(value: &Value, source: &str) -> Self {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        Self {
            lyric: text("lyric"),
            tlyric: text("tlyric"),
            rlyric: text("rlyric"),
            crlyric: text("crlyric"),
            source: source.to_string(),
        }
    }

    /// True when there is no line-level text to show.
    pub fn is_empty(&self) -> bool {
        self.lyric.trim().is_empty()
    }
}

/// Fetches only from the enabled external list plugin. The host does not resolve source URLs.
pub(crate) fn request(item: &MusicItem) -> Result<Value, String> {
    let provider =
        crate::provider_with(&item.source, |c| c.lyrics).ok_or("该来源的外部插件未提供歌词")?;
    let song = serde_json::to_value(item).map_err(|_| "无效歌曲信息")?;
    let payload = provider.lyrics(&song)?;
    if payload.to_string().len() > 4 * 1024 * 1024 {
        return Err("插件歌词数据过大".into());
    }
    Ok(payload)
}

/// Pure decoding of a plugin response; no network access or source API fallback.
pub fn decode_payload(value: &Value, source: &str) -> Result<Lyric, String> {
    use base64::Engine;
    if value.to_string().len() > 4 * 1024 * 1024 {
        return Err("插件歌词数据过大".into());
    }
    let data = &value["data"];
    let text = |key: &str| data[key].as_str().unwrap_or("");
    match value["format"].as_str() {
        Some("kw") => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text("bytes"))
                .map_err(|_| "歌词字节格式无效")?;
            let decoded =
                kw_parse::decode_lyric(&bytes, data["isLyricx"].as_bool().unwrap_or(false))?;
            Ok(Lyric::from_value(
                &kw_parse::build_lyric_result(&decoded)?,
                source,
            ))
        }
        Some("wy") => {
            let field = |key: &str| data[key]["lyric"].as_str().unwrap_or("");
            let (lrc, tlrc, rlrc) =
                wy_parse::fix_time_label(field("lrc"), field("tlyric"), field("romalrc"));
            if let Some(yrc) = data["yrc"]["lyric"].as_str() {
                return Ok(Lyric::from_value(
                    &wy_parse::parse_yrc_lyrics(
                        yrc,
                        field("ytlrc"),
                        field("yromalrc"),
                        &lrc,
                        &tlrc,
                        &rlrc,
                    ),
                    source,
                ));
            }
            let header = |s: &str| {
                if s.is_empty() {
                    String::new()
                } else {
                    wy_parse::parse_lrc_with_header(s)
                }
            };
            Ok(Lyric {
                lyric: header(&lrc),
                tlyric: header(&tlrc),
                rlyric: header(&rlrc),
                source: source.into(),
                ..Default::default()
            })
        }
        Some("tx") => {
            let mut lyric = Lyric::from_value(
                &tx_qrc::parse_qrc_lyrics(
                    &tx_qrc::qrc_decrypt(text("lyric"))?,
                    &tx_qrc::qrc_decrypt(text("trans"))?,
                    &tx_qrc::qrc_decrypt(text("roma"))?,
                ),
                source,
            );
            for text in [&mut lyric.tlyric, &mut lyric.rlyric] {
                if !text.lines().any(|line| {
                    let body = line.split_once(']').map_or(line, |(_, rest)| rest).trim();
                    !body.is_empty() && body != "//"
                }) {
                    text.clear();
                }
            }
            Ok(lyric)
        }
        Some("kg") => {
            if text("fmt") == "krc" {
                Ok(Lyric::from_value(
                    &kg_krc::decode_krc(text("content"))?,
                    source,
                ))
            } else {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(text("content"))
                    .map_err(|_| "歌词字节格式无效")?;
                Ok(Lyric {
                    lyric: String::from_utf8_lossy(&bytes).into_owned(),
                    source: source.into(),
                    ..Default::default()
                })
            }
        }
        Some("mg") => {
            let mut lyric = Lyric::from_value(
                &mg_mrc::parse_mrc_lyrics(&mg_mrc::decrypt_mrc(text("mrc"))),
                source,
            );
            lyric.tlyric = text("tlyric").into();
            Ok(lyric)
        }
        Some(_) => Err("插件歌词编码不支持".into()),
        None => Ok(Lyric::from_value(value, source)),
    }
}

pub fn get(item: &MusicItem) -> Result<Lyric, String> {
    let payload = request(item)?;
    let mut lyric = decode_payload(&payload, &item.source)?;
    if lyric.is_empty()
        && let Some(text) = payload["ttml"].as_str()
        && let Ok(lines) = ttml::parse(text)
    {
        lyric.lyric = lines
            .iter()
            .filter_map(|line| {
                let text = line
                    .words
                    .iter()
                    .map(|word| word.text.as_str())
                    .collect::<String>();
                if text.trim().is_empty() {
                    return None;
                }
                let start = line.start;
                Some(format!(
                    "[{:02}:{:02}.{:03}]{}",
                    start / 60000,
                    (start / 1000) % 60,
                    start % 1000,
                    text
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    Ok(lyric)
}
