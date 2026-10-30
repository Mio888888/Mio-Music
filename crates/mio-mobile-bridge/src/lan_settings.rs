//! Settings calls from the paired LAN browser. Each operation reads the current
//! value, applies the requested change on top of it and saves through the same
//! sessions the app's own settings pages use, so every rule and limit is shared.
use crate::api::*;
use serde_json::{Value, json};
use std::sync::Mutex;

/// A backup or restore waits here between its preview and the confirmation.
static UPLOAD: Mutex<Option<S3UploadPreparation>> = Mutex::new(None);
static RESTORE: Mutex<Option<S3RestorePreparation>> = Mutex::new(None);

fn s3_config(args: &Value) -> Result<S3RestoreConfig, String> {
    let config = args.get("config").ok_or("invalid_input: config")?;
    Ok(S3RestoreConfig {
        endpoint: text(config, "endpoint")?,
        region: text(config, "region")?,
        access_key_id: text(config, "accessKeyId")?,
        secret_access_key: text(config, "secretAccessKey")?,
        bucket: text(config, "bucket")?,
        path_style: flag(config, "pathStyle")?,
    })
}
fn preview(value: &S3RestorePreview) -> Value {
    json!({
        "timestamp": value.timestamp,
        "playlists": value.playlists,
        "songs": value.songs,
        "localReferences": value.local_references,
        "plugins": value.plugins,
    })
}
fn slot<T>(slot: &Mutex<Option<T>>) -> std::sync::MutexGuard<'_, Option<T>> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("invalid_input: {key}"))
}
fn flag(args: &Value, key: &str) -> Result<bool, String> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("invalid_input: {key}"))
}
fn number(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("invalid_input: {key}"))
}
fn whole(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("invalid_input: {key}"))
}
fn optional_flag(args: &Value, key: &str) -> Result<Option<bool>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("invalid_input: {key}")),
    }
}
fn optional_text(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|text| Some(text.to_owned()))
            .ok_or_else(|| format!("invalid_input: {key}")),
    }
}
fn optional_number(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(Some)
            .ok_or_else(|| format!("invalid_input: {key}")),
    }
}

