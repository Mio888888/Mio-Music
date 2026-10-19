//! Bounded mobile projections. SQLite work happens on the FRB caller, after
//! an owner-thread snapshot of handles; no runtime/registry lock spans I/O.
use crate::{api::*, runtime};
use mio_core::{download, library, online};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static READERS: AtomicUsize = AtomicUsize::new(0);
static SNAPSHOTS: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct SnapshotPermit;
impl SnapshotPermit {
    pub(crate) fn acquire() -> Result<Self, String> {
        SNAPSHOTS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 8).then_some(n + 1)
            })
            .map_err(|_| "busy: too many retained catalog snapshots")?;
        Ok(Self)
    }
}
impl Drop for SnapshotPermit {
    fn drop(&mut self) {
        SNAPSHOTS.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) struct ReadPermit;
impl ReadPermit {
    pub(crate) fn acquire() -> Result<Self, String> {
        READERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "busy: too many catalog requests")?;
        Ok(Self)
    }
}
impl Drop for ReadPermit {
    fn drop(&mut self) {
        READERS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn page_bounds(offset: u32, limit: u32) -> Result<(usize, usize), String> {
    if limit == 0 || limit > 200 || offset > i32::MAX as u32 {
        return Err("invalid_input: page limit must be 1..200 and offset bounded".into());
    }
    Ok((offset as usize, limit as usize))
}
pub(crate) fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err("invalid_input: song or playlist identifier".into());
    }
    Ok(())
}
pub(crate) fn selection_ids(song_ids: &[String]) -> Result<(), String> {
    if song_ids.is_empty() || song_ids.len() > 200 {
        return Err("invalid_input: select 1..200 songs".into());
    }
    let mut bytes = 0usize;
    let mut unique = std::collections::HashSet::new();
    for song_id in song_ids {
        identifier(song_id)?;
        bytes = bytes
            .checked_add(song_id.len())
            .filter(|n| *n <= 64 * 1024)
            .ok_or("capacity_exceeded: removal identifiers")?;
        if !unique.insert(song_id.as_str()) {
            return Err("invalid_input: duplicate removal song".into());
        }
    }
    Ok(())
}
fn count(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}
pub(crate) fn playlist_info(row: library::Playlist) -> PlaylistInfo {
    PlaylistInfo {
        create_date: row.create_date,
        id: row.id,
        title: row.name,
        description: row.description,
        cover: if row.cover_img_url.starts_with("data:") {
            library::EMBEDDED_COVER_MARKER.into()
        } else {
            row.cover_img_url
        },
        source: row.source,
        count: count(row.song_count),
        favorite: row.favorite,
    }
}

