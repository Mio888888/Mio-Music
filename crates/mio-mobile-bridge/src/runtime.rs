use crate::api::{HostPaths, PlayerSnapshot, Track};
use mio_core::{app_core::AppCore, paths::Paths, song::Song};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
// Native callbacks only acquire this short-lived projection lock, never the
// lifecycle lock held while initialization or shutdown waits on the owner.
static MEDIA_TARGET: Mutex<Option<Arc<Published>>> = Mutex::new(None);
// Source edits and replacement runtimes share one process-wide sequence so a
// retained Flutter form cannot match a restarted owner's revision by accident.
static SOURCE_REVISION: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Action {
    GeneralSettings(
        crate::api::GeneralSettings,
        crate::api::GeneralSettings,
        Arc<AtomicBool>,
    ),
    SelectSource(crate::source_selection::Commit),
    Background(
        mio_core::background_config::BackgroundConfig,
        mio_core::background_config::BackgroundConfig,
        Arc<AtomicBool>,
    ),
    Visuals(
        mio_core::visuals::Config,
        mio_core::visuals::Config,
        Arc<AtomicBool>,
    ),
    DlnaPosition(crate::dlna_cast::Playback, f64),
    BeginDlnaHandoff(crate::dlna_handoff::Transfer),
    EndDlnaHandoff(u64, Option<mio_core::dlna::Position>),
    BeginCapture(crate::microphone::Request),
    EndCapture(u64, bool),
    AutoUpdate(bool, bool, Arc<AtomicBool>),
    RecognitionHistory(crate::recognition::HistoryCommit),
    RestoreBackup(crate::backup_restore::PreparedCommit),
    SaveS3(crate::backup_upload::SaveSettings),
    Queue(crate::queue::Guard, crate::queue::Operation),
    MergeQueue(
        crate::queue::Guard,
        Vec<mio_core::library::PlaylistSong>,
        Arc<Mutex<Option<usize>>>,
    ),
    CycleMode,
    PlayDiscovery(
        crate::discovery::Guard,
        mio_core::online::PreparedSourceSearch,
    ),
    /// Adds the selected online songs behind the queue; playback is untouched.
    AppendDiscovery(
        crate::discovery::Guard,
        mio_core::online::PreparedSourceSearch,
    ),
    AppendLocal(String, u64),
    AppendSaved(
        Vec<mio_core::library::PlaylistSong>,
        crate::catalog::SavedPlayback,
    ),
    PlayRecognition(crate::discovery::Guard, mio_core::recognition::Match, bool),
    Equalizer(crate::equalizer::Prepared),
    LyricStyle(
        mio_core::lyric_style::Config,
        mio_core::lyric_style::Config,
        Arc<AtomicBool>,
    ),
    StorageSettings(
        mio_core::storage_settings::Config,
        mio_core::storage_settings::Config,
        Arc<AtomicBool>,
    ),
    Balance((bool, f64), (bool, f64), Arc<AtomicBool>),
    Seamless(
        mio_core::playback::seamless::Config,
        mio_core::playback::seamless::Config,
        Arc<AtomicBool>,
    ),
    RememberDownload(
        mio_core::app_core::DownloadRequest,
        String,
        Arc<AtomicBool>,
        Option<crate::discovery::Guard>,
    ),
    Host(crate::media::Event),
    Toggle,
    Inspect(Box<dyn FnOnce(&AppCore) + Send>),
    ImportLocal(mio_core::local_music::PreparedImport, Arc<AtomicBool>),
    CommitLocalTags(
        mio_core::local_music::PreparedTagEdit,
        u64,
        Arc<AtomicBool>,
        Arc<Mutex<Option<crate::api::LocalTagSaveResult>>>,
    ),
    PlayLocal(String),
    PlayLocalAt(String, u64),
    PlayLocalAllAt(String, u64, Arc<AtomicBool>),
    SeekDetails(crate::player_details::Token, f64),
    PlaySaved(
        mio_core::library::PlaylistSong,
        Option<Arc<mio_core::online::Online>>,
    ),
    PlaySavedSession(
        mio_core::library::PlaylistSong,
        crate::catalog::SavedPlayback,
    ),
    PlaySavedSelection(
        Vec<mio_core::library::PlaylistSong>,
        crate::catalog::SavedPlayback,
    ),
    Plugins(
        crate::plugins::Prepared,
        Option<Arc<mio_core::online::Online>>,
    ),
    Pause(bool),
    Step(bool),
    Seek(f64),
    SeekRelative(f64),
    Volume(f32),
    PlayIndex(usize),
    Appearance(bool, Option<bool>),
    Stop,
    Shutdown,
}

