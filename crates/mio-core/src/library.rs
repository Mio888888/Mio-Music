mod backup_export;
mod backup_restore;
use crate::song::{Song, SongQuality};
use crate::storage::SessionIdentity;
pub use backup_restore::{BackupRestoreOutcome, BackupSummary, PreparedBackup};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{collections::HashMap, path::Path, time::Duration};

// Process-local invalidation only: SQLite remains the source of truth. All
// successful library writes publish after commit; reads never advance it.
static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub fn revision() -> u64 {
    REVISION.load(std::sync::atomic::Ordering::Acquire)
}
fn mark_changed() {
    REVISION.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

// Match the original playlists.db schema. Extra source identity stays inside
// the existing MusicItem JSON, never in a request URL or credential field.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS playlists (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT DEFAULT '',
 coverImgUrl TEXT DEFAULT 'default-cover', source TEXT NOT NULL,
 meta TEXT DEFAULT '{}', createTime TEXT NOT NULL, updateTime TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS playlist_songs (
 playlist_id TEXT NOT NULL, songmid TEXT NOT NULL, position INTEGER NOT NULL,
 data TEXT NOT NULL, name TEXT DEFAULT '', singer TEXT DEFAULT '',
 albumName TEXT DEFAULT '', img TEXT DEFAULT '',
 PRIMARY KEY (playlist_id, songmid),
 FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_playlist_songs_position ON playlist_songs(playlist_id, position);
CREATE INDEX IF NOT EXISTS idx_playlist_songs_name ON playlist_songs(playlist_id, name);
CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

#[derive(Clone, Copy)]
pub enum LibraryError {
    Unavailable,
    DifferentSource,
}
type Result<T> = std::result::Result<T, LibraryError>;
fn unavailable(_: impl std::fmt::Display) -> LibraryError {
    // Raw SQLite/JSON errors can include data. Surface only a fixed message.
    LibraryError::Unavailable
}

/// Row loops reuse one compiled statement instead of preparing it again for every row.
fn execute_cached(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> rusqlite::Result<usize> {
    connection.prepare_cached(sql)?.execute(params)
}
/// The saved JSON of one playlist row, if the song is already in the playlist.
fn saved_song_json(
    connection: &Connection,
    params: impl rusqlite::Params,
) -> rusqlite::Result<Option<String>> {
    connection
        .prepare_cached("SELECT data FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2")?
        .query_row(params, |row| row.get(0))
        .optional()
}

#[derive(Default)]
pub struct Favorites {
    pub id: Option<String>,
    songs: HashMap<String, Option<SessionIdentity>>,
    /// `(source, songmid)` of favorites that are online-source songs (original MusicItem rows).
    online: std::collections::HashSet<(String, String)>,
}
impl Favorites {
    pub fn contains(&self, id: &str, identity: &SessionIdentity) -> bool {
        self.songs.get(id).and_then(Option::as_ref) == Some(identity)
    }
    pub fn contains_online(&self, source: &str, songmid: &str) -> bool {
        self.online
            .contains(&(source.to_owned(), songmid.to_owned()))
    }
}

/// A row written by an online source: the original MusicItem JSON with a known online `source`
/// and none of this app's Subsonic identity.
fn online_source(data: &serde_json::Value) -> Option<&str> {
    if data.get("_mioGpuiSource").is_some() {
        return None;
    }
    data.get("source")
        .and_then(serde_json::Value::as_str)
        .filter(|source| mio_music_sdk::is_implemented(source))
}

#[derive(Clone)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cover_img_url: String,
    pub source: String,
    pub create_date: String,
    pub song_count: usize,
    pub favorite: bool,
}

#[derive(Clone)]
pub struct PlaylistSong {
    pub song: Song,
    pub source: String,
    pub identity: Option<SessionIdentity>,
    pub quality: Option<SongQuality>,
    /// For online-source rows, the stored MusicItem, so the song can be resolved again.
    pub online: Option<serde_json::Value>,
}
impl PlaylistSong {
    pub fn playable(&self, identity: Option<&SessionIdentity>) -> bool {
        (self.source == "local"
            && crate::local_music::is_local(&self.song)
            && self.identity.as_ref() == Some(&SessionIdentity::local()))
            || (self.source == "subsonic"
                && self.identity.is_some()
                && self.identity.as_ref() == identity)
    }
}
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub songs: Vec<PlaylistSong>,
}

#[derive(Clone)]
pub struct PlaylistTrack {
    pub song: Song,
    pub quality: Option<SongQuality>,
}

pub enum PlaylistDestination {
    Existing(String),
    New(String),
}

fn playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
    Ok(Playlist {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        cover_img_url: row.get(3)?,
        source: row.get(4)?,
        create_date: row.get(5)?,
        song_count: row.get(6)?,
        favorite: row.get(7)?,
    })
}
const PLAYLIST_QUERY: &str = "SELECT p.id,p.name,p.description,p.coverImgUrl,p.source,COALESCE(strftime('%Y-%m-%d',p.createTime,'localtime'),'') AS create_date,
 (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id=p.id),
 EXISTS(SELECT 1 FROM kv_store k WHERE k.key='favoritesId' AND k.value=p.id)
 FROM playlists p";

/// Mobile pages carry a fixed marker rather than embedded originals; the cursor loads images lazily.
pub const EMBEDDED_COVER_MARKER: &str = "mio-saved-cover";
fn playlist_query(project_covers: bool) -> String {
    if project_covers {
        PLAYLIST_QUERY.replace("p.coverImgUrl", "CASE WHEN substr(p.coverImgUrl,1,5)='data:' THEN 'mio-saved-cover' ELSE p.coverImgUrl END")
    } else {
        PLAYLIST_QUERY.to_owned()
    }
}
fn playlist_columns(project_covers: bool) -> &'static [&'static str] {
    if project_covers {
        &[
            "p.id",
            "p.name",
            "p.description",
            "CASE WHEN substr(p.coverImgUrl,1,5)='data:' THEN 'mio-saved-cover' ELSE p.coverImgUrl END",
            "p.source",
            "COALESCE(strftime('%Y-%m-%d',p.createTime,'localtime'),'')",
        ]
    } else {
        PLAYLIST_TEXT_COLUMNS
    }
}

