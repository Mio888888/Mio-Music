//! The playback category of the settings page (`PlaybackSection.vue`): the play queue's
//! statistics, export, import and clearing (`PlaylistSettings.vue`), the audio output devices
//! with A/B comparison (`AudioOutputSettings.vue`), and the full-screen player's performance
//! switches. The background effect group lives in `background_settings.rs`.
use super::settings_page::Choice;
use super::*;
use crate::audio_device::{self, Device};
use crate::playlist_file;
use gpui_kit::component::{
    IndexPath, WindowExt,
    notification::Notification,
    select::{Select, SelectEvent, SelectState},
    slider::{SliderEvent, SliderState},
    switch::Switch,
};
use std::{cell::Cell, rc::Rc};

/// Output device state of the page (`useAudioOutputStore`).
pub(super) struct AudioOutput {
    devices: Vec<Device>,
    loading: bool,
    error: Option<String>,
    /// The device in use, as the system reports it.
    current: u32,
    /// `primaryDeviceId` / `secondaryDeviceId`: device ids as text.
    primary: String,
    secondary: String,
    /// `activeABChannel`: true while "A" is in use.
    channel_a: bool,
    select_a: Entity<SelectState<Vec<Choice>>>,
    select_b: Entity<SelectState<Vec<Choice>>>,
    volume: Entity<SliderState>,
    volume_pressed: Rc<Cell<bool>>,
    /// What the selects list, to refill them only on change.
    signature: String,
    /// Scan again the next time the category is drawn.
    pub dirty: bool,
}

impl AudioOutput {
    pub(super) fn new(
        primary: String,
        secondary: String,
        window: &mut Window,
        cx: &mut Context<Mio>,
    ) -> (Self, Vec<Subscription>) {
        let select_a = cx.new(|cx| SelectState::new(Vec::<Choice>::new(), None, window, cx));
        let select_b = cx.new(|cx| SelectState::new(Vec::<Choice>::new(), None, window, cx));
        let volume = cx.new(|_| SliderState::new().min(0.).max(100.).step(1.));
        let subscriptions = vec![
            cx.subscribe_in(&select_a, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(index)) = event
                    && let Some(id) = this.sorted_device_ids().get(*index).cloned()
                {
                    this.audio_output.primary = id;
                    this.persist_audio_output();
                    cx.notify();
                }
            }),
            cx.subscribe_in(&select_b, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(index)) = event
                    && let Some(id) = this.sorted_device_ids().get(*index).cloned()
                {
                    this.audio_output.secondary = id;
                    this.persist_audio_output();
                    cx.notify();
                }
            }),
            cx.subscribe_in(&volume, window, |this: &mut Mio, _, event, _, cx| {
                let value = match event {
                    SliderEvent::Change(v) | SliderEvent::Release(v) => v.start(),
                };
                let current = this.audio_output.current;
                let wanted = value / 100.;
                if let Some(device) = this
                    .audio_output
                    .devices
                    .iter_mut()
                    .find(|d| d.id == current)
                    && (device.volume - wanted).abs() > 0.004
                {
                    device.volume = wanted;
                    let _ = audio_device::set_volume(current, wanted);
                    cx.notify();
                }
            }),
        ];
        (
            Self {
                devices: Vec::new(),
                loading: false,
                error: None,
                current: 0,
                primary,
                secondary,
                channel_a: true,
                select_a,
                select_b,
                volume,
                volume_pressed: Rc::new(Cell::new(false)),
                signature: String::new(),
                dirty: true,
            },
            subscriptions,
        )
    }
}

/// What the queue dialogs are for.
#[derive(Clone, Copy)]
enum QueueDialog {
    Export,
    Import,
    Clear,
}

struct PlaylistDialog {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    kind: QueueDialog,
    /// The file chosen in the import dialog: display name and contents.
    file: Option<(String, Vec<u8>)>,
}

impl Mio {
    // ----- audio output -------------------------------------------------------------------