struct Command {
    action: Action,
    reply: mpsc::SyncSender<Result<(), String>>,
}

struct Published {
    runtime_id: u64,
    host_events: Mutex<crate::media::Events>,
    latest: Mutex<Option<PlayerSnapshot>>,
    artwork: crate::media_artwork::Artwork,
    lyrics: crate::media_lyrics::NativeLyrics,
    changed: Condvar,
    closed: AtomicBool,
    subscribers: AtomicUsize,
    source_revision: AtomicU64,
    playback_revision: AtomicU64,
    host_revision: AtomicU64,
    handoff_epoch: AtomicU64,
    playback_allowed: AtomicBool,
    playback_desired: AtomicBool,
    seek_intent: Mutex<Option<(u64, u64, f64)>>,
}

struct Runtime {
    paths: HostPaths,
    sender: mpsc::SyncSender<Command>,
    published: Arc<Published>,
    thread: JoinHandle<()>,
}

// Ensures a panic/disconnect wakes all consumers instead of leaving them waiting.
struct OwnerExit(Arc<Published>);
impl Drop for OwnerExit {
    fn drop(&mut self) {
        let _guard = self.0.latest.lock().unwrap_or_else(|e| e.into_inner());
        self.0.closed.store(true, Ordering::Release);
        self.0.artwork.close();
        self.0.lyrics.close();
        self.0.changed.notify_all();
    }
}

