//! Immutable restore preparation and explicit original-cursor library commits.
use super::{BackupFile, BrowseError, LibraryCursor, LibraryError, SessionIdentity, unavailable};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub struct BackupSummary {
    pub playlists: u32,
    pub songs: u32,
    pub local_references: u32,
}
#[derive(serde::Serialize)]
#[serde(transparent)]
pub struct PreparedBackup {
    pub(super) file: BackupFile,
    #[serde(skip)]
    summary: BackupSummary,
}

/// Actual changes from one committed library transaction; settings/plugins are separate.
#[derive(Clone, Copy, Debug, Default)]
pub struct BackupRestoreOutcome {
    pub playlists_added: u32,
    pub songs_added: u32,
    pub songs_skipped: u32,
    pub overwritten: bool,
}

impl LibraryCursor {
    /// Restore into the original existing file, or publish a complete new database
    /// without replacement if this cursor captured a missing file. No schema repair.
    pub fn restore_prepared(
        &mut self,
        prepared: &PreparedBackup,
        overwrite: bool,
        admitted: impl Fn() -> bool,
    ) -> Result<BackupRestoreOutcome, BrowseError> {
        let mut outcome = BackupRestoreOutcome {
            overwritten: overwrite,
            ..Default::default()
        };
        self.validate()?;
        if self.connection.is_none() {
            check_missing_sidecars(&self.path)?;
            // Build a complete database privately, then publish without replacing
            // any file that appeared since the missing-file preview was captured.
            if !admitted() {
                return Err(BrowseError::Unavailable);
            }
            let parent = self.path.parent().ok_or(BrowseError::Unavailable)?;
            let temp =
                tempfile::NamedTempFile::new_in(parent).map_err(|_| BrowseError::Unavailable)?;
            let mut connection = rusqlite::Connection::open_with_flags(
                temp.path(),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|_| BrowseError::Unavailable)?;
            connection
                .execute_batch("PRAGMA foreign_keys=ON;")
                .map_err(|_| BrowseError::Unavailable)?;
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| BrowseError::Unavailable)?;
            transaction
                .execute_batch(super::SCHEMA)
                .map_err(|_| BrowseError::Unavailable)?;
            restore_on(&transaction, prepared, overwrite, &mut outcome, &admitted)?;
            self.validate()?;
            check_missing_sidecars(&self.path)?;
            if !admitted() {
                return Err(BrowseError::Unavailable);
            }
            transaction.commit().map_err(|_| BrowseError::Unavailable)?;
            connection.close().map_err(|_| BrowseError::Unavailable)?;
            temp.as_file()
                .sync_all()
                .map_err(|_| BrowseError::Unavailable)?;
            self.validate()?;
            check_missing_sidecars(&self.path)?;
            if !admitted() {
                return Err(BrowseError::Unavailable);
            }
            temp.persist_noclobber(&self.path)
                .map_err(|_| BrowseError::Changed)?;
            super::mark_changed();
            self.invalid = true;
            self.sorted = None;
            return Ok(outcome);
        }
        self.mutate_playlist(
            || admitted(),
            |connection| restore_on(connection, prepared, overwrite, &mut outcome, &admitted),
        )?;
        Ok(outcome)
    }
}

