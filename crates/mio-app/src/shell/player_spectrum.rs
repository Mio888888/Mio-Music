use super::*;

pub(super) struct Spectrum {
    pub target: [f64; 128],
    shown: [f64; 128],
    peaks: [f64; 128],
    pub last_frame: Option<Instant>,
    pub generation: u64,
}
impl Default for Spectrum {
    fn default() -> Self {
        Self {
            target: [-80.; 128],
            shown: [-80.; 128],
            peaks: [-80.; 128],
            last_frame: None,
            generation: 0,
        }
    }
}
impl Spectrum {
    pub fn tick(&mut self, playing: bool) -> bool {
        if !playing {
            let changed = self.shown.iter().any(|v| *v != -80.);
            let generation = self.generation;
            *self = Self {
                generation,
                ..Default::default()
            };
            return changed;
        }
        let mut changed = false;
        for i in 0..128 {
            let target = self.target[i].clamp(-80., 10.);
            let prev = self.shown[i];
            let retain = if target > prev { 0.3 } else { 0.85 };
            self.shown[i] = prev * retain + target * (1. - retain);
            self.peaks[i] = self.shown[i].max(self.peaks[i] - 0.6);
            changed |= (prev - self.shown[i]).abs() > 0.01;
        }
        changed
    }
}
impl Mio {
    pub(super) fn player_spectrum(&self, width: f32, height: f32) -> impl IntoElement {
        let narrow = width <= 768.;
        let shown = self.player_spectrum.shown;
        let peaks = self.player_spectrum.peaks;
        let accent = self
            .theme_palette()
            .map(|p| p.main)
            .unwrap_or(white().opacity(0.8));
        let idle = self.player_motion.idle_visibility;
        let hidden = self.player_idle.hidden;
        let player_height = mio_tokens::player_height(height);
        let visible = self.audio_visualization && self.player.status == Status::Playing;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                if !visible {
                    return;
                }
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let slot = width / 80.;
                let norm = |db: f64| (((db.clamp(-80., 10.) + 80.) / 90.).powf(0.55)) as f32;
                for i in 0..40 {
                    let bar_h = norm(shown[i]) * height * 0.92;
                    let peak_h = norm(peaks[i]) * height * 0.92;
                    for x in [
                        width * 0.5 - (i + 1) as f32 * slot,
                        width * 0.5 + i as f32 * slot,
                    ] {
                        let rect = |x, y, w, h| {
                            Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h)))
                        };
                        if bar_h >= 1. {
                            let body = rect(x, height - bar_h, (slot - 0.5).max(0.5), bar_h);
                            // Soft glow follows the actual band height, never a synthetic signal.
                            window.paint_drop_shadows(
                                body,
                                Corners::all(px(2.)),
                                &[BoxShadow::new(px(0.), px(0.), accent.opacity(0.35))
                                    .blur_radius(px(8.))],
                            );
                            window.paint_quad(
                                fill(
                                    body,
                                    linear_gradient(
                                        0.,
                                        linear_color_stop(accent.opacity(0.2), 0.),
                                        linear_color_stop(accent.opacity(0.8), 1.),
                                    ),
                                )
                                .corner_radii(px((slot * 0.3).max(1.))),
                            );
                        }
                        if peak_h > 0.5 {
                            window.paint_quad(fill(
                                rect(x, height - peak_h - 2., (slot - 0.5).max(0.5), 2.),
                                accent,
                            ));
                        }
                    }
                }
            },
        )
        .absolute()
        .left_0()
        .w_full()
        .h(px(if narrow { 38. } else { 70. }))
        // The full player ends where the bar begins, unless the idle state has already given the
        // bar's space back; the spectrum rides on the bar's top edge and tucks 10px behind it.
        .bottom(px(if hidden { player_height } else { 0. }
            - if narrow { 0. } else { 10. }
            - player_height * idle))
        .opacity(if narrow { 0.42 } else { 1. })
        .into_any_element()
    }
}