    fn sorted_device_ids(&self) -> Vec<String> {
        let mut devices: Vec<&Device> = self.audio_output.devices.iter().collect();
        devices.sort_by_cached_key(|d| d.name.to_lowercase());
        devices.iter().map(|d| d.id.to_string()).collect()
    }

    pub(super) fn scan_audio_devices(&mut self, cx: &mut Context<Self>) {
        audio_device::listen();
        self.audio_output.loading = true;
        match audio_device::enumerate() {
            Ok(devices) => {
                if let Some(default) = devices.iter().find(|d| d.is_default) {
                    self.audio_output.current = default.id;
                }
                self.audio_output.devices = devices;
                self.audio_output.error = None;
            }
            Err(error) => {
                self.audio_output.devices.clear();
                self.audio_output.error = Some(error);
            }
        }
        self.audio_output.loading = false;
        cx.notify();
    }

    fn set_output_device(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        if id == self.audio_output.current {
            return;
        }
        match audio_device::set_default(id) {
            Ok(()) => {
                self.audio_output.current = id;
                for device in &mut self.audio_output.devices {
                    device.is_default = device.id == id;
                }
                if self.audio_output.channel_a {
                    self.audio_output.primary = id.to_string();
                } else {
                    self.audio_output.secondary = id.to_string();
                }
                self.persist_audio_output();
                let name = self
                    .audio_output
                    .devices
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| id.to_string());
                window.push_notification(
                    Notification::success(
                        self.t("settings.audioOutput.switchedTo")
                            .replace("{name}", &name),
                    ),
                    cx,
                );
            }
            Err(_) => {
                self.audio_output.error = Some(self.t("settings.audioOutput.switchFailed"));
                window.push_notification(
                    Notification::error(self.t("settings.audioOutput.switchFailed")),
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn persist_audio_output(&mut self) {
        self.app_settings.audio_primary = self.audio_output.primary.clone();
        self.app_settings.audio_secondary = self.audio_output.secondary.clone();
        self.save_preferences();
    }

    /// `toggleAB`: between the two chosen devices, with Alt+O.
    pub(super) fn toggle_audio_ab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let out = &self.audio_output;
        if out.channel_a {
            if !out.secondary.is_empty()
                && out.secondary != out.primary
                && let Ok(id) = out.secondary.parse::<u32>()
            {
                self.audio_output.channel_a = false;
                self.set_output_device(id, window, cx);
                return;
            }
            window.push_notification(
                Notification::info(self.t("settings.audioOutput.setCompareDevice")),
                cx,
            );
        } else if let Ok(id) = out.primary.parse::<u32>() {
            self.audio_output.channel_a = true;
            self.set_output_device(id, window, cx);
        }
    }

    /// Keeps the A/B selects and the volume slider in line with the device list.
    fn sync_audio_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut sorted: Vec<&Device> = self.audio_output.devices.iter().collect();
        sorted.sort_by_cached_key(|d| d.name.to_lowercase());
        let ids: Vec<String> = sorted.iter().map(|d| d.id.to_string()).collect();
        let signature = format!(
            "{}|{}|{}",
            ids.join(","),
            self.audio_output.primary,
            self.audio_output.secondary
        );
        if self.audio_output.signature != signature {
            self.audio_output.signature = signature;
            let items: Vec<Choice> = sorted
                .iter()
                .enumerate()
                .map(|(i, d)| Choice(i, d.name.clone().into()))
                .collect();
            let position = |wanted: &str| ids.iter().position(|id| id == wanted);
            let a = position(&self.audio_output.primary);
            let b = position(&self.audio_output.secondary);
            self.audio_output.select_a.update(cx, |state, cx| {
                state.set_items(items.clone(), window, cx);
                state.set_selected_index(a.map(|i| IndexPath::default().row(i)), window, cx);
            });
            self.audio_output.select_b.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                state.set_selected_index(b.map(|i| IndexPath::default().row(i)), window, cx);
            });
        }
        if let Some(device) = self
            .audio_output
            .devices
            .iter()
            .find(|d| d.id == self.audio_output.current)
        {
            let wanted = (device.volume * 100.).round();
            let slider = self.audio_output.volume.clone();
            if (slider.read(cx).value().start() - wanted).abs() > 0.5 {
                slider.update(cx, |state, cx| state.set_value(wanted, window, cx));
            }
        }
    }

    // ----- play queue ---------------------------------------------------------------------

    fn queue_items(&self) -> Vec<serde_json::Value> {
        let identity = None::<SessionIdentity>;
        self.player
            .queue
            .iter()
            .filter_map(|song| {
                if crate::local_music::is_local(song) {
                    self.local_music
                        .library
                        .as_ref()?
                        .tracks
                        .iter()
                        .find(|track| track.song.id == song.id)
                        .map(|track| track.playlist_item())
                } else if crate::online::is_online(song) {
                    self.online.as_ref()?.item_json(&song.id)
                } else {
                    let identity = identity.as_ref()?;
                    let quality = self.player.queue_qualities.get(&song.id).copied();
                    serde_json::from_str(&crate::library::saved_song_data(identity, song, quality))
                        .ok()
                }
            })
            .collect()
    }

    fn open_queue_dialog(
        &mut self,
        kind: QueueDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let content = cx.new(|_| PlaylistDialog {
            parent,
            colors,
            english,
            kind,
            file: None,
        });
        let wide = matches!(kind, QueueDialog::Import);
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            let limit = if wide { 700. } else { 500. };
            dialog
                .w(px((width - 2. * gutter).min(if narrow {
                    420.
                } else {
                    limit
                })))
                .margin_top(window.viewport_size().height * 0.15)
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(colors, "--td-bg-color-container"))
                .border_color(color(colors, "--td-border-level-1-color"))
                .overlay_tint(color(colors, "--td-mask-active"))
                .close_button(false)
                .on_ok(|_, _, _| false)
                .child(content.clone())
        });
    }

    /// Writes the queue to a `.cmpl` file (`exportPlaylistToFile`).
    fn export_queue_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.queue_items();
        self.export_song_items(items, window, cx);
    }

    pub(super) fn export_song_items(
        &mut self,
        items: Vec<serde_json::Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if items.is_empty() {
            window.push_notification(
                Notification::warning(self.t("settings.playlist.playlistEmpty")),
                cx,
            );
            return;
        }
        let Ok(text) = playlist_file::encrypt(&items) else {
            window.push_notification(
                Notification::error(self.t("settings.playlist.importFailedFormat")),
                cx,
            );
            return;
        };
        let Ok(bytes) = playlist_file::gzip(&text) else {
            return;
        };
        let directory = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Downloads"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let name = playlist_file::default_file_name();
        let selection = cx.prompt_for_new_path(&directory, Some(&name));
        let template = self.t("settings.playlist.exportSuccess");
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = selection.await else {
                return;
            };
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let written = cx
                .background_executor()
                .spawn(async move { std::fs::write(path, bytes) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let note = match written {
                    Ok(()) => Notification::success(template.replace("{name}", &file_name)),
                    Err(_) => Notification::error(this.t("settings.playlist.importFailedFormat")),
                };
                window.push_notification(note, cx);
            });
        })
        .detach();
    }

    /// Copies the encrypted queue text (`copyPlaylistToClipboard`).
    fn export_queue_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.queue_items();
        if items.is_empty() {
            window.push_notification(
                Notification::warning(self.t("settings.playlist.playlistEmptyCopy")),
                cx,
            );
            return;
        }
        match playlist_file::encrypt(&items) {
            Ok(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                window.push_notification(
                    Notification::success(self.t("settings.playlist.clipboardSuccess")),
                    cx,
                );
            }
            Err(_) => window.push_notification(
                Notification::error(self.t("settings.playlist.importFailedFormat")),
                cx,
            ),
        }
    }

    /// `mergePlaylist` plus `validateImportedPlaylist`: songs not yet in the queue are added.
    fn import_queue_items(
        &mut self,
        items: Vec<serde_json::Value>,
        clipboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let valid = !items.is_empty()
            && items.iter().all(|song| {
                let has = |key: &str| song.get(key).is_some_and(|v| !v.is_null());
                let text = |key: &str| song.get(key).is_some_and(serde_json::Value::is_string);
                has("songmid")
                    && song
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                    && text("img")
                    && text("singer")
                    && text("interval")
                    && text("source")
            });
        if !valid {
            let key = if clipboard {
                "settings.playlist.clipboardFormatError"
            } else {
                "settings.playlist.importFailedFormat"
            };
            window.push_notification(Notification::error(self.t(key)), cx);
            return false;
        }
        let total = items.len();
        let identity = None::<SessionIdentity>;
        let mut known: std::collections::HashSet<String> = self
            .player
            .queue
            .iter()
            .map(|song| song.id.clone())
            .collect();
        let mut add = Vec::new();
        let mut online_rows = Vec::new();
        for item in items {
            let raw = match item.get("songmid") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                _ => continue,
            };
            let field = |key: &str| {
                item.get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let row = crate::library::song_from_saved(
                raw,
                field("name"),
                field("singer"),
                field("albumName"),
                &item.to_string(),
            );
            // Online ids carry the source; the same song is not queued twice.
            if !known.insert(row.song.id.clone()) {
                continue;
            }
            if let Some(online) = &row.online {
                if self.online.is_some() {
                    online_rows.push(online.clone());
                    add.push((row.song, row.quality));
                }
            } else if row.playable(identity.as_ref())
                && (!crate::local_music::is_local(&row.song)
                    || self
                        .local_music
                        .library
                        .as_ref()
                        .and_then(|l| l.path(&row.song.id))
                        .is_some())
            {
                add.push((row.song, row.quality));
            }
        }
        if !online_rows.is_empty()
            && let Some(online) = self.online.clone()
        {
            let keep = self.online_keep_ids();
            online.register(online_rows, &keep);
        }
        let songs: Vec<Song> = add.iter().map(|(song, _)| song.clone()).collect();
        if !songs.is_empty() {
            if songs.iter().any(|s| !crate::online::is_online(s))
                && self.player.queue.iter().all(crate::online::is_online)
            {
                self.online_queue_identity = identity;
            }
            self.player.append_to_queue(songs);
            for (song, quality) in add {
                if let Some(quality) = quality {
                    self.player.queue_qualities.insert(song.id, quality);
                }
            }
            self.save_session();
        }
        window.push_notification(
            Notification::success(
                self.t("settings.playlist.importPlaylistSuccess")
                    .replace("{count}", &total.to_string()),
            ),
            cx,
        );
        cx.notify();
        true
    }

    fn clear_queue_confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_player_queue();
        self.save_session();
        window.push_notification(
            Notification::success(self.t("settings.playlist.clearSuccess")),
            cx,
        );
        cx.notify();
    }

    // ----- groups -------------------------------------------------------------------------

    pub(super) fn playback_groups(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if self.audio_output.dirty {
            self.audio_output.dirty = false;
            self.scan_audio_devices(cx);
        }
        self.sync_audio_controls(window, cx);
        vec![
            self.queue_group(narrow, cx),
            self.audio_output_group(narrow, cx),
            self.dlna_group(narrow, cx),
            self.setting_group("playback-background", None, narrow)
                .child(self.background_settings(narrow, cx))
                .into_any_element(),
            self.performance_group(narrow, cx),
        ]
    }

    fn queue_group(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let queue = &self.player.queue;
        let total_seconds: u64 = queue.iter().map(|s| s.duration).sum();
        let artists: std::collections::HashSet<String> = queue
            .iter()
            .flat_map(|s| {
                s.artist
                    .split(['/', '、', '&'])
                    .map(|a| a.trim().to_owned())
                    .filter(|a| !a.is_empty())
                    .collect::<Vec<_>>()
            })
            .collect();
        let duration = {
            let hours = total_seconds / 3600;
            let minutes = (total_seconds % 3600) / 60;
            let seconds = total_seconds % 60;
            if hours > 0 {
                self.t("settings.playlist.formatDuration")
                    .replace("{hours}", &hours.to_string())
                    .replace("{minutes}", &minutes.to_string())
            } else {
                self.t("settings.playlist.formatDurationShort")
                    .replace("{minutes}", &minutes.to_string())
                    .replace("{seconds}", &seconds.to_string())
            }
        };
        let stat = |label: String, value: String| {
            div()
                .flex_1()
                .min_w(px(140.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(12.)).text_color(secondary).child(label))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(primary)
                        .child(value),
                )
        };
        let card = |title: String| {
            div()
                .mt(px(12.))
                .p(px(16.))
                .rounded(px(8.))
                .border_1()
                .border_color(color(mode, "--settings-feature-border"))
                .bg(color(mode, "--settings-feature-bg"))
                .child(
                    div()
                        .mb(px(12.))
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(primary)
                        .child(title),
                )
        };
        let stats = card(self.t("settings.playlist.stats")).child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .child(stat(
                    self.t("settings.playlist.songCount"),
                    self.t("settings.playlist.songUnit")
                        .replace("{count}", &queue.len().to_string()),
                ))
                .child(stat(self.t("settings.playlist.totalDuration"), duration))
                .child(stat(
                    self.t("settings.playlist.artistCount"),
                    self.t("settings.playlist.artistUnit")
                        .replace("{count}", &artists.len().to_string()),
                )),
        );
        let actions = card(self.t("settings.playlist.management"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .child(
                        Button::new("queue-export")
                            .label(self.t("settings.playlist.exportPlaylist"))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_queue_dialog(QueueDialog::Export, window, cx)
                            })),
                    )
                    .child(
                        Button::new("queue-import")
                            .label(self.t("settings.playlist.importPlaylist"))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_queue_dialog(QueueDialog::Import, window, cx)
                            })),
                    )
                    .child(
                        Button::new("queue-clear")
                            .label(self.t("settings.playlist.clearPlaylist"))
                            .danger()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_queue_dialog(QueueDialog::Clear, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .mt(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.t("settings.playlist.featureDescription")),
                    )
                    .children(
                        [
                            "settings.playlist.exportDesc",
                            "settings.playlist.importDesc",
                            "settings.playlist.clearDesc",
                        ]
                        .map(|key| {
                            div()
                                .text_size(px(13.))
                                .text_color(secondary)
                                .child(format!("• {}", self.t(key)))
                        }),
                    ),
            );
        self.setting_group(
            "playback-playlist",
            Some(self.t("settings.playback.playlistManagement")),
            narrow,
        )
        .child(stats)
        .child(actions)
        .into_any_element()
    }

    fn audio_output_group(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let out = &self.audio_output;
        let mut sorted: Vec<&Device> = out.devices.iter().collect();
        sorted.sort_by_cached_key(|d| d.name.to_lowercase());
        let mut list = div().flex().flex_col().gap(px(12.));
        for device in sorted {
            let active = device.id == out.current;
            let id = device.id;
            let rate = if device.sample_rate >= 1000. {
                format!("{:.1}kHz", device.sample_rate / 1000.)
            } else {
                format!("{}Hz", device.sample_rate)
            };
            let tag = |text: String| {
                div()
                    .px(px(8.))
                    .py(px(1.))
                    .rounded(px(3.))
                    .text_size(px(12.))
                    .bg(color(mode, "--td-bg-color-secondarycontainer"))
                    .text_color(secondary)
                    .child(text)
            };
            let mut item = div()
                .id(SharedString::from(format!("audio-device-{id}")))
                .flex()
                .flex_col()
                .gap(px(8.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(8.))
                .border_1()
                .border_color(if active {
                    brand
                } else {
                    color(mode, "--td-component-border")
                })
                .bg(if active {
                    color(mode, "--td-brand-color-1")
                } else {
                    color(mode, "--td-bg-color-container")
                })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, _, window, cx| this.set_output_device(id, window, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.))
                        .flex_wrap()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(device.name.clone()),
                                )
                                .when(device.is_default, |row| {
                                    row.child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(brand)
                                            .child(self.t("settings.audioOutput.currentUsing")),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .when(device.sample_rate > 0., |row| row.child(tag(rate)))
                                .when(device.channels > 0, |row| {
                                    row.child(tag(self
                                        .t("settings.audioOutput.channel")
                                        .replace("{count}", &device.channels.to_string())))
                                }),
                        ),
                );
            if active && device.volume_supported {
                item = item.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().text_size(px(14.)).child("🔊"))
                        .child(div().flex_1().child(crate::skin::colored_playback_slider(
                            &out.volume,
                            &out.volume_pressed,
                            false,
                            mode,
                            cx,
                            None,
                        )))
                        .child(
                            div()
                                .w(px(40.))
                                .text_size(px(12.))
                                .text_color(secondary)
                                .child(format!("{}%", (device.volume * 100.).round() as i32)),
                        ),
                );
            }
            if active {
                item = item.child(
                    div().flex().justify_end().child(
                        Button::new(SharedString::from(format!("audio-test-{id}")))
                            .label(self.t("settings.audioOutput.playTestSound"))
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if audio_device::play_test_sound().is_err() {
                                    window.push_notification(
                                        Notification::error(
                                            this.t("settings.audioOutput.testPlayFailed"),
                                        ),
                                        cx,
                                    );
                                }
                            })),
                    ),
                );
            }
            list = list.child(item);
        }
        if let Some(error) = &out.error {
            list = list.child(
                div()
                    .text_size(px(13.))
                    .text_color(color(mode, "--td-error-color"))
                    .child(error.clone()),
            );
        }
        if out.devices.is_empty() && !out.loading {
            list = list.child(
                div()
                    .py(px(12.))
                    .text_size(px(13.))
                    .text_color(secondary)
                    .child(self.t("settings.audioOutput.noAudioDevice")),
            );
        }
        let channel = if out.channel_a { "A" } else { "B" };
        let select =
            |label: String, state: &Entity<SelectState<Vec<Choice>>>, placeholder: String| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .when(narrow, |row| row.flex_col().items_start())
                    .child(
                        div()
                            .w(px(160.))
                            .flex_shrink_0()
                            .text_size(px(14.))
                            .text_color(secondary)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .w_full()
                            .child(Select::new(state).placeholder(placeholder).w_full()),
                    )
            };
        let ab = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(primary)
                    .child(self.t("settings.audioOutput.abCompareMode")),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(secondary)
                    .child(self.t("settings.audioOutput.abCompareTip")),
            )
            .child(select(
                self.t("settings.audioOutput.deviceAPrimary"),
                &out.select_a,
                self.t("settings.audioOutput.selectDeviceA"),
            ))
            .child(select(
                self.t("settings.audioOutput.deviceBSecondary"),
                &out.select_b,
                self.t("settings.audioOutput.selectDeviceB"),
            ))
            .child(
                Button::new("audio-ab-toggle")
                    .label(format!(
                        "{}  Alt+O",
                        self.t("settings.audioOutput.switchAB")
                            .replace("{channel}", channel)
                    ))
                    .primary()
                    .outline()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_audio_ab(window, cx))),
            );
        self.setting_group(
            "playback-audio-output",
            Some(self.t("settings.playback.audioOutput")),
            narrow,
        )
        .child(
            div().flex().justify_end().mb(px(8.)).child(
                Button::new("audio-refresh")
                    .label(if self.audio_output.loading {
                        "…"
                    } else {
                        "↻"
                    })
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.scan_audio_devices(cx))),
            ),
        )
        .child(list)
        .child(
            div()
                .w_full()
                .h(px(1.))
                .my(px(20.))
                .bg(color(mode, "--td-border-level-1-color")),
        )
        .child(ab)
        .into_any_element()
    }

    fn performance_group(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let item = |id: &'static str,
                    title_key: &str,
                    desc_key: &str,
                    checked: bool,
                    on_change: fn(&mut Mio, bool)| {
            let this = &*self;
            this.setting_item(
                id,
                this.t(title_key),
                Some(this.t(desc_key)),
                Switch::new(SharedString::from(format!("{id}-switch")))
                    .accessibility_label(this.t(title_key))
                    .checked(checked)
                    .on_click(cx.listener(move |mio, value: &bool, _, cx| {
                        on_change(mio, *value);
                        mio.save_preferences();
                        cx.notify();
                    }))
                    .into_any_element(),
                narrow,
            )
        };
        let app = self.app_settings.clone();
        self.setting_group(
            "playback-performance",
            Some(self.t("settings.playback.fullscreenPerformance")),
            narrow,
        )
        .child(item(
            "perf-jump-lyric",
            "settings.playback.jumpLyric",
            "settings.playback.jumpLyricDesc",
            app.jump_lyric,
            |mio, v| mio.app_settings.jump_lyric = v,
        ))
        .child(item(
            "perf-bg-animation",
            "settings.playback.bgAnimation",
            "settings.playback.bgAnimationDesc",
            app.bg_playing,
            |mio, v| mio.app_settings.bg_playing = v,
        ))
        .child(item(
            "perf-visualizer",
            "settings.playback.audioVisualizer",
            "settings.playback.audioVisualizerDesc",
            app.audio_visualizer,
            |mio, v| mio.app_settings.audio_visualizer = v,
        ))
        .child(item(
            "playback-route-preload",
            "settings.playback.routePreload",
            "settings.playback.routePreloadDesc",
            app.route_preload,
            |mio, v| mio.app_settings.route_preload = v,
        ))
        .into_any_element()
    }
}

