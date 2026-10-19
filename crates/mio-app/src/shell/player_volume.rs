use super::*;

fn popup_bounds(button: Bounds<Pixels>) -> Bounds<Pixels> {
    // Includes the 10px gap so the pointer can reach the popup without closing it.
    Bounds::new(
        point(button.right() - px(30.), button.top() - px(160.)),
        size(px(40.), px(160.)),
    )
}

impl Mio {
    pub(super) fn volume_control(
        &self,
        mode: Mode,
        palette: Option<cover_palette::CoverPalette>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.player.song().is_some();
        let brand = palette
            .map(|p| p.main)
            .unwrap_or_else(|| color(mode, "--td-brand-color"));
        let text = palette
            .map(|p| p.text)
            .unwrap_or_else(|| color(mode, "--player-text-idle"));
        let hover = palette
            .map(|p| p.hover)
            .unwrap_or_else(|| color(mode, "--player-text-hover-idle"));
        let label = if self.english { "Volume" } else { "音量" };
        let entity = cx.entity().downgrade();
        let observer = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let popup = popup_bounds(bounds);
                let pointer_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase.capture() {
                        let _ = pointer_entity.update(cx, |this, cx| {
                            if this.volume_open
                                && !this.volume_pressed.get()
                                && !bounds.contains(&event.position)
                                && !popup.contains(&event.position)
                            {
                                this.volume_open = false;
                                cx.notify();
                            }
                        });
                    }
                });
                let release_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase.capture() && event.button == MouseButton::Left {
                        let position = event.position;
                        let entity = release_entity.clone();
                        // Let the slider commit mouse-up before removing its popup.
                        window.defer(cx, move |_, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                if this.volume_open
                                    && !bounds.contains(&position)
                                    && !popup.contains(&position)
                                {
                                    this.volume_open = false;
                                    cx.notify();
                                }
                            });
                        });
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let visibility = self.player_motion.volume_visibility;
        div()
            .id("volume-control")
            .relative()
            .flex_shrink_0()
            .size(px(28.))
            .on_scroll_wheel(cx.listener(|this, event, window, cx| {
                this.adjust_volume(event, window, cx);
            }))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered
                    && active
                    && !this.queue_open
                    && (!this.full_player_open || !this.player_idle.hidden)
                    && !this.volume_open
                {
                    this.volume_open = true;
                    cx.notify();
                }
            }))
            .child(
                crate::skin::colored_control(
                    "volume",
                    "VolumeUp",
                    label.into(),
                    text,
                    hover,
                    transparent_black(),
                    transparent_black(),
                )
                .disabled(!active)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_full_player(window, cx))),
            )
            .child(observer)
            .when(active && visibility > 0., |control| {
                // Paint behind the popup fill, shadow and controls.
                let control = control.when(cfg!(target_os = "macos"), |control| {
                    #[cfg(target_os = "macos")]
                    {
                        control.child(
                            canvas(
                                |_, _, _| (),
                                |bounds, _, window, _| {
                                    window.paint_backdrop(bounds, Corners::all(px(8.)), 8., 1.);
                                },
                            )
                            .absolute()
                            .bottom(px(38. - 10. * (1. - visibility)))
                            .right(px(-10.))
                            .w(px(40.))
                            .h(px(150.))
                            .opacity(visibility),
                        )
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        control
                    }
                });
                control.child(
                    div()
                        .id("volume-popup")
                        .aria_label(label)
                        .on_scroll_wheel(cx.listener(|this, event, window, cx| {
                            if this.volume_open && !this.queue_open {
                                this.adjust_volume(event, window, cx);
                            }
                        }))
                        .absolute()
                        .bottom(px(38. - 10. * (1. - visibility)))
                        .right(px(-10.))
                        .w(px(40.))
                        .h(px(150.))
                        .rounded(px(8.))
                        .py(px(15.))
                        .px(px(10.))
                        .bg(text)
                        .opacity(visibility)
                        .shadow(vec![BoxShadow {
                            color: black().opacity(0.2),
                            offset: point(px(0.), px(4.)),
                            blur_radius: px(12.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_between()
                                .gap(px(8.))
                                .child(crate::skin::volume_slider(
                                    &self.volume,
                                    &self.volume_pressed,
                                    !self.volume_open,
                                    brand,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .whitespace_nowrap()
                                        .text_size(px(12.))
                                        .line_height(relative(1.6))
                                        .mt(px(8.))
                                        .flex_shrink_0()
                                        .text_color(brand)
                                        .child(format!(
                                            "{}%",
                                            (self.player.volume * 100.).round() as u32
                                        )),
                                ),
                        ),
                )
            })
    }

    fn adjust_volume(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.player.song().is_none() {
            return;
        }
        let delta = f32::from(event.delta.pixel_delta(px(16.)).y);
        if delta == 0. {
            return;
        }
        // GPUI uses positive Y for up; DOM wheel deltaY uses down.
        let percent = (self.player.volume * 100. + if delta > 0. { 5. } else { -5. })
            .clamp(0., 100.)
            .round();
        self.set_volume_percent(percent, window, cx);
        cx.stop_propagation();
    }

    /// Applies a volume in percent: engine, slider, saved preference.
    pub(super) fn set_volume_percent(
        &mut self,
        percent: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let percent = percent.clamp(0., 100.).round();
        if percent != (self.player.volume * 100.).round() {
            self.player.volume = percent / 100.;
            if let Some(engine) = &self.player.engine {
                engine.volume(self.player.volume);
            }
            self.volume
                .update(cx, |state, cx| state.set_value(percent, window, cx));
            self.save_preferences();
            cx.notify();
        }
    }
}
