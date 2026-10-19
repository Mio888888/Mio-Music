//! Local audio scanning and atomic tag editing.
use crate::song::Song;
use crate::storage::Storage;
use lofty::{file::AudioFile, file::TaggedFileExt, tag::Accessor};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

const INDEX: &str = "local-music.json";
const IMPORT_OWNER: &str = ".mio-import.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportOwner {
    version: u8,
    file: String,
}
const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac",
];

#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    pub song: Song,
    pub path: PathBuf,
    pub size: u64,
    mtime_ms: u128,
    #[serde(default)]
    pub year: u32,
}
impl Track {
    pub fn playlist_item(&self) -> serde_json::Value {
        let mut item: serde_json::Value = serde_json::from_str(&crate::library::saved_song_data(
            &crate::storage::SessionIdentity::local(),
            &self.song,
            None,
        ))
        .expect("serialized song");
        item["path"] = self.path.to_string_lossy().into_owned().into();
        item["url"] = reqwest::Url::from_file_path(&self.path)
            .ok()
            .map(|url| url.to_string())
            .unwrap_or_default()
            .into();
        item
    }
    pub fn cover_key(&self) -> String {
        format!("{}:{}:{}", self.song.id, self.size, self.mtime_ms)
    }
}

pub struct TagValues {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: u32,
}
pub struct EditResult {
    pub library: Library,
    pub track: Track,
    // Audio and index cannot share a transaction; allow rescan to recover after an index failure.
    pub index_error: Option<String>,
}

pub fn embedded_cover(path: &Path) -> Option<Vec<u8>> {
    let file = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    let picture = file
        .tags()
        .iter()
        .flat_map(|t| t.pictures())
        .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| file.tags().iter().flat_map(|t| t.pictures()).next())?;
    (picture.data().len() <= 8 * 1024 * 1024).then(|| picture.data().to_vec())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Library {
    version: u8,
    pub directories: Vec<PathBuf>,
    pub tracks: Vec<Track>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            directories: vec![],
            tracks: vec![],
        }
    }
}
#[derive(Default)]
pub struct ScanResult {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub errors: usize,
}

/// Owns new copies until the current library has durably indexed them.
pub struct PreparedImport {
    tracks: Vec<Track>,
    directories: Vec<tempfile::TempDir>,
}

impl PreparedImport {
    pub fn prepare(paths: &[PathBuf], data: &Path) -> Result<Self, String> {
        if paths.is_empty() || paths.len() > 100 {
            return Err("Select 1–100 audio files".into());
        }
        let named = paths
            .iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Invalid audio filename")?
                    .to_owned();
                Ok((path.clone(), name))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Self::prepare_named(&named, data, || false)
    }