impl PlaylistDialog {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("settings.playlist.selectFile").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = selection.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            // Only `.cmpl` and `.cpl` are accepted, up to 64 MiB.
            if extension != "cmpl" && extension != "cpl" {
                return;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bytes = cx
                .background_executor()
                .spawn(async move {
                    std::fs::read(&path)
                        .ok()
                        .filter(|b| b.len() <= 64 * 1024 * 1024)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.file = bytes.map(|b| (name, b));
                cx.notify();
            });
        })
        .detach();
    }

    fn import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((name, bytes)) = self.file.clone() else {
            window.push_notification(
                Notification::warning(self.t("settings.playlist.selectFileFirst")),
                cx,
            );
            return;
        };
        let extension = name.rsplit('.').next().unwrap_or("").to_owned();
        let result = playlist_file::read_file(&bytes, &extension);
        let closed = self
            .parent
            .update(cx, |parent, cx| match result {
                Ok(items) => parent.import_queue_items(items, false, window, cx),
                Err(_) => {
                    window.push_notification(
                        Notification::error(parent.t("settings.playlist.importFailedFormat")),
                        cx,
                    );
                    false
                }
            })
            .unwrap_or(false);
        if closed {
            window.close_dialog(cx);
        }
    }

    fn import_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default();
        let result = playlist_file::decrypt(&text);
        let closed = self
            .parent
            .update(cx, |parent, cx| match result {
                Ok(items) => parent.import_queue_items(items, true, window, cx),
                Err(_) => {
                    window.push_notification(
                        Notification::error(parent.t("settings.playlist.importFailedFormat")),
                        cx,
                    );
                    false
                }
            })
            .unwrap_or(false);
        if closed {
            window.close_dialog(cx);
        }
    }
}