// Check the selected SQL page inside its read transaction before allocating
// owned strings or parsing saved JSON. Never skip/truncate an oversized row.
const BROWSE_TEXT_LIMIT: usize = 2 * 1024 * 1024;
fn check_page_text(
    connection: &Connection,
    from: &str,
    parameters: impl rusqlite::Params,
    columns: &[&str],
    used: &mut usize,
) -> Result<()> {
    let lengths = columns
        .iter()
        .map(|column| format!("length(CAST(COALESCE({column},'') AS BLOB))"))
        .collect::<Vec<_>>()
        .join("+");
    let bytes: i64 = connection
        .query_row(
            &format!("SELECT COALESCE(SUM(bytes),0) FROM (SELECT {lengths} AS bytes {from})"),
            parameters,
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    *used = used
        .checked_add(usize::try_from(bytes).map_err(unavailable)?)
        .filter(|bytes| *bytes <= BROWSE_TEXT_LIMIT)
        .ok_or(LibraryError::Unavailable)?;
    Ok(())
}
const PLAYLIST_TEXT_COLUMNS: &[&str] = &[
    "p.id",
    "p.name",
    "p.description",
    "p.coverImgUrl",
    "p.source",
    "COALESCE(strftime('%Y-%m-%d',p.createTime,'localtime'),'')",
];
const PLAYLIST_PAGE_ORDER: &str = "ORDER BY EXISTS(SELECT 1 FROM kv_store k WHERE k.key='favoritesId' AND k.value=p.id) DESC, p.createTime, p.id";
const SONG_TEXT_COLUMNS: &[&str] = &["songmid", "name", "singer", "albumName", "data"];

pub fn list_playlists(path: &Path) -> Result<Vec<Playlist>> {
    let connection = open(path)?;
    // A data folder that has never held a playlist has no tables yet: that is an empty list,
    // not a failure.
    connection.execute_batch(SCHEMA).map_err(unavailable)?;
    playlist_rows(&connection)
}

fn playlist_page_on(
    connection: &mut Connection,
    offset: i64,
    limit: i64,
    project_covers: bool,
) -> Result<(Vec<Playlist>, usize)> {
    let transaction = connection.transaction().map_err(unavailable)?;
    let total: usize = transaction
        .query_row("SELECT COUNT(*) FROM playlists", [], |row| row.get(0))
        .map_err(unavailable)?;
    let from = format!("FROM playlists p {PLAYLIST_PAGE_ORDER} LIMIT ?1 OFFSET ?2");
    let query = format!(
        "{} {PLAYLIST_PAGE_ORDER} LIMIT ?1 OFFSET ?2",
        playlist_query(project_covers)
    );
    check_page_text(
        &transaction,
        &from,
        params![limit, offset],
        playlist_columns(project_covers),
        &mut 0,
    )?;
    let mut statement = transaction.prepare(&query).map_err(unavailable)?;
    let rows = statement
        .query_map(params![limit, offset], playlist)
        .map_err(unavailable)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(unavailable)?;
    drop(statement);
    transaction.commit().map_err(unavailable)?;
    Ok((rows, total))
}

/// A read-only cursor which rejects writes between pages without retaining a WAL
/// transaction or materializing the whole library. Each cursor owns its SQLite
/// connection: data_version values are only comparable on the same connection.
pub struct LibraryCursor {
    path: std::path::PathBuf,
    identity: Option<DatabaseIdentity>,
    connection: Option<Connection>,
    version: i64,
    invalid: bool,
    project_covers: bool,
    sorted: Option<(String, crate::song_sort::Sort, Vec<String>)>,
}

#[derive(Clone, Copy)]
pub enum BrowseError {
    Changed,
    Unavailable,
    Capacity,
}
impl From<LibraryError> for BrowseError {
    fn from(_: LibraryError) -> Self {
        Self::Unavailable
    }
}

#[derive(PartialEq, Eq)]
struct DatabaseIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(not(unix))]
    modified: Option<std::time::SystemTime>,
    #[cfg(not(unix))]
    length: u64,
}
fn database_identity(path: &Path) -> std::result::Result<Option<DatabaseIdentity>, BrowseError> {
    let metadata = match std::fs::metadata(path) {
        Ok(value) if value.is_file() => value,
        Ok(_) => return Err(BrowseError::Unavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(BrowseError::Unavailable),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Some(DatabaseIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        }))
    }
    #[cfg(not(unix))]
    {
        Ok(Some(DatabaseIdentity {
            modified: metadata.modified().ok(),
            length: metadata.len(),
        }))
    }
}
fn data_version(connection: &Connection) -> std::result::Result<i64, BrowseError> {
    connection
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .map_err(|_| BrowseError::Unavailable)
}
impl LibraryCursor {
    pub fn open(path: &Path) -> std::result::Result<Self, BrowseError> {
        let identity = database_identity(path)?;
        let connection = if identity.is_some() {
            Some(open_read_only(path)?)
        } else {
            None
        };
        let version = connection
            .as_ref()
            .map(data_version)
            .transpose()?
            .unwrap_or(0);
        // A file replaced while opening must never be adopted silently.
        if database_identity(path)? != identity {
            return Err(BrowseError::Changed);
        }
        Ok(Self {
            path: path.into(),
            identity,
            connection,
            version,
            invalid: false,
            project_covers: false,
            sorted: None,
        })
    }
    pub fn open_mobile(path: &Path) -> std::result::Result<Self, BrowseError> {
        let mut cursor = Self::open(path)?;
        cursor.project_covers = true;
        Ok(cursor)
    }
    pub fn check(&mut self) -> std::result::Result<(), BrowseError> {
        self.validate()
    }
    /// Read only a bounded embedded original, on this same connection/version.
    pub fn cover_data(&mut self, id: &str) -> std::result::Result<Option<String>, BrowseError> {
        self.validate()?;
        let connection = self.connection.as_ref().ok_or(BrowseError::Unavailable)?;
        let result = connection.query_row(
            "SELECT coverImgUrl FROM playlists WHERE id=?1 AND substr(coverImgUrl,1,5)='data:' AND length(CAST(coverImgUrl AS BLOB))<=22369700",
            [id], |row| row.get(0)).optional().map_err(|_| BrowseError::Unavailable);
        self.validate()?;
        result
    }
    /// Preserve all song rows and timestamp semantics; same original cursor admission.
    pub fn save_cover(
        &mut self,
        id: &str,
        cover: &str,
        admitted: impl Fn() -> bool,
    ) -> std::result::Result<Playlist, BrowseError> {
        let mut result = None;
        self.mutate_playlist(
            || admitted(),
            |connection| {
                if connection
                    .execute(
                        "UPDATE playlists SET coverImgUrl=?2 WHERE id=?1",
                        params![id, cover],
                    )
                    .map_err(unavailable)?
                    != 1
                {
                    return Err(LibraryError::Unavailable);
                }
                check_page_text(
                    connection,
                    "FROM playlists p WHERE p.id=?1",
                    [id],
                    playlist_columns(true),
                    &mut 0,
                )?;
                result = Some(
                    connection
                        .query_row(
                            &format!("{} WHERE p.id=?1", playlist_query(true)),
                            [id],
                            playlist,
                        )
                        .map_err(unavailable)?,
                );
                if !admitted() {
                    return Err(LibraryError::Unavailable);
                }
                Ok(())
            },
        )?;
        result.ok_or(BrowseError::Unavailable)
    }
    fn validate(&mut self) -> std::result::Result<(), BrowseError> {
        if self.invalid {
            return Err(BrowseError::Changed);
        }
        if database_identity(&self.path)? != self.identity
            || self
                .connection
                .as_ref()
                .map(data_version)
                .transpose()?
                .unwrap_or(0)
                != self.version
        {
            self.invalid = true;
            return Err(BrowseError::Changed);
        }
        Ok(())
    }
    pub fn playlists(
        &mut self,
        offset: usize,
        limit: usize,
    ) -> std::result::Result<(Vec<Playlist>, usize), BrowseError> {
        self.validate()?;
        let offset = i64::try_from(offset).map_err(|_| BrowseError::Unavailable)?;
        let limit = i64::try_from(limit).map_err(|_| BrowseError::Unavailable)?;
        let result = match self.connection.as_mut() {
            Some(connection) => playlist_page_on(connection, offset, limit, self.project_covers)
                .map_err(BrowseError::from),
            None => Ok((Vec::new(), 0)),
        };
        // Validate even when a concurrent schema change caused the SQL to fail.
        self.validate()?;
        result
    }
    /// Delete only the playlist shown by this cursor. Acquire the writer lock
    /// before revalidating: another writer cannot slip between validation and
    /// deletion. Admission may cancel; an admitted transaction returns its actual
    /// outcome. No full Favorites map is needed by mobile.
    pub fn delete_playlist(
        &mut self,
        id: &str,
        admitted: impl FnOnce() -> bool,
    ) -> std::result::Result<(), BrowseError> {
        self.mutate_playlist(admitted, |connection| delete_playlist_on(connection, id))
    }

    pub fn remove_songs(
        &mut self,
        id: &str,
        song_ids: &[String],
        admitted: impl FnOnce() -> bool,
    ) -> std::result::Result<(), BrowseError> {
        self.mutate_playlist(admitted, |connection| {
            remove_playlist_songs_on(connection, id, song_ids)
        })
    }

