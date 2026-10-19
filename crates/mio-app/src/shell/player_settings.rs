use super::*;
use gpui_kit::base::motion::{Transition, transition};

type SwitchHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

#[derive(IntoElement)]
struct PlayerStyleCard {
    id: &'static str,
    selected: bool,
    enabled: bool,
    accent: Hsla,
    button: gpui_kit::base::Button,
}

impl RenderOnce for PlayerStyleCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pointer = window.use_keyed_state((ElementId::from(self.id), "hover"), cx, |_, _| false);
        if !self.enabled && *pointer.read(cx) {
            pointer.update(cx, |hovered, _| *hovered = false);
        }
        let hovered = *pointer.read(cx);
        let timing = || {
            Transition::new(Duration::from_millis(300))
                .ease(|t| player_motion::easing(t, [0.25, 0.8, 0.25, 1.]))
        };
        let lift = transition(
            (self.id, "lift"),
            if hovered { -2_f32 } else { 0. },
            timing(),
            window,
            cx,
        );
        let background = transition(
            (self.id, "background"),
            white().opacity(if self.selected {
                0.15
            } else if hovered {
                0.1
            } else {
                0.05
            }),
            timing(),
            window,
            cx,
        );
        let border = transition(
            (self.id, "border"),
            self.accent
                .opacity(if self.selected { self.accent.a } else { 0. }),
            timing(),
            window,
            cx,
        );
        let shadow_alpha = transition(
            (self.id, "shadow"),
            if self.selected { 0.3_f32 } else { 0. },
            timing(),
            window,
            cx,
        );
        self.button
            .relative()
            .top(px(lift))
            .bg(background)
            .border_color(border)
            .shadow(vec![
                BoxShadow::new(px(0.), px(8.), black().opacity(shadow_alpha))
                    .blur_radius(px(20.))
                    .spread_radius(px(-5.)),
            ])
            .when(self.enabled, |button| {
                button.on_hover(move |hovered, _, cx| {
                    pointer.update(cx, |state, cx| {
                        *state = *hovered;
                        cx.notify();
                    });
                })
            })
    }
}

#[derive(Default)]
struct SwitchPointer {
    hovered: bool,
    pressed: bool,
}

#[derive(IntoElement)]
struct PlayerStyleEntry {
    button: gpui_kit::base::Button,
    size: f32,
    narrow: bool,
    enabled: bool,
    accent: Hsla,
    label: SharedString,
}