pub(crate) fn initialize(paths: HostPaths) -> Result<PlayerSnapshot, String> {
    #[cfg(target_os = "android")]
    if !crate::android::context_ready() {
        return Err("context_not_initialized: initialize Android host before audio".into());
    }
    #[cfg(target_env = "ohos")]
    crate::ohos::install_interruptions();
    let mut slot = RUNTIME.lock().map_err(|_| "runtime_failed")?;
    if let Some(runtime) = slot.as_ref() {
        if runtime.paths != paths {
            return Err("already_initialized: application paths differ".into());
        }
        return read(&runtime.published);
    }
    let core_paths = Paths {
        data: paths.data.clone().into(),
        cache: paths.cache.clone().into(),
        downloads: paths.downloads.clone().into(),
    };
    core_paths.validate()?;
    let initial_revision = SOURCE_REVISION.fetch_add(1, Ordering::AcqRel);
    let published = Arc::new(Published {
        runtime_id: initial_revision,
        host_events: Mutex::new(crate::media::Events::default()),
        latest: Mutex::new(None),
        artwork: crate::media_artwork::Artwork::default(),
        lyrics: crate::media_lyrics::NativeLyrics::default(),
        changed: Condvar::new(),
        closed: AtomicBool::new(false),
        subscribers: AtomicUsize::new(0),
        source_revision: AtomicU64::new(initial_revision),
        playback_revision: AtomicU64::new(0),
        host_revision: AtomicU64::new(1),
        handoff_epoch: AtomicU64::new(0),
        playback_allowed: AtomicBool::new(false),
        playback_desired: AtomicBool::new(false),
        seek_intent: Mutex::new(None),
    });
    let (sender, receiver) = mpsc::sync_channel::<Command>(32);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let output = published.clone();
    let thread = std::thread::Builder::new()
        .name("mio-core-owner".into())
        .spawn(move || {
            let _exit = OwnerExit(output.clone());
            let _spectrum_owner = match mio_core::effects::shared().spectrum_owner(initial_revision)
            {
                Ok(owner) => owner,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            struct ProviderExit(mio_core::storage::Storage);
            impl Drop for ProviderExit {
                fn drop(&mut self) {
                    mio_core::list_plugins::invalidate_storage(&self.0);
                }
            }
            let _providers = ProviderExit(mio_core::storage::Storage::from_paths(&core_paths));
            // The cleanup guard also covers a partially failed initialization.
            let mut core = match AppCore::open(core_paths) {
                Ok(core) => core,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };
            crate::imports::recover(&core);
            publish(&core, &output, None);
            let mut media = crate::media::State::new();
            if ready_tx.send(Ok(())).is_err() {
                return;
            }
            loop {
                dispatch_host_events(&mut core, &mut media, &output);
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        let closing = matches!(command.action, Action::Shutdown);
                        let plugins = matches!(
                            command.action,
                            Action::Plugins(..) | Action::RestoreBackup(..)
                        );
                        if !matches!(
                            &command.action,
                            Action::Inspect(_)
                                | Action::SelectSource(_)
                                | Action::DlnaPosition(..)
                                | Action::BeginDlnaHandoff(..)
                                | Action::EndDlnaHandoff(..)
                        ) {
                            output.playback_revision.fetch_add(1, Ordering::AcqRel);
                        }
                        let seeking = matches!(
                            &command.action,
                            Action::Seek(_) | Action::SeekRelative(_) | Action::SeekDetails(..)
                        );
                        let before_position = core.player.position;
                        let mut result =
                            media.dispatch(&mut core, command.action, output.runtime_id);
                        if seeking && (result.is_ok() || before_position != core.player.position) {
                            *output
                                .seek_intent
                                .lock()
                                .unwrap_or_else(|error| error.into_inner()) = Some((
                                output.playback_revision.load(Ordering::Acquire),
                                core.player.generation,
                                core.player.position,
                            ));
                        }
                        if plugins && result.is_ok() {
                            output.source_revision.store(
                                SOURCE_REVISION.fetch_add(1, Ordering::AcqRel),
                                Ordering::Release,
                            );
                        }
                        let finished = closing && result.is_ok();
                        if finished {
                            result = core.downloads.shutdown();
                            mio_core::list_plugins::invalidate_storage(&core.store);
                            output.closed.store(true, Ordering::Release);
                        }
                        output
                            .playback_allowed
                            .store(media.allowed, Ordering::Release);
                        output
                            .playback_desired
                            .store(media.playback_desired(), Ordering::Release);
                        output
                            .handoff_epoch
                            .store(media.handoff_epoch(), Ordering::Release);
                        publish(&core, &output, result.as_ref().err().cloned());
                        let _ = command.reply.send(result);
                        if finished {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = core.save_session();
                        break;
                    }
                }
                // A focus loss arriving during prepare/command handling must
                // be applied before tick can start or advance audio.
                dispatch_host_events(&mut core, &mut media, &output);
                let error = core.tick_with_playback_allowed(media.allowed).err();
                media.observe(&core);
                output
                    .playback_allowed
                    .store(media.allowed, Ordering::Release);
                output
                    .playback_desired
                    .store(media.playback_desired(), Ordering::Release);
                output
                    .handoff_epoch
                    .store(media.handoff_epoch(), Ordering::Release);
                publish(&core, &output, error);
            }
        })
        .map_err(|e| format!("runtime_failed: {e}"))?;
    match ready_rx.recv() {
        Ok(Ok(())) => (),
        result => {
            let _ = thread.join();
            return Err(match result {
                Ok(Err(error)) => error,
                _ => "runtime_failed: initialization interrupted".into(),
            });
        }
    }
    let runtime = Runtime {
        paths,
        sender,
        published,
        thread,
    };
    let snapshot = read(&runtime.published)?;
    *slot = Some(runtime);
    *MEDIA_TARGET.lock().unwrap_or_else(|e| e.into_inner()) =
        slot.as_ref().map(|runtime| runtime.published.clone());
    Ok(snapshot)
}

fn dispatch_host_events(core: &mut AppCore, media: &mut crate::media::State, output: &Published) {
    let events = output
        .host_events
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    for event in events {
        output.playback_revision.fetch_add(1, Ordering::AcqRel);
        let error = media
            .dispatch(core, Action::Host(event), output.runtime_id)
            .err();
        output
            .playback_allowed
            .store(media.allowed, Ordering::Release);
        output
            .playback_desired
            .store(media.playback_desired(), Ordering::Release);
        output
            .handoff_epoch
            .store(media.handoff_epoch(), Ordering::Release);
        publish(core, output, error);
    }
}