pub(crate) fn call(
    lease: &crate::runtime::RuntimeLease,
    closed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    op: &str,
    args: Value,
) -> Result<Value, String> {
    lease.ensure_open()?;
    match op {
        "general.get" => general(),
        "general.set" => {
            let session = begin_general_settings()?;
            let expected = session.overview()?.settings;
            let mut next = expected.clone();
            if let Some(dark) = optional_flag(&args, "dark")? {
                next.dark = dark;
            }
            if let Some(language) = optional_text(&args, "language")? {
                next.language = language;
            }
            if let Some(preload) = optional_flag(&args, "routePreload")? {
                next.route_preload = preload;
            }
            if next != expected {
                session.save(expected, next)?;
            }
            general()
        }
        "cache.clear" => {
            begin_general_settings()?.clear_audio_cache()?;
            general()
        }
        "queue.clear" => {
            let player = player_snapshot()?;
            begin_queue(player.queue_revision, player.source_revision)?.clear()?;
            general()
        }
        "lyric.get" => lyric(),
        "lyric.set" => {
            let session = begin_lyric_settings()?;
            let expected = session.state()?;
            let mut next = expected.clone();
            if let Some(family) = optional_text(&args, "family")? {
                next.family = family;
            }
            if let Some(rate) = optional_number(&args, "rate")? {
                next.rate = rate as f32;
            }
            if let Some(weight) = optional_number(&args, "weight")? {
                next.weight = weight as u32;
            }
            if next != expected {
                session.save(expected, next)?;
            }
            lyric()
        }
        "visual.get" => visual(),
        "visual.set" => {
            let session = begin_visual_settings()?;
            let expected = session.state()?;
            let mut next = expected.clone();
            for (key, slot) in [
                ("immersiveLyricColor", &mut next.immersive_lyric_color),
                ("coverLayout", &mut next.cover_layout),
                ("showLeftPanel", &mut next.show_left_panel),
                ("autoHideBottom", &mut next.auto_hide_bottom),
                ("pauseTransition", &mut next.pause_transition),
                ("coverDynamicTheme", &mut next.cover_dynamic_theme),
                ("jumpLyrics", &mut next.jump_lyrics),
                ("background", &mut next.background),
                ("blurLyrics", &mut next.blur_lyrics),
                ("spectrum", &mut next.spectrum),
            ] {
                if let Some(value) = optional_flag(&args, key)? {
                    *slot = value;
                }
            }
            if let Some(mode) = optional_text(&args, "mode")? {
                next.mode = mode;
            }
            if next != expected {
                session.save(expected, next)?;
            }
            visual()
        }
        "background.get" => background(),
        "background.set" => {
            let session = begin_visual_settings()?;
            let expected = session.background()?;
            if let Some(preset) = args.get("preset").and_then(Value::as_str) {
                session.preset_background(
                    expected,
                    if preset.is_empty() {
                        None
                    } else {
                        Some(preset.to_owned())
                    },
                )?;
            } else {
                let mut next = expected.clone();
                if let Some(value) = optional_flag(&args, "enabled")? {
                    next.enabled = value;
                }
                if let Some(value) = optional_flag(&args, "audioResponse")? {
                    next.audio_response = value;
                }
                if let Some(value) = optional_flag(&args, "staticMode")? {
                    next.static_mode = value;
                }
                if let Some(value) = optional_number(&args, "renderScale")? {
                    next.render_scale = value as f32;
                }
                if let Some(value) = optional_number(&args, "flowSpeed")? {
                    next.flow_speed = value as f32;
                }
                if let Some(value) = optional_number(&args, "fps")? {
                    next.fps = value as u32;
                }
                if next != expected {
                    // Hand-tuned values are a custom profile, like the app's own page.
                    next.preset = "custom".into();
                    session.save_background(expected, next)?;
                }
            }
            background()
        }
        "playback.get" => playback(),
        "playback.set" => {
            let session = begin_playback_settings()?;
            let expected = session.state()?;
            let mut next = expected;
            if let Some(value) = optional_flag(&args, "enabled")? {
                next.enabled = value;
            }
            if let Some(value) = optional_flag(&args, "crossfade")? {
                next.crossfade = value;
            }
            if let Some(value) = optional_number(&args, "durationMs")? {
                next.duration_ms = value as u32;
            }
            if next.enabled != expected.enabled
                || next.crossfade != expected.crossfade
                || next.duration_ms != expected.duration_ms
            {
                session.save(expected, next)?;
            }
            playback()
        }
        "balance.get" => balance(),
        "balance.set" => {
            let session = begin_audio_effects()?;
            let expected = session.state()?;
            let mut next = expected;
            if let Some(value) = optional_flag(&args, "enabled")? {
                next.enabled = value;
            }
            if let Some(value) = optional_number(&args, "balance")? {
                next.balance = value;
            }
            if next.enabled != expected.enabled || next.balance != expected.balance {
                session.save(expected, next)?;
            }
            balance()
        }
        "storage.get" => storage(),
        "storage.set" => {
            let session = begin_storage_settings()?;
            let expected = session.state()?;
            let mut next = expected.clone();
            for (key, slot) in [
                ("autoCache", &mut next.auto_cache),
                ("basicInfo", &mut next.basic_info),
                ("cover", &mut next.cover),
                ("lyrics", &mut next.lyrics),
                ("downloadLyrics", &mut next.download_lyrics),
            ] {
                if let Some(value) = optional_flag(&args, key)? {
                    *slot = value;
                }
            }
            if let Some(value) = optional_number(&args, "cacheLimit")? {
                next.cache_limit = value as u64;
            }
            if let Some(value) = optional_text(&args, "filenameTemplate")? {
                next.filename_template = value;
            }
            if next != expected {
                session.save(expected, next)?;
            }
            storage()
        }
        "filename.preview" => Ok(json!({
            "name": preview_download_filename(
                text(&args, "template")?,
                flag(&args, "english")?,
            )?
        })),
        "sources.get" => sources(),
        "sources.select" => {
            source_selection()?.select(text(&args, "source")?)?;
            sources()
        }
        "sources.quality" => {
            source_selection()?.select_quality(text(&args, "quality")?, flag(&args, "all")?)?;
            sources()
        }
        "plugins.list" => plugins(),
        "plugins.logs" => Ok(json!({ "lines": playback_plugin_logs(text(&args, "id")?)? })),
        "plugins.activate" => {
            activate_playback_plugin(text(&args, "id")?)?;
            plugins()
        }
        "plugins.enable" => {
            set_list_plugin_enabled(text(&args, "id")?, flag(&args, "enabled")?)?;
            plugins()
        }
        "plugins.provide" => {
            set_list_plugin_provide(
                text(&args, "id")?,
                flag(&args, "play")?,
                flag(&args, "download")?,
            )?;
            plugins()
        }
        "plugins.configure" => {
            configure_list_plugin(
                text(&args, "id")?,
                text(&args, "values")?,
                whole(&args, "revision")?,
            )?;
            plugins()
        }
        "plugins.remove" => {
            let kind = match text(&args, "kind")?.as_str() {
                "playback" => PluginKind::Playback,
                "list" => PluginKind::List,
                _ => return Err("invalid_input: kind".into()),
            };
            remove_plugin(kind, text(&args, "id")?)?;
            plugins()
        }
        "plugins.installUrl" => {
            crate::plugins::install_received(
                lease.clone(),
                text(&args, "url")?,
                true,
                closed.clone(),
            )
            .map_err(plugin_error)?;
            plugins()
        }
        "plugins.installCode" => {
            crate::plugins::install_received(
                lease.clone(),
                text(&args, "code")?,
                false,
                closed.clone(),
            )
            .map_err(plugin_error)?;
            plugins()
        }
        "eq.get" => {
            let session = begin_equalizer()?;
            Ok(equalizer(&session.state()?))
        }
        "eq.edit" => equalizer_edit(&args),
        "eq.export" => {
            let session = begin_equalizer()?;
            let state = session.state()?;
            Ok(json!({ "text": session.export_config(state.revision)? }))
        }
        "queue.export" => Ok(json!({ "text": queue_document()?.export_text()? })),
        "queue.import" => {
            let document = queue_document()?;
            document.prepare_text(text(&args, "text")?)?;
            let result = document.merge()?;
            Ok(json!({
                "total": result.total,
                "added": result.added,
                "skipped": result.skipped,
            }))
        }
        "backup.get" => backup(),
        "backup.save" => {
            let session = begin_s3_settings()?;
            let saved = session.state()?;
            session.save(S3SavedConnection {
                endpoint: text(&args, "endpoint")?,
                region: text(&args, "region")?,
                access_key_id: text(&args, "accessKeyId")?,
                bucket: text(&args, "bucket")?,
                path_style: flag(&args, "pathStyle")?,
                max_backups: whole(&args, "maxBackups")? as u32,
                last_backup_time: saved.last_backup_time,
            })?;
            backup()
        }
        "backup.uploadPrepare" => {
            slot(&UPLOAD).take();
            let session =
                begin_s3_upload_preparation(s3_config(&args)?, whole(&args, "maxBackups")? as u32)?;
            let value = preview(&session.load()?);
            *slot(&UPLOAD) = Some(session);
            Ok(value)
        }
        "backup.uploadRun" => {
            let session = slot(&UPLOAD)
                .take()
                .ok_or("invalid_input: no backup prepared")?;
            let outcome = session.upload(text(&args, "password")?)?;
            Ok(json!({
                "timestamp": outcome.timestamp,
                "deleted": outcome.deleted,
                "warnings": outcome.warnings,
            }))
        }
        "backup.restorePrepare" => {
            slot(&RESTORE).take();
            let session = begin_s3_restore_preparation(s3_config(&args)?)?;
            let value = preview(&session.load(text(&args, "password")?)?);
            *slot(&RESTORE) = Some(session);
            Ok(value)
        }
        "backup.restoreRun" => {
            let session = slot(&RESTORE)
                .take()
                .ok_or("invalid_input: no restore prepared")?;
            let outcome = session.commit(flag(&args, "overwrite")?)?;
            Ok(json!({
                "playlistsAdded": outcome.playlists_added,
                "songsAdded": outcome.songs_added,
                "songsSkipped": outcome.songs_skipped,
                "pluginsApplied": outcome.plugins_applied,
                "pluginsAdded": outcome.plugins_added,
                "pluginsSkipped": outcome.plugins_skipped,
                "settingsApplied": outcome.settings_applied,
                "warnings": outcome.warnings,
            }))
        }
        "backup.cancel" => {
            slot(&UPLOAD).take();
            slot(&RESTORE).take();
            Ok(Value::Null)
        }
        "about.get" => about(),
        "about.autoUpdate" => {
            let session = begin_release_session(env!("CARGO_PKG_VERSION").into())?;
            let expected = session.state()?;
            let enabled = flag(&args, "enabled")?;
            if enabled != expected {
                session.save(expected, enabled)?;
            }
            about()
        }
        "about.check" => {
            let session = begin_release_session(env!("CARGO_PKG_VERSION").into())?;
            Ok(match session.check(false)? {
                Some(release) => json!({
                    "version": release.version,
                    "tag": release.tag,
                    "notes": release.notes,
                    "notesTruncated": release.notes_truncated,
                    "newer": release.newer,
                }),
                None => Value::Null,
            })
        }
        _ => Err("invalid_path".into()),
    }
}

