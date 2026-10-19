//! Runtime-bound preference editing; no stale view redirects to a new owner.
use crate::{
    api::{
        BalanceSettings, LyricStyleSettings, PlaybackSettingsSession, SeamlessSettings,
        StorageSettings,
    },
    catalog::SnapshotPermit,
    runtime,
};
use mio_core::playback::seamless::Config;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    closed: Arc<AtomicBool>,
    writer: Mutex<()>,
    _permit: SnapshotPermit,
}
fn project(config: Config) -> SeamlessSettings {
    SeamlessSettings {
        enabled: config.enabled,
        crossfade: config.crossfade,
        duration_ms: config.duration_ms as u32,
    }
}
fn config(value: SeamlessSettings) -> Result<Config, String> {
    if !(500..=8000).contains(&value.duration_ms) {
        return Err("invalid_input: transition duration".into());
    }
    Ok(Config {
        enabled: value.enabled,
        crossfade: value.crossfade,
        duration_ms: u64::from(value.duration_ms),
    })
}
pub(crate) fn project_style(value: mio_core::lyric_style::Config) -> LyricStyleSettings {
    LyricStyleSettings {
        family: value.family,
        rate: value.rate,
        weight: value.weight,
    }
}
fn style(value: LyricStyleSettings) -> Result<mio_core::lyric_style::Config, String> {
    let config = mio_core::lyric_style::Config {
        family: value.family,
        rate: value.rate,
        weight: value.weight,
    };
    config.validate()?;
    Ok(config)
}
pub(crate) fn begin() -> Result<PlaybackSettingsSession, String> {
    Ok(PlaybackSettingsSession {
        session: new_session()?,
    })
}
pub(crate) fn new_session() -> Result<Session, String> {
    let permit = SnapshotPermit::acquire()?;
    let (_, lease) = runtime::inspect_with_lease(|_| ())?;
    Ok(Session {
        lease,
        closed: Arc::new(AtomicBool::new(false)),
        writer: Mutex::new(()),
        _permit: permit,
    })
}

fn project_storage(v: mio_core::storage_settings::Config) -> StorageSettings {
    StorageSettings {
        auto_cache: v.auto_cache,
        cache_limit: v.cache_limit,
        filename_template: v.filename_template,
        basic_info: v.basic_info,
        cover: v.cover,
        lyrics: v.lyrics,
        download_lyrics: v.download_lyrics,
    }
}
fn storage_config(v: StorageSettings) -> Result<mio_core::storage_settings::Config, String> {
    let config = mio_core::storage_settings::Config {
        auto_cache: v.auto_cache,
        cache_limit: v.cache_limit,
        filename_template: v.filename_template,
        basic_info: v.basic_info,
        cover: v.cover,
        lyrics: v.lyrics,
        download_lyrics: v.download_lyrics,
    };
    config.validate()?;
    Ok(config)
}
impl Session {
    pub(crate) fn storage_settings(&self) -> Result<StorageSettings, String> {
        self.ensure_open()?;
        let value = self.lease.inspect(|core| core.storage_settings())?;
        self.ensure_open()?;
        Ok(project_storage(value))
    }
    pub(crate) fn save_storage_settings(
        &self,
        expected: StorageSettings,
        next: StorageSettings,
    ) -> Result<StorageSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: storage settings save in progress")?;
        self.ensure_open()?;
        let expected = storage_config(expected)?;
        let next = storage_config(next)?;
        self.lease.execute(runtime::Action::StorageSettings(
            expected,
            next.clone(),
            self.closed.clone(),
        ))?;
        Ok(project_storage(next))
    }
    pub(crate) fn lyric_style(&self) -> Result<LyricStyleSettings, String> {
        self.ensure_open()?;
        let value = self.lease.inspect(|core| core.lyric_style())?;
        self.ensure_open()?;
        Ok(project_style(value))
    }
    pub(crate) fn save_lyric_style(
        &self,
        expected: LyricStyleSettings,
        next: LyricStyleSettings,
    ) -> Result<LyricStyleSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: lyric settings save in progress")?;
        self.ensure_open()?;
        let expected = style(expected)?;
        let next = style(next)?;
        self.lease.execute(runtime::Action::LyricStyle(
            expected,
            next.clone(),
            self.closed.clone(),
        ))?;
        Ok(project_style(next))
    }
    pub(crate) fn balance(&self) -> Result<BalanceSettings, String> {
        self.ensure_open()?;
        let (enabled, balance) = self.lease.inspect(|core| core.balance())?;
        self.ensure_open()?;
        Ok(BalanceSettings { enabled, balance })
    }
    pub(crate) fn save_balance(
        &self,
        expected: BalanceSettings,
        next: BalanceSettings,
    ) -> Result<BalanceSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: audio effects save in progress")?;
        self.ensure_open()?;
        for value in [expected.balance, next.balance] {
            if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
                return Err("invalid_input: channel balance".into());
            }
        }
        self.lease.execute(runtime::Action::Balance(
            (expected.enabled, expected.balance),
            (next.enabled, next.balance),
            self.closed.clone(),
        ))?;
        Ok(next)
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn state(&self) -> Result<SeamlessSettings, String> {
        self.ensure_open()?;
        let value = self.lease.inspect(|core| core.seamless_config())?;
        self.ensure_open()?;
        Ok(project(value))
    }
    pub(crate) fn save(
        &self,
        expected: SeamlessSettings,
        next: SeamlessSettings,
    ) -> Result<SeamlessSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: playback settings save in progress")?;
        self.ensure_open()?;
        let expected = config(expected)?;
        let next = config(next)?;
        self.lease.execute(runtime::Action::Seamless(
            expected,
            next,
            self.closed.clone(),
        ))?;
        // Once admitted, preserve the real result even if this view closed.
        Ok(project(next))
    }
}

