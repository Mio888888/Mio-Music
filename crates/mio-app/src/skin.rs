use gpui_kit::component::{
    Icon,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use mio_tokens::Mode;
use std::{cell::Cell, rc::Rc};

mod dynamic_theme;
pub use dynamic_theme::{Colors, SongColors};

pub fn color(mode: impl Into<Colors>, key: &str) -> Hsla {
    let colors = mode.into();
    let mode = colors.mode;
    if let Some(song) = colors.song {
        // Resolve static aliases against the same dynamic overrides as CSS var().
        let mut resolved = key;
        for _ in 0..32 {
            if let Some(value) = song.color(resolved) {
                return value;
            }
            match mio_tokens::raw(mode, resolved).and_then(|value| {
                value
                    .strip_prefix("var(")
                    .and_then(|value| value.strip_suffix(')'))
            }) {
                Some(alias) if alias.starts_with("--") && !alias.contains(',') => {
                    resolved = alias.trim()
                }
                _ => break,
            }
        }
    }
    let [r, g, b, a] = mio_tokens::color(mode, key).unwrap_or_else(|error| panic!("{error}"));
    Hsla::from(Rgba { r, g, b, a })
}
pub fn icon(name: &'static str) -> Icon {
    Icon::default().path(format!("icons/{name}.svg"))
}

// TDesign appearance with Kit focus/keyboard behavior.
pub fn control(
    id: impl Into<ElementId>,
    name: &'static str,
    label: String,
    mode: impl Into<Colors>,
    cx: &App,
) -> Button {
    let mode = mode.into();
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(color(mode, "--td-text-color-secondary"))
                .hover(color(mode, "--td-bg-color-component-hover"))
                .active(color(mode, "--td-bg-color-component-active")),
        )
        .icon(icon(name))
        .accessibility_label(label.clone())
        .tooltip(label)
        .rounded(px(999.))
        .w(px(32.))
        .h(px(32.))
}

pub fn colored_control(
    id: &'static str,
    name: &'static str,
    label: String,
    foreground: Hsla,
    hover: Hsla,
    background: Hsla,
    hover_background: Hsla,
) -> gpui_kit::base::Button {
    let original = match name {
        "SkipPrev" => Some("shangyishou"),
        "SkipNext" => Some("xiayishou"),
        "PlayFilled" => Some("bofang"),
        "Pause" => Some("zanting"),
        "Repeat" => Some("shunxubofangtubiao"),
        "RepeatSong" => Some("bofang-xunhuanbofang"),
        "Shuffle" => Some("suijibofang"),
        "VolumeUp" | "VolumeMute" => Some("shengyin"),
        "PlayList" => Some("liebiao"),
        _ => None,
    };
    let mobile = id.starts_with("mobile-");
    let center = matches!(id, "play" | "previous" | "next");
    let icon_size = match id {
        "mobile-play" => 22.,
        "mobile-previous" | "mobile-next" => 20.,
        "mobile-queue" => 16.,
        "volume" => 20.,
        "queue" => 21.,
        _ if center => 28.,
        _ => 18.,
    };
    let control_size = match id {
        _ if mobile => 44.,
        "play-mode" | "queue" | "desktop-lyrics" => 32.,
        "volume" => 28.,
        _ if center => 38.,
        _ => 26.,
    };
    let image = original
        .map(glyph)
        .unwrap_or_else(|| {
            icon(if name == "DesktopLyric" {
                "lyricOpen"
            } else {
                name
            })
        })
        .size(px(icon_size));
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        })
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .size(px(control_size))
        .rounded(px(999.))
        .cursor_pointer()
        .bg(background)
        .text_color(foreground)
        .hover(move |style| style.bg(hover_background).text_color(hover))
        .styles(|styles| styles.disabled(|style| style.opacity(if mobile { 0.4 } else { 0.6 })))
        .focus_visible(|style| style.border_1().border_color(hover))
        .child(image)
}

#[derive(IntoElement)]
pub struct QueueCount(pub usize);