impl RenderOnce for PlayerStyleEntry {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = "player-style-entry";
        let pointer = window.use_keyed_state((ElementId::from(id), "pointer"), cx, |_, _| {
            SwitchPointer::default()
        });
        if !self.enabled && (pointer.read(cx).hovered || pointer.read(cx).pressed) {
            pointer.update(cx, |state, _| *state = SwitchPointer::default());
        }
        let hovered = pointer.read(cx).hovered;
        let pressed = pointer.read(cx).pressed;
        let timing = || {
            Transition::new(Duration::from_millis(if pressed { 120 } else { 280 })).ease(move |t| {
                player_motion::easing(
                    t,
                    if pressed {
                        [0.2, 0., 0., 1.]
                    } else {
                        [0.34, 1.35, 0.64, 1.]
                    },
                )
            })
        };
        let press = transition(
            (id, "press"),
            if pressed { 1_f32 } else { 0. },
            timing(),
            window,
            cx,
        );
        let scale = 1. - 0.08 * press;
        let offset = press;
        let background = transition(
            (id, "background"),
            if hovered {
                white().opacity(0.25)
            } else if self.narrow {
                black().opacity(0.22)
            } else {
                white().opacity(0.15)
            },
            timing(),
            window,
            cx,
        );
        let glow = transition(
            (id, "glow"),
            if hovered || pressed { 1_f32 } else { 0. },
            timing(),
            window,
            cx,
        )
        .clamp(0., 1.);
        let tooltip = transition(
            (id, "tooltip"),
            if hovered && !self.narrow { 1_f32 } else { 0. },
            Transition::new(Duration::from_millis(180))
                .ease(|t| player_motion::easing(t, [0.2, 0., 0., 1.])),
            window,
            cx,
        );
        let button_size = self.size * scale;
        let left = (self.size - button_size) * 0.5;
        let top = left + offset;
        let button = self
            .button
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(button_size))
            .rounded(px(button_size * 0.5))
            .border(px(scale))
            .bg(background)
            .shadow(vec![
                BoxShadow::new(
                    px(0.),
                    px((6. - 2. * press) * scale),
                    black().opacity((0.15 - 0.05 * press).clamp(0., 1.) * glow),
                )
                .blur_radius(px((16. - 6. * press) * scale)),
                BoxShadow::new(px(0.), px(0.), self.accent.opacity(self.accent.a * glow))
                    .blur_radius(px((16. - 6. * press) * scale)),
                BoxShadow::new(
                    px(0.),
                    px(0.),
                    white().opacity((0.4 - 0.3 * press).clamp(0., 1.) * glow),
                )
                .spread_radius(px(scale))
                .inset(),
            ])
            .child(icon("PenBall").size(px(20. * scale)))
            .when(self.enabled, |button| {
                let hover_state = pointer.clone();
                let down_state = pointer.clone();
                let up_state = pointer.clone();
                button
                    .on_hover(move |hovered, _, cx| {
                        hover_state.update(cx, |state, cx| {
                            state.hovered = *hovered;
                            cx.notify();
                        });
                    })
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        down_state.update(cx, |state, cx| {
                            state.pressed = true;
                            cx.notify();
                        });
                    })
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        up_state.update(cx, |state, cx| {
                            state.pressed = false;
                            cx.notify();
                        });
                    })
                    .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                        pointer.update(cx, |state, cx| {
                            state.pressed = false;
                            cx.notify();
                        });
                    })
            });
        div()
            .absolute()
            .bottom(px(0.))
            .right(px(0.))
            .size(px(self.size))
            .when(cfg!(target_os = "macos"), |view| {
                #[cfg(target_os = "macos")]
                {
                    view.child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                window.paint_backdrop(
                                    bounds,
                                    Corners::all(px(button_size * 0.5)),
                                    6. * scale,
                                    1.,
                                );
                            },
                        )
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .size(px(button_size)),
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    view
                }
            })
            .child(button)
            .when(!self.narrow && tooltip > 0., |view| {
                view.child(
                    div()
                        .absolute()
                        .bottom(px(-30. * scale + left - offset))
                        .w_full()
                        .flex()
                        .justify_center()
                        .opacity(tooltip)
                        .child(
                            gpui_kit::base::Tooltip::new("player-style-entry-tooltip")
                                .flex_shrink_0()
                                .text_size(px(12. * scale))
                                .line_height(relative(1.6))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(white())
                                .px(px(8. * scale))
                                .py(px(4. * scale))
                                .rounded(px(4. * scale))
                                .bg(black().opacity(0.75))
                                .child(self.label),
                        ),
                )
            })
    }
}

#[derive(IntoElement)]
struct PlayerSettingSwitch {
    id: &'static str,
    label: SharedString,
    checked: bool,
    disabled: bool,
    on_change: Option<SwitchHandler>,
}

