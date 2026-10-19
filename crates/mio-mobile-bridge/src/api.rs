use crate::runtime;
use flutter_rust_bridge::frb;

#[derive(Clone, Debug)]
pub struct DeviceCapabilities {
    pub limited_memory: bool,
    pub television: bool,
    pub microphone: bool,
    pub tag_edit: bool,
    pub dlna: bool,
}

#[frb(sync)]
pub fn capabilities() -> Result<DeviceCapabilities, String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::capabilities();
    }
    #[cfg(not(target_os = "android"))]
    Ok(DeviceCapabilities {
        limited_memory: false,
        television: false,
        microphone: cfg!(any(target_os = "ios", target_env = "ohos")),
        tag_edit: true,
        dlna: false,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostPaths {
    pub data: String,
    pub cache: String,
    pub downloads: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub cover: Option<String>,
}

/// One source-bound favorite intent captured from the displayed row session.
#[frb(opaque)]
pub struct FavoriteIntent {
    pub(crate) intent: crate::favorites::Intent,
}

#[derive(Clone, Debug)]
pub struct RecentTrack {
    pub index: u32,
    pub track: Track,
}
#[derive(Clone, Debug)]
pub struct RecentPage {
    pub items: Vec<RecentTrack>,
    pub total: u32,
}
impl FavoriteIntent {
    pub fn state(&self) -> Result<bool, String> {
        self.intent.state()
    }
    pub fn set_favorite(&self, liked: bool) -> Result<bool, String> {
        self.intent.set(liked)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.intent.close();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayMode {
    List,
    Sequence,
    Random,
    Single,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoverTheme {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub use_black_text: bool,
}
#[derive(Clone, Debug)]
pub struct ThemeColor {
    pub key: String,
    pub argb: u32,
}
/// Pure bounded token projection; no runtime, persistence or image requests.
#[frb(sync)]
pub fn cover_theme_colors(theme: CoverTheme) -> Vec<ThemeColor> {
    let colors = mio_core::song_theme::SongColors {
        base: [theme.red, theme.green, theme.blue],
        use_black_text: theme.use_black_text,
    };
    mio_core::song_theme::KEYS
        .iter()
        .filter_map(|key| {
            colors.color(key).map(|c| {
                let byte = |value: f32| (value * 255.).round().clamp(0., 255.) as u32;
                ThemeColor {
                    key: (*key).into(),
                    argb: (byte(c.a) << 24) | (byte(c.r) << 16) | (byte(c.g) << 8) | byte(c.b),
                }
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnapshot {
    pub library_revision: u64,
    pub local_revision: u64,
    pub cover_theme: Option<CoverTheme>,
    pub background_rendering: BackgroundRendering,
    pub visuals: VisualSettings,
    pub lyric_style: LyricStyleSettings,
    pub revision: u64,
    pub status: String,
    pub current: Option<Track>,
    pub queue_count: u64,
    pub queue_revision: u64,
    pub playback_generation: u64,
    pub source_revision: u64,
    pub selected_source: String,
    pub source_selection_revision: u64,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub play_mode: PlayMode,
    pub dark: bool,
    pub route_preload: bool,
    pub english: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LyricWord {
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,
    pub romanization: String,
}
#[derive(Clone, Debug)]
pub struct LyricRow {
    pub start_ms: Option<f64>,
    pub end_ms: Option<f64>,
    pub text: String,
    pub translation: Option<String>,
    pub romanization: Option<String>,
    pub words: Vec<LyricWord>,
    pub background: bool,
    pub duet: bool,
    pub seek_seconds: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct LyricDocument {
    pub rows: Vec<LyricRow>,
    pub synced: bool,
    pub word_timed: bool,
}
#[derive(Clone, Debug)]
pub struct LyricClock {
    pub position_ms: f64,
    pub playing: bool,
    pub active_row: Option<u32>,
}
#[frb(opaque)]
pub struct PlayerDetails {
    pub(crate) session: crate::player_details::Session,
}
impl PlayerDetails {
    pub fn comments_available(&self) -> Result<bool, String> {
        self.session.comments_available()
    }
    pub fn open_comments(&self) -> Result<CommentsSession, String> {
        self.session.open_comments()
    }
    pub fn singer_available(&self) -> Result<bool, String> {
        self.session.singer_available()
    }
    pub fn open_singer(&self) -> Result<SingerSession, String> {
        self.session.open_singer()
    }

    pub fn prepare_download(&self) -> Result<DownloadChoice, String> {
        self.session.prepare_download()
    }
    pub fn favorite(&self) -> Result<bool, String> {
        self.session.favorite()
    }
    pub fn set_favorite(&self, liked: bool) -> Result<bool, String> {
        self.session.set_favorite(liked)
    }
    pub fn lyrics(&self) -> Result<LyricDocument, String> {
        self.session.lyrics()
    }
    pub fn cover(&self) -> Result<Option<Vec<u8>>, String> {
        self.session.cover()
    }
    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        self.session.seek(seconds)
    }
    pub fn clock(&self) -> Result<LyricClock, String> {
        self.session.clock()
    }
    pub fn seek_line(&self, index: u32) -> Result<(), String> {
        self.session.seek_line(index)
    }
    #[frb(sync)]
    pub fn cancel(&self) {
        self.session.cancel();
    }
}
pub fn begin_player_details(
    song_id: String,
    generation: u64,
    queue_revision: u64,
    source_revision: u64,
) -> Result<PlayerDetails, String> {
    Ok(PlayerDetails {
        session: crate::player_details::begin(
            song_id,
            generation,
            queue_revision,
            source_revision,
        )?,
    })
}
pub fn seek_current(
    song_id: String,
    generation: u64,
    queue_revision: u64,
    source_revision: u64,
    seconds: f64,
) -> Result<(), String> {
    crate::player_details::seek_current(
        song_id,
        generation,
        queue_revision,
        source_revision,
        seconds,
    )
}

#[derive(Clone, Debug)]
pub struct TrackPage {
    pub items: Vec<Track>,
    pub total: u32,
    pub offset: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct PlaylistInfo {
    pub create_date: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub cover: String,
    pub source: String,
    pub count: u32,
    pub favorite: bool,
}

#[derive(Clone, Debug)]
pub struct PlaylistPage {
    pub items: Vec<PlaylistInfo>,
    pub total: u32,
    pub offset: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug)]
pub struct SavedPlaylistPage {
    pub playlist: PlaylistInfo,
    pub tracks: TrackPage,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DownloadState {
    Queued,
    Resolving,
    Downloading,
    Paused,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DownloadBucket {
    Active,
    Completed,
    Failed,
}

#[derive(Clone, Debug)]
pub struct DownloadItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub quality: String,
    pub state: DownloadState,
    pub received: u64,
    pub total: Option<u64>,
    pub speed: f64,
    pub remaining: Option<f64>,
    pub missing: bool,
    pub skipped: bool,
    pub tag_failed: bool,
}

#[derive(Clone, Debug)]
pub struct DownloadPage {
    pub concurrent: u32,
    pub can_start: bool,
    pub can_pause: bool,
    pub items: Vec<DownloadItem>,
    pub total: u32,
    pub offset: u32,
    pub has_more: bool,
    pub counts: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct DownloadQuality {
    pub quality: String,
    pub label_key: Option<String>,
    pub size: Option<String>,
}
#[derive(Clone, Debug)]
pub struct DownloadChoiceInfo {
    pub title: String,
    pub source: String,
    pub remembered: String,
    pub choices: Vec<DownloadQuality>,
}
#[derive(Clone, Copy, Debug)]
pub enum DownloadEnqueueResult {
    Added,
    Duplicate,
    AlreadyDone,
}
#[frb(opaque)]
pub struct DownloadChoice {
    pub(crate) choice: crate::downloads::Choice,
}
impl DownloadChoice {
    #[frb(sync)]
    pub fn close(&self) {
        self.choice.close();
    }
    pub fn info(&self) -> Result<DownloadChoiceInfo, String> {
        self.choice.info()
    }
    pub fn enqueue(&self, quality: String) -> Result<DownloadEnqueueResult, String> {
        self.choice.enqueue(quality)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DownloadAction {
    Pause,
    Resume,
    Retry,
    Cancel,
    Remove,
}

#[derive(Clone, Copy, Debug)]
pub enum DownloadBulkAction {
    StartAll,
    PauseAll,
    Clear,
}

/// Retain one immutable download ordering while browsing its pages.
#[frb(opaque)]
pub struct DownloadSnapshot {
    pub(crate) concurrent: u32,
    pub(crate) items: Vec<DownloadItem>,
    pub(crate) counts: Vec<u32>,
    pub(crate) bucket: DownloadBucket,
    pub(crate) bulk_targets: Vec<(String, DownloadState)>,
    pub(crate) control: crate::catalog::DownloadControl,
    pub(crate) _permit: crate::catalog::SnapshotPermit,
}

impl DownloadSnapshot {
    /// The toolbar ignores the text filter, matching desktop, but only
    /// addresses IDs captured in this snapshot's original bucket.
    pub fn bulk_command(&self, action: DownloadBulkAction) -> Result<(), String> {
        self.control
            .bulk_command(&self.bulk_targets, self.bucket, action)
    }
    pub fn set_concurrent(&self, next: u32) -> Result<u32, String> {
        self.control.set_concurrent(self.concurrent, next)
    }
    pub fn completed_file_name(&self, id: String) -> Result<String, String> {
        let row = self
            .items
            .iter()
            .find(|row| row.id == id)
            .ok_or("not_found: download is not in this snapshot")?;
        if row.state != DownloadState::Done || row.missing {
            return Err("file_unavailable: refresh completed downloads".into());
        }
        self.control.completed_file_name(&id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.control.close();
    }
    /// Requests a command for a row in this immutable snapshot. The manager
    /// applies only valid live transitions; refresh to see the actual result.
    pub fn command(&self, id: String, action: DownloadAction) -> Result<(), String> {
        let row = self
            .items
            .iter()
            .find(|row| row.id == id)
            .ok_or("not_found: download is not in this snapshot")?;
        let allowed = match action {
            DownloadAction::Pause => matches!(
                row.state,
                DownloadState::Queued | DownloadState::Resolving | DownloadState::Downloading
            ),
            DownloadAction::Resume => row.state == DownloadState::Paused,
            DownloadAction::Retry => {
                matches!(row.state, DownloadState::Failed | DownloadState::Cancelled)
            }
            DownloadAction::Cancel => matches!(
                row.state,
                DownloadState::Queued
                    | DownloadState::Resolving
                    | DownloadState::Downloading
                    | DownloadState::Paused
            ),
            DownloadAction::Remove => matches!(
                row.state,
                DownloadState::Done | DownloadState::Failed | DownloadState::Cancelled
            ),
        };
        if !allowed {
            return Err("state_changed: refresh downloads".into());
        }
        self.control.command(&id, action)
    }
    pub fn page(&self, offset: u32, limit: u32) -> Result<DownloadPage, String> {
        self.control.ensure_open()?;
        if limit == 0 || limit > 200 || offset > i32::MAX as u32 {
            return Err("invalid_input: invalid download page bounds".into());
        }
        let items: Vec<_> = self
            .items
            .iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(DownloadPage {
            concurrent: self.concurrent,
            can_start: self
                .bulk_targets
                .iter()
                .any(|(_, s)| *s == DownloadState::Paused),
            can_pause: self.bulk_targets.iter().any(|(_, s)| {
                matches!(
                    s,
                    DownloadState::Queued | DownloadState::Resolving | DownloadState::Downloading
                )
            }),
            has_more: offset as usize + items.len() < self.items.len(),
            items,
            total: self.items.len() as u32,
            offset,
            counts: self.counts.clone(),
        })
    }
}

pub fn download_snapshot(
    bucket: DownloadBucket,
    query: String,
) -> Result<DownloadSnapshot, String> {
    crate::catalog::download_snapshot(bucket, query)
}

pub fn local_tracks(query: String, offset: u32, limit: u32) -> Result<TrackPage, String> {
    crate::catalog::local_tracks(query, offset, limit)
}

/// A revision-bound local index; refresh after imports or metadata edits.
#[frb(opaque)]
pub struct LocalTrackSession {
    pub(crate) session: crate::catalog::LocalSession,
}
impl LocalTrackSession {
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
    pub fn play_all(&self) -> Result<(), String> {
        self.session.play_all()
    }
    pub fn cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        self.session.cover(id)
    }
    pub fn prepare_tag_editor(&self, id: String) -> Result<LocalTagEditor, String> {
        if !capabilities()?.tag_edit {
            return Err("unsupported: tag editing on television".into());
        }
        self.session.prepare_tag_editor(id)
    }

    pub fn prepare_favorite(&self, id: String) -> Result<FavoriteIntent, String> {
        self.session.prepare_favorite(id)
    }
    pub fn play(&self, id: String) -> Result<(), String> {
        self.session.play(id)
    }
    /// Adds the track behind the current queue without interrupting playback.
    pub fn append(&self, id: String) -> Result<(), String> {
        self.session.append(id)
    }
    /// Adds the track to an existing saved playlist; returns how many were added.
    pub fn add_to_playlist(&self, id: String, destination_id: String) -> Result<u32, String> {
        self.session.add_to_playlist(id, destination_id)
    }
    pub fn page(&self, offset: u32, limit: u32) -> Result<TrackPage, String> {
        self.session.page(offset, limit)
    }
}
pub fn begin_local_tracks(query: String) -> Result<LocalTrackSession, String> {
    crate::catalog::begin_local(query)
}

pub struct PlaylistDraft {
    pub name: String,
    pub description: String,
}
/// A metadata form bound to the original runtime and expected saved values.
#[frb(opaque)]
pub struct PlaylistEditor {
    pub(crate) editor: crate::catalog::PlaylistEditorState,
}
impl PlaylistEditor {
    pub fn draft(&self) -> PlaylistDraft {
        self.editor.draft()
    }
    pub fn save(&self, name: String, description: String) -> Result<PlaylistInfo, String> {
        self.editor.save(name, description)
    }
}
pub fn begin_playlist_editor(id: Option<String>) -> Result<PlaylistEditor, String> {
    crate::catalog::begin_playlist_editor(id)
}

/// Display order shared with the desktop; never persists song positions.
#[derive(Clone, Copy, Debug)]
pub enum PlaylistSort {
    Default,
    TitleAsc,
    TitleDesc,
    ArtistAsc,
    ArtistDesc,
    AlbumAsc,
    AlbumDesc,
    DurationAsc,
    DurationDesc,
}
impl PlaylistSort {
    pub(crate) fn core(self) -> mio_core::song_sort::Sort {
        use mio_core::song_sort::Sort;
        match self {
            Self::Default => Sort::Default,
            Self::TitleAsc => Sort::TitleAsc,
            Self::TitleDesc => Sort::TitleDesc,
            Self::ArtistAsc => Sort::ArtistAsc,
            Self::ArtistDesc => Sort::ArtistDesc,
            Self::AlbumAsc => Sort::AlbumAsc,
            Self::AlbumDesc => Sort::AlbumDesc,
            Self::DurationAsc => Sort::DurationAsc,
            Self::DurationDesc => Sort::DurationDesc,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum SortColumn {
    Title,
    Album,
    Duration,
}
#[frb(sync)]
pub fn cycle_playlist_sort(sort: PlaylistSort, column: SortColumn) -> PlaylistSort {
    use mio_core::song_sort::{Column, Sort};
    let column = match column {
        SortColumn::Title => Column::Title,
        SortColumn::Album => Column::Album,
        SortColumn::Duration => Column::Duration,
    };
    match sort.core().cycle(column) {
        Sort::Default => PlaylistSort::Default,
        Sort::TitleAsc => PlaylistSort::TitleAsc,
        Sort::TitleDesc => PlaylistSort::TitleDesc,
        Sort::ArtistAsc => PlaylistSort::ArtistAsc,
        Sort::ArtistDesc => PlaylistSort::ArtistDesc,
        Sort::AlbumAsc => PlaylistSort::AlbumAsc,
        Sort::AlbumDesc => PlaylistSort::AlbumDesc,
        Sort::DurationAsc => PlaylistSort::DurationAsc,
        Sort::DurationDesc => PlaylistSort::DurationDesc,
    }
}

/// A connection-bound playlist cursor, invalidated by database writes/replacement.
#[frb(opaque)]
pub struct LibrarySession {
    pub(crate) session: crate::catalog::LibrarySessionState,
}
impl LibrarySession {
    /// Decode only an embedded image from the original cursor, never an arbitrary host path.
    pub fn cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        self.session.cover(id)
    }
    pub fn prepare_cover_editor(&self) -> Result<PlaylistCoverEditor, String> {
        self.session.prepare_cover_editor()
    }

    pub fn prepare_favorite(&self, song_id: String) -> Result<FavoriteIntent, String> {
        self.session.prepare_favorite(song_id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
    /// Delete the displayed playlist after a UI confirmation. A changed cursor
    /// is rejected; admitted storage work returns its actual committed outcome.
    pub fn delete_playlist(&self) -> Result<(), String> {
        self.session.delete_playlist()
    }
    /// Replace playback queue with original selected members in display order.
    pub fn play_songs(&self, song_ids: Vec<String>, sort: PlaylistSort) -> Result<(), String> {
        self.session.play_songs(song_ids, sort)
    }
    /// Copy original members into an existing or new saved playlist atomically.
    /// Successful commit invalidates this cursor; returns newly inserted count.
    pub fn copy_songs(
        &self,
        song_ids: Vec<String>,
        destination_id: Option<String>,
        new_name: Option<String>,
    ) -> Result<u32, String> {
        self.session.copy_songs(song_ids, destination_id, new_name)
    }
    pub fn destinations(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        self.session.destinations(offset, limit)
    }
    /// Remove original members atomically; commit invalidates this cursor.
    pub fn remove_songs(&self, song_ids: Vec<String>) -> Result<(), String> {
        self.session.remove_songs(song_ids)
    }
    pub fn play(&self, song_id: String) -> Result<(), String> {
        self.session.play(song_id)
    }
    /// Adds the saved song behind the current queue without interrupting playback.
    pub fn append_song(&self, song_id: String) -> Result<(), String> {
        self.session.append_song(song_id)
    }
    /// Play the full captured playlist, rejecting oversized or changed lists.
    pub fn play_all(&self) -> Result<(), String> {
        self.session.play_all()
    }
    pub fn playlists(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        self.session.playlists(offset, limit)
    }
    pub fn playlist(&self, offset: u32, limit: u32) -> Result<SavedPlaylistPage, String> {
        self.session.playlist(offset, limit)
    }
    pub fn sorted_playlist(
        &self,
        sort: PlaylistSort,
        offset: u32,
        limit: u32,
    ) -> Result<SavedPlaylistPage, String> {
        self.session.sorted_playlist(sort, offset, limit)
    }
}
pub fn begin_library(id: Option<String>) -> Result<LibrarySession, String> {
    crate::catalog::begin_library(id)
}

pub fn playlists(offset: u32, limit: u32) -> Result<PlaylistPage, String> {
    crate::catalog::playlists(offset, limit)
}

pub fn saved_playlist(id: String, offset: u32, limit: u32) -> Result<SavedPlaylistPage, String> {
    crate::catalog::saved_playlist(id, offset, limit)
}

pub fn downloads(
    bucket: DownloadBucket,
    query: String,
    offset: u32,
    limit: u32,
) -> Result<DownloadPage, String> {
    download_snapshot(bucket, query)?.page(offset, limit)
}

pub fn play_local(id: String) -> Result<(), String> {
    crate::catalog::play_local(id)
}

#[derive(Clone, Debug)]
pub struct SearchSource {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SearchPage {
    pub items: Vec<Track>,
    pub labels: Vec<SearchRowLabel>,
    pub singer_ids: Vec<String>,
    pub total: u32,
    pub has_more: bool,
    pub sources: Vec<SearchSourceState>,
}

#[derive(Clone, Debug)]
pub struct SearchRowLabel {
    pub id: String,
    pub source: String,
    /// Locale key for known qualities, otherwise the bounded provider label.
    pub quality: String,
}

#[derive(Clone, Debug)]
pub struct SearchSourceState {
    pub id: String,
    pub name: String,
    pub total: u32,
    pub has_more: bool,
    pub failed: bool,
}

#[frb(opaque)]
pub struct SearchSession {
    pub(crate) session: crate::search::Session,
}

impl SearchSession {
    pub fn prepare_selected_save(&self, ids: Vec<String>) -> Result<OnlineSaveSession, String> {
        self.session.prepare_selected_save(ids)
    }

    pub fn open_singer(&self, id: String) -> Result<SingerSession, String> {
        self.session.open_singer(id)
    }
    pub fn prepare_favorite(&self, id: String) -> Result<FavoriteIntent, String> {
        self.session.prepare_favorite(id)
    }
    /// Returns the complete deduplicated list, including aggregate replacements.
    pub fn next_page(&self) -> Result<SearchPage, String> {
        self.session.next(false)
    }
    pub fn retry_failed(&self) -> Result<SearchPage, String> {
        self.session.next(true)
    }
    #[frb(sync)]
    pub fn cancel(&self) {
        self.session.cancel();
    }
    pub fn play(&self, id: String) -> Result<(), String> {
        self.session.play(id)
    }
    pub fn append(&self, id: String) -> Result<(), String> {
        self.session.append(id)
    }
}

pub fn search_sources() -> Result<Vec<SearchSource>, String> {
    crate::search::sources()
}

#[derive(Clone, Copy, Debug)]
pub enum PluginKind {
    Playback,
    List,
}
#[derive(Clone, Debug)]
pub struct PluginOption {
    pub value: String,
    pub label: String,
}
#[derive(Clone, Debug)]
pub struct PluginField {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub value_json: String,
    pub required: bool,
    pub placeholder: String,
    pub description: String,
    pub options: Vec<PluginOption>,
}
#[derive(Clone, Debug)]
pub struct PluginInfo {
    pub revision: u64,
    pub id: String,
    pub kind: PluginKind,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub sources: Vec<SearchSource>,
    pub enabled: bool,
    pub active: bool,
    pub can_play: bool,
    pub can_download: bool,
    pub provide_play: bool,
    pub provide_download: bool,
    pub fields: Vec<PluginField>,
    /// Ships with the app: always installed, switched off rather than removed.
    pub bundled: bool,
    /// Every required field has a value; until then the plugin offers no sources.
    pub configured: bool,
}
/// Read the same runtime/saved playback-plugin log shown by the desktop UI.
pub fn playback_plugin_logs(id: String) -> Result<Vec<String>, String> {
    crate::plugins::logs(id)
}
pub fn installed_plugins() -> Result<Vec<PluginInfo>, String> {
    crate::plugins::installed()
}
pub fn install_plugin_url(url: String) -> Result<(), String> {
    crate::plugins::install_url(url)
}
pub fn install_plugin_code(code: String, expected_revision: u64) -> Result<(), String> {
    crate::plugins::install_code(code, expected_revision)
}
pub fn activate_playback_plugin(id: String) -> Result<(), String> {
    crate::plugins::activate(id)
}
pub fn set_list_plugin_enabled(id: String, enabled: bool) -> Result<(), String> {
    crate::plugins::enabled(id, enabled)
}
pub fn configure_list_plugin(
    id: String,
    values_json: String,
    expected_revision: u64,
) -> Result<(), String> {
    crate::plugins::configure(id, values_json, expected_revision)
}
pub fn set_list_plugin_provide(id: String, play: bool, download: bool) -> Result<(), String> {
    crate::plugins::provide(id, play, download)
}
pub fn remove_plugin(kind: PluginKind, id: String) -> Result<(), String> {
    crate::plugins::remove(kind, id)
}
pub fn begin_search(query: String, source: String) -> Result<SearchSession, String> {
    Ok(SearchSession {
        session: crate::search::begin(query, source)?,
    })
}

pub fn play_saved_song(playlist_id: String, song_id: String) -> Result<(), String> {
    crate::catalog::play_saved_song(playlist_id, song_id)
}

pub fn bridge_version() -> String {
    format!("Mio {} / FRB 2.13.0", env!("CARGO_PKG_VERSION"))
}

/// Reconnecting a view with the same paths reuses the process runtime.
pub fn initialize(paths: HostPaths) -> Result<PlayerSnapshot, String> {
    runtime::initialize(paths)
}

pub fn player_snapshot() -> Result<PlayerSnapshot, String> {
    runtime::snapshot()
}

pub fn set_paused(paused: bool) -> Result<(), String> {
    runtime::execute(runtime::Action::Pause(paused))
}

pub fn step(forward: bool) -> Result<(), String> {
    runtime::execute(runtime::Action::Step(forward))
}

pub fn seek(seconds: f64) -> Result<(), String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("invalid_input: position must be finite and nonnegative".into());
    }
    runtime::execute(runtime::Action::Seek(seconds))
}

pub fn set_volume(volume: f32) -> Result<(), String> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err("invalid_input: volume must be between zero and one".into());
    }
    runtime::execute(runtime::Action::Volume(volume))
}

pub fn play_index(index: u32) -> Result<(), String> {
    runtime::execute(runtime::Action::PlayIndex(index as usize))
}

pub fn set_appearance(dark: bool, english: Option<bool>) -> Result<(), String> {
    runtime::execute(runtime::Action::Appearance(dark, english))
}

pub fn stop() -> Result<(), String> {
    runtime::execute(runtime::Action::Stop)
}

/// Use only for application shutdown, never for page/view disposal.
pub fn shutdown() -> Result<(), String> {
    runtime::shutdown()
}

#[frb(opaque)]
pub struct PlayerSubscription {
    inner: runtime::Subscription,
}

pub fn subscribe_player() -> Result<PlayerSubscription, String> {
    Ok(PlayerSubscription {
        inner: runtime::subscribe()?,
    })
}

impl PlayerSubscription {
    /// First call returns the current snapshot. Later calls coalesce updates;
    /// Timeout/view close return None; runtime shutdown returns runtime_closed.
    /// No per-subscriber event queue is allocated.
    pub fn next(&self, after_revision: Option<u64>) -> Result<Option<PlayerSnapshot>, String> {
        self.inner.next(after_revision)
    }

    pub fn close(&self) {
        self.inner.close();
    }
}

/// Bound before opening the native chooser; commits only to its original owner.
#[frb(opaque)]
pub struct AudioImport {
    pub(crate) session: crate::imports::Session,
}
impl AudioImport {
    pub fn commit(&self, ticket: String) -> Result<u32, String> {
        self.session.commit(ticket)
    }
    #[frb(sync)]
    pub fn cancel(&self) {
        self.session.cancel();
    }
}
pub fn begin_audio_import() -> Result<AudioImport, String> {
    crate::imports::begin().map(|session| AudioImport { session })
}

#[frb(opaque)]
pub struct QueueSession {
    pub(crate) session: crate::queue::Session,
}
impl QueueSession {
    /// At most 64 rows and 256 KiB of projected text from the original queue.
    pub fn page(&self, offset: u32) -> Result<QueuePage, String> {
        self.session.page(offset)
    }
    pub fn locate(&self, id: String) -> Result<Option<u32>, String> {
        self.session.locate(id)
    }
    /// Desktop recent-page order: last 200 queue entries, reversed.
    pub fn recent(&self) -> Result<RecentPage, String> {
        self.session.recent()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
    pub fn play(&self, index: u32, id: String) -> Result<(), String> {
        self.session.play(index, id)
    }
    pub fn remove(&self, id: String) -> Result<(), String> {
        self.session.remove(id)
    }
    pub fn move_song(&self, from: u32, boundary: u32) -> Result<(), String> {
        self.session.move_song(from, boundary)
    }
    pub fn clear(&self) -> Result<(), String> {
        self.session.clear()
    }
    pub fn cycle_mode(&self) -> Result<(), String> {
        self.session.mode()
    }
}
#[derive(Clone, Debug)]
pub struct QueuePage {
    pub offset: u32,
    pub total: u32,
    pub items: Vec<Track>,
}
pub fn begin_queue(revision: u64, source_revision: u64) -> Result<QueueSession, String> {
    crate::queue::begin(revision, source_revision).map(|session| QueueSession { session })
}
pub fn cycle_play_mode() -> Result<(), String> {
    runtime::execute(runtime::Action::CycleMode)
}

#[derive(Clone, Copy, Debug)]
pub struct SeamlessSettings {
    pub enabled: bool,
    pub crossfade: bool,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneralSettings {
    pub dark: bool,
    pub language: String,
    pub route_preload: bool,
}
#[derive(Clone, Debug)]
pub struct SettingsOverview {
    pub settings: GeneralSettings,
    pub paths: HostPaths,
    pub queue_count: u32,
    pub queue_seconds: u64,
    pub artist_count: u32,
    pub cache_files: u32,
    pub cache_bytes: u64,
}
#[frb(opaque)]
pub struct GeneralSettingsSession {
    pub(crate) session: crate::settings::Session,
}
impl GeneralSettingsSession {
    pub fn overview(&self) -> Result<SettingsOverview, String> {
        self.session.overview()
    }
    pub fn save(
        &self,
        expected: GeneralSettings,
        next: GeneralSettings,
    ) -> Result<GeneralSettings, String> {
        self.session.save_general(expected, next)
    }
    pub fn clear_audio_cache(&self) -> Result<(), String> {
        self.session.clear_audio_cache()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_general_settings() -> Result<GeneralSettingsSession, String> {
    Ok(GeneralSettingsSession {
        session: crate::settings::new_session()?,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorageSettings {
    pub auto_cache: bool,
    pub cache_limit: u64,
    pub filename_template: String,
    pub basic_info: bool,
    pub cover: bool,
    pub lyrics: bool,
    pub download_lyrics: bool,
}
#[frb(opaque)]
pub struct StorageSettingsSession {
    pub(crate) session: crate::settings::Session,
}
impl StorageSettingsSession {
    pub fn state(&self) -> Result<StorageSettings, String> {
        self.session.storage_settings()
    }
    pub fn save(
        &self,
        expected: StorageSettings,
        next: StorageSettings,
    ) -> Result<StorageSettings, String> {
        self.session.save_storage_settings(expected, next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
#[frb(sync)]
pub fn preview_download_filename(template: String, english: bool) -> Result<String, String> {
    mio_core::storage_settings::filename_preview(&template, english)
}
pub fn begin_storage_settings() -> Result<StorageSettingsSession, String> {
    Ok(StorageSettingsSession {
        session: crate::settings::new_session()?,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct LyricStyleSettings {
    pub family: String,
    pub rate: f32,
    pub weight: u32,
}
#[frb(opaque)]
pub struct LyricSettingsSession {
    pub(crate) session: crate::settings::Session,
}
impl LyricSettingsSession {
    pub fn state(&self) -> Result<LyricStyleSettings, String> {
        self.session.lyric_style()
    }
    pub fn save(
        &self,
        expected: LyricStyleSettings,
        next: LyricStyleSettings,
    ) -> Result<LyricStyleSettings, String> {
        self.session.save_lyric_style(expected, next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_lyric_settings() -> Result<LyricSettingsSession, String> {
    Ok(LyricSettingsSession {
        session: crate::settings::new_session()?,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct BalanceSettings {
    pub enabled: bool,
    pub balance: f64,
}

#[frb(opaque)]
pub struct AudioEffectsSession {
    pub(crate) session: crate::settings::Session,
}
impl AudioEffectsSession {
    pub fn state(&self) -> Result<BalanceSettings, String> {
        self.session.balance()
    }
    pub fn save(
        &self,
        expected: BalanceSettings,
        next: BalanceSettings,
    ) -> Result<BalanceSettings, String> {
        self.session.save_balance(expected, next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_audio_effects() -> Result<AudioEffectsSession, String> {
    Ok(AudioEffectsSession {
        session: crate::settings::new_session()?,
    })
}

#[frb(opaque)]
pub struct PlaybackSettingsSession {
    pub(crate) session: crate::settings::Session,
}
impl PlaybackSettingsSession {
    pub fn state(&self) -> Result<SeamlessSettings, String> {
        self.session.state()
    }
    pub fn save(
        &self,
        expected: SeamlessSettings,
        next: SeamlessSettings,
    ) -> Result<SeamlessSettings, String> {
        self.session.save(expected, next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_playback_settings() -> Result<PlaybackSettingsSession, String> {
    crate::settings::begin()
}

#[derive(Clone, Copy, Debug)]
pub enum EqFilter {
    Peak,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
}
#[derive(Clone, Debug)]
pub struct EqBand {
    pub frequency: f64,
    pub gain: f64,
    pub q: f64,
    pub kind: EqFilter,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct EqPreset {
    pub id: String,
    pub name: String,
    pub locale_key: Option<String>,
    pub builtin: bool,
}
#[derive(Clone, Debug)]
pub struct SpectrumSnapshot {
    pub age_ms: u32,
    pub generation: u64,
    pub playing: bool,
    pub fresh: bool,
    pub levels: Vec<f64>,
}
#[derive(Clone, Debug)]
pub struct EqResponsePoint {
    pub frequency: f64,
    pub db: f64,
}
#[derive(Clone, Debug)]
pub struct EqSnapshot {
    pub revision: u64,
    pub enabled: bool,
    pub global_gain: f64,
    pub response: Vec<EqResponsePoint>,
    pub reference_rate: f64,
    pub bands: Vec<EqBand>,
    pub current_preset_id: String,
    pub presets: Vec<EqPreset>,
}
#[frb(opaque)]
pub struct EqualizerSession {
    pub(crate) session: crate::equalizer::Session,
}
impl EqualizerSession {
    pub fn state(&self) -> Result<EqSnapshot, String> {
        self.session.state()
    }
    pub fn set_enabled(&self, revision: u64, enabled: bool) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::Enabled(enabled))
    }
    pub fn set_global_gain(&self, revision: u64, gain: f64) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::GlobalGain(gain))
    }
    pub fn set_band(&self, revision: u64, index: u32, value: EqBand) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::Band { index, value })
    }
    pub fn select_preset(&self, revision: u64, id: String) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::Preset(id))
    }
    pub fn save_preset(&self, revision: u64, name: String) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::SavePreset(name))
    }
    pub fn update_preset(&self, revision: u64, id: String) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::UpdatePreset(id))
    }
    pub fn delete_preset(&self, revision: u64, id: String) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::DeletePreset(id))
    }
    #[frb(sync)]
    pub fn spectrum_active(&self, active: bool) -> Result<(), String> {
        self.session.spectrum_active(active)
    }
    pub fn spectrum(&self) -> Result<SpectrumSnapshot, String> {
        self.session.spectrum()
    }
    pub fn export_config(&self, revision: u64) -> Result<String, String> {
        self.session.export(revision)
    }
    pub fn import_config(&self, revision: u64, text: String) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::ImportConfig(text))
    }
    pub fn import_text(
        &self,
        revision: u64,
        name: String,
        text: String,
    ) -> Result<EqSnapshot, String> {
        self.session
            .edit(revision, crate::equalizer::Edit::ImportText { name, text })
    }
    pub fn reset(&self, revision: u64) -> Result<EqSnapshot, String> {
        self.session.edit(revision, crate::equalizer::Edit::Reset)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_equalizer() -> Result<EqualizerSession, String> {
    crate::equalizer::begin()
}

#[derive(Clone, Debug)]
pub struct DiscoverySource {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SourceOption {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub qualities: Vec<String>,
    pub preferred_quality: Option<String>,
    pub plugin_name: String,
}
#[derive(Clone, Debug)]
pub struct SourceSelectionSnapshot {
    pub sources: Vec<SourceOption>,
    pub selected: String,
}
#[frb(opaque)]
pub struct SourceSelectionSession {
    session: crate::source_selection::Session,
}
impl SourceSelectionSession {
    pub fn snapshot(&self) -> Result<SourceSelectionSnapshot, String> {
        self.session.snapshot()
    }
    pub fn select(&self, source: String) -> Result<(), String> {
        self.session.select(source)
    }
    pub fn select_quality(&self, quality: String, all_sources: bool) -> Result<(), String> {
        self.session.select_quality(quality, all_sources)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_source_selection(
    source_revision: u64,
    selection_revision: u64,
) -> Result<SourceSelectionSession, String> {
    crate::source_selection::begin(source_revision, selection_revision)
        .map(|session| SourceSelectionSession { session })
}
#[derive(Clone, Debug)]
pub struct DiscoveryCategory {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct DiscoveryGroup {
    pub name: String,
    pub items: Vec<DiscoveryCategory>,
}
#[derive(Clone, Debug)]
pub struct DiscoveryCategories {
    pub hot: Vec<DiscoveryCategory>,
    pub groups: Vec<DiscoveryGroup>,
}
#[derive(Clone, Debug)]
pub struct DiscoveryCard {
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub cover: Option<String>,
    pub song_count: Option<u64>,
    pub play_count: Option<u64>,
    pub chart: bool,
}
#[derive(Clone, Debug)]
pub struct DiscoveryPage {
    pub cards: Vec<DiscoveryCard>,
    pub has_more: bool,
}
#[derive(Clone, Debug)]
pub struct PlaylistSearchItem {
    pub source: String,
    pub card: DiscoveryCard,
}
#[derive(Clone, Debug)]
pub struct PlaylistSearchPage {
    pub items: Vec<PlaylistSearchItem>,
    pub total: u32,
    pub has_more: bool,
    pub sources: Vec<SearchSourceState>,
}
#[frb(opaque)]
pub struct PlaylistSearchSession {
    pub(crate) session: crate::playlist_search::Session,
}
impl PlaylistSearchSession {
    pub fn next_page(&self) -> Result<PlaylistSearchPage, String> {
        self.session.next(false)
    }
    pub fn retry_failed(&self) -> Result<PlaylistSearchPage, String> {
        self.session.next(true)
    }
    pub fn open(&self, source: String, id: String) -> Result<OnlinePlaylistSession, String> {
        self.session.open(source, id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn playlist_search_sources() -> Result<Vec<SearchSource>, String> {
    crate::playlist_search::sources()
}
pub fn begin_playlist_search(
    query: String,
    source: String,
) -> Result<PlaylistSearchSession, String> {
    Ok(PlaylistSearchSession {
        session: crate::playlist_search::begin(query, source)?,
    })
}
#[derive(Clone, Debug)]
pub struct OnlinePlaylistPage {
    pub tracks: Vec<Track>,
    pub labels: Vec<SearchRowLabel>,
    pub description: String,
    pub singer_ids: Vec<String>,
    pub total: u32,
    pub has_more: bool,
}
#[frb(opaque)]
pub struct DiscoverySession {
    pub(crate) session: crate::discovery::Session,
}
impl DiscoverySession {
    pub fn categories(&self) -> Result<DiscoveryCategories, String> {
        self.session.categories()
    }
    pub fn next(&self) -> Result<DiscoveryPage, String> {
        self.session.next()
    }
    pub fn open(&self, id: String) -> Result<OnlinePlaylistSession, String> {
        self.session.open(id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
#[frb(opaque)]
pub struct OnlinePlaylistSession {
    pub(crate) session: crate::discovery::PlaylistSession,
}
impl OnlinePlaylistSession {
    pub fn prepare_favorite(&self, id: String) -> Result<FavoriteIntent, String> {
        self.session.prepare_favorite(id)
    }
    pub fn play_filtered(&self, query: String, shuffle: bool) -> Result<(), String> {
        self.session.play_filtered(query, shuffle)
    }

    pub fn begin_complete_download(&self) -> Result<OnlineDownloadCollection, String> {
        self.session.begin_complete_download()
    }

    pub fn prepare_download(&self, id: String) -> Result<DownloadChoice, String> {
        self.session.prepare_download(id)
    }
    pub fn prepare_selected_save(&self, ids: Vec<String>) -> Result<OnlineSaveSession, String> {
        self.session.prepare_selected_save(ids)
    }
    pub fn prepare_save(&self) -> Result<OnlineSaveSession, String> {
        self.session.prepare_save()
    }
    pub fn open_singer(&self, id: String) -> Result<SingerSession, String> {
        self.session.open_singer(id)
    }
    pub fn play_all(&self) -> Result<(), String> {
        self.session.play_all()
    }
    pub fn next(&self) -> Result<OnlinePlaylistPage, String> {
        self.session.next()
    }
    pub fn play(&self, id: String) -> Result<(), String> {
        self.session.play(id)
    }
    pub fn append(&self, id: String) -> Result<(), String> {
        self.session.append(id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn discovery_sources() -> Result<Vec<DiscoverySource>, String> {
    crate::discovery::sources()
}
pub fn begin_discovery(
    source: String,
    category: String,
    charts: bool,
) -> Result<DiscoverySession, String> {
    Ok(DiscoverySession {
        session: crate::discovery::begin(source, category, charts)?,
    })
}

#[derive(Clone, Debug)]
pub struct SingerProfile {
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub song_count: Option<u64>,
    pub album_count: Option<u64>,
    pub album_details: bool,
}
#[derive(Clone, Debug)]
pub struct SingerAlbumsPage {
    pub cards: Vec<DiscoveryCard>,
    pub total: u32,
    pub has_more: bool,
}
#[frb(opaque)]
pub struct SingerSession {
    pub(crate) session: crate::singer::Session,
}
impl SingerSession {
    pub fn info(&self) -> Result<SingerProfile, String> {
        self.session.info()
    }
    pub fn reopen(&self) -> Result<SingerSession, String> {
        self.session.reopen()
    }
    pub fn profile(&self) -> Result<SingerProfile, String> {
        self.session.profile()
    }
    pub fn songs(&self) -> Result<OnlinePlaylistSession, String> {
        self.session.songs()
    }
    pub fn next_albums(&self) -> Result<SingerAlbumsPage, String> {
        self.session.next_albums()
    }
    pub fn open_album(&self, id: String) -> Result<OnlinePlaylistSession, String> {
        self.session.open_album(id)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}

#[derive(Clone, Debug)]
pub struct CommentBody {
    pub id: String,
    pub name: String,
    pub time: String,
    pub text: String,
    pub avatar: Option<String>,
    pub images: Vec<String>,
    pub liked_count: u64,
}
#[derive(Clone, Debug)]
pub struct Comment {
    pub body: CommentBody,
    pub replies: Vec<CommentBody>,
}
#[derive(Clone, Debug)]
pub struct CommentsPage {
    pub rows: Vec<Comment>,
    pub total: u64,
    pub has_more: bool,
}
#[frb(opaque)]
pub struct CommentsSession {
    pub(crate) session: crate::comments::Session,
}
impl CommentsSession {
    pub fn next(&self, hot: bool) -> Result<CommentsPage, String> {
        self.session.next(hot)
    }
    pub fn reopen(&self) -> Result<CommentsSession, String> {
        self.session.reopen()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}

#[frb(opaque)]
pub struct OnlineSaveSession {
    pub(crate) session: crate::online_save::Session,
}
impl OnlineSaveSession {
    pub fn destinations(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        self.session.destinations(offset, limit)
    }
    pub fn destination_cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        self.session.destination_cover(id)
    }
    pub fn append(&self, destination_id: String) -> Result<u32, String> {
        self.session.append(destination_id)
    }

    #[frb(sync)]
    pub fn count(&self) -> u32 {
        self.session.count()
    }
    pub fn save(
        &self,
        name: String,
        description: String,
        cover: Option<String>,
    ) -> Result<PlaylistInfo, String> {
        self.session.save(name, description, cover)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}

/// A file-import intent bound to the runtime that opened the original chooser.
#[frb(opaque)]
pub struct PlaylistFileImport {
    pub(crate) session: crate::playlist_import::Session,
}
impl PlaylistFileImport {
    /// Read the entire captured provider playlist before any database write.
    pub fn prepare_network(&self) -> Result<u32, String> {
        self.session.prepare_network()
    }
    /// Number of original rows retained by an already prepared queue import.
    pub fn prepared_count(&self) -> Result<u32, String> {
        self.session.prepared_count()
    }
    /// Parse the selected plain JSON array once, retaining its original rows.
    pub fn prepare(&self, bytes: Vec<u8>) -> Result<u32, String> {
        self.session.prepare(bytes)
    }
    /// Commit at most one new playlist; subsequent calls return the actual outcome.
    pub fn save(&self, name: String, description: String) -> Result<PlaylistInfo, String> {
        self.session.save(name, description)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_playlist_file_import() -> Result<PlaylistFileImport, String> {
    crate::playlist_import::begin()
}

#[derive(Clone, Debug)]
pub struct QueueExportFile {
    pub name: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct QueueImportResult {
    pub total: u32,
    pub added: u32,
    pub skipped: u32,
}
/// An original-owner queue transfer. Import merges; it never starts playback.
#[frb(opaque)]
pub struct QueueDocumentSession {
    pub(crate) session: crate::queue_document::Session,
}
impl QueueDocumentSession {
    pub fn export_text(&self) -> Result<String, String> {
        self.session.export_text()
    }
    pub fn export_file(&self) -> Result<QueueExportFile, String> {
        self.session.export_file()
    }
    pub fn prepare_file(&self, bytes: Vec<u8>, extension: String) -> Result<u32, String> {
        self.session.prepare_file(bytes, extension)
    }
    pub fn prepare_text(&self, text: String) -> Result<u32, String> {
        self.session.prepare_text(text)
    }
    pub fn merge(&self) -> Result<QueueImportResult, String> {
        self.session.merge()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_queue_document(
    queue_revision: u64,
    source_revision: u64,
) -> Result<QueueDocumentSession, String> {
    crate::queue_document::begin(queue_revision, source_revision)
}

/// Capture the displayed local queue before opening the playlist save confirmation.
pub fn begin_playlist_queue_import(
    queue_revision: u64,
    source_revision: u64,
) -> Result<PlaylistFileImport, String> {
    crate::playlist_import::begin_queue(queue_revision, source_revision)
}

/// Installed sources that support network playlist import, in desktop order.
pub fn playlist_network_sources() -> Result<Vec<DiscoverySource>, String> {
    crate::playlist_import::network_sources()
}
/// Capture an original runtime and installed provider before fetching any pages.
pub fn begin_playlist_network_import(
    source: String,
    input: String,
) -> Result<PlaylistFileImport, String> {
    crate::playlist_import::begin_network(source, input)
}

#[frb(opaque)]
pub struct PlaylistCoverEditor {
    pub(crate) editor: crate::playlist_cover::Editor,
}
impl PlaylistCoverEditor {
    pub fn prepare(&self, bytes: Vec<u8>, extension: String) -> Result<Vec<u8>, String> {
        self.editor.prepare(bytes, extension)
    }
    pub fn save(&self) -> Result<PlaylistInfo, String> {
        self.editor.save()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.editor.close();
    }
}

pub enum LocalTagCoverMode {
    Keep,
    Remove,
    Replace,
}

#[derive(Clone, Debug)]
pub struct LocalTagDraft {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: u32,
    pub path: String,
}
#[derive(Clone, Debug)]
pub struct LocalTagSaveResult {
    pub persistence_warning: bool,
    pub playback_warning: bool,
}
#[frb(opaque)]
pub struct LocalTagEditor {
    pub(crate) editor: crate::local_tags::Editor,
}
impl LocalTagEditor {
    pub fn draft(&self) -> Result<LocalTagDraft, String> {
        self.editor.draft()
    }
    pub fn prepare_cover(&self, bytes: Vec<u8>, extension: String) -> Result<Vec<u8>, String> {
        self.editor.prepare_cover(bytes, extension)
    }
    pub fn save(
        &self,
        title: String,
        artist: String,
        album: String,
        year: u32,
        cover_mode: LocalTagCoverMode,
    ) -> Result<LocalTagSaveResult, String> {
        self.editor.save(title, artist, album, year, cover_mode)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.editor.close();
    }
}

#[frb(opaque)]
pub struct OnlineDownloadCollection {
    pub(crate) session: crate::collection_download::Session,
}
impl OnlineDownloadCollection {
    pub fn load(&self) -> Result<u32, String> {
        self.session.load()
    }
    pub fn prepare_download(&self, index: u32) -> Result<DownloadChoice, String> {
        self.session.prepare_download(index)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}

/// In-memory credentials for one immutable restore preparation. Never persisted.
pub struct S3RestoreConfig {
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket: String,
    pub path_style: bool,
}
#[derive(Clone)]
pub struct S3RestorePreview {
    pub timestamp: String,
    pub playlists: u32,
    pub songs: u32,
    pub local_references: u32,
    pub plugins: u32,
}
#[derive(Clone)]
pub struct S3RestoreOutcome {
    pub playlists_added: u32,
    pub songs_added: u32,
    pub songs_skipped: u32,
    pub plugins_applied: bool,
    pub plugins_added: u32,
    pub plugins_skipped: u32,
    pub settings_applied: bool,
    pub warnings: Vec<String>,
}
#[frb(opaque)]
pub struct S3RestorePreparation {
    pub(crate) session: crate::backup_restore::Session,
}
impl S3RestorePreparation {
    pub fn load(&self, password: String) -> Result<S3RestorePreview, String> {
        self.session.load(password)
    }
    pub fn commit(&self, overwrite: bool) -> Result<S3RestoreOutcome, String> {
        self.session.commit(overwrite)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_s3_restore_preparation(
    config: S3RestoreConfig,
) -> Result<S3RestorePreparation, String> {
    crate::backup_restore::begin(config)
}

/// Nonsecret connection preferences. Secret key and encryption password are absent.
pub struct S3SavedConnection {
    pub endpoint: String,
    pub region: String,
    pub access_key_id: String,
    pub bucket: String,
    pub path_style: bool,
    pub max_backups: u32,
    pub last_backup_time: Option<String>,
}
#[frb(opaque)]
pub struct S3SettingsSession {
    pub(crate) session: crate::backup_upload::SettingsSession,
}
impl S3SettingsSession {
    pub fn state(&self) -> Result<S3SavedConnection, String> {
        self.session.state()
    }
    pub fn save(&self, next: S3SavedConnection) -> Result<S3SavedConnection, String> {
        self.session.save(next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_s3_settings() -> Result<S3SettingsSession, String> {
    crate::backup_upload::begin_settings()
}
#[derive(Clone)]
pub struct S3UploadOutcome {
    pub timestamp: String,
    pub deleted: u32,
    pub warnings: Vec<String>,
}
#[frb(opaque)]
pub struct S3UploadPreparation {
    pub(crate) session: crate::backup_upload::UploadSession,
}
impl S3UploadPreparation {
    pub fn load(&self) -> Result<S3RestorePreview, String> {
        self.session.load()
    }
    pub fn upload(&self, password: String) -> Result<S3UploadOutcome, String> {
        self.session.upload(password)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_s3_upload_preparation(
    config: S3RestoreConfig,
    max_backups: u32,
) -> Result<S3UploadPreparation, String> {
    crate::backup_upload::begin_upload(config, max_backups)
}

#[derive(Clone)]
pub struct MobileRelease {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub notes_truncated: bool,
    pub newer: bool,
}
#[frb(opaque)]
pub struct ReleaseSession {
    pub(crate) session: crate::releases::Session,
}
impl ReleaseSession {
    pub fn state(&self) -> Result<bool, String> {
        self.session.state()
    }
    pub fn save(&self, expected: bool, enabled: bool) -> Result<bool, String> {
        self.session.save(expected, enabled)
    }
    pub fn check(&self, startup: bool) -> Result<Option<MobileRelease>, String> {
        self.session.check(startup)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
/// Pass the installed package's native version, not a Dart build-time guess.
pub fn begin_release_session(current_version: String) -> Result<ReleaseSession, String> {
    crate::releases::begin(current_version)
}

#[derive(Clone)]
pub struct RecognizedTrack {
    pub index: u32,
    pub track: Track,
    pub timestamp: i64,
    pub start_time_ms: u64,
}
pub struct MicrophoneProgress {
    pub seconds: u32,
    pub level: u32,
    pub capturing: bool,
    pub restore_failed: bool,
}
pub struct RecognitionHistory {
    pub rows: Vec<RecognizedTrack>,
    pub revision: u64,
    pub warning: Option<String>,
    pub can_clear: bool,
}
#[frb(opaque)]
pub struct RecognitionSession {
    pub(crate) session: crate::recognition::Session,
}
impl RecognitionSession {
    #[frb(sync)]
    pub fn microphone_token(&self) -> Result<u64, String> {
        self.session.microphone_token()
    }
    #[frb(sync)]
    pub fn microphone_progress(&self) -> MicrophoneProgress {
        self.session.microphone_progress()
    }
    pub fn recognize_microphone(&self) -> Result<Vec<RecognizedTrack>, String> {
        self.session.recognize_microphone()
    }
    pub fn history(&self) -> Result<RecognitionHistory, String> {
        self.session.history()
    }
    pub fn clear_history(&self, revision: u64) -> Result<RecognitionHistory, String> {
        self.session.clear_history(revision)
    }
    pub fn play_history(&self, index: u32, revision: u64) -> Result<(), String> {
        self.session.play_history(index, revision)
    }
    /// Only a session-owned recognized row can be played; Dart supplies no song metadata.
    pub fn play(&self, index: u32, jump_to_match: bool) -> Result<(), String> {
        self.session.play(index, jump_to_match)
    }
    /// Explicit user action only: sends an AFP fingerprint of one staged file.
    pub fn recognize_file(&self, ticket: String) -> Result<Vec<RecognizedTrack>, String> {
        self.session.recognize_file(ticket)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_recognition() -> Result<RecognitionSession, String> {
    crate::recognition::begin()
}

/// Display rows only; renderer endpoints stay in the original Rust discovery session.
pub struct DlnaDevice {
    pub index: u32,
    pub name: String,
    pub address: String,
}
pub struct DlnaDevices {
    pub revision: u64,
    pub rows: Vec<DlnaDevice>,
}
#[frb(opaque)]
pub struct DlnaSession {
    pub(crate) session: crate::dlna::Session,
}
impl DlnaSession {
    pub fn select(&self, revision: u64, index: u32) -> Result<DlnaCastState, String> {
        self.session.select(revision, index)
    }
    pub fn search(&self) -> Result<DlnaDevices, String> {
        self.session.search()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_dlna() -> Result<DlnaSession, String> {
    crate::dlna::begin()
}

#[derive(Clone)]
pub struct DlnaCastState {
    pub token: u64,
    pub name: String,
    pub address: String,
    pub phase: String,
    pub song: Option<String>,
    pub error: Option<String>,
}
pub fn dlna_cast_state() -> Result<Option<DlnaCastState>, String> {
    crate::dlna_cast::state()
}
pub fn stop_dlna_cast(token: u64) -> Result<DlnaCastState, String> {
    crate::dlna_cast::stop(token)
}

#[derive(Clone, Debug)]
pub struct LanTransferEvent {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub success: bool,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct LanTransferView {
    pub address: String,
    pub url: String,
    pub code: String,
    pub paired: bool,
    pub closed: bool,
    pub phase: String,
    pub received_bytes: u64,
    pub total_reserved: u64,
    pub events: Vec<LanTransferEvent>,
    pub error: Option<String>,
}
#[frb(opaque)]
pub struct LanTransferSession {
    pub(crate) session: crate::lan_transfer::Session,
}
impl LanTransferSession {
    pub fn start(&self, address: String) -> Result<LanTransferView, String> {
        self.session.start(address)
    }
    pub fn state(&self) -> Result<LanTransferView, String> {
        self.session.state()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
    pub fn finish_close(&self) {
        self.session.finish_close();
    }
}
pub fn lan_transfer_addresses() -> Result<Vec<String>, String> {
    mio_core::lan_transfer::addresses()
}
pub fn begin_lan_transfer() -> Result<LanTransferSession, String> {
    Ok(LanTransferSession {
        session: crate::lan_transfer::begin()?,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualSettings {
    pub immersive_lyric_color: bool,
    pub cover_layout: bool,
    pub show_left_panel: bool,
    pub auto_hide_bottom: bool,
    pub pause_transition: bool,
    pub cover_dynamic_theme: bool,
    pub jump_lyrics: bool,
    pub background: bool,
    pub blur_lyrics: bool,
    pub spectrum: bool,
    pub mode: String,
}
#[frb(opaque)]
pub struct VisualSettingsSession {
    pub(crate) session: crate::settings::Session,
}
impl VisualSettingsSession {
    pub fn background(&self) -> Result<BackgroundSettings, String> {
        self.session.background()
    }
    pub fn save_background(
        &self,
        expected: BackgroundSettings,
        next: BackgroundSettings,
    ) -> Result<BackgroundSettings, String> {
        self.session.save_background(expected, next)
    }
    /// None resets all background fields to shared defaults; a preset uses shared preset semantics.
    pub fn preset_background(
        &self,
        expected: BackgroundSettings,
        preset: Option<String>,
    ) -> Result<BackgroundSettings, String> {
        self.session.preset_background(expected, preset)
    }

    pub fn state(&self) -> Result<VisualSettings, String> {
        self.session.visuals()
    }
    pub fn save(
        &self,
        expected: VisualSettings,
        next: VisualSettings,
    ) -> Result<VisualSettings, String> {
        self.session.save_visuals(expected, next)
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_visual_settings() -> Result<VisualSettingsSession, String> {
    Ok(VisualSettingsSession {
        session: crate::settings::new_session()?,
    })
}
#[frb(opaque)]
pub struct PlayerSpectrumSession {
    pub(crate) session: crate::spectrum::Session,
}
impl PlayerSpectrumSession {
    pub fn frame(&self) -> Result<SpectrumSnapshot, String> {
        self.session.frame()
    }
    #[frb(sync)]
    pub fn close(&self) {
        self.session.close();
    }
}
pub fn begin_player_spectrum(generation: u64) -> Result<PlayerSpectrumSession, String> {
    Ok(PlayerSpectrumSession {
        session: crate::spectrum::Session::begin(generation)?,
    })
}

/// Read-only projection of the shared desktop background configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundRendering {
    pub audio_response: bool,
    pub render_scale: f32,
    pub flow_speed: f32,
    pub static_mode: bool,
    pub fps: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundSettings {
    pub preset: String,
    pub enabled: bool,
    pub audio_response: bool,
    pub render_scale: f32,
    pub flow_speed: f32,
    pub static_mode: bool,
    pub fps: u32,
}

// The mobile host owns the platform locale; environment variables are not reliable on phones.
static SYSTEM_ENGLISH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[flutter_rust_bridge::frb(sync)]
pub fn set_system_language(english: bool) {
    SYSTEM_ENGLISH.store(english, std::sync::atomic::Ordering::Release);
    mio_core::host::set_system_language_resolver(|| {
        SYSTEM_ENGLISH.load(std::sync::atomic::Ordering::Acquire)
    });
}
