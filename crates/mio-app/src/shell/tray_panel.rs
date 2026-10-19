//! The mini player that opens under the tray icon: now playing with transport controls and
//! volume, the progress slider and the play queue with favourites. It is a second, borderless
//! window of the same process that reads the main view directly (like the desktop lyric) and
//! closes as soon as it loses the focus. The tray's own menu (right click) stays as it was.
use super::*;
use gpui_kit::component::Root;

const WIDTH: f32 = 336.;
/// The tallest the panel gets; fewer songs make it shorter, never leaving blank space.
const MAX_HEIGHT: f32 = 436.;
/// Fixed parts: the header with the controls, the queue's title row, the list's bottom
/// padding and the border; and what an empty queue shows in place of rows.
const HEADER: f32 = 146.;
const LIST_TITLE: f32 = 30.;
const LIST_PADDING: f32 = 6.;
const BORDER: f32 = 2.;
const ROW: f32 = 44.;
const EMPTY_BODY: f32 = 56.;

fn panel_height(songs: usize) -> f32 {
    let body = if songs == 0 {
        EMPTY_BODY
    } else {
        songs as f32 * ROW
    };
    (HEADER + LIST_TITLE + body + LIST_PADDING + BORDER).min(MAX_HEIGHT)
}
/// Distance between the icon and the panel, and the margin kept to the screen edges.
const GAP: f32 = 6.;
const MARGIN: f32 = 8.;
/// A click on the tray icon first takes the focus away from the panel, which closes it; the
/// click that follows must not open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(250);
/// Rows drawn at most; a queue can hold thousands of songs.
const MAX_ROWS: usize = 300;

/// What the panel shows of the main view, copied when the view changes.
#[derive(Clone)]
struct Snapshot {
    mode: crate::skin::Colors,
    palette: Option<super::cover_palette::CoverPalette>,
    tint: Hsla,
    song: Option<Song>,
    cover: Option<Arc<Image>>,
    status: Status,
    play_mode: crate::playback::PlayMode,
    position: f64,
    duration: f64,
    /// `(queue index, song, liked, current)`.
    rows: Vec<(usize, Song, bool, bool)>,
    total: usize,
    muted: bool,
    lyric_open: bool,
}