fn plugin_error(error: String) -> String {
    if error == "cancelled" {
        error
    } else if error.starts_with("source_changed") {
        "source_changed".into()
    } else {
        "plugin_install_failed".into()
    }
}

fn general() -> Result<Value, String> {
    let overview = begin_general_settings()?.overview()?;
    Ok(json!({
        "dark": overview.settings.dark,
        "language": overview.settings.language,
        "routePreload": overview.settings.route_preload,
        "paths": {
            "data": overview.paths.data,
            "cache": overview.paths.cache,
            "downloads": overview.paths.downloads,
        },
        "queueCount": overview.queue_count,
        "queueSeconds": overview.queue_seconds,
        "artistCount": overview.artist_count,
        "cacheFiles": overview.cache_files,
        "cacheBytes": overview.cache_bytes,
    }))
}

fn lyric() -> Result<Value, String> {
    let value = begin_lyric_settings()?.state()?;
    Ok(json!({ "family": value.family, "rate": value.rate, "weight": value.weight }))
}

fn visual() -> Result<Value, String> {
    let value = begin_visual_settings()?.state()?;
    Ok(json!({
        "immersiveLyricColor": value.immersive_lyric_color,
        "coverLayout": value.cover_layout,
        "showLeftPanel": value.show_left_panel,
        "autoHideBottom": value.auto_hide_bottom,
        "pauseTransition": value.pause_transition,
        "coverDynamicTheme": value.cover_dynamic_theme,
        "jumpLyrics": value.jump_lyrics,
        "background": value.background,
        "blurLyrics": value.blur_lyrics,
        "spectrum": value.spectrum,
        "mode": value.mode,
    }))
}