    /// Mobile staging paths retain the document's display name in durable copies.
    pub fn prepare_named(
        paths: &[(PathBuf, String)],
        data: &Path,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, String> {
        const FILE_LIMIT: u64 = 256 * 1024 * 1024;
        const BATCH_LIMIT: u64 = 512 * 1024 * 1024;
        if paths.is_empty() || paths.len() > 100 || !data.is_absolute() {
            return Err("Select 1–100 audio files".into());
        }
        let root = data.join("local-audio");
        fs::create_dir_all(&root).map_err(|_| "Cannot create local audio directory")?;
        if !fs::symlink_metadata(&root)
            .map_err(|_| "Cannot inspect local audio directory")?
            .is_dir()
        {
            return Err("Local audio directory must not be a symlink".into());
        }
        let mut prepared = Self {
            tracks: vec![],
            directories: vec![],
        };
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for (path, name) in paths {
            if cancelled() {
                return Err("Audio import cancelled".into());
            }
            if !seen.insert(path.clone()) {
                continue;
            }
            if !path.is_absolute() || !audio(path) {
                return Err("Unsupported audio file extension".into());
            }
            let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot read selected audio")?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > FILE_LIMIT {
                return Err("Audio must be a regular file between 1 byte and 256 MiB".into());
            }
            if name.is_empty()
                || name.len() > 240
                || name.contains(['\0', '/', '\\'])
                || !audio(Path::new(name))
                || Path::new(name).file_name().and_then(|name| name.to_str()) != Some(name)
            {
                return Err("Invalid audio filename".into());
            }
            let directory = tempfile::Builder::new()
                .prefix("music-")
                .tempdir_in(&root)
                .map_err(|_| "Cannot create local audio copy")?;
            let copy = directory.path().join(name);
            let owner = ImportOwner {
                version: 1,
                file: name.clone(),
            };
            let mut marker = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.path().join(IMPORT_OWNER))
                .map_err(|_| "Cannot record local audio ownership")?;
            std::io::Write::write_all(
                &mut marker,
                &serde_json::to_vec(&owner).map_err(|_| "Cannot record local audio ownership")?,
            )
            .map_err(|_| "Cannot record local audio ownership")?;
            marker
                .sync_all()
                .map_err(|_| "Cannot save local audio ownership")?;
            let copy_limit = FILE_LIMIT.min(BATCH_LIMIT.saturating_sub(total));
            let mut input = fs::File::open(path)
                .map_err(|_| "Cannot read selected audio")?
                .take(copy_limit + 1);
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&copy)
                .map_err(|_| "Cannot create local audio copy")?;
            let mut size = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancelled() {
                    return Err("Audio import cancelled".into());
                }
                let read = std::io::Read::read(&mut input, &mut buffer)
                    .map_err(|_| "Cannot copy selected audio")?;
                if read == 0 {
                    break;
                }
                std::io::Write::write_all(&mut output, &buffer[..read])
                    .map_err(|_| "Cannot copy selected audio")?;
                size += read as u64;
            }
            total = total
                .checked_add(size)
                .ok_or("Audio selection is too large")?;
            if size == 0 || size > FILE_LIMIT || total > BATCH_LIMIT {
                return Err("Audio limits: 256 MiB per file, 512 MiB per selection".into());
            }
            output
                .sync_all()
                .map_err(|_| "Cannot save local audio copy")?;
            drop(output);
            // Probe actual content rather than trusting the document's name/MIME.
            let file = fs::File::open(&copy).map_err(|_| "Cannot validate audio copy")?;
            let mut decoder = rodio::Decoder::try_from(file)
                .map_err(|_| "Selected file is not playable audio")?;
            if decoder.next().is_none() {
                return Err("Selected file contains no decodable audio".into());
            }
            let track = refresh_track(&copy).map_err(|_| "Cannot read selected audio tags")?;
            if cancelled() {
                return Err("Audio import cancelled".into());
            }
            prepared.tracks.push(track);
            prepared.directories.push(directory);
        }
        Ok(prepared)
    }

    pub(crate) fn commit(self, library: &mut Library, store: &Storage) -> Result<usize, String> {
        let mut updated = library.clone();
        updated.tracks.extend(self.tracks.iter().cloned());
        updated.save(store)?;
        let count = self.tracks.len();
        for directory in self.directories {
            let _ = directory.keep();
        }
        *library = updated;
        Ok(count)
    }
}

impl Library {
    /// Run once in the mobile host's private data directory, before starting workers or imports.
    /// Only flat, marked import directories are owned; legacy/unrecognized content is preserved.
    pub fn recover_imports(&self, data: &Path) -> Result<usize, String> {
        let root = data.join("local-audio");
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(_) => return Err("Cannot inspect local audio copies".into()),
        };
        if !metadata.is_dir() {
            return Err("Local audio directory is not a regular directory; copies retained".into());
        }
        let canonical_root = root
            .canonicalize()
            .map_err(|_| "Cannot resolve local audio directory")?;
        let mut referenced = HashSet::new();
        for track in &self.tracks {
            // Keep lexical references even if their audio is missing. Resolve aliases too;
            // an unresolved reference makes cleanup unsafe, so preserve all candidates.
            if let Ok(relative) = track.path.strip_prefix(&root) {
                if let Some(component) = relative.components().next() {
                    referenced.insert(root.join(component.as_os_str()));
                }
            }
            let path = track
                .path
                .canonicalize()
                .map_err(|_| "Some indexed audio cannot be resolved; copies retained")?;
            if let Ok(relative) = path.strip_prefix(&canonical_root) {
                if let Some(component) = relative.components().next() {
                    referenced.insert(root.join(component.as_os_str()));
                }
            }
        }
        let mut removed = 0;
        for entry in fs::read_dir(&root).map_err(|_| "Cannot inspect local audio copies")? {
            let entry = entry.map_err(|_| "Cannot inspect local audio copy")?;
            let path = entry.path();
            let name = entry.file_name();
            if !name.to_str().is_some_and(|name| name.starts_with("music-"))
                || referenced.contains(&path)
                || !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir())
            {
                continue;
            }
            let marker = path.join(IMPORT_OWNER);
            if !fs::symlink_metadata(&marker)
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 4096)
            {
                continue;
            }
            let Ok(bytes) = fs::read(&marker) else {
                continue;
            };
            let Ok(owner) = serde_json::from_slice::<ImportOwner>(&bytes) else {
                continue;
            };
            if owner.version != 1
                || owner.file == IMPORT_OWNER
                || Path::new(&owner.file)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(owner.file.as_str())
                || !audio(Path::new(&owner.file))
            {
                continue;
            }
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            let mut recognized = true;
            let mut audio_exists = false;
            for child in entries {
                let Ok(child) = child else {
                    recognized = false;
                    break;
                };
                let child_name = child.file_name();
                if (child_name != IMPORT_OWNER && child_name != owner.file.as_str())
                    || !fs::symlink_metadata(child.path()).is_ok_and(|metadata| metadata.is_file())
                {
                    recognized = false;
                    break;
                }
                audio_exists |= child_name == owner.file.as_str();
            }
            if !recognized {
                continue;
            }
            if audio_exists {
                fs::remove_file(path.join(&owner.file))
                    .map_err(|_| "Cannot remove abandoned local audio copy")?;
            }
            fs::remove_file(marker).map_err(|_| "Cannot remove abandoned import marker")?;
            fs::remove_dir(path).map_err(|_| "Cannot remove abandoned import directory")?;
            removed += 1;
        }
        Ok(removed)
    }

    /// An index taken over from the original's `local-music.db`.
    pub fn from_legacy(directories: Vec<PathBuf>, tracks: Vec<Track>) -> Self {
        Self {
            version: 1,
            directories,
            tracks,
        }
    }
}

