//! The desktop lyric window (`desktop-lyric` of the original): a transparent, borderless,
//! always-on-top 600 × 180 window that shows the current lyric lines over everything else, with
//! hover controls and a lock that lets the pointer through.
//!
//! The original runs it as a second WebView fed by `desktop-lyric-*` events. Here both windows
//! belong to one process, so the window reads the main view's lyrics, clock and player directly.
use super::full_player::{LYRIC_DIM, LYRIC_LIT, word_highlight};
use super::*;
use gpui_kit::component::Root;
use serde::{Deserialize, Serialize};

const WINDOW_FILE: &str = "desktop_lyric_window.json";
const OPTION_FILE: &str = "desktop_lyric_option.json";
const WIDTH: f32 = 600.;
const HEIGHT: f32 = 180.;
/// `y = screen height - window height - 100` for the first opening.
const BOTTOM_MARGIN: f32 = 100.;
/// While word-timed lyrics play, the window redraws at 25 Hz.
const FRAME: Duration = Duration::from_millis(40);
/// The main window's chrome opacity of lines other than the one being sung (`opacity: 0.5`).
const OTHER_LINE: f32 = 0.5;

/// `desktop_lyric_option.json`. A key that is missing takes the window's own default (what
/// `DeskTopLyric.vue` falls back to), which differs from the settings page's: 30 / 800 here, 36 /
/// 700 there; the settings page writes every key when it saves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LyricOption {
    pub font_size: f32,
    pub main_color: String,
    pub font_weight: u32,
    /// `left` | `center` | `right` | `both`: text alignment, and where the current line sits.
    pub position: String,
    pub always_show_play_info: bool,
    pub animation: bool,
    pub show_yrc: bool,
    pub show_tran: bool,
    pub is_double_line: bool,
    pub font_family: String,
}

impl Default for LyricOption {
    fn default() -> Self {
        Self {
            font_size: 30.,
            main_color: "rgba(255, 255, 255, 1)".into(),
            font_weight: 800,
            position: "center".into(),
            always_show_play_info: false,
            animation: true,
            show_yrc: true,
            show_tran: true,
            is_double_line: false,
            font_family: String::new(),
        }
    }
}

/// `desktop_lyric_window.json`: whether the window was open and where, in physical pixels.
#[derive(Default, Serialize, Deserialize)]
struct WindowState {
    #[serde(default)]
    is_open: bool,
    #[serde(default)]
    x: Option<i64>,
    #[serde(default)]
    y: Option<i64>,
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)` and `rgba(r, g, b, a)`.
fn parse_color(text: &str) -> Option<Hsla> {
    let text = text.trim();
    let channel = |value: &str| {
        value
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| (v / 255.).clamp(0., 1.))
    };
    if let Some(hex) = text.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()?;
        let pair = |i: usize| (digits[i] * 16 + digits[i + 1]) as f32 / 255.;
        let (r, g, b, a) = match digits.len() {
            3 => (
                digits[0] as f32 * 17. / 255.,
                digits[1] as f32 * 17. / 255.,
                digits[2] as f32 * 17. / 255.,
                1.,
            ),
            6 => (pair(0), pair(2), pair(4), 1.),
            8 => (pair(0), pair(2), pair(4), pair(6)),
            _ => return None,
        };
        return Some(Hsla::from(Rgba { r, g, b, a }));
    }
    let inner = text
        .strip_prefix("rgba(")
        .or_else(|| text.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    let a = parts
        .get(3)
        .map_or(Some(1.), |v| v.trim().parse::<f32>().ok())?
        .clamp(0., 1.);
    Some(Hsla::from(Rgba {
        r: channel(parts[0])?,
        g: channel(parts[1])?,
        b: channel(parts[2])?,
        a,
    }))
}

/// A style colour for drawing a swatch; white when it cannot be read.
pub(super) fn style_color(text: &str) -> Hsla {
    parse_color(text).unwrap_or_else(white)
}

/// Whether a style colour can be drawn (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`).
pub(super) fn color_is_valid(text: &str) -> bool {
    parse_color(text).is_some()
}