pub(super) fn check_missing_sidecars(path: &std::path::Path) -> Result<(), BrowseError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_owned();
        sidecar.push(suffix);
        match std::fs::symlink_metadata(std::path::PathBuf::from(sidecar)) {
            Ok(_) => return Err(BrowseError::Changed),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(BrowseError::Unavailable),
        }
    }
    Ok(())
}
fn restore_on(
    connection: &rusqlite::Connection,
    prepared: &PreparedBackup,
    overwrite: bool,
    outcome: &mut BackupRestoreOutcome,
    admitted: &dyn Fn() -> bool,
) -> super::Result<()> {
    let check = || {
        if admitted() {
            Ok(())
        } else {
            Err("cancelled".to_owned())
        }
    };
    if overwrite {
        connection
            .execute("DELETE FROM playlist_songs", [])
            .map_err(unavailable)?;
        connection
            .execute("DELETE FROM playlists", [])
            .map_err(unavailable)?;
    }
    // Bound duplicate metadata reads independently of incoming bytes.
    let mut duplicate_bytes = 0usize;
    for item in &prepared.file.playlists {
        check().map_err(unavailable)?;
        let p = &item.playlist;
        let same_source: Option<bool> = connection
            .query_row(
                "SELECT source=?2 FROM playlists WHERE id=?1",
                params![p.id, p.source],
                |row| row.get(0),
            )
            .optional()
            .map_err(unavailable)?;
        match same_source {
            Some(false) => return Err(LibraryError::DifferentSource),
            Some(true) => {}
            None => {
                connection.execute(
                "INSERT INTO playlists (id,name,description,coverImgUrl,source,meta,createTime,updateTime) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![p.id,p.name,p.description,p.cover_img_url,p.source,p.meta,p.create_time,p.update_time],
            ).map_err(unavailable)?;
                outcome.playlists_added += 1;
            }
        }
        let mut last: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(position),-1) FROM playlist_songs WHERE playlist_id=?1",
                [&p.id],
                |row| row.get(0),
            )
            .map_err(unavailable)?;
        if last < -1 {
            return Err(LibraryError::Unavailable);
        }
        for song in &item.songs {
            check().map_err(unavailable)?;
            let old_bytes: Option<i64> = connection.query_row(
            "SELECT length(CAST(data AS BLOB)) FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
            params![p.id,song.songmid], |row| row.get(0),
        ).optional().map_err(unavailable)?;
            if let Some(bytes) = old_bytes {
                let bytes = usize::try_from(bytes).map_err(unavailable)?;
                if bytes > 2 * 1024 * 1024 {
                    return Err(LibraryError::Unavailable);
                }
                duplicate_bytes = duplicate_bytes
                    .checked_add(bytes)
                    .filter(|n| *n <= crate::backup_restore::MAX_DOCUMENT_BYTES)
                    .ok_or(LibraryError::Unavailable)?;
                let old: String = connection
                    .query_row(
                        "SELECT data FROM playlist_songs WHERE playlist_id=?1 AND songmid=?2",
                        params![p.id, song.songmid],
                        |row| row.get(0),
                    )
                    .map_err(unavailable)?;
                let old = crate::backup_restore::parse_json(old.as_bytes(), &check)
                    .map_err(unavailable)?;
                let incoming = crate::backup_restore::parse_json(song.data.as_bytes(), &check)
                    .map_err(unavailable)?;
                if !old.is_object()
                    || old.get("source") != incoming.get("source")
                    || old.get("_mioGpuiSource") != incoming.get("_mioGpuiSource")
                {
                    return Err(LibraryError::DifferentSource);
                }
                outcome.songs_skipped += 1;
                continue;
            }
            let position = if overwrite {
                song.position
            } else {
                last = last.checked_add(1).ok_or(LibraryError::Unavailable)?;
                last
            };
            connection.execute(
            "INSERT INTO playlist_songs (playlist_id,songmid,position,data,name,singer,albumName,img) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![p.id,song.songmid,position,song.data,song.name,song.singer,song.album_name,song.img],
        ).map_err(unavailable)?;
            outcome.songs_added += 1;
        }
    }
    connection.execute(
    "DELETE FROM kv_store WHERE key='favoritesId' AND value NOT IN (SELECT id FROM playlists)", [],
).map_err(unavailable)?;
    check().map_err(unavailable)?;
    Ok(())
}