pub(crate) fn apply(core: &mut AppCore, action: Action) -> Result<(), String> {
    match action {
        Action::GeneralSettings(expected, next, closed) => {
            crate::settings::apply_general(core, expected, next, &closed)
        }
        Action::SelectSource(commit) => commit.apply(core),
        Action::DlnaPosition(snapshot, seconds) => snapshot.correct(core, seconds),
        Action::Host(_)
        | Action::BeginDlnaHandoff(..)
        | Action::EndDlnaHandoff(..)
        | Action::BeginCapture(..)
        | Action::EndCapture(..)
        | Action::Toggle
        | Action::PlayLocalAt(..)
        | Action::SeekDetails(..)
        | Action::CommitLocalTags(..) => Err("internal_error: media dispatch".into()),
        Action::Queue(guard, op) => crate::queue::apply(core, guard, op),
        Action::MergeQueue(guard, rows, output) => {
            guard.validate(core)?;
            let count = core.merge_imported_queue(rows, guard.online(), || guard.cancelled())?;
            *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(count);
            Ok(())
        }
        Action::CycleMode => core.cycle_play_mode(),
        Action::RememberDownload(request, quality, closed, guard) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            if let Some(guard) = guard {
                guard.validate(core)?;
            }
            core.remember_download_quality(&request, &quality)
                .map_err(|_| {
                    "download_preference_failed: source changed or preferences unavailable".into()
                })
        }
        Action::Inspect(read) => {
            read(core);
            Ok(())
        }
        Action::AutoUpdate(expected, enabled, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_auto_update(expected, enabled)
        }
        Action::RecognitionHistory(prepared) => prepared.apply(core),
        Action::RestoreBackup(prepared) => prepared.apply(core),
        Action::SaveS3(prepared) => prepared.apply(core),
        Action::Plugins(prepared, expected) => {
            let same = match (&core.online, &expected) {
                (Some(actual), Some(expected)) => Arc::ptr_eq(actual, expected),
                (None, None) => true,
                _ => false,
            };
            if !same {
                return Err("source_changed: plugin state changed; retry".into());
            }
            let online = prepared.commit(&core.store)?;
            core.replace_online(Some(online));
            Ok(())
        }
        Action::ImportLocal(prepared, cancelled) => {
            if cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.import_local(prepared)
                .map(|_| ())
                .map_err(|_| "audio_import_failed: cannot save audio library".into())
        }
        Action::PlayLocalAllAt(query, revision, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".into());
            }
            let rows = core
                .local_track_index()
                .iter()
                .filter(|track| {
                    query.is_empty()
                        || [&track.song.title, &track.song.artist, &track.song.album]
                            .iter()
                            .any(|value| value.to_lowercase().contains(&query))
                })
                .take(12_001)
                .map(|track| mio_core::library::PlaylistSong {
                    song: track.song.clone(),
                    source: "local".into(),
                    identity: Some(mio_core::storage::SessionIdentity::local()),
                    quality: None,
                    online: None,
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                return Err("not_found: local songs unavailable".into());
            }
            if rows.len() > 12_000 {
                return Err("capacity_exceeded: local playback".into());
            }
            core.play_saved_playlist(rows, None, || closed.load(Ordering::Acquire))
        }
        Action::PlayLocal(id) => {
            let song = core
                .local_track_index()
                .iter()
                .find(|track| track.song.id == id)
                .map(|track| track.song.clone())
                .ok_or("not_found: local song unavailable")?;
            core.play_saved_song(
                mio_core::library::PlaylistSong {
                    song,
                    source: "local".into(),
                    identity: Some(mio_core::storage::SessionIdentity::local()),
                    quality: None,
                    online: None,
                },
                None,
                || false,
            )
        }
        Action::Equalizer(prepared) => prepared.apply(core),
        Action::StorageSettings(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_storage_settings(expected, next)
        }
        Action::Background(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_background(expected, next)
        }
        Action::Visuals(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_visuals(expected, next)
        }
        Action::LyricStyle(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_lyric_style(expected, next)
        }
        Action::Balance(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_balance(expected, next)
        }
        Action::Seamless(expected, next, closed) => {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            core.save_seamless(expected, next)
        }
        Action::PlaySaved(row, expected) => core.play_saved_song(row, expected.as_ref(), || false),
        Action::PlaySavedSession(row, guard) => {
            guard.validate(core)?;
            core.play_saved_song(row, guard.online.as_ref(), || guard.cancelled())
        }
        Action::PlaySavedSelection(rows, guard) => {
            guard.validate(core)?;
            core.play_saved_playlist(rows, guard.online.as_ref(), || guard.cancelled())
        }
        Action::PlayDiscovery(guard, prepared) => {
            guard.validate(core)?;
            let mut keep = core
                .player
                .queue
                .iter()
                .map(|s| s.id.clone())
                .collect::<std::collections::HashSet<_>>();
            keep.extend(core.player.song().map(|s| s.id.clone()));
            let fetched = guard
                .online
                .publish_source_search(prepared, &keep, &guard.cancelled)?;
            core.play_online_queue(fetched.songs, &guard.online, || guard.is_cancelled())
        }
        Action::AppendDiscovery(guard, prepared) => {
            guard.validate(core)?;
            let mut keep = core
                .player
                .queue
                .iter()
                .map(|s| s.id.clone())
                .collect::<std::collections::HashSet<_>>();
            keep.extend(core.player.song().map(|s| s.id.clone()));
            let fetched = guard
                .online
                .publish_source_search(prepared, &keep, &guard.cancelled)?;
            core.append_online_songs_to_queue(fetched.songs, &guard.online, || guard.is_cancelled())
                .map(|_| ())
        }
        Action::AppendLocal(id, revision) => {
            if core.local_revision() != revision {
                return Err("catalog_changed: refresh local music".into());
            }
            let song = core
                .local_track_index()
                .iter()
                .find(|track| track.song.id == id)
                .map(|track| track.song.clone())
                .ok_or("not_found: local song unavailable")?;
            core.merge_imported_queue(
                vec![mio_core::library::PlaylistSong {
                    song,
                    source: "local".into(),
                    identity: Some(mio_core::storage::SessionIdentity::local()),
                    quality: None,
                    online: None,
                }],
                None,
                || false,
            )
            .map(|_| ())
        }
        Action::AppendSaved(rows, guard) => {
            guard.validate(core)?;
            core.merge_imported_queue(rows, guard.online.as_ref(), || guard.cancelled())
                .map(|_| ())
        }
        Action::PlayRecognition(guard, row, jump_to_match) => {
            guard.validate(core)?;
            core.play_recognized_song(row, &guard.online, &guard.cancelled, jump_to_match)
        }
        Action::Pause(value) => core.set_paused(value),
        Action::Step(value) => core.step(value),
        Action::Seek(value) => core.seek(value),
        Action::SeekRelative(value) => core.seek((core.player.position + value).max(0.0)),
        Action::Volume(value) => core.save_volume(value),
        Action::PlayIndex(index) => core.play_index(index),
        Action::Appearance(dark, english) => core.set_appearance(dark, english),
        Action::Stop => core.stop(),
        Action::Shutdown => core.set_paused(true),
    }
}