pub(crate) fn project_visuals(v: mio_core::visuals::Config) -> crate::api::VisualSettings {
    crate::api::VisualSettings {
        background: v.background,
        blur_lyrics: v.blur_lyrics,
        spectrum: v.spectrum,
        jump_lyrics: v.jump_lyrics,
        immersive_lyric_color: v.immersive_lyric_color,
        cover_layout: v.cover_layout,
        show_left_panel: v.show_left_panel,
        auto_hide_bottom: v.auto_hide_bottom,
        pause_transition: v.pause_transition,
        cover_dynamic_theme: v.cover_dynamic_theme,
        mode: match v.mode {
            mio_core::visuals::Mode::Auto => "auto",
            mio_core::visuals::Mode::Reduced => "reduced",
            mio_core::visuals::Mode::Full => "full",
        }
        .into(),
    }
}
fn visual_config(v: crate::api::VisualSettings) -> Result<mio_core::visuals::Config, String> {
    Ok(mio_core::visuals::Config {
        background: v.background,
        blur_lyrics: v.blur_lyrics,
        spectrum: v.spectrum,
        jump_lyrics: v.jump_lyrics,
        immersive_lyric_color: v.immersive_lyric_color,
        cover_layout: v.cover_layout,
        show_left_panel: v.show_left_panel,
        auto_hide_bottom: v.auto_hide_bottom,
        pause_transition: v.pause_transition,
        cover_dynamic_theme: v.cover_dynamic_theme,
        mode: match v.mode.as_str() {
            "auto" => mio_core::visuals::Mode::Auto,
            "reduced" => mio_core::visuals::Mode::Reduced,
            "full" => mio_core::visuals::Mode::Full,
            _ => return Err("invalid_input: visual mode".into()),
        },
    })
}
impl Session {
    pub(crate) fn visuals(&self) -> Result<crate::api::VisualSettings, String> {
        self.ensure_open()?;
        let value = self.lease.inspect(|core| core.visuals())?;
        self.ensure_open()?;
        Ok(project_visuals(value))
    }
    pub(crate) fn save_visuals(
        &self,
        expected: crate::api::VisualSettings,
        next: crate::api::VisualSettings,
    ) -> Result<crate::api::VisualSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: visual settings")?;
        self.ensure_open()?;
        let expected = visual_config(expected)?;
        let next = visual_config(next)?;
        self.lease.execute(runtime::Action::Visuals(
            expected,
            next,
            self.closed.clone(),
        ))?;
        Ok(project_visuals(next))
    }
}

