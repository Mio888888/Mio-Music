use super::*;
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::{
    WindowExt,
    button::ButtonCustomVariant,
    dialog::DialogFooter,
    notification::Notification,
    scroll::{Scrollbar, ScrollbarMode},
};

pub(super) struct QueuePress {
    index: usize,
    revision: u64,
    started: Instant,
    start_y: Pixels,
    y: Pixels,
    eligible: bool,
    sorting: bool,
    changed: bool,
}

const TIP_DURATION: f32 = 0.2;
const TIP_CURVE: [f32; 4] = [0.25, 0.1, 0.25, 1.];

pub(super) struct QueueTip {
    index: Option<usize>,
    revision: u64,
    opacity: player_motion::Tween,
    visibility: f32,
}
impl Default for QueueTip {
    fn default() -> Self {
        Self {
            index: None,
            revision: 0,
            opacity: player_motion::Tween::new(0.),
            visibility: 0.,
        }
    }
}

impl Mio {
    pub(super) fn update_queue_tip(
        &mut self,
        desktop: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let sorting = self.queue_press.as_ref().is_some_and(|press| press.sorting);
        let enabled = desktop && self.queue_open && window.is_window_active() && !sorting;
        let target = if enabled
            && self
                .queue_hover_since
                .is_some_and(|since| now.duration_since(since) >= Duration::from_millis(1500))
        {
            self.queue_hover
        } else {
            None
        };
        let tip = &mut self.queue_tip;
        if !enabled || tip.revision != self.player.queue_revision {
            *tip = QueueTip::default();
            tip.revision = self.player.queue_revision;
        }
        if let Some(index) = target
            && tip.index != Some(index)
        {
            tip.opacity = player_motion::Tween::new(0.);
            tip.index = Some(index);
        }
        tip.opacity.target(
            if target.is_some() { 1. } else { 0. },
            now,
            TIP_DURATION,
            TIP_CURVE,
            cx.reduce_motion(),
        );
        tip.visibility = tip.opacity.value(now, TIP_DURATION, TIP_CURVE);
        if tip.opacity.active(now, TIP_DURATION) {
            window.request_animation_frame();
        } else if target.is_none() {
            tip.index = None;
        }
    }

    fn move_queue_pointer(&mut self, y: Pixels) {
        if let Some(press) = self.queue_press.as_mut() {
            if press.eligible && press.started.elapsed() >= Duration::from_millis(500) {
                press.sorting = true;
            }
            if !press.sorting && f32::from(y - press.start_y).abs() > 10. {
                press.eligible = false;
            }
            press.y = y;
            if press.sorting {
                self.queue_hover = None;
                self.queue_hover_since = None;
            }
        }
    }

    fn finish_queue_gesture(&mut self, play: bool, cx: &mut Context<Self>) {
        if let Some(press) = self.queue_press.take()
            && press.revision == self.player.queue_revision
        {
            if press.changed {
                self.save_session();
            } else if play && press.eligible && !press.sorting && self.queue_open {
                self.play_index(press.index, cx);
            }
        }
        self.queue_hover = None;
        self.queue_hover_since = None;
        cx.notify();
    }

