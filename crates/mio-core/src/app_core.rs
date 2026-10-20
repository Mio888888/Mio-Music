//! Application state shared by native hosts; presentation stays outside this crate.
use crate::{
    list_plugins, local_music,
    online::Online,
    paths::Paths,
    playback::{Player, Status},
    song::Song,
    storage::{PlaybackSession, Preferences, SessionIdentity, Storage},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Queue edits preserve the currently playing song and commit storage first.
pub enum QueueEdit {
    Remove(String),
    Move { from: usize, boundary: usize },
    Clear,
}

pub enum EqualizerChange {
    Enabled(bool),
    GlobalGain(f64),
    Band(usize, crate::equalizer::Band),
    Preset(String),
    SavePreset(String),
    UpdatePreset(String),
    DeletePreset(String),
    ImportConfig(serde_json::Value),
    ImportText { name: String, text: String },
    Reset,
}

/// Immutable song/provider snapshot for a quality dialog. It remains valid if
/// browsing evicts the song's Online cache entry while the dialog is open.
#[derive(Clone)]
pub struct DownloadRequest {
    pub choices: Vec<crate::quality::Choice>,
    pub remembered: String,
    pub source: String,
    item: mio_music_sdk::MusicItem,
    online: Arc<Online>,
    manager: Arc<crate::download::Downloads>,
}

impl DownloadRequest {
    pub fn enqueue(&self, quality: &str) -> Result<crate::download::Enqueued, String> {
        if !self.choices.iter().any(|choice| choice.quality == quality) {
            return Err("Download quality is unavailable".into());
        }
        self.manager
            .enqueue_current(&self.online, self.item.clone(), quality.to_owned())
    }
}

/// Immutable, source-bound favorite intent captured before background disk I/O.
#[derive(Clone)]
pub struct FavoriteTarget {
    path: std::path::PathBuf,
    data: FavoriteData,
}

#[derive(Clone)]
enum FavoriteData {
    Online {
        source: String,
        raw: String,
        item: serde_json::Value,
    },
    Local {
        song: Song,
        quality: Option<crate::song::SongQuality>,
    },
}

impl FavoriteTarget {
    /// Mobile reads one indexed row, without loading the entire favorites map.
    pub fn read_state(&self) -> Result<bool, crate::library::LibraryError> {
        let id = match &self.data {
            FavoriteData::Online { raw, .. } => raw,
            FavoriteData::Local { song, .. } => &song.id,
        };
        let saved = crate::library::load_selected_favorite(&self.path, id)?;
        Ok(self.liked(&saved))
    }

    /// Preserve the shared transaction/collision rules and return only this row.
    pub fn save_state(
        &self,
        liked: bool,
        english: bool,
    ) -> Result<bool, crate::library::LibraryError> {
        let name = crate::i18n::text(english, "music.songList.myFavoriteMusic");
        let description = crate::i18n::text(english, "music.songList.collectedSongs");
        let saved = match &self.data {
            FavoriteData::Online { item, .. } => crate::library::set_selected_online_favorite(
                &self.path,
                item,
                liked,
                &name,
                &description,
            ),
            FavoriteData::Local { song, quality } => crate::library::set_selected_favorite(
                &self.path,
                &SessionIdentity::local(),
                song,
                *quality,
                liked,
                &name,
                &description,
            ),
        }?;
        Ok(self.liked(&saved))
    }

    pub fn liked(&self, snapshot: &crate::library::Favorites) -> bool {
        match &self.data {
            FavoriteData::Online { source, raw, .. } => snapshot.contains_online(source, raw),
            FavoriteData::Local { song, .. } => {
                snapshot.contains(&song.id, &SessionIdentity::local())
            }
        }
    }

    pub fn save(
        self,
        liked: bool,
        english: bool,
    ) -> Result<crate::library::Favorites, crate::library::LibraryError> {
        let name = crate::i18n::text(english, "music.songList.myFavoriteMusic");
        let description = crate::i18n::text(english, "music.songList.collectedSongs");
        match self.data {
            FavoriteData::Online { item, .. } => {
                crate::library::set_online_favorite(&self.path, &item, liked, &name, &description)
            }
            FavoriteData::Local { song, quality } => crate::library::set_favorite(
                &self.path,
                &SessionIdentity::local(),
                &song,
                quality,
                liked,
                &name,
                &description,
            ),
        }
    }
}

pub struct AppCore {
    pub player: Player,
    pub downloads: Arc<crate::download::Downloads>,
    pub online: Option<Arc<Online>>,
    pub preferences: Preferences,
    pub paths: Paths,
    pub store: Storage,
    local: local_music::Library,
    local_revision: u64,
    source_selection_revision: u64,
    queue_identity: Option<SessionIdentity>,
    restored: bool,
    checkpoint: Instant,
    /// What the last periodic checkpoint saved; an unchanged player is not written again.
    checkpointed: Option<(u64, Status, Option<usize>, u64, f64)>,
    equalizer: crate::equalizer::State,
    audio_effects: crate::audio_effects::AudioEffects,
}

impl AppCore {
    /// Snapshot queue metadata on its owner lock; the caller writes off-thread.
    /// Never assign today's source to a queued song with missing metadata.
    pub fn playlist_queue_snapshot(&self) -> Result<Vec<serde_json::Value>, String> {
        if self.player.queue.len() > 2048 {
            return Err("Queue exceeds session capacity".into());
        }
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= 64 * 1024 * 1024)
                    .ok_or_else(|| std::io::Error::other("Queue metadata exceeds limit"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut size = Counter(2);
        let mut rows = Vec::with_capacity(self.player.queue.len());
        for song in &self.player.queue {
            if [
                &song.id,
                &song.title,
                &song.artist,
                &song.album,
                song.cover_art.as_deref().unwrap_or(""),
            ]
            .iter()
            .map(|s| s.len())
            .sum::<usize>()
                > 2 * 1024 * 1024
            {
                return Err("Queue metadata exceeds limit".into());
            }
            let quality = self.player.queue_qualities.get(&song.id).copied();
            let mut row = if crate::local_music::is_local(song) {
                serde_json::from_str(&crate::library::saved_song_data(
                    &SessionIdentity::local(),
                    song,
                    quality,
                ))
                .map_err(|_| "Queue metadata is unavailable")?
            } else if crate::online::is_online(song) {
                let row = self
                    .online
                    .as_ref()
                    .map(|online| online.bounded_item(&song.id))
                    .transpose()?
                    .flatten()
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|_| "Queue metadata is unavailable")?
                    .ok_or("Queue metadata is unavailable")?;
                let source = row
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Queue metadata is unavailable")?;
                let raw = row
                    .get("songmid")
                    .and_then(|raw| {
                        raw.as_str()
                            .map(str::to_owned)
                            .or_else(|| raw.as_u64().map(|raw| raw.to_string()))
                    })
                    .ok_or("Queue metadata is unavailable")?;
                if crate::online::song_id(source, &raw) != song.id {
                    return Err("Queue metadata is unavailable".into());
                }
                row
            } else {
                return Err("Queue metadata is unavailable".into());
            };
            if let Some(quality) = quality {
                row["_mioGpuiQuality"] =
                    serde_json::to_value(quality).map_err(|_| "Queue metadata is unavailable")?;
            }
            serde_json::to_writer(&mut size, &row).map_err(|_| "Queue metadata exceeds limit")?;
            std::io::Write::write_all(&mut size, b",")
                .map_err(|_| "Queue metadata exceeds limit")?;
            rows.push(row);
        }
        Ok(rows)
    }

    /// Merge supported original rows behind the current queue without starting playback.
    /// Persist the proposed queue before publishing it; metadata cache writes are separate.
    pub fn merge_imported_queue(
        &mut self,
        rows: Vec<crate::library::PlaylistSong>,
        expected: Option<&Arc<Online>>,
        cancelled: impl Fn() -> bool,
    ) -> Result<usize, String> {
        if cancelled() {
            return Err("Queue import cancelled".into());
        }
        let mut known: std::collections::HashSet<_> =
            self.player.queue.iter().map(|s| s.id.clone()).collect();
        let mut add = Vec::new();
        let sources = self
            .online
            .as_ref()
            .map(|o| o.sources())
            .unwrap_or_default();
        for row in rows {
            if !known.insert(row.song.id.clone()) {
                continue;
            }
            if row.online.is_some() {
                if sources.iter().any(|s| s.id == row.source) {
                    add.push(row);
                }
            } else if row.playable(None)
                && self.local.path(&row.song.id).is_some_and(|p| p.is_file())
            {
                add.push(row);
            }
        }
        if add.is_empty() {
            return Ok(0);
        }
        if self.player.queue.len().saturating_add(add.len()) > 2048 {
            return Err("Queue exceeds session capacity".into());
        }
        self.validate_saved_rows(&add, expected)?;
        let added = add.len();
        let mut queue = self.player.queue.clone();
        let mut qualities = self.player.queue_qualities.clone();
        for row in &add {
            queue.push(row.song.clone());
            if let Some(quality) = row.quality {
                qualities.insert(row.song.id.clone(), quality);
            }
        }
        let session = self
            .capture_queue_session(&queue, &qualities, self.queue_identity.clone())?
            .ok_or("Cannot capture queue session")?;
        if cancelled() {
            return Err("Queue import cancelled".into());
        }
        self.store
            .save_session(Some(&session))
            .map_err(|_| "Cannot save queue; previous queue retained")?;
        // A successful session write is the admission point: report its actual outcome
        // even if the UI closes while the write completes.
        self.player
            .append_to_queue(add.into_iter().map(|row| row.song).collect());
        self.player.queue_qualities = qualities;
        self.prepare_next();
        Ok(added)
    }

    /// Appends already published online songs behind the queue without touching
    /// the current track. Returns how many were new.
    pub fn append_online_songs_to_queue(
        &mut self,
        songs: Vec<Song>,
        expected: &Arc<Online>,
        cancelled: impl Fn() -> bool,
    ) -> Result<usize, String> {
        if cancelled() {
            return Err("Queue append cancelled".into());
        }
        if !self
            .online
            .as_ref()
            .is_some_and(|online| Arc::ptr_eq(online, expected))
        {
            return Err("Playback source changed; open the playlist again".into());
        }
        let mut known: std::collections::HashSet<_> =
            self.player.queue.iter().map(|s| s.id.clone()).collect();
        let mut add = Vec::new();
        for song in songs {
            if !crate::online::is_online(&song) || expected.item(&song.id).is_none() {
                return Err("Playlist metadata is unavailable".into());
            }
            if known.insert(song.id.clone()) {
                add.push(song);
            }
        }
        if add.is_empty() {
            return Ok(0);
        }
        if self.player.queue.len().saturating_add(add.len()) > 2048 {
            return Err("Queue exceeds session capacity".into());
        }
        let mut queue = self.player.queue.clone();
        queue.extend(add.iter().cloned());
        let qualities = self.player.queue_qualities.clone();
        let session = self
            .capture_queue_session(&queue, &qualities, self.queue_identity.clone())?
            .ok_or("Cannot capture queue session")?;
        if cancelled() {
            return Err("Queue append cancelled".into());
        }
        self.store
            .save_session(Some(&session))
            .map_err(|_| "Cannot save queue; previous queue retained")?;
        let added = add.len();
        self.player.append_to_queue(add);
        self.prepare_next();
        Ok(added)
    }

    pub fn prepare_download(
        &self,
        online: Arc<Online>,
        item: serde_json::Value,
    ) -> Result<DownloadRequest, String> {
        let item: mio_music_sdk::MusicItem = serde_json::from_value(item)
            .map_err(|_| crate::i18n::text(self.preferences.english(), "download.unknownError"))?;
        if !self
            .online
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &online))
        {
            return Err("Source changed; choose download quality again".into());
        }
        let sources = online.sources();
        let Some(source) = sources.iter().find(|source| source.id == item.source) else {
            return Err(if self.preferences.english() {
                "The current plugin does not support this song's source"
            } else {
                "当前插件不支持这首歌曲的来源"
            }
            .into());
        };
        let choices = crate::quality::choices(&item, &source.qualities);
        if choices.is_empty() {
            return Err(crate::i18n::text(
                self.preferences.english(),
                "download.getUrlFailed",
            ));
        }
        let selected = sources
            .iter()
            .find(|source| source.id == self.preferences.app.selected_source)
            .or_else(|| sources.first());
        let fallback = selected.and_then(|source| {
            self.preferences
                .download_quality
                .get(&source.id)
                .filter(|quality| source.qualities.contains(quality))
                .or(Some(&self.preferences.app.selected_quality)
                    .filter(|quality| source.qualities.contains(quality)))
                .or_else(|| crate::quality::default_quality(&source.qualities))
                .cloned()
        });
        Ok(DownloadRequest {
            remembered: self
                .preferences
                .download_quality
                .get(&item.source)
                .cloned()
                .or(fallback)
                .unwrap_or_default(),
            source: item.source.clone(),
            choices,
            item,
            online,
            manager: self.downloads.clone(),
        })
    }

    pub fn remember_download_quality(
        &mut self,
        request: &DownloadRequest,
        quality: &str,
    ) -> Result<(), String> {
        if !self
            .online
            .as_ref()
            .is_some_and(|online| Arc::ptr_eq(online, &request.online))
        {
            return Err("Source changed; choose download quality again".into());
        }
        if !request
            .choices
            .iter()
            .any(|choice| choice.quality == quality)
        {
            return Err("Download quality is unavailable".into());
        }
        let previous = self
            .preferences
            .download_quality
            .insert(request.source.clone(), quality.into());
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            match previous {
                Some(previous) => {
                    self.preferences
                        .download_quality
                        .insert(request.source.clone(), previous);
                }
                None => {
                    self.preferences.download_quality.remove(&request.source);
                }
            }
            return Err(format!("Cannot save download quality: {error}"));
        }
        Ok(())
    }

    pub fn favorite_target(&self, song: &Song) -> Option<FavoriteTarget> {
        let data = if let Some((source, raw)) = crate::online::parse_song_id(&song.id) {
            FavoriteData::Online {
                source,
                raw,
                item: self.online.as_ref()?.item_json(&song.id)?,
            }
        } else if local_music::is_local(song) && self.local.path(&song.id).is_some() {
            FavoriteData::Local {
                song: song.clone(),
                quality: self.player.queue_qualities.get(&song.id).copied(),
            }
        } else {
            return None;
        };
        Some(FavoriteTarget {
            path: self.store.library_path(),
            data,
        })
    }

    /// Capture favorites from the saved row, without registering a playback cache entry.
    pub fn saved_favorite_target(
        &self,
        row: &crate::library::PlaylistSong,
    ) -> Option<FavoriteTarget> {
        let data = if let Some(item) = &row.online {
            let parsed: mio_music_sdk::MusicItem = serde_json::from_value(item.clone()).ok()?;
            let raw = parsed
                .songmid
                .as_str()
                .map(str::to_owned)
                .or_else(|| parsed.songmid.as_u64().map(|id| id.to_string()))?;
            if parsed.source != row.source
                || crate::online::song_id(&parsed.source, &raw) != row.song.id
            {
                return None;
            }
            FavoriteData::Online {
                source: parsed.source,
                raw,
                item: item.clone(),
            }
        } else if row.playable(None) && self.local.path(&row.song.id).is_some() {
            FavoriteData::Local {
                song: row.song.clone(),
                quality: row.quality,
            }
        } else {
            return None;
        };
        Some(FavoriteTarget {
            path: self.store.library_path(),
            data,
        })
    }

    pub fn open(paths: Paths) -> Result<Self, String> {
        paths.validate()?;
        for path in [&paths.data, &paths.cache, &paths.downloads] {
            std::fs::create_dir_all(path)
                .map_err(|e| format!("Cannot create application directory: {e}"))?;
        }
        let store = Storage::from_paths(&paths);
        let preferences = store
            .load::<Preferences>("preferences.json")
            .map_err(|e| format!("Cannot read preferences: {e}"))?
            .unwrap_or_default();
        crate::quality::set_preference(crate::quality::Preference {
            per_source: preferences.download_quality.clone(),
            selected_source: Some(preferences.app.selected_source.clone())
                .filter(|s| !s.is_empty()),
            selected_quality: Some(preferences.app.selected_quality.clone())
                .filter(|s| !s.is_empty()),
        });
        // Providers must be registered before restoring a virtual playback client.
        list_plugins::restore(&store)?;
        let online = Online::restore(&store)?;
        let local = local_music::Library::load(&store)?;
        let mut equalizer = store
            .load::<serde_json::Value>("equalizer.json")
            .map_err(|error| format!("Cannot read equalizer: {error}"))?
            .map(|raw| crate::equalizer::hydrate(&raw, false))
            .transpose()?
            .unwrap_or_default();
        crate::equalizer::fit_bands(&mut equalizer.gains);
        let audio_effects = store
            .load::<crate::audio_effects::AudioEffects>("audio-effects.json")
            .map_err(|error| format!("Cannot read audio effects: {error}"))?
            .unwrap_or_default();
        audio_effects.validate()?;
        crate::image_cache::configure(paths.cache.clone());
        crate::audio_cache::configure(
            preferences
                .app
                .auto_cache_music
                .then(|| (paths.cache.clone(), preferences.app.cache_size_limit)),
        );
        let mut player = Player::new(preferences.volume, preferences.play_mode);
        player.seamless = preferences.seamless;
        let mut restored = false;
        let mut queue_identity = None;
        if let Some(session) = store
            .load_session()
            .map_err(|e| format!("Cannot read session: {e}"))?
        {
            let qualities = session.online_queue_qualities();
            let state = if session.is_local_session() {
                session.restore_local()
            } else {
                session.restore_online()
            };
            if let Some((queue, current, song, position, identity)) = state {
                player.queue = queue;
                player.queue_qualities = qualities;
                queue_identity = identity;
                player.current = Some(current);
                player.duration = song.duration as f64;
                player.active_song = Some(song);
                player.position = position;
                player.status = Status::Paused;
                restored = true;
            }
        }
        crate::effects::shared().set_equalizer(
            equalizer.enabled,
            equalizer.gains.global,
            &equalizer.gains.band_params(),
        );
        audio_effects.apply();
        // Initialize once before the host starts its playback thread. Restoration
        // and partial-file reconciliation do not run under the playback mutex.
        let downloads = Arc::new(crate::download::Downloads::new(
            paths.downloads.clone(),
            Some(store.downloads_path()),
        ));
        downloads.set_tag_options(preferences.tag_write_options.clone());
        downloads.set_filename_template(
            &crate::storage_settings::Config::from_preferences(&preferences).filename_template,
        );
        if let Some(online) = &online {
            downloads.set_online(online.clone());
        }
        Ok(Self {
            player,
            downloads,
            online,
            preferences,
            paths,
            store,
            local,
            local_revision: 1,
            source_selection_revision: 1,
            queue_identity,
            restored,
            checkpoint: Instant::now(),
            checkpointed: None,
            equalizer,
            audio_effects,
        })
    }

    pub fn play_song(&mut self, song: Song) -> Result<(), String> {
        self.validate_song(&song)?;
        let index = self.player.enqueue_first(song);
        self.play_index(index)
    }

    /// Keep the existing queue and commit the recognized row plus requested start
    /// position before publishing a new playback generation.
    pub fn play_recognized_song(
        &mut self,
        row: crate::recognition::Match,
        expected: &Arc<Online>,
        cancelled: &std::sync::atomic::AtomicBool,
        jump_to_match: bool,
    ) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if !self
            .online
            .as_ref()
            .is_some_and(|online| Arc::ptr_eq(online, expected))
            || !expected.sources().iter().any(|source| source.id == "wy")
        {
            return Err("source_unavailable: enable the original recognition source".into());
        }
        let prepared = crate::online::PreparedSourceSearch::from_recognition(&row)?;
        let mut keep = self
            .player
            .queue
            .iter()
            .map(|song| song.id.clone())
            .collect::<std::collections::HashSet<_>>();
        keep.extend(self.player.song().map(|song| song.id.clone()));
        let song = expected
            .publish_source_search(prepared, &keep, cancelled)?
            .songs
            .into_iter()
            .next()
            .ok_or("recognition_invalid")?;
        let mut songs = self.player.queue.clone();
        let index = if let Some(index) = songs.iter().position(|value| value.id == song.id) {
            songs[index] = song;
            index
        } else {
            songs.insert(0, song);
            0
        };
        let position = if jump_to_match {
            row.start_time_ms as f64 / 1000.
        } else {
            0.
        };
        self.commit_queue_at_position(
            songs,
            self.player.queue_qualities.clone(),
            index,
            position,
            || cancelled.load(Ordering::Acquire),
        )
    }

    /// Commit a complete online queue only after its metadata and persisted session are valid.
    pub fn play_online_queue(
        &mut self,
        songs: Vec<Song>,
        expected: &Arc<Online>,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        if !self
            .online
            .as_ref()
            .is_some_and(|online| Arc::ptr_eq(online, expected))
        {
            return Err("Playback source changed; open the playlist again".into());
        }
        songs.first().ok_or("Playlist is empty")?;
        let mut ids = std::collections::HashSet::new();
        for song in &songs {
            if !crate::online::is_online(song)
                || expected.item(&song.id).is_none()
                || !ids.insert(&song.id)
            {
                return Err("Playlist metadata is unavailable or duplicated".into());
            }
        }
        self.commit_queue(songs, Default::default(), cancelled)
    }

    /// A saved library can contain both local and online tracks. Validate the
    /// entire snapshot before replacing the queue or its durable session.
    pub fn play_saved_playlist(
        &mut self,
        rows: Vec<crate::library::PlaylistSong>,
        expected: Option<&Arc<Online>>,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        self.validate_saved_rows(&rows, expected)?;
        let qualities = rows
            .iter()
            .filter_map(|row| row.quality.map(|quality| (row.song.id.clone(), quality)))
            .collect();
        self.commit_queue(
            rows.into_iter().map(|row| row.song).collect(),
            qualities,
            cancelled,
        )
    }

    fn validate_saved_rows(
        &self,
        rows: &[crate::library::PlaylistSong],
        expected: Option<&Arc<Online>>,
    ) -> Result<(), String> {
        if rows.is_empty() {
            return Err("Playlist is empty".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut items = Vec::new();
        for row in rows {
            if !ids.insert(row.song.id.clone()) {
                return Err("Playlist contains duplicated songs".into());
            }
            if let Some(item) = &row.online {
                let online = expected
                    .filter(|expected| {
                        self.online
                            .as_ref()
                            .is_some_and(|online| Arc::ptr_eq(online, expected))
                    })
                    .ok_or("Playback source changed; open the playlist again")?;
                let item: mio_music_sdk::MusicItem = serde_json::from_value(item.clone())
                    .map_err(|_| "Saved song metadata is unavailable")?;
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|raw| raw.to_string()))
                    .ok_or("Saved song identity is unavailable")?;
                if crate::online::song_id(&item.source, &raw) != row.song.id
                    || item.source != row.source
                    || !online
                        .sources()
                        .iter()
                        .any(|source| source.id == item.source)
                {
                    return Err("Enable this playlist's matching music source".into());
                }
                items.push(
                    serde_json::to_value(item).map_err(|_| "Saved song metadata is unavailable")?,
                );
            } else if !row.playable(None)
                || !self
                    .local
                    .path(&row.song.id)
                    .is_some_and(|path| path.is_file())
            {
                return Err("Saved local audio is unavailable".into());
            }
        }
        if !items.is_empty() {
            let online = expected.ok_or("Playback source unavailable")?;
            let mut keep = ids;
            keep.extend(self.player.queue.iter().map(|song| song.id.clone()));
            if let Some(song) = self.player.song() {
                keep.insert(song.id.clone());
            }
            if !online.register(items, &keep)
                || rows
                    .iter()
                    .any(|row| row.online.is_some() && online.item(&row.song.id).is_none())
            {
                return Err("Saved song metadata cache is unavailable".into());
            }
        }
        Ok(())
    }

    /// Manual saved-song selection retains the queue, just like enqueue_first.
    /// Persist the proposed queue/index before publishing it to the player.
    pub fn play_saved_song(
        &mut self,
        row: crate::library::PlaylistSong,
        expected: Option<&Arc<Online>>,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        self.validate_saved_rows(std::slice::from_ref(&row), expected)?;
        let mut songs = self.player.queue.clone();
        let index = if let Some(index) = songs.iter().position(|song| song.id == row.song.id) {
            index
        } else {
            songs.insert(0, row.song.clone());
            0
        };
        let mut qualities = self.player.queue_qualities.clone();
        if let Some(quality) = row.quality {
            qualities.insert(row.song.id, quality);
        }
        self.commit_queue_at(songs, qualities, index, cancelled)
    }

    fn commit_queue(
        &mut self,
        songs: Vec<Song>,
        qualities: std::collections::HashMap<String, crate::song::SongQuality>,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        self.commit_queue_at(songs, qualities, 0, cancelled)
    }

    fn commit_queue_at(
        &mut self,
        songs: Vec<Song>,
        qualities: std::collections::HashMap<String, crate::song::SongQuality>,
        index: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        self.commit_queue_at_position(songs, qualities, index, 0., cancelled)
    }

    fn commit_queue_at_position(
        &mut self,
        songs: Vec<Song>,
        qualities: std::collections::HashMap<String, crate::song::SongQuality>,
        index: usize,
        position: f64,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        let first = songs.get(index).ok_or("Queue item is unavailable")?;
        let session = if crate::online::is_online(first) {
            PlaybackSession::capture_online(&songs, index, first, position, None)
        } else {
            PlaybackSession::capture_local(&songs, index, first, position, None)
        }
        .map_err(|error| format!("Cannot capture playlist session: {error}"))?;
        let session = session.with_queue_qualities(&qualities);
        let previous = self.capture_session()?;
        if cancelled() {
            return Err("Playlist playback cancelled".into());
        }
        // Persistence failure must not discard the user's existing queue.
        self.store
            .save_session(Some(&session))
            .map_err(|error| format!("Cannot save playlist session: {error}"))?;
        if cancelled() {
            self.store
                .save_session(previous.as_ref())
                .map_err(|error| {
                    format!("Playback cancelled; cannot restore previous session: {error}")
                })?;
            return Err("Playlist playback cancelled".into());
        }
        self.player.replace_queue(songs);
        self.player.queue_qualities = qualities;
        self.queue_identity = None;
        self.start_index(index)?;
        if position > 0. {
            self.player.seek(position);
        }
        Ok(())
    }

    fn validate_song(&self, song: &Song) -> Result<(), String> {
        if crate::online::is_online(song) {
            if self.online.is_none() {
                return Err("Install a playback plugin first".into());
            }
        } else if local_music::is_local(song) {
            if self.local.path(&song.id).is_none() {
                return Err("Local audio file is unavailable".into());
            }
        } else {
            return Err("Unsupported song identity".into());
        }
        Ok(())
    }

    pub fn play_index(&mut self, index: usize) -> Result<(), String> {
        self.start_index(index)?;
        self.save_session()
    }

    fn start_index(&mut self, index: usize) -> Result<(), String> {
        let song = self
            .player
            .queue
            .get(index)
            .ok_or("Queue item is unavailable")?
            .clone();
        self.validate_song(&song)?;
        if crate::online::is_online(&song) {
            self.player.play_online(self.online.clone().unwrap(), index);
        } else {
            self.player
                .play_local(index, self.local.path(&song.id).unwrap());
        }
        self.restored = false;
        self.prepare_next();
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        if !paused
            && (self.restored || matches!(self.player.status, Status::Ended | Status::Failed))
        {
            let song = self.player.song().ok_or("No song selected")?.clone();
            self.validate_song(&song)?;
            let position = if self.restored {
                self.player.position
            } else {
                0.
            };
            if crate::online::is_online(&song) {
                self.player
                    .resume_online(self.online.clone().unwrap(), position);
            } else {
                self.player
                    .resume_local(self.local.path(&song.id).unwrap(), position);
            }
            self.restored = false;
            self.prepare_next();
        } else if let Some(engine) = &self.player.engine {
            engine.pause(self.player.generation, paused);
        }
        self.save_session()
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.player.stop();
        self.restored = false;
        self.queue_identity = None;
        self.save_session()
    }

    pub fn source_selection_revision(&self) -> u64 {
        self.source_selection_revision
    }

    /// Persist first, then publish the selected source and its quality policy.
    /// This revision also rejects retained menus after an A -> B -> A change.
    pub fn select_music_source(&mut self, expected_revision: u64, id: &str) -> Result<(), String> {
        if self.source_selection_revision != expected_revision {
            return Err("selection_changed: refresh source selection".into());
        }
        let source = self
            .online
            .as_ref()
            .and_then(|online| online.sources().into_iter().find(|source| source.id == id))
            .ok_or("source_unavailable")?;
        let mut next = self.preferences.clone();
        next.app.selected_source = id.to_owned();
        if !source.qualities.is_empty() {
            let quality = next
                .download_quality
                .get(id)
                .filter(|quality| source.qualities.contains(quality))
                .or_else(|| crate::quality::default_quality(&source.qualities))
                .cloned()
                .unwrap_or_default();
            next.download_quality.insert(id.to_owned(), quality.clone());
            next.app.selected_quality = quality;
        }
        self.store
            .save("preferences.json", &next)
            .map_err(|_| "storage_unavailable: cannot save source selection".to_owned())?;
        crate::quality::set_preference(crate::quality::Preference {
            per_source: next.download_quality.clone(),
            selected_source: Some(next.app.selected_source.clone()).filter(|s| !s.is_empty()),
            selected_quality: Some(next.app.selected_quality.clone()).filter(|s| !s.is_empty()),
        });
        self.preferences = next;
        self.source_selection_revision = self.source_selection_revision.wrapping_add(1);
        Ok(())
    }

    /// The source settings and download dialogs share one persisted quality map.
    /// Save before publication and reject a form retained across another quality edit.
    pub fn select_music_quality(
        &mut self,
        expected_revision: u64,
        expected: &std::collections::BTreeMap<String, String>,
        quality: &str,
        all_sources: bool,
    ) -> Result<(), String> {
        if expected_revision != self.source_selection_revision
            || expected != &self.preferences.download_quality
        {
            return Err("selection_changed: refresh music settings".into());
        }
        let sources = self.online.as_ref().ok_or("source_unavailable")?.sources();
        let selected = sources
            .iter()
            .find(|source| source.id == self.preferences.app.selected_source)
            .or_else(|| sources.first())
            .ok_or("source_unavailable")?;
        if !selected.qualities.iter().any(|q| q == quality)
            || (all_sources
                && sources
                    .iter()
                    .any(|source| !source.qualities.iter().any(|q| q == quality)))
        {
            return Err("invalid_input: unavailable quality".into());
        }
        let mut next = self.preferences.clone();
        for source in &sources {
            if all_sources || source.id == selected.id {
                next.download_quality
                    .insert(source.id.clone(), quality.to_owned());
            }
        }
        next.app.selected_source = selected.id.clone();
        next.app.selected_quality = quality.to_owned();
        self.store
            .save("preferences.json", &next)
            .map_err(|_| "storage_unavailable: cannot save music quality")?;
        crate::quality::set_preference(crate::quality::Preference {
            per_source: next.download_quality.clone(),
            selected_source: Some(next.app.selected_source.clone()),
            selected_quality: Some(next.app.selected_quality.clone()),
        });
        self.preferences = next;
        self.source_selection_revision = self.source_selection_revision.wrapping_add(1);
        Ok(())
    }

    pub fn set_appearance(&mut self, dark: bool, language: Option<bool>) -> Result<(), String> {
        let previous = (
            self.preferences.dark,
            self.preferences.english,
            self.preferences.language.clone(),
        );
        self.preferences.dark = dark;
        self.preferences.english = language.unwrap_or_else(crate::host::system_is_english);
        self.preferences.language = Some(
            match language {
                Some(true) => "en-US",
                Some(false) => "zh-CN",
                None => "system",
            }
            .into(),
        );
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.dark = previous.0;
            self.preferences.english = previous.1;
            self.preferences.language = previous.2;
            return Err(format!("Cannot save preferences: {error}"));
        }
        Ok(())
    }

    pub fn step(&mut self, forward: bool) -> Result<(), String> {
        if let Some(index) = self.player.next_index(forward) {
            self.play_index(index)
        } else {
            Ok(())
        }
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        self.player.seek(seconds);
        self.save_session()
    }

    pub fn equalizer(&self) -> &crate::equalizer::State {
        &self.equalizer
    }

    pub fn change_equalizer(
        &mut self,
        change: EqualizerChange,
    ) -> Result<crate::equalizer::State, String> {
        use crate::effects::*;
        let mut next = self.equalizer.clone();
        match change {
            EqualizerChange::Enabled(enabled) => next.enabled = enabled,
            EqualizerChange::GlobalGain(gain) => {
                if !gain.is_finite() || !(EQ_GAIN_MIN..=EQ_GAIN_MAX).contains(&gain) {
                    return Err("Gain must be between -24 and 24 dB".into());
                }
                next.gains.global = gain;
            }
            EqualizerChange::Band(index, mut band) => {
                if !band.frequency.is_finite()
                    || !(EQ_FREQ_MIN..=EQ_FREQ_MAX).contains(&band.frequency)
                    || !band.gain.is_finite()
                    || !(EQ_GAIN_MIN..=EQ_GAIN_MAX).contains(&band.gain)
                    || !band.q.is_finite()
                    || !(EQ_Q_MIN..=EQ_Q_MAX).contains(&band.q)
                {
                    return Err("Invalid band: frequency 20–20000 Hz, gain ±24 dB, Q 0.1–10".into());
                }
                if !band.kind.has_gain() {
                    band.gain = 0.;
                }
                crate::equalizer::fit_bands(&mut next.gains);
                *next
                    .gains
                    .bands
                    .get_mut(index)
                    .ok_or("Invalid equalizer band")? = band;
            }
            EqualizerChange::Preset(id) => {
                if !next.apply_preset(&id) {
                    return Err("Equalizer preset is unavailable".into());
                }
            }
            EqualizerChange::SavePreset(name) => {
                if name.trim().chars().count() > 100 || next.user_presets.len() >= 100 {
                    return Err("Preset name or preset limit exceeded".into());
                }
                next.create_user_preset(&name, next.gains.clone(), true)
                    .map_err(str::to_owned)?;
            }
            EqualizerChange::UpdatePreset(id) => {
                if !next.update_user_preset(&id) {
                    return Err("Only a user preset can be updated".into());
                }
            }
            EqualizerChange::DeletePreset(id) => {
                if !next.delete_user_preset(&id) {
                    return Err("Only a user preset can be deleted".into());
                }
            }
            EqualizerChange::ImportConfig(raw) => {
                let imported = crate::equalizer::hydrate(&raw, true)?;
                next.enabled = imported.enabled;
                next.current_preset_id = imported.current_preset_id;
                next.gains = imported.gains;
                next.user_presets = imported.user_presets;
                next.add_log("Imported configuration".into());
            }
            EqualizerChange::ImportText { name, text } => {
                if name.trim().chars().count() > 100 || next.user_presets.len() >= 100 {
                    return Err("Preset name or preset limit exceeded".into());
                }
                let gains = crate::equalizer::parse_text_preset(&text)
                    .ok_or("settings.equalizer.textImportParseFailed")?;
                next.create_user_preset(&name, gains, true)
                    .map_err(str::to_owned)?;
            }
            EqualizerChange::Reset => {
                next.apply_preset(crate::equalizer::FLAT_PRESET_ID);
            }
        }
        // Imported presets may be short; overwrite all ten DSP filter slots.
        crate::equalizer::fit_bands(&mut next.gains);
        self.store.save("equalizer.json", &next).map_err(|error| {
            format!("Cannot save equalizer; previous settings retained: {error}")
        })?;
        crate::effects::shared().set_equalizer(
            next.enabled,
            next.gains.global,
            &next.gains.band_params(),
        );
        self.equalizer = next;
        Ok(self.equalizer.clone())
    }

    pub fn edit_queue(&mut self, edit: QueueEdit) -> Result<(), String> {
        let mut queue = self.player.queue.clone();
        let mut qualities = self.player.queue_qualities.clone();
        let identity = if matches!(&edit, QueueEdit::Clear) {
            None
        } else {
            self.queue_identity.clone()
        };
        match &edit {
            QueueEdit::Remove(id) => {
                let index = queue
                    .iter()
                    .position(|s| &s.id == id)
                    .ok_or("Queue song unavailable")?;
                queue.remove(index);
                qualities.remove(id);
            }
            QueueEdit::Move { from, boundary } => {
                if *from >= queue.len() || *boundary > queue.len() {
                    return Err("Invalid queue position".into());
                }
                let to = boundary.saturating_sub(usize::from(*boundary > *from));
                if *from == to {
                    return Ok(());
                }
                let song = queue.remove(*from);
                queue.insert(to, song);
            }
            QueueEdit::Clear => {
                if queue.is_empty() {
                    return Ok(());
                }
                queue.clear();
                qualities.clear();
            }
        }
        let session = self.capture_queue_session(&queue, &qualities, identity)?;
        self.store
            .save_session(session.as_ref())
            .map_err(|_| "Cannot save queue; previous queue retained")?;
        match edit {
            QueueEdit::Remove(id) => self.player.remove_from_queue(&id),
            QueueEdit::Move { from, boundary } => {
                self.player.move_in_queue(from, boundary);
            }
            QueueEdit::Clear => {
                self.player.clear_queue();
                self.queue_identity = None;
            }
        }
        self.prepare_next();
        Ok(())
    }

    pub fn play_queue_index(
        &mut self,
        index: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        let song = self
            .player
            .queue
            .get(index)
            .ok_or("Queue song unavailable")?;
        self.validate_song(song)?;
        self.commit_queue_at(
            self.player.queue.clone(),
            self.player.queue_qualities.clone(),
            index,
            cancelled,
        )
    }

    pub fn storage_settings(&self) -> crate::storage_settings::Config {
        crate::storage_settings::Config::from_preferences(&self.preferences)
    }
    pub fn save_storage_settings(
        &mut self,
        expected: crate::storage_settings::Config,
        next: crate::storage_settings::Config,
    ) -> Result<(), String> {
        expected.validate()?;
        next.validate()?;
        if self.storage_settings() != expected {
            return Err("settings_changed: refresh storage settings".into());
        }
        if expected == next {
            return Ok(());
        }
        let cache = next
            .auto_cache
            .then(|| (self.paths.cache.clone(), next.cache_limit));
        let downloads = self.downloads.clone();
        crate::audio_cache::configure_when(cache, || {
            downloads.configure_output_when(&next, || {
                let previous_auto = self.preferences.app.auto_cache_music;
                let previous_limit = self.preferences.app.cache_size_limit;
                let previous_template = std::mem::replace(
                    &mut self.preferences.app.filename_template,
                    next.filename_template.clone(),
                );
                let previous_tags = (
                    self.preferences.tag_write_options.basic_info,
                    self.preferences.tag_write_options.cover,
                    self.preferences.tag_write_options.lyrics,
                    self.preferences.tag_write_options.download_lyrics,
                );
                self.preferences.app.auto_cache_music = next.auto_cache;
                self.preferences.app.cache_size_limit = next.cache_limit;
                next.apply_tags(&mut self.preferences.tag_write_options);
                if let Err(error) = self.store.save("preferences.json", &self.preferences) {
                    self.preferences.app.auto_cache_music = previous_auto;
                    self.preferences.app.cache_size_limit = previous_limit;
                    self.preferences.app.filename_template = previous_template;
                    let tags = &mut self.preferences.tag_write_options;
                    (
                        tags.basic_info,
                        tags.cover,
                        tags.lyrics,
                        tags.download_lyrics,
                    ) = previous_tags;
                    return Err(format!("Cannot save storage settings: {error}"));
                }
                Ok(())
            })
        })
    }

    /// Only replaces the background object after matching the original snapshot.
    pub fn save_background(
        &mut self,
        expected: crate::background_config::BackgroundConfig,
        next: crate::background_config::BackgroundConfig,
    ) -> Result<(), String> {
        if expected != expected.validated() || next != next.validated() {
            return Err("invalid_input: background settings".into());
        }
        if self.preferences.background != expected {
            return Err("settings_changed: refresh background settings".into());
        }
        if expected == next {
            return Ok(());
        }
        self.preferences.background = next;
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.background = expected;
            return Err(format!("Cannot save background settings: {error}"));
        }
        Ok(())
    }

    pub fn visuals(&self) -> crate::visuals::Config {
        crate::visuals::Config::from_preferences(&self.preferences)
    }
    pub fn save_visuals(
        &mut self,
        expected: crate::visuals::Config,
        next: crate::visuals::Config,
    ) -> Result<(), String> {
        if self.visuals() != expected {
            return Err("settings_changed: refresh visual settings".into());
        }
        if expected == next {
            return Ok(());
        }
        next.apply(&mut self.preferences);
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            expected.apply(&mut self.preferences);
            return Err(format!("Cannot save visual settings: {error}"));
        }
        Ok(())
    }

    pub fn lyric_style(&self) -> crate::lyric_style::Config {
        crate::lyric_style::Config::from_app(&self.preferences.app)
    }
    /// Typography edits preserve all unrelated preference fields.
    pub fn save_lyric_style(
        &mut self,
        expected: crate::lyric_style::Config,
        next: crate::lyric_style::Config,
    ) -> Result<(), String> {
        expected.validate()?;
        next.validate()?;
        if self.lyric_style() != expected {
            return Err("settings_changed: refresh lyric settings".into());
        }
        if expected == next {
            return Ok(());
        }
        let previous = self.preferences.app.clone();
        self.preferences.app.lyric_font_family = next.family;
        self.preferences.app.lyric_font_rate = next.rate;
        self.preferences.app.lyric_font_weight = next.weight;
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.app = previous;
            return Err(format!("Cannot save lyric settings: {error}"));
        }
        Ok(())
    }

    pub fn balance(&self) -> (bool, f64) {
        (
            self.audio_effects.balance_enabled,
            self.audio_effects.balance,
        )
    }

    /// Save before applying DSP; retain all other desktop effects preferences.
    pub fn save_balance(&mut self, expected: (bool, f64), next: (bool, f64)) -> Result<(), String> {
        if !next.1.is_finite() || !(-1.0..=1.0).contains(&next.1) {
            return Err("invalid_input: channel balance".into());
        }
        if self.balance() != expected {
            return Err("settings_changed: refresh audio effects".into());
        }
        if expected == next {
            return Ok(());
        }
        let mut settings = self.audio_effects.clone();
        settings.balance_enabled = next.0;
        settings.balance = next.1;
        settings.validate()?;
        self.store
            .save("audio-effects.json", &settings)
            .map_err(|error| {
                format!("Cannot save audio effects; previous settings retained: {error}")
            })?;
        settings.apply();
        self.audio_effects = settings;
        Ok(())
    }

    pub fn seamless_config(&self) -> crate::playback::seamless::Config {
        let mut config = self.preferences.seamless;
        config.duration_ms = config.duration_ms.clamp(500, 8000);
        config
    }

    /// Compare the displayed configuration, persist, then reconfigure staging.
    /// A failed preference write leaves the active player configuration intact.
    pub fn save_seamless(
        &mut self,
        expected: crate::playback::seamless::Config,
        next: crate::playback::seamless::Config,
    ) -> Result<(), String> {
        if !(500..=8000).contains(&next.duration_ms) {
            return Err("Crossfade duration must be 500–8000 ms".into());
        }
        if self.seamless_config() != expected {
            return Err("settings_changed: refresh playback settings".into());
        }
        if self.preferences.seamless == next {
            return Ok(());
        }
        let previous = self.preferences.seamless;
        self.preferences.seamless = next;
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.seamless = previous;
            return Err(format!("Cannot save playback settings: {error}"));
        }
        self.player.seamless = next;
        self.prepare_next();
        Ok(())
    }

    pub fn cycle_play_mode(&mut self) -> Result<(), String> {
        let next = self.player.mode.next();
        let previous = self.preferences.play_mode;
        self.preferences.play_mode = next;
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.play_mode = previous;
            return Err(format!("Cannot save playback mode: {error}"));
        }
        self.player.mode = next;
        self.player.refresh_prefetch();
        self.prepare_next();
        Ok(())
    }

    pub fn preview_volume(&mut self, volume: f32) -> Result<(), String> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("Volume must be between 0 and 100 percent".into());
        }
        self.player.volume = volume;
        if let Some(engine) = &self.player.engine {
            engine.volume(volume);
        }
        Ok(())
    }

    pub fn cancel_volume_preview(&mut self) {
        let saved = if self.preferences.volume.is_finite() {
            self.preferences.volume.clamp(0., 1.)
        } else {
            0.5
        };
        let _ = self.preview_volume(saved);
    }

    pub fn save_volume(&mut self, volume: f32) -> Result<(), String> {
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
            return Err("Volume must be between 0 and 100 percent".into());
        }
        let previous = self.preferences.volume;
        self.preferences.volume = volume;
        if let Err(error) = self.store.save("preferences.json", &self.preferences) {
            self.preferences.volume = previous;
            self.cancel_volume_preview();
            return Err(format!(
                "Cannot save volume; previous volume restored: {error}"
            ));
        }
        self.preview_volume(volume)
    }

    pub fn lyrics_source(&self, song: &Song) -> Result<crate::lyrics::Source, String> {
        if crate::online::is_online(song) {
            self.online
                .as_ref()
                .and_then(|online| online.item(&song.id))
                .map(crate::lyrics::Source::Online)
                .ok_or_else(|| "Song details are unavailable; search again".into())
        } else {
            self.local
                .path(&song.id)
                .map(crate::lyrics::Source::Local)
                .ok_or_else(|| "Local song is unavailable".into())
        }
    }

    pub fn cover_source(&self, song: &Song) -> Option<crate::cover::Source> {
        if crate::online::is_online(song) {
            Some(crate::cover::Source::Online {
                url: song.cover_art.clone(),
                item: self
                    .online
                    .as_ref()
                    .and_then(|online| online.item(&song.id)),
            })
        } else {
            self.local
                .tracks
                .iter()
                .find(|track| track.song.id == song.id)
                .map(|track| crate::cover::Source::Local {
                    path: track.path.clone(),
                    cache_key: track.cover_key(),
                })
        }
    }

    pub fn tick(&mut self) -> Result<(), String> {
        self.tick_with_playback_allowed(true)
    }

    pub fn tick_with_playback_allowed(&mut self, allow_playback: bool) -> Result<(), String> {
        let before = (
            self.player.generation,
            self.player.status,
            self.player.current,
        );
        let events = self
            .player
            .engine
            .as_ref()
            .map(|engine| engine.drain())
            .unwrap_or_default();
        for event in events {
            if self.player.apply(event) && allow_playback {
                self.step(true)?;
            }
        }
        self.prepare_next();
        if before
            != (
                self.player.generation,
                self.player.status,
                self.player.current,
            )
            || self.checkpoint.elapsed() >= Duration::from_secs(5)
        {
            self.checkpoint = Instant::now();
            let clock = Some((
                self.player.generation,
                self.player.status,
                self.player.current,
                self.player.queue_revision,
                self.player.position,
            ));
            if clock != self.checkpointed {
                self.save_session()?;
                self.checkpointed = clock;
            }
        }
        Ok(())
    }

    fn prepare_next(&mut self) {
        let config = self.player.seamless;
        let next = if matches!(
            self.player.status,
            Status::Playing | Status::Paused | Status::Loading
        ) {
            self.player.next_index(true).and_then(|index| {
                let song = self.player.queue.get(index)?.clone();
                let input = if crate::online::is_online(&song) {
                    crate::playback::seamless::Input::Online(self.online.clone()?)
                } else {
                    crate::playback::seamless::Input::Local(self.local.path(&song.id)?)
                };
                Some((index, song, input))
            })
        } else {
            None
        };
        if let Some(engine) = &mut self.player.engine {
            engine.configure(config);
            engine.prepare_next(self.player.generation, self.player.queue_revision, next);
        }
    }

    pub fn save_session(&self) -> Result<(), String> {
        let session = self.capture_session()?;
        self.store
            .save_session(session.as_ref())
            .map_err(|e| format!("Cannot save session: {e}"))
    }

    fn capture_session(&self) -> Result<Option<PlaybackSession>, String> {
        self.capture_queue_session(
            &self.player.queue,
            &self.player.queue_qualities,
            self.queue_identity.clone(),
        )
    }
    fn capture_queue_session(
        &self,
        queue: &[Song],
        qualities: &std::collections::HashMap<String, crate::song::SongQuality>,
        identity: Option<SessionIdentity>,
    ) -> Result<Option<PlaybackSession>, String> {
        // A merged queue can be nonempty before the user selects a track. Keep
        // that dormant queue in every checkpoint; importing must not start audio.
        let active = self.player.song();
        let Some(song) = active.or_else(|| queue.first()) else {
            return Ok(None);
        };
        let current = if active.is_some() {
            self.player.current.unwrap_or(0)
        } else {
            0
        };
        let position = if active.is_none() {
            0.
        } else if song.duration > 0 {
            self.player.position.min(song.duration as f64)
        } else {
            self.player.position
        };
        let session = if crate::online::is_online(song) {
            PlaybackSession::capture_online(queue, current, song, position, identity)
        } else {
            PlaybackSession::capture_local(queue, current, song, position, identity)
        }
        .map_err(|e| format!("Cannot capture session: {e}"))?
        .with_queue_qualities(qualities);
        Ok(Some(session))
    }

    pub fn local_tracks(&self) -> Vec<local_music::Track> {
        self.local.tracks.clone()
    }

    /// Borrow the index for bounded mobile projection without cloning every track.
    pub fn local_track_index(&self) -> &[local_music::Track] {
        &self.local.tracks
    }

    pub fn local_revision(&self) -> u64 {
        self.local_revision
    }

    /// Mobile host startup only; no import workers may be running yet.
    pub fn recover_local_imports(&self) -> Result<usize, String> {
        self.local.recover_imports(&self.paths.data)
    }

    pub fn import_local(&mut self, prepared: local_music::PreparedImport) -> Result<usize, String> {
        let count = prepared.commit(&mut self.local, &self.store)?;
        if count != 0 {
            self.local_revision = self.local_revision.wrapping_add(1);
        }
        Ok(count)
    }

    /// Admit a verified original-file replacement on the owner, then merge its actual result.
    /// Once the audio is replaced, index/session failures are warnings, never a retry signal.
    pub fn commit_local_tag_edit(
        &mut self,
        prepared: local_music::PreparedTagEdit,
        revision: u64,
        admitted: impl Fn() -> bool,
    ) -> Result<(bool, bool), String> {
        if self.local_revision != revision
            || !self
                .local
                .tracks
                .iter()
                .any(|track| prepared.matches(track))
        {
            return Err("Local library changed; reopen the editor".into());
        }
        let track = prepared.commit(admitted)?;
        let id = track.song.id.clone();
        let warning = match self.update_local_tracks(vec![track]) {
            Ok(warning) => warning.is_some(),
            Err(_) => true,
        };
        let reopened = self.reopen_local_audio_paused(&id);
        Ok((warning, reopened))
    }

    /// Merge refreshed metadata into the current index, retaining concurrent imports.
    /// Audio may already have been replaced, so publish metadata even if persistence fails.
    pub fn update_local_tracks(
        &mut self,
        tracks: Vec<local_music::Track>,
    ) -> Result<Option<String>, String> {
        let mut updated = self.local.clone();
        for track in &tracks {
            let indexed = updated
                .tracks
                .iter_mut()
                .find(|old| old.song.id == track.song.id && old.path == track.path)
                .ok_or("Local library changed; reopen the editor")?;
            *indexed = track.clone();
        }
        let mut warnings = Vec::new();
        if updated.save(&self.store).is_err() {
            warnings.push("Audio metadata was read, but the library index could not be saved. Refresh to retry.");
        }
        self.local = updated;
        self.local_revision = self.local_revision.wrapping_add(1);
        let mut queue_changed = false;
        for song in &mut self.player.queue {
            if let Some(track) = tracks.iter().find(|track| track.song.id == song.id)
                && *song != track.song
            {
                *song = track.song.clone();
                queue_changed = true;
            }
        }
        let mut active_changed = false;
        if let Some(song) = &mut self.player.active_song
            && let Some(track) = tracks.iter().find(|track| track.song.id == song.id)
        {
            active_changed = *song != track.song;
            *song = track.song.clone();
            self.player.duration = song.duration as f64;
        }
        if queue_changed {
            self.player.queue_revision = self.player.queue_revision.wrapping_add(1);
        }
        self.prepare_next();
        if queue_changed || active_changed {
            if self.save_session().is_err() {
                warnings.push(
                    "Audio metadata was updated, but the playback session could not be saved.",
                );
            }
        }
        Ok((!warnings.is_empty()).then(|| warnings.join(" ")))
    }

    /// Always reopen replaced audio paused. Only the host playback thread may resume it.
    pub fn reopen_local_audio_paused(&mut self, id: &str) -> bool {
        if self.restored
            || !matches!(
                self.player.status,
                Status::Playing | Status::Paused | Status::Loading
            )
            || !self.player.song().is_some_and(|song| song.id == id)
        {
            return false;
        }
        let Some(path) = self.local.path(id) else {
            return false;
        };
        self.player.status = Status::Paused;
        self.player.reload_local(path);
        self.prepare_next();
        true
    }

    /// Future download resolutions use the same provider as playback. Existing
    /// transfers retain their source-bound request, matching desktop behavior.
    pub fn replace_online(&mut self, online: Option<Arc<Online>>) {
        self.online = online;
        if let Some(online) = &self.online {
            self.downloads.set_online(online.clone());
        } else {
            self.downloads.clear_online();
        }
    }
}