fn background_preset(value: &str) -> Result<mio_core::background_config::Preset, String> {
    use mio_core::background_config::Preset;
    Ok(match value {
        "auto" => Preset::Auto,
        "performance" => Preset::Performance,
        "quality" => Preset::Quality,
        "custom" => Preset::Custom,
        _ => return Err("invalid_input: background preset".into()),
    })
}
fn project_background(
    v: mio_core::background_config::BackgroundConfig,
) -> crate::api::BackgroundSettings {
    use mio_core::background_config::Preset;
    crate::api::BackgroundSettings {
        preset: match v.preset {
            Preset::Auto => "auto",
            Preset::Performance => "performance",
            Preset::Quality => "quality",
            Preset::Custom => "custom",
        }
        .into(),
        enabled: v.enabled,
        audio_response: v.audio_response,
        render_scale: v.render_scale,
        flow_speed: v.flow_speed,
        static_mode: v.static_mode,
        fps: v.fps,
    }
}
fn background_config(
    v: crate::api::BackgroundSettings,
) -> Result<mio_core::background_config::BackgroundConfig, String> {
    let next = mio_core::background_config::BackgroundConfig {
        preset: background_preset(&v.preset)?,
        enabled: v.enabled,
        audio_response: v.audio_response,
        render_scale: v.render_scale,
        flow_speed: v.flow_speed,
        static_mode: v.static_mode,
        fps: v.fps,
    };
    if next != next.validated() {
        return Err("invalid_input: background values".into());
    }
    Ok(next)
}
impl Session {
    pub(crate) fn background(&self) -> Result<crate::api::BackgroundSettings, String> {
        self.ensure_open()?;
        let value = self.lease.inspect(|core| core.preferences.background)?;
        self.ensure_open()?;
        Ok(project_background(value))
    }
    pub(crate) fn save_background(
        &self,
        expected: crate::api::BackgroundSettings,
        next: crate::api::BackgroundSettings,
    ) -> Result<crate::api::BackgroundSettings, String> {
        self.commit_background(background_config(expected)?, background_config(next)?)
    }
    pub(crate) fn preset_background(
        &self,
        expected: crate::api::BackgroundSettings,
        preset: Option<String>,
    ) -> Result<crate::api::BackgroundSettings, String> {
        let expected = background_config(expected)?;
        let next = match preset {
            Some(value) => {
                let mut next = expected;
                next.apply_preset(background_preset(&value)?);
                next
            }
            None => mio_core::background_config::BackgroundConfig::default(),
        };
        self.commit_background(expected, next)
    }
    fn commit_background(
        &self,
        expected: mio_core::background_config::BackgroundConfig,
        next: mio_core::background_config::BackgroundConfig,
    ) -> Result<crate::api::BackgroundSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: background settings")?;
        self.ensure_open()?;
        self.lease.execute(runtime::Action::Background(
            expected,
            next,
            self.closed.clone(),
        ))?;
        // Preserve the result of an admitted write even if the UI closes meanwhile.
        Ok(project_background(next))
    }
}