fn text(value: &str, max: usize, nonempty: bool) -> Result<(), String> {
    if value.len() > max || value.contains('\0') || (nonempty && value.trim().is_empty()) {
        return Err("invalid_data: backup text".into());
    }
    Ok(())
}
fn identifier(value: &str, max: usize) -> Result<(), String> {
    text(value, max, true)?;
    if value.chars().any(char::is_control) {
        return Err("invalid_data: backup identity".into());
    }
    Ok(())
}
impl PreparedBackup {
    pub fn summary(&self) -> BackupSummary {
        self.summary
    }
    pub(crate) fn prepare(
        value: Value,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        check()?;
        // The document parser already bounded input bytes, nodes, and duplicate JSON fields.
        let file: BackupFile =
            serde_json::from_value(value).map_err(|_| "invalid_data: backup playlists")?;
        if file.playlists.len() > 1000 {
            return Err("capacity_exceeded: backup playlists".into());
        }
        let mut ids = HashSet::new();
        let mut songs = 0u32;
        let mut local = 0u32;
        for item in &file.playlists {
            check()?;
            let p = &item.playlist;
            identifier(&p.id, 256)?;
            if !ids.insert(p.id.as_str()) {
                return Err("invalid_data: duplicate backup playlist".into());
            }
            text(&p.name, 4096, true)?;
            text(&p.description, 64 * 1024, false)?;
            identifier(&p.source, 128)?;
            text(&p.create_time, 128, false)?;
            text(&p.update_time, 128, false)?;
            text(&p.meta, 256 * 1024, true)?;
            if !crate::backup_restore::parse_json(p.meta.as_bytes(), check)?.is_object() {
                return Err("invalid_data: backup playlist metadata".into());
            }
            if p.cover_img_url.starts_with("data:") {
                if p.cover_img_url.len() > 22_369_700 {
                    return Err("capacity_exceeded: backup cover".into());
                }
                // Reuse the same bounded decoder used by visible saved covers.
                crate::cover::saved_playlist_thumbnail(&p.cover_img_url)
                    .map_err(|_| "invalid_data: backup cover")?;
            } else {
                text(&p.cover_img_url, 8192, false)?;
            }
            songs = songs
                .checked_add(
                    u32::try_from(item.songs.len())
                        .map_err(|_| "capacity_exceeded: backup songs")?,
                )
                .filter(|n| *n <= 20_000)
                .ok_or("capacity_exceeded: backup songs")?;
            let mut keys = HashSet::new();
            for row in &item.songs {
                check()?;
                identifier(&row.songmid, 512)?;
                if !keys.insert(row.songmid.as_str()) || row.position < 0 {
                    return Err("invalid_data: duplicate or invalid backup song".into());
                }
                for field in [&row.name, &row.singer, &row.album_name] {
                    text(field, 4096, false)?;
                }
                text(&row.img, 8192, false)?;
                text(&row.data, 2 * 1024 * 1024, true)?;
                let data = crate::backup_restore::parse_json(row.data.as_bytes(), check)?;
                let source = data
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or("invalid_data: backup song source")?;
                identifier(source, 128)?;
                let raw = data
                    .get("songmid")
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .or_else(|| v.as_u64().map(|v| v.to_string()))
                    })
                    .ok_or("invalid_data: backup raw song ID")?;
                if raw != row.songmid {
                    return Err("invalid_data: backup song identity mismatch".into());
                }
                if source == "local" {
                    if row.songmid.strip_prefix("mio-local:").is_none_or(|id| {
                        id.len() != 16
                            || !id
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    }) {
                        return Err("invalid_data: backup local song ID".into());
                    }
                } else if source == "subsonic"
                    && (row.songmid.starts_with("mio-local:")
                        || row.songmid.starts_with("mio-online:"))
                {
                    return Err("invalid_data: reserved backup song ID".into());
                }
                match data.get("_mioGpuiSource") {
                    Some(identity) => {
                        let parsed: SessionIdentity = serde_json::from_value(identity.clone())
                            .map_err(|_| "invalid_data: backup local identity")?;
                        for key in ["base_url", "username"] {
                            identifier(
                                identity
                                    .get(key)
                                    .and_then(Value::as_str)
                                    .ok_or("invalid_data: backup local identity")?,
                                4096,
                            )?;
                        }
                        if !matches!(source, "local" | "subsonic")
                            || (source == "local" && parsed != SessionIdentity::local())
                        {
                            return Err("invalid_data: backup source identity mismatch".into());
                        }
                        if source == "subsonic" {
                            let url = reqwest::Url::parse(
                                identity
                                    .get("base_url")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                            )
                            .map_err(|_| "invalid_data: backup remote identity")?;
                            if !matches!(url.scheme(), "http" | "https")
                                || url.host_str().is_none()
                                || matches!(
                                    url.host_str(),
                                    Some("mio-local.invalid" | "mio-online.invalid")
                                )
                                || !url.username().is_empty()
                                || url.password().is_some()
                                || url.query().is_some()
                                || url.fragment().is_some()
                            {
                                return Err("invalid_data: backup remote identity".into());
                            }
                        }
                        if source == "local" {
                            local += 1;
                        }
                    }
                    None if matches!(source, "local" | "subsonic") => {
                        return Err("invalid_data: missing backup source identity".into());
                    }
                    None => {}
                }
            }
        }
        check()?;
        let summary = BackupSummary {
            playlists: file.playlists.len() as u32,
            songs,
            local_references: local,
        };
        Ok(Self { file, summary })
    }
}