impl RenderOnce for QueueCount {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let label: SharedString = if self.0 > 99 {
            "99+".into()
        } else {
            self.0.to_string().into()
        };
        let features = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        let run = TextRun {
            len: label.len(),
            font: Font {
                family: ".SystemUIFont".into(),
                features: features.clone(),
                ..Font::default()
            },
            color: white(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = window
            .text_system()
            .shape_line(label.clone(), px(12.), &[run], None)
            .width()
            + px(12.);
        div()
            .absolute()
            .left(relative(1.))
            .top(px(-9.))
            .ml(-width * 0.5)
            .w(width)
            .h(px(18.))
            .rounded(px(9.))
            .bg(rgb(0xbbbbbb))
            .font_family(".SystemUIFont")
            .font_features(features)
            .font_weight(FontWeight::NORMAL)
            .text_size(px(12.))
            .line_height(px(18.))
            .text_color(white())
            .text_center()
            .child(label)
    }
}

/// Keep Kit state and accessibility, committing the pointer's final position even
/// when a native drag coalesces move events before mouse-up.
pub fn playback_slider(
    state: &Entity<gpui_kit::component::slider::SliderState>,
    pressed: &Rc<Cell<bool>>,
    disabled: bool,
    mode: impl Into<Colors>,
    cx: &App,
) -> impl IntoElement {
    colored_playback_slider(state, pressed, disabled, mode, cx, None)
}

pub fn colored_playback_slider(
    state: &Entity<gpui_kit::component::slider::SliderState>,
    pressed: &Rc<Cell<bool>>,
    disabled: bool,
    mode: impl Into<Colors>,
    cx: &App,
    palette: Option<(Hsla, Hsla)>,
) -> impl IntoElement {
    let mode = mode.into();
    let percentage = state.read(cx).percentage().end.clamp(0., 1.);
    let brand = palette
        .map(|colors| colors.0)
        .unwrap_or_else(|| color(mode, "--td-brand-color"));
    let thumb = palette
        .map(|colors| colors.1)
        .unwrap_or_else(|| color(mode, "--td-bg-color-container"));
    let pointer = slider_pointer(state, pressed, Axis::Horizontal, disabled);
    gpui_kit::base::Slider::new(state)
        .disabled(disabled)
        .h(px(4.))
        .w_full()
        .flex()
        .items_center()
        .child(
            div()
                .h(px(16.))
                .w_full()
                .flex()
                .items_center()
                .relative()
                .child(
                    gpui_kit::base::SliderIndicator::new(state)
                        .h(px(4.))
                        .w_full()
                        .relative()
                        .bg(if palette.is_some() {
                            transparent_black()
                        } else {
                            color(mode, "--td-border-level-1-color")
                        })
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .h_full()
                                .w(relative(percentage))
                                .when(palette.is_some(), |fill| {
                                    fill.bg(linear_gradient(
                                        90.,
                                        linear_color_stop(brand.opacity(0.2), 0.),
                                        linear_color_stop(brand, 0.8),
                                    )
                                    .color_space(ColorSpace::Srgb))
                                })
                                .when(palette.is_none(), |fill| fill.bg(brand)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(relative(percentage))
                                .top(px(-6.))
                                .ml(px(-8.))
                                .size(px(16.))
                                .rounded(px(8.))
                                .bg(thumb)
                                .border_1()
                                .border_color(brand),
                        ),
                )
                .child(pointer),
        )
}

pub fn volume_slider(
    state: &Entity<gpui_kit::component::slider::SliderState>,
    pressed: &Rc<Cell<bool>>,
    disabled: bool,
    brand: Hsla,
    cx: &App,
) -> impl IntoElement {
    let percentage = state.read(cx).percentage().end.clamp(0., 1.);
    gpui_kit::base::Slider::new(state)
        .vertical()
        .disabled(disabled)
        .w(px(12.))
        .h(px(100.))
        .relative()
        .flex()
        .justify_center()
        .child(
            gpui_kit::base::SliderIndicator::new(state)
                .relative()
                .w(px(4.))
                .h_full()
                .rounded(px(2.))
                .bg(rgba(0xffffff71))
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(relative(percentage))
                        .rounded(px(2.))
                        .bg(brand),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(-4.))
                        .bottom(relative(percentage))
                        .mb(px(-6.))
                        .size(px(12.))
                        .rounded(px(6.))
                        .bg(brand),
                ),
        )
        .child(slider_pointer(state, pressed, Axis::Vertical, disabled))
}