    /// Copy only selected original members. All source reads, destination
    /// duplicate checks and optional playlist creation share the writer txn.
    pub fn copy_songs(
        &mut self,
        source_id: &str,
        song_ids: &[String],
        destination: PlaylistDestination,
        admitted: impl FnOnce() -> bool,
    ) -> std::result::Result<usize, BrowseError> {
        if song_ids.is_empty()
            || song_ids.len() > 200
            || song_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != song_ids.len()
        {
            return Err(BrowseError::Unavailable);
        }
        let mut added = 0;
        self.mutate_playlist(admitted, |connection| {
            let mut used = 0;
            let mut selected = Vec::with_capacity(song_ids.len());
            for id in song_ids {
                let raw = crate::online::parse_song_id(id)
                    .map(|(_, raw)| raw).unwrap_or_else(|| id.clone());
                check_page_text(connection,
                    "FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                    params![source_id,raw], &["songmid","data","name","singer","albumName","img"], &mut used)?;
                let row: SavedCopyRow = connection.query_row(
                    "SELECT songmid,data,name,singer,albumName,img FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                    params![source_id,raw], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))
                ).map_err(unavailable)?;
                let parsed = song_from_saved(row.0.clone(),row.2.clone(),row.3.clone(),row.4.clone(),&row.1);
                let data: serde_json::Value = serde_json::from_str(&row.1).map_err(unavailable)?;
                if parsed.song.id != *id || row.0.is_empty() || !data.is_object() ||
                    data.get("source").and_then(serde_json::Value::as_str).is_none() {
                    return Err(LibraryError::Unavailable);
                }
                selected.push((row, data));
            }
            added = append_saved_copy_on(connection, destination, selected, Some(&mut used))?.1;
            Ok(())
        })?;
        Ok(added)
    }

    /// Append captured online rows to an existing playlist on this original cursor.
    /// Original metadata/source identity and duplicate conflict semantics match saved copies.
    pub fn append_online_songs(
        &mut self,
        destination_id: &str,
        sources: &[&str],
        items: &[serde_json::Value],
        admitted: impl Fn() -> bool,
    ) -> std::result::Result<usize, BrowseError> {
        if items.is_empty() || items.len() > 200 || sources.is_empty() || sources.len() > 32 {
            return Err(BrowseError::Unavailable);
        }
        struct LimitedJson(Vec<u8>);
        impl std::io::Write for LimitedJson {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self
                    .0
                    .len()
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > BROWSE_TEXT_LIMIT)
                {
                    return Err(std::io::Error::other(
                        "selected metadata exceeds mobile limit",
                    ));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut used = 0usize;
        let mut seen = std::collections::HashSet::new();
        let mut selected = Vec::with_capacity(items.len());
        for item in items {
            let invalid = || BrowseError::Unavailable;
            let raw = item
                .get("songmid")
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| v.as_u64().map(|v| v.to_string()))
                })
                .ok_or_else(invalid)?;
            if raw.is_empty()
                || raw.len() > 512
                || raw.chars().any(char::is_control)
                || item
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|source| !sources.contains(&source))
                || !seen.insert(raw.clone())
            {
                return Err(invalid());
            }
            let field = |key: &str| -> std::result::Result<String, BrowseError> {
                match item.get(key) {
                    None | Some(serde_json::Value::Null) => Ok(String::new()),
                    Some(serde_json::Value::String(v))
                        if v.len() <= BROWSE_TEXT_LIMIT && !v.contains('\0') =>
                    {
                        Ok(v.clone())
                    }
                    _ => Err(invalid()),
                }
            };
            let mut json = LimitedJson(Vec::new());
            serde_json::to_writer(&mut json, item).map_err(|_| invalid())?;
            let row = (
                raw,
                String::from_utf8(json.0).map_err(|_| invalid())?,
                field("name")?,
                field("singer")?,
                field("albumName")?,
                field("img")?,
            );
            used = used
                .checked_add(
                    row.0.len()
                        + row.1.len()
                        + row.2.len()
                        + row.3.len()
                        + row.4.len()
                        + row.5.len(),
                )
                .filter(|n| *n <= BROWSE_TEXT_LIMIT)
                .ok_or_else(invalid)?;
            selected.push((row, item.clone()));
        }
        let mut added = 0;
        self.mutate_playlist(
            || admitted(),
            |connection| {
                added = append_saved_copy_on(
                    connection,
                    PlaylistDestination::Existing(destination_id.to_owned()),
                    selected,
                    Some(&mut used),
                )?
                .1;
                if !admitted() {
                    return Err(LibraryError::Unavailable);
                }
                Ok(())
            },
        )?;
        Ok(added)
    }

    /// All cursor-bound writes share existing-file opening, writer admission,
    /// rollback and invalidation. Admission runs before any row mutation.
    fn mutate_playlist(
        &mut self,
        admitted: impl FnOnce() -> bool,
        write: impl FnOnce(&Connection) -> Result<()>,
    ) -> std::result::Result<(), BrowseError> {
        self.validate()?;
        if self.connection.is_none() {
            return Err(BrowseError::Unavailable);
        }
        // No CREATE or persistent journal-mode change before admission. A file
        // removed/replaced after the first validation must not be recreated.
        let mut connection = Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| BrowseError::Unavailable)?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|_| BrowseError::Unavailable)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|_| BrowseError::Unavailable)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| BrowseError::Unavailable)?;
        self.validate()?;
        if !admitted() {
            return Err(BrowseError::Unavailable);
        }
        write(&transaction)?;
        // The writer lock excludes other SQL writers. Check file identity here,
        // without asking the reader for data_version: a large rollback-journal
        // DELETE may already hold EXCLUSIVE and would block our own reader.
        if database_identity(&self.path)? != self.identity {
            self.invalid = true;
            return Err(BrowseError::Changed);
        }
        transaction.commit().map_err(|_| BrowseError::Unavailable)?;
        mark_changed();
        self.invalid = true;
        self.sorted = None;
        Ok(())
    }

    /// Capture a bounded selection in the same database snapshot. Stable sort
    /// ties follow default position/songmid order, independent of click order.
    pub fn selected_songs(
        &mut self,
        id: &str,
        song_ids: &[String],
        sort: crate::song_sort::Sort,
    ) -> std::result::Result<Vec<PlaylistSong>, BrowseError> {
        self.validate()?;
        let result = match self.connection.as_mut() {
            Some(connection) => selected_songs_on(connection, id, song_ids, sort),
            None => Err(LibraryError::Unavailable),
        };
        // Includes writes/replacement during the selection read and sort.
        self.validate()?;
        result.map_err(BrowseError::from)
    }

    /// Resolve selection on the same connection/version as the displayed pages.
    /// The final validation is the selection's database linearization point;
    /// subsequent edits do not rewrite this captured row into another song.
    pub fn song(
        &mut self,
        playlist_id: &str,
        song_id: &str,
    ) -> std::result::Result<PlaylistSong, BrowseError> {
        self.validate()?;
        let result = match self.connection.as_ref() {
            Some(connection) => {
                playlist_song_on(connection, playlist_id, song_id, true).map_err(BrowseError::from)
            }
            None => Err(BrowseError::Unavailable),
        };
        self.validate()?;
        result
    }

    pub fn playlist(
        &mut self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> std::result::Result<PlaylistDetail, BrowseError> {
        self.validate()?;
        let offset = i64::try_from(offset).map_err(|_| BrowseError::Unavailable)?;
        let limit = i64::try_from(limit).map_err(|_| BrowseError::Unavailable)?;
        let result = match self.connection.as_mut() {
            Some(connection) => {
                playlist_range_on(connection, id, offset, limit, true, self.project_covers)
                    .map_err(BrowseError::from)
            }
            None => Err(BrowseError::Unavailable),
        };
        self.validate()?;
        result
    }

    /// Display-only sorting: retain raw row keys, never rewrite positions.
    /// The same connection/version guards key construction and every page.
    pub fn sorted_playlist(
        &mut self,
        id: &str,
        sort: crate::song_sort::Sort,
        offset: usize,
        limit: usize,
    ) -> std::result::Result<PlaylistDetail, BrowseError> {
        if sort == crate::song_sort::Sort::Default {
            self.sorted = None;
            return self.playlist(id, offset, limit);
        }
        self.validate()?;
        if !self
            .sorted
            .as_ref()
            .is_some_and(|(old_id, old_sort, _)| old_id == id && *old_sort == sort)
        {
            self.sorted = None;
            let result = match self.connection.as_ref() {
                Some(connection) => playlist_sort_order(connection, id, sort),
                None => Err(BrowseError::Unavailable),
            };
            self.validate()?;
            self.sorted = Some((id.to_owned(), sort, result?));
        }
        let keys = &self.sorted.as_ref().ok_or(BrowseError::Unavailable)?.2;
        let start = offset.min(keys.len());
        let end = start.saturating_add(limit).min(keys.len());
        let result = match self.connection.as_mut() {
            Some(connection) => {
                playlist_key_page_on(connection, id, &keys[start..end], self.project_covers)
            }
            None => Err(LibraryError::Unavailable),
        };
        self.validate()?;
        result.map_err(BrowseError::from)
    }
}