/// A track of the original's database: its recorded tags are kept (they may have been edited
/// in the original), the file's size and modification time are read from the disk. `None` when
/// the file is gone or is not a usable audio file.
pub fn legacy_track(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    duration: u64,
    year: u32,
) -> Option<Track> {
    if !path.is_absolute() || !audio(path) {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let title = if title.trim().is_empty() {
        path.file_stem()?.to_string_lossy().into_owned()
    } else {
        title.to_owned()
    };
    Some(Track {
        song: Song {
            id: song_id(path),
            title,
            artist: artist.to_owned(),
            album: album.to_owned(),
            duration,
            cover_art: None,
        },
        path: path.to_owned(),
        size: metadata.len(),
        mtime_ms: modified(&metadata),
        year,
    })
}

pub fn is_local(song: &Song) -> bool {
    song.id.starts_with("mio-local:")
}

fn song_id(path: &Path) -> String {
    // Original scanner's stable path hash (named md5_hash there, actually djb2).
    let hash = path.to_string_lossy().bytes().fold(5381u64, |h, b| {
        h.wrapping_mul(33).wrapping_add(u64::from(b))
    });
    format!("mio-local:{hash:016x}")
}
fn audio(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| EXTENSIONS.contains(&s.to_ascii_lowercase().as_str()))
}
fn read_track(path: &Path, metadata: &fs::Metadata) -> (Track, bool) {
    let parsed = lofty::probe::Probe::open(path)
        .and_then(|p| Ok(p.guess_file_type()?))
        .and_then(|p| {
            p.options(lofty::config::ParseOptions::new().read_cover_art(false))
                .read()
        });
    let mut song = Song {
        id: song_id(path),
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        artist: String::new(),
        album: String::new(),
        duration: 0,
        cover_art: None,
    };
    let mut year = 0;
    if let Ok(file) = &parsed {
        song.duration = file.properties().duration().as_secs();
        if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
            if let Some(title) = tag.title().filter(|s| !s.trim().is_empty()) {
                song.title = title.into_owned();
            }
            song.artist = tag.artist().map(|s| s.into_owned()).unwrap_or_default();
            song.album = tag.album().map(|s| s.into_owned()).unwrap_or_default();
            year = tag.year().unwrap_or(0);
        }
    }
    (
        Track {
            song,
            path: path.to_owned(),
            size: metadata.len(),
            mtime_ms: modified(metadata),
            year,
        },
        parsed.is_err(),
    )
}
fn modified(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
pub fn refresh_track(path: &Path) -> Result<Track, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "无法读取音乐文件")?;
    if !metadata.is_file() {
        return Err("音乐文件已改变，请重新扫描".into());
    }
    let (track, failed) = read_track(path, &metadata);
    if failed {
        Err("无法读取音乐标签".into())
    } else {
        Ok(track)
    }
}
/// Atomically replace audio tags without publishing an index snapshot.
/// Callers must merge the returned track into their current library.
pub fn write_audio_tags(expected: &Track, values: &TagValues) -> Result<Track, String> {
    prepare_audio_tags_when(expected, values, || false)?.commit(|| true)
}

