//! The DLNA group of the playback category (`DlnaDeviceSettings.vue`) and the casting follow-up
//! the original's `PlayMusic.vue` does while a renderer is selected: a new track is handed to the
//! renderer, pause/resume and volume are mirrored, and the local position follows the renderer's.
use super::*;
use crate::dlna::{self, Device};
use gpui_kit::component::{WindowExt, notification::Notification};
use std::time::Duration as StdDuration;

/// Local playback may drift from the renderer by this much before it is corrected.
const DRIFT_SECONDS: f64 = 2.;

#[derive(Default)]
pub(super) struct DlnaUi {
    devices: Vec<Device>,
    /// `currentDevice`: the USN of the selected renderer.
    current: Option<String>,
    searching: bool,
    error: Option<String>,
    /// Bumped on every selection change; late results of an older one are dropped.
    generation: u64,
    /// The song the renderer was last given.
    handed_song: Option<String>,
    handoff_active: bool,
    last_playing: bool,
    last_volume: i32,
    position_pending: bool,
    last_position_at: Option<std::time::Instant>,
    /// Search when the category is first drawn.
    searched: bool,
}

impl Mio {
    fn dlna_fail(&mut self, message: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dlna_ui.current = None;
        self.dlna_ui.error = Some(message.clone());
        self.dlna_ui.generation += 1;
        window.push_notification(Notification::error(message), cx);
        cx.notify();
    }