// The parsed projection may expand IDs/default text relative to stored JSON.
// Count every UTF-8 string sent over FRB, independently of the SQL raw budget.
#[derive(Default)]
struct PageText(usize);
impl PageText {
    fn add<'a>(&mut self, values: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
        for value in values {
            self.0 = self
                .0
                .checked_add(value.len())
                .filter(|bytes| *bytes <= 2 * 1024 * 1024)
                .ok_or("capacity_exceeded: catalog page text")?;
        }
        Ok(())
    }
    fn playlist(&mut self, row: &library::Playlist) -> Result<(), String> {
        self.add([
            row.id.as_str(),
            &row.name,
            &row.description,
            if row.cover_img_url.starts_with("data:") {
                library::EMBEDDED_COVER_MARKER
            } else {
                &row.cover_img_url
            },
            &row.source,
        ])
    }
    fn song(&mut self, row: &mio_core::song::Song) -> Result<(), String> {
        self.add([
            row.id.as_str(),
            &row.title,
            &row.artist,
            &row.album,
            row.cover_art.as_deref().unwrap_or(""),
        ])
    }
}
pub(crate) fn playlist_items(rows: Vec<library::Playlist>) -> Result<Vec<PlaylistInfo>, String> {
    let mut budget = PageText::default();
    rows.into_iter()
        .map(|row| {
            budget.playlist(&row)?;
            Ok(playlist_info(row))
        })
        .collect()
}
fn saved_items(detail: library::PlaylistDetail) -> Result<(PlaylistInfo, Vec<Track>), String> {
    let mut budget = PageText::default();
    budget.playlist(&detail.playlist)?;
    let items = detail
        .songs
        .iter()
        .map(|row| {
            budget.song(&row.song)?;
            Ok(runtime::track(&row.song))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((playlist_info(detail.playlist), items))
}

pub(crate) struct LocalSession {
    closed: Arc<AtomicBool>,
    query: String,
    revision: u64,
    lease: runtime::RuntimeLease,
    _permit: SnapshotPermit,
}
pub(crate) fn begin_local(query: String) -> Result<LocalTrackSession, String> {
    if query.len() > 1024 {
        return Err("invalid_input: query too long".into());
    }
    let _read = ReadPermit::acquire()?;
    let permit = SnapshotPermit::acquire()?;
    let (revision, lease) = runtime::inspect_with_lease(|core| core.local_revision())?;
    Ok(LocalTrackSession {
        session: LocalSession {
            closed: Arc::new(AtomicBool::new(false)),
            query: query.trim().to_lowercase(),
            revision,
            lease,
            _permit: permit,
        },
    })
}
impl LocalSession {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    pub(crate) fn play_all(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.execute(runtime::Action::PlayLocalAllAt(
            self.query.clone(),
            self.revision,
            self.closed.clone(),
        ))
    }
    pub(crate) fn cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        identifier(&id)?;
        let _permit = ReadPermit::acquire()?;
        let revision = self.revision;
        let query = self.query.clone();
        let source = self.lease.inspect(move |core| {
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".to_owned());
            }
            let row = core
                .local_track_index()
                .iter()
                .find(|row| {
                    row.song.id == id
                        && (query.is_empty()
                            || [&row.song.title, &row.song.artist, &row.song.album]
                                .iter()
                                .any(|text| text.to_lowercase().contains(&query)))
                })
                .ok_or("not_found: local track")?;
            // Force an indexed local source: never accept UI paths or invoke providers.
            Ok(mio_core::cover::Source::Local {
                path: row.path.clone(),
                cache_key: row.cover_key(),
            })
        })??;
        let bytes = source.load_thumbnail()?;
        self.lease.inspect(move |core| {
            if core.local_revision() != revision {
                Err("catalog_changed: refresh local music".to_owned())
            } else {
                Ok(())
            }
        })??;
        Ok(bytes)
    }

    pub(crate) fn prepare_tag_editor(&self, id: String) -> Result<LocalTagEditor, String> {
        identifier(&id)?;
        let revision = self.revision;
        let query = self.query.clone();
        let expected = self.lease.inspect(move |core| {
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".to_owned());
            }
            let row = core
                .local_track_index()
                .iter()
                .find(|row| {
                    row.song.id == id
                        && (query.is_empty()
                            || [&row.song.title, &row.song.artist, &row.song.album]
                                .iter()
                                .any(|text| text.to_lowercase().contains(&query)))
                })
                .ok_or("not_found: local track")?;
            if row.song.id.len() > 4096
                || row.path.to_str().is_none_or(|s| s.len() > 4096)
                || row.size > 256 * 1024 * 1024
                || [&row.song.title, &row.song.artist, &row.song.album]
                    .iter()
                    .any(|s| s.len() > 4096)
            {
                return Err("capacity_exceeded: local tags".into());
            }
            Ok(row.clone())
        })??;
        crate::local_tags::begin(expected, revision, self.lease.clone())
    }

    pub(crate) fn prepare_favorite(&self, id: String) -> Result<FavoriteIntent, String> {
        identifier(&id)?;
        let context = crate::favorites::Context::Local(self.revision);
        let guard = context.clone();
        let query = self.query.clone();
        crate::favorites::begin(self.lease.clone(), context, || {
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                let row = core
                    .local_track_index()
                    .iter()
                    .find(|row| {
                        row.song.id == id
                            && (query.is_empty()
                                || [&row.song.title, &row.song.artist, &row.song.album]
                                    .iter()
                                    .any(|text| text.to_lowercase().contains(&query)))
                    })
                    .ok_or("not_found: local song unavailable")?;
                let target = core
                    .favorite_target(&row.song)
                    .ok_or("favorite_unavailable: song unsupported")?;
                Ok((target, core.preferences.english()))
            })?
        })
    }
    pub(crate) fn play(&self, id: String) -> Result<(), String> {
        identifier(&id)?;
        self.lease
            .execute(runtime::Action::PlayLocalAt(id, self.revision))
    }
    pub(crate) fn append(&self, id: String) -> Result<(), String> {
        identifier(&id)?;
        self.lease
            .execute(runtime::Action::AppendLocal(id, self.revision))
    }
    /// Adds one local track to an existing saved playlist.
    pub(crate) fn add_to_playlist(
        &self,
        id: String,
        destination_id: String,
    ) -> Result<u32, String> {
        identifier(&id)?;
        identifier(&destination_id)?;
        let revision = self.revision;
        let (path, song) = self.lease.inspect(move |core| {
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".to_string());
            }
            let song = core
                .local_track_index()
                .iter()
                .find(|track| track.song.id == id)
                .map(|track| track.song.clone())
                .ok_or("not_found: local song unavailable")?;
            Ok((core.store.library_path(), song))
        })??;
        library::add_playlist_songs(
            &path,
            &mio_core::storage::SessionIdentity::local(),
            library::PlaylistDestination::Existing(destination_id),
            &[library::PlaylistTrack {
                song,
                quality: None,
            }],
        )
        .map_err(|_| "playlist_unavailable: cannot add song".to_string())?;
        Ok(1)
    }
    pub(crate) fn page(&self, offset: u32, limit: u32) -> Result<TrackPage, String> {
        let (start, size) = page_bounds(offset, limit)?;
        let _permit = ReadPermit::acquire()?;
        let revision = self.revision;
        let query = self.query.clone();
        self.lease.inspect(move |core| {
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".into());
            }
            local_page(core, &query, start, size, offset)
        })?
    }
}
pub(crate) fn local_tracks(query: String, offset: u32, limit: u32) -> Result<TrackPage, String> {
    let (start, size) = page_bounds(offset, limit)?;
    if query.len() > 1024 {
        return Err("invalid_input: query too long".into());
    }
    let _permit = ReadPermit::acquire()?;
    let query = query.trim().to_lowercase();
    runtime::inspect(move |core| local_page(core, &query, start, size, offset))?
}
fn local_page(
    core: &mio_core::app_core::AppCore,
    query: &str,
    start: usize,
    size: usize,
    offset: u32,
) -> Result<TrackPage, String> {
    let matching = || {
        core.local_track_index().iter().filter(|track| {
            query.is_empty()
                || [&track.song.title, &track.song.artist, &track.song.album]
                    .iter()
                    .any(|value| value.to_lowercase().contains(query))
        })
    };
    let total = matching().count();
    let mut items = Vec::new();
    let mut bytes = 0usize;
    for row in matching().skip(start).take(size) {
        for text in [
            &row.song.id,
            &row.song.title,
            &row.song.artist,
            &row.song.album,
        ] {
            bytes = bytes
                .checked_add(text.len())
                .ok_or("capacity_exceeded: local page")?;
        }
        bytes = bytes
            .checked_add(row.song.cover_art.as_ref().map_or(0, String::len))
            .ok_or("capacity_exceeded: local page")?;
        if bytes > 2 * 1024 * 1024 {
            return Err("capacity_exceeded: local page".into());
        }
        items.push(runtime::track(&row.song));
    }
    Ok(TrackPage {
        has_more: start + items.len() < total,
        items,
        total: count(total),
        offset,
    })
}