fn selected_songs_on(
    connection: &mut Connection,
    id: &str,
    song_ids: &[String],
    sort: crate::song_sort::Sort,
) -> Result<Vec<PlaylistSong>> {
    let requested: std::collections::HashSet<_> = song_ids.iter().map(String::as_str).collect();
    if song_ids.is_empty() || song_ids.len() > 200 || requested.len() != song_ids.len() {
        return Err(LibraryError::Unavailable);
    }
    let raw_ids: Vec<_> = song_ids
        .iter()
        .map(|id| {
            crate::online::parse_song_id(id)
                .map(|(_, raw)| raw)
                .unwrap_or_else(|| id.clone())
        })
        .collect();
    if raw_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != raw_ids.len()
    {
        return Err(LibraryError::Unavailable);
    }
    let from = format!(
        "FROM playlist_songs WHERE playlist_id=? AND songmid IN ({}) ORDER BY position,songmid",
        vec!["?"; raw_ids.len()].join(",")
    );
    let parameters = || {
        rusqlite::params_from_iter(std::iter::once(id).chain(raw_ids.iter().map(String::as_str)))
    };
    let transaction = connection.transaction().map_err(unavailable)?;
    let mut used = 0;
    check_page_text(
        &transaction,
        &from,
        parameters(),
        SONG_TEXT_COLUMNS,
        &mut used,
    )?;
    let mut statement = transaction
        .prepare(&format!("SELECT songmid,name,singer,albumName,data {from}"))
        .map_err(unavailable)?;
    let rows = statement
        .query_map(parameters(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(unavailable)?;
    let mut selected = Vec::with_capacity(song_ids.len());
    for row in rows {
        let (raw, title, artist, album, data) = row.map_err(unavailable)?;
        let row = song_from_saved(raw, title, artist, album, &data);
        if !requested.contains(row.song.id.as_str()) {
            return Err(LibraryError::Unavailable);
        }
        selected.push(row);
    }
    drop(statement);
    transaction.commit().map_err(unavailable)?;
    if selected.len() != song_ids.len() {
        return Err(LibraryError::Unavailable);
    }
    if sort != crate::song_sort::Sort::Default {
        let mut songs: Vec<_> = selected.iter().map(|row| row.song.clone()).collect();
        sort.apply(&mut songs);
        let mut by_id: HashMap<_, _> = selected
            .into_iter()
            .map(|row| (row.song.id.clone(), row))
            .collect();
        selected = songs
            .into_iter()
            .map(|song| by_id.remove(&song.id).ok_or(LibraryError::Unavailable))
            .collect::<Result<Vec<_>>>()?;
    }
    Ok(selected)
}

fn playlist_sort_order(
    connection: &Connection,
    id: &str,
    sort: crate::song_sort::Sort,
) -> std::result::Result<Vec<String>, BrowseError> {
    let duration = matches!(
        sort,
        crate::song_sort::Sort::DurationAsc | crate::song_sort::Sort::DurationDesc
    );
    // Read SQLite-owned values first. Check bounds before cloning key strings
    // or parsing one JSON record; no MusicItem/cover/provider data is retained.
    let mut statement = connection.prepare(
        "SELECT songmid,name,singer,albumName,CASE WHEN ?2 THEN data ELSE '' END FROM playlist_songs WHERE playlist_id=?1 ORDER BY position,songmid"
    ).map_err(|_| BrowseError::Unavailable)?;
    let mut rows = statement
        .query(params![id, duration])
        .map_err(|_| BrowseError::Unavailable)?;
    let mut songs = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = rows.next().map_err(|_| BrowseError::Unavailable)? {
        if songs.len() >= 50_000 {
            return Err(BrowseError::Capacity);
        }
        let text = |index| {
            row.get_ref(index)
                .map_err(|_| BrowseError::Unavailable)?
                .as_str()
                .map_err(|_| BrowseError::Unavailable)
        };
        let (key, title, artist, album, data) = (text(0)?, text(1)?, text(2)?, text(3)?, text(4)?);
        for field in [key, title, artist, album] {
            bytes = bytes
                .checked_add(field.len())
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or(BrowseError::Capacity)?;
        }
        if data.len() > 2 * 1024 * 1024 {
            return Err(BrowseError::Capacity);
        }
        let seconds = if duration {
            saved_duration(&serde_json::from_str(data).unwrap_or_default())
        } else {
            0
        };
        songs.push(Song {
            id: key.to_owned(),
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: album.to_owned(),
            duration: seconds,
            cover_art: None,
        });
    }
    drop(rows);
    drop(statement);
    // Stable ties follow the default page's position/songmid order, even DESC.
    sort.apply(&mut songs);
    Ok(songs.into_iter().map(|song| song.id).collect())
}

fn playlist_key_page_on(
    connection: &mut Connection,
    id: &str,
    keys: &[String],
    project_covers: bool,
) -> Result<PlaylistDetail> {
    let transaction = connection.transaction().map_err(unavailable)?;
    let mut used = 0;
    check_page_text(
        &transaction,
        "FROM playlists p WHERE p.id=?1",
        [id],
        playlist_columns(project_covers),
        &mut used,
    )?;
    for key in keys {
        check_page_text(
            &transaction,
            "FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
            params![id, key],
            SONG_TEXT_COLUMNS,
            &mut used,
        )?;
    }
    let playlist = transaction
        .query_row(
            &format!("{} WHERE p.id=?1", playlist_query(project_covers)),
            [id],
            playlist,
        )
        .map_err(unavailable)?;
    let songs = keys
        .iter()
        .map(|key| playlist_song_on(&transaction, id, key, true))
        .collect::<Result<Vec<_>>>()?;
    transaction.commit().map_err(unavailable)?;
    Ok(PlaylistDetail { playlist, songs })
}

fn playlist_rows(connection: &Connection) -> Result<Vec<Playlist>> {
    let mut statement = connection
        .prepare(&format!("{PLAYLIST_QUERY} ORDER BY 8 DESC, p.createTime"))
        .map_err(unavailable)?;
    statement
        .query_map([], playlist)
        .map_err(unavailable)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(unavailable)
}

/// Metadata-only mutation: song rows, cover, source and favorites ID stay intact.
pub fn save_playlist(
    path: &Path,
    id: Option<&str>,
    name: &str,
    description: &str,
) -> Result<Playlist> {
    save_playlist_checked(path, id, None, name, description)
}

/// Optimistic metadata edit. The comparison and update share an immediate
/// transaction so another editor cannot overwrite a newer name/description.
/// `None` preserves the desktop's existing unconditional save behavior.
pub fn save_playlist_checked(
    path: &Path,
    id: Option<&str>,
    expected: Option<(&str, &str)>,
    name: &str,
    description: &str,
) -> Result<Playlist> {
    let name = name.trim();
    let description = description.trim();
    // The create form uses HTML maxlength; TDesign's edit input counts Unicode scalars.
    let name_length = if id.is_some() {
        name.chars().count()
    } else {
        name.encode_utf16().count()
    };
    if name.is_empty() || name_length > 50 {
        return Err(LibraryError::Unavailable);
    }
    if description.encode_utf16().count() > 200 {
        return Err(LibraryError::Unavailable);
    }
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let id = if let Some(id) = id {
        if let Some(expected) = expected {
            let current: (String, String) = transaction
                .query_row(
                    "SELECT name,description FROM playlists WHERE id=?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(unavailable)?;
            if (current.0.as_str(), current.1.as_str()) != expected {
                return Err(LibraryError::Unavailable);
            }
        }
        let changed = transaction.execute(
            "UPDATE playlists SET name=?2,description=?3,updateTime=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            params![id,name,description],
        ).map_err(unavailable)?;
        if changed != 1 {
            return Err(LibraryError::Unavailable);
        }
        id.to_owned()
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO playlists (id,name,description,source,createTime,updateTime)
             VALUES (?1,?2,?3,'local',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            params![id,name,description],
        ).map_err(unavailable)?;
        id
    };
    let saved = transaction
        .query_row(&format!("{PLAYLIST_QUERY} WHERE p.id=?1"), [&id], playlist)
        .map_err(unavailable)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(saved)
}

/// Only the cover changes, matching the original update_cover timestamp behavior.
pub fn save_playlist_cover(path: &Path, id: &str, cover: &str) -> Result<Playlist> {
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    let changed = transaction
        .execute(
            "UPDATE playlists SET coverImgUrl=?2 WHERE id=?1",
            params![id, cover],
        )
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(LibraryError::Unavailable);
    }
    let saved = transaction
        .query_row(&format!("{PLAYLIST_QUERY} WHERE p.id=?1"), [id], playlist)
        .map_err(unavailable)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(saved)
}

/// One stored MusicItem as a playable song, the way playlists read their rows. `data` may be
/// malformed JSON: the display columns then stand in, and nothing is guessed.
pub fn song_from_saved(
    id: String,
    title: String,
    artist: String,
    album: String,
    data: &str,
) -> PlaylistSong {
    let data: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
    // Online-source rows are keyed by their source and raw ID like everywhere else in the
    // app; the stored MusicItem lets the song be resolved without searching again.
    let online_row = online_source(&data).map(str::to_owned);
    let id = match &online_row {
        Some(source) => crate::online::song_id(source, &id),
        None => id,
    };
    let identity = data
        .get("_mioGpuiSource")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let duration = saved_duration(&data);
    let types = data.get("types").and_then(serde_json::Value::as_array);
    let quality = data
        .get("_mioGpuiQuality")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .or_else(|| {
            types.and_then(|types| {
                if types.iter().any(|kind| kind.as_str() == Some("flac")) {
                    Some(SongQuality::Lossless)
                } else if types.iter().any(|kind| kind.as_str() == Some("320k")) {
                    Some(SongQuality::High)
                } else if types.iter().any(|kind| kind.as_str() == Some("128k")) {
                    Some(SongQuality::Standard)
                } else {
                    None
                }
            })
        });
    PlaylistSong {
        song: Song {
            id,
            title,
            artist,
            album,
            duration,
            cover_art: data
                .get("_mioGpuiCoverArt")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    online_row
                        .as_ref()
                        .and_then(|_| data.get("img").and_then(serde_json::Value::as_str))
                        .filter(|img| img.starts_with("http"))
                        .map(str::to_owned)
                }),
        },
        source: data
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("local")
            .to_owned(),
        identity,
        quality,
        online: online_row.as_ref().map(|_| data.clone()),
    }
}