impl Mio {
    pub(super) fn desktop_lyric_open(&self) -> bool {
        self.desktop_lyric.is_some()
    }

    /// Reads the saved style; keys that are missing keep the window's defaults.
    pub(super) fn load_desktop_lyric_option(&mut self) {
        if let Some(Ok(Some(option))) = self
            .store
            .as_ref()
            .map(|store| store.load::<LyricOption>(OPTION_FILE))
        {
            self.desktop_lyric_option = option;
        }
    }

    /// Changes the style from the settings page: the file is written whole (as the original's
    /// `set-desktop-lyric-option` does) and an open window picks it up at once.
    pub(super) fn change_desktop_lyric_option(
        &mut self,
        change: impl FnOnce(&mut LyricOption),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.settings_ui.desktop_style);
        self.desktop_lyric_option = self.settings_ui.desktop_style.clone();
        if let Some(store) = &self.store {
            let _ = store.save(OPTION_FILE, &self.desktop_lyric_option);
        }
        cx.notify();
    }

    /// The style the settings page shows: the saved keys over the page's own defaults (36 /
    /// 700 / white), which are not the window's.
    pub(super) fn settings_desktop_style(&self) -> LyricOption {
        let mut style = LyricOption {
            font_size: 36.,
            main_color: "#ffffff".into(),
            font_weight: 700,
            font_family: "lyricfont".into(),
            ..LyricOption::default()
        };
        let saved: Option<serde_json::Value> = self
            .store
            .as_ref()
            .and_then(|store| store.load(OPTION_FILE).ok().flatten());
        if let Some(serde_json::Value::Object(map)) = saved {
            if let Some(v) = map.get("fontSize").and_then(|v| v.as_f64()) {
                style.font_size = v as f32;
            }
            if let Some(v) = map.get("mainColor").and_then(|v| v.as_str()) {
                style.main_color = v.to_string();
            }
            if let Some(v) = map.get("fontWeight").and_then(|v| v.as_u64()) {
                style.font_weight = v as u32;
            }
            if let Some(v) = map.get("position").and_then(|v| v.as_str()) {
                style.position = v.to_string();
            }
            if let Some(v) = map.get("alwaysShowPlayInfo").and_then(|v| v.as_bool()) {
                style.always_show_play_info = v;
            }
            if let Some(v) = map.get("animation").and_then(|v| v.as_bool()) {
                style.animation = v;
            }
            if let Some(v) = map.get("showYrc").and_then(|v| v.as_bool()) {
                style.show_yrc = v;
            }
            if let Some(v) = map.get("showTran").and_then(|v| v.as_bool()) {
                style.show_tran = v;
            }
            if let Some(v) = map.get("isDoubleLine").and_then(|v| v.as_bool()) {
                style.is_double_line = v;
            }
            if let Some(v) = map.get("fontFamily").and_then(|v| v.as_str()) {
                style.font_family = v.to_string();
            }
        }
        style
    }

    fn save_desktop_lyric_state(&self, open: bool) {
        let (x, y) = self.desktop_lyric_position.unzip();
        if let Some(store) = &self.store {
            let _ = store.save(
                WINDOW_FILE,
                &WindowState {
                    is_open: open,
                    x,
                    y,
                },
            );
        }
    }

    /// Opens the window again when it was open at the last exit.
    pub(super) fn restore_desktop_lyric(&mut self, scale: f32, cx: &mut Context<Self>) {
        let was_open = self
            .store
            .as_ref()
            .and_then(|store| store.load::<WindowState>(WINDOW_FILE).ok().flatten())
            .is_some_and(|state| state.is_open);
        if was_open {
            self.open_desktop_lyric(scale, cx);
        }
    }

    /// The player bar button: opens the window; when it is open and locked, unlocks it; when
    /// open and unlocked, closes it.
    pub(super) fn toggle_desktop_lyric(&mut self, scale: f32, cx: &mut Context<Self>) {
        if self.desktop_lyric_opening {
            return;
        }
        if self.desktop_lyric.is_none() {
            self.open_desktop_lyric(scale, cx);
        } else if self.desktop_lyric_locked {
            self.set_desktop_lyric_locked(false, cx);
        } else {
            self.close_desktop_lyric(cx);
        }
    }

    pub(super) fn open_desktop_lyric(&mut self, scale: f32, cx: &mut Context<Self>) {
        if self.desktop_lyric.is_some() || self.desktop_lyric_opening {
            return;
        }
        self.load_desktop_lyric_option();
        let saved = self
            .store
            .as_ref()
            .and_then(|store| store.load::<WindowState>(WINDOW_FILE).ok().flatten());
        let scale = scale.max(1.);
        let origin = match saved.and_then(|s| s.x.zip(s.y)) {
            Some((x, y)) => point(px(x as f32 / scale), px(y as f32 / scale)),
            None => {
                let screen = cx
                    .primary_display()
                    .map(|display| display.bounds().size)
                    .unwrap_or_else(|| size(px(1440.), px(900.)));
                point(
                    (screen.width - px(WIDTH)) / 2.,
                    screen.height - px(HEIGHT + BOTTOM_MARGIN),
                )
            }
        };
        // What the file will say until the window reports a move: where it opens.
        self.desktop_lyric_position = Some((
            (f32::from(origin.x) * scale).round() as i64,
            (f32::from(origin.y) * scale).round() as i64,
        ));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                origin,
                size(px(WIDTH), px(HEIGHT)),
            ))),
            titlebar: Some(TitlebarOptions {
                title: None,
                appears_transparent: true,
                traffic_light_position: None,
            }),
            kind: WindowKind::Floating,
            focus: false,
            is_movable: true,
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("com.vant.Mio.Music.GPUI".into()),
            ..Default::default()
        };
        // Opened from the executor, outside this view's update: the new window draws at once
        // and reads this view, which must not still be leased by the click that asked for it.
        let main = cx.entity();
        self.desktop_lyric_opening = true;
        cx.spawn(async move |this, cx| {
            let opened = cx.update(|cx| {
                cx.open_window(options, move |window, cx| {
                    if let Err(error) = crate::platform::configure_overlay(window) {
                        eprintln!("Unable to configure the desktop lyric window: {error}");
                    }
                    let view = cx.new(|cx| DesktopLyric::new(main, window, cx));
                    // The component Root paints the theme background; this window is clear.
                    cx.new(|cx| Root::new(view, window, cx).bg(transparent_black()))
                })
            });
            let _ = this.update(cx, |this, cx| {
                this.desktop_lyric_opening = false;
                match opened {
                    Ok(handle) => {
                        this.desktop_lyric = Some(handle.into());
                        this.desktop_lyric_locked = false;
                        this.save_desktop_lyric_state(true);
                        // Lyrics are otherwise only fetched for the full player.
                        if matches!(this.lyrics, Remote::Idle | Remote::Loading)
                            && this.player.song().is_some()
                        {
                            this.lyrics = Remote::Loading;
                            this.load_lyrics(cx);
                        }
                    }
                    Err(_) => eprintln!("Unable to open the desktop lyric window"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Closes the window from the main window's side and remembers it was closed.
    pub(super) fn close_desktop_lyric(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.desktop_lyric.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.desktop_lyric_locked = false;
        self.save_desktop_lyric_state(false);
        cx.notify();
    }

    /// The window closed itself (its close button): only the state is left to update.
    fn desktop_lyric_closed(&mut self, cx: &mut Context<Self>) {
        self.desktop_lyric = None;
        self.desktop_lyric_locked = false;
        self.save_desktop_lyric_state(false);
        cx.notify();
    }

    /// Lock and unlock from the main window; the window's own button sets the native state
    /// itself and only records the flag through [`Mio::desktop_lyric_lock_changed`].
    pub(super) fn set_desktop_lyric_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if let Some(handle) = self.desktop_lyric {
            let _ = handle.update(cx, |_, window, _| {
                if let Err(error) = crate::platform::set_click_through(window, locked) {
                    eprintln!("Unable to change the desktop lyric lock: {error}");
                }
            });
        }
        self.desktop_lyric_locked = locked;
        cx.notify();
    }

    fn desktop_lyric_lock_changed(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.desktop_lyric_locked = locked;
        cx.notify();
    }

    fn desktop_lyric_moved(&mut self, x: i64, y: i64) {
        if self.desktop_lyric_position != Some((x, y)) {
            self.desktop_lyric_position = Some((x, y));
            self.save_desktop_lyric_state(true);
        }
    }
}

pub struct DesktopLyric {
    main: Entity<Mio>,
    hovering: bool,
    _subscriptions: Vec<Subscription>,
    /// The main-view state last shown; other main-view redraws leave this window alone.
    shown: Option<ShownKey>,
}

#[derive(PartialEq)]
struct ShownKey {
    song: Option<(String, String, String)>,
    lyrics: (u64, Option<usize>),
    active: Option<usize>,
    playing: bool,
    locked: bool,
    option: LyricOption,
}

impl DesktopLyric {
    fn new(main: Entity<Mio>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Whatever the main view shows changes (song, lyrics, lock, playback state): redraw.
        let follow = cx.observe(&main, |this: &mut Self, main, cx| {
            let key = main.read(cx).desktop_lyric_key();
            if this.shown.as_ref() != Some(&key) {
                this.shown = Some(key);
                cx.notify();
            }
        });
        let tracker = main.clone();
        let moved = cx.observe_window_bounds(window, move |_, window, cx| {
            let bounds = window.bounds();
            let scale = window.scale_factor();
            let (x, y) = (
                (f32::from(bounds.origin.x) * scale).round() as i64,
                (f32::from(bounds.origin.y) * scale).round() as i64,
            );
            tracker.update(cx, |main, _| main.desktop_lyric_moved(x, y));
        });
        // Word-timed lyrics move between the engine's position reports.
        cx.spawn_in(window, async move |this, cx| {
            // Frame rate only while words are sung; otherwise the observer redraws on change.
            let mut wait = FRAME;
            loop {
                cx.background_executor().timer(wait).await;
                let alive = this.update(cx, |this, cx| {
                    let animating = this.main.read(cx).lyrics_playing_words();
                    if animating {
                        cx.notify();
                    }
                    animating
                });
                match alive {
                    Ok(animating) => {
                        wait = if animating {
                            FRAME
                        } else {
                            Duration::from_millis(200)
                        }
                    }
                    Err(_) => break,
                }
            }
        })
        .detach();
        Self {
            main,
            hovering: false,
            _subscriptions: vec![follow, moved],
            shown: None,
        }
    }

    fn control(
        id: &'static str,
        icon: &'static str,
        label: String,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .text_color(white().opacity(0.8))
            .hover(|style| style.text_color(white()).bg(white().opacity(0.2)))
            .active(|style| style.opacity(0.9))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
            })
            .child(super::icon(icon).size(px(20.)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
    }
}

impl Mio {
    /// The synced line at `position`, as the desktop lyric window highlights it.
    fn desktop_lyric_active(&self, position: f64) -> Option<usize> {
        match &self.lyrics {
            Remote::Ready(tracks) => {
                tracks
                    .first()
                    .filter(|lyrics| lyrics.synced)
                    .and_then(|lyrics| {
                        lyrics.lines.iter().rposition(|line| {
                            (line.start.unwrap_or(0) as f64 - lyrics.offset as f64) / 1000.
                                <= position
                        })
                    })
            }
            _ => None,
        }
    }

    fn desktop_lyric_key(&self) -> ShownKey {
        ShownKey {
            song: self
                .player
                .song()
                .map(|song| (song.id.clone(), song.title.clone(), song.artist.clone())),
            lyrics: (
                self.lyrics_revision,
                match &self.lyrics {
                    Remote::Ready(tracks) => Some(tracks.first().map_or(0, |l| l.lines.len())),
                    _ => None,
                },
            ),
            active: self.desktop_lyric_active(self.lyric_position()),
            playing: self.player.status == Status::Playing,
            locked: self.desktop_lyric_locked,
            option: self.desktop_lyric_option.clone(),
        }
    }

    /// Word-timed lyrics are being sung: the desktop window needs frame-rate redraws.
    pub(super) fn lyrics_playing_words(&self) -> bool {
        self.player.status == Status::Playing
            && !self.player.buffering
            && matches!(&self.lyrics, Remote::Ready(tracks)
                if tracks.first().is_some_and(|lyrics| lyrics.word_timed))
    }
}

impl Render for DesktopLyric {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Everything the window shows, read from the main view in one go.
        let (option, locked, playing, position, title, show, active) = {
            let main = self.main.read(cx);
            let option = main.desktop_lyric_option.clone();
            let position = main.lyric_position();
            let song = main.player.song();
            let title = song
                .map(|song| format!("{} - {}", song.title, song.artist))
                .unwrap_or_default();
            let active = main.desktop_lyric_active(position);
            // Previous, current and next line; before the first line the window shows the
            // first two. Only those are copied out of the main view.
            let first = active.map_or(0, |index| index.saturating_sub(1));
            let show: Vec<(usize, crate::lyrics::Line)> = match &main.lyrics {
                Remote::Ready(tracks) => tracks
                    .first()
                    .map(|lyrics| {
                        lyrics
                            .lines
                            .iter()
                            .enumerate()
                            .skip(first)
                            .take(if active.is_some() { 3 } else { 2 })
                            .map(|(index, line)| (index, line.clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            (
                option,
                main.desktop_lyric_locked,
                main.player.status == Status::Playing,
                position,
                title,
                show,
                active,
            )
        };
        let now_ms = (position * 1000.).max(0.) as u64;
        let color = parse_color(&option.main_color).unwrap_or_else(white);
        let family: Option<SharedString> = match option.font_family.trim() {
            "" => None,
            // The bundled lyric font is registered under its own family name.
            "lyricfont" => Some("Alimama FangYuanTi VF".into()),
            other => Some(other.to_string().into()),
        };
        let weight = FontWeight(option.font_weight.clamp(100, 900) as f32);
        let size_px = option.font_size.clamp(12., 96.);
        // `position` anchors the current line vertically (left 20 %, centre 50 %, right 80 %)
        // and aligns the text the same way.
        let (anchor, align) = match option.position.as_str() {
            "left" => (0.2, 0),
            "right" => (0.8, 2),
            _ => (0.5, 1),
        };

        let body = if show.is_empty() {
            // No lyrics: the song, or the app's name.
            let placeholder = if title.trim().is_empty() || title == " - " {
                "Mio Desktop Lyric".to_string()
            } else {
                title.clone()
            };
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size_px))
                .font_weight(weight)
                .text_color(color)
                .child(placeholder)
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(div().flex_grow(anchor))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .when(align == 0, |column| column.items_start())
                        .when(align == 1, |column| column.items_center())
                        .when(align == 2, |column| column.items_end())
                        .px(px(24.))
                        .children(show.into_iter().map(|(index, line)| {
                            let current = active == Some(index);
                            let words = option.show_yrc && !line.words.is_empty();
                            let body: AnyElement = if current && words {
                                word_highlight(&line, now_ms, color, LYRIC_DIM, LYRIC_LIT)
                                    .into_any_element()
                            } else {
                                line.text.clone().into_any_element()
                            };
                            let alpha = if current {
                                if words { LYRIC_DIM } else { 1. }
                            } else {
                                OTHER_LINE
                            };
                            div()
                                .w_full()
                                .py(px(2.))
                                .flex()
                                .flex_col()
                                .when(align == 0, |l| l.items_start().text_left())
                                .when(align == 1, |l| l.items_center().text_center())
                                .when(align == 2, |l| l.items_end().text_right())
                                .text_size(px(if current {
                                    size_px * 1.2
                                } else {
                                    size_px * 0.85
                                }))
                                .font_weight(weight)
                                .text_color(Hsla {
                                    a: color.a * alpha,
                                    ..color
                                })
                                .child(body)
                                .children(
                                    [&line.translation, &line.romanization]
                                        .into_iter()
                                        .flatten()
                                        .map(|sub| {
                                            div()
                                                .text_size(px(size_px * 0.5))
                                                .text_color(Hsla {
                                                    a: color.a * if current { 0.85 } else { 0.4 },
                                                    ..color
                                                })
                                                .child(sub.clone())
                                        }),
                                )
                        })),
                )
                .child(div().flex_grow(1. - anchor))
                .into_any_element()
        };

        let controls = (self.hovering && !locked).then(|| {
            let play_icon = if playing { "dl-pause" } else { "dl-play" };
            let english = self.main.read(cx).english;
            let label = |zh: &'static str, en: &'static str| {
                if english {
                    en.to_string()
                } else {
                    zh.to_string()
                }
            };
            div()
                .absolute()
                .bottom(px(10.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(16.))
                        .py(px(8.))
                        .rounded(px(20.))
                        .bg(black().opacity(0.6))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .when(!title.trim().is_empty() && title != " - ", |bar| {
                            bar.child(
                                div()
                                    .max_w(px(300.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(12.))
                                    .text_color(white().opacity(0.8))
                                    .child(title.clone()),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(Self::control(
                                    "desktop-lyric-prev",
                                    "dl-prev",
                                    label("上一首", "Previous"),
                                    cx,
                                    |this, _, cx| {
                                        this.main.update(cx, |main, cx| main.step_track(false, cx))
                                    },
                                ))
                                .child(Self::control(
                                    "desktop-lyric-toggle",
                                    play_icon,
                                    if playing {
                                        label("暂停", "Pause")
                                    } else {
                                        label("播放", "Play")
                                    },
                                    cx,
                                    |this, _, cx| {
                                        this.main.update(cx, |main, cx| main.toggle_play(cx))
                                    },
                                ))
                                .child(Self::control(
                                    "desktop-lyric-next",
                                    "dl-next",
                                    label("下一首", "Next"),
                                    cx,
                                    |this, _, cx| {
                                        this.main.update(cx, |main, cx| main.step_track(true, cx))
                                    },
                                ))
                                .child(Self::control(
                                    "desktop-lyric-lock",
                                    "dl-unlock",
                                    label("锁定", "Lock"),
                                    cx,
                                    |this, window, cx| {
                                        // Locking is the window's own native state; the lock
                                        // button is only ever shown while unlocked.
                                        if crate::platform::set_click_through(window, true).is_ok()
                                        {
                                            this.hovering = false;
                                            this.main.update(cx, |main, cx| {
                                                main.desktop_lyric_lock_changed(true, cx)
                                            });
                                        }
                                    },
                                ))
                                .child(Self::control(
                                    "desktop-lyric-close",
                                    "dl-close",
                                    label("关闭", "Close"),
                                    cx,
                                    |this, window, cx| {
                                        window.remove_window();
                                        this.main
                                            .update(cx, |main, cx| main.desktop_lyric_closed(cx));
                                    },
                                )),
                        ),
                )
        });

        div()
            .id("desktop-lyric")
            .size_full()
            .relative()
            .overflow_hidden()
            .font_family(family.unwrap_or_else(|| ".SystemUIFont".into()))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.hovering != *hovered {
                    this.hovering = *hovered;
                    cx.notify();
                }
            }))
            // The whole window drags while it is unlocked (`data-tauri-drag-region`).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, window, _| window.start_window_move()),
            )
            .child(body)
            .children(controls)
    }
}