pub struct TrayPanel {
    main: Entity<Mio>,
    was_active: bool,
    /// The panel has been told to go away; later triggers are ignored.
    closed: bool,
    /// The count of clicks in other applications when the panel opened.
    clicks: u64,
    /// The panel's own sliders (a Kit slider keeps the bounds of the window it was painted
    /// in, so the main window's cannot be shared); they feed the main view on release.
    progress: Entity<SliderState>,
    progress_pressed: Rc<Cell<bool>>,
    volume: Entity<SliderState>,
    volume_pressed: Rc<Cell<bool>>,
    height: f32,
    snapshot: Snapshot,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    /// Tray left click: opens the panel under the icon, or closes it when it is already open.
    /// `rect` is the icon's `(x, y, width, height)` in physical pixels.
    pub(super) fn toggle_tray_panel(
        &mut self,
        rect: (i32, i32, u32, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // An open panel is asked to close (it does so from its own window). A handle whose
        // window is already gone must not swallow the click.
        self.tray_panel = self.tray_panel.filter(|handle| {
            cx.windows()
                .iter()
                .any(|w| w.window_id() == handle.window_id())
        });
        if self.tray_panel_opening {
            return;
        }
        if self.tray_panel.is_some() {
            // The panel hears about it through its observer and closes at once.
            self.tray_panel_close = true;
            cx.notify();
            return;
        }
        if self
            .tray_panel_closed
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
        {
            return;
        }
        self.tray_panel_rect = Some(rect);
        let height_px = panel_height(self.player.queue.len());
        let scale = f64::from(window.scale_factor()).max(1.);
        let (x, y, width, height) = rect;
        let center = (f64::from(x) + f64::from(width) / 2.) / scale;
        let bottom = (f64::from(y) + f64::from(height)) / scale;
        let screen = cx
            .primary_display()
            .map(|display| display.bounds().size)
            .unwrap_or_else(|| size(px(1440.), px(900.)));
        let (screen_width, screen_height) = (f32::from(screen.width), f32::from(screen.height));
        let left =
            (center as f32 - WIDTH / 2.).clamp(MARGIN, (screen_width - WIDTH - MARGIN).max(MARGIN));
        // Below the icon (menu bar); above it when the icon sits in a bottom taskbar.
        let top = if bottom as f32 + GAP + height_px + MARGIN <= screen_height {
            bottom as f32 + GAP
        } else {
            (f64::from(y) / scale) as f32 - GAP - height_px
        }
        .max(MARGIN);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(left), px(top)),
                size(px(WIDTH), px(height_px)),
            ))),
            titlebar: None,
            kind: WindowKind::Floating,
            focus: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("com.vant.Mio.Music.GPUI".into()),
            ..Default::default()
        };
        // Opened from the executor, outside this view's update: the new window draws at once
        // and reads this view, which must not still be leased by the click that asked for it.
        let main = cx.entity();
        self.tray_panel_opening = true;
        cx.spawn(async move |this, cx| {
            let opened = cx.update(|cx| {
                cx.open_window(options, move |window, cx| {
                    if let Err(error) = crate::platform::configure_overlay(window) {
                        eprintln!("Unable to configure the tray panel: {error}");
                    }
                    let view = cx.new(|cx| TrayPanel::new(main, window, cx));
                    cx.new(|cx| Root::new(view, window, cx).bg(transparent_black()))
                })
            });
            let _ = this.update(cx, |this, _| {
                this.tray_panel_opening = false;
                if let Ok(handle) = opened {
                    this.tray_panel = Some(handle.into());
                }
            });
        })
        .detach();
    }

    /// The panel is gone. `by_focus`: it closed because the focus moved elsewhere, in which
    /// case the icon click that caused it may still arrive and must not open it again.
    fn tray_panel_closed(&mut self, window: WindowId, by_focus: bool) {
        // A window that is closing late (its deactivation arrives after a newer panel was
        // opened) must not take the newer panel's registration with it.
        if self
            .tray_panel
            .is_some_and(|handle| handle.window_id() != window)
        {
            return;
        }
        self.tray_panel = None;
        self.tray_panel_close = false;
        self.tray_panel_closed = by_focus.then(Instant::now);
    }

    fn tray_snapshot(&self) -> Snapshot {
        let current = self.player.queue_position();
        Snapshot {
            mode: self.ui_colors(),
            palette: self.theme_palette(),
            tint: self.player_bar_tint(true),
            song: self.player.song().cloned(),
            cover: self.cover.clone(),
            status: self.player.status,
            play_mode: self.player.mode,
            position: self.player.position,
            duration: self.player.duration,
            rows: self
                .player
                .queue
                .iter()
                .enumerate()
                .take(MAX_ROWS)
                .map(|(index, song)| {
                    (
                        index,
                        song.clone(),
                        self.song_liked(song),
                        current == Some(index),
                    )
                })
                .collect(),
            total: self.player.queue.len(),
            muted: self.player.volume <= 0.,
            lyric_open: self.desktop_lyric_open(),
        }
    }

    /// Whether the favourites hold the song, the same lookup the heart buttons make.
    pub(super) fn song_liked(&self, song: &Song) -> bool {
        let Some(favorites) = &self.favorites.snapshot else {
            return false;
        };
        if let Some((source, raw)) = &crate::online::parse_song_id(&song.id) {
            return favorites.contains_online(source, raw);
        }
        crate::local_music::is_local(song)
            && favorites.contains(&song.id, &SessionIdentity::local())
    }
}