impl Render for PlaylistDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let card = |title: String, description: String| {
            div()
                .p(px(16.))
                .rounded(px(8.))
                .border_1()
                .border_color(color(self.colors, "--td-border-level-1-color"))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(primary)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(secondary)
                        .child(description),
                )
        };
        let mut body = div().flex().flex_col().gap(px(12.));
        let title = match self.kind {
            QueueDialog::Export => {
                body = body
                    .child(
                        div()
                            .text_color(secondary)
                            .child(self.t("settings.playlist.selectExportMethod")),
                    )
                    .child(
                        card(
                            self.t("settings.playlist.exportToFile"),
                            self.t("settings.playlist.exportToFileDesc"),
                        )
                        .id("queue-export-file")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            window.close_dialog(cx);
                            let _ = this
                                .parent
                                .update(cx, |p, cx| p.export_queue_file(window, cx));
                        })),
                    )
                    .child(
                        card(
                            self.t("settings.playlist.copyToClipboard"),
                            self.t("settings.playlist.copyToClipboardDesc"),
                        )
                        .id("queue-export-clipboard")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            window.close_dialog(cx);
                            let _ = this
                                .parent
                                .update(cx, |p, cx| p.export_queue_clipboard(window, cx));
                        })),
                    );
                self.t("settings.playlist.exportDialogTitle")
            }
            QueueDialog::Import => {
                let selected = self.file.as_ref().map(|(name, _)| name.clone());
                body = body
                    .child(
                        div()
                            .text_color(secondary)
                            .child(self.t("settings.playlist.selectImportMethod")),
                    )
                    .child(
                        card(
                            self.t("settings.playlist.importFromFile"),
                            self.t("settings.playlist.importFromFileDesc"),
                        )
                        .child(
                            div()
                                .mt(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(
                                    Button::new("queue-pick-file")
                                        .label(self.t("settings.playlist.selectFile"))
                                        .primary()
                                        .outline()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.choose_file(window, cx)
                                        })),
                                )
                                .when_some(selected.clone(), |row, name| {
                                    row.child(
                                        div().text_size(px(13.)).text_color(secondary).child(
                                            self.t("settings.playlist.selected")
                                                .replace("{name}", &name),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new("queue-import-file")
                                        .label(self.t("settings.playlist.doImport"))
                                        .primary()
                                        .disabled(selected.is_none())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.import_file(window, cx)
                                        })),
                                ),
                        ),
                    )
                    .child(
                        card(
                            self.t("settings.playlist.importFromClipboard"),
                            self.t("settings.playlist.importFromClipboardDesc"),
                        )
                        .child(
                            div().mt(px(8.)).child(
                                Button::new("queue-import-clipboard")
                                    .label(self.t("settings.playlist.fromClipboardImport"))
                                    .primary()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.import_clipboard(window, cx)
                                    })),
                            ),
                        ),
                    );
                self.t("settings.playlist.importDialogTitle")
            }
            QueueDialog::Clear => {
                body = body.child(
                    div()
                        .text_color(secondary)
                        .child(self.t("settings.playlist.confirmClearBody")),
                );
                self.t("settings.playlist.confirmClearTitle")
            }
        };
        let confirm_clear = matches!(self.kind, QueueDialog::Clear);
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().mt(px(16.)).child(body))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(24.))
                    .child(
                        Button::new("queue-dialog-cancel")
                            .label(self.t("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .when(confirm_clear, |row| {
                        row.child(
                            Button::new("queue-dialog-clear")
                                .label(self.t("common.clear"))
                                .danger()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = this
                                        .parent
                                        .update(cx, |p, cx| p.clear_queue_confirmed(window, cx));
                                })),
                        )
                    }),
            )
    }
}