    pub(super) fn update_queue_gesture(
        &mut self,
        auto_scroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        if !window.is_window_active() || !self.queue_open {
            if self.queue_press.is_some() {
                self.finish_queue_gesture(false, cx);
            }
            self.queue_hover_since = None;
            return;
        }
        if let Some(press) = self.queue_press.as_ref()
            && press.revision != self.player.queue_revision
        {
            self.finish_queue_gesture(false, cx);
            return;
        }
        if let Some(press) = self.queue_press.as_mut() {
            if press.eligible && now.duration_since(press.started) >= Duration::from_millis(500) {
                press.sorting = true;
                self.queue_hover = None;
                self.queue_hover_since = None;
            }
            if press.sorting {
                let scroll = self.queue_scroll.0.borrow().base_handle.clone();
                let bounds = scroll.bounds();
                let top = f32::from(press.y - bounds.top());
                let bottom = f32::from(bounds.bottom() - press.y);
                let speed = if !auto_scroll {
                    0.
                } else if top > 0. && top < 80. {
                    -(80. - top) / 80. * 15.
                } else if bottom > 0. && bottom < 80. {
                    (80. - bottom) / 80. * 15.
                } else {
                    0.
                };
                let mut offset = scroll.offset();
                offset.y = (offset.y - px(speed)).clamp(-scroll.max_offset().y.max(px(0.)), px(0.));
                scroll.set_offset(offset);
                let height = self
                    .queue_scroll
                    .0
                    .borrow()
                    .last_item_size
                    .filter(|_| !self.player.queue.is_empty())
                    .map(|size| size.contents.height / self.player.queue.len() as f32);
                if let Some(height) = height.filter(|height| *height > px(0.)) {
                    let y = f32::from(press.y - bounds.top() - scroll.offset().y);
                    let boundary = (y / f32::from(height) - 0.5)
                        .ceil()
                        .clamp(0., self.player.queue.len() as f32)
                        as usize;
                    if let Some(index) = self.player.move_in_queue(press.index, boundary) {
                        press.changed |= index != press.index;
                        press.index = index;
                        press.revision = self.player.queue_revision;
                    }
                }
            }
            if auto_scroll && press.eligible {
                window.request_animation_frame();
            }
        }
        if auto_scroll
            && f32::from(window.viewport_size().width) > 768.
            && self
                .queue_hover_since
                .is_some_and(|since| now.duration_since(since) < Duration::from_millis(1500))
        {
            window.request_animation_frame();
        }
    }

    fn queue_colors(&self) -> QueueColors {
        let colors = self.ui_colors();
        QueueColors {
            primary: color(colors, "--td-text-color-primary"),
            secondary: color(colors, "--td-text-color-secondary"),
            muted: color(colors, "--td-text-color-placeholder"),
            brand: color(colors, "--td-brand-color"),
            surface: color(colors, "--td-bg-color-container"),
            mobile_surface: color(colors, "--mobile-glass-bg-strong"),
            mobile_border: color(colors, "--mobile-glass-border"),
        }
    }

    pub(super) fn toggle_queue(&mut self, cx: &mut Context<Self>) {
        self.finish_queue_gesture(false, cx);
        self.queue_open = !self.queue_open;
        self.queue_hover = None;
        self.volume_open = false;
        if self.queue_open {
            self.locate_queue_song();
        }
        cx.notify();
    }

    fn locate_queue_song(&self) {
        if let Some(index) = self.player.queue_position() {
            self.queue_scroll
                .scroll_to_item_strict(index, ScrollStrategy::Center);
        }
    }

    fn locate_queue_with_feedback(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.player.song().is_none() {
            window.push_notification(Notification::info(self.t("play.noSongPlaying")), cx);
        } else if self.player.queue_position().is_none() {
            window.push_notification(
                Notification::warning(self.t("play.currentSongNotInPlaylist")),
                cx,
            );
        } else {
            self.locate_queue_song();
        }
        cx.notify();
    }