pub(crate) struct PlaylistEditorState {
    path: std::path::PathBuf,
    lease: runtime::RuntimeLease,
    id: Option<String>,
    expected: Option<(String, String)>,
    saved: std::sync::Mutex<Option<(String, String, library::Playlist)>>,
    _permit: SnapshotPermit,
}
pub(crate) fn begin_playlist_editor(id: Option<String>) -> Result<PlaylistEditor, String> {
    if let Some(id) = &id {
        identifier(id)?;
    }
    let _read = ReadPermit::acquire()?;
    let permit = SnapshotPermit::acquire()?;
    let (path, lease) = runtime::inspect_with_lease(|core| core.store.library_path())?;
    let info = id
        .as_ref()
        .map(|id| {
            library::LibraryCursor::open_mobile(&path)
                .and_then(|mut cursor| cursor.playlist(id, 0, 0))
        })
        .transpose()
        .map_err(|_| "storage_unavailable: cannot open playlist editor")?
        .map(|detail| detail.playlist);
    if info
        .as_ref()
        .is_some_and(|row| row.name.len() > 4096 || row.description.len() > 16384)
    {
        return Err("capacity_exceeded: playlist form too large".into());
    }
    lease.ensure_open()?;
    let expected = info.map(|row| (row.name, row.description));
    Ok(PlaylistEditor {
        editor: PlaylistEditorState {
            path,
            lease,
            id,
            expected,
            saved: std::sync::Mutex::new(None),
            _permit: permit,
        },
    })
}
impl PlaylistEditorState {
    pub(crate) fn draft(&self) -> PlaylistDraft {
        PlaylistDraft {
            name: self
                .expected
                .as_ref()
                .map(|v| v.0.clone())
                .unwrap_or_default(),
            description: self
                .expected
                .as_ref()
                .map(|v| v.1.clone())
                .unwrap_or_default(),
        }
    }
    pub(crate) fn save(&self, name: String, description: String) -> Result<PlaylistInfo, String> {
        if name.len() > 4096 || description.len() > 16384 {
            return Err("invalid_input: playlist form too large".into());
        }
        let name = name.trim().to_owned();
        let description = description.trim().to_owned();
        let name_len = if self.id.is_some() {
            name.chars().count()
        } else {
            name.encode_utf16().count()
        };
        if name.is_empty() || name_len > 50 || description.encode_utf16().count() > 200 {
            return Err("invalid_input: playlist name or description length".into());
        }
        let mut saved = self
            .saved
            .try_lock()
            .map_err(|_| "busy: playlist save in progress")?;
        if let Some((old_name, old_description, row)) = saved.as_ref() {
            return if (&name, &description) == (old_name, old_description) {
                Ok(playlist_info(row.clone()))
            } else {
                Err("catalog_changed: reopen playlist editor".into())
            };
        }
        let _read = ReadPermit::acquire()?;
        // Admission binds this write to the original path/runtime. Once started,
        // return the actual commit result even if the view/runtime then closes.
        self.lease.ensure_open()?;
        let expected = self.expected.as_ref().map(|v| (v.0.as_str(), v.1.as_str()));
        let row = library::save_playlist_checked(
            &self.path,
            self.id.as_deref(),
            expected,
            &name,
            &description,
        )
        .map_err(|_| "storage_changed_or_unavailable: reopen editor and retry")?;
        let info = playlist_info(row.clone());
        *saved = Some((name, description, row));
        Ok(info)
    }
}