fn saved_duration(data: &serde_json::Value) -> u64 {
    data.get("_mioGpuiDuration")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| {
            data.get("interval")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .split(':')
                .try_fold(0_u64, |seconds, part| {
                    seconds.checked_mul(60)?.checked_add(part.parse().ok()?)
                })
                .unwrap_or(0)
        })
}

pub fn load_playlist(path: &Path, id: &str) -> Result<PlaylistDetail> {
    load_playlist_range(path, id, None)
}

pub fn load_playlist_song(path: &Path, playlist_id: &str, song_id: &str) -> Result<PlaylistSong> {
    let connection = open_read_only(path)?;
    playlist_song_on(&connection, playlist_id, song_id, false)
}
fn playlist_song_on(
    connection: &Connection,
    playlist_id: &str,
    song_id: &str,
    bounded: bool,
) -> Result<PlaylistSong> {
    // Do not materialize or parse oversized saved JSON on the mobile bridge.
    // Reject the whole row rather than truncating a song's metadata.
    let query = if bounded {
        "SELECT songmid,name,singer,albumName,data FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2 AND length(CAST(songmid AS BLOB))+length(CAST(COALESCE(name,'') AS BLOB))+length(CAST(COALESCE(singer,'') AS BLOB))+length(CAST(COALESCE(albumName,'') AS BLOB))+length(CAST(data AS BLOB)) <= 2097152"
    } else {
        "SELECT songmid,name,singer,albumName,data FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2"
    };
    let (id, title, artist, album, data) = connection
        .query_row(query, params![playlist_id, song_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(unavailable)?;
    Ok(song_from_saved(id, title, artist, album, &data))
}

fn load_playlist_range(
    path: &Path,
    id: &str,
    page: Option<(usize, usize)>,
) -> Result<PlaylistDetail> {
    let (offset, limit) = match page {
        Some((offset, limit)) => (
            i64::try_from(offset).map_err(unavailable)?,
            i64::try_from(limit).map_err(unavailable)?,
        ),
        None => (0, -1),
    };
    let mut connection = open_read_only(path)?;
    playlist_range_on(&mut connection, id, offset, limit, page.is_some(), false)
}

fn playlist_range_on(
    connection: &mut Connection,
    id: &str,
    offset: i64,
    limit: i64,
    bounded: bool,
    project_covers: bool,
) -> Result<PlaylistDetail> {
    // One consistent snapshot for metadata and songs; this reads existing data only.
    let transaction = connection.transaction().map_err(unavailable)?;
    let metadata_query = format!("{} WHERE p.id=?1", playlist_query(project_covers));
    let songs_query = "SELECT songmid,name,singer,albumName,data FROM playlist_songs WHERE playlist_id=?1 ORDER BY position, songmid LIMIT ?2 OFFSET ?3";
    if bounded {
        let mut used = 0;
        check_page_text(
            &transaction,
            "FROM playlists p WHERE p.id=?1",
            [id],
            playlist_columns(project_covers),
            &mut used,
        )?;
        check_page_text(
            &transaction,
            "FROM playlist_songs WHERE playlist_id=?1 ORDER BY position, songmid LIMIT ?2 OFFSET ?3",
            params![id, limit, offset],
            SONG_TEXT_COLUMNS,
            &mut used,
        )?;
    }
    let playlist = transaction
        .query_row(&metadata_query, [id], playlist)
        .map_err(unavailable)?;
    let mut statement = transaction.prepare(songs_query).map_err(unavailable)?;
    let rows = statement
        .query_map(params![id, limit, offset], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(unavailable)?;
    let mut songs = Vec::new();
    for row in rows {
        let (id, title, artist, album, data) = row.map_err(unavailable)?;
        songs.push(song_from_saved(id, title, artist, album, &data));
    }
    drop(statement);
    transaction.commit().map_err(unavailable)?;
    Ok(PlaylistDetail { playlist, songs })
}

pub fn delete_playlist(path: &Path, id: &str) -> Result<Favorites> {
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    delete_playlist_on(&transaction, id)?;
    let favorites = read(&transaction)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(favorites)
}

// The FK removes this playlist's rows; the favorite pointer is cleared in
// the same transaction. Audio files and the playback queue are independent.
fn delete_playlist_on(connection: &Connection, id: &str) -> Result<()> {
    let changed = connection
        .execute("DELETE FROM playlists WHERE id=?1", [id])
        .map_err(unavailable)?;
    if changed != 1 {
        return Err(LibraryError::Unavailable);
    }
    // FK removes only this playlist's songs. Remove a matching favorite pointer
    // in the same commit so subsequent favorites can create a valid playlist.
    connection
        .execute(
            "DELETE FROM kv_store WHERE key='favoritesId' AND value=?1",
            [id],
        )
        .map_err(unavailable)?;
    Ok(())
}

pub fn remove_playlist_songs(path: &Path, id: &str, song_ids: &[String]) -> Result<Favorites> {
    if song_ids.is_empty() {
        return Err(LibraryError::Unavailable);
    }
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    remove_playlist_songs_on(&transaction, id, song_ids)?;
    // Desktop retains its full favorite snapshot in the same commit.
    let favorites = read(&transaction)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(favorites)
}

fn remove_playlist_songs_on(connection: &Connection, id: &str, song_ids: &[String]) -> Result<()> {
    if song_ids.is_empty() {
        return Err(LibraryError::Unavailable);
    }
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
            [id],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    if !exists {
        return Err(LibraryError::Unavailable);
    }
    for song_id in song_ids {
        let changed = match crate::online::parse_song_id(song_id) {
            // Online rows are keyed by the raw songmid; the source disambiguates collisions.
            Some((source, raw)) => execute_cached(
                connection,
                "DELETE FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2
                 AND json_extract(data,'$.source')=?3 AND json_extract(data,'$._mioGpuiSource') IS NULL",
                params![id, raw, source],
            ),
            None => execute_cached(
                connection,
                "DELETE FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                params![id, song_id],
            ),
        }
        .map_err(unavailable)?;
        // A stale member or duplicate request must not commit a partial batch.
        if changed != 1 {
            return Err(LibraryError::Unavailable);
        }
    }
    // Preserve original positions and playlist timestamps, including gaps.
    Ok(())
}

pub fn add_playlist_songs(
    path: &Path,
    identity: &SessionIdentity,
    destination: PlaylistDestination,
    songs: &[PlaylistTrack],
) -> Result<(Playlist, Favorites)> {
    if songs.is_empty() {
        return Err(LibraryError::Unavailable);
    }
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let id = match destination {
        PlaylistDestination::Existing(id) => {
            let exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(unavailable)?;
            if !exists {
                return Err(LibraryError::Unavailable);
            }
            id
        }
        PlaylistDestination::New(name) => {
            let name = name.trim();
            // The original inline create-and-add input has no maxlength.
            if name.is_empty() {
                return Err(LibraryError::Unavailable);
            }
            let id = uuid::Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO playlists (id,name,source,createTime,updateTime)
                 VALUES (?1,?2,'local',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                params![id,name],
            ).map_err(unavailable)?;
            id
        }
    };
    let max_position: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(position),-1) FROM playlist_songs WHERE playlist_id=?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    for (index, track) in songs.iter().enumerate() {
        let song = &track.song;
        let old = saved_song_json(&transaction, params![id, song.id]).map_err(unavailable)?;
        if let Some(old) = old {
            let old: serde_json::Value = serde_json::from_str(&old).map_err(unavailable)?;
            let old_identity: Option<SessionIdentity> = old
                .get("_mioGpuiSource")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(unavailable)?;
            let expected_source = if crate::local_music::is_local(song) {
                "local"
            } else {
                "subsonic"
            };
            if old.get("source").and_then(serde_json::Value::as_str) != Some(expected_source)
                || old_identity.as_ref() != Some(identity)
            {
                return Err(LibraryError::DifferentSource);
            }
            continue;
        }
        let position = max_position
            .checked_add(1)
            .and_then(|position| {
                i64::try_from(index)
                    .ok()
                    .and_then(|index| position.checked_add(index))
            })
            .ok_or(LibraryError::Unavailable)?;
        execute_cached(
            &transaction,
            "INSERT INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img)
             VALUES (?1,?2,?3,?4,?5,?6,?7,'')",
            params![id,song.id,position,saved_song_data(identity,song,track.quality),song.title,song.artist,song.album],
        ).map_err(unavailable)?;
    }
    let playlist = transaction
        .query_row(&format!("{PLAYLIST_QUERY} WHERE p.id=?1"), [&id], playlist)
        .map_err(unavailable)?;
    let favorites = read(&transaction)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok((playlist, favorites))
}

type SavedCopyRow = (String, String, String, String, String, String);

/// Common duplicate/source checks and raw-data copy for desktop and mobile.
/// Optional mobile budget includes source rows and existing destination data.
fn append_saved_copy_on(
    transaction: &Connection,
    destination: PlaylistDestination,
    selected: Vec<(SavedCopyRow, serde_json::Value)>,
    mut text_used: Option<&mut usize>,
) -> Result<(String, usize)> {
    let id = match destination {
        PlaylistDestination::Existing(id) => {
            let exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(unavailable)?;
            if !exists {
                return Err(LibraryError::Unavailable);
            }
            id
        }
        PlaylistDestination::New(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(LibraryError::Unavailable);
            }
            let id = uuid::Uuid::new_v4().to_string();
            transaction.execute("INSERT INTO playlists (id,name,source,createTime,updateTime) VALUES (?1,?2,'local',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))", params![id,name]).map_err(unavailable)?;
            id
        }
    };
    let mut position: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(position),-1) FROM playlist_songs WHERE playlist_id=?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    let mut added = 0;
    for (row, data) in selected {
        if let Some(used) = text_used.as_deref_mut() {
            check_page_text(
                transaction,
                "FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                params![id, row.0],
                &["data"],
                used,
            )?;
        }
        let old = saved_song_json(transaction, params![id, row.0]).map_err(unavailable)?;
        if let Some(old) = old {
            let old: serde_json::Value = serde_json::from_str(&old).map_err(unavailable)?;
            if !old.is_object()
                || old.get("source") != data.get("source")
                || old.get("_mioGpuiSource") != data.get("_mioGpuiSource")
            {
                return Err(LibraryError::DifferentSource);
            }
            continue;
        }
        position = position.checked_add(1).ok_or(LibraryError::Unavailable)?;
        execute_cached(transaction, "INSERT INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![id,row.0,position,row.1,row.2,row.3,row.4,row.5]).map_err(unavailable)?;
        added += 1;
    }
    Ok((id, added))
}

pub fn saved_song_data(
    identity: &SessionIdentity,
    song: &Song,
    quality: Option<SongQuality>,
) -> String {
    let types = match quality {
        Some(SongQuality::Lossless) => vec!["flac", "128k"],
        Some(SongQuality::High) => vec!["320k", "128k"],
        Some(SongQuality::Standard) => vec!["128k"],
        None => vec![],
    };
    serde_json::json!({
        "songmid": song.id, "name": song.title, "singer": song.artist,
        "albumName": song.album, "img": "", "source": if crate::local_music::is_local(song) { "local" } else { "subsonic" }, "url": "",
        "interval": format!("{:02}:{:02}", song.duration / 60, song.duration % 60),
        "types": types, "_mioGpuiSource": identity,
        "_mioGpuiCoverArt": song.cover_art, "_mioGpuiDuration": song.duration,
    })
    .to_string()
}

pub enum PlaylistImportError {
    Parse,
    Format,
    Save,
}

pub fn import_playlist_file(
    path: &Path,
    file: &Path,
    description: &str,
) -> std::result::Result<(Playlist, Favorites, usize), PlaylistImportError> {
    use std::io::Read;
    // This page reads plain JSON, including files named .cpl/.cmpl. The
    // encrypted queue-settings format is a separate original workflow.
    let extension = file.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if !["json", "cpl", "cmpl"]
        .iter()
        .any(|ext| extension.eq_ignore_ascii_case(ext))
    {
        return Err(PlaylistImportError::Format);
    }
    const MAX_BYTES: u64 = 32 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(file)
        .map_err(|_| PlaylistImportError::Save)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PlaylistImportError::Save)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(PlaylistImportError::Format);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| PlaylistImportError::Parse)?;
    let songs = value.as_array().ok_or(PlaylistImportError::Format)?;
    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or(PlaylistImportError::Format)?;
    create_imported_playlist(path, Some(name), description, songs, "local", None)
        .map_err(|_| PlaylistImportError::Save)
}

