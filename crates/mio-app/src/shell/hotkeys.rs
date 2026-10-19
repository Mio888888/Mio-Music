//! Global hotkeys and system media keys: the registrations with the operating system, what each
//! action does to the player, the in-app single keys, and the hotkey group of the settings page
//! (`components/Settings/HotkeySettings.vue`) with its recorder dialog.
use super::*;
use crate::hotkeys::{self, ACTIONS, Action};
use crate::media_keys::{self, Command, NowPlaying};
use gpui_kit::component::{WindowExt, notification::Notification, switch::Switch};

const HOTKEY_FILE: &str = "hotkeys.json";
/// The original's in-app handler lets one key press through per 100 ms.
const IN_APP_THROTTLE: Duration = Duration::from_millis(100);
/// `seekDelta` / `volumeDelta` of the original.
const STEP: f64 = 5.;

/// The recorder dialog's content (`recording` of the original: preview, captured text, tips).
struct HotkeyRecorder {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    action: Action,
    /// What the keys held right now would be (`recording.preview`).
    preview: String,
    /// The accepted accelerator (`recording.captured`); empty clears the binding.
    captured: String,
    focus: FocusHandle,
}

impl Mio {
    /// Loads the saved configuration, registers it with the system and starts listening to the
    /// hotkeys and the media keys. Called once for the real application.
    pub(super) fn start_hotkeys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(config) = self
            .store
            .as_ref()
            .and_then(|store| store.load::<hotkeys::Config>(HOTKEY_FILE).ok().flatten())
        {
            self.hotkey_config = config;
        }
        match hotkeys::Registry::new() {
            Ok(mut registry) => {
                self.hotkey_failed = registry.apply(&self.hotkey_config).failed;
                self.hotkey_registry = Some(registry);
            }
            Err(error) => eprintln!("Unable to start global hotkeys: {error}"),
        }
        match media_keys::MediaControl::new(crate::platform::window_handle(window)) {
            Ok(control) => self.media_control = Some(control),
            Err(error) => eprintln!("Unable to start media keys: {error}"),
        }
        if self.hotkey_registry.is_none() && self.media_control.is_none() {
            return;
        }
        // Weak entity: the loop ends with the window.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                if update_main(&this, cx, |this, window, cx| {
                    let actions = this
                        .hotkey_registry
                        .as_ref()
                        .map(|registry| registry.poll())
                        .unwrap_or_default();
                    for action in actions {
                        this.run_hotkey_action(action, window, cx);
                    }
                    for command in media_keys::drain() {
                        this.run_media_command(command, window, cx);
                    }
                    this.publish_now_playing();
                })
                .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// `globalControls` of the original, for what `hotkey-triggered` carries.
    fn run_hotkey_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Toggle => self.toggle_play(cx),
            Action::PlayPrev => self.step_track(false, cx),
            Action::PlayNext => self.step_track(true, cx),
            Action::SeekBackward => self.seek_by(-STEP, window, cx),
            Action::SeekForward => self.seek_by(STEP, window, cx),
            Action::VolumeDown => self.volume_by(-STEP, window, cx),
            Action::VolumeUp => self.volume_by(STEP, window, cx),
            Action::ToggleDesktopLyric => self.toggle_desktop_lyric(window.scale_factor(), cx),
            // The original registers and delivers these three, but its player has no handler for
            // them: pressing them changes nothing.
            Action::SetPlayModeSequence
            | Action::SetPlayModeRandom
            | Action::TogglePlayModeSingle => {}
        }
    }

    fn run_media_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        // Even a no-op Pause while recognition has paused playback is user intent.
        self.suppress_recognition_resume();
        match command {
            Command::Toggle => self.toggle_play(cx),
            Command::Play => {
                if self.player.status != Status::Playing {
                    self.toggle_play(cx);
                }
            }
            Command::Pause => {
                if self.player.status == Status::Playing {
                    self.toggle_play(cx);
                }
            }
            Command::Next => self.step_track(true, cx),
            Command::Previous => self.step_track(false, cx),
            Command::Seek(seconds) => self.seek_to(seconds, window, cx),
            Command::SeekBy(delta) => self.seek_by(delta, window, cx),
        }
    }

    /// `seekDelta`: relative to the position, kept inside the track.
    fn seek_by(&mut self, delta: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.seek_to(self.player.position + delta, window, cx);
    }

    pub(super) fn seek_to(&mut self, seconds: f64, window: &mut Window, cx: &mut Context<Self>) {
        if self.player.song().is_none() || !seconds.is_finite() {
            return;
        }
        self.suppress_recognition_resume();
        let upper = if self.player.duration > 0. {
            self.player.duration
        } else {
            seconds.max(0.)
        };
        self.player.seek(seconds.clamp(0., upper));
        self.save_session();
        self.update_progress(window, cx);
        cx.notify();
    }

    /// `volumeDelta`: percent points, 0 to 100.
    fn volume_by(&mut self, delta: f64, window: &mut Window, cx: &mut Context<Self>) {
        let percent = (f64::from(self.player.volume) * 100. + delta)
            .clamp(0., 100.)
            .round();
        self.set_volume_percent(percent as f32, window, cx);
    }

    /// Tells the system what is playing, when the track, state or a jump in position changed.
    fn publish_now_playing(&mut self) {
        let Some(control) = &self.media_control else {
            return;
        };
        let info = self.player.song().map(|song| NowPlaying {
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            duration: if self.player.duration > 0. {
                self.player.duration
            } else {
                song.duration as f64
            },
            position: self.player.position,
            playing: self.player.status == Status::Playing,
            cover: self.cover.clone(),
        });
        // The system runs the clock while playing; only a different track, state or a seek
        // (position off the extrapolated one) needs a new entry.
        let key = info.as_ref().map(|i| {
            (
                i.title.clone(),
                i.artist.clone(),
                i.album.clone(),
                i.duration.round() as i64,
                i.playing,
                i.cover.as_ref().map(|cover| cover.id),
            )
        });
        let now = Instant::now();
        let expected = self.media_published.as_ref().map(|(_, at, position)| {
            if key.as_ref().is_some_and(|k| k.4) {
                position + now.duration_since(*at).as_secs_f64()
            } else {
                *position
            }
        });
        let moved = match (&info, expected) {
            (Some(info), Some(expected)) => (info.position - expected).abs() > 2.,
            _ => false,
        };
        if self.media_published.as_ref().map(|(k, _, _)| k) == Some(&key) && !moved {
            return;
        }
        control.set_now_playing(info.as_ref());
        self.media_published = Some((key, now, info.map(|i| i.position).unwrap_or(0.)));
    }

    /// The single keys of the original's document-level handler (`globalControls.ts`): Space,
    /// the arrows and F, unless a text field has the focus. Returns whether it was one.
    pub(super) fn in_app_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return false;
        }
        if window.text_input_active() || window.has_active_dialog(cx) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if !matches!(key, "space" | "up" | "down" | "left" | "right" | "f") {
            return false;
        }
        // Only the playing page reacts; the original's player lives behind the same routes.
        if self.context_actions.is_some() {
            return false;
        }
        let now = Instant::now();
        if now.duration_since(self.in_app_key_at) < IN_APP_THROTTLE {
            return true;
        }
        self.in_app_key_at = now;
        match key {
            "space" => self.toggle_play(cx),
            "up" => self.volume_by(STEP, window, cx),
            "down" => self.volume_by(-STEP, window, cx),
            "left" => self.seek_by(-STEP, window, cx),
            "right" => self.seek_by(STEP, window, cx),
            _ => self.toggle_full_player(window, cx),
        }
        true
    }

    /// Registers the configuration again and persists it (`hotkeys__set`).
    fn commit_hotkeys(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(registry) = &mut self.hotkey_registry {
            self.hotkey_failed = registry.apply(&self.hotkey_config).failed;
        }
        let saved = match &self.store {
            Some(store) => store.save(HOTKEY_FILE, &self.hotkey_config).is_ok(),
            None => true,
        };
        if !saved {
            eprintln!("Unable to save the hotkeys");
        }
        cx.notify();
        saved
    }

    fn save_hotkeys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let notification = if self.commit_hotkeys(cx) {
            Notification::success(self.t("settings.hotkey.saveSuccess"))
        } else {
            Notification::error(self.t("settings.hotkey.saveFailed"))
        };
        window.push_notification(notification, cx);
    }

    fn open_hotkey_recorder(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        // The system would swallow the keys that are bound right now; let the recorder see them.
        if let Some(registry) = &mut self.hotkey_registry {
            registry.clear();
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let captured = self.hotkey_config.binding(action).to_owned();
        let recorder = cx.new(|cx| {
            let focus = cx.focus_handle();
            cx.on_release({
                let parent = parent.clone();
                move |_, cx| {
                    // Closed by any route: the registrations come back.
                    let _ = parent.update(cx, |this, cx| this.commit_hotkeys(cx));
                }
            })
            .detach();
            HotkeyRecorder {
                parent: parent.clone(),
                colors,
                english,
                action,
                preview: String::new(),
                captured,
                focus,
            }
        });
        let focus = recorder.read(cx).focus.clone();
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            dialog
                .w(px((width - 2. * gutter).min(if narrow {
                    420.
                } else {
                    480.
                })))
                .margin_top(window.viewport_size().height * 0.2)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(
                    colors,
                    if narrow {
                        "--mobile-glass-bg-strong"
                    } else {
                        "--td-bg-color-container"
                    },
                ))
                .border_color(color(
                    colors,
                    if narrow {
                        "--mobile-glass-border"
                    } else {
                        "--td-border-level-1-color"
                    },
                ))
                .overlay_tint(color(
                    colors,
                    if narrow {
                        "--mobile-scrim"
                    } else {
                        "--td-mask-active"
                    },
                ))
                // `:close-on-esc-keydown="false"` and `:close-on-overlay-click="false"`.
                .keyboard(false)
                .overlay_closable(false)
                .close_button(false)
                .on_ok(|_, _, _| false)
                .child(recorder.clone())
        });
        focus.focus(window, cx);
    }

    pub(super) fn hotkey_groups(
        &mut self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let enabled = self.hotkey_config.enabled;
        let secondary = color(mode, "--td-text-color-secondary");
        let primary = color(mode, "--td-text-color-primary");
        let title = |text: String| {
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(primary)
                .child(text)
        };

        let mut rows = div()
            .mt(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(!enabled, |list| list.opacity(0.6))
            .child(
                div().flex().justify_end().child(
                    Button::new("hotkey-restore-default")
                        .label(self.t("settings.hotkey.restoreDefault"))
                        .outline()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.hotkey_config = hotkeys::Config::default();
                            this.save_hotkeys(window, cx);
                        })),
                ),
            );
        for action in ACTIONS {
            let failed = self.hotkey_failed.contains(&action);
            let binding = self.hotkey_config.binding(action);
            let shown = if binding.is_empty() {
                self.t("common.notSet")
            } else {
                hotkeys::display(binding)
            };
            let (tag_bg, tag_fg) = if failed {
                (
                    color(mode, "--td-error-color-1"),
                    color(mode, "--td-error-color"),
                )
            } else {
                (
                    color(mode, "--td-brand-color-1"),
                    color(mode, "--td-brand-color"),
                )
            };
            let stem = action.locale_stem();
            let id = action.id();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("hotkey-row-{id}")))
                    .flex()
                    .when(narrow, |row| row.flex_col().items_start())
                    .when(!narrow, |row| row.items_center().justify_between())
                    .gap(px(12.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(color(mode, "--settings-feature-border"))
                    .bg(color(mode, "--settings-feature-bg"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(self.t(&format!("settings.hotkey.{stem}"))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(self.t(&format!("settings.hotkey.{stem}Desc"))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .gap(px(8.))
                            .child(
                                div()
                                    .px(px(8.))
                                    .h(px(24.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(3.))
                                    .bg(tag_bg)
                                    .text_color(tag_fg)
                                    .text_size(px(12.))
                                    .child(shown),
                            )
                            .child(
                                Button::new(SharedString::from(format!("hotkey-set-{id}")))
                                    .label(self.t("settings.hotkey.set"))
                                    .outline()
                                    .disabled(!enabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_hotkey_recorder(action, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("hotkey-clear-{id}")))
                                    .label(self.t("settings.hotkey.clear"))
                                    .ghost()
                                    .disabled(!enabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.hotkey_config.set_binding(action, "");
                                        this.save_hotkeys(window, cx);
                                    })),
                            ),
                    ),
            );
        }

        let in_app = [
            ("Space", "inAppToggle"),
            ("↑", "inAppVolumeUp"),
            ("↓", "inAppVolumeDown"),
            ("←", "inAppSeekBackward"),
            ("→", "inAppSeekForward"),
            ("F", "inAppFullscreen"),
        ];
        let mut in_app_list = div().mt(px(12.)).flex().flex_col().gap(px(10.));
        for (keys, stem) in in_app {
            in_app_list = in_app_list.child(
                div()
                    .flex()
                    .when(narrow, |row| row.flex_col().items_start())
                    .when(!narrow, |row| row.items_center().justify_between())
                    .gap(px(12.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(color(mode, "--settings-feature-border"))
                    .bg(color(mode, "--settings-feature-bg"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(self.t(&format!("settings.hotkey.{stem}"))),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(self.t(&format!("settings.hotkey.{stem}Desc"))),
                            ),
                    )
                    .child(
                        div()
                            .px(px(8.))
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .rounded(px(3.))
                            .bg(color(mode, "--td-bg-color-component"))
                            .text_color(primary)
                            .text_size(px(12.))
                            .child(keys),
                    ),
            );
        }

        vec![
            self.setting_group(
                "hotkey-settings",
                Some(self.t("settings.hotkey.title")),
                narrow,
            )
            .child(
                div()
                    .mb(px(24.))
                    .text_size(px(14.))
                    .text_color(secondary)
                    .child(self.t("settings.hotkey.description")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .mb(px(16.))
                    .child(
                        self.setting_item(
                            "hotkey-enabled",
                            self.t("settings.hotkey.enableGlobal"),
                            Some(self.t("settings.hotkey.enableGlobalDesc")),
                            Switch::new("hotkey-enabled-switch")
                                .accessibility_label(self.t("settings.hotkey.enableGlobal"))
                                .checked(enabled)
                                .on_click(cx.listener(|this, checked: &bool, window, cx| {
                                    this.hotkey_config.enabled = *checked;
                                    this.save_hotkeys(window, cx);
                                }))
                                .into_any_element(),
                            narrow,
                        )
                        .mt(px(0.)),
                    )
                    .child(rows),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(title(self.t("settings.hotkey.inAppTitle")))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(secondary)
                            .child(self.t("settings.hotkey.inAppDesc")),
                    )
                    .child(in_app_list),
            )
            .into_any_element(),
        ]
    }
}

impl HotkeyRecorder {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn set_preview(&mut self, preview: String, cx: &mut Context<Self>) {
        if self.preview != preview {
            self.preview = preview;
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => {
                window.close_dialog(cx);
                return;
            }
            "backspace" | "delete" => {
                self.captured.clear();
                cx.notify();
                return;
            }
            _ => {}
        }
        let modifiers = event.keystroke.modifiers;
        let preview = hotkeys::normalize(
            modifiers.control || modifiers.platform,
            modifiers.alt,
            modifiers.shift,
            Some(key),
        );
        self.set_preview(preview.clone(), cx);
        if hotkeys::is_complete(&preview) {
            let conflict = self
                .parent
                .read_with(cx, |parent, _| {
                    parent
                        .hotkey_config
                        .conflict(self.action, &preview)
                        .is_some()
                })
                .unwrap_or(false);
            if conflict {
                window.push_notification(
                    Notification::warning(self.t("settings.hotkey.hotkeyConflict")),
                    cx,
                );
                return;
            }
            self.captured = preview;
            cx.notify();
        }
    }

    fn modifiers_changed(&mut self, event: &ModifiersChangedEvent, cx: &mut Context<Self>) {
        let m = event.modifiers;
        let preview = hotkeys::normalize(m.control || m.platform, m.alt, m.shift, None);
        self.set_preview(preview, cx);
    }

    fn can_save(&self) -> bool {
        self.captured.is_empty() || hotkeys::is_complete(&self.captured)
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_save() {
            return;
        }
        let action = self.action;
        let captured = self.captured.clone();
        window.close_dialog(cx);
        let _ = self.parent.update(cx, |parent, cx| {
            parent.hotkey_config.set_binding(action, &captured);
            parent.save_hotkeys(window, cx);
        });
    }
}

impl Render for HotkeyRecorder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let name = self.t(&format!("settings.hotkey.{}", self.action.locale_stem()));
        let header = self.t("settings.hotkey.setWith").replace("{name}", &name);
        let preview = if self.preview.is_empty() {
            self.t("settings.hotkey.pressCombo")
        } else {
            hotkeys::display(&self.preview)
        };
        let captured = if self.captured.is_empty() {
            self.t("settings.hotkey.waitForInput")
        } else {
            hotkeys::display(&self.captured)
        };
        let can_save = self.can_save();
        let boxed = |text: String, weight: FontWeight, text_color: Hsla| {
            div()
                .p(px(12.))
                .border_1()
                .border_dashed()
                .border_color(color(colors, "--td-border-level-1-color"))
                .rounded(px(8.))
                .font_weight(weight)
                .text_align(TextAlign::Center)
                .text_color(text_color)
                .child(text)
        };
        div()
            .id("hotkey-recorder")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .on_modifiers_changed(
                cx.listener(|this, event, _, cx| this.modifiers_changed(event, cx)),
            )
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(header),
            )
            .child(
                div()
                    .mt(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(boxed(preview, FontWeight::BOLD, primary))
                    .child(boxed(captured, FontWeight::SEMIBOLD, secondary))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(12.))
                            .text_color(secondary)
                            .child(div().child(self.t("settings.hotkey.pressEscToCancel")))
                            .child(div().child(self.t("settings.hotkey.pressBackspaceToClear"))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(24.))
                    .child(
                        Button::new("hotkey-recorder-cancel")
                            .label(self.t("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("hotkey-recorder-save")
                            .label(self.t("common.save"))
                            .primary()
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