fn background() -> Result<Value, String> {
    let value = begin_visual_settings()?.background()?;
    Ok(json!({
        "preset": value.preset,
        "enabled": value.enabled,
        "audioResponse": value.audio_response,
        "renderScale": value.render_scale,
        "flowSpeed": value.flow_speed,
        "staticMode": value.static_mode,
        "fps": value.fps,
    }))
}

fn playback() -> Result<Value, String> {
    let value = begin_playback_settings()?.state()?;
    Ok(json!({
        "enabled": value.enabled,
        "crossfade": value.crossfade,
        "durationMs": value.duration_ms,
    }))
}

fn balance() -> Result<Value, String> {
    let value = begin_audio_effects()?.state()?;
    Ok(json!({ "enabled": value.enabled, "balance": value.balance }))
}

fn storage() -> Result<Value, String> {
    let value = begin_storage_settings()?.state()?;
    Ok(json!({
        "autoCache": value.auto_cache,
        "cacheLimit": value.cache_limit,
        "filenameTemplate": value.filename_template,
        "basicInfo": value.basic_info,
        "cover": value.cover,
        "lyrics": value.lyrics,
        "downloadLyrics": value.download_lyrics,
    }))
}

fn source_selection() -> Result<SourceSelectionSession, String> {
    let player = player_snapshot()?;
    begin_source_selection(player.source_revision, player.source_selection_revision)
}

fn sources() -> Result<Value, String> {
    let snapshot = source_selection()?.snapshot()?;
    Ok(json!({
        "selected": snapshot.selected,
        "sources": snapshot.sources.iter().map(|source| json!({
            "id": source.id,
            "name": source.name,
            "qualities": source.qualities,
            "preferredQuality": source.preferred_quality,
            "pluginName": source.plugin_name,
        })).collect::<Vec<_>>(),
    }))
}