fn project_general(core: &mio_core::app_core::AppCore) -> crate::api::GeneralSettings {
    let p = &core.preferences;
    crate::api::GeneralSettings {
        dark: p.dark,
        language: p
            .language
            .clone()
            .unwrap_or_else(|| if p.english { "en-US" } else { "zh-CN" }.into()),
        route_preload: p.app.route_preload,
    }
}
pub(crate) fn apply_general(
    core: &mut mio_core::app_core::AppCore,
    expected: crate::api::GeneralSettings,
    value: crate::api::GeneralSettings,
    closed: &AtomicBool,
) -> Result<(), String> {
    if closed.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    if project_general(core) != expected {
        return Err("settings_changed: reload settings".into());
    }
    let english = match value.language.as_str() {
        "system" => mio_core::host::system_is_english(),
        "zh-CN" => false,
        "en-US" => true,
        _ => return Err("invalid_input: language".into()),
    };
    let mut next = core.preferences.clone();
    next.dark = value.dark;
    next.english = english;
    next.language = Some(value.language);
    next.app.route_preload = value.route_preload;
    core.store
        .save("preferences.json", &next)
        .map_err(|_| "storage_unavailable: cannot save settings")?;
    core.preferences = next;
    Ok(())
}
impl Session {
    pub(crate) fn save_general(
        &self,
        expected: crate::api::GeneralSettings,
        next: crate::api::GeneralSettings,
    ) -> Result<crate::api::GeneralSettings, String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: settings save in progress")?;
        self.ensure_open()?;
        self.lease.execute(runtime::Action::GeneralSettings(
            expected,
            next.clone(),
            self.closed.clone(),
        ))?;
        Ok(next)
    }
    pub(crate) fn overview(&self) -> Result<crate::api::SettingsOverview, String> {
        self.ensure_open()?;
        let mut view = self.lease.inspect(|core| {
            let artists: std::collections::HashSet<_> = core
                .player
                .queue
                .iter()
                .flat_map(|song| song.artist.split(['、', '/', '&', ',', ';']))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            crate::api::SettingsOverview {
                settings: project_general(core),
                paths: crate::api::HostPaths {
                    data: core.paths.data.to_string_lossy().into_owned(),
                    cache: core.paths.cache.to_string_lossy().into_owned(),
                    downloads: core.paths.downloads.to_string_lossy().into_owned(),
                },
                queue_count: core.player.queue.len() as u32,
                queue_seconds: core.player.queue.iter().map(|song| song.duration).sum(),
                artist_count: artists.len() as u32,
                cache_files: 0,
                cache_bytes: 0,
            }
        })?;
        let directory = std::path::Path::new(&view.paths.cache).join("audio");
        if std::fs::symlink_metadata(&directory)
            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        {
            if let Ok(entries) = std::fs::read_dir(directory) {
                for entry in entries {
                    self.ensure_open()?;
                    let entry = entry.map_err(|_| "storage_unavailable: cache entry")?;
                    let name = entry.file_name();
                    if !name.to_str().is_some_and(|name| {
                        !name.is_empty()
                            && name
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    }) {
                        continue;
                    }
                    if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
                        if meta.is_file() && !meta.file_type().is_symlink() {
                            view.cache_files += 1;
                            view.cache_bytes = view.cache_bytes.saturating_add(meta.len());
                        }
                    }
                }
            }
        }
        self.ensure_open()?;
        Ok(view)
    }
    /// Delete completed audio only; leave partial transfer locks and other caches intact.
    pub(crate) fn clear_audio_cache(&self) -> Result<(), String> {
        let _writer = self
            .writer
            .try_lock()
            .map_err(|_| "busy: cache operation")?;
        self.ensure_open()?;
        let paths = self.lease.inspect(|core| core.paths.clone())?;
        let directory = paths.cache.join("audio");
        match std::fs::symlink_metadata(&directory) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => (),
            _ => return Err("storage_unavailable: invalid audio cache directory".into()),
        }
        let canonical =
            std::fs::canonicalize(&directory).map_err(|_| "storage_unavailable: cache path")?;
        for protected in [paths.data, paths.downloads] {
            let protected = std::fs::canonicalize(protected)
                .map_err(|_| "storage_unavailable: protected path")?;
            if canonical.starts_with(&protected) || protected.starts_with(&canonical) {
                return Err("invalid_input: cache overlaps application data".into());
            }
        }
        let entries =
            std::fs::read_dir(&directory).map_err(|_| "storage_unavailable: cache directory")?;
        for entry in entries {
            self.ensure_open()?;
            let entry = entry.map_err(|_| "storage_unavailable: cache entry")?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("storage_unavailable: cache filename")?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                continue;
            }
            let meta = std::fs::symlink_metadata(entry.path())
                .map_err(|_| "storage_unavailable: cache metadata")?;
            if meta.is_file() && !meta.file_type().is_symlink() {
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err("storage_unavailable: cannot clear audio cache".into());
                    }
                }
            }
        }
        Ok(())
    }
}
