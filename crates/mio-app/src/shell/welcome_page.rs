//! Native counterpart of the reference's Mio text-particle convergence and three-second skip.
//! The fixed Arial-Black mask is sampled once; no per-frame text rasterization or disk writes.
use super::*;
use std::sync::OnceLock;

fn text_points() -> &'static [(f32, f32)] {
    static POINTS: OnceLock<Vec<(f32, f32)>> = OnceLock::new();
    POINTS.get_or_init(|| {
        let image = image::load_from_memory(include_bytes!("../../assets/welcome/mio-mask.png"))
            .expect("bundled welcome mask")
            .to_rgba8();
        (0..image.height())
            .step_by(4)
            .flat_map(|y| {
                let image = &image;
                (0..image.width()).step_by(4).filter_map(move |x| {
                    (image.get_pixel(x, y).0[3] > 128).then_some((x as f32 - 300., y as f32 - 150.))
                })
            })
            .collect()
    })
}

impl Mio {
    pub(super) fn finish_welcome(&mut self, cx: &mut Context<Self>) {
        // Replace the splash route: Back must never restart the startup countdown.
        self.navigation = Navigation::new();
        self.ensure_online_discovery(cx);
        cx.notify();
    }
    pub(super) fn welcome_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let elapsed = self.welcome_started.elapsed().as_secs_f32();
        let reduced = cx.reduce_motion();
        let elapsed = if reduced { elapsed + 2. } else { elapsed };
        if elapsed >= 5. {
            let entity = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = entity.update(cx, |this, cx| this.finish_welcome(cx));
            });
        }
        // Inactive windows do not paint particles continuously; wake completes elapsed time.
        if window.is_window_active() {
            window.request_animation_frame();
        }
        let dark = self.mode == Mode::Dark;
        let brand = color(self.ui_colors(), "--td-brand-color");
        let started = self.welcome_started;
        div()
            .id("welcome-page")
            .size_full()
            .relative()
            .rounded(px(20.))
            .overflow_hidden()
            .bg(if dark { rgb(0x0d0d0d) } else { rgb(0xeef3f0) })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let t = if reduced {
                            2.
                        } else {
                            started.elapsed().as_secs_f32()
                        };
                        let width = f32::from(bounds.size.width);
                        let height = f32::from(bounds.size.height);
                        let scale = (width / 600.).min(height / 500.).min(1.5);
                        let convergence = (1. - (-1.5 * t).exp()).clamp(0., 1.);
                        let center = point(
                            bounds.origin.x + px(width * 0.5),
                            bounds.origin.y + px(height * 0.44),
                        );
                        let dot = |window: &mut Window, x: f32, y: f32, r: f32, tint: Hsla| {
                            window.paint_quad(PaintQuad {
                                bounds: Bounds::new(
                                    point(center.x + px(x - r), center.y + px(y - r)),
                                    size(px(r * 2.), px(r * 2.)),
                                ),
                                corner_radii: Corners::all(px(r)),
                                background: tint.into(),
                                border_widths: Edges::all(px(0.)),
                                border_color: transparent_black(),
                                border_style: BorderStyle::Solid,
                            });
                        };
                        // Fixed phases make the native animation reproducible, with a bounded scene.
                        for i in 0..160 {
                            let phase = i as f32 * 2.399963;
                            let x = phase.sin() * width * 0.55 + (t * 0.22 + phase).sin() * 14.;
                            let y = (phase * 1.73).cos() * height * 0.55
                                + (t * 0.35 + phase).cos() * 16.;
                            dot(
                                window,
                                x,
                                y,
                                1.5 + (phase.sin() + 1.) * 1.5,
                                brand.opacity(0.12),
                            );
                        }
                        for (i, (x, y)) in text_points().iter().enumerate() {
                            let phase = i as f32 * 2.399963;
                            let drift = (1. - convergence) * 320. * scale;
                            let x = x * scale + phase.cos() * drift;
                            let y = y * scale + phase.sin() * drift + (t * 1.2).sin() * 3.;
                            dot(
                                window,
                                x,
                                y,
                                1.4 * scale.max(0.65),
                                brand.opacity(0.55 + convergence * 0.4),
                            );
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(110.))
                    .left(px(0.))
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap(px(5.))
                    .children((0..5).map(|i| {
                        div()
                            .w(px(4.))
                            .h(px(if reduced {
                                14.
                            } else {
                                12. + ((elapsed * 3. + i as f32).sin() + 1.) * 10.
                            }))
                            .rounded(px(2.))
                            .bg(brand.opacity(0.8))
                    })),
            )
            .when(elapsed >= 2., |body| {
                body.child(
                    div()
                        .absolute()
                        .bottom(px(52.))
                        .left(px(0.))
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(
                            Button::new("welcome-skip")
                                .ghost()
                                .label(format!(
                                    "{} {}s",
                                    self.t("music.welcome.skip"),
                                    (5. - elapsed).ceil().max(0.) as u32
                                ))
                                .on_click(cx.listener(|this, _, _, cx| this.finish_welcome(cx))),
                        ),
                )
            })
    }
}