pub(crate) fn slider_pointer(
    state: &Entity<gpui_kit::component::slider::SliderState>,
    pressed: &Rc<Cell<bool>>,
    axis: Axis,
    disabled: bool,
) -> impl IntoElement {
    if disabled {
        pressed.set(false);
    }
    let pointer_state = state.clone();
    let pressed = pressed.clone();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            if disabled {
                return;
            }
            let state = pointer_state.clone();
            let active = pressed.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase.bubble() && event.button == MouseButton::Left && hitbox.is_hovered(window)
                {
                    active.set(true);
                    state.update(cx, |state, cx| {
                        state.update_value_by_position(axis, event.position, false, window, cx);
                    });
                    cx.stop_propagation();
                }
            });
            let state = pointer_state.clone();
            let active = pressed.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase.capture() && active.get() {
                    state.update(cx, |state, cx| {
                        state.update_value_by_position(axis, event.position, false, window, cx);
                    });
                }
            });
            let state = pointer_state.clone();
            let active = pressed.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase.capture() && event.button == MouseButton::Left && active.replace(false) {
                    state.update(cx, |state, cx| {
                        state.update_value_by_position(axis, event.position, false, window, cx);
                        state.handle_release(cx);
                    });
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}

/// Original HomeLayout viewport-relative two-stop gradients.
pub fn workspace_background(
    mode: impl Into<Colors>,
    viewport: f32,
    area: f32,
    start_vh: f32,
    end_vh: f32,
) -> Background {
    let mode = mode.into();
    let area = area.max(1.);
    linear_gradient(
        180.,
        linear_color_stop(
            color(mode, "--td-brand-color-4"),
            start_vh * viewport / area,
        ),
        linear_color_stop(
            color(mode, "--td-bg-color-container"),
            end_vh * viewport / area,
        ),
    )
    .color_space(ColorSpace::Srgb)
}

pub fn mixed_color(
    mode: impl Into<Colors>,
    foreground: &str,
    amount: f32,
    background: &str,
) -> Hsla {
    let mode = mode.into();
    let foreground = color(mode, foreground).to_rgb();
    let background = color(mode, background).to_rgb();
    Hsla::from(Rgba {
        r: foreground.r * amount + background.r * (1. - amount),
        g: foreground.g * amount + background.g * (1. - amount),
        b: foreground.b * amount + background.b * (1. - amount),
        a: foreground.a * amount + background.a * (1. - amount),
    })
}

/// Paths extracted unchanged from the original iconfont SVG, avoiding platform font fallback.
pub fn glyph(name: &str) -> Icon {
    Icon::default()
        .path(format!("icons/nav-{name}.svg"))
        .size(rems(1.))
}

/// The reference CSS radial-gradient, rendered through GPUI's SVG image support.
/// Intrinsic coordinates match the first 880 × 425 desktop fixture.
pub fn guide_glow(mode: Mode) -> std::sync::Arc<Image> {
    guide_glow_sized(mode, 880, 425)
}

/// `radial-gradient(circle at top, brand 12%, transparent 42%)` of a `width` × `height` box:
/// the circle reaches the far corner, so its radius depends on the box.
pub fn guide_glow_sized(mode: Mode, width: u32, height: u32) -> std::sync::Arc<Image> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<(bool, u32, u32), Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = (mode == Mode::Light, width, height);
    if let Some(image) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return image;
    }
    let rgba = color(mode, "--td-brand-color").to_rgb();
    let (w, h) = (width as f32, height as f32);
    let radius = ((w / 2.).powi(2) + h.powi(2)).sqrt() * 0.42;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><defs><radialGradient id="glow" gradientUnits="userSpaceOnUse" cx="{}" cy="0" r="{radius}"><stop stop-color="rgb({},{},{})" stop-opacity="0.12"/><stop offset="1" stop-color="rgb({},{},{})" stop-opacity="0"/></radialGradient></defs><rect width="{width}" height="{height}" fill="url(#glow)"/></svg>"##,
        w / 2.,
        rgba.r * 255.,
        rgba.g * 255.,
        rgba.b * 255.,
        rgba.r * 255.,
        rgba.g * 255.,
        rgba.b * 255.
    );
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()));
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, image.clone());
    }
    image
}
