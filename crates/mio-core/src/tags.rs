//! Tags for downloaded files, ported from the original's `scanner::write_download_tags`.
//! Existing values are never replaced: a field is filled only when the file has none.
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    tag::{Accessor, ItemKey, ItemValue, Tag, TagItem, TagType},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `tagWriteOptions` of the original settings (same keys and defaults).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TagOptions {
    pub basic_info: bool,
    pub cover: bool,
    pub lyrics: bool,
    /// Also keep the lyrics as a `.lrc` file next to the audio.
    pub download_lyrics: bool,
    /// Kept for the settings file's sake; plain lyrics text is written whatever it says.
    pub lyric_format: String,
}
impl Default for TagOptions {
    fn default() -> Self {
        Self {
            basic_info: true,
            cover: true,
            lyrics: true,
            download_lyrics: false,
            lyric_format: "word-by-word".into(),
        }
    }
}
impl TagOptions {
    pub fn wants_cover(&self) -> bool {
        self.cover
    }
    pub fn any(&self) -> bool {
        self.basic_info || self.cover || self.lyrics || self.download_lyrics
    }
}

fn default_tag_type(path: &Path) -> TagType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("mp3") => TagType::Id3v2,
        Some("m4a" | "mp4" | "aac" | "alac") => TagType::Mp4Ilst,
        Some("wav") => TagType::RiffInfo,
        _ => TagType::VorbisComments,
    }
}

fn mime_of(data: &[u8]) -> MimeType {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        MimeType::Png
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        MimeType::Gif
    } else if data.starts_with(b"BM") {
        MimeType::Bmp
    } else {
        MimeType::Jpeg
    }
}

fn empty(value: Option<impl AsRef<str>>) -> bool {
    value.map(|v| v.as_ref().trim().is_empty()).unwrap_or(true)
}

/// Writes title/artist/album, cover and lyrics into `path` according to `options`.
/// `lyrics` is plain text (usually LRC) when the song has any.
pub fn write_download_tags(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    options: &TagOptions,
    cover: Option<&[u8]>,
    lyrics: Option<&str>,
) -> Result<(), String> {
    // The extension decides first, as in the original. Kuwo serves MP4 audio under an `.aac`
    // name, which that reads as raw ADTS and rejects; the content tells the real format.
    let mut file = lofty::read_from_path(path)
        .or_else(|_| {
            lofty::probe::Probe::new(std::io::BufReader::new(std::fs::File::open(path)?))
                .guess_file_type()?
                .read()
        })
        .map_err(|e| e.to_string())?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(default_tag_type(path)));
    }
    let tag = file
        .primary_tag_mut()
        .ok_or_else(|| "No tag found".to_string())?;

    if options.basic_info {
        if !title.is_empty() && empty(tag.title()) {
            tag.set_title(title.to_owned());
        }
        if !artist.is_empty() && empty(tag.artist()) {
            tag.set_artist(artist.to_owned());
        }
        if !album.is_empty() && empty(tag.album()) {
            tag.set_album(album.to_owned());
        }
    }
    if options.cover && tag.pictures().is_empty() {
        if let Some(data) = cover.filter(|d| !d.is_empty()) {
            tag.push_picture(Picture::new_unchecked(
                PictureType::CoverFront,
                Some(mime_of(data)),
                None,
                data.to_vec(),
            ));
        }
    }
    let lyrics = lyrics.map(str::trim).filter(|l| !l.is_empty());
    if options.lyrics && empty(tag.get_string(&ItemKey::Lyrics)) {
        if let Some(lrc) = lyrics {
            tag.push(TagItem::new(
                ItemKey::Lyrics,
                ItemValue::Text(lrc.to_owned()),
            ));
        }
    }
    file.save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())?;

    if options.download_lyrics {
        if let Some(lrc) = lyrics {
            let sidecar = path.with_extension("lrc");
            if !sidecar.exists() {
                std::fs::write(&sidecar, lrc).map_err(|e| format!("保存歌词文件失败: {e}"))?;
            }
        }
    }
    Ok(())
}