impl TrayPanel {
    fn new(main: Entity<Mio>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let follow = cx.observe_in(&main, window, |this, main, window, cx| {
            if main.read(cx).tray_panel_close {
                this.close(window, false, cx);
            } else {
                this.snapshot = main.read(cx).tray_snapshot();
                cx.notify();
            }
        });
        // Losing the focus closes the panel, once it has had it.
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                this.close(window, true, cx);
            }
        });
        let (volume_value, height, main_snapshot) = {
            let main = main.read(cx);
            (
                main.player.volume * 100.,
                panel_height(main.player.queue.len()),
                main.tray_snapshot(),
            )
        };
        let progress = cx.new(|_| SliderState::new().min(0.).max(100.).step(0.1));
        let volume = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(volume_value)
        });
        let seek = cx.subscribe_in(&progress, window, |this, _, event, _, cx| {
            if let SliderEvent::Release(value) = event {
                let seconds = f64::from(value.start()) / 100. * this.main.read(cx).player.duration;
                this.act(cx, move |main, window, cx| {
                    main.seek_to(seconds, window, cx)
                });
            }
        });
        let level = cx.subscribe_in(&volume, window, |this, _, event, _, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            let percent = value.start();
            this.act(cx, move |main, window, cx| {
                main.set_volume_percent(percent, window, cx)
            });
        });
        // Every 100 ms: closes on a request or a click in another application, follows the
        // main view's position and volume, and reopens at another height when the queue's
        // length changed (a window cannot be resized without its top edge moving).
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let alive = this.update_in(cx, |this, window, cx| {
                    let requested = this.main.read(cx).tray_panel_close;
                    let close = requested || crate::platform::outside_clicks() != this.clicks;
                    if close {
                        this.close(window, !requested, cx);
                    } else {
                        let (position, duration, volume, songs, playing) = {
                            let main = this.main.read(cx);
                            (
                                main.player.position,
                                main.player.duration,
                                main.player.volume,
                                main.player.queue.len(),
                                main.player.status == Status::Playing,
                            )
                        };
                        if !this.progress_pressed.get() {
                            let percent = if duration > 0. {
                                (position / duration * 100.).clamp(0., 100.) as f32
                            } else {
                                0.
                            };
                            let shown = this.progress.read(cx).percentage().end * 100.;
                            if (shown - percent).abs() > 0.05 {
                                this.progress
                                    .update(cx, |state, cx| state.set_value(percent, window, cx));
                            }
                        }
                        if !this.volume_pressed.get() {
                            let percent = (volume * 100.).round();
                            let shown = (this.volume.read(cx).percentage().end * 100.).round();
                            if shown != percent {
                                this.volume
                                    .update(cx, |state, cx| state.set_value(percent, window, cx));
                            }
                        }
                        if (panel_height(songs) - this.height).abs() > 0.5 {
                            this.close(window, false, cx);
                            this.act(cx, |main, window, cx| {
                                if let Some(rect) = main.tray_panel_rect {
                                    main.toggle_tray_panel(rect, window, cx);
                                }
                            });
                        } else if playing {
                            this.snapshot = this.main.read(cx).tray_snapshot();
                            cx.notify();
                        }
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            main,
            was_active: false,
            closed: false,
            clicks: crate::platform::outside_clicks(),
            progress,
            progress_pressed: Rc::new(Cell::new(false)),
            volume,
            volume_pressed: Rc::new(Cell::new(false)),
            height,
            snapshot: main_snapshot,
            _subscriptions: vec![follow, activation, seek, level],
        }
    }

    /// Closes this panel once, however many of the triggers fire for the same closing.
    fn close(&mut self, window: &mut Window, by_focus: bool, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        let id = window.window_handle().window_id();
        self.main
            .update(cx, |main, _| main.tray_panel_closed(id, by_focus));
        window.remove_window();
    }

    /// Runs `action` on the main view with the main window, as its own controls would.
    fn act(
        &self,
        cx: &mut App,
        action: impl FnOnce(&mut Mio, &mut Window, &mut Context<Mio>) + 'static,
    ) {
        let Some(handle) = cx.try_global::<crate::MainWindow>().map(|window| window.0) else {
            return;
        };
        let main = self.main.clone();
        let _ = handle.update(cx, move |_, window, cx| {
            main.update(cx, |main, cx| action(main, window, cx))
        });
    }
}

