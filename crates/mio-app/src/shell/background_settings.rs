use super::*;
use crate::background_config::{BackgroundConfig, Preset};
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectItem, SelectState},
    switch::Switch,
};

#[derive(Clone)]
pub(super) struct PresetOption(Preset, SharedString);
impl SelectItem for PresetOption {
    type Value = Preset;
    fn title(&self) -> SharedString {
        self.1.clone()
    }
    fn value(&self) -> &Preset {
        &self.0
    }
}
pub(super) struct Controls {
    preset: Entity<SelectState<Vec<PresetOption>>>,
    sliders: [Entity<SliderState>; 3],
    pressed: [Rc<Cell<bool>>; 3],
}
fn options(english: bool) -> Vec<PresetOption> {
    Preset::ALL
        .into_iter()
        .map(|preset| {
            PresetOption(
                preset,
                i18n::text(
                    english,
                    &format!("settings.backgroundRender.{}", preset.key()),
                )
                .into(),
            )
        })
        .collect()
}
impl Controls {
    pub(super) fn new(
        config: BackgroundConfig,
        english: bool,
        window: &mut Window,
        cx: &mut Context<Mio>,
    ) -> (Self, Vec<Subscription>) {
        let preset = cx.new(|cx| {
            SelectState::new(
                options(english),
                Some(
                    IndexPath::default().row(
                        Preset::ALL
                            .iter()
                            .position(|p| *p == config.preset)
                            .unwrap(),
                    ),
                ),
                window,
                cx,
            )
        });
        let sliders = [
            (0.1, 1., 0.1, config.render_scale),
            (0.1, 5., 0.1, config.flow_speed),
            (15., 60., 5., config.fps as f32),
        ]
        .map(|(min, max, step, value)| {
            cx.new(|_| {
                SliderState::new()
                    .min(min)
                    .max(max)
                    .step(step)
                    .default_value(value)
            })
        });
        let mut subscriptions =
            vec![
                cx.subscribe_in(&preset, window, |this, _, event, window, cx| {
                    if let SelectEvent::Confirm(Some(preset)) = event {
                        this.background_config.apply_preset(*preset);
                        this.background_config_changed(window, cx);
                    }
                }),
            ];
        for (index, slider) in sliders.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                slider,
                window,
                move |this, _, event, window, cx| {
                    if !this.background_config.enabled
                        || this.background_config.preset != Preset::Custom
                    {
                        return;
                    }
                    let value = match event {
                        SliderEvent::Change(v) | SliderEvent::Release(v) => v.start(),
                    };
                    match index {
                        0 => this.background_config.render_scale = value,
                        1 => this.background_config.flow_speed = value,
                        _ => this.background_config.fps = value.round() as u32,
                    }
                    #[cfg(target_os = "macos")]
                    this.background.configure(this.background_config);
                    if matches!(event, SliderEvent::Release(_)) {
                        this.background_config_changed(window, cx);
                    }
                    cx.notify();
                },
            ));
        }
        (
            Self {
                preset,
                sliders,
                pressed: std::array::from_fn(|_| Rc::new(Cell::new(false))),
            },
            subscriptions,
        )
    }
}
impl Mio {
    pub(super) fn sync_background_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.background_controls.preset.update(cx, |state, cx| {
            state.set_items(options(self.english), window, cx);
            state.set_selected_value(&self.background_config.preset, window, cx);
        });
        for (state, value) in self.background_controls.sliders.iter().zip([
            self.background_config.render_scale,
            self.background_config.flow_speed,
            self.background_config.fps as f32,
        ]) {
            state.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }
    fn background_config_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.background_config = self.background_config.validated();
        #[cfg(target_os = "macos")]
        self.background.configure(self.background_config);
        self.background_warning = None;
        self.sync_background_controls(window, cx);
        self.save_preferences();
        cx.notify();
    }
    pub(super) fn background_settings(
        &self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = |key: &str| self.t(&format!("settings.backgroundRender.{key}"));
        let row = |title: String, description: String, element: AnyElement| {
            div()
                .flex()
                .when(narrow, |row| row.flex_col().items_start().gap(px(10.)))
                .when(!narrow, |row| {
                    row.items_center().justify_between().gap(px(16.))
                })
                .px(px(if narrow { 12. } else { 16. }))
                .py(px(if narrow { 10. } else { 14. }))
                .border_1()
                .border_color(color(self.ui_colors(), "--td-border-level-1-color"))
                .bg(color(self.ui_colors(), "--td-bg-color-container"))
                .rounded(px(8.))
                .child(
                    div()
                        .when(!narrow, |info| info.flex_1())
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(15.2))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.8))
                                .text_color(color(self.ui_colors(), "--td-text-color-secondary"))
                                .child(description),
                        ),
                )
                .child(element)
        };
        let config = self.background_config;
        let custom_disabled = !config.enabled || config.preset != Preset::Custom;
        let mut view = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .w_full()
            .max_w(px(920.))
            .child(
                div()
                    .text_size(px(if narrow { 16. } else { 18. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t("title")),
            )
            .child(row(
                t("presetMode"),
                t("presetModeDesc"),
                div()
                    .w(px(200.))
                    .flex_shrink_0()
                    .child(
                        Select::new(&self.background_controls.preset)
                            .accessibility_label(t("presetMode"))
                            .menu_width(px(200.))
                            .w_full(),
                    )
                    .into_any_element(),
            ));
        for (index, key, desc, checked, disabled) in [
            (
                0,
                "enableBgEffect",
                "enableBgEffectDesc",
                config.enabled,
                false,
            ),
            (
                1,
                "audioResponse",
                "audioResponseDesc",
                config.audio_response,
                !config.enabled,
            ),
        ] {
            view = view.child(row(
                t(key),
                t(desc),
                Switch::new(("background-switch", index as usize))
                    .accessibility_label(t(key))
                    .checked(checked)
                    .disabled(disabled)
                    .on_change(cx.listener(move |this, checked, window, cx| {
                        if index == 0 {
                            this.background_config.enabled = *checked;
                        } else {
                            this.background_config.audio_response = *checked;
                        }
                        this.background_config_changed(window, cx);
                    }))
                    .into_any_element(),
            ));
        }
        for (index, key, desc, value) in [
            (
                0,
                "effectIntensity",
                "renderScaleDesc",
                format!("{:.1}", config.render_scale),
            ),
            (
                1,
                "flowSpeed",
                "flowSpeedDesc",
                format!("{:.1}", config.flow_speed),
            ),
        ] {
            let mut description = t(desc).replace("{value}", &value);
            if index == 0 {
                description.push_str(&format!(
                    " · {}",
                    t(if config.render_scale <= 0.3 {
                        "intensityLow"
                    } else if config.render_scale <= 0.6 {
                        "intensityMedium"
                    } else {
                        "intensityHigh"
                    })
                ));
            }
            view = view.child(row(
                t(key),
                description,
                div()
                    .w(px(200.))
                    .opacity(if custom_disabled { 0.45 } else { 1. })
                    .child(playback_slider(
                        &self.background_controls.sliders[index],
                        &self.background_controls.pressed[index],
                        custom_disabled,
                        self.ui_colors(),
                        cx,
                    ))
                    .into_any_element(),
            ));
        }
        view = view
            .child(row(
                t("staticMode"),
                t("staticModeDesc"),
                Switch::new("background-static")
                    .accessibility_label(t("staticMode"))
                    .checked(config.static_mode)
                    .disabled(custom_disabled)
                    .on_change(cx.listener(|this, checked, window, cx| {
                        this.background_config.static_mode = *checked;
                        this.background_config_changed(window, cx);
                    }))
                    .into_any_element(),
            ))
            .child(row(
                t("fpsLimit"),
                t("fpsLimitDesc").replace("{value}", &config.fps.to_string()),
                div()
                    .w(px(200.))
                    .opacity(if custom_disabled { 0.45 } else { 1. })
                    .child(playback_slider(
                        &self.background_controls.sliders[2],
                        &self.background_controls.pressed[2],
                        custom_disabled,
                        self.ui_colors(),
                        cx,
                    ))
                    .into_any_element(),
            ))
            .child(row(
                t("resetConfig"),
                t("resetConfigDesc"),
                Button::new("background-reset")
                    .label(t("resetConfig"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.background_config = BackgroundConfig::default();
                        this.background_config_changed(window, cx);
                    }))
                    .into_any_element(),
            ));
        if config.render_scale > 0.7 && config.fps > 45 {
            view = view.child(
                div()
                    .p(px(12.))
                    .rounded(px(8.))
                    .bg(color(self.ui_colors(), "--td-warning-color-1"))
                    .text_color(color(self.ui_colors(), "--td-warning-color-6"))
                    .text_size(px(14.))
                    .child(t("performanceWarning")),
            );
        }
        view
    }
}