    pub(super) fn dlna_search(&mut self, cx: &mut Context<Self>) {
        if self.dlna_ui.searching {
            return;
        }
        self.dlna_ui.searching = true;
        self.dlna_ui.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { dlna::search() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dlna_ui.searching = false;
                match result {
                    Ok(devices) => {
                        // The selection stays only while its renderer is still listed.
                        if let Some(current) = &this.dlna_ui.current
                            && !devices.iter().any(|d| &d.usn == current)
                        {
                            this.dlna_ui.current = None;
                        }
                        this.dlna_ui.devices = devices;
                    }
                    Err(error) => this.dlna_ui.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The song's playable address and title, resolved the way playback does (online songs
    /// only; the original has no address for anything else either).
    fn dlna_media(&self) -> Option<(String, String, Arc<crate::online::Online>)> {
        let song = self.player.song()?;
        if !crate::online::is_online(song) {
            return None;
        }
        Some((song.id.clone(), song.title.clone(), self.online.clone()?))
    }

    fn set_local_playing(&mut self, play: bool, cx: &mut Context<Self>) {
        let playing = self.player.status == Status::Playing;
        let paused = self.player.status == Status::Paused;
        if (playing && !play) || (paused && play) {
            self.toggle_play(cx);
        }
    }

    /// `handleDlnaDeviceChange`.
    fn dlna_select(&mut self, usn: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.dlna_ui.current.as_deref() == Some(usn.as_str()) {
            return;
        }
        let Some(device) = self.dlna_ui.devices.iter().find(|d| d.usn == usn).cloned() else {
            return;
        };
        self.dlna_ui.generation += 1;
        let generation = self.dlna_ui.generation;
        let had_previous = self.dlna_ui.current.is_some();
        self.dlna_ui.current = Some(usn.clone());
        self.dlna_ui.error = None;
        self.dlna_ui.handed_song = None;
        let media = self.dlna_media();
        let playing = self.player.status == Status::Playing;
        self.dlna_ui.last_playing = playing;
        self.dlna_ui.last_volume = (self.player.volume * 100.).round() as i32;
        let has_song = self.player.song().is_some();
        let name = device.name.clone();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if had_previous {
                        let _ = dlna::stop();
                    }
                    if has_song {
                        let (id, title, online) = media
                            .ok_or_else(|| "媒体地址必须是无凭据的 HTTP(S) 地址".to_owned())?;
                        let url = online
                            .resolve(&id, Arc::new(std::sync::atomic::AtomicBool::new(false)))?;
                        let url = mio_plugin::runtime_url(&url)?.to_string();
                        dlna::play(&device.location, &url, &title)?;
                        if playing {
                            dlna::resume()?;
                        }
                        return Ok(Some(id));
                    }
                    Ok::<Option<String>, String>(None)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.dlna_ui.generation != generation {
                    return;
                }
                match result {
                    Ok(song) => {
                        this.dlna_ui.handed_song = song;
                        window.push_notification(
                            Notification::success(
                                this.t("settings.dlna.connected").replace("{name}", &name),
                            ),
                            cx,
                        );
                        cx.notify();
                    }
                    Err(message) => this.dlna_fail(message, window, cx),
                }
            });
        })
        .detach();
    }

    fn dlna_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dlna_ui.current.is_none() {
            return;
        }
        self.dlna_ui.generation += 1;
        let generation = self.dlna_ui.generation;
        self.dlna_ui.current = None;
        self.dlna_ui.handed_song = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { dlna::stop() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.dlna_ui.generation != generation {
                    return;
                }
                let note = match result {
                    Ok(()) => Notification::success(this.t("settings.dlna.disconnected")),
                    Err(message) => Notification::error(message),
                };
                window.push_notification(note, cx);
            });
        })
        .detach();
    }

    /// Runs every quarter second: mirrors what the original's watchers do while casting.
    pub(super) fn dlna_follow(&mut self, cx: &mut Context<Self>) {
        let Some(usn) = self.dlna_ui.current.clone() else {
            return;
        };
        if self.dlna_ui.handoff_active {
            return;
        }
        let generation = self.dlna_ui.generation;
        let song = self.player.song().map(|s| s.id.clone());
        // A new track goes to the renderer.
        if song.is_some() && song != self.dlna_ui.handed_song {
            self.dlna_handoff(usn, generation, cx);
            return;
        }
        // Pause and resume follow the local player.
        let playing = self.player.status == Status::Playing;
        if playing != self.dlna_ui.last_playing {
            self.dlna_ui.last_playing = playing;
            let _ = usn;
            self.dlna_background(
                generation,
                move || {
                    if playing {
                        dlna::resume()
                    } else {
                        dlna::pause()
                    }
                },
                cx,
            );
        }
        // The volume follows too.
        let volume = (self.player.volume * 100.).round() as i32;
        if volume != self.dlna_ui.last_volume {
            self.dlna_ui.last_volume = volume;
            self.dlna_background(generation, move || dlna::set_volume(f64::from(volume)), cx);
        }
        // About once a second, the local position is pulled to the renderer's.
        let due = self
            .dlna_ui
            .last_position_at
            .is_none_or(|at| at.elapsed() >= StdDuration::from_secs(1));
        if playing && due && !self.dlna_ui.position_pending {
            self.dlna_ui.position_pending = true;
            self.dlna_ui.last_position_at = Some(std::time::Instant::now());
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { dlna::position() })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.dlna_ui.position_pending = false;
                    if this.dlna_ui.generation != generation {
                        return;
                    }
                    if let Ok(position) = result
                        && this.player.status == Status::Playing
                        && (this.player.position - position.position).abs() > DRIFT_SECONDS
                    {
                        this.player.seek(position.position);
                        this.save_session();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// Runs a control call off the main thread; a failure drops the selection like the
    /// original does (`dlnaStore.selectDevice(null)`).
    fn dlna_background(
        &mut self,
        generation: u64,
        call: impl FnOnce() -> Result<(), String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { call() }).await;
            let _ = this.update(cx, |this, cx| {
                if this.dlna_ui.generation == generation && result.is_err() {
                    this.dlna_ui.current = None;
                    this.dlna_ui.error = result.err();
                    this.dlna_ui.generation += 1;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The `watch(Audio.url)` of the original: pause locally, hand the track over, wait, adopt
    /// the renderer's position, and resume (or pause the renderer) as the user had it.
    fn dlna_handoff(&mut self, usn: String, generation: u64, cx: &mut Context<Self>) {
        let Some(device) = self.dlna_ui.devices.iter().find(|d| d.usn == usn).cloned() else {
            return;
        };
        let Some((id, title, online)) = self.dlna_media() else {
            // Not castable (a Subsonic song): nothing to hand over; the selection stays.
            self.dlna_ui.handed_song = self.player.song().map(|s| s.id.clone());
            return;
        };
        self.dlna_ui.handoff_active = true;
        self.dlna_ui.handed_song = Some(id.clone());
        let resume = self.player.status == Status::Playing || self.dlna_ui.last_playing;
        self.set_local_playing(false, cx);
        cx.spawn(async move |this, cx| {
            let handed = cx
                .background_executor()
                .spawn({
                    let id = id.clone();
                    async move {
                        let url = online
                            .resolve(&id, Arc::new(std::sync::atomic::AtomicBool::new(false)))?;
                        let url = mio_plugin::runtime_url(&url)?.to_string();
                        dlna::play(&device.location, &url, &title)
                    }
                })
                .await;
            if handed.is_ok() {
                cx.background_executor()
                    .timer(StdDuration::from_millis(1500))
                    .await;
            }
            let position = match &handed {
                Ok(()) => {
                    cx.background_executor()
                        .spawn(async move { dlna::position() })
                        .await
                }
                Err(e) => Err(e.clone()),
            };
            let paused_remote = if let (Ok(_), false) = (&position, resume) {
                cx.background_executor()
                    .spawn(async move { dlna::pause() })
                    .await
            } else {
                Ok(())
            };
            let _ = this.update(cx, |this, cx| {
                this.dlna_ui.handoff_active = false;
                if this.dlna_ui.generation != generation {
                    return;
                }
                match (position, paused_remote) {
                    (Ok(position), Ok(())) => {
                        if position.duration > 0. {
                            this.player.duration = position.duration;
                        }
                        this.player.seek(position.position);
                        this.set_local_playing(resume, cx);
                        this.dlna_ui.last_playing = resume;
                        this.save_session();
                    }
                    (Err(message), _) | (_, Err(message)) => {
                        this.dlna_ui.current = None;
                        this.dlna_ui.error = Some(message);
                        this.dlna_ui.generation += 1;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn dlna_group(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        if !self.dlna_ui.searched {
            self.dlna_ui.searched = true;
            self.dlna_search(cx);
        }
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .max_h(px(200.))
            .overflow_y_hidden();
        for device in &self.dlna_ui.devices {
            let active = self.dlna_ui.current.as_deref() == Some(device.usn.as_str());
            let usn = device.usn.clone();
            let mut row =
                div()
                    .id(SharedString::from(format!("dlna-{}", device.usn)))
                    .flex()
                    .items_center()
                    .justify_between()
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dlna_select(usn.clone(), window, cx)
                    }))
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
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(device.address.clone()),
                            ),
                    );
            if active {
                row = row.child(
                    Button::new("dlna-stop")
                        .label(self.t("settings.dlna.stopCast"))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dlna_stop(window, cx)
                        })),
                );
            }
            list = list.child(row);
        }
        if self.dlna_ui.devices.is_empty() && !self.dlna_ui.searching {
            list = list.child(
                div()
                    .py(px(12.))
                    .text_size(px(13.))
                    .text_color(secondary)
                    .child(self.t("settings.dlna.noDevice")),
            );
        }
        if let Some(error) = &self.dlna_ui.error {
            list = list.child(
                div()
                    .text_size(px(13.))
                    .text_color(color(mode, "--td-error-color"))
                    .child(error.clone()),
            );
        }
        self.setting_group("playback-dlna", None, narrow)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.t("settings.dlna.title")),
                    )
                    .child(
                        Button::new("dlna-refresh")
                            .label(if self.dlna_ui.searching { "…" } else { "↻" })
                            .ghost()
                            .disabled(self.dlna_ui.searching)
                            .on_click(cx.listener(|this, _, _, cx| this.dlna_search(cx))),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    /// Starts the quarter-second follow loop (once, real application only).
    pub(super) fn start_dlna_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(StdDuration::from_millis(250))
                    .await;
                if this.update(cx, |this, cx| this.dlna_follow(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}