impl PlayerSettingSwitch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PlayerSettingSwitch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = 20. / 0.618;
        let pointer = window.use_keyed_state((ElementId::from(self.id), "pointer"), cx, |_, _| {
            SwitchPointer::default()
        });
        if self.disabled && (pointer.read(cx).hovered || pointer.read(cx).pressed) {
            pointer.update(cx, |state, _| *state = SwitchPointer::default());
        }
        let hovered = pointer.read(cx).hovered;
        let pressed = pointer.read(cx).pressed;
        let timing = || {
            Transition::new(Duration::from_millis(200))
                .ease(|t| player_motion::easing(t, [0.38, 0., 0.24, 1.]))
        };
        let progress = transition(
            (self.id, "checked-motion"),
            if self.checked { 1_f32 } else { 0. },
            timing(),
            window,
            cx,
        );
        let extension = transition(
            (self.id, "press-motion"),
            if pressed { 6_f32 } else { 0. },
            timing(),
            window,
            cx,
        );
        let dark = Theme::global(cx).is_dark();
        let track: Hsla = if self.checked {
            white().opacity(0.15)
        } else {
            rgb(match (dark, hovered) {
                (false, false) => 0xdcdcdc,
                (false, true) => 0xc5c5c5,
                (true, false) => 0x4b4b4b,
                (true, true) => 0x5e5e5e,
            })
            .into()
        };
        let track = transition((self.id, "track-motion"), track, timing(), window, cx);
        let thumb_shadow = if dark {
            vec![
                BoxShadow::new(px(0.), px(4.), black().opacity(0.06)).blur_radius(px(6.)),
                BoxShadow::new(px(0.), px(1.), black().opacity(0.08)).blur_radius(px(10.)),
                BoxShadow::new(px(0.), px(2.), black().opacity(0.12)).blur_radius(px(4.)),
            ]
        } else {
            vec![
                BoxShadow::new(px(0.), px(1.), black().opacity(0.05)).blur_radius(px(10.)),
                BoxShadow::new(px(0.), px(4.), black().opacity(0.08)).blur_radius(px(5.)),
                BoxShadow::new(px(0.), px(2.), black().opacity(0.12))
                    .blur_radius(px(4.))
                    .spread_radius(px(-1.)),
            ]
        };
        let thumb_size = 12. + 3. * progress;
        let thumb_left = 4. + (width - 15. - 2.5 - 4.) * progress;
        let focus = window
            .use_keyed_state((ElementId::from(self.id), "focus"), cx, |_, cx| {
                cx.focus_handle()
            })
            .read(cx)
            .clone();
        gpui_kit::base::Switch::new(self.id)
            .checked(self.checked)
            .disabled(self.disabled)
            .tab_stop(!self.disabled)
            .accessibility_label(self.label)
            .track_focus(&focus)
            .relative()
            .w(px(width))
            .h(px(20.))
            .flex_shrink_0()
            .rounded(px(10.))
            .bg(track)
            .when(!self.disabled, |view| {
                let hover_state = pointer.clone();
                let down_state = pointer.clone();
                let up_state = pointer.clone();
                view.on_hover(move |hovered, _, cx| {
                    hover_state.update(cx, |state, cx| {
                        state.hovered = *hovered;
                        cx.notify();
                    });
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    down_state.update(cx, |state, cx| {
                        state.pressed = true;
                        cx.notify();
                    });
                })
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    up_state.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
                .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                    pointer.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                })
            })
            .cursor_pointer()
            .when(self.disabled, |view| view.cursor_not_allowed())
            .when(focus.is_focused(window), |view| {
                view.shadow(vec![
                    BoxShadow::new(px(0.), px(0.), white().opacity(0.6)).spread_radius(px(2.)),
                ])
            })
            .when_some(self.on_change, |view, handler| {
                view.on_change(move |value, _, window, cx| handler(&value, window, cx))
            })
            .child(
                div()
                    .absolute()
                    .left(px(thumb_left))
                    .top(px(4. - 1.5 * progress))
                    .size(px(thumb_size))
                    .rounded(px(thumb_size * 0.5))
                    .shadow(thumb_shadow)
                    .child(
                        div()
                            .absolute()
                            .left(px(if self.checked { -extension } else { 0. }))
                            .w(px(thumb_size + extension))
                            .h(px(thumb_size))
                            .rounded(px(thumb_size * 0.5))
                            .bg(white()),
                    ),
            )
    }
}