pub(crate) fn track(song: &Song) -> Track {
    Track {
        id: song.id.clone(),
        title: song.title.clone(),
        artist: song.artist.clone(),
        album: song.album.clone(),
        duration: song.duration as f64,
        cover: song.cover_art.clone(),
    }
}

fn publish(core: &AppCore, output: &Published, error: Option<String>) {
    let mut latest = output.latest.lock().unwrap_or_else(|e| e.into_inner());
    output
        .artwork
        .observe(core, output.source_revision.load(Ordering::Acquire));
    output
        .lyrics
        .observe(core, output.source_revision.load(Ordering::Acquire));
    let background = core.preferences.background.validated();
    let snapshot = PlayerSnapshot {
        route_preload: core.preferences.app.route_preload,
        library_revision: mio_core::library::revision(),
        local_revision: core.local_revision(),
        cover_theme: core
            .preferences
            .app
            .cover_dynamic_theme
            .then(|| output.artwork.theme())
            .flatten(),
        background_rendering: crate::api::BackgroundRendering {
            audio_response: background.audio_response,
            render_scale: background.render_scale,
            flow_speed: background.flow_speed,
            static_mode: background.static_mode,
            fps: background.fps,
        },
        visuals: crate::settings::project_visuals(core.visuals()),
        lyric_style: crate::settings::project_style(core.lyric_style()),
        revision: latest.as_ref().map_or(0, |s| s.revision),
        status: match core.player.status {
            mio_core::playback::Status::Idle => "idle",
            mio_core::playback::Status::Loading => "loading",
            mio_core::playback::Status::Playing => "playing",
            mio_core::playback::Status::Paused => "paused",
            mio_core::playback::Status::Ended => "ended",
            mio_core::playback::Status::Failed => "failed",
        }
        .into(),
        current: core.player.song().map(track),
        queue_count: core.player.queue.len() as u64,
        queue_revision: core.player.queue_revision,
        playback_generation: core.player.generation,
        source_revision: output.source_revision.load(Ordering::Acquire),
        selected_source: core.preferences.app.selected_source.clone(),
        source_selection_revision: core.source_selection_revision(),
        position: core.player.position,
        duration: core.player.duration,
        volume: core.player.volume,
        play_mode: match core.player.mode {
            mio_core::playback::PlayMode::List => crate::api::PlayMode::List,
            mio_core::playback::PlayMode::Sequence => crate::api::PlayMode::Sequence,
            mio_core::playback::PlayMode::Random => crate::api::PlayMode::Random,
            mio_core::playback::PlayMode::Single => crate::api::PlayMode::Single,
        },
        dark: core.preferences.dark,
        english: core.preferences.english(),
        error: error.or_else(|| core.player.error.clone()),
    };
    if latest.as_ref() != Some(&snapshot) {
        let mut snapshot = snapshot;
        snapshot.revision = snapshot.revision.wrapping_add(1);
        *latest = Some(snapshot);
        output.changed.notify_all();
    }
}

