use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};
#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
}

const SESSION_FILE: &str = "session.json";
const SESSION_BYTES: usize = 2 * 1024 * 1024;
const SESSION_QUEUE: usize = 2048;
const SESSION_SECONDS: f64 = 7. * 24. * 60. * 60.;

// Only the kind of the session (local library or online plugin) and display metadata belong
// here; nothing that authenticates anything.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionIdentity {
    base_url: String,
    username: String,
}
impl SessionIdentity {
    pub fn local() -> Self {
        Self {
            base_url: "https://mio-local.invalid/".into(),
            username: "local".into(),
        }
    }
    fn online() -> Self {
        Self {
            base_url: "https://mio-online.invalid/".into(),
            username: "online".into(),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PlaybackSession {
    version: u8,
    identity: SessionIdentity,
    queue: Vec<crate::song::Song>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    queue_qualities: std::collections::HashMap<String, crate::song::SongQuality>,
    current: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_song: Option<crate::song::Song>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library_identity: Option<SessionIdentity>,
    position: f64,
}
impl PlaybackSession {
    pub fn with_queue_qualities(
        mut self,
        qualities: &std::collections::HashMap<String, crate::song::SongQuality>,
    ) -> Self {
        self.queue_qualities = self
            .queue
            .iter()
            .filter_map(|song| {
                qualities
                    .get(&song.id)
                    .map(|quality| (song.id.clone(), *quality))
            })
            .collect();
        self
    }
    fn capture(
        identity: SessionIdentity,
        version: u8,
        queue: &[crate::song::Song],
        current: usize,
        song: &crate::song::Song,
        position: f64,
        library_identity: Option<SessionIdentity>,
    ) -> io::Result<Self> {
        // A longer queue keeps playing in memory; only a window around the cursor persists,
        // so the restored track and its relative index stay the same.
        let start = if queue.len() > SESSION_QUEUE {
            current
                .min(queue.len() - 1)
                .saturating_sub(SESSION_QUEUE / 2)
                .min(queue.len() - SESSION_QUEUE)
        } else {
            0
        };
        let session = Self {
            version,
            identity,
            queue: queue[start..(start + SESSION_QUEUE).min(queue.len())].to_vec(),
            queue_qualities: Default::default(),
            current: current - start,
            active_song: Some(song.clone()),
            library_identity,
            position,
        };
        session.validate()?;
        Ok(session)
    }
    pub fn capture_online(
        queue: &[crate::song::Song],
        current: usize,
        song: &crate::song::Song,
        position: f64,
        library_identity: Option<SessionIdentity>,
    ) -> io::Result<Self> {
        if !crate::online::is_online(song) {
            return Err(io::Error::other("Invalid online session"));
        }
        Self::capture(
            SessionIdentity::online(),
            3,
            queue,
            current,
            song,
            position,
            library_identity,
        )
    }
    pub fn capture_local(
        queue: &[crate::song::Song],
        current: usize,
        song: &crate::song::Song,
        position: f64,
        library_identity: Option<SessionIdentity>,
    ) -> io::Result<Self> {
        if !crate::local_music::is_local(song) {
            return Err(io::Error::other("Invalid local session"));
        }
        Self::capture(
            SessionIdentity::local(),
            4,
            queue,
            current,
            song,
            position,
            library_identity,
        )
    }
    pub fn is_local_session(&self) -> bool {
        self.version == 4
    }
    pub fn restore_local(
        self,
    ) -> Option<(
        Vec<crate::song::Song>,
        usize,
        crate::song::Song,
        f64,
        Option<SessionIdentity>,
    )> {
        self.validate().ok()?;
        if !self.is_local_session() {
            return None;
        }
        Some((
            self.queue,
            self.current,
            self.active_song?,
            self.position,
            self.library_identity,
        ))
    }
    pub fn is_online_session(&self) -> bool {
        self.version == 3
    }
    pub fn online_queue_qualities(
        &self,
    ) -> std::collections::HashMap<String, crate::song::SongQuality> {
        if (self.is_online_session() || self.is_local_session()) && self.validate().is_ok() {
            self.queue_qualities.clone()
        } else {
            Default::default()
        }
    }
    pub fn restore_online(
        self,
    ) -> Option<(
        Vec<crate::song::Song>,
        usize,
        crate::song::Song,
        f64,
        Option<SessionIdentity>,
    )> {
        self.validate().ok()?;
        if self.version != 3
            || self.identity != SessionIdentity::online()
            || !self
                .active_song
                .as_ref()
                .is_some_and(crate::online::is_online)
        {
            return None;
        }
        Some((
            self.queue,
            self.current,
            self.active_song?,
            self.position,
            self.library_identity,
        ))
    }
    fn validate(&self) -> io::Result<()> {
        let text = |s: &str| s.len() <= 4096 && !s.contains('\0');
        // Only the online (3) and local (4) sessions exist.
        if !matches!(self.version, 3 | 4) {
            return Err(io::Error::other("Invalid playback session"));
        }
        let local = self.is_local_session();
        let expected_identity = if local {
            SessionIdentity::local()
        } else {
            SessionIdentity::online()
        };
        let library_valid = self
            .library_identity
            .as_ref()
            .is_none_or(|identity| text(&identity.base_url) && text(&identity.username));
        if self.identity != expected_identity
            || !self.active_song.as_ref().is_some_and(|song| {
                if local {
                    crate::local_music::is_local(song)
                } else {
                    crate::online::is_online(song)
                }
            })
            || !library_valid
            || (self
                .queue
                .iter()
                .any(|s| !crate::online::is_online(s) && !crate::local_music::is_local(s))
                && self.library_identity.is_none())
        {
            return Err(io::Error::other("Invalid online playback session"));
        }
        // A cursor may outlive a larger queue that was subsequently cleared.
        // It is constant-size metadata; navigation applies modulo before arithmetic.
        let song = self.active_song.as_ref();
        let ids: std::collections::HashSet<&str> =
            self.queue.iter().map(|song| song.id.as_str()).collect();
        if song.is_none()
            || self.queue.len() > SESSION_QUEUE
            || self.queue_qualities.len() > self.queue.len()
            || self
                .queue_qualities
                .keys()
                .any(|id| !ids.contains(id.as_str()))
            || !self.position.is_finite()
            || !(0. ..=SESSION_SECONDS).contains(&self.position)
            || self
                .queue
                .iter()
                .chain(self.active_song.iter())
                .any(|song| {
                    song.id.is_empty()
                        || !text(&song.id)
                        || !text(&song.title)
                        || !text(&song.artist)
                        || !text(&song.album)
                        || song.duration > SESSION_SECONDS as u64
                        || song.cover_art.as_deref().is_some_and(|id| !text(id))
                })
            || song.is_some_and(|song| song.duration > 0 && self.position > song.duration as f64)
        {
            return Err(io::Error::other("Invalid playback session"));
        }
        Ok(())
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    #[serde(deserialize_with = "crate::background_config::deserialize")]
    pub background: crate::background_config::BackgroundConfig,
    pub dark: bool,
    pub english: bool,
    pub volume: f32,
    pub play_mode: crate::playback::PlayMode,
    pub auto_hide_bottom: bool,
    pub cover_layout: bool,
    pub show_left_panel: bool,
    pub pause_transition: bool,
    pub audio_visualization: bool,
    pub immersive_lyric_color: bool,
    pub blur_lyrics: bool,
    pub seamless: crate::playback::seamless::Config,
    /// Last download quality chosen per source (`sourceQualityMap` of the original).
    pub download_quality: std::collections::BTreeMap<String, String>,
    /// What to write into the tags of downloaded files (the original's `tagWriteOptions`).
    pub tag_write_options: crate::tags::TagOptions,
    /// `closeToTray`: the close button hides the window instead of quitting.
    pub close_to_tray: bool,
    /// `hasConfiguredCloseBehavior`: the first-close question has been answered and remembered.
    pub close_behavior_configured: bool,
    /// `language` of the settings page: `system`, `zh-CN` or `en-US`. Files written before the
    /// setting existed have none, and keep the explicit `english` flag.
    #[serde(default = "legacy_language")]
    pub language: Option<String>,
    /// The settings page's own values, in the original's `appSettings` / user-info terms.
    pub app: AppSettings,
}
fn legacy_language() -> Option<String> {
    None
}
/// Settings of the settings page that are not tied to one feature's own file.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppSettings {
    #[serde(deserialize_with = "crate::visuals::deserialize_mode")]
    pub mobile_visual_mode: crate::visuals::Mode,
    /// `userInfo.topBarStyle`: traffic-light window buttons instead of the Windows ones.
    pub traffic_light: bool,
    pub cover_dynamic_theme: bool,
    /// `lyricFontFamily`: a comma separated list; `lyricfont` is the bundled font.
    pub lyric_font_family: String,
    /// `FullPlayLyricFontRate`: 0.1 to 2.
    pub lyric_font_rate: f32,
    /// `lyricFontWeight`: 100 to 900.
    pub lyric_font_weight: u32,
    /// `autoUpdate`: check for a new version at startup.
    pub auto_update: bool,
    /// `filenameTemplate`.
    pub filename_template: String,
    /// `autoCacheMusic`: online songs are kept in the cache directory while played.
    pub auto_cache_music: bool,
    /// `cacheSizeLimit` in bytes; the least recently played files go first.
    pub cache_size_limit: u64,
    /// `skipHiddenFiles`: dot files and folders are skipped by the local music scan.
    pub skip_hidden_files: bool,
    /// `playSetting.isJumpLyric`: the full player's lyrics use the spring/scale effect.
    pub jump_lyric: bool,
    /// `playSetting.bgPlaying` and `isAudioVisualizer`: stored, like the original's.
    pub bg_playing: bool,
    pub audio_visualizer: bool,
    /// `routePreloadEnabled`: stored, read by the original's router.
    pub route_preload: bool,
    /// `primaryDeviceId` / `secondaryDeviceId` of the audio output page's A/B comparison.
    pub audio_primary: String,
    pub audio_secondary: String,
    /// `selectSources` / `selectQuality`; empty until chosen (the first source then applies).
    pub selected_source: String,
    pub selected_quality: String,
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mobile_visual_mode: Default::default(),
            traffic_light: false,
            cover_dynamic_theme: true,
            lyric_font_family: "lyricfont".into(),
            lyric_font_rate: 1.,
            lyric_font_weight: 700,
            auto_update: true,
            filename_template: "%t - %s".into(),
            auto_cache_music: true,
            cache_size_limit: 1024 * 1024 * 1024,
            skip_hidden_files: true,
            jump_lyric: true,
            bg_playing: true,
            audio_visualizer: true,
            route_preload: true,
            audio_primary: String::new(),
            audio_secondary: String::new(),
            selected_source: String::new(),
            selected_quality: String::new(),
        }
    }
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            background: Default::default(),
            dark: false,
            english: false,
            volume: 0.5,
            play_mode: Default::default(),
            auto_hide_bottom: true,
            cover_layout: false,
            show_left_panel: true,
            pause_transition: true,
            audio_visualization: true,
            immersive_lyric_color: true,
            blur_lyrics: false,
            seamless: Default::default(),
            download_quality: Default::default(),
            tag_write_options: Default::default(),
            close_to_tray: true,
            close_behavior_configured: false,
            language: Some("system".into()),
            app: AppSettings::default(),
        }
    }
}
impl Preferences {
    /// Whether the interface is English: the chosen language, or the system's.
    pub fn english(&self) -> bool {
        match self.language.as_deref() {
            Some("en-US") => true,
            Some("zh-CN") => false,
            Some(_) => crate::host::system_is_english(),
            None => self.english,
        }
    }
}
impl Storage {
    /// Mobile hosts provide their private data directory explicitly.
    pub fn from_paths(paths: &crate::paths::Paths) -> Self {
        Self {
            root: paths.data.clone(),
        }
    }
    /// A store rooted at `root` (tests and tools).
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn application() -> Option<Self> {
        let root = if let Some(path) = std::env::var_os("MIO_GPUI_DATA_DIR") {
            PathBuf::from(path)
        } else if cfg!(target_os = "macos") {
            PathBuf::from(std::env::var_os("HOME")?)
                .join("Library/Application Support/Mio Music GPUI")
        } else if cfg!(target_os = "windows") {
            PathBuf::from(std::env::var_os("APPDATA")?).join("Mio Music GPUI")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))?
                .join("mio-music-gpui")
        };
        Some(Self { root })
    }
    /// The application's data directory.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn library_path(&self) -> PathBuf {
        self.root.join("playlists.db")
    }
    /// Download task records, in the original's `downloads.json` layout.
    pub fn downloads_path(&self) -> PathBuf {
        self.root.join("downloads.json")
    }
    pub fn load_online<T: DeserializeOwned>(&self, name: &str) -> io::Result<Option<T>> {
        if !matches!(name, "online-plugin.json" | "online-songs.json") {
            return Err(io::Error::other("Invalid online storage name"));
        }
        let file = match fs::File::open(self.root.join(name)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(io::Error::other("Online storage exceeds limit"));
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other)
    }
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> io::Result<Option<T>> {
        match fs::read(self.root.join(name)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn load_session(&self) -> io::Result<Option<PlaybackSession>> {
        let file = match fs::File::open(self.root.join(SESSION_FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(SESSION_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > SESSION_BYTES {
            return Err(io::Error::other("Playback session too large"));
        }
        let session: Option<PlaybackSession> = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::other("Invalid playback session"))?;
        if let Some(session) = &session {
            session.validate()?;
        }
        Ok(session)
    }
    fn encode_session(session: Option<&PlaybackSession>) -> io::Result<Vec<u8>> {
        if let Some(session) = session {
            session.validate()?;
        }
        let mut bytes = serde_json::to_vec(&session).map_err(io::Error::other)?;
        bytes.push(b'\n');
        if bytes.len() > SESSION_BYTES {
            return Err(io::Error::other("Playback session too large"));
        }
        Ok(bytes)
    }
    /// Writes the session and waits for it. A null tombstone clears the session through the
    /// same atomic replacement.
    pub fn save_session(&self, session: Option<&PlaybackSession>) -> io::Result<()> {
        let bytes = Self::encode_session(session)?;
        session_writer().write(self.root.clone(), bytes, true)
    }
    /// Validates and encodes now; the disk write (and its fsync) happens on the session writer
    /// thread. A failed write is reported by [`take_session_write_failure`].
    pub fn save_session_in_background(&self, session: Option<&PlaybackSession>) -> io::Result<()> {
        let bytes = Self::encode_session(session)?;
        session_writer().write(self.root.clone(), bytes, false)
    }
    pub fn save<T: Serialize>(&self, name: &str, data: &T) -> io::Result<()> {
        let mut bytes = serde_json::to_vec_pretty(data).map_err(io::Error::other)?;
        bytes.push(b'\n');
        write_atomic(&self.root, name, &bytes)
    }
}

fn write_atomic(root: &std::path::Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(root.join(name)).map_err(|error| error.error)?;
    Ok(())
}

/// Every session write goes through one thread in submission order: a newer snapshot
/// replaces one still waiting, and an older one can never land after it.
struct SessionWriter {
    state: std::sync::Mutex<WriterState>,
    changed: std::sync::Condvar,
}
#[derive(Default)]
struct WriterState {
    pending: Option<(PathBuf, Vec<u8>)>,
    /// Submitted and finished snapshot numbers, and the last write's outcome.
    submitted: u64,
    finished: u64,
    last: Option<(u64, io::ErrorKind)>,
    failed: bool,
}
fn session_writer() -> &'static SessionWriter {
    static WRITER: std::sync::OnceLock<&'static SessionWriter> = std::sync::OnceLock::new();
    WRITER.get_or_init(|| {
        let writer: &'static SessionWriter = Box::leak(Box::new(SessionWriter {
            state: Default::default(),
            changed: Default::default(),
        }));
        std::thread::Builder::new()
            .name("mio-session-writer".into())
            .spawn(move || writer.run())
            .expect("session writer thread");
        writer
    })
}
impl SessionWriter {
    fn lock(&self) -> std::sync::MutexGuard<'_, WriterState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn write(&self, root: PathBuf, bytes: Vec<u8>, wait: bool) -> io::Result<()> {
        let mut state = self.lock();
        state.submitted += 1;
        let number = state.submitted;
        state.pending = Some((root, bytes));
        self.changed.notify_all();
        if !wait {
            return Ok(());
        }
        while state.finished < number {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        match state.last {
            Some((at, kind)) if at >= number => Err(io::Error::new(kind, "Cannot save session")),
            _ => Ok(()),
        }
    }
    fn run(&self) {
        let mut state = self.lock();
        loop {
            let Some((root, bytes)) = state.pending.take() else {
                state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
                continue;
            };
            let number = state.submitted;
            drop(state);
            let result = write_atomic(&root, SESSION_FILE, &bytes);
            state = self.lock();
            match result {
                Ok(()) => state.last = None,
                Err(error) => {
                    state.last = Some((number, error.kind()));
                    state.failed = true;
                }
            }
            state.finished = number;
            self.changed.notify_all();
        }
    }
}
/// Whether a background session write failed since the last call.
pub fn take_session_write_failure() -> bool {
    std::mem::take(&mut session_writer().lock().failed)
}
/// Waits until every submitted session write reached the disk (or failed).
pub fn flush_session_writes() {
    let writer = session_writer();
    let mut state = writer.lock();
    let number = state.submitted;
    while state.finished < number {
        state = writer
            .changed
            .wait(state)
            .unwrap_or_else(|e| e.into_inner());
    }
}