    fn confirm_clear_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.player.queue.is_empty() {
            window.push_notification(
                Notification::warning(self.t("play.playlistAlreadyEmpty")),
                cx,
            );
            return;
        }
        self.toggle_queue(cx);
        let revision = self.player.queue_revision;
        let view = cx.entity().downgrade();
        let title = self.t("play.clearPlaylistConfirmTitle");
        let body = self.t("play.clearPlaylistConfirmBody");
        let confirm = self.t("play.confirmClear");
        let cancel = self.t("common.cancel");
        let colors = self.queue_colors();
        let dark_mask = if self.mode == Mode::Dark { 0.4 } else { 0.6 };
        let border = colors
            .primary
            .opacity(if self.mode == Mode::Dark { 0.12 } else { 0.1 });
        let mut close_surface = colors.surface;
        close_surface.a *= 0.6;
        let mut cancel_surface = colors.surface;
        cancel_surface.a *= 0.7;
        let cancel_style = ButtonCustomVariant::new(cx)
            .color(cancel_surface)
            .foreground(colors.primary)
            .hover(color(self.ui_colors(), "--td-bg-color-container-hover"))
            .active(color(self.ui_colors(), "--td-bg-color-container-active"));
        // A solid fill with a white label: the fixed red, not the cover-adjusted one.
        let danger = color(self.ui_colors(), "--td-error-color-6");
        // White on red regardless of the cover: `anti` follows the brand fill.
        let anti = color(self.ui_colors(), "--td-font-white-1");
        let danger_style = ButtonCustomVariant::new(cx)
            .color(danger)
            .foreground(anti)
            .hover(color(self.ui_colors(), "--td-error-color-hover"))
            .active(color(self.ui_colors(), "--td-error-color-active"));
        let clear = Rc::new(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |this, cx| {
                // A delayed confirmation must never clear a replacement queue.
                if this.player.queue_revision == revision {
                    this.clear_player_queue();
                    this.queue_hover = None;
                    this.save_session();
                    window.push_notification(
                        Notification::success(this.t("play.playlistCleared")),
                        cx,
                    );
                    cx.notify();
                }
            });
            true
        });
        window.open_dialog(cx, move |dialog, window, _| {
            let on_ok = clear.clone();
            let on_click = clear.clone();
            let body = body.clone();
            let viewport = window.viewport_size();
            let narrow = viewport.width <= px(768.);
            let gutter = (f32::from(viewport.width) * 0.04).clamp(16., 24.);
            dialog
                .w(px(if narrow {
                    (f32::from(viewport.width) - gutter * 2.).min(420.)
                } else {
                    480.
                }))
                .margin_top(window.viewport_size().height * 0.2)
                .rounded(px(if narrow { 22. } else { 8. }))
                .p(px(if narrow { 16. } else { 32. }))
                .when(narrow, |dialog| dialog.pt(px(20.)).bottom_offset(px(12.)))
                .gap(px(if narrow { 12. } else { 32. }))
                .bg(if narrow {
                    colors.mobile_surface
                } else {
                    colors.surface
                })
                .border_color(if narrow { colors.mobile_border } else { border })
                .shadow(if narrow {
                    vec![
                        BoxShadow {
                            color: rgb(0x0f172a).opacity(0.18).into(),
                            offset: point(px(0.), px(18.)),
                            blur_radius: px(42.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                        BoxShadow {
                            color: rgb(0x0f172a).opacity(0.08).into(),
                            offset: point(px(0.), px(4.)),
                            blur_radius: px(12.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                    ]
                } else {
                    vec![]
                })
                .overlay_tint(black().opacity(if narrow { 0.28 } else { dark_mask }))
                .when(cfg!(target_os = "macos"), |dialog| {
                    dialog
                        .overlay_filter(
                            if narrow { 12. } else { 5. },
                            if narrow { 1.8 } else { 1. },
                        )
                        .when(narrow, |dialog| dialog.surface_filter(12., 1.8, px(22.)))
                })
                .text_color(colors.primary)
                .close_button_style(
                    px(if narrow { 10. } else { 32. }),
                    px(if narrow { 10. } else { 32. }),
                    px(if narrow { 18. } else { 16. }),
                    {
                        let mut style = div()
                            .size(px(if narrow { 44. } else { 24. }))
                            .border_0()
                            .rounded_full()
                            .text_color(colors.secondary)
                            .when(narrow, |button| button.bg(close_surface));
                        style.style().clone()
                    },
                )
                .title(
                    div()
                        .relative()
                        .when(narrow, |header| {
                            header.child(
                                div()
                                    .w(px(38.))
                                    .h(px(4.))
                                    .mt(px(0.))
                                    .mb(px(12.))
                                    .mx_auto()
                                    .rounded_full()
                                    .bg(rgb(0x787880).opacity(0.36)),
                            )
                        })
                        .text_size(px(16.))
                        .font_weight(FontWeight::NORMAL)
                        .line_height(relative(1.5))
                        .child(title.clone()),
                )
                .content(move |content, _, _| {
                    content
                        .mt(px(if narrow { 0. } else { 8. }))
                        .when(narrow, |content| content.py(px(14.)))
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(colors.secondary)
                        .child(body.clone())
                })
                .footer(
                    DialogFooter::new()
                        .gap(px(if narrow { 16. } else { 8. }))
                        .child(
                            Button::new("cancel-clear-queue")
                                .label(cancel.clone())
                                .custom(cancel_style)
                                .border_color(border)
                                .h(px(if narrow { 44. } else { 32. }))
                                .px(px(16.))
                                .text_size(px(14.))
                                .font_weight(if narrow {
                                    FontWeight::MEDIUM
                                } else {
                                    FontWeight::NORMAL
                                })
                                .rounded(px(if narrow { 999. } else { 3. }))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("confirm-clear-queue")
                                .label(confirm.clone())
                                .custom(danger_style)
                                .bg(danger)
                                .h(px(if narrow { 44. } else { 32. }))
                                .px(px(16.))
                                .text_size(px(14.))
                                .font_weight(if narrow {
                                    FontWeight::MEDIUM
                                } else {
                                    FontWeight::NORMAL
                                })
                                .rounded(px(if narrow { 999. } else { 3. }))
                                .on_click(move |event, window, cx| {
                                    if on_click(event, window, cx) {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
                .on_ok(move |event, window, cx| on_ok(event, window, cx))
        });
    }

    pub(super) fn queue_drawer(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> Div {
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        let mobile = width <= 768.;
        let gutter = (width * 0.04).clamp(16., 24.);
        let panel_width = if mobile { width } else { 380. };
        let panel_height = if mobile {
            (height * 0.76).min(620.).min((height - 12.).max(0.))
        } else {
            (height - mio_tokens::player_height(height) - 80.).max(0.)
        };
        let visibility = self.player_motion.queue_visibility;
        let mode = self.ui_colors();
        let colors = self.queue_colors();
        let primary = colors.primary;
        let secondary = colors.secondary;
        let brand = colors.brand;
        let error = color(mode, "--td-error-color");
        let surface = colors.surface;
        let background = if mobile {
            colors.mobile_surface
        } else {
            surface.opacity(if self.full_player_open { 0.2 } else { 0.6 })
        };
        let open = self.queue_open;
        let gesture_view = cx.entity().downgrade();
        let panel = div()
            .id("playback-queue")
            .occlude()
            .absolute()
            .w(px(panel_width))
            .h(px(panel_height))
            .when(mobile, |panel| {
                panel
                    .right_0()
                    .bottom(px(-panel_height * (1. - visibility)))
                    .rounded_t(px(22.))
                    .border_t(px(0.5))
                    .border_l(px(0.5))
                    .border_r(px(0.5))
                    .border_color(colors.mobile_border)
                    .shadow(vec![
                        BoxShadow {
                            color: rgb(0x0f172a).opacity(0.18).into(),
                            offset: point(px(0.), px(18.)),
                            blur_radius: px(42.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                        BoxShadow {
                            color: rgb(0x0f172a).opacity(0.08).into(),
                            offset: point(px(0.), px(4.)),
                            blur_radius: px(12.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                    ])
            })
            .when(!mobile, |panel| {
                panel
                    .top(px(72.))
                    .right(px(-panel_width * (1. - visibility)))
                    .rounded_l(px(16.))
                    .shadow(vec![BoxShadow {
                        color: primary.opacity(0.15),
                        offset: point(px(-3.), px(0.)),
                        blur_radius: px(12.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
            })
            .bg(background)
            .text_color(primary)
            .line_height(relative(1.6))
            .overflow_hidden()
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .when(mobile, |panel| {
                panel.child(
                    div()
                        .w(px(38.))
                        .h(px(4.))
                        .mt(px(8.))
                        .flex_shrink_0()
                        .self_center()
                        .rounded(px(999.))
                        .bg(rgba(0x7878805c)),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(if mobile { gutter } else { 16. }))
                    .py(px(if mobile { 12. } else { 16. }))
                    .border_b_1()
                    .border_color(primary.opacity(0.1))
                    .child(
                        div()
                            .text_size(px(if mobile { 17. } else { 16. }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "{} ({})",
                                self.t("play.playlistTitle"),
                                self.player.queue.len()
                            )),
                    )
                    .child(
                        gpui_kit::base::Button::new("close-queue")
                            .accessibility_label(self.t("common.close"))
                            .disabled(!open)
                            .size(px(if mobile { 44. } else { 24. }))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(secondary)
                            .cursor_pointer()
                            .when(mobile, |button| {
                                button.rounded(px(999.)).bg(rgba(0x7878801f))
                            })
                            .child(glyph("guanbi").size(px(16.)))
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_queue(cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .my(px(if mobile { 6. } else { 10. }))
                    .px(px(if mobile { 10. } else { 8. }))
                    .when(self.player.queue.is_empty(), |content| {
                        content.child(
                            div()
                                .h(px(if mobile { 180. } else { 100. }))
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .text_center()
                                .text_size(px(14.))
                                .text_color(secondary)
                                .gap(px(14.))
                                .child(self.t("play.playlistEmpty"))
                                .child(self.t("play.playlistEmptyTip")),
                        )
                    })
                    .when(!self.player.queue.is_empty(), |content| {
                        content.child(
                            uniform_list(
                                "queue-items",
                                self.player.queue.len(),
                                cx.processor(
                                    move |this, range: std::ops::Range<usize>, window, cx| {
                                        range
                                            .map(|index| this.queue_row(index, mobile, window, cx))
                                            .collect::<Vec<_>>()
                                    },
                                ),
                            )
                            .track_scroll(&self.queue_scroll)
                            .when(
                                self.queue_scroll.0.borrow().base_handle.max_offset().y > px(0.),
                                |list| list.pr(px(8.)),
                            )
                            .size_full(),
                        )
                    })
                    .when(open && !self.player.queue.is_empty(), |content| {
                        content.child(
                            Scrollbar::vertical(&self.queue_scroll)
                                .id("queue-scrollbar")
                                .mode(ScrollbarMode::Always)
                                .styles(|styles| {
                                    let thumb = if self.full_player_open {
                                        primary
                                    } else {
                                        colors.muted
                                    };
                                    let normal = thumb.opacity(0.3);
                                    let hover = if self.full_player_open {
                                        thumb.opacity(0.5)
                                    } else {
                                        normal
                                    };
                                    styles
                                        .track(|style| {
                                            style
                                                .width(px(8.))
                                                .bg(transparent_black())
                                                .border_color(transparent_black())
                                        })
                                        .track_hover(|style| {
                                            style
                                                .bg(transparent_black())
                                                .border_color(transparent_black())
                                        })
                                        .track_active(|style| {
                                            style
                                                .bg(transparent_black())
                                                .border_color(transparent_black())
                                        })
                                        .thumb(|style| {
                                            style
                                                .bg(normal)
                                                .width(px(8.))
                                                .inset(px(0.))
                                                .radius(px(4.))
                                        })
                                        .thumb_hover(|style| {
                                            style
                                                .bg(hover)
                                                .width(px(8.))
                                                .inset(px(0.))
                                                .radius(px(4.))
                                        })
                                        .thumb_active(|style| {
                                            style
                                                .bg(hover)
                                                .width(px(8.))
                                                .inset(px(0.))
                                                .radius(px(4.))
                                        })
                                }),
                        )
                    }),
            )
            .when(!self.player.queue.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .gap(px(8.))
                        .px(px(if mobile { gutter } else { 16. }))
                        .pt(px(if mobile { 10. } else { 12. }))
                        .pb(px(12.))
                        .border_t_1()
                        .border_color(primary.opacity(0.1))
                        .child(queue_action_glass(
                            queue_action(
                                "locate-queue",
                                self.t("play.locateCurrentPlay"),
                                brand,
                                mobile,
                                None,
                            )
                            .disabled(!open)
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.locate_queue_with_feedback(window, cx);
                                },
                            )),
                            mobile,
                        ))
                        .child(queue_action_glass(
                            queue_action(
                                "clear-queue",
                                self.t("play.clearPlaylist"),
                                error,
                                mobile,
                                Some("TD-Delete"),
                            )
                            .disabled(!open)
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.confirm_clear_queue(window, cx);
                                },
                            )),
                            mobile,
                        )),
                )
            });
        div()
            .absolute()
            .size_full()
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let view = gesture_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase.capture() {
                                let _ = view.update(cx, |this, cx| {
                                    if this.queue_press.is_some() {
                                        if event.pressed_button != Some(MouseButton::Left) {
                                            this.finish_queue_gesture(false, cx);
                                        } else {
                                            this.move_queue_pointer(event.position.y);
                                            cx.notify();
                                        }
                                    }
                                });
                            }
                        });
                        let view = gesture_view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase.capture() && event.button == MouseButton::Left {
                                let _ = view.update(cx, |this, cx| {
                                    if this.queue_press.is_some() {
                                        this.move_queue_pointer(event.position.y);
                                        this.update_queue_gesture(false, window, cx);
                                        this.finish_queue_gesture(true, cx);
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .when(mobile && cfg!(target_os = "macos"), |root| {
                #[cfg(target_os = "macos")]
                {
                    root.child(
                        canvas(
                            |_, _, _| (),
                            |bounds, _, window, _| {
                                window.paint_backdrop(bounds, Corners::all(px(0.)), 12., 1.8);
                            },
                        )
                        .absolute()
                        .size_full()
                        .opacity(visibility),
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    root
                }
            })
            .child(
                div()
                    .id("queue-mask")
                    .occlude()
                    .absolute()
                    .size_full()
                    .bg(black().opacity(if mobile { 0.28 * visibility } else { 0. }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.queue_open {
                            this.toggle_queue(cx);
                        }
                        cx.stop_propagation();
                    })),
            )
            .when(cfg!(target_os = "macos"), |root| {
                #[cfg(target_os = "macos")]
                {
                    root.child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                let radii = if mobile {
                                    Corners {
                                        top_left: px(22.),
                                        top_right: px(22.),
                                        bottom_left: px(0.),
                                        bottom_right: px(0.),
                                    }
                                } else {
                                    Corners {
                                        top_left: px(16.),
                                        bottom_left: px(16.),
                                        top_right: px(0.),
                                        bottom_right: px(0.),
                                    }
                                };
                                window.paint_backdrop(
                                    bounds,
                                    radii,
                                    if mobile { 12. } else { 8. },
                                    if mobile { 1.8 } else { 1. },
                                );
                            },
                        )
                        .absolute()
                        .w(px(panel_width))
                        .h(px(panel_height))
                        .when(mobile, |glass| {
                            glass
                                .right_0()
                                .bottom(px(-panel_height * (1. - visibility)))
                        })
                        .when(!mobile, |glass| {
                            glass
                                .top(px(72.))
                                .right(px(-panel_width * (1. - visibility)))
                        }),
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    root
                }
            })
            .child(panel)
    }

    fn queue_row(
        &self,
        index: usize,
        mobile: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let song = &self.player.queue[index];
        let mode = self.ui_colors();
        let colors = self.queue_colors();
        let primary = colors.primary;
        let secondary = colors.secondary;
        let placeholder = colors.muted;
        let brand = colors.brand;
        let active = self.player.queue_position() == Some(index);
        let full_player = self.full_player_open;
        let hovered = self.queue_hover == Some(index);
        let sorting = self.queue_press.as_ref().is_some_and(|press| press.sorting);
        let dragging = self
            .queue_press
            .as_ref()
            .is_some_and(|press| press.sorting && press.index == index);
        let tip = !mobile && self.queue_tip.index == Some(index) && self.queue_tip.visibility > 0.;
        let action_visibility = if mobile {
            1.
        } else {
            transition(
                format!("queue-actions-motion-{index}"),
                if hovered { 1_f32 } else { 0. },
                Transition::new(Duration::from_millis(200))
                    .ease(|t| player_motion::easing(t, TIP_CURVE)),
                window,
                cx,
            )
        };
        let row_background = transition(
            format!("queue-background-motion-{index}"),
            if active {
                brand.opacity(0.12)
            } else if hovered {
                primary.opacity(if full_player { 0.1 } else { 0.08 })
            } else {
                transparent_black()
            },
            Transition::new(Duration::from_millis(300))
                .ease(|t| player_motion::easing(t, [0.4, 0., 0.2, 1.])),
            window,
            cx,
        );
        div()
            .h(px(if mobile { 68. } else { 66. }))
            .py(px(if mobile { 4. } else { 5. }))
            .child(
                gpui_kit::base::Button::new(SharedString::from(format!("queue-song-{index}")))
                    .accessibility_label(format!("{} · {}", song.title, song.artist))
                    .disabled(!self.queue_open)
                    .size_full()
                    .relative()
                    .flex()
                    .items_center()
                    .px(px(if mobile { 12. } else { 16. }))
                    .rounded(px(if mobile { 16. } else { 10. }))
                    .cursor_pointer()
                    .text_color(primary)
                    .bg(row_background)
                    .when(active, |row| {
                        row.border_l(px(4.))
                            .border_color(brand.opacity(if self.full_player_open {
                                0.36
                            } else {
                                0.58
                            }))
                    })
                    .when(dragging, |row| {
                        row.opacity(0.8)
                            .bg(primary.opacity(0.12))
                            .shadow(vec![BoxShadow {
                                color: primary.opacity(0.15),
                                offset: point(px(0.), px(4.)),
                                blur_radius: px(12.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            gpui_kit::component::GlobalState::suppress_text_selection(cx);
                            if this.queue_open {
                                this.queue_press = Some(QueuePress {
                                    index,
                                    revision: this.player.queue_revision,
                                    started: Instant::now(),
                                    start_y: event.position.y,
                                    y: event.position.y,
                                    eligible: true,
                                    sorting: false,
                                    changed: false,
                                });
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if this.queue_press.as_ref().is_some_and(|press| press.sorting) {
                            return;
                        }
                        if *hovered {
                            this.queue_hover = Some(index);
                            this.queue_hover_since = Some(Instant::now());
                        } else if this.queue_hover == Some(index) {
                            this.queue_hover = None;
                            this.queue_hover_since = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(20.))
                            .mr(px(if mobile { 12. } else { 15. }))
                            .flex_shrink_0()
                            .text_center()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .font_family("DIN Alternate")
                            .text_color(if self.full_player_open {
                                secondary
                            } else {
                                placeholder
                            })
                            .child(if dragging {
                                "⋮⋮".into()
                            } else {
                                format!("{:02}", index + 1)
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .line_height(relative(1.6))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_ellipsis()
                                    .child(song.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .text_ellipsis()
                                    .child(song.artist.clone()),
                            ),
                    )
                    .child(
                        div()
                            .w(px(if mobile { 88. } else { 60. }))
                            .h_full()
                            .flex_shrink_0()
                            .relative()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(6.))
                            .child(
                                div()
                                    .when(!mobile, |time| {
                                        time.absolute()
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                    })
                                    .text_size(px(if mobile {
                                        12.
                                    } else {
                                        12. * (1. - 0.2 * action_visibility)
                                    }))
                                    .text_color(if full_player { secondary } else { placeholder })
                                    .opacity(if mobile { 0.72 } else { 1. - action_visibility })
                                    .child(format!(
                                        "{:02}:{:02}",
                                        song.duration / 60,
                                        song.duration % 60
                                    )),
                            )
                            .when(mobile || action_visibility > 0., |actions| {
                                let scale = if mobile {
                                    1.
                                } else {
                                    0.8 + 0.2 * action_visibility
                                };
                                let foreground = if mobile {
                                    color(mode, "--td-error-color")
                                } else if full_player {
                                    secondary
                                } else {
                                    placeholder
                                };
                                actions.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "queue-remove-wrap-{index}"
                                        )))
                                        .when(!mobile, |wrapper| {
                                            wrapper
                                                .absolute()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                        })
                                        .opacity(action_visibility)
                                        .when(mobile || hovered, |wrapper| {
                                            wrapper
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(|_, _, cx| cx.stop_propagation())
                                                .child(
                                                    gpui_kit::base::Button::new(
                                                        SharedString::from(format!(
                                                            "remove-queue-{index}"
                                                        )),
                                                    )
                                                    .accessibility_label(self.t("common.delete"))
                                                    .disabled(!self.queue_open || sorting)
                                                    .size(px(if mobile {
                                                        36.
                                                    } else {
                                                        24. * scale
                                                    }))
                                                    .text_color(foreground)
                                                    .when(mobile, |button| {
                                                        button.rounded(px(999.)).bg(color(
                                                            mode,
                                                            "--td-error-color",
                                                        )
                                                        .opacity(0.1))
                                                    })
                                                    .when(!mobile, |button| {
                                                        button.cursor_pointer().hover(
                                                            move |style| {
                                                                style
                                                                    .text_color(color(
                                                                        mode,
                                                                        "--td-error-color",
                                                                    ))
                                                                    .rounded(px(4.))
                                                            },
                                                        )
                                                    })
                                                    .child(
                                                        glyph("xuanxiangshanchu")
                                                            .size(px(16. * scale)),
                                                    )
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if let Some(song) =
                                                            this.player.queue.get(index)
                                                        {
                                                            let id = song.id.clone();
                                                            this.player.remove_from_queue(&id);
                                                            this.queue_hover = None;
                                                            this.save_session();
                                                            cx.notify();
                                                        }
                                                        cx.stop_propagation();
                                                    })),
                                                )
                                        })
                                        .when(!mobile && !hovered, |wrapper| {
                                            // Leave animation paints only; row selection remains available.
                                            wrapper.child(
                                                div()
                                                    .size(px(24. * scale))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_color(foreground)
                                                    .child(
                                                        glyph("xuanxiangshanchu")
                                                            .size(px(16. * scale)),
                                                    ),
                                            )
                                        }),
                                )
                            }),
                    )
                    .when(tip, |row| {
                        let visibility = self.queue_tip.visibility;
                        let scale = 0.9 + visibility * 0.1;
                        let text: SharedString = self.t("play.longPressToSort").into();
                        let run = TextRun {
                            len: text.len(),
                            font: window.text_style().font(),
                            color: primary,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let width = window
                            .text_system()
                            .shape_line(text.clone(), px(12.), &[run], None)
                            .width()
                            + px(16.);
                        // Scale the tooltip's geometry around its center; no hitbox or input handler.
                        row.child(
                            div()
                                .absolute()
                                .right(px(70.) + width * (1. - scale) * 0.5)
                                .top(px((56. - 27.2 * scale) * 0.5))
                                .w(width * scale)
                                .px(px(8. * scale))
                                .py(px(4. * scale))
                                .rounded(px(4. * scale))
                                .text_size(px(12. * scale))
                                .line_height(relative(1.6))
                                .whitespace_nowrap()
                                .opacity(visibility)
                                .shadow(vec![BoxShadow {
                                    color: primary.opacity(0.2),
                                    offset: point(px(0.), px(2. * scale)),
                                    blur_radius: px(8. * scale),
                                    spread_radius: px(0.),
                                    inset: false,
                                }])
                                .bg(if full_player {
                                    colors.surface.opacity(0.6)
                                } else {
                                    primary.opacity(0.6)
                                })
                                .text_color(if full_player { primary } else { colors.surface })
                                .child(text),
                        )
                    })
                    .on_click(cx.listener(move |this, event, _, cx| {
                        if !matches!(event, ClickEvent::Mouse(_)) && !sorting {
                            this.play_index(index, cx);
                        }
                    })),
            )
    }
}

struct QueueColors {
    primary: Hsla,
    secondary: Hsla,
    muted: Hsla,
    brand: Hsla,
    surface: Hsla,
    mobile_surface: Hsla,
    mobile_border: Hsla,
}

fn queue_action(
    id: &'static str,
    label: String,
    tint: Hsla,
    mobile: bool,
    icon_name: Option<&'static str>,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .accessibility_label(label.clone())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(if mobile { 999. } else { 8. }))
        .border_1()
        .border_color(tint.opacity(0.2))
        .bg(tint.opacity(0.1))
        .text_color(tint)
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |button| {
            button
                .bg(tint.opacity(0.15))
                .border_color(tint.opacity(0.3))
        })
        .children(icon_name.map(|name| icon(name).size(px(16.))))
        .child(label)
}

fn queue_action_glass(button: gpui_kit::base::Button, mobile: bool) -> Div {
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(if mobile { 44. } else { 34. }))
        .when(cfg!(target_os = "macos"), |wrapper| {
            #[cfg(target_os = "macos")]
            {
                wrapper.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            window.paint_backdrop(
                                bounds,
                                Corners::all(px(if mobile { 22. } else { 8. })),
                                10.,
                                1.,
                            );
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            }
            #[cfg(not(target_os = "macos"))]
            {
                wrapper
            }
        })
        .child(button)
}
