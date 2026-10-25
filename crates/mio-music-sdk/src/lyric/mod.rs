//! Lyric retrieval through the enabled list plugin. A plugin answers with the generic
//! shape below, decoding its own service's formats itself.
//!
//! The result keeps the original shape: `lyric` is line-level LRC (`[mm:ss.xxx]text`), `tlyric`
//! a translation, `rlyric` a romanization, and `crlyric` the word-timed form, whose layout
//! `crlyricFormat` names (see `mio-core`'s lyric parser).
pub mod ttml;

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
    /// How `crlyric` lays out its word timing; empty means `wordTagBefore`.
    #[serde(default)]
    pub crlyric_format: String,
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
            crlyric_format: text("crlyricFormat"),
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

/// Pure decoding of a plugin response; no network access or source API fallback. A plugin
/// decodes its own service's formats and answers with the generic shape above; an answer that
/// still carries a source-specific `format` (an older plugin) is refused.
pub fn decode_payload(value: &Value, source: &str) -> Result<Lyric, String> {
    if value.to_string().len() > 4 * 1024 * 1024 {
        return Err("插件歌词数据过大".into());
    }
    if value.get("format").is_some_and(|format| !format.is_null()) {
        return Err("插件歌词编码不支持，请更新插件".into());
    }
    Ok(Lyric::from_value(value, source))
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