fn plugins() -> Result<Value, String> {
    let rows = installed_plugins()?
        .into_iter()
        .map(|plugin| {
            json!({
                "revision": plugin.revision,
                "id": plugin.id,
                "kind": match plugin.kind {
                    PluginKind::Playback => "playback",
                    PluginKind::List => "list",
                },
                "name": plugin.name,
                "version": plugin.version,
                "author": plugin.author,
                "description": plugin.description,
                "sources": plugin.sources.iter()
                    .map(|source| json!({"id": source.id, "name": source.name}))
                    .collect::<Vec<_>>(),
                "enabled": plugin.enabled,
                "active": plugin.active,
                "canPlay": plugin.can_play,
                "canDownload": plugin.can_download,
                "providePlay": plugin.provide_play,
                "provideDownload": plugin.provide_download,
                "bundled": plugin.bundled,
                "configured": plugin.configured,
                "fields": plugin.fields.iter().map(|field| json!({
                    "key": field.key,
                    "label": field.label,
                    "kind": field.kind,
                    "value": serde_json::from_str::<Value>(&field.value_json).unwrap_or(Value::Null),
                    "required": field.required,
                    "placeholder": field.placeholder,
                    "description": field.description,
                    "options": field.options.iter()
                        .map(|option| json!({"value": option.value, "label": option.label}))
                        .collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "plugins": rows }))
}

fn filter_name(kind: EqFilter) -> &'static str {
    match kind {
        EqFilter::Peak => "peak",
        EqFilter::LowShelf => "lowShelf",
        EqFilter::HighShelf => "highShelf",
        EqFilter::LowPass => "lowPass",
        EqFilter::HighPass => "highPass",
        EqFilter::Notch => "notch",
    }
}

fn filter_kind(name: &str) -> Result<EqFilter, String> {
    Ok(match name {
        "peak" => EqFilter::Peak,
        "lowShelf" => EqFilter::LowShelf,
        "highShelf" => EqFilter::HighShelf,
        "lowPass" => EqFilter::LowPass,
        "highPass" => EqFilter::HighPass,
        "notch" => EqFilter::Notch,
        _ => return Err("invalid_input: filter".into()),
    })
}

fn equalizer(state: &EqSnapshot) -> Value {
    json!({
        "enabled": state.enabled,
        "globalGain": state.global_gain,
        "referenceRate": state.reference_rate,
        "currentPresetId": state.current_preset_id,
        "bands": state.bands.iter().map(|band| json!({
            "frequency": band.frequency,
            "gain": band.gain,
            "q": band.q,
            "kind": filter_name(band.kind),
            "enabled": band.enabled,
        })).collect::<Vec<_>>(),
        "presets": state.presets.iter().map(|preset| json!({
            "id": preset.id,
            "name": preset.name,
            "localeKey": preset.locale_key,
            "builtin": preset.builtin,
        })).collect::<Vec<_>>(),
        "response": state.response.iter()
            .map(|point| json!([point.frequency, point.db]))
            .collect::<Vec<_>>(),
    })
}

fn equalizer_edit(args: &Value) -> Result<Value, String> {
    let session = begin_equalizer()?;
    let revision = session.state()?.revision;
    let state = match text(args, "action")?.as_str() {
        "enabled" => session.set_enabled(revision, flag(args, "enabled")?)?,
        "gain" => session.set_global_gain(revision, number(args, "gain")?)?,
        "band" => session.set_band(
            revision,
            whole(args, "index")? as u32,
            EqBand {
                frequency: number(args, "frequency")?,
                gain: number(args, "gain")?,
                q: number(args, "q")?,
                kind: filter_kind(&text(args, "kind")?)?,
                enabled: flag(args, "enabled")?,
            },
        )?,
        "preset" => session.select_preset(revision, text(args, "id")?)?,
        "savePreset" => session.save_preset(revision, text(args, "name")?)?,
        "updatePreset" => session.update_preset(revision, text(args, "id")?)?,
        "deletePreset" => session.delete_preset(revision, text(args, "id")?)?,
        "importConfig" => session.import_config(revision, text(args, "text")?)?,
        "importText" => session.import_text(revision, text(args, "name")?, text(args, "text")?)?,
        "reset" => session.reset(revision)?,
        _ => return Err("invalid_input: action".into()),
    };
    Ok(equalizer(&state))
}

fn about() -> Result<Value, String> {
    let session = begin_release_session(env!("CARGO_PKG_VERSION").into())?;
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "autoUpdate": session.state()?,
    }))
}

fn backup() -> Result<Value, String> {
    let value = begin_s3_settings()?.state()?;
    Ok(json!({
        "endpoint": value.endpoint,
        "region": value.region,
        "accessKeyId": value.access_key_id,
        "bucket": value.bucket,
        "pathStyle": value.path_style,
        "maxBackups": value.max_backups,
        "lastBackupTime": value.last_backup_time,
    }))
}

fn queue_document() -> Result<QueueDocumentSession, String> {
    let player = player_snapshot()?;
    begin_queue_document(player.queue_revision, player.source_revision)
}