pub fn import_current_queue(
    path: &Path,
    identity: &SessionIdentity,
    tracks: &[PlaylistTrack],
    description: &str,
) -> Result<(Playlist, Favorites, usize)> {
    if tracks.is_empty() {
        return Err(LibraryError::Unavailable);
    }
    let songs = tracks
        .iter()
        .map(|track| {
            serde_json::from_str(&saved_song_data(identity, &track.song, track.quality))
                .map_err(unavailable)
        })
        .collect::<Result<Vec<_>>>()?;
    create_imported_playlist(path, None, description, &songs, "local", None)
}

/// A playlist imported from a platform (`songlist.vue` `handleNetworkPlaylistImport`): the new
/// playlist's `source` is the platform and its cover the platform playlist's, the rows are the
/// songs' MusicItem JSON.
pub fn import_network_playlist(
    path: &Path,
    name: &str,
    description: &str,
    source: &str,
    cover: Option<&str>,
    songs: &[serde_json::Value],
) -> Result<(Playlist, Favorites, usize)> {
    if songs.is_empty() || name.trim().is_empty() {
        return Err(LibraryError::Unavailable);
    }
    create_imported_playlist(path, Some(name), description, songs, source, cover)
}

fn create_imported_playlist(
    path: &Path,
    name: Option<&str>,
    description: &str,
    songs: &[serde_json::Value],
    source: &str,
    cover: Option<&str>,
) -> Result<(Playlist, Favorites, usize)> {
    create_imported_playlist_when(path, name, description, songs, source, cover, || false)
}

fn create_imported_playlist_when(
    path: &Path,
    name: Option<&str>,
    description: &str,
    songs: &[serde_json::Value],
    source: &str,
    cover: Option<&str>,
    cancelled: impl Fn() -> bool,
) -> Result<(Playlist, Favorites, usize)> {
    create_imported_playlist_with_favorites(
        path,
        name,
        description,
        songs,
        source,
        cover,
        cancelled,
        true,
    )
}

/// Save a new online playlist without loading every unrelated favorite row.
/// The caller has already bounded and fixed the source's retained metadata.
pub fn save_network_playlist_when(
    path: &Path,
    name: &str,
    description: &str,
    source: &str,
    cover: Option<&str>,
    songs: &[serde_json::Value],
    cancelled: impl Fn() -> bool,
) -> Result<(Playlist, usize)> {
    if songs.is_empty() || name.trim().is_empty() {
        return Err(LibraryError::Unavailable);
    }
    let (playlist, _, count) = create_imported_playlist_with_favorites(
        path,
        Some(name),
        description,
        songs,
        source,
        cover,
        cancelled,
        false,
    )?;
    Ok((playlist, count))
}

/// Import bounded, already parsed file rows without reading unrelated favorites.
/// The host owns file selection; source identity fields inside rows are retained.
pub fn save_file_playlist_when(
    path: &Path,
    name: &str,
    description: &str,
    songs: &[serde_json::Value],
    cancelled: impl Fn() -> bool,
) -> Result<(Playlist, usize)> {
    let (playlist, _, count) = create_imported_playlist_with_favorites(
        path,
        Some(name),
        description,
        songs,
        "local",
        None,
        cancelled,
        false,
    )?;
    Ok((playlist, count))
}

