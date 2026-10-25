use crate::song::{Album, Song};
use crate::{
    i18n,
    navigation::{Navigation, Page},
    playback::{EventKind, Player, Status},
    skin::{
        color, colored_playback_slider, control, glyph, guide_glow, icon, mixed_color,
        playback_slider, workspace_background,
    },
    storage::{PlaybackSession, Preferences, SessionIdentity, Storage},
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    Disableable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    slider::{SliderEvent, SliderState},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use mio_tokens::Mode;
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
/// Updates the main view from a task of its own window.
///
/// `WeakEntity::update_in` finds the window through the window the entity was last *drawn* in.
/// A second window that reads the view (the tray panel, the desktop lyric) takes that mapping
/// away when it closes, and the update then fails until the main window draws again, which
/// an occluded window does not; a polling loop that stops on the error stays stopped.
pub(super) fn update_main<R>(
    this: &WeakEntity<Mio>,
    cx: &mut AsyncWindowContext,
    update: impl FnOnce(&mut Mio, &mut Window, &mut Context<Mio>) -> R,
) -> anyhow::Result<R> {
    cx.update(|window, app| this.update(app, |main, cx| update(main, window, cx)))?
}

mod album_backdrop;
mod album_covers;
#[cfg(target_os = "macos")]
mod background;
mod background_settings;
mod backup_dialog;
mod close_behavior;
mod compact_navigation;
mod context_actions;
mod cover_palette;
mod desktop_lyric;
mod dlna_settings;
mod download_quality;
mod downloads_page;
mod effects_settings;
mod favorites;
mod full_player;
mod hotkeys;
mod local_music;
mod local_playlists;
mod local_tag_editor;
mod music_source_settings;
mod network_import;
mod online_discovery;
mod online_playlist;
mod playback_settings;
mod player_comments;
mod player_idle;
mod player_lyrics;
mod player_motion;
mod player_settings;
mod player_spectrum;
mod player_volume;
mod playlist_add;
mod playlist_cover;
mod playlist_delete;
mod playlist_editor;
mod playlist_import;
mod playlist_search;
mod plugin_manager;
mod plugin_playlists;
mod profile_page;
mod queue_drawer;
mod recent_page;
mod recognize_page;
mod replace_confirm;
mod search_page;
mod settings_page;
mod settings_sections;
mod singer_page;
mod song_sort;
mod song_view;
mod storage_settings;
mod tag_settings;
mod tray_panel;
mod update_settings;
mod welcome_page;

enum Remote<T> {
    Idle,
    Loading,
    Ready(Vec<T>),
    Failed(String),
}

pub struct Mio {
    navigation: Navigation,
    search: Entity<InputState>,
    mode: Mode,
    english: bool,
    background_config: crate::background_config::BackgroundConfig,
    background_controls: background_settings::Controls,
    background_warning: Option<Instant>,
    _subscriptions: Vec<Subscription>,
    online: Option<Arc<crate::online::Online>>,
    installed_plugins: Vec<crate::online::InstalledPlugin>,
    list_plugins: Vec<crate::list_plugins::StoredListPlugin>,
    /// The backup dialog; kept so its connection and passwords last for the window.
    backup_dialog: Option<Entity<backup_dialog::BackupDialog>>,
    online_busy: bool,
    plugin_url: Entity<InputState>,
    online_error: Option<String>,
    online_total: usize,
    online_search_busy: bool,
    online_search_pending: Option<bool>,
    online_search_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pending_online_session: Option<PlaybackSession>,
    online_queue_identity: Option<SessionIdentity>,
    online_progress_pending: bool,
    albums: Remote<Album>,
    online_discovery: online_discovery::OnlineDiscovery,
    welcome_started: Instant,
    recognition: recognize_page::RecognitionState,
    album_covers: album_covers::AlbumCovers,
    songs: Remote<Song>,
    album_songs: Remote<Song>,
    album_song_revision: u64,
    album_revision: u64,
    song_revision: u64,
    store: Option<Storage>,
    favorites: favorites::FavoriteState,
    local_playlists: local_playlists::LocalPlaylists,
    local_music: local_music::LocalMusic,
    pending_local_session: Option<PlaybackSession>,
    pending_session: Option<PlaybackSession>,
    first_source_change: bool,
    session_write_blocked: bool,
    session_error: Option<&'static str>,
    session_dirty: bool,
    session_checkpoint: Instant,
    session_restored: bool,
    selected_album: Option<Album>,
    song_view: song_view::SongView,
    context_actions: Option<context_actions::ContextActions>,
    context_focus: FocusHandle,
    player: Player,
    progress: Entity<SliderState>,
    crossfade_duration: Entity<SliderState>,
    volume: Entity<SliderState>,
    progress_pressed: Rc<Cell<bool>>,
    volume_pressed: Rc<Cell<bool>>,
    seeking: bool,
    volume_open: bool,
    queue_open: bool,
    queue_scroll: UniformListScrollHandle,
    queue_hover: Option<usize>,
    queue_hover_since: Option<Instant>,
    queue_press: Option<queue_drawer::QueuePress>,
    queue_tip: queue_drawer::QueueTip,
    cover: Option<Arc<Image>>,
    cover_background: Option<Arc<Image>>,
    cover_pixels: Option<Arc<image::RgbaImage>>,
    cover_palette: Option<cover_palette::CoverPalette>,
    cover_palette_cache: std::collections::VecDeque<(String, cover_palette::CoverPalette)>,
    vinyl_grooves: Option<Arc<RenderImage>>,
    player_motion: player_motion::PlayerMotion,
    #[cfg(target_os = "macos")]
    background: background::Background,
    full_player_open: bool,
    mobile_lyrics: bool,
    player_focus: FocusHandle,
    player_idle: player_idle::PlayerIdle,
    auto_hide_bottom: bool,
    cover_layout: bool,
    show_left_panel: bool,
    pause_transition: bool,
    audio_visualization: bool,
    immersive_lyric_color: bool,
    blur_lyrics: bool,
    lyric_motion: player_lyrics::Motion,
    player_spectrum: player_spectrum::Spectrum,
    comments: player_comments::Comments,
    player_settings_open: bool,
    player_settings_scroll: ScrollHandle,
    lyrics: Remote<crate::lyrics::Lyrics>,
    lyrics_revision: u64,
    lyrics_busy: bool,
    lyric_scroll: ScrollHandle,
    lyrics_active: Option<usize>,
    /// Last reported playback position with the moment it was seen, so word highlights can run
    /// between the engine's 200 ms position reports: (seconds, playing, generation, seen at).
    lyric_clock: Cell<(f64, bool, u64, Instant)>,
    /// The desktop lyric window while it is open.
    desktop_lyric: Option<AnyWindowHandle>,
    desktop_lyric_opening: bool,
    desktop_lyric_locked: bool,
    desktop_lyric_option: desktop_lyric::LyricOption,
    /// Last known window position in physical pixels, as `desktop_lyric_window.json` keeps it.
    desktop_lyric_position: Option<(i64, i64)>,
    /// The close button hides the window to the tray instead of quitting (`closeToTray`).
    close_to_tray: bool,
    close_behavior_configured: bool,
    /// The "remember my choice" box of the close question, kept while the app runs.
    close_remember: bool,
    /// The tray icon; only the real application creates one.
    tray: Option<crate::tray::Tray>,
    /// The tray's mini player window while it is open, and when it last closed itself.
    tray_panel: Option<AnyWindowHandle>,
    tray_panel_closed: Option<Instant>,
    /// The icon was clicked while the panel is open: the panel closes itself.
    tray_panel_close: bool,
    /// The panel's window is being opened (it opens from the executor).
    tray_panel_opening: bool,
    /// Where the tray icon was when the panel opened, to open it again at another height.
    tray_panel_rect: Option<(i32, i32, u32, u32)>,
    /// Global hotkeys (`hotkeys.json`): the configuration, what could not be registered, and the
    /// registrations themselves; only the real application registers anything.
    hotkey_config: crate::hotkeys::Config,
    hotkey_failed: Vec<crate::hotkeys::Action>,
    hotkey_registry: Option<crate::hotkeys::Registry>,
    /// System media keys and the "Now Playing" entry, with what was last published (track,
    /// when, position) to publish again only on changes.
    media_control: Option<crate::media_keys::MediaControl>,
    media_published: Option<(
        Option<(String, String, String, i64, bool, Option<u64>)>,
        Instant,
        f64,
    )>,
    /// When the last in-app single key was accepted (the original throttles to 100 ms).
    in_app_key_at: Instant,
    /// `system`, `zh-CN` or `en-US`; `english` is what it resolves to.
    language: String,
    app_settings: crate::storage::AppSettings,
    settings_ui: settings_page::SettingsUi,
    /// The sound-effects category: equalizer (`equalizer.json`) and surround/balance
    /// (`audio-effects.json`), applied to the audio chain at startup and on every change.
    eq: crate::equalizer::State,
    audio_effects: effects_settings::AudioEffects,
    effects_ui: effects_settings::EffectsUi,
    /// The saved cache and download folders, and the ones being edited on the settings page.
    directories: crate::directories::Directories,
    dir_edit: crate::directories::Directories,
    storage_info: storage_settings::StorageInfo,
    audio_output: playback_settings::AudioOutput,
    dlna_ui: dlna_settings::DlnaUi,
    music_source_ui: music_source_settings::MusicSourceUi,
    update_ui: update_settings::UpdateUi,
}
impl Mio {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::new_in(Storage::application(), window, cx);
        this.navigation.push(Page::Welcome);
        this.start_tray(window, cx);
        this.start_hotkeys(window, cx);
        // Like the original's player on mount: the saved equalizer and balance go to the audio.
        this.apply_all_effects();
        this.apply_cache_config();
        this.sync_quality_preference();
        this.start_spectrum_loop(window, cx);
        this.start_dlna_loop(window, cx);
        this.check_for_update_at_startup(cx);
        this
    }
    #[cfg(feature = "parity")]
    pub fn capture(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Pages other than the idle discovery screen read an (empty, throw-away) data folder:
        // the golden captures run with MIO_GPUI_DATA_DIR pointing at one.
        let store = std::env::var_os("MIO_GPUI_CAPTURE_PAGE").and_then(|_| Storage::application());
        let mut this = Self::new_in(store, window, cx);
        if std::env::var("MIO_GPUI_CAPTURE_THEME").as_deref() == Ok("dark") {
            this.mode = Mode::Dark;
            Theme::change(ThemeMode::Dark, Some(window), cx);
            this.apply_theme(cx);
        }
        // The reference is captured with Simplified Chinese chosen in its settings.
        this.set_language("zh-CN", window, cx);
        this.settings_ui.language.update(cx, |state, cx| {
            state.set_selected_index(
                Some(gpui_kit::component::IndexPath::default().row(1)),
                window,
                cx,
            );
        });
        // A page other than the idle discovery screen, for the golden captures of
        // tools/golden: `settings` (with MIO_GPUI_CAPTURE_CATEGORY), `downloads`, `playlists`.
        if let Ok(page) = std::env::var("MIO_GPUI_CAPTURE_PAGE") {
            match page.as_str() {
                "settings" => {
                    use settings_page::Category;
                    let category = match std::env::var("MIO_GPUI_CAPTURE_CATEGORY")
                        .unwrap_or_default()
                        .as_str()
                    {
                        "playlist" => Category::Playlist,
                        "effects" => Category::Effects,
                        "hotkeys" => Category::Hotkeys,
                        "plugins" => Category::Plugins,
                        "music" => Category::Music,
                        "storage" => Category::Storage,
                        "about" => Category::About,
                        _ => Category::Appearance,
                    };
                    this.settings_ui.category = category;
                    this.go(Page::Appearance, cx);
                }
                "downloads" => this.go(Page::Downloads, cx),
                "playlists" => {
                    this.go(Page::Playlists, cx);
                    if let (Some(store), Ok(id)) = (
                        this.store.as_ref(),
                        std::env::var("MIO_GPUI_CAPTURE_PLAYLIST_ID"),
                    ) {
                        match crate::library::load_playlist(&store.library_path(), &id) {
                            Ok(detail) => this.open_local_playlist(detail.playlist, window, cx),
                            Err(_) => eprintln!("Cannot capture saved playlist"),
                        }
                    }
                }
                _ => {}
            }
        }
        if let Ok(path) = std::env::var("MIO_GPUI_CAPTURE_TTML") {
            if let Ok(lines) = std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|s| mio_music_sdk::parse_ttml(&s))
            {
                if let Some(lyrics) = crate::lyrics::from_ttml(lines) {
                    if let Some(dir) = std::env::var_os("MIO_GPUI_CAPTURE_DIR") {
                        let lines: Vec<_> = lyrics.lines.iter().map(|line| serde_json::json!({
                            "startTime": line.start, "endTime": line.end,
                            "isBG": line.background, "isDuet": line.duet,
                            "translatedLyric": line.translation, "romanLyric": line.romanization,
                            "words": line.words.iter().map(|w| serde_json::json!({"word": w.text, "startTime": w.start, "endTime": w.end, "romanWord": w.romanization})).collect::<Vec<_>>()
                        })).collect();
                        let dir = std::path::PathBuf::from(dir);
                        let _ = std::fs::create_dir_all(&dir);
                        if let Ok(bytes) = serde_json::to_vec_pretty(&lines) {
                            let _ = std::fs::write(dir.join("lyrics-timeline.json"), bytes);
                        }
                    }
                    this.full_player_open = true;
                    this.mobile_lyrics = true;
                    this.show_left_panel = false;
                    this.background_config.enabled = false;
                    this.immersive_lyric_color = false;
                    this.blur_lyrics = std::env::var("MIO_GPUI_CAPTURE_BLUR").as_deref() == Ok("1");
                    this.app_settings.lyric_font_weight = 700;
                    this.player.position = std::env::var("MIO_GPUI_CAPTURE_TIME")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(34.);
                    this.player.status = Status::Paused;
                    this.lyrics = Remote::Ready(vec![lyrics]);
                    this.lyrics_revision += 1;
                }
            }
        }
        this
    }
    fn new_in(store: Option<Storage>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // List plugins register their sources first: stored songs, downloads and sessions of a
        // plugin's source are only recognised once its provider exists.
        if let Some(store) = &store {
            let english = store
                .load::<Preferences>("preferences.json")
                .ok()
                .flatten()
                .unwrap_or_default()
                .english();
            crate::list_plugins::set_language(english);
            if let Err(error) = crate::list_plugins::restore(store) {
                eprintln!("list plugins: {error}");
            }
        }
        let (pending_session, session_write_blocked) =
            match store.as_ref().map(Storage::load_session) {
                Some(Ok(session)) => (session, false),
                Some(Err(_)) => (None, true),
                None => (None, false),
            };
        let (pending_session, pending_local_session) = match pending_session {
            Some(session) if session.is_local_session() => (None, Some(session)),
            other => (other, None),
        };
        let (pending_session, pending_online_session) = match pending_session {
            Some(session) if session.is_online_session() => (None, Some(session)),
            other => (other, None),
        };
        let preferences = store
            .as_ref()
            .and_then(|s| s.load::<Preferences>("preferences.json").ok().flatten())
            .unwrap_or_default();
        let background_config = preferences.background.validated();
        let (background_controls, background_subscriptions) = background_settings::Controls::new(
            background_config,
            preferences.english(),
            window,
            cx,
        );
        let (settings_ui, settings_subscriptions) =
            settings_page::SettingsUi::new(&preferences, window, cx);
        let directories = crate::directories::Directories::load(store.as_ref());
        let (eq, audio_effects) = Self::load_effects(&store);
        let (effects_ui, effects_subscriptions) = effects_settings::EffectsUi::new(
            &eq,
            &audio_effects,
            preferences.english(),
            window,
            cx,
        );
        let (audio_output, audio_subscriptions) = playback_settings::AudioOutput::new(
            preferences.app.audio_primary.clone(),
            preferences.app.audio_secondary.clone(),
            window,
            cx,
        );
        let (music_source_ui, music_source_subscriptions) =
            music_source_settings::MusicSourceUi::new(window, cx);
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(i18n::text(
                preferences.english(),
                "common.searchMusicArtist",
            ))
        });
        let (song_view, song_view_subscriptions) =
            song_view::SongView::new(preferences.english(), window, cx);
        let subscription = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.search(cx);
                window.blur(cx);
            } else if matches!(event, InputEvent::Blur)
                && this.navigation.current() == Page::Search
                && (this.search.read(cx).value().trim() != this.song_view.search_query
                    || matches!(this.songs, Remote::Idle))
            {
                this.search(cx);
            }
        });
        let plugin_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://…")
                .masked(true)
        });
        let mut player = Player::new(preferences.volume, preferences.play_mode);
        player.seamless = preferences.seamless;
        player.seamless.duration_ms = player.seamless.duration_ms.clamp(500, 8000);
        let progress = cx.new(|_| SliderState::new().min(0.).max(100.).step(0.1));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(player.volume * 100.)
        });
        let seek_subscription = cx.subscribe(&progress, |this, _, event, cx| {
            match event {
                SliderEvent::Change(_) => this.seeking = true,
                SliderEvent::Release(value) => {
                    this.seeking = false;
                    this.suppress_recognition_resume();
                    let seconds = value.start() as f64 / 100. * this.player.duration;
                    this.player.seek(seconds);
                    this.save_session();
                }
            }
            cx.notify();
        });
        let crossfade_duration = cx.new(|_| {
            SliderState::new()
                .max(8000.)
                .min(500.)
                .step(500.)
                .default_value(player.seamless.duration_ms.clamp(500, 8000) as f32)
        });
        let crossfade_subscription = cx.subscribe(&crossfade_duration, |this, _, event, cx| {
            if let SliderEvent::Release(value) = event {
                this.player.seamless.duration_ms = value.start().round().clamp(500., 8000.) as u64;
                this.prepare_seamless_next();
                this.save_preferences();
                cx.notify();
            }
        });
        let release_subscription = cx.on_release(|this, _| {
            this.save_session();
            mio_core::storage::flush_session_writes();
        });
        let quit_subscription = cx.on_app_quit(|this, _| {
            this.save_session();
            mio_core::storage::flush_session_writes();
            async {}
        });
        let volume_subscription = cx.subscribe(&volume, |this, _, event, cx| {
            let value = match event {
                SliderEvent::Change(v) | SliderEvent::Release(v) => v.start() / 100.,
            };
            this.player.volume = value;
            if let Some(engine) = &this.player.engine {
                engine.volume(value);
            }
            if matches!(event, SliderEvent::Release(_)) {
                this.save_preferences();
            }
            cx.notify();
        });
        // Weak entity: closing the window ends polling and drops the audio engine.
        // The engine reports its position every 200 ms; polling at 50 ms keeps the lyric clock
        // within a frame or two of it. Nothing is redrawn unless an event arrived.
        cx.spawn_in(window, async move |this, cx| {
            // Before anything has played there is no engine to drain: tick slowly.
            let mut interval = 250;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(interval))
                    .await;
                match update_main(&this, cx, |this, window, cx| {
                    this.poll_audio(window, cx);
                    this.player.engine.is_some() || this.online_progress_pending
                }) {
                    Ok(active) => interval = if active { 50 } else { 250 },
                    Err(_) => break,
                }
            }
        })
        .detach();
        let (local_music, local_music_subscription) =
            local_music::LocalMusic::new(preferences.english(), window, cx);
        let mut this = Self {
            navigation: Navigation::new(),
            search,
            mode: if preferences.dark {
                Mode::Dark
            } else {
                Mode::Light
            },
            english: preferences.english(),
            background_config,
            background_controls,
            background_warning: None,
            _subscriptions: background_subscriptions
                .into_iter()
                .chain(settings_subscriptions)
                .chain(effects_subscriptions)
                .chain(audio_subscriptions)
                .chain(music_source_subscriptions)
                .chain(song_view_subscriptions)
                .chain([local_music_subscription])
                .chain([
                    subscription,
                    seek_subscription,
                    crossfade_subscription,
                    volume_subscription,
                    release_subscription,
                    quit_subscription,
                ])
                .collect(),
            online: None,
            installed_plugins: Vec::new(),
            list_plugins: Vec::new(),
            backup_dialog: None,
            online_busy: false,
            plugin_url,
            online_error: None,
            online_total: 0,
            online_search_busy: false,
            online_search_pending: None,
            online_search_cancel: None,
            pending_online_session,
            online_queue_identity: None,
            online_progress_pending: false,
            albums: Remote::Idle,
            online_discovery: Default::default(),
            welcome_started: Instant::now(),
            recognition: recognize_page::RecognitionState::load(store.as_ref()),
            album_covers: album_covers::AlbumCovers::default(),
            songs: Remote::Idle,
            album_songs: Remote::Idle,
            album_song_revision: 0,
            album_revision: 0,
            song_revision: 0,
            store,
            favorites: Default::default(),
            local_playlists: Default::default(),
            local_music,
            pending_local_session,
            pending_session,
            first_source_change: true,
            session_write_blocked,
            session_error: session_write_blocked
                .then_some("无法恢复播放会话：文件损坏或读取失败，已保留原文件并停止自动保存"),
            session_dirty: false,
            session_checkpoint: Instant::now(),
            session_restored: false,
            selected_album: None,
            song_view,
            context_actions: None,
            context_focus: cx.focus_handle(),
            player,
            progress,
            crossfade_duration,
            volume,
            progress_pressed: Rc::new(Cell::new(false)),
            volume_pressed: Rc::new(Cell::new(false)),
            seeking: false,
            volume_open: false,
            queue_open: false,
            queue_scroll: UniformListScrollHandle::new(),
            queue_hover: None,
            queue_hover_since: None,
            queue_press: None,
            queue_tip: queue_drawer::QueueTip::default(),
            cover: None,
            cover_background: None,
            cover_pixels: None,
            cover_palette: None,
            cover_palette_cache: Default::default(),
            vinyl_grooves: None,
            player_motion: player_motion::PlayerMotion::default(),
            #[cfg(target_os = "macos")]
            background: background::Background::default(),
            full_player_open: false,
            mobile_lyrics: false,
            player_focus: cx.focus_handle(),
            player_idle: Default::default(),
            auto_hide_bottom: preferences.auto_hide_bottom,
            cover_layout: preferences.cover_layout,
            show_left_panel: preferences.show_left_panel,
            pause_transition: preferences.pause_transition,
            audio_visualization: preferences.audio_visualization,
            immersive_lyric_color: preferences.immersive_lyric_color,
            blur_lyrics: preferences.blur_lyrics,
            lyric_motion: Default::default(),
            player_spectrum: Default::default(),
            comments: Default::default(),
            player_settings_open: false,
            player_settings_scroll: ScrollHandle::new(),
            lyrics: Remote::Idle,
            lyrics_revision: 0,
            lyrics_busy: false,
            lyric_scroll: ScrollHandle::new(),
            lyrics_active: None,
            lyric_clock: Cell::new((0., false, 0, Instant::now())),
            desktop_lyric: None,
            desktop_lyric_opening: false,
            desktop_lyric_locked: false,
            close_to_tray: preferences.close_to_tray,
            close_behavior_configured: preferences.close_behavior_configured,
            close_remember: true,
            tray: None,
            tray_panel: None,
            tray_panel_closed: None,
            tray_panel_close: false,
            tray_panel_opening: false,
            tray_panel_rect: None,
            hotkey_config: crate::hotkeys::Config::default(),
            hotkey_failed: Vec::new(),
            hotkey_registry: None,
            media_control: None,
            media_published: None,
            in_app_key_at: Instant::now(),
            language: preferences.language.clone().unwrap_or_else(|| {
                if preferences.english {
                    "en-US"
                } else {
                    "zh-CN"
                }
                .into()
            }),
            app_settings: preferences.app.clone(),
            settings_ui,
            eq,
            audio_effects,
            effects_ui,
            dir_edit: directories.clone(),
            directories,
            storage_info: storage_settings::StorageInfo::new(),
            audio_output,
            dlna_ui: Default::default(),
            music_source_ui,
            update_ui: Default::default(),
            desktop_lyric_option: Default::default(),
            desktop_lyric_position: None,
        };
        Theme::change(
            if this.mode == Mode::Dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        this.apply_theme(cx);
        this.load_favorites(window, cx);
        // The desktop lyric window opens again when it was open at the last exit; once this
        // view exists, since it reads from it.
        let scale = window.scale_factor();
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.restore_desktop_lyric(scale, cx));
        })
        .detach();
        this.song_view.downloads.preferred = preferences.download_quality.clone();
        this.song_view.downloads.tags = preferences.tag_write_options.clone();
        this.attach_downloads(cx);
        if let Some(store) = this.store.clone() {
            this.online_busy = true;
            let restore = cx
                .background_executor()
                .spawn(async move { crate::online::Online::restore(&store) });
            cx.spawn(async move |this, cx| {
                let result = restore.await;
                let _ = this.update(cx, |this, cx| {
                    this.online_busy = false;
                    match result {
                        Ok(client) => this.online = client,
                        Err(error) => this.online_error = Some(error),
                    }
                    this.refresh_installed_plugins();
                    this.attach_downloads(cx);
                    this.restore_online_session(cx);
                    if this.navigation.current() == Page::Search && this.song_view.search_online {
                        this.load_online_search(false, cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let grooves = cx
            .background_executor()
            .spawn(async { full_player::vinyl_grooves() });
        cx.spawn(async move |this, cx| {
            let grooves = grooves.await;
            let _ = this.update(cx, |this, cx| {
                this.vinyl_grooves = Some(grooves);
                cx.notify();
            });
        })
        .detach();
        this.load_local_music(cx);
        this.sync_desktop_lyric_controls(window, cx);
        // What the last run left (a local or online session), once the view exists.
        cx.spawn_in(window, async move |this, cx| {
            let _ = this.update_in(cx, |this, window, cx| this.startup_restore(window, cx));
        })
        .detach();
        this
    }
    fn theme_palette(&self) -> Option<cover_palette::CoverPalette> {
        self.cover_palette
            .filter(|_| self.app_settings.cover_dynamic_theme)
    }
    fn ui_colors(&self) -> crate::skin::Colors {
        crate::skin::Colors {
            mode: self.mode,
            song: self.player.song().and(self.theme_palette()).map(|palette| {
                let rgb = palette.main.to_rgb();
                crate::skin::SongColors {
                    base: [rgb.r, rgb.g, rgb.b].map(|channel| (channel * 255.).round() as u8),
                    use_black_text: palette.use_black_text,
                }
            }),
        }
    }
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }
    /// The first thing after construction: restores what the last run left (a local or online
    /// session) once the window exists. Runs once.
    fn startup_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_context_actions(window, cx);
        self.song_view.reset_list();
        let first = std::mem::replace(&mut self.first_source_change, false);
        let local = self.player.song().is_some_and(crate::local_music::is_local);
        if first && self.player.song().is_some_and(crate::online::is_online) {
            self.load_favorites(window, cx);
            cx.notify();
            return;
        }
        // A session of a source that no longer exists (an old Subsonic server's) is dropped.
        self.pending_session = None;
        self.album_revision += 1;
        self.song_revision += 1;
        self.album_song_revision += 1;
        if !local {
            self.online_queue_identity = None;
            // The initial player is already idle. Stopping it would advance queue_revision
            // and make a slower online plugin's pending session look user-replaced.
            if !first {
                self.player.stop(); // Invalidates queued audio/cover events on later resets.
            }
            self.session_restored = false;
            self.seeking = false;
            self.progress_pressed.set(false);
            self.volume_pressed.set(false);
            self.volume_open = false;
            self.cover = None;
            self.queue_open = false;
            self.queue_press = None;
            self.queue_hover = None;
            self.queue_hover_since = None;
            self.cover_background = None;
            self.cover_pixels = None;
            self.cover_palette = None;
            self.cover_palette_cache.clear();
            #[cfg(target_os = "macos")]
            {
                self.background = background::Background::default();
            }
            // A lyric capture opens the full player itself, before this startup reset runs.
            #[cfg(feature = "parity")]
            let keep_full_player = std::env::var_os("MIO_GPUI_CAPTURE_TTML").is_some();
            #[cfg(not(feature = "parity"))]
            let keep_full_player = false;
            if !keep_full_player {
                self.full_player_open = false;
                self.player_settings_open = false;
                self.lyrics_revision += 1;
                self.lyrics = Remote::Idle;
            }
        }
        self.favorites.qualities.clear();
        self.album_covers.replace(&[]);
        self.song_view.album_covers.replace(&[]);
        self.song_view.search_covers.replace(&[]);
        self.song_view.album_qualities.clear();
        self.song_view.search_qualities.clear();
        self.song_view.search_query.clear();
        self.song_view.search_has_more = false;
        self.song_view.search_loading_more = false;
        self.song_view.search_offset = 0;
        self.song_view.search_page_error = None;
        self.selected_album = None;
        self.song_view.backdrop.update(cx, |state, _| state.clear());
        self.albums = Remote::Idle;
        self.songs = Remote::Idle;
        self.album_songs = Remote::Idle;
        if !local {
            self.session_dirty = false;
            self.session_checkpoint = Instant::now();
            self.restore_online_session(cx);
        }
        if !local
            && !self.session_restored
            && self.pending_online_session.is_none()
            && self.pending_local_session.is_none()
        {
            self.clear_session();
        }
        if self.navigation.current() == Page::Playlist {
            self.load_local_playlist(cx);
        }
        self.apply_theme(cx);
        if self.session_restored && local {
            self.load_restored_cover(cx);
        }
        self.update_progress(window, cx);
        if self.navigation.current() == Page::Search {
            self.search(cx);
        }
        cx.notify();
    }
    fn clear_session(&mut self) {
        // Never replace an unreadable/corrupt file as a side effect of startup.
        if self.session_write_blocked {
            return;
        }
        if let Some(store) = &self.store
            && store.save_session(None).is_err()
        {
            self.session_write_blocked = true;
            self.session_error = Some("无法清除旧播放会话，自动保存已停止；重启前请处理会话文件");
            eprintln!("Unable to clear playback session");
        }
    }
    fn restore_online_session(&mut self, cx: &mut Context<Self>) {
        if self.online.is_none() {
            return;
        }
        // A delayed startup restore must not replace a queue the user has already changed.
        if self.pending_online_session.is_some()
            && (self.player.queue_revision != 0
                || self.player.song().is_some()
                || !self.player.queue.is_empty())
        {
            self.pending_online_session = None;
            return;
        }
        if let Some(session) = self.pending_online_session.take() {
            let qualities = session.online_queue_qualities();
            if let Some((queue, index, song, position, identity)) = session.restore_online() {
                self.online_queue_identity = identity;
                self.player.queue_qualities = qualities;
                self.player.queue = queue;
                self.player.current = Some(index);
                self.player.duration = song.duration as f64;
                self.player.active_song = Some(song);
                self.player.position = position;
                self.player.status = Status::Paused;
                self.session_restored = true;
                self.online_progress_pending = true;
                // A desktop lyric window that reopened before the song was back shows its
                // lyrics as soon as there is a song.
                self.lyrics_revision += 1;
                self.lyrics = Remote::Loading;
                if self.desktop_lyric.is_some() || self.desktop_lyric_opening {
                    self.load_lyrics(cx);
                }
                self.load_restored_cover(cx);
                cx.notify();
            }
        }
    }
    fn save_session(&mut self) {
        self.session_checkpoint = Instant::now();
        if !self.session_write_blocked
            && self.player.song().is_some_and(crate::local_music::is_local)
        {
            let result = self
                .store
                .as_ref()
                .zip(self.player.current.zip(self.player.song()))
                .map(|(store, (index, song))| {
                    PlaybackSession::capture_local(
                        &self.player.queue,
                        index,
                        song,
                        if song.duration > 0 {
                            self.player.position.min(song.duration as f64)
                        } else {
                            self.player.position
                        },
                        self.online_queue_identity.clone(),
                    )
                    .and_then(|session| {
                        store.save_session_in_background(Some(
                            &session.with_queue_qualities(&self.player.queue_qualities),
                        ))
                    })
                });
            if matches!(result, Some(Ok(()))) {
                self.session_dirty = false;
                self.session_error = None;
            } else {
                self.session_dirty = true;
                self.session_error = Some("无法保存本地播放会话，旧文件已保留");
            }
            return;
        }
        if !self.session_write_blocked && self.player.song().is_some_and(crate::online::is_online) {
            let result = self
                .store
                .as_ref()
                .zip(self.player.current)
                .zip(self.player.song())
                .map(|((store, index), song)| {
                    PlaybackSession::capture_online(
                        &self.player.queue,
                        index,
                        song,
                        if song.duration > 0 {
                            self.player.position.min(song.duration as f64)
                        } else {
                            self.player.position
                        },
                        self.online_queue_identity.clone(),
                    )
                    .and_then(|session| {
                        store.save_session_in_background(Some(
                            &session.with_queue_qualities(&self.player.queue_qualities),
                        ))
                    })
                });
            if matches!(result, Some(Ok(()))) {
                self.session_dirty = false;
                self.session_error = None;
            } else {
                self.session_dirty = true;
                self.session_error = Some("无法保存在线播放会话，旧文件已保留");
            }
        }
    }
    fn update_progress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.seeking {
            let progress = if self.player.duration > 0. {
                self.player.position / self.player.duration * 100.
            } else {
                0.
            };
            self.progress.update(cx, |state, cx| {
                state.set_value(progress.clamp(0., 100.) as f32, window, cx)
            });
        }
    }
    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        // The source and plugin settings are categories of the settings page.
        let page = match page {
            Page::MusicSettings => {
                self.settings_ui.category = settings_page::Category::Music;
                self.settings_ui.compact_detail = true;
                Page::Appearance
            }
            Page::PluginSettings => {
                self.settings_ui.category = settings_page::Category::Plugins;
                self.settings_ui.compact_detail = true;
                Page::Appearance
            }
            other => other,
        };
        if page != self.navigation.current() {
            self.context_actions = None;
            self.online_discovery.more_categories = false;
            self.song_view.reset_list();
        }
        if self.navigation.current() == Page::Recognize && page != Page::Recognize {
            self.cancel_recognition(cx);
        }
        self.navigation.push(page);
        if page == Page::Discover {
            self.ensure_online_discovery(cx);
        }
        if page == Page::Playlists {
            self.load_playlists(cx);
        }
        cx.notify();
    }
    fn navigate_history(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigation.current() == Page::Recognize {
            self.cancel_recognition(cx);
        }
        self.close_context_actions(window, cx);
        self.song_view.reset_list();
        if forward {
            if !self.navigation.can_forward() {
                return;
            }
            self.navigation.forward();
        } else {
            if !self.navigation.can_back() {
                return;
            }
            self.navigation.back();
        }
        if let Some(playlist) = self.navigation.playlist().cloned() {
            self.restore_local_playlist(playlist, window, cx);
        } else if let Some(playlist) = self.navigation.online_playlist().cloned() {
            self.restore_online_playlist(playlist, window, cx);
        } else if let Some(singer) = self.navigation.singer().cloned() {
            self.restore_singer(singer, cx);
        } else if self.navigation.current() == Page::Playlists {
            self.load_playlists(cx);
        } else if self.navigation.current() == Page::Search {
            let query = self.search.read(cx).value().trim().to_string();
            if query.is_empty() {
                self.go(Page::Discover, cx);
            } else if query != self.song_view.search_query || matches!(self.songs, Remote::Idle) {
                self.song_view
                    .search_scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(0.)));
                // The box is what the user wants searched when they come back to this page.
                self.song_view.search_query = query;
                self.load_songs(None, cx);
            }
        }
        cx.notify();
    }
    fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_string();
        if query.is_empty() {
            self.go(Page::Discover, cx);
            return;
        }
        let changed = query != self.song_view.search_query;
        self.go(Page::Search, cx);
        if changed || matches!(self.songs, Remote::Idle | Remote::Failed(_)) {
            self.song_view
                .search_scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.), px(0.)));
            self.song_view.search_query = query;
            self.load_songs(None, cx);
        }
    }
    fn sidebar(&self, height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        div()
            .id("sidebar")
            .test_support()
            .w(rems(15.))
            .h_full()
            .flex_shrink_0()
            .p(rems(1.))
            .bg(workspace_background(
                mode,
                height,
                height - mio_tokens::player_height(height),
                -1.4,
                0.3,
            ))
            .border_r_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .child(
                div()
                    .id("brand")
                    .test_support()
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .mb(rems(1.5))
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if !window.default_prevented() {
                            if event.click_count == 2 {
                                window.titlebar_double_click();
                            } else {
                                window.start_window_move();
                            }
                        }
                    })
                    .child(img("images/icon.png").size(rems(2.)).rounded(rems(0.625)))
                    .child(
                        div()
                            .text_size(rems(1.125))
                            // `.app-title` is 500 with a 800 span inside; the page font stops at
                            // Semibold, which is what 800 resolves to.
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .child("Mio Music"),
                    ),
            )
            .child(
                div().flex().flex_col().gap(rems(0.25)).children(
                    [
                        (Page::Discover, "faxian", "common.homeNav.discover"),
                        (Page::Playlists, "yanchu", "common.homeNav.playlists"),
                        (Page::Local, "shouye", "common.homeNav.local"),
                        (Page::Downloads, "xiazai", "common.homeNav.download"),
                    ]
                    .into_iter()
                    .map(|(page, name, key)| {
                        let active = self.navigation.current() == page;
                        gpui_kit::base::Button::new(SharedString::from(key))
                            .accessibility_label(self.t(key))
                            .justify_start()
                            .flex()
                            .items_center()
                            .gap(rems(0.75))
                            .w_full()
                            .h(rems(2.4))
                            .px(rems(1.))
                            .rounded(px(8.))
                            .text_size(px(14.))
                            .font_weight(FontWeight::NORMAL)
                            .bg(if active {
                                color(mode, "--td-brand-color-4")
                            } else {
                                transparent_black()
                            })
                            .text_color(color(
                                mode,
                                if active {
                                    "--td-text-color-anti"
                                } else {
                                    "--td-text-color-secondary"
                                },
                            ))
                            .hover(move |style| {
                                style.bg(color(
                                    mode,
                                    if active {
                                        "--td-brand-color-5"
                                    } else {
                                        "--td-bg-color-component-hover"
                                    },
                                ))
                            })
                            .child(glyph(name))
                            .child(self.t(key))
                            .on_click(cx.listener(move |this, _, _, cx| this.go(page, cx)))
                    }),
                ),
            )
    }
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        div()
            .id("header")
            .test_support()
            .h(px(88.2))
            .flex_shrink_0()
            .flex()
            .items_center()
            .p(rems(1.5))
            .gap(rems(0.5))
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if !window.default_prevented() {
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                }
            })
            .child(
                control("back", "NavigateBefore", self.t("common.back"), mode, cx)
                    .icon(glyph("xiangzuo").size(rems(1.)))
                    .disabled(!self.navigation.can_back())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate_history(false, window, cx);
                    })),
            )
            .child(
                control(
                    "forward",
                    "NavigateNext",
                    if self.english {
                        "Forward".into()
                    } else {
                        "前进".into()
                    },
                    mode,
                    cx,
                )
                .icon(glyph("xiangyou").size(rems(1.)))
                .disabled(!self.navigation.can_forward())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.navigate_history(true, window, cx);
                })),
            )
            .child(
                div()
                    .id("search")
                    .test_support()
                    .h(px(32.))
                    .w(px(220.))
                    .flex_shrink_0()
                    .child(
                        Input::new(&self.search)
                            .when(
                                self.online
                                    .as_ref()
                                    .is_some_and(|o| !o.sources().is_empty()),
                                |input| input.prefix(self.discovery_source_menu(cx)),
                            )
                            .bg(color(mode, "--td-bg-color-container"))
                            .h(px(30.5))
                            .pl(px(15.5))
                            .rounded(px(20.))
                            .suffix(
                                control(
                                    "search-submit",
                                    "Search",
                                    self.t("common.search"),
                                    mode,
                                    cx,
                                )
                                .icon(icon("Search").size(px(16.)))
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.search(cx);
                                        window.blur(cx);
                                    },
                                )),
                            ),
                    ),
            )
            .child(
                control(
                    "recognize",
                    "Record",
                    self.t("music.recognize.startRecognize"),
                    mode,
                    cx,
                )
                .icon(crate::skin::icon("TD-Microphone").size(px(16.)))
                .on_click(cx.listener(|this, _, _, cx| this.go(Page::Recognize, cx))),
            )
            .child(div().flex_1())
            .child(
                // `TitleBarControls.vue` .window-controls: 0.125rem between its children.
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(rems(0.125))
                    .child(
                        // `BackupRestore.vue`: the cloud icon with the word "备份", and a green dot while
                        // the S3 connection is up.
                        div()
                            .relative()
                            .child(
                                gpui_kit::base::Button::new("backup")
                                    .accessibility_label(self.t("backup.backupAndRestore"))
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .h(px(30.))
                                    .pl(px(8.))
                                    .pr(px(10.))
                                    .rounded(px(999.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .line_height(px(12.))
                                    .hover(move |s| {
                                        s.bg(rgba(0x7d7d7d1f))
                                            .text_color(color(mode, "--td-text-color-primary"))
                                    })
                                    .active(|s| s.bg(rgba(0x7d7d7d2e)))
                                    .child(icon("BackupCloud").size(px(15.)))
                                    .child(self.t("backup.backup"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_backup_dialog(window, cx)
                                    })),
                            )
                            .when(self.backup_connected(cx), |wrap| {
                                wrap.child(
                                    div()
                                        .absolute()
                                        .top(px(5.))
                                        .right(px(7.))
                                        .size(px(5.))
                                        .rounded_full()
                                        .bg(color(mode, "--td-success-color")),
                                )
                            }),
                    )
                    .child(
                        control(
                            "settings",
                            "Settings",
                            self.t("common.titleBar.settings"),
                            mode,
                            cx,
                        )
                        .icon(glyph("shezhi").size(rems(1.)))
                        .w(rems(2.25))
                        .h(rems(2.25))
                        .on_click(cx.listener(|this, _, _, cx| this.go(Page::Appearance, cx))),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h(rems(1.))
                            .mx(px(3.))
                            .rounded(px(2.))
                            .bg(color(mode, "--td-border-level-1-color")),
                    )
                    .child(self.window_buttons(cx)),
            )
    }
    fn discover(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        div()
            .id("discover-setup")
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(rems(2.))
            .py(px(24.))
            .child(
                div()
                    .w_full()
                    .max_w(px(880.))
                    .id("guide")
                    .test_support()
                    .p(px(40.))
                    .flex()
                    .flex_col()
                    .gap(px(24.))
                    .items_center()
                    .rounded(px(20.))
                    .relative()
                    .overflow_hidden()
                    .border_1()
                    .border_dashed()
                    .border_color(color(mode, "--td-border-level-1-color"))
                    .bg(color(mode, "--td-bg-color-container"))
                    .child(img(guide_glow(mode.mode)).absolute().inset_0().size_full())
                    .child(
                        div()
                            .size(px(72.))
                            .id("orb")
                            .test_support()
                            .rounded(px(22.))
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(color(mode, "--td-brand-color-1"), 0.),
                                linear_color_stop(color(mode, "--td-brand-color-3"), 1.),
                            ))
                            .child(img("images/icon.png").size_full().rounded(px(22.))),
                    )
                    .child(
                        div()
                            .w_full()
                            .id("copy")
                            .test_support()
                            .text_center()
                            .child(
                                div()
                                    .id("title")
                                    .test_support()
                                    .text_size(rems(1.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(px(22.))
                                    .mb(px(10.))
                                    .child(self.t("music.find.configGuide")),
                            )
                            .child(
                                div()
                                    .id("description")
                                    .test_support()
                                    .max_w(px(580.))
                                    .mx_auto()
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .line_height(px(23.))
                                    .child(self.t("music.find.guideDescription")),
                            ),
                    )
                    .child(
                        div()
                            .id("cards")
                            .test_support()
                            .flex()
                            .justify_center()
                            .gap(px(16.))
                            .w_full()
                            .children(
                                [(
                                    Page::PluginSettings,
                                    "music.find.installListPlugin",
                                    "music.find.installListPluginDesc",
                                    "music.find.goToPluginManage",
                                )]
                                .into_iter()
                                .map(
                                    |(page, title, description, action)| {
                                        div()
                                            .id("primary")
                                            .test_support()
                                            .flex_1()
                                            .min_w_0()
                                            .max_w(px(440.))
                                            .min_h(px(168.))
                                            .p(px(22.))
                                            .flex()
                                            .flex_col()
                                            .justify_between()
                                            .gap(px(20.))
                                            .rounded(px(16.))
                                            .border_1()
                                            .border_color(mixed_color(
                                                mode,
                                                "--td-brand-color",
                                                0.26,
                                                "--td-border-level-1-color",
                                            ))
                                            .bg(mixed_color(
                                                mode,
                                                "--td-brand-color",
                                                0.08,
                                                "--td-bg-color-container",
                                            ))
                                            .child(
                                                div()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_size(rems(1.))
                                                            .line_height(px(22.))
                                                            .mb(px(8.))
                                                            .child(self.t(title)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(color(
                                                                mode,
                                                                "--td-text-color-secondary",
                                                            ))
                                                            .line_height(relative(1.6))
                                                            .child(self.t(description)),
                                                    ),
                                            )
                                            .child(
                                                Button::new(SharedString::from(action))
                                                    .label(self.t(action))
                                                    .font_weight(FontWeight::NORMAL)
                                                    .rounded(px(3.))
                                                    .primary()
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| this.go(page, cx),
                                                    )),
                                            )
                                    },
                                ),
                            ),
                    ),
            )
    }
    /// The setup guide of `find.vue` at ≤768px: one column, a 60px logo, stacked full-width
    /// cards with centered text and a full-width button.
    fn discover_narrow(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        let gutter = (width * 0.04).clamp(16., 24.);
        let top = (width * 0.035).clamp(14., 22.);
        let pad = (width * 0.05).clamp(16., 24.);
        let gap = (width * 0.035).clamp(12., 16.);
        div()
            .id("discover-setup")
            .size_full()
            .overflow_y_scroll()
            .px(px(gutter))
            // `.find-container` top gutter plus `.setup-state` 24px.
            .pt(px(top + 24.))
            .pb(px(24.))
            .child(
                div()
                    .id("guide")
                    .test_support()
                    .w_full()
                    .p(px(pad))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(gap))
                    .rounded(px(18.))
                    .relative()
                    .overflow_hidden()
                    .border_1()
                    .border_dashed()
                    .border_color(color(mode, "--td-border-level-1-color"))
                    .bg(color(mode, "--td-bg-color-container"))
                    .child(
                        img(crate::skin::guide_glow_sized(
                            mode.mode,
                            (width - 2. * gutter) as u32,
                            // Orb, copy, the card and the gaps and padding around them.
                            (2. * pad + 60. + 80. + 136. + 3. * gap + 2.) as u32,
                        ))
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .child(
                        div()
                            .size(px(60.))
                            .id("orb")
                            .test_support()
                            .flex_shrink_0()
                            .rounded(px(18.))
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(color(mode, "--td-brand-color-1"), 0.),
                                linear_color_stop(color(mode, "--td-brand-color-3"), 1.),
                            ))
                            .child(img("images/icon.png").size_full().rounded(px(18.))),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_w(px(580.))
                            .id("copy")
                            .test_support()
                            .text_center()
                            .child(
                                div()
                                    .id("title")
                                    .test_support()
                                    .text_size(rems(1.2))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(px(22.))
                                    .mb(px(10.))
                                    .child(self.t("music.find.configGuide")),
                            )
                            .child(
                                div()
                                    .id("description")
                                    .test_support()
                                    .max_w(px(580.))
                                    .mx_auto()
                                    // The browser breaks this sentence into two lines and leaves only its last two
                                    // characters on the second; with the same text here the
                                    // break falls one character earlier. The inset puts the
                                    // limit where the browser's line ends (556px of ink).
                                    .px(px(7.5))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .text_size(rems(0.9))
                                    .line_height(px(24.))
                                    .child(self.t("music.find.guideDescription")),
                            ),
                    )
                    .child(
                        div()
                            .id("cards")
                            .test_support()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .w_full()
                            .children(
                                [(
                                    Page::PluginSettings,
                                    "music.find.installListPlugin",
                                    "music.find.installListPluginDesc",
                                    "music.find.goToPluginManage",
                                )]
                                .into_iter()
                                .map(
                                    |(page, title, description, action)| {
                                        div()
                                            .id("primary")
                                            .test_support()
                                            .w_full()
                                            .p(px(14.))
                                            .flex()
                                            .flex_col()
                                            .gap(px(12.))
                                            .text_center()
                                            .rounded(px(16.))
                                            .border_1()
                                            .border_color(mixed_color(
                                                mode,
                                                "--td-brand-color",
                                                0.26,
                                                "--td-border-level-1-color",
                                            ))
                                            .bg(mixed_color(
                                                mode,
                                                "--td-brand-color",
                                                0.08,
                                                "--td-bg-color-container",
                                            ))
                                            .child(
                                                div()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_size(rems(0.95))
                                                            .line_height(px(21.))
                                                            .mb(px(8.))
                                                            .child(self.t(title)),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(color(
                                                                mode,
                                                                "--td-text-color-secondary",
                                                            ))
                                                            .text_size(rems(0.82))
                                                            .line_height(relative(1.6))
                                                            .child(self.t(description)),
                                                    ),
                                            )
                                            .child(
                                                Button::new(SharedString::from(action))
                                                    .label(self.t(action))
                                                    .font_weight(FontWeight::NORMAL)
                                                    .rounded(px(3.))
                                                    .w_full()
                                                    .min_h(px(44.))
                                                    .primary()
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| this.go(page, cx),
                                                    )),
                                            )
                                    },
                                ),
                            ),
                    ),
            )
    }
    fn pending_page(&self) -> impl IntoElement {
        let (title, description) = match self.navigation.current() {
            Page::Playlists => ("common.homeNav.playlists", "music.list.noSongs"),
            Page::Local => ("common.homeNav.local", "common.noData"),
            Page::Downloads => ("common.homeNav.download", "common.noData"),
            Page::MusicSettings => ("music.find.configSubsonic", "music.find.subsonicDesc"),
            Page::PluginSettings => ("music.find.installPlugin", "music.find.installPluginDesc"),
            Page::Search => ("common.search", "common.noData"),
            _ => ("common.titleBar.settings", "common.noData"),
        };
        div()
            .id("content-page")
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(
                div()
                    .text_size(rems(1.875))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t(title)),
            )
            .child(
                div()
                    .text_color(color(self.ui_colors(), "--td-text-color-secondary"))
                    .child(self.t(description)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(self.ui_colors(), "--td-text-color-placeholder"))
                    .child(if self.english {
                        "This page is under construction."
                    } else {
                        "此页面正在复刻，服务尚未接入。"
                    }),
            )
    }
    /// The preferences as they are right now, the content of `preferences.json`.
    fn preferences_snapshot(&self) -> Preferences {
        Preferences {
            background: self.background_config,
            dark: self.mode == Mode::Dark,
            english: self.english,
            volume: self.player.volume,
            play_mode: self.player.mode,
            auto_hide_bottom: self.auto_hide_bottom,
            cover_layout: self.cover_layout,
            show_left_panel: self.show_left_panel,
            pause_transition: self.pause_transition,
            audio_visualization: self.audio_visualization,
            immersive_lyric_color: self.immersive_lyric_color,
            blur_lyrics: self.blur_lyrics,
            seamless: self.player.seamless,
            download_quality: self.song_view.downloads.preferred.clone(),
            tag_write_options: self.song_view.downloads.tags.clone(),
            close_to_tray: self.close_to_tray,
            close_behavior_configured: self.close_behavior_configured,
            language: Some(self.language.clone()),
            app: self.app_settings.clone(),
        }
    }
    fn save_preferences(&self) {
        // Playback reads the quality choice from here; every change to it ends in a save.
        self.sync_quality_preference();
        if let Some(store) = &self.store
            && store
                .save("preferences.json", &self.preferences_snapshot())
                .is_err()
        {
            // Surface a save failure without logging the path or source credentials.
            eprintln!("Unable to save appearance preferences");
        }
    }
    fn load_songs(&mut self, album: Option<String>, cx: &mut Context<Self>) {
        if !self.song_view.search_online {
            if let Some(cancel) = &self.online_search_cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.online_search_pending = None;
        }
        if album.is_none() && self.song_view.search_online {
            self.load_online_search(false, cx);
        }
    }

    pub fn apply_theme(&self, cx: &mut App) {
        Theme::update(cx, |theme| {
            theme.font_family = if cfg!(target_os = "macos") {
                "PingFang SC"
            } else if cfg!(target_os = "windows") {
                "Microsoft YaHei"
            } else {
                "Ubuntu"
            }
            .into();
            theme.primary = color(self.ui_colors(), "--td-brand-color");
            theme.primary_foreground = color(self.ui_colors(), "--td-text-color-anti");
            theme.button_primary = theme.primary;
            theme.button_primary_hover = color(self.ui_colors(), "--td-brand-color-hover");
            theme.button_primary_active = color(self.ui_colors(), "--td-brand-color-active");
            theme.button_primary_foreground = theme.primary_foreground;
            // Outline danger text; solid danger fills keep the kit's fixed red and white label.
            theme.danger = color(self.ui_colors(), "--td-error-color");
            theme.background = color(self.ui_colors(), "--td-bg-color-page");
            theme.foreground = color(self.ui_colors(), "--td-text-color-primary");
            theme.button = color(self.ui_colors(), "--td-bg-color-component");
            theme.button_hover = color(self.ui_colors(), "--td-bg-color-component-hover");
            theme.button_active = color(self.ui_colors(), "--td-bg-color-component-active");
            theme.button_foreground = theme.foreground;
            theme.border = color(self.ui_colors(), "--td-border-level-1-color");
            theme.input = color(self.ui_colors(), "--td-bg-color-container");
            theme.muted = color(self.ui_colors(), "--td-bg-color-component");
            theme.muted_foreground = color(self.ui_colors(), "--td-text-color-placeholder");
            theme.secondary = color(self.ui_colors(), "--td-bg-color-secondarycontainer");
            theme.secondary_foreground = color(self.ui_colors(), "--td-text-color-secondary");
            theme.popover = color(self.ui_colors(), "--td-bg-color-container");
            theme.popover_foreground = theme.foreground;
        });
    }
    fn replace_player_queue(&mut self, queue: Vec<Song>) {
        self.online_queue_identity = None::<SessionIdentity>;
        self.player.replace_queue(queue);
    }
    fn clear_player_queue(&mut self) {
        self.player.clear_queue();
        self.online_queue_identity = None;
    }
    fn enqueue_player_first(&mut self, song: Song) -> usize {
        if !crate::online::is_online(&song)
            && !crate::local_music::is_local(&song)
            && self
                .player
                .queue
                .iter()
                .all(|s| crate::online::is_online(s) || crate::local_music::is_local(s))
        {
            self.online_queue_identity = None::<SessionIdentity>;
        }
        self.player.enqueue_first(song)
    }
    fn play_index(&mut self, index: usize, cx: &mut Context<Self>) {
        self.suppress_recognition_resume();
        let Some(song) = self.player.queue.get(index) else {
            return;
        };
        if crate::local_music::is_local(song) {
            let path = self
                .local_music
                .library
                .as_ref()
                .and_then(|l| l.path(&song.id))
                .unwrap_or_default();
            self.player.play_local(index, path);
        } else if crate::online::is_online(song) {
            let Some(online) = self.online.clone() else {
                self.player.error = Some("请先导入可用在线插件".into());
                self.player.status = Status::Failed;
                cx.notify();
                return;
            };
            self.player.play_online(online, index);
        } else {
            // Neither a local nor an online song: nothing can play it.
            self.player.error = Some("无法播放这首歌曲".into());
            cx.notify();
            return;
        }
        self.finish_track_change(cx);
    }
    fn finish_track_change(&mut self, cx: &mut Context<Self>) {
        // A delayed local index load must not replace a track the user just started.
        self.pending_local_session = None;
        self.session_restored = false;
        self.cover = None;
        self.cover_background = None;
        self.cover_pixels = None;
        self.cover_palette = None;
        self.apply_theme(cx);
        #[cfg(target_os = "macos")]
        self.background.invalidate_track();
        self.lyrics_revision += 1;
        self.lyrics = Remote::Loading;
        if self.full_player_open || self.desktop_lyric.is_some() {
            self.load_lyrics(cx);
        }
        self.seeking = false;
        self.load_restored_cover(cx);
        self.save_session();
        cx.notify();
    }
    fn load_restored_cover(&self, cx: &mut Context<Self>) {
        if self.player.song().is_some_and(crate::local_music::is_local) {
            self.load_local_player_cover(cx);
            return;
        }
        if let Some(song) = self
            .player
            .song()
            .filter(|song| crate::online::is_online(song))
        {
            let cache_key = crate::image_cache::song_key(&song.id);
            let url = song.cover_art.clone();
            let online = self.online.clone();
            let id = song.id.clone();
            let generation = self.player.generation;
            let restored_id = id.clone();
            let request = cx.background_executor().spawn(async move {
                if let Some(bytes) = crate::image_cache::read(&cache_key) {
                    return Some(bytes);
                }
                let url = url.or_else(|| {
                    let online = online?;
                    let item =
                        serde_json::from_value::<mio_music_sdk::MusicItem>(online.item_json(&id)?)
                            .ok()?;
                    mio_music_sdk::get_cover(&item)
                        .ok()
                        .filter(|url| !url.is_empty())
                })?;
                crate::playback::online_cover(url)
            });
            cx.spawn(async move |this, cx| {
                if let Some(bytes) = request.await {
                    let _ = this.update(cx, |this, cx| {
                        if this.player.generation == generation
                            && this
                                .player
                                .song()
                                .is_some_and(|song| song.id == restored_id)
                            && this.cover.is_none()
                        {
                            this.decode_cover(bytes, generation, cx);
                        }
                    });
                }
            })
            .detach();
        }
    }

    fn decode_cover(&mut self, bytes: Vec<u8>, generation: u64, cx: &mut Context<Self>) {
        let local_key = self.local_player_cover_key();
        let key = self.player.song().and_then(|song| song.cover_art.clone());
        let cached = key.as_ref().and_then(|key| {
            self.cover_palette_cache
                .iter()
                .find(|(id, _)| id == key)
                .map(|(_, palette)| *palette)
        });
        let song_cover_key = self
            .player
            .song()
            .filter(|song| crate::online::is_online(song))
            .map(|song| crate::image_cache::song_key(&song.id));
        let task = cx.background_executor().spawn(async move {
            if let Some(key) = song_cover_key {
                crate::image_cache::write(&key, &bytes);
            }
            let decoded = cover_palette::decode_image(bytes)?;
            let accent = cached.unwrap_or_else(|| cover_palette::cover_palette(&decoded));
            let decoded = decoded.thumbnail(512, 512);
            let background = decoded.thumbnail(96, 96).blur(18.);
            let mut background_png = std::io::Cursor::new(Vec::new());
            background
                .write_to(&mut background_png, image::ImageFormat::Png)
                .ok()?;
            let mut png = std::io::Cursor::new(Vec::new());
            decoded.write_to(&mut png, image::ImageFormat::Png).ok()?;
            Some((
                Arc::new(Image::from_bytes(ImageFormat::Png, png.into_inner())),
                Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    background_png.into_inner(),
                )),
                Arc::new(cover_palette::vinyl_pixels(&decoded)),
                accent,
                Arc::new(decoded.to_rgba8()),
            ))
        });
        cx.spawn(async move |this, cx| {
            let cover = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.player.generation == generation
                    && this.local_player_cover_key() == local_key
                {
                    if let Some((cover, background, pixels, accent, background_pixels)) = cover {
                        this.cover = Some(cover);
                        this.cover_background = Some(background);
                        this.cover_pixels = Some(pixels);
                        // Original analysis cache holds 50 covers in insertion order.
                        // Concurrent decodes of the same cover retain the first result.
                        let accent = if let Some(key) = key {
                            if let Some((_, palette)) =
                                this.cover_palette_cache.iter().find(|(id, _)| id == &key)
                            {
                                *palette
                            } else {
                                if this.cover_palette_cache.len() >= 50 {
                                    this.cover_palette_cache.pop_front();
                                }
                                this.cover_palette_cache.push_back((key, accent));
                                accent
                            }
                        } else {
                            accent
                        };
                        this.cover_palette = Some(accent);
                        this.apply_theme(cx);
                        #[cfg(target_os = "macos")]
                        this.background.set_cover(background_pixels);
                        #[cfg(not(target_os = "macos"))]
                        let _ = background_pixels;
                        this.player_motion.invalidate_cover();
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn step_track(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.player.queue.is_empty() {
            return;
        }
        if self.player.current.is_some() {
            if let Some(index) = self.player.next_index(forward) {
                self.play_index(index, cx);
            } else if forward {
                // Original LIST mode pauses when Next is pressed on its last track.
                if self.player.status == Status::Loading {
                    self.player.generation += 1;
                    if let Some(engine) = &self.player.engine {
                        engine.stop(self.player.generation);
                    }
                    self.player.status = Status::Ended;
                    self.save_session();
                    cx.notify();
                } else if self.player.status == Status::Playing
                    && let Some(engine) = &self.player.engine
                {
                    engine.pause(self.player.generation, true);
                }
            }
        }
    }
    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        self.suppress_recognition_resume();
        if self.player.song().is_some_and(crate::local_music::is_local)
            && (self.session_restored
                || matches!(self.player.status, Status::Ended | Status::Failed))
        {
            let song = self.player.song().unwrap();
            let path = self
                .local_music
                .library
                .as_ref()
                .and_then(|l| l.path(&song.id))
                .unwrap_or_default();
            let position = if self.session_restored {
                self.player.position
            } else {
                0.
            };
            self.player.resume_local(path, position);
            self.finish_track_change(cx);
            return;
        }
        if self.player.song().is_some_and(crate::online::is_online)
            && (self.session_restored
                || matches!(self.player.status, Status::Ended | Status::Failed))
        {
            let position = if self.session_restored {
                self.player.position
            } else {
                0.
            };
            if let Some(online) = self.online.clone() {
                self.player.resume_online(online, position);
                self.finish_track_change(cx);
            }
            return;
        }
        match self.player.status {
            Status::Playing | Status::Paused => {
                if let Some(engine) = &self.player.engine {
                    engine.pause(
                        self.player.generation,
                        self.player.status == Status::Playing,
                    );
                }
            }
            Status::Ended | Status::Failed => {}
            _ => {}
        }
    }
    fn prepare_seamless_next(&mut self) {
        let config = self.player.seamless;
        let index = if matches!(
            self.player.status,
            Status::Playing | Status::Paused | Status::Loading
        ) {
            self.player.next_index(true)
        } else {
            None
        };
        let next = index.and_then(|index| {
            let song = self.player.queue.get(index)?.clone();
            let input = if crate::local_music::is_local(&song) {
                crate::playback::seamless::Input::Local(
                    self.local_music.library.as_ref()?.path(&song.id)?,
                )
            } else if crate::online::is_online(&song) {
                crate::playback::seamless::Input::Online(self.online.clone()?)
            } else {
                return None;
            };
            Some((index, song, input))
        });
        if let Some(engine) = self.player.engine.as_mut() {
            engine.configure(config);
            engine.prepare_next(self.player.generation, self.player.queue_revision, next);
        }
    }
    fn poll_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Session files are written off this thread; a failed write retries at the next checkpoint.
        if mio_core::storage::take_session_write_failure() {
            self.session_dirty = true;
            self.session_error = Some(
                if self.player.song().is_some_and(crate::online::is_online) {
                    "无法保存在线播放会话，旧文件已保留"
                } else {
                    "无法保存本地播放会话，旧文件已保留"
                },
            );
            cx.notify();
        }
        if std::mem::take(&mut self.online_progress_pending) {
            self.update_progress(window, cx);
            cx.notify();
        }
        self.prepare_seamless_next();
        let before = (
            self.player.generation,
            self.player.status,
            self.player.current,
        );
        let position = self.player.position;
        let events = self
            .player
            .engine
            .as_ref()
            .map(|e| e.drain_timed())
            .unwrap_or_default();
        if events.is_empty() {
            if self.session_dirty && self.session_checkpoint.elapsed() >= Duration::from_secs(5) {
                self.save_session();
                cx.notify();
            }
            return;
        }
        for (event, sampled_at) in events {
            let sampled_position = match &event.kind {
                EventKind::Position(pos, _) | EventKind::Seeked(pos, _) => Some(*pos),
                EventKind::Swapped(_, _, _, pos) => Some(*pos),
                _ => None,
            };
            let accepted_clock = self.player.accepts_clock(&event);
            let swapped = matches!(event.kind, EventKind::Swapped(..))
                && event.generation == self.player.generation;
            if self.player.apply(event) {
                self.step_track(true, cx);
            } else if swapped {
                self.finish_track_change(cx);
                self.prepare_seamless_next();
            }
            if accepted_clock && sampled_position == Some(self.player.position) {
                #[cfg(feature = "parity")]
                {
                    self.lyric_motion.delivery_lag = sampled_at.elapsed().as_secs_f64();
                }
                self.lyric_clock.set((
                    self.player.position,
                    self.player.status == Status::Playing && !self.player.buffering,
                    self.player.generation,
                    sampled_at,
                ));
            }
        }
        self.session_dirty |= self.player.position != position;
        if before
            != (
                self.player.generation,
                self.player.status,
                self.player.current,
            )
            || (self.session_dirty && self.session_checkpoint.elapsed() >= Duration::from_secs(5))
        {
            self.save_session();
        }
        self.update_progress(window, cx);
        cx.notify();
    }
    fn player_bar_tint(&self, desktop: bool) -> Hsla {
        if self.full_player_open {
            black().opacity(32. / 255.)
        } else if let Some(palette) = self.theme_palette() {
            let base = palette.main.to_rgb();
            let channel = |value: f32| {
                if palette.use_black_text {
                    (value * 255. + (255. - value * 255.) * 0.78).round() / 255.
                } else {
                    (value * 255. * if desktop { 0.65 } else { 0.35 }).round() / 255.
                }
            };
            Hsla::from(Rgba {
                r: channel(base.r),
                g: channel(base.g),
                b: channel(base.b),
                a: 0.88,
            })
        } else {
            color(
                self.mode,
                if self.player.song().is_some() {
                    "--player-bg-default"
                } else {
                    "--player-bg-idle"
                },
            )
        }
    }
    fn player(
        &self,
        height: f32,
        viewport_width: f32,
        viewport_height: f32,
        footer_extension: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let desktop = viewport_width > 768.;
        let mode = if self.full_player_open {
            Mode::Dark
        } else {
            self.mode
        };
        let palette = self.theme_palette().map(|mut palette| {
            if self.full_player_open
                && (!self.background_config.enabled || self.cover_background.is_none())
            {
                palette.text = white().opacity(0.6);
                palette.hover = white();
            }
            palette
        });
        let bar_tint = self.player_bar_tint(desktop);
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut footer_background = self
            .cover_background
            .clone()
            .filter(|_| self.full_player_open && self.background_config.enabled)
            .map(|image| {
                img(image)
                    .absolute()
                    .w_full()
                    .h(px(viewport_height))
                    .top(px(height - viewport_height))
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            });
        #[cfg(target_os = "macos")]
        if self.full_player_open
            && self.background_config.enabled
            && let Some(frame) = self.background.frame()
        {
            footer_background = Some(
                surface(frame)
                    .absolute()
                    .w_full()
                    .h(px(viewport_height))
                    .top(px(height - viewport_height))
                    .object_fit(ObjectFit::Fill)
                    .into_any_element(),
            );
        }
        let control =
            |id: &'static str, name: &'static str, label: String, mode: Mode, _cx: &App| {
                let text = palette
                    .map(|p| p.text)
                    .unwrap_or_else(|| color(mode, "--player-text-idle"));
                let hover = palette
                    .map(|p| p.hover)
                    .unwrap_or_else(|| color(mode, "--player-text-hover-idle"));
                let play = matches!(id, "play" | "mobile-play");
                crate::skin::colored_control(
                    id,
                    name,
                    label,
                    if play { hover } else { text },
                    hover,
                    if play {
                        if palette.is_some() {
                            white().opacity(0.2)
                        } else {
                            color(mode, "--player-btn-bg-idle")
                        }
                    } else {
                        transparent_black()
                    },
                    if play {
                        if palette.is_some() {
                            white().opacity(0.33)
                        } else {
                            color(mode, "--player-btn-bg-hover-idle")
                        }
                    } else {
                        transparent_black()
                    },
                )
            };
        let active = self.player.song().is_some();
        let usable = active && self.player.status != Status::Loading;
        let time = |seconds: f64| {
            let seconds = seconds.max(0.) as u64;
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        let title = self
            .player
            .song()
            .map(|s| s.title.clone())
            .unwrap_or_else(|| self.t("play.welcomeSong"));
        let subtitle = self
            .session_error
            .map(str::to_owned)
            .or_else(|| self.player.error.clone())
            .unwrap_or_else(|| {
                if self.player.status == Status::Loading || self.player.buffering {
                    if self.english {
                        "Buffering…".into()
                    } else {
                        "缓冲中…".into()
                    }
                } else {
                    self.player
                        .song()
                        .map(|s| s.artist.clone())
                        .unwrap_or_else(|| self.t("play.welcomeSinger"))
                }
            });
        div()
            .id("player-bar")
            .test_support()
            .relative()
            .w_full()
            .h(px(height + footer_extension))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .shadow(vec![
                BoxShadow::new(px(0.), px(-1.), black().opacity(57. / 255.)).blur_radius(px(8.)),
            ])
            .bg(if self.full_player_open {
                Hsla::from(rgb(0x19232b))
            } else {
                bar_tint
            })
            .when(cfg!(target_os = "macos"), |bar| bar.bg(transparent_black()))
            .when(self.full_player_open, |bar| {
                bar.child(
                    div()
                        .absolute()
                        .size_full()
                        .overflow_hidden()
                        .when(cfg!(target_os = "macos"), |base| base.bg(rgb(0x19232b)))
                        .children(footer_background)
                        .when(!cfg!(target_os = "macos"), |base| {
                            base.child(div().absolute().size_full().bg(black().opacity(32. / 255.)))
                        }),
                )
            })
            .when(cfg!(target_os = "macos"), |bar| {
                #[cfg(target_os = "macos")]
                {
                    bar.child(
                        canvas(
                            |_, _, _| (),
                            |bounds, _, window, _| {
                                window.paint_backdrop(bounds, Corners::all(px(0.)), 8., 1.8);
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(div().absolute().size_full().bg(bar_tint))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = bar_tint;
                    bar
                }
            })
            .child(if active {
                div()
                    .absolute()
                    .top_0()
                    .h(px(4.))
                    .w_full()
                    .child(colored_playback_slider(
                        &self.progress,
                        &self.progress_pressed,
                        !usable,
                        mode,
                        cx,
                        palette.map(|p| (p.main, p.hover)),
                    ))
                    .into_any_element()
            } else {
                div()
                    .absolute()
                    .top_0()
                    .h(px(4.))
                    .w_full()
                    .bg(color(mode, "--player-bg-idle"))
                    .into_any_element()
            })
            .child(
                div()
                    .h(px(height - 4.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(if desktop {
                        40.
                    } else {
                        (viewport_width * 0.04).clamp(16., 24.) - 6.
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            // `.left-section { padding-top: 2px }`
                            .pt(px(2.))
                            .when(active, |row| {
                                row.child(
                                    img(self.cover.clone().map(ImageSource::from).unwrap_or_else(
                                        || ImageSource::from("images/default-cover.png"),
                                    ))
                                    .size(px(if desktop { 50. } else { 42. }))
                                    .rounded(px(if desktop { 4. } else { 6. }))
                                    .mr(px(if desktop { 12. } else { 10. }))
                                    .flex_shrink_0()
                                    .id("open-full-player-cover")
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        |this, _, window, cx| this.toggle_full_player(window, cx),
                                    )),
                                )
                            })
                            .child(
                                gpui_kit::base::Button::new("open-full-player")
                                    .accessibility_label(if self.full_player_open {
                                        "收起播放器 / Collapse player"
                                    } else {
                                        "展开播放器 / Expand player"
                                    })
                                    .flex()
                                    .flex_col()
                                    .items_start()
                                    .justify_start()
                                    .min_w_0()
                                    .mr(px(12.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_full_player(window, cx)
                                    }))
                                    .when(desktop && active, |text| text.max_w(px(190.)))
                                    .when(viewport_width <= 576., |text| text.max_w(px(140.)))
                                    .when(!desktop, |text| text.flex_1().mr(px(0.)))
                                    .child(
                                        div()
                                            .text_size(px(if desktop { 14. } else { 13. }))
                                            // `line-height: normal` of PingFang SC, rounded
                                            // the way the browser does.
                                            .line_height(px(if desktop { 21. } else { 18. }))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(palette.map(|p| p.hover).unwrap_or_else(
                                                || color(mode, "--td-brand-color-active"),
                                            ))
                                            .mb(px(if desktop { 4. } else { 2. }))
                                            .text_ellipsis()
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(if desktop { 12. } else { 11. }))
                                            .line_height(px(if desktop { 17. } else { 16. }))
                                            .text_color(palette.map(|p| p.text).unwrap_or_else(
                                                || color(mode, "--player-text-idle"),
                                            ))
                                            .text_ellipsis()
                                            .child(subtitle),
                                    ),
                            )
                            .when(desktop, |row| {
                                row.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.))
                                        .child(self.favorite_button(
                                            "like",
                                            self.player.song(),
                                            false,
                                            cx,
                                        ))
                                        .child(
                                            control(
                                                "download-track",
                                                "Download",
                                                self.t("play.download"),
                                                mode,
                                                cx,
                                            )
                                            .disabled(
                                                self.online.is_none()
                                                    || !self
                                                        .player
                                                        .song()
                                                        .is_some_and(crate::online::is_online),
                                            )
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    if let Some(song) = this.player.song().cloned()
                                                    {
                                                        this.download_songs(vec![song], window, cx);
                                                    }
                                                }),
                                            ),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(16.))
                            .child(
                                control(
                                    "previous",
                                    "SkipPrev",
                                    self.t("common.previous"),
                                    mode,
                                    cx,
                                )
                                .disabled(!active)
                                .on_click(cx.listener(|this, _, _, cx| this.step_track(false, cx))),
                            )
                            .child(
                                control(
                                    "play",
                                    if self.player.status == Status::Playing {
                                        "Pause"
                                    } else {
                                        "PlayFilled"
                                    },
                                    if self.player.status == Status::Playing {
                                        self.t("common.pause")
                                    } else {
                                        self.t("common.play")
                                    },
                                    mode,
                                    cx,
                                )
                                .size(px(42.))
                                .bg(if palette.is_some() {
                                    white().opacity(0.2)
                                } else {
                                    color(mode, "--player-btn-bg-idle")
                                })
                                .disabled(!usable)
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx))),
                            )
                            .child(
                                control("next", "SkipNext", self.t("common.next"), mode, cx)
                                    .disabled(!active)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.step_track(true, cx)),
                                    ),
                            )
                            .when(!desktop, |row| row.hidden()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(16.))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(12.))
                                    .text_color(
                                        palette
                                            .map(|p| p.text)
                                            .unwrap_or_else(|| color(mode, "--player-text-idle")),
                                    )
                                    .child(format!(
                                        "{} / {}",
                                        time(self.player.position),
                                        time(self.player.duration)
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(
                                        control(
                                            "play-mode",
                                            self.player.mode.icon(),
                                            self.t(self.player.mode.label()),
                                            mode,
                                            cx,
                                        )
                                        .disabled(!active)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.player.mode = this.player.mode.next();
                                                this.player.refresh_prefetch();
                                                this.save_preferences();
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                    .child(self.volume_control(mode, palette, cx))
                                    .when(
                                        self.full_player_open && self.comment_item().is_some(),
                                        |row| {
                                            row.child(
                                                control(
                                                    "player-comments",
                                                    "Chat",
                                                    self.t("play.comment"),
                                                    mode,
                                                    cx,
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.toggle_comments(window, cx)
                                                })),
                                            )
                                        },
                                    )
                                    .child(
                                        control(
                                            "desktop-lyrics",
                                            "DesktopLyric",
                                            self.t(if self.desktop_lyric_open() {
                                                "play.closeDesktopLyric"
                                            } else {
                                                "play.openDesktopLyric"
                                            }),
                                            mode,
                                            cx,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.toggle_desktop_lyric(window.scale_factor(), cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .flex_shrink_0()
                                            .child(
                                                control(
                                                    "queue",
                                                    "PlayList",
                                                    self.t("play.playlist"),
                                                    mode,
                                                    cx,
                                                )
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.toggle_queue(cx);
                                                })),
                                            )
                                            .when(!self.player.queue.is_empty(), |view| {
                                                view.child(crate::skin::QueueCount(
                                                    self.player.queue.len(),
                                                ))
                                            }),
                                    ),
                            )
                            .when(!desktop, |row| row.hidden()),
                    )
                    .when(!desktop, |row| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .flex_shrink_0()
                                .ml(px(4.))
                                .child(self.favorite_button(
                                    "mobile-like",
                                    self.player.song(),
                                    false,
                                    cx,
                                ))
                                .child(
                                    control(
                                        "mobile-previous",
                                        "SkipPrev",
                                        self.t("common.previous"),
                                        mode,
                                        cx,
                                    )
                                    .disabled(false)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.step_track(false, cx)),
                                    ),
                                )
                                .child(
                                    control(
                                        "mobile-play",
                                        if self.player.status == Status::Playing {
                                            "Pause"
                                        } else {
                                            "PlayFilled"
                                        },
                                        if self.player.status == Status::Playing {
                                            self.t("common.pause")
                                        } else {
                                            self.t("common.play")
                                        },
                                        mode,
                                        cx,
                                    )
                                    .disabled(self.player.status == Status::Loading)
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_play(cx))),
                                )
                                .child(
                                    control(
                                        "mobile-next",
                                        "SkipNext",
                                        self.t("common.next"),
                                        mode,
                                        cx,
                                    )
                                    .disabled(false)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.step_track(true, cx)),
                                    ),
                                )
                                .when(
                                    self.full_player_open && self.comment_item().is_some(),
                                    |row| {
                                        row.child(
                                            control(
                                                "mobile-comments",
                                                "Chat",
                                                self.t("play.comment"),
                                                mode,
                                                cx,
                                            )
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.toggle_comments(window, cx)
                                                }),
                                            ),
                                        )
                                    },
                                )
                                .child(
                                    control(
                                        "mobile-queue",
                                        "PlayList",
                                        self.t("play.playlist"),
                                        mode,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.toggle_queue(cx);
                                        },
                                    )),
                                ),
                        )
                    }),
            )
    }
}
impl Render for Mio {
    #[allow(refining_impl_trait)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let cover_view_key = (
            self.navigation.current(),
            window.viewport_size(),
            self.local_playlists.scroll.offset(),
            self.local_playlists.list_revision,
        );
        if self.local_playlists.cover_view_key != Some(cover_view_key) {
            self.local_playlists.cover_view_key = Some(cover_view_key);
            for id in self.local_playlists.degraded_covers.drain() {
                if let Some((_, image)) = self.local_playlists.covers.remove(&id) {
                    self.local_playlists.retired_covers.extend(image);
                }
                self.local_playlists.cover_order.retain(|old| old != &id);
            }
            if self.local_playlists.covers.len() > 64
                || playlist_cover::cache_bytes(&self.local_playlists.covers) > 128 * 1024 * 1024
            {
                let parent = cx.weak_entity();
                // Run after this view's prepaint has registered its complete visible set.
                window.on_next_frame(move |_, cx| {
                    let _ = parent.update(cx, |this, cx| {
                        if this.trim_playlist_cover_cache() {
                            cx.notify();
                        }
                    });
                });
                window.request_animation_frame();
            }
        }
        self.local_playlists.visible_cover_ids.clear();
        let backdrop_retired = self
            .song_view
            .backdrop
            .update(cx, |state, _| std::mem::take(&mut state.retired));
        for image in self
            .album_covers
            .retired
            .drain(..)
            .chain(self.song_view.album_covers.retired.drain(..))
            .chain(self.song_view.search_covers.retired.drain(..))
            .chain(self.local_playlists.retired_covers.drain(..))
            .chain(self.local_music.retired_covers.drain(..))
            .chain(backdrop_retired)
        {
            let _ = window.drop_image(image);
        }
        self.pump_album_covers(cx);
        let size = window.viewport_size();
        let desktop = f32::from(size.width) > 768.;
        let compact_home = !desktop && !self.full_player_open;
        let footer_extension = if compact_home { 74. } else { 0. };
        self.update_queue_gesture(true, window, cx);
        self.update_queue_tip(desktop, window, cx);
        if self.player_idle.desktop != desktop {
            self.player_idle.desktop = desktop;
            if !desktop || !self.player_idle.manual {
                self.player_idle.hidden = false;
                self.wake_player(cx);
            }
        }
        if !desktop
            || self.queue_open
            || (self.full_player_open && self.player_idle.hidden)
            || !window.is_window_active()
        {
            self.volume_open = false;
            self.volume_pressed.set(false);
        }
        self.sync_comments(cx);
        self.update_player_motion(window, cx);
        if self.full_player_open && self.player_idle.hidden && !self.queue_open {
            self.player_focus.focus(window, cx);
        }
        #[cfg(target_os = "macos")]
        self.update_background(window, cx);
        window.set_rem_size(px(mio_tokens::rem_size(f32::from(size.width))));
        // Wide settings have their own layout; compact settings share the player and navigation.
        if self.navigation.current() == Page::Welcome {
            return self.welcome_page(window, cx).into_any_element();
        }
        if desktop && self.navigation.current() == Page::Appearance && !self.full_player_open {
            return self.settings_root(window, cx);
        }
        div()
            .id("mio-root")
            .track_focus(&self.player_focus)
            .on_mouse_move(cx.listener(|this, _, _, cx| this.wake_player(cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.online_discovery.more_categories || this.comments.open {
                    return;
                }
                if f32::from(window.viewport_size().width) <= 768.
                    && this.navigation.current() == Page::Appearance
                    && !this.full_player_open
                    && !this.queue_open
                    && this.compact_settings_key(event, window, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                if this.in_app_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.full_player_open
                    && this.player_idle.hidden
                    && !this.queue_open
                    && event.keystroke.key == "tab"
                {
                    cx.stop_propagation();
                    return;
                }
                if this.comments.open {
                    if event.keystroke.key == "escape" {
                        if this.comments.image.is_some() {
                            this.comments.image = None;
                        } else {
                            this.comments.open = false;
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.full_player_open && event.keystroke.key == "f1" {
                    this.player_idle.manual = !this.player_idle.manual;
                    this.player_idle.hidden = this.player_idle.manual;
                    if !this.player_idle.manual {
                        this.wake_player(cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .rounded(px(20.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
            .line_height(relative(1.6))
            .text_color(color(self.ui_colors(), "--td-text-color-primary"))
            .bg(color(self.mode, "--td-bg-color-page"))
            .child(
                div()
                    .id("workspace-focus-group")
                    .tab_group()
                    .tab_index(0)
                    .tab_stop(false)
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .bg(color(self.ui_colors(), "--td-bg-color-page"))
                    .child(if self.player_motion.visibility < 1. {
                        div()
                            .size_full()
                            .min_h_0()
                            .flex()
                            .when(desktop, |body| {
                                body.child(self.sidebar(f32::from(size.height), cx))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .when(!desktop, |body| body.bg(self.compact_page_background()))
                                    .when(desktop, |body| {
                                        body.bg(workspace_background(
                                            self.ui_colors(),
                                            f32::from(size.height),
                                            f32::from(size.height)
                                                - mio_tokens::player_height(f32::from(size.height)),
                                            -1.1,
                                            0.15,
                                        ))
                                    })
                                    .when(desktop, |body| body.child(self.header(cx)))
                                    .when(
                                        !desktop
                                            && !self.navigation.current().is_song_detail()
                                            && !matches!(
                                                self.navigation.current(),
                                                Page::OnlinePlaylist
                                                    | Page::Singer
                                                    | Page::Appearance
                                            ),
                                        |body| {
                                            body.child(
                                                self.compact_header(f32::from(size.width), cx),
                                            )
                                        },
                                    )
                                    .child(
                                        div()
                                            .id("page-focus-group")
                                            .tab_group()
                                            .tab_index(1)
                                            .tab_stop(false)
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_hidden()
                                            .child(match self.navigation.current() {
                                                Page::Discover
                                                    if self.discovery_online_source().is_some() =>
                                                {
                                                    self.online_discovery_page(window, cx)
                                                        .into_any_element()
                                                }
                                                Page::Recent => self
                                                    .recent_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::Recognize => self
                                                    .recognize_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::Profile => {
                                                    self.profile_page(cx).into_any_element()
                                                }
                                                Page::Discover if !desktop => self
                                                    .discover_narrow(f32::from(size.width), cx)
                                                    .into_any_element(),
                                                Page::Discover => {
                                                    self.discover(cx).into_any_element()
                                                }
                                                Page::MusicSettings => div().into_any_element(),
                                                Page::Search => self
                                                    .search_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        f32::from(window.rem_size()),
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::Local => self
                                                    .local_music_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::LocalTagEditor => self
                                                    .local_tag_editor_page(cx)
                                                    .into_any_element(),
                                                Page::Downloads => self
                                                    .downloads_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        window,
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::OnlinePlaylist => self
                                                    .online_playlist_page(!desktop, window, cx)
                                                    .into_any_element(),
                                                Page::Singer => self
                                                    .singer_page(
                                                        !desktop,
                                                        f32::from(size.width),
                                                        f32::from(window.rem_size()),
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                Page::Playlists => self
                                                    .playlists_page(window, cx)
                                                    .into_any_element(),
                                                Page::Playlist => self
                                                    .song_list(!desktop, window, cx)
                                                    .into_any_element(),
                                                Page::Appearance => self.settings_root(window, cx),
                                                _ => self.pending_page().into_any_element(),
                                            }),
                                    ),
                            )
                    } else {
                        div().size_full()
                    })
                    .when(self.player_motion.visibility > 0., |body| {
                        body.child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px((1. - self.player_motion.visibility)
                                    * (f32::from(size.height)
                                        - mio_tokens::player_height(f32::from(size.height)))))
                                .w_full()
                                .h(px(f32::from(size.height)
                                    - if self.player_idle.hidden {
                                        0.
                                    } else {
                                        mio_tokens::player_height(f32::from(size.height))
                                    }))
                                .child(self.full_player(
                                    f32::from(size.width),
                                    f32::from(size.height),
                                    window.is_fullscreen(),
                                    cx,
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .id("player-focus-group")
                    .tab_group()
                    .tab_index(1)
                    .tab_stop(false)
                    .relative()
                    .w_full()
                    .h(px(
                        mio_tokens::player_height(f32::from(size.height)) + footer_extension
                    ))
                    .flex_shrink_0()
                    .child(
                        div()
                            .absolute()
                            .w_full()
                            .when(self.player_motion.idle_visibility >= 1., |bar| {
                                bar.invisible()
                            })
                            .top(px(mio_tokens::player_height(f32::from(size.height))
                                * self.player_motion.idle_visibility))
                            .child(self.player(
                                mio_tokens::player_height(f32::from(size.height)),
                                f32::from(size.width),
                                f32::from(size.height),
                                footer_extension,
                                cx,
                            )),
                    ),
            )
            .when(compact_home, |root| {
                root.child(self.compact_bottom_navigation(f32::from(size.width), cx))
            })
            .when(
                !desktop
                    && !self.full_player_open
                    && (self.navigation.current().is_song_detail()
                        || self.navigation.current() == Page::OnlinePlaylist),
                |root| root.child(self.compact_detail_back(f32::from(size.width), cx)),
            )
            .when(self.comments.open, |root| {
                root.child(self.comments_overlay(f32::from(size.width), f32::from(size.height), cx))
            })
            .when(
                self.queue_open || self.player_motion.queue_visibility > 0.,
                |root| root.child(self.queue_drawer(size, cx)),
            )
            .when(
                !desktop
                    && self.navigation.current() == Page::Discover
                    && self.online_discovery.more_categories,
                |root| root.child(self.discovery_category_sheet(size, cx)),
            )
            .when(self.context_actions.is_some(), |root| {
                root.child(self.render_context_actions(cx))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Mio, Page};
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, Entity, InputEvent, IntoElement, MouseButton, MouseDownEvent,
        MouseMoveEvent, MouseUpEvent, Render, TestAppContext, Window, WindowBounds, WindowOptions,
        point, px, size,
    };

    #[gpui_kit::test]
    fn spectrum_loop_survives_a_closed_auxiliary_window(cx: &mut TestAppContext) {
        struct Auxiliary(Entity<Mio>);
        impl Render for Auxiliary {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let _ = self.0.read(cx);
                gpui_kit::div()
            }
        }

        cx.update(gpui_kit::init);
        let mut view = None;
        let main = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let content = cx.new(|cx| {
                    let mut this = Mio::new_in(None, window, cx);
                    this.start_spectrum_loop(window, cx);
                    this
                });
                view = Some(content.clone());
                cx.new(|cx| Root::new(content, window, cx))
            })
            .unwrap()
        });
        let view = view.unwrap();
        let mut main_cx = cx
            .update_window(main.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                window.to_async(cx)
            })
            .unwrap();
        cx.run_until_parked();
        view.update(cx, |this, _| {
            this.full_player_open = true;
            this.audio_visualization = true;
            this.player.status = crate::playback::Status::Playing;
        });
        let auxiliary = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |_, cx| {
                cx.new(|_| Auxiliary(view.clone()))
            })
            .unwrap()
        });
        cx.update_window(auxiliary.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.remove_window();
        })
        .unwrap();
        assert!(
            view.downgrade()
                .update_in(&mut main_cx, |_, _, _| {})
                .is_err()
        );
        for _ in 0..2 {
            view.update(cx, |this, _| this.player_spectrum.target.fill(10.));
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(33));
            cx.run_until_parked();
            cx.update(|cx| assert_eq!(view.read(cx).player_spectrum.target, [-80.; 128]));
        }
        cx.update_window(main.into(), |_, window, _| window.remove_window())
            .unwrap();
        drop(view);
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn damaged_session_is_reported_and_never_overwritten_by_checkpoint_or_close(
        cx: &mut TestAppContext,
    ) {
        use crate::storage::Storage;
        cx.update(gpui_kit::init);
        let temp = tempfile::tempdir().unwrap();
        let store = Storage::at(temp.path().into());
        let path = temp.path().join("session.json");
        std::fs::write(&path, "broken session").unwrap();
        let mut view = None;
        let handle = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let content = cx.new(|cx| Mio::new_in(Some(store.clone()), window, cx));
                view = Some(content.clone());
                cx.new(|cx| Root::new(content, window, cx))
            })
            .unwrap()
        });
        let view = view.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            view.update(cx, |this, cx| {
                assert!(this.session_write_blocked);
                assert!(this.session_error.is_some());
                assert!(this.player.engine.is_none());
                this.session_dirty = true;
                this.session_checkpoint =
                    std::time::Instant::now() - std::time::Duration::from_secs(6);
                this.poll_audio(window, cx);
                this.clear_session();
                this.save_session();
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken session");
            });
            window.remove_window();
        })
        .unwrap();
        drop(view);
        cx.update(|_| {});
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken session");
    }

    #[gpui_kit::test]
    fn playback_thumb_drags_across_track_and_releases_seek(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let handle = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1200.), px(800.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let content = cx.new(|cx| {
                        let mut this = Mio::new_in(None, window, cx);
                        this.player.queue.push(crate::song::Song {
                            id: "fixture".into(),
                            title: "Fixture".into(),
                            artist: "Artist".into(),
                            album: "Album".into(),
                            duration: 100,
                            cover_art: None,
                        });
                        this.player.current = Some(0);
                        this.player.status = crate::playback::Status::Paused;
                        this.player.duration = 100.;
                        this.progress
                            .update(cx, |s, cx| s.set_value(50., window, cx));
                        this
                    });
                    view = Some(content.clone());
                    cx.new(|cx| Root::new(content, window, cx))
                },
            )
            .unwrap()
        });
        let view = view.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            for (from, to) in [(0.5, 0.75), (0.75, 0.25)] {
                let bounds = view.read(cx).progress.read(cx).bounds();
                let position = |fraction| {
                    point(
                        bounds.origin.x + bounds.size.width * fraction,
                        bounds.center().y,
                    )
                };
                window.drag(position(from), position(to), cx);
                assert!((view.read(cx).progress.read(cx).value().start() - to * 100.).abs() < 0.2);
                assert!(!view.read(cx).seeking, "drag release must commit seek");
                window.draw(cx).clear(cx);
            }
            // A native backend may coalesce every intermediate drag move.
            // Mouse-up must still commit its final position, even off the track.
            let bounds = view.read(cx).progress.read(cx).bounds();
            let from = point(bounds.left() + bounds.size.width * 0.25, bounds.center().y);
            let to = point(
                bounds.left() + bounds.size.width * 0.8,
                bounds.center().y - px(40.),
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: from,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.draw(cx).clear(cx);
            window.dispatch_event(
                MouseDownEvent {
                    position: from,
                    button: MouseButton::Left,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.draw(cx).clear(cx);
            window.dispatch_event(
                MouseUpEvent {
                    position: to,
                    button: MouseButton::Left,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            assert!((view.read(cx).progress.read(cx).value().start() - 80.).abs() < 0.2);
            assert!(!view.read(cx).seeking);
            assert!(!view.read(cx).progress_pressed.get());
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn shell_navigation_and_appearance_use_real_controls(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut view = None;
        let handle = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1200.), px(800.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let content = cx.new(|cx| Mio::new_in(None, window, cx));
                    view = Some(content.clone());
                    cx.new(|cx| Root::new(content, window, cx))
                },
            )
            .unwrap()
        });
        let view = view.unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.click("common.homeNav.local", cx);
            assert_eq!(view.read(cx).navigation.current(), Page::Local);
            window.click("back", cx);
            assert_eq!(view.read(cx).navigation.current(), Page::Discover);
            window.click("forward", cx);
            assert_eq!(view.read(cx).navigation.current(), Page::Local);
            window.click("settings", cx);
            assert_eq!(view.read(cx).navigation.current(), Page::Appearance);
            // The settings page has no theme or locale toggle: the language is a select.
            view.update(cx, |this, cx| this.set_language("en-US", window, cx));
            assert!(view.read(cx).english);
        })
        .unwrap();
    }
}
