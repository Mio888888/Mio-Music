//! The bounded, editable subset of the existing desktop storage preferences.
use crate::{download, storage::Preferences, tags::TagOptions};

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub auto_cache: bool,
    pub cache_limit: u64,
    pub filename_template: String,
    pub basic_info: bool,
    pub cover: bool,
    pub lyrics: bool,
    pub download_lyrics: bool,
}
fn valid_template(value: &str) -> bool {
    value.len() <= 1024 && value.chars().count() <= 256 && !value.chars().any(char::is_control)
}
impl Config {
    pub fn from_preferences(p: &Preferences) -> Self {
        Self {
            auto_cache: p.app.auto_cache_music,
            cache_limit: p.app.cache_size_limit,
            filename_template: if valid_template(&p.app.filename_template) {
                p.app.filename_template.clone()
            } else {
                download::DEFAULT_TEMPLATE.into()
            },
            basic_info: p.tag_write_options.basic_info,
            cover: p.tag_write_options.cover,
            lyrics: p.tag_write_options.lyrics,
            download_lyrics: p.tag_write_options.download_lyrics,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_template(&self.filename_template) {
            return Err("invalid_input: download filename template".into());
        }
        Ok(())
    }
    pub fn apply_tags(&self, tags: &mut TagOptions) {
        tags.basic_info = self.basic_info;
        tags.cover = self.cover;
        tags.lyrics = self.lyrics;
        tags.download_lyrics = self.download_lyrics;
        // Keep the existing stored format; the tag writer consumes provider text.
    }
}

/// A bounded example using the same formatter as the download manager.
pub fn filename_preview(template: &str, english: bool) -> Result<String, String> {
    if !valid_template(template) {
        return Err("invalid_input: download filename template".into());
    }
    Ok(download::sanitize_stem(&download::format_template(
        template,
        if english { "Song" } else { "歌曲" },
        if english { "Artist" } else { "歌手" },
        if english { "Album" } else { "专辑" },
        if english { "Source" } else { "平台" },
        "flac",
        "2026-01-01",
    )))
}