/// The glyph set of the main window's player bar (`colored_control`).
fn original(name: &str) -> Option<&'static str> {
    Some(match name {
        "SkipPrev" => "shangyishou",
        "SkipNext" => "xiayishou",
        "PlayFilled" => "bofang",
        "Pause" => "zanting",
        "Repeat" => "shunxubofangtubiao",
        "RepeatSong" => "bofang-xunhuanbofang",
        "Shuffle" => "suijibofang",
        "VolumeUp" | "VolumeMute" => "shengyin",
        _ => return None,
    })
}

/// A round icon button; sizes are explicit, colours come from the player bar's rules.
struct Tool {
    id: &'static str,
    name: &'static str,
    label: String,
    size: f32,
    icon: f32,
    foreground: Hsla,
    hover: Hsla,
    background: Hsla,
    hover_background: Hsla,
}

impl Tool {
    fn build(self) -> gpui_kit::base::Button {
        let label = self.label.clone();
        let image = original(self.name)
            .map(glyph)
            .unwrap_or_else(|| {
                icon(if self.name == "DesktopLyric" {
                    "lyricOpen"
                } else {
                    self.name
                })
            })
            .size(px(self.icon));
        let (hover, hover_background) = (self.hover, self.hover_background);
        gpui_kit::base::Button::new(self.id)
            .accessibility_label(self.label)
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(px(self.size))
            .rounded(px(999.))
            .cursor_pointer()
            .bg(self.background)
            .text_color(self.foreground)
            .hover(move |style| style.bg(hover_background).text_color(hover))
            .focus_visible(move |style| style.border_1().border_color(hover))
            .child(image)
    }
}

impl TrayPanel {
    /// Runs `action` on the main view when the button is clicked.
    fn on(
        button: gpui_kit::base::Button,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Mio, &mut Window, &mut Context<Mio>) + Clone + 'static,
    ) -> gpui_kit::base::Button {
        button.on_click(cx.listener(move |this, _, _, cx| {
            let action = action.clone();
            this.act(cx, action);
        }))
    }
}

/// Raises the main window and runs `then` on the main view.
fn show_main(main: &mut Mio, window: &mut Window, then: impl FnOnce(&mut Mio)) {
    if let Err(error) = crate::platform::show_window(window) {
        eprintln!("Unable to show the main window: {error}");
    }
    then(main);
}