fn read(output: &Published) -> Result<PlayerSnapshot, String> {
    if output.closed.load(Ordering::Acquire) {
        return Err("runtime_closed".into());
    }
    output
        .latest
        .lock()
        .map_err(|_| "runtime_failed")?
        .clone()
        .ok_or_else(|| "not_initialized".into())
}

fn published() -> Result<Arc<Published>, String> {
    RUNTIME
        .lock()
        .map_err(|_| "runtime_failed")?
        .as_ref()
        .map(|r| r.published.clone())
        .ok_or_else(|| "not_initialized".into())
}

pub(crate) fn snapshot() -> Result<PlayerSnapshot, String> {
    read(published()?.as_ref())
}

pub(crate) fn execute(action: Action) -> Result<(), String> {
    let sender = RUNTIME
        .lock()
        .map_err(|_| "runtime_failed")?
        .as_ref()
        .map(|r| r.sender.clone())
        .ok_or("not_initialized")?;
    send(&sender, action)
}

pub(crate) fn execute_media(runtime_id: u64, action: Action) -> Result<(), String> {
    let lease = {
        let slot = RUNTIME.lock().map_err(|_| "runtime_failed")?;
        let runtime = slot.as_ref().ok_or("not_initialized")?;
        if runtime.published.runtime_id != runtime_id {
            return Err("runtime_changed".into());
        }
        RuntimeLease {
            sender: runtime.sender.clone(),
            published: runtime.published.clone(),
        }
    };
    lease.execute(action)
}

pub(crate) fn enqueue_media_event(
    runtime_id: u64,
    event: crate::media::Event,
) -> Result<(), String> {
    // System callbacks never wait behind owner commands or a closing runtime.
    let output = MEDIA_TARGET
        .lock()
        .map_err(|_| "runtime_failed")?
        .clone()
        .ok_or("not_initialized")?;
    if output.runtime_id != runtime_id || output.closed.load(Ordering::Acquire) {
        return Err("runtime_changed".into());
    }
    let mut events = output.host_events.lock().map_err(|_| "runtime_failed")?;
    output.host_revision.fetch_add(1, Ordering::AcqRel);
    events.push(event);
    Ok(())
}

/// Live query avoids a delayed service snapshot dropping focus during an internal pause.
#[cfg(target_os = "android")]
pub(crate) fn dlna_handoff_active(runtime_id: u64) -> bool {
    let output = MEDIA_TARGET
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    output.is_some_and(|output| {
        output.runtime_id == runtime_id
            && !output.closed.load(Ordering::Acquire)
            && output.handoff_epoch.load(Ordering::Acquire)
                == output.host_revision.load(Ordering::Acquire)
    })
}

/// The runtime the system media integration currently addresses.
#[cfg(target_env = "ohos")]
pub(crate) fn media_runtime_id() -> Option<u64> {
    MEDIA_TARGET
        .lock()
        .ok()?
        .as_ref()
        .filter(|output| !output.closed.load(Ordering::Acquire))
        .map(|output| output.runtime_id)
}

