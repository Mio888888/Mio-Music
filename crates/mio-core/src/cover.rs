//! Owned, bounded cover requests for hosts that do not use a GPUI HTTP client.
use image::ImageDecoder;
use std::{io::Cursor, path::PathBuf};

pub enum Source {
    Online {
        url: Option<String>,
        item: Option<mio_music_sdk::MusicItem>,
    },
    Local {
        path: PathBuf,
        cache_key: String,
    },
}

impl Source {
    /// Runs on a background worker, outside application state locks. Returns a static PNG
    /// thumbnail so rendering cannot decode an unbounded original or animated image.
    pub fn load(self) -> Result<Option<Vec<u8>>, String> {
        self.load_sized(1024)
    }
    /// Small static projection for visible library rows.
    pub fn load_thumbnail(self) -> Result<Option<Vec<u8>>, String> {
        self.load_sized(256)
    }
    /// Palette and native artwork derive from one bounded original decode.
    pub fn load_media_theme(
        self,
    ) -> Result<Option<(String, crate::cover_palette::CoverPalette)>, String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        Ok(self
            .load_projection(256, true)?
            .and_then(|(bytes, palette)| {
                if bytes.len() > 512 * 1024 {
                    return None;
                }
                Some((STANDARD.encode(bytes), palette?))
            }))
    }
    fn load_sized(self, size: u32) -> Result<Option<Vec<u8>>, String> {
        Ok(self.load_projection(size, false)?.map(|(bytes, _)| bytes))
    }
    fn load_projection(
        self,
        size: u32,
        with_palette: bool,
    ) -> Result<Option<(Vec<u8>, Option<crate::cover_palette::CoverPalette>)>, String> {
        let cache_key;
        let bytes = match self {
            Self::Online { url, item } => {
                let url = url.filter(|url| !url.is_empty()).or_else(|| {
                    mio_music_sdk::get_cover(&item?)
                        .ok()
                        .filter(|url| !url.is_empty())
                });
                let Some(url) = url else {
                    return Ok(None);
                };
                cache_key = url.clone();
                crate::playback::online_cover(url)
            }
            Self::Local {
                path,
                cache_key: key,
            } => {
                cache_key = key;
                let bytes = crate::image_cache::read(&cache_key)
                    .or_else(|| crate::local_music::embedded_cover(&path));
                if let Some(bytes) = &bytes {
                    crate::image_cache::write(&cache_key, bytes);
                }
                bytes
            }
        };
        let result = bytes
            .map(|bytes| thumbnail_projection(bytes, size, with_palette))
            .transpose();
        if result.is_err() {
            // Retry must fetch a corrected remote image instead of decoding the
            // same corrupt cache entry forever.
            crate::image_cache::remove(&cache_key);
        }
        result
    }
}

#[derive(Clone, Copy)]
pub enum PlaylistCoverError {
    Type,
    Size,
    Process,
}

/// Original image data stays compatible with desktop; UI receives a bounded static preview.
pub struct PreparedPlaylistCover {
    pub data_url: String,
    pub preview: Vec<u8>,
}
pub fn prepare_playlist_cover(
    bytes: Vec<u8>,
    extension: &str,
) -> Result<PreparedPlaylistCover, PlaylistCoverError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let mime = match extension {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err(PlaylistCoverError::Type),
    };
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 {
        return Err(PlaylistCoverError::Size);
    }
    let preview = thumbnail(bytes.clone()).map_err(|_| PlaylistCoverError::Process)?;
    Ok(PreparedPlaylistCover {
        data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        preview,
    })
}

/// Fixed small projection for lazily displayed playlist grids.
pub fn saved_playlist_thumbnail(url: &str) -> Result<Vec<u8>, String> {
    saved_data_thumbnail_sized(url, 256)
}
fn saved_data_thumbnail_sized(url: &str, size: u32) -> Result<Vec<u8>, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let (header, encoded) = url.split_once(',').ok_or("Invalid cover data")?;
    if ![
        "data:image/png;base64",
        "data:image/jpeg;base64",
        "data:image/gif;base64",
        "data:image/webp;base64",
    ]
    .contains(&header)
        || encoded.len() > (16 * 1024 * 1024 + 2) / 3 * 4
    {
        return Err("Unsupported cover image".into());
    }
    thumbnail_sized(
        STANDARD.decode(encoded).map_err(|_| "Invalid cover data")?,
        size,
    )
}