impl Render for TrayPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The same theme handling as the main window's player bar: its mode, the cover's
        // palette, and the bar's own tint for the header.
        // Nothing of the main view is read while drawing: an entity read in a draw makes this
        // window its "current window", and the main view then has none once this one closes.
        let Snapshot {
            mode,
            palette,
            tint,
            song,
            cover,
            status,
            play_mode,
            position,
            duration,
            rows,
            total,
            muted,
            lyric_open,
        } = self.snapshot.clone();
        let (progress, progress_pressed) = (self.progress.clone(), self.progress_pressed.clone());
        let (volume, volume_pressed) = (self.volume.clone(), self.volume_pressed.clone());
        let playing = status == Status::Playing;
        let active = song.is_some();
        let text = palette
            .map(|p| p.text)
            .unwrap_or_else(|| color(mode, "--player-text-idle"));
        let hover = palette
            .map(|p| p.hover)
            .unwrap_or_else(|| color(mode, "--player-text-hover-idle"));
        let play_background = if palette.is_some() {
            white().opacity(0.2)
        } else {
            color(mode, "--player-btn-bg-idle")
        };
        let play_hover_background = if palette.is_some() {
            white().opacity(0.33)
        } else {
            color(mode, "--player-btn-bg-hover-idle")
        };
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let placeholder = color(mode, "--td-text-color-placeholder");
        let brand = color(mode, "--td-brand-color");
        let surface = color(mode, "--td-bg-color-container");
        let row_hover = color(mode, "--td-bg-color-container-hover");
        let border = color(mode, "--td-border-level-1-color");

        let plain = |id, name, label: &str, size, icon| Tool {
            id,
            name,
            label: label.into(),
            size,
            icon,
            foreground: text,
            hover,
            background: transparent_black(),
            hover_background: transparent_black(),
        };

        let title = song
            .as_ref()
            .map(|song| song.title.clone())
            .unwrap_or_else(|| "从一首好歌开始".into());
        let subtitle = song
            .as_ref()
            .map(|song| {
                [song.artist.as_str(), song.album.as_str()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|subtitle| !subtitle.is_empty())
            .unwrap_or_else(|| "搜索歌曲，或打开你的歌单".into());
        let time = |seconds: f64| {
            let seconds = if seconds.is_finite() {
                seconds.max(0.) as u64
            } else {
                0
            };
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };

        let cover_view = div()
            .size(px(52.))
            .flex_shrink_0()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(white().opacity(0.14))
            .flex()
            .items_center()
            .justify_center()
            .child(match cover {
                Some(image) => img(ImageSource::from(image)).size_full().into_any_element(),
                None => icon("Music")
                    .size(px(22.))
                    .text_color(text)
                    .opacity(0.6)
                    .into_any_element(),
            });

        let actions = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(Self::on(
                plain("panel-home", "Home", "显示主窗口", 26., 16.).build(),
                cx,
                |main, window, _| show_main(main, window, |_| {}),
            ))
            .child(Self::on(
                plain("panel-search", "Search", "搜索", 26., 16.).build(),
                cx,
                |main, window, cx| {
                    show_main(main, window, |main| main.go(Page::Search, cx));
                },
            ))
            .child(Self::on(
                Tool {
                    foreground: if lyric_open { hover } else { text },
                    background: if lyric_open {
                        play_background
                    } else {
                        transparent_black()
                    },
                    hover_background: play_hover_background,
                    ..plain("panel-lyric", "DesktopLyric", "桌面歌词", 26., 17.)
                }
                .build(),
                cx,
                |main, window, cx| main.toggle_desktop_lyric(window.scale_factor(), cx),
            ))
            .child(Self::on(
                plain("panel-quit", "Power", "退出", 26., 16.).build(),
                cx,
                |_, _, cx| cx.quit(),
            ));

        let info = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(cover_view)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(
                                palette
                                    .map(|p| p.hover)
                                    .unwrap_or_else(|| color(mode, "--td-brand-color-active")),
                            )
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(text)
                            .child(subtitle),
                    ),
            )
            .child(actions);

        let progress_row = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(11.))
            .line_height(px(14.))
            .text_color(text)
            .child(div().w(px(32.)).child(time(position)))
            .child(div().flex_1().h(px(4.)).child(colored_playback_slider(
                &progress,
                &progress_pressed,
                !active || status == Status::Loading,
                mode,
                cx,
                palette.map(|p| (p.main, p.hover)),
            )))
            .child(div().w(px(32.)).flex().justify_end().child(time(duration)));

        let transport = div()
            .flex()
            .items_center()
            .justify_between()
            .child(Self::on(
                plain("panel-mode", play_mode.icon(), "播放模式", 28., 18.).build(),
                cx,
                |main, _, cx| {
                    main.player.mode = main.player.mode.next();
                    main.player.refresh_prefetch();
                    main.save_preferences();
                    cx.notify();
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(Self::on(
                        plain("panel-prev", "SkipPrev", "上一首", 30., 20.)
                            .build()
                            .disabled(!active),
                        cx,
                        |main, _, cx| main.step_track(false, cx),
                    ))
                    .child(Self::on(
                        Tool {
                            id: "panel-play",
                            name: if playing { "Pause" } else { "PlayFilled" },
                            label: if playing { "暂停" } else { "播放" }.into(),
                            size: 36.,
                            icon: 22.,
                            foreground: hover,
                            hover,
                            background: play_background,
                            hover_background: play_hover_background,
                        }
                        .build()
                        .disabled(!active),
                        cx,
                        |main, _, cx| main.toggle_play(cx),
                    ))
                    .child(Self::on(
                        plain("panel-next", "SkipNext", "下一首", 30., 20.)
                            .build()
                            .disabled(!active),
                        cx,
                        |main, _, cx| main.step_track(true, cx),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(Self::on(
                        plain("panel-mute", "VolumeUp", "静音", 28., 18.)
                            .build()
                            .opacity(if muted { 0.5 } else { 1. }),
                        cx,
                        |main, window, cx| {
                            let percent = if main.player.volume > 0. { 0. } else { 50. };
                            main.set_volume_percent(percent, window, cx);
                        },
                    ))
                    .child(div().w(px(56.)).child(colored_playback_slider(
                        &volume,
                        &volume_pressed,
                        false,
                        mode,
                        cx,
                        palette.map(|p| (p.main, p.hover)),
                    ))),
            );

        let header = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(HEADER))
            .gap(px(10.))
            .px(px(14.))
            .pt(px(12.))
            .pb(px(10.))
            .bg(tint)
            .child(info)
            .child(progress_row)
            .child(transport);

        let list = rows.into_iter().fold(
            div()
                .id("panel-queue")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .px(px(6.))
                .pb(px(6.))
                .overflow_y_scroll(),
            |list, (index, song, liked, current)| {
                let cover = song
                    .cover_art
                    .as_deref()
                    .filter(|url| url.starts_with("http"))
                    .map(str::to_owned);
                let subtitle = [song.artist.as_str(), song.album.as_str()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let favourite = song.clone();
                list.child(
                    div()
                        .id(SharedString::from(format!("panel-row-{index}")))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(44.))
                        .px(px(8.))
                        .flex_shrink_0()
                        .rounded(px(8.))
                        .cursor_pointer()
                        .when(current, |row| row.bg(brand.opacity(0.1)))
                        .hover(move |row| row.bg(row_hover))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.act(cx, move |main, _, cx| main.play_index(index, cx));
                        }))
                        .child(
                            div()
                                .size(px(32.))
                                .flex_shrink_0()
                                .rounded(px(6.))
                                .overflow_hidden()
                                .bg(border)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(match cover {
                                    Some(url) => img(url).size_full().into_any_element(),
                                    None => icon("Music")
                                        .size(px(14.))
                                        .text_color(placeholder)
                                        .into_any_element(),
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(13.))
                                        .line_height(px(18.))
                                        .text_color(if current { brand } else { primary })
                                        .child(song.title.clone()),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(11.))
                                        .line_height(px(15.))
                                        .text_color(secondary)
                                        .child(subtitle),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("panel-like-{index}")))
                                .size(px(26.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .cursor_pointer()
                                .text_color(if liked {
                                    color(mode, "--td-error-color")
                                } else {
                                    placeholder
                                })
                                .hover(|style| style.opacity(0.75))
                                .child(
                                    icon(if liked { "TD-Heart-Filled" } else { "TD-Heart" })
                                        .size(px(16.)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let song = favourite.clone();
                                    this.act(cx, move |main, window, cx| {
                                        main.toggle_favorite(song, window, cx)
                                    });
                                })),
                        ),
                )
            },
        );

        let queue = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(14.))
                    .h(px(30.))
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .text_color(placeholder)
                    .child("播放队列")
                    .child(format!("{total} 首")),
            )
            .child(if total == 0 {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(placeholder)
                    .child("队列为空")
                    .into_any_element()
            } else {
                list.into_any_element()
            });

        div()
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .overflow_hidden()
            .bg(surface)
            .border_1()
            .border_color(border)
            .shadow(vec![BoxShadow {
                color: black().opacity(0.18),
                offset: point(px(0.), px(6.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(header)
            .child(queue)
    }
}