/// Native observers read a bounded projection, without cloning the entire queue.
pub(crate) fn media_snapshot(seen_artwork: u64, lyrics_enabled: bool) -> Result<String, String> {
    let output = MEDIA_TARGET
        .lock()
        .map_err(|_| "runtime_failed")?
        .clone()
        .ok_or("not_initialized")?;
    if output.closed.load(Ordering::Acquire) {
        return Err("runtime_closed".into());
    }
    let latest = output.latest.lock().map_err(|_| "runtime_failed")?;
    let state = latest.as_ref().ok_or("not_initialized")?;
    let text = |value: &str| value.chars().take(1024).collect::<String>();
    let current = state.current.as_ref().map(|s| serde_json::json!({
        "id": text(&s.id), "title": text(&s.title), "artist": text(&s.artist), "album": text(&s.album),
        "cover": s.cover.as_ref().map(|s| s.chars().take(4096).collect::<String>()),
    }));
    let (artwork_revision, artwork_png) = output.artwork.snapshot(seen_artwork);
    serde_json::to_string(&serde_json::json!({
        "artwork_revision": artwork_revision, "artwork_png": artwork_png,
        "lyric": output.lyrics.line(state.position, lyrics_enabled),
        "dark": state.dark,
        "english": state.english,
        "runtime_id": output.runtime_id, "status": state.status, "current": current,
        "position": state.position, "duration": state.duration, "revision": state.revision,
    }))
    .map_err(|_| "media_snapshot_failed".into())
}

/// Small projections run on the owner. Disk/network work uses cloned handles
/// returned by this function, never a borrowed AppCore or an owner lock.
pub(crate) fn inspect<T: Send + 'static>(
    read: impl FnOnce(&AppCore) -> T + Send + 'static,
) -> Result<T, String> {
    inspect_with_lease(read).map(|(value, _)| value)
}

/// A follow-up command belongs to the owner that produced its input. Retaining
/// the sender never redirects an old request to a replacement runtime.
#[derive(Clone)]
pub(crate) struct RuntimeLease {
    sender: mpsc::SyncSender<Command>,
    published: Arc<Published>,
}

impl RuntimeLease {
    pub(crate) fn runtime_id(&self) -> u64 {
        self.published.runtime_id
    }
    pub(crate) fn host_revision(&self) -> u64 {
        self.published.host_revision.load(Ordering::Acquire)
    }
    /// Only same-engine enqueue operations may run here: never native/main/disk work.
    pub(crate) fn with_host_epoch(
        &self,
        epoch: u64,
        operation: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let _events = self
            .published
            .host_events
            .lock()
            .map_err(|_| "runtime_failed")?;
        self.ensure_open()?;
        if self.host_revision() != epoch {
            return Err("dlna_playback_changed".into());
        }
        operation()
    }
    pub(crate) fn playback_desired(&self) -> bool {
        self.published.playback_desired.load(Ordering::Acquire)
    }
    pub(crate) fn playback_allowed(&self) -> bool {
        self.published.playback_allowed.load(Ordering::Acquire)
    }
    pub(crate) fn seek_intent(&self) -> Option<(u64, u64, f64)> {
        *self
            .published
            .seek_intent
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
    pub(crate) fn playback_revision(&self) -> u64 {
        self.published.playback_revision.load(Ordering::Acquire)
    }
    pub(crate) fn source_revision(&self) -> u64 {
        self.published.source_revision.load(Ordering::Acquire)
    }
    pub(crate) fn inspect<T: Send + 'static>(
        &self,
        read: impl FnOnce(&AppCore) -> T + Send + 'static,
    ) -> Result<T, String> {
        self.ensure_open()?;
        let (reply, result) = mpsc::sync_channel(1);
        send(
            &self.sender,
            Action::Inspect(Box::new(move |core| {
                let _ = reply.send(read(core));
            })),
        )?;
        let value = result.recv().map_err(|_| "runtime_closed")?;
        self.ensure_open()?;
        Ok(value)
    }
    pub(crate) fn ensure_open(&self) -> Result<(), String> {
        if self.published.closed.load(Ordering::Acquire) {
            Err("runtime_closed".into())
        } else {
            Ok(())
        }
    }