impl Mio {
    pub(super) fn set_player_settings_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.player_settings_open = open;
        if open {
            self.player_idle.hidden = false;
        } else {
            self.player_focus.focus(window, cx);
            self.wake_player(cx);
        }
        cx.notify();
    }

    pub(super) fn player_settings(
        &self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.player_settings_open;
        let panel_visibility = self.player_motion.settings_visibility;
        let show_panel = open || panel_visibility > 0.;
        let narrow = width <= 768.;
        let footer = mio_tokens::player_height(height);
        let gutter = if narrow {
            (width * 0.04).clamp(16., 24.)
        } else {
            20.
        };
        let button_size = if narrow { 44. } else { 50. };
        let panel_bottom = if narrow { button_size + 10. } else { 70. };
        let panel_width = if narrow { width - gutter * 2. } else { 340. };
        let root_font_size = mio_tokens::rem_size(width);
        let panel_height = (height
            - footer
            - if narrow {
                120.
            } else {
                170. + 2.25 * root_font_size
            })
        .max(80.);
        let padding = if narrow { 16. } else { 20. };
        let accent = self
            .theme_palette()
            .map(|p| {
                let main = p.main.to_rgb();
                Hsla::from(Rgba {
                    r: (main.r * 51. + 204.).round() / 255.,
                    g: (main.g * 51. + 204.).round() / 255.,
                    b: (main.b * 51. + 204.).round() / 255.,
                    a: 0.9,
                })
            })
            .unwrap_or(white().opacity(0.9));
        let heading = |key: &str| {
            div()
                .text_size(px(18.))
                .line_height(px(28.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(white().opacity(0.95))
                .mb(px(20.))
                .child(self.t(key))
        };
        let card = |cover: bool, key: &str, path: &'static str| {
            let preview_width = (panel_width - padding * 2. - 16.) * 0.5 - 36.;
            let id = if cover {
                "player-style-cover"
            } else {
                "player-style-vinyl"
            };
            let button = gpui_kit::base::Button::new(id)
                .accessibility_label(self.t(key))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .p(px(16.))
                .gap(px(12.))
                .border_2()
                .rounded(px(16.))
                .disabled(!open)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cover_layout = cover;
                    this.save_preferences();
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(preview_width + 20.))
                        .p(px(10.))
                        .rounded(px(10.))
                        .overflow_hidden()
                        .bg(black().opacity(0.2))
                        .child(
                            img(path)
                                .w(px(preview_width))
                                .h(px(preview_width
                                    * if cover { 1135. / 1772. } else { 1118. / 1766. }))
                                .object_fit(ObjectFit::Contain),
                        ),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(white().opacity(0.8))
                        .child(self.t(key)),
                );
            PlayerStyleCard {
                id,
                selected: self.cover_layout == cover,
                enabled: open,
                accent,
                button,
            }
        };
        let switch = |id: &'static str, key: &str, checked: bool| PlayerSettingSwitch {
            id,
            label: self.t(key).into(),
            checked,
            disabled: !open,
            on_change: None,
        };
        let row = |key: &str, control: AnyElement| {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.))
                .py(px(4.))
                .text_size(px(14.))
                .line_height(px(22.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0xf1f1f1).opacity(0.8))
                .child(self.t(key))
                .child(control)
        };
        let idle = if narrow {
            0.
        } else {
            self.player_motion.idle_visibility
        };
        div()
            .id("player-float-action")
            .absolute()
            .right(px(gutter))
            .bottom(px(if narrow {
                mio_tokens::player_height(height) + 12.
            } else {
                60.
            } - 20. * idle))
            .w(px(if show_panel { panel_width } else { button_size }))
            .h(px(if show_panel {
                panel_height + panel_bottom
            } else {
                button_size
            }))
            .opacity((1. - idle).clamp(0., 1.))
            .when(idle >= 1., |view| view.invisible())
            .when(open, |view| {
                view.on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.set_player_settings_open(false, window, cx);
                }))
            })
            .child(PlayerStyleEntry {
                size: button_size,
                narrow,
                enabled: narrow || !self.player_idle.hidden,
                accent,
                label: self.t("play.playerTheme").into(),
                button: gpui_kit::base::Button::new("player-style-settings")
                    .accessibility_label(self.t("play.playerTheme"))
                    .border_1()
                    .border_color(white().opacity(if narrow { 0.18 } else { 0.628 }))
                    .text_color(white().opacity(0.9))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .disabled(!narrow && self.player_idle.hidden)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_player_settings_open(!this.player_settings_open, window, cx);
                    })),
            })
            .when(show_panel, |view| {
                let view = view.when(cfg!(target_os = "macos"), |view| {
                    #[cfg(target_os = "macos")]
                    {
                        view.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    window.paint_backdrop(
                                        bounds,
                                        Corners::all(px(if narrow { 22. } else { 24. })),
                                        6.,
                                        1.,
                                    );
                                },
                            )
                            .absolute()
                            .right(px(0.))
                            .bottom(px(panel_bottom - 20. * (1. - panel_visibility)))
                            .w(px(panel_width))
                            .h(px(panel_height))
                            .opacity(panel_visibility.clamp(0., 1.)),
                        )
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        view
                    }
                });
                view.child(
                    div()
                        .id("player-style-panel")
                        .absolute()
                        .right(px(0.))
                        .bottom(px(panel_bottom - 20. * (1. - panel_visibility)))
                        .opacity(panel_visibility.clamp(0., 1.))
                        .w(px(panel_width))
                        .h(px(panel_height))
                        .rounded(px(if narrow { 22. } else { 24. }))
                        .p(px(padding))
                        .font_family(".SystemUIFont")
                        .bg(rgb(0x1e1e1e).opacity(0.29))
                        .shadow(vec![
                            BoxShadow::new(px(0.), px(6.), black().opacity(0.4))
                                .blur_radius(px(12.)),
                            BoxShadow::new(px(0.), px(0.), white().opacity(0.1))
                                .spread_radius(px(1.)),
                        ])
                        .child(
                            div()
                                .id("player-style-scroll")
                                .size_full()
                                .overflow_y_scroll()
                                .track_scroll(&self.player_settings_scroll)
                                .child(heading("play.playerStyle"))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(16.))
                                        .child(card(false, "play.classicVinyl", "images/cd.png"))
                                        .child(card(
                                            true,
                                            "play.immersiveCover",
                                            "images/cover-play.png",
                                        )),
                                )
                                .child(div().mt(px(24.)).child(heading("play.uiSettings")))
                                .child(row(
                                    "play.showLeftPanel",
                                    switch(
                                        "player-show-left",
                                        "play.showLeftPanel",
                                        self.show_left_panel,
                                    )
                                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                                        this.show_left_panel = *value;
                                        this.save_preferences();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                ))
                                .child(row(
                                    "play.immersiveLyricColor",
                                    switch(
                                        "player-immersive-lyrics",
                                        "play.immersiveLyricColor",
                                        self.immersive_lyric_color,
                                    )
                                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                                        this.immersive_lyric_color = *value;
                                        this.save_preferences();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                ))
                                .child(row(
                                    "play.lyricBlurEffect",
                                    switch("player-blur-lyrics", "play.lyricBlurEffect", self.blur_lyrics)
                                        .on_click(cx.listener(|this, value: &bool, _, cx| {
                                            this.blur_lyrics = *value;
                                            this.save_preferences();
                                            cx.notify();
                                        }))
                                        .into_any_element(),
                                ))
                                .child(row(
                                    "play.audioVisualization",
                                    switch("player-visualizer", "play.audioVisualization", self.audio_visualization)
                                        .on_click(cx.listener(|this,value: &bool,_,cx| {this.audio_visualization=*value;this.save_preferences();cx.notify();}))
                                        .into_any_element(),
                                ))
                                .child(row(
                                    "play.autoHideControlBar",
                                    switch(
                                        "player-auto-hide",
                                        "play.autoHideControlBar",
                                        self.auto_hide_bottom,
                                    )
                                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                                        this.set_auto_hide_bottom(*value, cx)
                                    }))
                                    .into_any_element(),
                                ))
                                .child(div().mt(px(24.)).child(heading("play.playbackSettings")))
                                .child(row(
                                    "play.pauseTransition",
                                    switch(
                                        "player-pause-transition",
                                        "play.pauseTransition",
                                        self.pause_transition,
                                    )
                                    .on_click(cx.listener(|this, value: &bool, _, cx| {
                                        // This preference is also unused by the current reference player.
                                        this.pause_transition = *value;
                                        this.save_preferences();
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                                ))
                                .child(row(
                                    "play.seamlessSwitch",
                                    switch("player-gapless", "play.seamlessSwitch", self.player.seamless.enabled)
                                        .on_click(cx.listener(|this, value: &bool, _, cx| {
                                            this.player.seamless.enabled = *value;
                                            this.prepare_seamless_next();
                                            this.save_preferences();
                                            cx.notify();
                                        }))
                                        .into_any_element(),
                                ))
                                .when(self.player.seamless.enabled, |column| {
                                    let mode_button = |crossfade: bool, id: &'static str, key: &str, cx: &Context<Self>| {
                                        gpui_kit::base::Button::new(id)
                                            .accessibility_label(self.t(key))
                                            .px(px(8.)).py(px(5.)).rounded(px(4.))
                                            .bg(white().opacity(if self.player.seamless.crossfade == crossfade { 0.2 } else { 0.07 }))
                                            .text_color(white()).cursor_pointer().child(self.t(key))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.player.seamless.crossfade = crossfade;
                                                this.prepare_seamless_next();
                                                this.save_preferences();
                                                cx.notify();
                                            }))
                                    };
                                    column.child(row("play.transitionMode", div().flex().gap(px(4.))
                                        .child(mode_button(false, "seamless-gapless", "play.gaplessMode", cx))
                                        .child(mode_button(true, "seamless-crossfade", "play.crossfadeMode", cx)).into_any_element()))
                                        .when(self.player.seamless.crossfade, |column| {
                                            column.child(div().flex().items_center().justify_between().gap(px(12.)).py(px(8.))
                                                .text_color(white().opacity(0.9)).text_size(px(14.))
                                                .child(self.t("play.transitionDuration").replace("{duration}", &format!("{:.1}", self.player.seamless.duration_ms as f64 / 1000.)))
                                                .child(gpui_kit::component::slider::Slider::new(&self.crossfade_duration).w(px(160.))))
                                        })
                                })
                                .child(div().mt(px(24.)).child(heading("play.lyricSettings")))
                                .child(row(
                                    "play.filterLyricSongInfo",
                                    switch(
                                        "player-filter-lyrics",
                                        "play.filterLyricSongInfo",
                                        false,
                                    )
                                    .disabled(true)
                                    .into_any_element(),
                                ))
                                .child(row(
                                    "play.strictFilterMode",
                                    switch("player-strict-lyrics", "play.strictFilterMode", false)
                                        .disabled(true)
                                        .into_any_element(),
                                )),
                        ),
                )
            })
    }
}