fn thumbnail(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    thumbnail_sized(bytes, 1024)
}
fn thumbnail_sized(bytes: Vec<u8>, size: u32) -> Result<Vec<u8>, String> {
    thumbnail_projection(bytes, size, false).map(|(bytes, _)| bytes)
}
fn thumbnail_projection(
    bytes: Vec<u8>,
    size: u32,
    with_palette: bool,
) -> Result<(Vec<u8>, Option<crate::cover_palette::CoverPalette>), String> {
    if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
        return Err("Cover exceeds image limits".into());
    }
    // image-webp allocates metadata and decodes VP8 frames before checking their
    // canvas dimensions. Validate every chunk before entering that decoder.
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        validate_webp(&bytes).map_err(|_| "Cover exceeds image limits".to_owned())?;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Unsupported cover image".to_owned())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "Cover exceeds image limits".to_owned())?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
        return Err("Cover exceeds image limits".into());
    }
    let orientation = decoder
        .orientation()
        .map_err(|_| "Invalid cover orientation".to_owned())?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|_| "Unable to decode cover".to_owned())?;
    image.apply_orientation(orientation);
    let palette = with_palette.then(|| crate::cover_palette::cover_palette(&image));
    let mut output = Cursor::new(Vec::new());
    image
        .thumbnail(size, size)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "Unable to prepare cover".to_owned())?;
    Ok((output.into_inner(), palette))
}

fn bounded_dimensions(width: u32, height: u32) -> Result<(u32, u32), ()> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16 * 1024 * 1024
    {
        return Err(());
    }
    Ok((width, height))
}

fn le24(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

fn webp_chunks(
    mut bytes: &[u8],
    mut visit: impl FnMut(&[u8], &[u8]) -> Result<(), ()>,
) -> Result<(), ()> {
    while !bytes.is_empty() {
        let header = bytes.get(..8).ok_or(())?;
        let size = u32::from_le_bytes(header[4..8].try_into().map_err(|_| ())?) as usize;
        let end = 8usize.checked_add(size).ok_or(())?;
        let padded_end = end.checked_add(size & 1).ok_or(())?;
        let payload = bytes.get(8..end).ok_or(())?;
        visit(&header[..4], payload)?;
        bytes = bytes.get(padded_end..).ok_or(())?;
    }
    Ok(())
}

fn validate_webp(bytes: &[u8]) -> Result<(), ()> {
    let declared =
        u32::from_le_bytes(bytes.get(4..8).ok_or(())?.try_into().map_err(|_| ())?) as usize;
    let end = declared.checked_add(8).ok_or(())?;
    // The decoder may scan beyond RIFF's declared end when VP8X is present.
    // Reject trailing data so no unvalidated chunk can reach it.
    if end != bytes.len() {
        return Err(());
    }
    let chunks = bytes.get(12..end).ok_or(())?;
    let mut canvas = None;
    webp_chunks(chunks, |kind, payload| {
        if kind == b"VP8X" {
            if payload.len() != 10 || canvas.is_some() {
                return Err(());
            }
            canvas = Some(bounded_dimensions(
                le24(&payload[4..7]) + 1,
                le24(&payload[7..10]) + 1,
            )?);
        }
        Ok(())
    })?;
    webp_chunks(chunks, |kind, payload| {
        if kind == b"ANMF" {
            let header = payload.get(..16).ok_or(())?;
            let frame = bounded_dimensions(le24(&header[6..9]) + 1, le24(&header[9..12]) + 1)?;
            let canvas = canvas.ok_or(())?;
            if le24(&header[..3]) * 2 + frame.0 > canvas.0
                || le24(&header[3..6]) * 2 + frame.1 > canvas.1
            {
                return Err(());
            }
            webp_chunks(&payload[16..], |kind, data| {
                if kind == b"ANMF" || kind == b"VP8X" {
                    return Err(());
                }
                validate_webp_frame(kind, data, Some(frame))
            })
        } else {
            validate_webp_frame(kind, payload, canvas)
        }
    })
}

fn validate_webp_frame(kind: &[u8], bytes: &[u8], expected: Option<(u32, u32)>) -> Result<(), ()> {
    let dimensions = if kind == b"VP8 " {
        let header = bytes.get(..10).ok_or(())?;
        if header[0] & 1 != 0 || &header[3..6] != b"\x9d\x01\x2a" {
            return Err(());
        }
        bounded_dimensions(
            u32::from(u16::from_le_bytes([header[6], header[7]]) & 0x3fff),
            u32::from(u16::from_le_bytes([header[8], header[9]]) & 0x3fff),
        )?
    } else if kind == b"VP8L" {
        let header = bytes.get(..5).ok_or(())?;
        if header[0] != 0x2f {
            return Err(());
        }
        let packed = u32::from_le_bytes(header[1..5].try_into().map_err(|_| ())?);
        bounded_dimensions((packed & 0x3fff) + 1, ((packed >> 14) & 0x3fff) + 1)?
    } else {
        return Ok(());
    };
    if expected.is_some_and(|expected| expected != dimensions) {
        return Err(());
    }
    Ok(())
}