    /// Background lease cleanup must reach its owner even when the ordinary
    /// admission queue is full. No owner/native lock is held by the caller.
    pub(crate) fn end_dlna_handoff(
        &self,
        token: u64,
        position: Option<mio_core::dlna::Position>,
    ) -> Result<(), String> {
        self.ensure_open()?;
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(Command {
                action: Action::EndDlnaHandoff(token, position),
                reply,
            })
            .map_err(|_| "runtime_closed")?;
        result.recv().map_err(|_| "runtime_closed")?
    }
    pub(crate) fn execute(&self, action: Action) -> Result<(), String> {
        self.ensure_open()?;
        send(&self.sender, action)
    }
}

pub(crate) fn inspect_with_lease<T: Send + 'static>(
    read: impl FnOnce(&AppCore) -> T + Send + 'static,
) -> Result<(T, RuntimeLease), String> {
    let lease = RUNTIME
        .lock()
        .map_err(|_| "runtime_failed")?
        .as_ref()
        .map(|runtime| RuntimeLease {
            sender: runtime.sender.clone(),
            published: runtime.published.clone(),
        })
        .ok_or("not_initialized")?;
    lease.ensure_open()?;
    let (reply, result) = mpsc::sync_channel(1);
    send(
        &lease.sender,
        Action::Inspect(Box::new(move |core| {
            let _ = reply.send(read(core));
        })),
    )?;
    let value = result.recv().map_err(|_| "runtime_closed")?;
    lease.ensure_open()?;
    Ok((value, lease))
}

fn send(sender: &mpsc::SyncSender<Command>, action: Action) -> Result<(), String> {
    let (reply, done) = mpsc::sync_channel(1);
    sender
        .try_send(Command { action, reply })
        .map_err(|e| match e {
            mpsc::TrySendError::Full(_) => "busy: command queue is full",
            mpsc::TrySendError::Disconnected(_) => "runtime_closed",
        })?;
    // Once admitted, a mutating command returns its actual result, never a timeout
    // that could be mistaken for cancellation of a committed write.
    done.recv().map_err(|_| "runtime_closed")?
}

pub(crate) fn shutdown() -> Result<(), String> {
    let mut slot = RUNTIME.lock().map_err(|_| "runtime_failed")?;
    let Some(runtime) = slot.as_ref() else {
        return Ok(());
    };
    let result = send(&runtime.sender, Action::Shutdown);
    if result.is_err() && !runtime.published.closed.load(Ordering::Acquire) {
        return result;
    }
    let runtime = slot.take().ok_or("runtime_closed")?;
    runtime
        .thread
        .join()
        .map_err(|_| "runtime_failed: owner panicked")?;
    result
}

pub(crate) struct Subscription {
    published: Arc<Published>,
    closed: AtomicBool,
}

pub(crate) fn subscribe() -> Result<Subscription, String> {
    let output = published()?;
    if output.closed.load(Ordering::Acquire) {
        return Err("runtime_closed".into());
    }
    output
        .subscribers
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < 32).then_some(count + 1)
        })
        .map_err(|_| "busy: too many subscriptions")?;
    Ok(Subscription {
        published: output,
        closed: AtomicBool::new(false),
    })
}

impl Subscription {
    pub(crate) fn next(&self, after: Option<u64>) -> Result<Option<PlayerSnapshot>, String> {
        let output = &self.published;
        let latest = output.latest.lock().map_err(|_| "runtime_failed")?;
        let (latest, _) = output
            .changed
            .wait_timeout_while(latest, Duration::from_secs(1), |state| {
                !self.closed.load(Ordering::Acquire)
                    && !output.closed.load(Ordering::Acquire)
                    && state.as_ref().is_none_or(|s| after == Some(s.revision))
            })
            .map_err(|_| "runtime_failed")?;
        if self.closed.load(Ordering::Acquire) {
            return Ok(None);
        }
        if output.closed.load(Ordering::Acquire) {
            return Err("runtime_closed".into());
        }
        Ok(latest
            .as_ref()
            .filter(|s| after != Some(s.revision))
            .cloned())
    }

    pub(crate) fn close(&self) {
        let _guard = self
            .published
            .latest
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.published.subscribers.fetch_sub(1, Ordering::AcqRel);
            self.published.changed.notify_all();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.close();
    }
}