pub(crate) fn browse_error(error: library::BrowseError) -> String {
    match error {
        library::BrowseError::Changed => "catalog_changed: refresh playlists",
        library::BrowseError::Unavailable => "storage_unavailable: cannot read playlists",
        library::BrowseError::Capacity => {
            "capacity_exceeded: playlist sort keys exceed 50000 rows, 8 MiB text or 2 MiB record"
        }
    }
    .into()
}
pub(crate) struct LibrarySessionState {
    cursor: Arc<std::sync::Mutex<library::LibraryCursor>>,
    lease: runtime::RuntimeLease,
    id: Option<String>,
    playback: SavedPlayback,
    _permit: SnapshotPermit,
}
/// Owner preflight contains no SQLite/provider reads. The selected row is
/// captured off-owner after checking the displayed cursor's database version.
#[derive(Clone)]
pub(crate) struct SavedPlayback {
    cancelled: Arc<AtomicBool>,
    local_revision: u64,
    pub(crate) online: Option<Arc<online::Online>>,
}
impl SavedPlayback {
    pub(crate) fn validate(&self, core: &mio_core::app_core::AppCore) -> Result<(), String> {
        if self.cancelled() {
            return Err("cancelled".into());
        }
        let same_source = match (&self.online, &core.online) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same_source || core.local_revision() != self.local_revision {
            return Err("catalog_changed: reopen saved playlist".into());
        }
        Ok(())
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
impl Drop for LibrarySessionState {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin_library(id: Option<String>) -> Result<LibrarySession, String> {
    if let Some(id) = &id {
        identifier(id)?;
    }
    let _read = ReadPermit::acquire()?;
    let permit = SnapshotPermit::acquire()?;
    let ((path, local_revision, online), lease) = runtime::inspect_with_lease(|core| {
        (
            core.store.library_path(),
            core.local_revision(),
            core.online.clone(),
        )
    })?;
    let cursor = library::LibraryCursor::open_mobile(&path).map_err(browse_error)?;
    lease.ensure_open()?;
    Ok(LibrarySession {
        session: LibrarySessionState {
            cursor: Arc::new(std::sync::Mutex::new(cursor)),
            lease,
            id,
            playback: SavedPlayback {
                cancelled: Arc::new(AtomicBool::new(false)),
                local_revision,
                online,
            },
            _permit: permit,
        },
    })
}
impl LibrarySessionState {
    pub(crate) fn cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        identifier(&id)?;
        if self.id.as_ref().is_some_and(|v| v != &id) {
            return Err("invalid_input: playlist ID".into());
        }
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let data = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: cover read")?
            .cover_data(&id)
            .map_err(browse_error)?;
        let image = data
            .map(|data| mio_core::cover::saved_playlist_thumbnail(&data))
            .transpose()?;
        self.cursor
            .try_lock()
            .map_err(|_| "busy: cover read")?
            .check()
            .map_err(browse_error)?;
        self.ensure_open()?;
        Ok(image)
    }
    pub(crate) fn prepare_cover_editor(&self) -> Result<PlaylistCoverEditor, String> {
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: playlist detail required")?;
        self.ensure_open()?;
        self.cursor
            .try_lock()
            .map_err(|_| "busy: cover editor")?
            .check()
            .map_err(browse_error)?;
        crate::playlist_cover::begin(
            self.cursor.clone(),
            self.lease.clone(),
            id.clone(),
            self.playback.cancelled.clone(),
        )
    }

    pub(crate) fn prepare_favorite(&self, song_id: String) -> Result<FavoriteIntent, String> {
        identifier(&song_id)?;
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let context = crate::favorites::Context::Saved(self.playback.clone());
        let guard = context.clone();
        crate::favorites::begin(self.lease.clone(), context, || {
            let raw = online::parse_song_id(&song_id)
                .map(|(_, raw)| raw)
                .unwrap_or_else(|| song_id.clone());
            let row = self
                .cursor
                .try_lock()
                .map_err(|_| "busy: playlist read in progress")?
                .song(id, &raw)
                .map_err(browse_error)?;
            if row.song.id != song_id {
                return Err("catalog_changed: saved song identity differs".into());
            }
            self.ensure_open()?;
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                let target = core
                    .saved_favorite_target(&row)
                    .ok_or("favorite_unavailable: song unsupported")?;
                Ok((target, core.preferences.english()))
            })?
        })
    }
    pub(crate) fn close(&self) {
        self.playback.cancelled.store(true, Ordering::Release);
    }
    fn ensure_open(&self) -> Result<(), String> {
        if self.playback.cancelled() {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn delete_playlist(&self) -> Result<(), String> {
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        self.cursor
            .try_lock()
            .map_err(|_| "busy: playlist operation in progress")?
            .delete_playlist(id, || self.ensure_open().is_ok())
            .map_err(browse_error)
    }

    pub(crate) fn play_songs(
        &self,
        song_ids: Vec<String>,
        sort: PlaylistSort,
    ) -> Result<(), String> {
        selection_ids(&song_ids)?;
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let rows = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist read in progress")?
            .selected_songs(id, &song_ids, sort.core())
            .map_err(browse_error)?;
        self.ensure_open()?;
        self.lease.execute(runtime::Action::PlaySavedSelection(
            rows,
            self.playback.clone(),
        ))
    }

    pub(crate) fn copy_songs(
        &self,
        song_ids: Vec<String>,
        destination_id: Option<String>,
        new_name: Option<String>,
    ) -> Result<u32, String> {
        selection_ids(&song_ids)?;
        let destination = match (destination_id, new_name) {
            (Some(id), None) => {
                identifier(&id)?;
                library::PlaylistDestination::Existing(id)
            }
            (None, Some(name)) => {
                let name = name.trim();
                if name.is_empty()
                    || name.len() > 4096
                    || name.encode_utf16().count() > 50
                    || name.contains('\0')
                {
                    return Err("invalid_input: playlist name".into());
                }
                library::PlaylistDestination::New(name.to_owned())
            }
            _ => return Err("invalid_input: choose one playlist destination".into()),
        };
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let guard = self.playback.clone();
        let added = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist operation in progress")?
            .copy_songs(id, &song_ids, destination, || {
                matches!(
                    self.lease.inspect(move |core| guard.validate(core)),
                    Ok(Ok(()))
                )
            })
            .map_err(browse_error)?;
        Ok(count(added))
    }

    pub(crate) fn destinations(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        if self.id.is_none() {
            return Err("invalid_input: expected playlist detail session".into());
        }
        let (start, size) = page_bounds(offset, limit)?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let (rows, total) = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist page in progress")?
            .playlists(start, size)
            .map_err(browse_error)?;
        self.ensure_open()?;
        Ok(PlaylistPage {
            has_more: start + rows.len() < total,
            items: playlist_items(rows)?,
            total: count(total),
            offset,
        })
    }

    pub(crate) fn remove_songs(&self, song_ids: Vec<String>) -> Result<(), String> {
        selection_ids(&song_ids)?;
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let guard = self.playback.clone();
        self.cursor
            .try_lock()
            .map_err(|_| "busy: playlist operation in progress")?
            .remove_songs(id, &song_ids, || {
                matches!(
                    self.lease.inspect(move |core| guard.validate(core)),
                    Ok(Ok(()))
                )
            })
            .map_err(browse_error)
    }

    pub(crate) fn play_all(&self) -> Result<(), String> {
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let page = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist read in progress")?
            .playlist(id, 0, 12_001)
            .map_err(browse_error)?;
        if page.songs.len() > 12_000 || page.playlist.song_count > 12_000 {
            return Err("capacity_exceeded: playlist playback".into());
        }
        if page.songs.is_empty() {
            return Err("playlist_empty".into());
        }
        if page.songs.len() != page.playlist.song_count {
            return Err("catalog_changed: incomplete playlist".into());
        }
        self.ensure_open()?;
        self.lease.execute(runtime::Action::PlaySavedSelection(
            page.songs,
            self.playback.clone(),
        ))
    }

    pub(crate) fn append_song(&self, song_id: String) -> Result<(), String> {
        identifier(&song_id)?;
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let raw_id = online::parse_song_id(&song_id)
            .map(|(_, raw)| raw)
            .unwrap_or_else(|| song_id.clone());
        let row = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist read in progress")?
            .song(id, &raw_id)
            .map_err(browse_error)?;
        if row.song.id != song_id {
            return Err("catalog_changed: saved song identity differs".into());
        }
        self.ensure_open()?;
        self.lease.execute(runtime::Action::AppendSaved(
            vec![row],
            self.playback.clone(),
        ))
    }

    pub(crate) fn play(&self, song_id: String) -> Result<(), String> {
        identifier(&song_id)?;
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let raw_id = online::parse_song_id(&song_id)
            .map(|(_, raw)| raw)
            .unwrap_or_else(|| song_id.clone());
        let row = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist read in progress")?
            .song(id, &raw_id)
            .map_err(browse_error)?;
        if row.song.id != song_id {
            return Err("catalog_changed: saved song identity differs".into());
        }
        self.ensure_open()?;
        self.lease.execute(runtime::Action::PlaySavedSession(
            row,
            self.playback.clone(),
        ))
    }

    pub(crate) fn playlists(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        if self.id.is_some() {
            return Err("invalid_input: expected playlist list session".into());
        }
        let (start, size) = page_bounds(offset, limit)?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let (rows, total) = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist page in progress")?
            .playlists(start, size)
            .map_err(browse_error)?;
        self.ensure_open()?;
        Ok(PlaylistPage {
            has_more: start + rows.len() < total,
            items: playlist_items(rows)?,
            total: count(total),
            offset,
        })
    }
    pub(crate) fn playlist(&self, offset: u32, limit: u32) -> Result<SavedPlaylistPage, String> {
        self.sorted_playlist(PlaylistSort::Default, offset, limit)
    }
    pub(crate) fn sorted_playlist(
        &self,
        sort: PlaylistSort,
        offset: u32,
        limit: u32,
    ) -> Result<SavedPlaylistPage, String> {
        let id = self
            .id
            .as_ref()
            .ok_or("invalid_input: expected playlist detail session")?;
        let (start, size) = page_bounds(offset, limit)?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let detail = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: playlist page in progress")?
            .sorted_playlist(id, sort.core(), start, size)
            .map_err(browse_error)?;
        self.ensure_open()?;
        let total = detail.playlist.song_count;
        let (playlist, items) = saved_items(detail)?;
        Ok(SavedPlaylistPage {
            playlist,
            tracks: TrackPage {
                has_more: start + items.len() < total,
                items,
                total: count(total),
                offset,
            },
        })
    }
}

pub(crate) fn playlists(offset: u32, limit: u32) -> Result<PlaylistPage, String> {
    let (start, size) = page_bounds(offset, limit)?;
    let _permit = ReadPermit::acquire()?;
    let (path, lease) = runtime::inspect_with_lease(|core| core.store.library_path())?;
    let (rows, total) = library::LibraryCursor::open_mobile(&path)
        .and_then(|mut cursor| cursor.playlists(start, size))
        .map_err(|_| "storage_unavailable: cannot read playlists")?;
    lease.ensure_open()?;
    Ok(PlaylistPage {
        has_more: start + rows.len() < total,
        items: playlist_items(rows)?,
        total: count(total),
        offset,
    })
}

pub(crate) fn saved_playlist(
    id: String,
    offset: u32,
    limit: u32,
) -> Result<SavedPlaylistPage, String> {
    identifier(&id)?;
    let (start, size) = page_bounds(offset, limit)?;
    let _permit = ReadPermit::acquire()?;
    let (path, lease) = runtime::inspect_with_lease(|core| core.store.library_path())?;
    let detail = library::LibraryCursor::open_mobile(&path)
        .and_then(|mut cursor| cursor.playlist(&id, start, size))
        .map_err(|_| "storage_unavailable: cannot read playlist")?;
    lease.ensure_open()?;
    let total = detail.playlist.song_count;
    let (playlist, items) = saved_items(detail)?;
    Ok(SavedPlaylistPage {
        playlist,
        tracks: TrackPage {
            has_more: start + items.len() < total,
            items,
            total: count(total),
            offset,
        },
    })
}

fn download_state(state: &download::Status) -> DownloadState {
    match state {
        download::Status::Queued => DownloadState::Queued,
        download::Status::Resolving => DownloadState::Resolving,
        download::Status::Downloading => DownloadState::Downloading,
        download::Status::Paused => DownloadState::Paused,
        download::Status::Done => DownloadState::Done,
        download::Status::Failed(_) => DownloadState::Failed,
        download::Status::Cancelled => DownloadState::Cancelled,
    }
}

/// Controls remain attached to the manager displayed by this snapshot. A
/// request may become a no-op when a worker changes state concurrently; callers
/// refresh actual state rather than claiming a successful state transition.
pub(crate) struct DownloadControl {
    root: std::path::PathBuf,
    manager: Arc<download::Downloads>,
    lease: runtime::RuntimeLease,
    closed: AtomicBool,
    operation: std::sync::Mutex<()>,
}
impl DownloadControl {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    pub(crate) fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn set_concurrent(&self, expected: u32, next: u32) -> Result<u32, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "busy: download command in progress")?;
        self.ensure_open()?;
        self.manager
            .compare_max_concurrent(expected as usize, next as usize)
            .map(|v| v as u32)
    }
    pub(crate) fn completed_file_name(&self, id: &str) -> Result<String, String> {
        identifier(id)?;
        let _read = ReadPermit::acquire()?;
        self.ensure_open()?;
        let name = self.manager.completed_file_name(id, &self.root)?;
        self.ensure_open()?;
        Ok(name)
    }
    pub(crate) fn command(&self, id: &str, action: DownloadAction) -> Result<(), String> {
        identifier(id)?;
        let _read = ReadPermit::acquire()?;
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "busy: download command in progress")?;
        self.ensure_open()?;
        // Admission completed. Preserve the original manager, even during
        // teardown; its own shutdown gate rejects work after manager close.
        match action {
            DownloadAction::Pause => self.manager.pause(id),
            DownloadAction::Resume => self.manager.resume(id),
            DownloadAction::Retry => self.manager.retry(id),
            DownloadAction::Cancel => self.manager.cancel(id),
            DownloadAction::Remove => self.manager.remove(id),
        }
        Ok(())
    }

    pub(crate) fn bulk_command(
        &self,
        rows: &[(String, DownloadState)],
        bucket: DownloadBucket,
        action: DownloadBulkAction,
    ) -> Result<(), String> {
        let _read = ReadPermit::acquire()?;
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "busy: download command in progress")?;
        self.ensure_open()?;
        if !matches!(action, DownloadBulkAction::Clear) && bucket != DownloadBucket::Active {
            return Err("invalid_input: expected active download tab".into());
        }
        match action {
            DownloadBulkAction::Clear => {
                let ids = rows.iter().map(|(id, _)| id.clone()).collect();
                let scope = match bucket {
                    DownloadBucket::Active => download::ClearScope::Queue,
                    DownloadBucket::Completed => download::ClearScope::Completed,
                    DownloadBucket::Failed => download::ClearScope::Failed,
                };
                self.manager.clear_selected(scope, &ids);
            }
            _ => {
                for (id, state) in rows {
                    // Closing the view stops further admissions. Earlier
                    // accepted transitions remain committed; no rollback claim.
                    self.ensure_open()?;
                    match action {
                        DownloadBulkAction::StartAll if *state == DownloadState::Paused => {
                            self.manager.resume(id)
                        }
                        DownloadBulkAction::PauseAll
                            if matches!(
                                state,
                                DownloadState::Queued
                                    | DownloadState::Resolving
                                    | DownloadState::Downloading
                            ) =>
                        {
                            self.manager.pause(id)
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}
impl Drop for DownloadControl {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn download_snapshot(
    bucket: DownloadBucket,
    query: String,
) -> Result<DownloadSnapshot, String> {
    let _permit = ReadPermit::acquire()?;
    let snapshot_permit = SnapshotPermit::acquire()?;
    if query.len() > 1024 {
        return Err("invalid_input: query too long".into());
    }
    let query = query.trim().to_lowercase();
    let ((manager, root), lease) =
        runtime::inspect_with_lease(|core| (core.downloads.clone(), core.paths.downloads.clone()))?;
    let concurrent = manager.max_concurrent() as u32;
    // The persisted-record cap does not bound live tasks. Reject oversized
    // snapshots before cloning; never silently omit active transfers.
    let bucket_of = |row: &download::TaskView| match row.status {
        download::Status::Done => DownloadBucket::Completed,
        download::Status::Failed(_) | download::Status::Cancelled => DownloadBucket::Failed,
        _ => DownloadBucket::Active,
    };
    let all = manager
        .try_snapshot_bounded(5000, 8 * 1024 * 1024)
        .map_err(|_| "snapshot_unavailable: download list exceeds capacity or cannot be read")?;
    let counts = [
        DownloadBucket::Active,
        DownloadBucket::Completed,
        DownloadBucket::Failed,
    ]
    .iter()
    .map(|bucket| count(all.iter().filter(|row| bucket_of(row) == *bucket).count()))
    .collect();
    let bulk_targets = all
        .iter()
        .filter(|row| bucket_of(row) == bucket)
        .map(|row| (row.id.clone(), download_state(&row.status)))
        .collect();
    let mut rows: Vec<_> = all
        .into_iter()
        .filter(|row| bucket_of(row) == bucket)
        .filter(|row| {
            query.is_empty()
                || row.title.to_lowercase().contains(&query)
                || row.artist.to_lowercase().contains(&query)
        })
        .collect();
    if bucket == DownloadBucket::Active {
        rows.sort_by_key(|row| match row.status {
            download::Status::Downloading | download::Status::Resolving => 0,
            download::Status::Queued => 1,
            _ => 2,
        });
    } else {
        rows.reverse();
    }
    let items: Vec<_> = rows
        .into_iter()
        .map(|row| DownloadItem {
            state: download_state(&row.status),
            id: row.id,
            title: row.title,
            artist: row.artist,
            quality: row.quality,
            received: row.received,
            total: row.total,
            speed: row.speed,
            remaining: row.remaining,
            missing: row.missing,
            skipped: row.skipped,
            tag_failed: row.tag_error.is_some(),
        })
        .collect();
    lease.ensure_open()?;
    Ok(DownloadSnapshot {
        concurrent,
        items,
        counts,
        bucket,
        bulk_targets,
        control: DownloadControl {
            root,
            manager,
            lease,
            closed: AtomicBool::new(false),
            operation: std::sync::Mutex::new(()),
        },
        _permit: snapshot_permit,
    })
}

pub(crate) fn play_local(id: String) -> Result<(), String> {
    identifier(&id)?;
    runtime::execute(runtime::Action::PlayLocal(id))
}

pub(crate) fn play_saved_song(playlist_id: String, song_id: String) -> Result<(), String> {
    identifier(&playlist_id)?;
    identifier(&song_id)?;
    let _permit = ReadPermit::acquire()?;
    let ((path, expected), lease) =
        runtime::inspect_with_lease(|core| (core.store.library_path(), core.online.clone()))?;
    let raw_id = online::parse_song_id(&song_id)
        .map(|(_, raw)| raw)
        .unwrap_or_else(|| song_id.clone());
    let row = library::load_playlist_song(&path, &playlist_id, &raw_id)
        .map_err(|_| "not_found: saved song unavailable")?;
    if row.song.id != song_id {
        return Err("source_changed: saved song identity differs".into());
    }
    // AppCore validates both source Arc identity and local file identity at commit.
    lease.execute(runtime::Action::PlaySaved(row, expected))
}