/// Worker-owned completed copy. Only a host owner may admit its final replacement.
pub struct PreparedTagEdit {
    copy: tempfile::NamedTempFile,
    expected: Track,
    original: fs::Metadata,
    track: Track,
}
impl PreparedTagEdit {
    pub fn matches(&self, track: &Track) -> bool {
        self.expected.path == track.path && self.expected.song.id == track.song.id
    }
    pub fn commit(self, admitted: impl Fn() -> bool) -> Result<Track, String> {
        let current = fs::symlink_metadata(&self.expected.path)
            .map_err(|_| "音乐文件已改变，原文件已保留")?;
        if !same_audio_file(&current, &self.original) {
            return Err("音乐文件已改变，请重新打开后再编辑".into());
        }
        if !admitted() {
            return Err("标签编辑已取消，原文件已保留".into());
        }
        self.copy
            .persist(&self.expected.path)
            .map_err(|_| "无法替换音乐文件，原文件已保留")?;
        Ok(self.track)
    }
}
fn same_audio_file(current: &fs::Metadata, original: &fs::Metadata) -> bool {
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        current.ino() == original.ino() && current.dev() == original.dev()
    };
    #[cfg(not(unix))]
    let same = true;
    current.is_file()
        && same
        && current.len() == original.len()
        && current.modified().ok() == original.modified().ok()
}
/// A validated static PNG, constructed only through the bounded cover decoder.
#[derive(Clone, PartialEq, Eq)]
pub struct TagCover(Vec<u8>);
impl TagCover {
    pub fn prepare(bytes: Vec<u8>, extension: &str) -> Result<Self, String> {
        crate::cover::prepare_playlist_cover(bytes, extension)
            .map(|cover| Self(cover.preview))
            .map_err(|_| "invalid_image: PNG/JPG/GIF/WebP <=5 MiB within image limits".into())
    }
    pub fn preview(&self) -> Vec<u8> {
        self.0.clone()
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum TagCoverEdit {
    Keep,
    Remove,
    Replace(TagCover),
}

/// Copy/parse/verify off the owner, without changing the original audio or index.
pub fn prepare_audio_tags_when(
    expected: &Track,
    values: &TagValues,
    cancelled: impl Fn() -> bool,
) -> Result<PreparedTagEdit, String> {
    prepare_audio_tags_with_cover_when(expected, values, &TagCoverEdit::Keep, cancelled)
}
pub fn prepare_audio_tags_with_cover_when(
    expected: &Track,
    values: &TagValues,
    cover: &TagCoverEdit,
    cancelled: impl Fn() -> bool,
) -> Result<PreparedTagEdit, String> {
    let check = || {
        if cancelled() {
            Err("标签编辑已取消，原文件已保留")
        } else {
            Ok(())
        }
    };
    check()?;
    if values.year > 2099
        || [&values.title, &values.artist, &values.album]
            .iter()
            .any(|s| s.len() > 4096 || s.contains('\0'))
    {
        return Err("标签内容无效：年份须为 0–2099，文字不超过 4096 字节".into());
    }
    let metadata = fs::symlink_metadata(&expected.path).map_err(|_| "音乐文件不存在或无法读取")?;
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err("音乐文件不可写，原文件已保留".into());
    }
    if metadata.len() != expected.size || modified(&metadata) != expected.mtime_ms {
        return Err("音乐文件已改变，请重新扫描后再编辑".into());
    }
    let parent = expected.path.parent().ok_or("无文件路径")?;
    let mut copy = tempfile::Builder::new()
        .prefix(".mio-tags-")
        .suffix(&format!(
            ".{}",
            expected
                .path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
        ))
        .tempfile_in(parent)
        .map_err(|_| "无法创建标签保存副本，原文件已保留")?;
    let mut source = fs::File::open(&expected.path).map_err(|_| "无法读取音乐文件")?;
    if !same_audio_file(
        &source.metadata().map_err(|_| "无法读取音乐文件")?,
        &metadata,
    ) {
        return Err("音乐文件已改变，原文件已保留".into());
    }
    let mut buffer = [0u8; 65536];
    let mut copied = 0u64;
    loop {
        check()?;
        let count = source
            .read(&mut buffer)
            .map_err(|_| "无法复制音乐文件，原文件已保留")?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .filter(|n| *n <= metadata.len())
            .ok_or("音乐文件已改变，原文件已保留")?;
        std::io::Write::write_all(copy.as_file_mut(), &buffer[..count])
            .map_err(|_| "无法复制音乐文件，原文件已保留")?;
    }
    if copied != metadata.len() {
        return Err("音乐文件已改变，原文件已保留".into());
    }
    check()?;
    let mut file = lofty::probe::Probe::open(copy.path())
        .and_then(|p| Ok(p.guess_file_type()?))
        .and_then(|p| p.read())
        .map_err(|_| "无法读取音乐标签，原文件已保留")?;
    // Hash image payloads instead of cloning potentially large existing artwork.
    // Include all metadata and tag placement so Keep cannot silently lose images
    // during a format-specific tag rewrite.
    let artwork = |file: &lofty::file::TaggedFile| {
        use sha2::{Digest, Sha256};
        file.tags()
            .iter()
            .filter(|tag| !tag.pictures().is_empty())
            .map(|tag| {
                (
                    tag.tag_type(),
                    tag.pictures()
                        .iter()
                        .map(|picture| {
                            (
                                picture.pic_type(),
                                picture.mime_type().cloned(),
                                picture.description().map(str::to_owned),
                                Sha256::digest(picture.data()).to_vec(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    let original_artwork = matches!(cover, TagCoverEdit::Keep).then(|| artwork(&file));
    if file.primary_tag().is_none() {
        file.insert_tag(lofty::tag::Tag::new(file.primary_tag_type()));
    }
    // Only the primary tag receives a new image. Remove old images from every
    // tag so fallback readers cannot resurrect an obsolete cover.
    let primary = file.primary_tag_type();
    let types: Vec<_> = file.tags().iter().map(|t| t.tag_type()).collect();
    for kind in types {
        let tag = file.tag_mut(kind).ok_or("无法编辑该音频格式的标签")?;
        if !matches!(cover, TagCoverEdit::Keep) {
            while !tag.pictures().is_empty() {
                tag.remove_picture(0);
            }
            if kind == primary {
                if let TagCoverEdit::Replace(image) = cover {
                    let mut picture =
                        lofty::picture::Picture::from_reader(&mut std::io::Cursor::new(&image.0))
                            .map_err(|_| "封面图片无效，原文件已保留")?;
                    picture.set_pic_type(lofty::picture::PictureType::CoverFront);
                    tag.push_picture(picture);
                }
            }
        }
        tag.set_title(values.title.clone());
        tag.set_artist(values.artist.clone());
        tag.set_album(values.album.clone());
        if values.year == 0 {
            tag.remove_year();
        } else {
            tag.set_year(values.year);
        }
    }
    check()?;
    file.save_to(copy.as_file_mut(), lofty::config::WriteOptions::default())
        .map_err(|_| "无法写入该音频格式的标签，原文件已保留")?;
    let verify = lofty::probe::Probe::open(copy.path())
        .and_then(|p| Ok(p.guess_file_type()?))
        .and_then(|p| p.read())
        .map_err(|_| "标签写入校验失败，原文件已保留")?;
    let tag = verify
        .primary_tag()
        .ok_or("标签写入校验失败，原文件已保留")?;
    if tag.title().as_deref().unwrap_or("") != values.title
        || tag.artist().as_deref().unwrap_or("") != values.artist
        || tag.album().as_deref().unwrap_or("") != values.album
        || tag.year().unwrap_or(0) != values.year
    {
        return Err("该音频格式未完整保存标签，原文件已保留".into());
    }
    let pictures: Vec<_> = verify
        .tags()
        .iter()
        .flat_map(|tag| tag.pictures())
        .collect();
    let cover_saved = match cover {
        TagCoverEdit::Keep => original_artwork.as_ref() == Some(&artwork(&verify)),
        TagCoverEdit::Remove => pictures.is_empty(),
        TagCoverEdit::Replace(image) => {
            pictures.len() == 1
                && pictures[0].data() == image.0
                && pictures[0].pic_type() == lofty::picture::PictureType::CoverFront
        }
    };
    if !cover_saved {
        return Err("该音频格式未完整保存封面，原文件已保留".into());
    }
    fs::set_permissions(copy.path(), metadata.permissions())
        .map_err(|_| "无法保留文件权限，原文件已保留")?;
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // Keep Finder metadata, ACLs and resource forks when replacing the audio inode.
        let copied = unsafe {
            libc::fcopyfile(
                source.as_raw_fd(),
                copy.as_file().as_raw_fd(),
                std::ptr::null_mut(),
                libc::COPYFILE_METADATA,
            )
        };
        if copied != 0 {
            return Err("无法保留文件属性，原文件已保留".into());
        }
    }
    copy.as_file()
        .sync_all()
        .map_err(|_| "无法保存音乐文件，原文件已保留")?;
    // Validate metadata on the completed copy before replacing the original audio.
    let (mut track, failed) = read_track(
        copy.path(),
        &copy.as_file().metadata().map_err(|_| "无法读取保存副本")?,
    );
    if failed {
        return Err("无法校验保存副本，原文件已保留".into());
    }
    track.path = expected.path.clone();
    track.song.id = expected.song.id.clone();
    if values.title.trim().is_empty() {
        track.song.title = expected
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    }
    check()?;
    let current =
        fs::symlink_metadata(&expected.path).map_err(|_| "音乐文件已改变，原文件已保留")?;
    if !same_audio_file(&current, &metadata) {
        return Err("音乐文件已改变，请重新打开后再编辑".into());
    }
    Ok(PreparedTagEdit {
        copy,
        expected: expected.clone(),
        original: metadata,
        track,
    })
}

impl Library {
    pub fn load(store: &Storage) -> Result<Self, String> {
        let file = match fs::File::open(store.root().join(INDEX)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("无法读取本地音乐索引，原文件已保留".into()),
        };
        let mut bytes = Vec::new();
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取本地音乐索引")?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("本地音乐索引超过 64MiB".into());
        }
        let library: Self =
            serde_json::from_slice(&bytes).map_err(|_| "本地音乐索引损坏，原文件已保留")?;
        if library.version != 1
            || library.tracks.len() > 100_000
            || library.directories.iter().any(|p| !p.is_absolute())
            || library
                .tracks
                .iter()
                .any(|t| !t.path.is_absolute() || !audio(&t.path) || t.song.id != song_id(&t.path))
        {
            return Err("本地音乐索引格式无效，原文件已保留".into());
        }
        Ok(library)
    }
    pub fn save(&self, store: &Storage) -> Result<(), String> {
        if self.tracks.len() > 100_000 {
            return Err("本地音乐超过十万首，原索引已保留".into());
        }
        if serde_json::to_vec_pretty(self)
            .map_err(|_| "无法保存本地音乐索引")?
            .len()
            + 1
            > 64 * 1024 * 1024
        {
            return Err("本地音乐索引过大，原文件已保留".into());
        }
        store
            .save(INDEX, self)
            .map_err(|_| "无法保存本地音乐索引，原文件已保留".into())
    }
    pub fn path(&self, id: &str) -> Option<PathBuf> {
        self.tracks
            .iter()
            .find(|t| t.song.id == id)
            .map(|t| t.path.clone())
    }
    pub fn edit_tags(
        mut self,
        expected: &Track,
        values: &TagValues,
        store: &Storage,
    ) -> Result<EditResult, String> {
        if values.year > 2099
            || [&values.title, &values.artist, &values.album]
                .iter()
                .any(|s| s.len() > 4096 || s.contains('\0'))
        {
            return Err("标签内容无效：年份须为 0–2099，文字不超过 4096 字节".into());
        }
        let index = self
            .tracks
            .iter()
            .position(|t| t.path == expected.path && t.song.id == expected.song.id)
            .ok_or("歌曲索引已改变，请重新扫描后再编辑")?;
        let track = write_audio_tags(expected, values)?;
        self.tracks[index] = track.clone();
        let index_error = self
            .save(store)
            .err()
            .map(|_| "音频标签已保存，但索引保存失败；请重新扫描恢复".into());
        Ok(EditResult {
            library: self,
            track,
            index_error,
        })
    }
    pub fn scan(
        mut self,
        skip_hidden: bool,
        store: &Storage,
    ) -> Result<(Self, ScanResult), String> {
        let mut result = ScanResult::default();
        let mut tracks: HashMap<PathBuf, Track> = self
            .tracks
            .into_iter()
            .map(|t| (t.path.clone(), t))
            .collect();
        let mut found = HashSet::new();
        let mut completed = Vec::new();
        for root in &self.directories {
            let mut visited = HashSet::new();
            let mut pending = vec![root.clone()];
            let mut complete = true;
            while let Some(dir) = pending.pop() {
                if !visited.insert(dir.clone()) {
                    continue;
                }
                let entries = match fs::read_dir(&dir) {
                    Ok(e) => e,
                    Err(_) => {
                        result.errors += 1;
                        complete = false;
                        continue;
                    }
                };
                for entry in entries {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => {
                            result.errors += 1;
                            complete = false;
                            continue;
                        }
                    };
                    if skip_hidden && entry.file_name().to_string_lossy().starts_with('.') {
                        continue;
                    }
                    let path = entry.path();
                    let kind = match entry.file_type() {
                        Ok(t) => t,
                        Err(_) => {
                            result.errors += 1;
                            complete = false;
                            continue;
                        }
                    };
                    // Do not follow directory or file symlinks into another tree / cycles.
                    if kind.is_dir() {
                        pending.push(path);
                        continue;
                    }
                    if !kind.is_file() || !audio(&path) {
                        continue;
                    }
                    if found.insert(path.clone()) {
                        result.scanned += 1;
                    }
                    if result.scanned > 100_000 {
                        return Err("本地音乐超过十万首，原索引已保留".into());
                    }
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => {
                            result.errors += 1;
                            complete = false;
                            continue;
                        }
                    };
                    let previous = tracks.get(&path);
                    if previous.is_some_and(|t| {
                        t.mtime_ms == modified(&metadata) && t.size == metadata.len()
                    }) {
                        continue;
                    }
                    if previous.is_some() {
                        result.updated += 1;
                    } else {
                        result.added += 1;
                    }
                    let (track, error) = read_track(&path, &metadata);
                    result.errors += usize::from(error);
                    tracks.insert(path, track);
                }
            }
            if complete {
                completed.push(root.clone());
            }
        }
        // An unreadable or missing root must never erase its previous indexed songs.
        tracks.retain(|path, _| {
            found.contains(path) || !completed.iter().any(|root| path.starts_with(root))
        });
        self.tracks = tracks.into_values().collect();
        self.tracks.sort_by(|a, b| a.path.cmp(&b.path));
        self.save(store)?;
        Ok((self, result))
    }
}

/// Local sidecar first, then embedded unsynchronised lyrics; no online lookup.
fn lyric_sidecar(path: &Path) -> Result<Option<String>, String> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "无法读取本地歌词")?;
            if bytes.len() > 1024 * 1024 {
                return Err("本地歌词超过 1MiB".into());
            }
            Ok(Some(
                String::from_utf8(bytes)
                    .map_err(|_| "本地歌词不是 UTF-8")?
                    .trim_start_matches('\u{feff}')
                    .to_owned(),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("无法读取本地歌词".into()),
    }
}

pub fn lyrics(path: &Path) -> Result<Option<crate::lyrics::Lyrics>, String> {
    if let Some(text) = lyric_sidecar(&path.with_extension("ttml"))? {
        let lines = mio_music_sdk::parse_ttml(&text)?;
        return crate::lyrics::from_ttml(lines)
            .map(Some)
            .ok_or_else(|| "本地 TTML 歌词没有有效行".into());
    }
    let text = lyric_sidecar(&path.with_extension("lrc"))?;
    let text = if let Some(text) = text {
        text
    } else {
        if fs::metadata(path).map_err(|_| "无法读取音乐文件")?.len() > 128 * 1024 * 1024 {
            return Ok(None);
        }
        let file = lofty::probe::Probe::open(path)
            .and_then(|p| Ok(p.guess_file_type()?))
            .and_then(|p| {
                p.options(lofty::config::ParseOptions::new().read_cover_art(false))
                    .read()
            })
            .map_err(|_| "无法读取内嵌歌词")?;
        file.tags()
            .iter()
            .find_map(|tag| tag.get_string(&lofty::tag::ItemKey::Lyrics))
            .unwrap_or_default()
            .to_owned()
    };
    if text.len() > 1024 * 1024 {
        return Err("本地歌词超过 1MiB".into());
    }
    Ok(crate::lyrics::from_online(&mio_music_sdk::Lyric {
        lyric: text,
        source: "local".into(),
        ..Default::default()
    }))
}