fn create_imported_playlist_with_favorites(
    path: &Path,
    name: Option<&str>,
    description: &str,
    songs: &[serde_json::Value],
    source: &str,
    cover: Option<&str>,
    cancelled: impl Fn() -> bool,
    read_favorites: bool,
) -> Result<(Playlist, Favorites, usize)> {
    // Validate all rows before any mutation. Legacy JSON keeps its original
    // identity (or absence of one); never assign the current server to it.
    let rows = songs
        .iter()
        .map(|song| {
            let object = song.as_object().ok_or(LibraryError::Unavailable)?;
            let id = match object.get("songmid") {
                Some(serde_json::Value::String(id)) => id.clone(),
                Some(serde_json::Value::Number(id)) => id.to_string(),
                _ => return Err(LibraryError::Unavailable),
            };
            let field = |key| -> Result<String> {
                match object.get(key) {
                    None | Some(serde_json::Value::Null) => Ok(String::new()),
                    Some(serde_json::Value::String(value)) => Ok(value.clone()),
                    _ => Err(LibraryError::Unavailable),
                }
            };
            Ok((
                id,
                song.to_string(),
                field("name")?,
                field("singer")?,
                field("albumName")?,
                field("img")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    if cancelled() {
        return Err(LibraryError::Unavailable);
    }
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let name = match name {
        Some(name) => name.to_owned(),
        // The original uses this Chinese timestamp name in both locales.
        None => transaction
            .query_row(
                "SELECT '播放列表 ' || strftime('%Y-%m-%d %H:%M','now','localtime')",
                [],
                |row| row.get(0),
            )
            .map_err(unavailable)?,
    };
    let id = uuid::Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO playlists (id,name,description,coverImgUrl,source,createTime,updateTime)
         VALUES (?1,?2,?3,COALESCE(?4,'default-cover'),?5,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        params![id,name,description,cover.filter(|c| !c.is_empty()),source],
    ).map_err(unavailable)?;
    for (position, (songmid, data, name, singer, album, img)) in rows.iter().enumerate() {
        let position = i64::try_from(position).map_err(unavailable)?;
        execute_cached(
            &transaction,
            "INSERT OR IGNORE INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id,songmid,position,data,name,singer,album,img],
        ).map_err(unavailable)?;
    }
    let playlist = transaction
        .query_row(&format!("{PLAYLIST_QUERY} WHERE p.id=?1"), [&id], playlist)
        .map_err(unavailable)?;
    let favorites = if read_favorites {
        read(&transaction)?
    } else {
        Favorites::default()
    };
    if cancelled() {
        return Err(LibraryError::Unavailable);
    }
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    // Match the original notification's requested count, including duplicates.
    Ok((playlist, favorites, songs.len()))
}

/// Every playlist and its songs in the original's `PlaylistBackup` layout
/// (`{playlists:[{playlist:{…,songCount}, songs:[{playlist_id,position,data,songmid,…}]}]}`),
/// the playlist part of an S3 backup.
pub fn export_backup(path: &Path) -> Result<serde_json::Value> {
    let connection = open(path)?;
    connection.execute_batch(SCHEMA).map_err(unavailable)?;
    let mut playlists = connection
        .prepare(
            "SELECT id,name,description,coverImgUrl,source,meta,createTime,updateTime,
             (SELECT COUNT(*) FROM playlist_songs s WHERE s.playlist_id=p.id)
             FROM playlists p ORDER BY createTime, id",
        )
        .map_err(unavailable)?;
    let mut songs = connection
        .prepare(
            "SELECT playlist_id,position,data,songmid,name,singer,albumName,img
             FROM playlist_songs WHERE playlist_id=?1 ORDER BY position",
        )
        .map_err(unavailable)?;
    let rows = playlists
        .query_map([], |row| {
            Ok((
                serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "coverImgUrl": row.get::<_, String>(3)?,
                    "source": row.get::<_, String>(4)?,
                    "meta": row.get::<_, String>(5)?,
                    "createTime": row.get::<_, String>(6)?,
                    "updateTime": row.get::<_, String>(7)?,
                    "songCount": row.get::<_, i64>(8)?,
                }),
                row.get::<_, String>(0)?,
            ))
        })
        .map_err(unavailable)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(unavailable)?;
    let mut items = Vec::with_capacity(rows.len());
    for (playlist, id) in rows {
        let songs = songs
            .query_map([&id], |row| {
                Ok(serde_json::json!({
                    "playlist_id": row.get::<_, String>(0)?,
                    "position": row.get::<_, i64>(1)?,
                    "data": row.get::<_, String>(2)?,
                    "songmid": row.get::<_, String>(3)?,
                    "name": row.get::<_, String>(4)?,
                    "singer": row.get::<_, String>(5)?,
                    "albumName": row.get::<_, String>(6)?,
                    "img": row.get::<_, String>(7)?,
                }))
            })
            .map_err(unavailable)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(unavailable)?;
        items.push(serde_json::json!({ "playlist": playlist, "songs": songs }));
    }
    Ok(serde_json::json!({ "playlists": items }))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BackupFile {
    playlists: Vec<BackupItem>,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct BackupItem {
    playlist: BackupPlaylist,
    #[serde(default)]
    songs: Vec<BackupSong>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupPlaylist {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_cover")]
    cover_img_url: String,
    source: String,
    #[serde(default = "default_meta")]
    meta: String,
    create_time: String,
    update_time: String,
}
fn default_cover() -> String {
    "default-cover".into()
}
fn default_meta() -> String {
    "{}".into()
}
#[derive(serde::Serialize, serde::Deserialize)]
struct BackupSong {
    songmid: String,
    name: String,
    #[serde(default)]
    position: i64,
    #[serde(default)]
    data: String,
    #[serde(default)]
    singer: String,
    #[serde(default, rename = "albumName")]
    album_name: String,
    #[serde(default)]
    img: String,
}

/// Checks the `playlists` value of a backup and returns how many playlists and songs it holds.
/// Nothing is written.
pub fn check_backup(backup: &serde_json::Value) -> Option<(usize, usize)> {
    const MAX_SONGS: usize = 2_000_000;
    let file: BackupFile = serde_json::from_value(backup.clone()).ok()?;
    let songs: usize = file.playlists.iter().map(|item| item.songs.len()).sum();
    let valid = file.playlists.iter().all(|item| {
        let playlist = &item.playlist;
        !playlist.id.is_empty()
            && playlist.id.len() <= 256
            && item.songs.iter().all(|song| !song.songmid.is_empty())
    });
    (valid && songs <= MAX_SONGS).then_some((file.playlists.len(), songs))
}

/// `restore_playlists` of the original: `overwrite` replaces every playlist, otherwise (merge)
/// playlists and songs that are missing are added after what is already there. One transaction.
/// After an overwrite a favorites pointer to a playlist the backup did not contain is dropped,
/// since the rest of the app treats a dangling pointer as an unusable library.
pub fn restore_backup(path: &Path, backup: &serde_json::Value, overwrite: bool) -> Result<()> {
    let file: BackupFile = serde_json::from_value(backup.clone()).map_err(unavailable)?;
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    if overwrite {
        transaction
            .execute("DELETE FROM playlist_songs", [])
            .map_err(unavailable)?;
        transaction
            .execute("DELETE FROM playlists", [])
            .map_err(unavailable)?;
    }
    for item in &file.playlists {
        let playlist = &item.playlist;
        transaction
            .execute(
                "INSERT OR IGNORE INTO playlists (id,name,description,coverImgUrl,source,meta,createTime,updateTime)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    playlist.id,
                    playlist.name,
                    playlist.description,
                    playlist.cover_img_url,
                    playlist.source,
                    playlist.meta,
                    playlist.create_time,
                    playlist.update_time
                ],
            )
            .map_err(unavailable)?;
        let mut next = if overwrite {
            0
        } else {
            transaction
                .query_row(
                    "SELECT COALESCE(MAX(position),-1)+1 FROM playlist_songs WHERE playlist_id=?1",
                    [&playlist.id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(unavailable)?
        };
        for song in &item.songs {
            let position = if overwrite { song.position } else { next };
            let inserted = execute_cached(
                    &transaction,
                    "INSERT OR IGNORE INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        playlist.id,
                        song.songmid,
                        position,
                        song.data,
                        song.name,
                        song.singer,
                        song.album_name,
                        song.img
                    ],
                )
                .map_err(unavailable)?;
            if !overwrite && inserted > 0 {
                next += 1;
            }
        }
    }
    transaction
        .execute(
            "DELETE FROM kv_store WHERE key='favoritesId'
             AND value NOT IN (SELECT id FROM playlists)",
            [],
        )
        .map_err(unavailable)?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(unavailable)?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(unavailable)?;
    Ok(connection)
}

fn open(path: &Path) -> Result<Connection> {
    let parent = path.parent().ok_or(LibraryError::Unavailable)?;
    std::fs::create_dir_all(parent).map_err(unavailable)?;
    let connection = Connection::open(path).map_err(unavailable)?;
    // SQLite may silently fall back to read-only. Stop before any PRAGMA or
    // transaction can create a read-only WAL that blocks the next write after
    // the database's permissions recover. Pure readers use open_read_only.
    if connection
        .is_readonly(rusqlite::DatabaseName::Main)
        .map_err(unavailable)?
    {
        return Err(LibraryError::Unavailable);
    }
    connection
        .busy_timeout(Duration::from_secs(2))
        .map_err(unavailable)?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )
        .map_err(unavailable)?;
    Ok(connection)
}

fn read(connection: &Connection) -> Result<Favorites> {
    let id: Option<String> = connection
        .query_row(
            "SELECT value FROM kv_store WHERE key='favoritesId'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(unavailable)?;
    let mut favorites = Favorites {
        id,
        songs: HashMap::new(),
        online: Default::default(),
    };
    if let Some(id) = &favorites.id {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(unavailable)?;
        if !exists {
            return Err(LibraryError::Unavailable);
        }
        let mut statement = connection
            .prepare(
                "SELECT songmid, data FROM playlist_songs WHERE playlist_id=?1 ORDER BY position",
            )
            .map_err(unavailable)?;
        let rows = statement
            .query_map([id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(unavailable)?;
        for row in rows {
            let (id, data) = row.map_err(unavailable)?;
            let data: serde_json::Value = serde_json::from_str(&data).map_err(unavailable)?;
            // Legacy/imported rows without our metadata are preserved but cannot
            // be attributed to a server safely. A colliding toggle is rejected.
            let identity = data
                .get("_mioGpuiSource")
                .map(|value| {
                    serde_json::from_value::<SessionIdentity>(value.clone()).map_err(unavailable)
                })
                .transpose()?;
            if let Some(source) = online_source(&data) {
                favorites.online.insert((source.to_owned(), id.clone()));
            }
            favorites.songs.insert(id, identity);
        }
    }
    Ok(favorites)
}

/// Read one song only. The surrounding transaction keeps the pointer, identity
/// and row in one SQLite snapshot; unrelated favorites are never materialized.
fn read_selected(connection: &Connection, songmid: &str) -> Result<Favorites> {
    if songmid.is_empty() || songmid.len() > 4096 {
        return Err(LibraryError::Unavailable);
    }
    let id_bytes: Option<i64> = connection
        .query_row(
            "SELECT length(CAST(value AS BLOB)) FROM kv_store WHERE key='favoritesId'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(unavailable)?;
    let Some(bytes) = id_bytes else {
        return Ok(Favorites::default());
    };
    if !(1..=4096).contains(&bytes) {
        return Err(LibraryError::Unavailable);
    }
    let id: String = connection
        .query_row(
            "SELECT value FROM kv_store WHERE key='favoritesId'",
            [],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)",
            [&id],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    if !exists {
        return Err(LibraryError::Unavailable);
    }
    let mut result = Favorites {
        id: Some(id.clone()),
        ..Default::default()
    };
    let bytes: Option<i64> = connection.query_row(
        "SELECT length(CAST(data AS BLOB)) FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
        params![id,songmid], |row| row.get(0),
    ).optional().map_err(unavailable)?;
    let Some(bytes) = bytes else {
        return Ok(result);
    };
    if !(0..=2 * 1024 * 1024).contains(&bytes) {
        return Err(LibraryError::Unavailable);
    }
    let data: String = connection
        .query_row(
            "SELECT data FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
            params![id, songmid],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    let data: serde_json::Value = serde_json::from_str(&data).map_err(unavailable)?;
    let identity = data
        .get("_mioGpuiSource")
        .map(|value| serde_json::from_value::<SessionIdentity>(value.clone()).map_err(unavailable))
        .transpose()?;
    if let Some(source) = online_source(&data) {
        result
            .online
            .insert((source.to_owned(), songmid.to_owned()));
    }
    result.songs.insert(songmid.to_owned(), identity);
    Ok(result)
}

fn read_projection(connection: &Connection, songmid: Option<&str>) -> Result<Favorites> {
    match songmid {
        Some(songmid) => read_selected(connection, songmid),
        None => read(connection),
    }
}

pub(crate) fn load_selected_favorite(path: &Path, songmid: &str) -> Result<Favorites> {
    if !path.try_exists().map_err(unavailable)? {
        return Ok(Favorites::default());
    }
    let mut connection = open_read_only(path)?;
    let transaction = connection.transaction().map_err(unavailable)?;
    let result = read_selected(&transaction, songmid)?;
    transaction.commit().map_err(unavailable)?;
    Ok(result)
}

pub fn load_favorites(path: &Path) -> Result<Favorites> {
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let favorites = read(&transaction)?;
    transaction.commit().map_err(unavailable)?;
    Ok(favorites)
}

pub fn set_favorite(
    path: &Path,
    identity: &SessionIdentity,
    song: &Song,
    quality: Option<SongQuality>,
    liked: bool,
    name: &str,
    description: &str,
) -> Result<Favorites> {
    set_favorite_projection(
        path,
        identity,
        song,
        quality,
        liked,
        name,
        description,
        false,
    )
}

pub(crate) fn set_selected_favorite(
    path: &Path,
    identity: &SessionIdentity,
    song: &Song,
    quality: Option<SongQuality>,
    liked: bool,
    name: &str,
    description: &str,
) -> Result<Favorites> {
    set_favorite_projection(
        path,
        identity,
        song,
        quality,
        liked,
        name,
        description,
        true,
    )
}

fn set_favorite_projection(
    path: &Path,
    identity: &SessionIdentity,
    song: &Song,
    quality: Option<SongQuality>,
    liked: bool,
    name: &str,
    description: &str,
    selected_only: bool,
) -> Result<Favorites> {
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let previous = read_projection(&transaction, selected_only.then_some(song.id.as_str()))?;
    if previous
        .songs
        .get(&song.id)
        .is_some_and(|saved| saved.as_ref() != Some(identity))
    {
        return Err(LibraryError::DifferentSource);
    }
    let id = match previous.id {
        Some(id) => id,
        None if !liked => return Ok(previous),
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO playlists (id,name,description,source,createTime,updateTime)
                 VALUES (?1,?2,?3,'local',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                params![id,name,description],
            ).map_err(unavailable)?;
            transaction
                .execute(
                    "INSERT INTO kv_store (key,value) VALUES ('favoritesId',?1)",
                    [&id],
                )
                .map_err(unavailable)?;
            id
        }
    };
    if liked {
        let data = saved_song_data(identity, song, quality);
        transaction.execute(
            "INSERT OR IGNORE INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img)
             VALUES (?1,?2,(SELECT COALESCE(MAX(position),-1)+1 FROM playlist_songs WHERE playlist_id=?1),?3,?4,?5,?6,'')",
            params![id,song.id,data,song.title,song.artist,song.album],
        ).map_err(unavailable)?;
    } else {
        transaction
            .execute(
                "DELETE FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                params![id, song.id],
            )
            .map_err(unavailable)?;
    }
    transaction
        .execute(
            "UPDATE playlists SET updateTime=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            [&id],
        )
        .map_err(unavailable)?;
    let favorites = read_projection(&transaction, selected_only.then_some(song.id.as_str()))?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(favorites)
}

/// Favorites toggle for an online-source song. The row is the song's original MusicItem JSON
/// (as the original app stores it), keyed by the raw songmid. A different source's or a
/// Subsonic row with the same ID is never overwritten.
pub fn set_online_favorite(
    path: &Path,
    item: &serde_json::Value,
    liked: bool,
    name: &str,
    description: &str,
) -> Result<Favorites> {
    set_online_favorite_projection(path, item, liked, name, description, false)
}

pub(crate) fn set_selected_online_favorite(
    path: &Path,
    item: &serde_json::Value,
    liked: bool,
    name: &str,
    description: &str,
) -> Result<Favorites> {
    set_online_favorite_projection(path, item, liked, name, description, true)
}

fn set_online_favorite_projection(
    path: &Path,
    item: &serde_json::Value,
    liked: bool,
    name: &str,
    description: &str,
    selected_only: bool,
) -> Result<Favorites> {
    let source = item
        .get("source")
        .and_then(serde_json::Value::as_str)
        .filter(|source| mio_music_sdk::is_implemented(source))
        .ok_or(LibraryError::Unavailable)?;
    let songmid = match item.get("songmid") {
        Some(serde_json::Value::String(id)) if !id.is_empty() => id.clone(),
        Some(serde_json::Value::Number(id)) => id.to_string(),
        _ => return Err(LibraryError::Unavailable),
    };
    let text = |key: &str| {
        item.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let mut connection = open(path)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(unavailable)?;
    transaction.execute_batch(SCHEMA).map_err(unavailable)?;
    let previous = read_projection(&transaction, selected_only.then_some(songmid.as_str()))?;
    if previous.songs.contains_key(&songmid) && !previous.contains_online(source, &songmid) {
        return Err(LibraryError::DifferentSource);
    }
    let id = match previous.id {
        Some(id) => id,
        None if !liked => return Ok(previous),
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            transaction.execute(
                "INSERT INTO playlists (id,name,description,source,createTime,updateTime)
                 VALUES (?1,?2,?3,'local',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
                params![id,name,description],
            ).map_err(unavailable)?;
            transaction
                .execute(
                    "INSERT INTO kv_store (key,value) VALUES ('favoritesId',?1)",
                    [&id],
                )
                .map_err(unavailable)?;
            id
        }
    };
    if liked {
        transaction.execute(
            "INSERT OR IGNORE INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img)
             VALUES (?1,?2,(SELECT COALESCE(MAX(position),-1)+1 FROM playlist_songs WHERE playlist_id=?1),?3,?4,?5,?6,?7)",
            params![id,songmid,item.to_string(),text("name"),text("singer"),text("albumName"),text("img")],
        ).map_err(unavailable)?;
    } else {
        transaction
            .execute(
                "DELETE FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                params![id, songmid],
            )
            .map_err(unavailable)?;
    }
    transaction
        .execute(
            "UPDATE playlists SET updateTime=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",
            [&id],
        )
        .map_err(unavailable)?;
    let favorites = read_projection(&transaction, selected_only.then_some(songmid.as_str()))?;
    transaction.commit().map_err(unavailable)?;
    mark_changed();
    Ok(favorites)
}
