//! The sound-effects category of the settings page (`AudioEffectsSection.vue`,
//! `EqualizerSettings.vue`, `AudioEffectSettings.vue`): the ten-band equalizer with its response
//! curve and spectrum, presets with import and export, and the surround/balance cards.
//!
//! As in the original, the surround choice is only stored (the player has no surround stage);
//! the balance and the equalizer drive the audio chain at once.
use super::settings_page::Choice;
use super::*;
use crate::effects::{self, BandParams, NUM_EQ_BANDS};
use crate::equalizer::{self, Gains, State};
use gpui_kit::component::{
    IndexPath, WindowExt,
    input::InputState,
    notification::Notification,
    select::{Select, SelectEvent, SelectState},
    slider::SliderState,
    switch::Switch,
};
use std::{cell::Cell, rc::Rc};

const EQUALIZER_FILE: &str = "equalizer.json";
const EFFECTS_FILE: &str = "audio-effects.json";
const PRESET_VAULT_URL: &str = "https://mio888888.github.io/EQVault/";
const PLOT_INSET: f32 = 16.;
const SILENCE_DB: f64 = -80.;
const GRID_FREQS: [f64; 10] = [
    20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000.,
];
const GRID_DB: [i32; 9] = [-24, -18, -12, -6, 0, 6, 12, 18, 24];

pub(super) use mio_core::audio_effects::AudioEffects;
use mio_core::audio_effects::SurroundMode;
const SURROUND_MODES: [(SurroundMode, &str); 4] = [
    (SurroundMode::Off, "settings.audioEffect.surroundOff"),
    (SurroundMode::Small, "settings.audioEffect.surroundSmall"),
    (SurroundMode::Medium, "settings.audioEffect.surroundMedium"),
    (SurroundMode::Large, "settings.audioEffect.surroundLarge"),
];

/// The controls and what the plot animates.
pub(super) struct EffectsUi {
    bands: Vec<Entity<SliderState>>,
    band_pressed: Vec<Rc<Cell<bool>>>,
    global: Entity<SliderState>,
    global_pressed: Rc<Cell<bool>>,
    frequency: Entity<SliderState>,
    frequency_pressed: Rc<Cell<bool>>,
    q: Entity<SliderState>,
    q_pressed: Rc<Cell<bool>>,
    balance: Entity<SliderState>,
    balance_pressed: Rc<Cell<bool>>,
    preset: Entity<SelectState<Vec<Choice>>>,
    kind: Entity<SelectState<Vec<Choice>>>,
    /// `selectedBandIndex`; none until a band is clicked.
    selected: Option<usize>,
    /// What the preset select currently lists, to refill it only on change.
    preset_signature: String,
    kind_signature: Option<(usize, bool)>,
    spectrum: Vec<f64>,
    shown: Vec<f64>,
}

fn frequency_to_slider(frequency: f64) -> f32 {
    ((frequency / effects::EQ_FREQ_MIN).log10()
        / (effects::EQ_FREQ_MAX / effects::EQ_FREQ_MIN).log10()) as f32
}
fn slider_to_frequency(value: f32) -> f64 {
    effects::EQ_FREQ_MIN * (effects::EQ_FREQ_MAX / effects::EQ_FREQ_MIN).powf(f64::from(value))
}
fn format_frequency(frequency: f64) -> String {
    if frequency >= 1000. {
        format!("{}k", frequency / 1000.)
    } else {
        format!("{frequency}")
    }
}

fn slider_event(event: &gpui_kit::component::slider::SliderEvent) -> (f32, bool) {
    use gpui_kit::component::slider::SliderEvent;
    match event {
        SliderEvent::Change(v) => (v.start(), false),
        SliderEvent::Release(v) => (v.start(), true),
    }
}

impl EffectsUi {
    pub(super) fn new(
        state: &State,
        effects: &AudioEffects,
        english: bool,
        window: &mut Window,
        cx: &mut Context<Mio>,
    ) -> (Self, Vec<Subscription>) {
        let mut subscriptions = Vec::new();
        let slider = |min: f32, max: f32, step: f32, value: f32, cx: &mut Context<Mio>| {
            cx.new(|_| {
                SliderState::new()
                    .min(min)
                    .max(max)
                    .step(step)
                    .default_value(value)
            })
        };
        let bands: Vec<Entity<SliderState>> = state
            .gains
            .bands
            .iter()
            .map(|band| slider(-24., 24., 0.1, band.gain as f32, cx))
            .collect();
        for (index, band) in bands.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(band, window, move |this, _, event, _, cx| {
                let (value, release) = slider_event(event);
                this.eq_band_gain(index, f64::from(value), release, cx);
            }));
        }
        let global = slider(-24., 24., 0.1, state.gains.global as f32, cx);
        subscriptions.push(cx.subscribe_in(&global, window, |this, _, event, _, cx| {
            let (value, release) = slider_event(event);
            this.eq_global_gain(f64::from(value), release, cx);
        }));
        let frequency = slider(0., 1., 0.001, 0.5, cx);
        subscriptions.push(
            cx.subscribe_in(&frequency, window, |this, _, event, _, cx| {
                let (value, release) = slider_event(event);
                if let Some(index) = this.effects_ui.selected {
                    this.eq_band_frequency(index, slider_to_frequency(value), release, cx);
                }
            }),
        );
        let q = slider(0.1, 10., 0.01, 1.414, cx);
        subscriptions.push(cx.subscribe_in(&q, window, |this, _, event, _, cx| {
            let (value, release) = slider_event(event);
            if let Some(index) = this.effects_ui.selected {
                this.eq_band_q(index, f64::from(value), release, cx);
            }
        }));
        let balance = slider(-1., 1., 0.05, effects.balance as f32, cx);
        subscriptions.push(cx.subscribe_in(&balance, window, |this, _, event, _, cx| {
            let (value, release) = slider_event(event);
            if (this.audio_effects.balance - f64::from(value)).abs() > 1e-6 {
                this.audio_effects.balance = f64::from(value);
                this.apply_audio_effects();
                cx.notify();
            }
            if release {
                this.save_audio_effects();
            }
        }));
        let preset = cx.new(|cx| SelectState::new(Vec::<Choice>::new(), None, window, cx));
        subscriptions.push(
            cx.subscribe_in(&preset, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(index)) = event {
                    let id = this.eq.presets().get(*index).map(|p| p.id.clone());
                    if let Some(id) = id {
                        this.eq_apply_preset(&id, window, cx);
                    }
                }
            }),
        );
        let kind = cx.new(|cx| {
            SelectState::new(
                effects::FILTER_TYPES
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Choice(i, i18n::text(english, t.locale_key()).into()))
                    .collect::<Vec<_>>(),
                Some(IndexPath::default().row(0)),
                window,
                cx,
            )
        });
        subscriptions.push(cx.subscribe_in(&kind, window, |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(index)) = event
                && let (Some(band), Some(kind)) = (
                    this.effects_ui.selected,
                    effects::FILTER_TYPES.get(*index).copied(),
                )
            {
                this.eq.set_band_type(band, kind);
                this.eq_changed(true, cx);
            }
        }));
        (
            Self {
                bands,
                band_pressed: (0..NUM_EQ_BANDS)
                    .map(|_| Rc::new(Cell::new(false)))
                    .collect(),
                global,
                global_pressed: Rc::new(Cell::new(false)),
                frequency,
                frequency_pressed: Rc::new(Cell::new(false)),
                q,
                q_pressed: Rc::new(Cell::new(false)),
                balance,
                balance_pressed: Rc::new(Cell::new(false)),
                preset,
                kind,
                selected: None,
                preset_signature: String::new(),
                kind_signature: None,
                spectrum: vec![SILENCE_DB; effects::SPECTRUM_BANDS],
                shown: vec![SILENCE_DB; effects::SPECTRUM_BANDS],
            },
            subscriptions,
        )
    }
}

/// What the name/confirm dialog is for.
#[derive(Clone)]
enum DialogKind {
    SaveAs,
    TextImport(Gains),
    Delete,
}

struct EffectsDialog {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    kind: DialogKind,
    name: Option<Entity<InputState>>,
    body: String,
}

impl Mio {
    /// Reads both saved configurations.
    pub(super) fn load_effects(store: &Option<Storage>) -> (State, AudioEffects) {
        let eq = store
            .as_ref()
            .and_then(|s| s.load::<serde_json::Value>(EQUALIZER_FILE).ok().flatten())
            .and_then(|raw| equalizer::hydrate(&raw, false).ok())
            .map(|mut state| {
                equalizer::fit_bands(&mut state.gains);
                state
            })
            .unwrap_or_default();
        let effects = store
            .as_ref()
            .and_then(|s| s.load::<AudioEffects>(EFFECTS_FILE).ok().flatten())
            .unwrap_or_default();
        (eq, effects)
    }

    /// The equalizer, to the audio chain (`syncToBackend`).
    fn apply_equalizer(&self) {
        let params: Vec<BandParams> = self.eq.gains.band_params();
        effects::shared().set_equalizer(self.eq.enabled, self.eq.gains.global, &params);
    }
    /// The balance, to the audio chain (`applyEffects`).
    fn apply_audio_effects(&self) {
        effects::shared().set_balance(if self.audio_effects.balance_enabled {
            self.audio_effects.balance
        } else {
            0.
        });
    }
    /// What the original does when the player mounts: restore both to the audio engine.
    pub(super) fn apply_all_effects(&self) {
        self.apply_equalizer();
        self.apply_audio_effects();
    }

    fn save_equalizer(&self) {
        if let Some(store) = &self.store
            && store.save(EQUALIZER_FILE, &self.eq).is_err()
        {
            eprintln!("Unable to save the equalizer");
        }
    }
    fn save_audio_effects(&self) {
        if let Some(store) = &self.store
            && store.save(EFFECTS_FILE, &self.audio_effects).is_err()
        {
            eprintln!("Unable to save the audio effects");
        }
    }

    /// The equalizer changed: hear it, redraw, and save when the gesture is finished.
    fn eq_changed(&mut self, save: bool, cx: &mut Context<Self>) {
        self.apply_equalizer();
        if save {
            self.save_equalizer();
        }
        cx.notify();
    }

    fn eq_band_gain(&mut self, index: usize, value: f64, release: bool, cx: &mut Context<Self>) {
        let Some(band) = self.eq.gains.bands.get(index) else {
            return;
        };
        if (band.gain - value).abs() > 1e-6 {
            self.eq.set_band_gain(index, value);
            self.eq_changed(release, cx);
        } else if release {
            self.save_equalizer();
        }
    }
    fn eq_global_gain(&mut self, value: f64, release: bool, cx: &mut Context<Self>) {
        if (self.eq.gains.global - value).abs() > 1e-6 {
            self.eq.gains.global = effects::clamp_gain(value);
            self.eq_changed(release, cx);
        } else if release {
            self.save_equalizer();
        }
    }
    fn eq_band_frequency(
        &mut self,
        index: usize,
        value: f64,
        release: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(band) = self.eq.gains.bands.get(index) else {
            return;
        };
        // The slider has 0.001 steps of a log scale; ignore the echo of our own write.
        if (frequency_to_slider(band.frequency) - frequency_to_slider(value)).abs() > 0.0006 {
            self.eq.set_band_frequency(index, value);
            self.eq_changed(release, cx);
        } else if release {
            self.save_equalizer();
        }
    }
    fn eq_band_q(&mut self, index: usize, value: f64, release: bool, cx: &mut Context<Self>) {
        let Some(band) = self.eq.gains.bands.get(index) else {
            return;
        };
        if (band.q - value).abs() > 0.004 {
            self.eq.set_band_q(index, value);
            self.eq_changed(release, cx);
        } else if release {
            self.save_equalizer();
        }
    }

    fn notify_success(&self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(Notification::success(text), cx);
    }
    fn notify_warning(&self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(Notification::warning(text), cx);
    }
    fn notify_error(&self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(Notification::error(text), cx);
    }

    fn preset_display_name(&self, preset: &equalizer::Preset) -> String {
        match equalizer::preset_locale_key(&preset.id) {
            Some(key) if preset.is_default => self.t(key),
            _ => preset.name.clone(),
        }
    }
    fn current_preset_name(&self) -> String {
        match self.eq.current_preset() {
            Some(preset) => self.preset_display_name(&preset),
            None => self.t("settings.equalizer.presetFlat"),
        }
    }
    fn default_preset_names(&self) -> Vec<String> {
        equalizer::default_presets()
            .iter()
            .map(|p| self.preset_display_name(p).trim().to_lowercase())
            .collect()
    }

    fn eq_apply_preset(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.eq.apply_preset(id) {
            self.eq_changed(true, cx);
        } else {
            self.notify_error(self.t("settings.equalizer.presetNotExist"), window, cx);
        }
    }

    fn eq_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.eq.current_preset_id.clone();
        if self.eq.apply_preset(&id) {
            let name = self.current_preset_name();
            self.eq_changed(true, cx);
            self.notify_success(
                self.t("settings.equalizer.resetToPreset")
                    .replace("{name}", &name),
                window,
                cx,
            );
        } else {
            self.eq.apply_preset(equalizer::FLAT_PRESET_ID);
            self.eq_changed(true, cx);
            self.notify_success(self.t("settings.equalizer.resetToFlat"), window, cx);
        }
    }

    fn eq_save_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.current_preset_name();
        let id = self.eq.current_preset_id.clone();
        if self.eq.update_user_preset(&id) {
            self.save_equalizer();
            cx.notify();
            self.notify_success(
                self.t("settings.equalizer.savedToPreset")
                    .replace("{name}", &name),
                window,
                cx,
            );
        } else {
            self.notify_warning(self.t("settings.equalizer.builtinNoModify"), window, cx);
        }
    }

    fn eq_delete_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.current_preset_name();
        let id = self.eq.current_preset_id.clone();
        if self.eq.delete_user_preset(&id) {
            self.eq_changed(true, cx);
            self.notify_success(
                self.t("settings.equalizer.presetDeleted")
                    .replace("{name}", &name),
                window,
                cx,
            );
        } else {
            self.notify_warning(self.t("settings.equalizer.builtinNoDelete"), window, cx);
        }
    }

    fn eq_export(&mut self, cx: &mut Context<Self>) {
        let Ok(json) = serde_json::to_string_pretty(&equalizer::export_config(&self.eq)) else {
            return;
        };
        let directory = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Downloads"))
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let selection = cx.prompt_for_new_path(&directory, Some("mio-eq-config.json"));
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(path))) = selection.await {
                let _ = cx
                    .background_executor()
                    .spawn(async move { std::fs::write(path, json) })
                    .await;
            }
        })
        .detach();
        self.eq.add_log("Exported configuration".into());
    }

    fn eq_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("settings.equalizer.importConfig").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = selection.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().trim().to_owned())
                .unwrap_or_default();
            let text = cx
                .background_executor()
                .spawn(async move {
                    std::fs::read(&path)
                        .ok()
                        .filter(|bytes| bytes.len() <= 4 * 1024 * 1024)
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.eq_imported(text, name, window, cx)
            });
        })
        .detach();
    }

    fn eq_imported(
        &mut self,
        text: Option<String>,
        file_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = text else {
            self.notify_error(self.t("settings.equalizer.importFailed"), window, cx);
            return;
        };
        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) {
            match equalizer::hydrate(&raw, true) {
                Ok(mut state) => {
                    equalizer::fit_bands(&mut state.gains);
                    self.eq.enabled = state.enabled;
                    self.eq.current_preset_id = state.current_preset_id;
                    self.eq.gains = state.gains;
                    self.eq.user_presets = state.user_presets;
                    self.eq.add_log("Imported configuration".into());
                    self.eq_changed(true, cx);
                    self.notify_success(self.t("settings.equalizer.importSuccess"), window, cx);
                }
                Err(message) => self.notify_error(message, window, cx),
            }
            return;
        }
        match equalizer::parse_text_preset(&text) {
            Some(gains) => {
                self.open_effects_dialog(DialogKind::TextImport(gains), file_name, window, cx)
            }
            None => self.notify_error(
                self.t("settings.equalizer.textImportParseFailed"),
                window,
                cx,
            ),
        }
    }

    /// The name was confirmed in a dialog; returns whether the dialog should close.
    fn eq_dialog_confirmed(
        &mut self,
        kind: DialogKind,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = name.trim().to_owned();
        match kind {
            DialogKind::Delete => {
                self.eq_delete_current(window, cx);
                true
            }
            DialogKind::SaveAs | DialogKind::TextImport(_) => {
                let import = match &kind {
                    DialogKind::TextImport(gains) => Some(gains.clone()),
                    _ => None,
                };
                if name.is_empty() {
                    if import.is_some() {
                        self.notify_warning(
                            self.t("settings.equalizer.textImportNameRequired"),
                            window,
                            cx,
                        );
                    }
                    return false;
                }
                if self.default_preset_names().contains(&name.to_lowercase()) {
                    self.notify_warning(
                        self.t("settings.equalizer.builtinNameConflict")
                            .replace("{name}", &name),
                        window,
                        cx,
                    );
                    return false;
                }
                let apply = import.is_some();
                let gains = import.unwrap_or_else(|| self.eq.gains.clone());
                match self.eq.create_user_preset(&name, gains, apply) {
                    Ok(()) => {
                        self.eq_changed(true, cx);
                        let message = if apply {
                            self.t("settings.equalizer.textImportSuccess")
                                .replace("{name}", &name)
                        } else {
                            self.t("settings.equalizer.presetSaveSuccess")
                        };
                        self.notify_success(message, window, cx);
                        true
                    }
                    Err(key) => {
                        self.notify_warning(self.t(key).replace("{name}", &name), window, cx);
                        false
                    }
                }
            }
        }
    }

    fn open_effects_dialog(
        &mut self,
        kind: DialogKind,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let body = match kind {
            DialogKind::Delete => self
                .t("settings.equalizer.deletePresetBody")
                .replace("{name}", &self.current_preset_name()),
            _ => String::new(),
        };
        let placeholder = self.t(match kind {
            DialogKind::TextImport(_) => "settings.equalizer.textImportPresetNamePlaceholder",
            _ => "settings.equalizer.presetNamePlaceholder",
        });
        let needs_name = !matches!(kind, DialogKind::Delete);
        let content = cx.new(|cx| {
            let name = needs_name.then(|| {
                cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder(placeholder);
                    state.set_value(initial, window, cx);
                    state
                })
            });
            EffectsDialog {
                parent,
                colors,
                english,
                kind,
                name,
                body,
            }
        });
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
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(colors, "--td-bg-color-container"))
                .border_color(color(colors, "--td-border-level-1-color"))
                .overlay_tint(color(colors, "--td-mask-active"))
                .close_button(false)
                .on_ok(|_, _, _| false)
                .child(content.clone())
        });
    }

    /// Brings the controls in line with the state; cheap when nothing changed.
    fn sync_effects_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        fn set(
            slider: &Entity<SliderState>,
            value: f32,
            window: &mut Window,
            cx: &mut Context<Mio>,
        ) {
            if (slider.read(cx).value().start() - value).abs() > 1e-4 {
                slider.update(cx, |state, cx| state.set_value(value, window, cx));
            }
        }
        let bands = self.effects_ui.bands.clone();
        for (index, slider) in bands.iter().enumerate() {
            if let Some(band) = self.eq.gains.bands.get(index) {
                set(slider, band.gain as f32, window, cx);
            }
        }
        set(
            &self.effects_ui.global.clone(),
            self.eq.gains.global as f32,
            window,
            cx,
        );
        set(
            &self.effects_ui.balance.clone(),
            self.audio_effects.balance as f32,
            window,
            cx,
        );
        let selected = self
            .effects_ui
            .selected
            .and_then(|i| self.eq.gains.bands.get(i))
            .cloned();
        if let Some(band) = selected {
            set(
                &self.effects_ui.frequency.clone(),
                frequency_to_slider(band.frequency),
                window,
                cx,
            );
            set(&self.effects_ui.q.clone(), band.q as f32, window, cx);
            let wanted = (
                effects::FILTER_TYPES
                    .iter()
                    .position(|t| *t == band.kind)
                    .unwrap_or(0),
                self.english,
            );
            if self.effects_ui.kind_signature != Some(wanted) {
                self.effects_ui.kind_signature = Some(wanted);
                let english = self.english;
                self.effects_ui.kind.update(cx, |state, cx| {
                    state.set_items(
                        effects::FILTER_TYPES
                            .iter()
                            .enumerate()
                            .map(|(i, t)| Choice(i, i18n::text(english, t.locale_key()).into()))
                            .collect(),
                        window,
                        cx,
                    );
                    state.set_selected_index(Some(IndexPath::default().row(wanted.0)), window, cx);
                });
            }
        }
        // The preset list: built-in names are localised, user presets are as saved.
        let presets = self.eq.presets();
        let signature = format!(
            "{}|{}|{}",
            self.english,
            self.eq.current_preset_id,
            presets
                .iter()
                .map(|p| format!("{}:{}", p.id, p.name))
                .collect::<Vec<_>>()
                .join(",")
        );
        if self.effects_ui.preset_signature != signature {
            self.effects_ui.preset_signature = signature;
            let items: Vec<Choice> = presets
                .iter()
                .enumerate()
                .map(|(i, p)| Choice(i, self.preset_display_name(p).into()))
                .collect();
            let current = presets
                .iter()
                .position(|p| p.id == self.eq.current_preset_id);
            self.effects_ui.preset.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                state.set_selected_index(current.map(|i| IndexPath::default().row(i)), window, cx);
            });
        }
    }

    /// Redraws the spectrum about 30 times a second while the effects category is showing.
    pub(super) fn start_spectrum_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            // ~30 Hz while a spectrum or the comments are showing; otherwise only the
            // output-device check runs, a few times a second.
            let mut interval = 33;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(interval))
                    .await;
                match update_main(&this, cx, |this, _, cx| {
                    let visible = this.navigation.current() == Page::Appearance
                        && !this.full_player_open
                        && this.settings_ui.category == settings_page::Category::Effects;
                    let player_visible = this.full_player_open && this.audio_visualization;
                    effects::shared().set_unowned_capture(visible || player_visible);
                    if (visible || player_visible) && this.tick_spectrum() {
                        cx.notify();
                    }
                    if this.comments.open {
                        this.sync_comments(cx);
                    }
                    // A device was plugged in or the default output changed.
                    if crate::audio_device::take_changed() {
                        if visible_playback(this) {
                            this.scan_audio_devices(cx);
                        } else {
                            this.audio_output.dirty = true;
                        }
                    }
                    visible || player_visible || this.comments.open
                }) {
                    Ok(active) => interval = if active { 33 } else { 250 },
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    /// Spectrum smoothing, once per frame while the page is visible; whether anything moved.
    pub(super) fn tick_spectrum(&mut self) -> bool {
        if self.player_spectrum.generation != self.player.generation {
            self.player_spectrum = Default::default();
            self.player_spectrum.generation = self.player.generation;
            self.effects_ui.spectrum.fill(SILENCE_DB);
        }
        if let Some(bands) = effects::shared().take_spectrum() {
            for (target, value) in self.effects_ui.spectrum.iter_mut().zip(bands) {
                *target = if value.is_finite() { value } else { SILENCE_DB };
            }
            self.player_spectrum.last_frame = Some(Instant::now());
        }
        if self.player.status != Status::Playing
            || self
                .player_spectrum
                .last_frame
                .is_none_or(|frame| frame.elapsed() > Duration::from_millis(200))
        {
            self.effects_ui.spectrum.fill(SILENCE_DB);
        }
        self.player_spectrum
            .target
            .copy_from_slice(&self.effects_ui.spectrum);
        let mut moving = self
            .player_spectrum
            .tick(self.player.status == Status::Playing);
        let ui = &mut self.effects_ui;
        for (shown, target) in ui.shown.iter_mut().zip(&ui.spectrum) {
            let next = *shown + (*target - *shown) * 0.18;
            if (next - *shown).abs() > 0.01 {
                moving = true;
            }
            *shown = next;
        }
        moving
    }

    pub(super) fn effects_groups(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.sync_effects_controls(window, cx);
        vec![
            self.setting_group(
                "effects-equalizer",
                Some(self.t("settings.effects.equalizer")),
                narrow,
            )
            .child(self.equalizer_panel(narrow, cx))
            .into_any_element(),
            self.setting_group(
                "effects-audio-effect",
                Some(self.t("settings.effects.audioEffect")),
                narrow,
            )
            .child(self.audio_effect_panel(narrow, cx))
            .into_any_element(),
        ]
    }

    fn equalizer_panel(&self, narrow: bool, cx: &mut Context<Self>) -> Div {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let enabled = self.eq.enabled;
        let selected = self.effects_ui.selected;
        let card_bg =
            crate::skin::mixed_color(mode, "--td-brand-color", 0.05, "--td-bg-color-container");
        let panel_border =
            crate::skin::mixed_color(mode, "--td-brand-color", 0.12, "--td-border-level-1-color");

        let header = div()
            .flex()
            .justify_end()
            .items_center()
            .gap(px(8.))
            .mb(px(14.))
            .child(
                Switch::new("eq-enabled")
                    .accessibility_label(self.t("settings.equalizer.title"))
                    .checked(enabled)
                    .label(if enabled {
                        self.t("settings.equalizer.on")
                    } else {
                        self.t("settings.equalizer.off")
                    })
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.eq.enabled = *checked;
                        this.eq.add_log(
                            if *checked {
                                "Equalizer enabled"
                            } else {
                                "Equalizer disabled"
                            }
                            .into(),
                        );
                        this.eq_changed(true, cx);
                    })),
            )
            .child(
                Button::new("eq-reset")
                    .label(self.t("settings.equalizer.reset"))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| this.eq_reset(window, cx))),
            );

        let plot = self.response_plot(narrow);

        // Ten gain sliders.
        let mut sliders = div()
            .flex()
            .justify_between()
            .h(px(if narrow { 232. } else { 280. }))
            .py(px(if narrow { 14. } else { 20. }))
            .gap(px(if narrow { 2. } else { 4. }));
        for (index, band) in self.eq.gains.bands.iter().enumerate() {
            let disabled = !enabled || !band.kind.has_gain();
            let is_selected = selected == Some(index);
            let slider_height = if narrow { 168. } else { 210. };
            let hover_bg =
                crate::skin::mixed_color(mode, "--td-brand-color", 0.08, "--td-bg-color-container");
            let selected_bg =
                crate::skin::mixed_color(mode, "--td-brand-color", 0.12, "--td-bg-color-container");
            let group = div()
                .id(SharedString::from(format!("eq-band-{index}")))
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .py(px(8.))
                .px(px(2.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(move |g| g.bg(hover_bg))
                .when(is_selected, move |g| {
                    g.bg(selected_bg)
                        .border_1()
                        .border_color(brand.opacity(0.4))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.effects_ui.selected = Some(index);
                    this.effects_ui.kind_signature = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .h(px(slider_height))
                        .flex()
                        .justify_center()
                        .child(self.band_slider(index, disabled, brand, cx)),
                )
                .child(
                    div()
                        .mt(px(10.))
                        .text_size(px(if narrow { 10. } else { 12. }))
                        .text_color(secondary)
                        .child(format_frequency(band.frequency)),
                )
                .child(
                    div()
                        .mt(px(4.))
                        .text_size(px(if narrow { 9. } else { 10. }))
                        .text_color(secondary)
                        .child(if band.kind.has_gain() {
                            format!("{:.1}dB", band.gain)
                        } else {
                            "--".into()
                        }),
                );
            sliders = sliders.child(group);
        }

        // The selected band's parameters.
        let params = selected
            .and_then(|i| self.eq.gains.bands.get(i).map(|b| (i, b)))
            .map(|(index, band)| {
                let row = |label: String, control: AnyElement, value: String| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .child(
                            div()
                                .w(px(if narrow { 72. } else { 64. }))
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .text_color(secondary)
                                .child(label),
                        )
                        .child(div().flex_1().min_w_0().child(control))
                        .child(
                            div()
                                .w(px(56.))
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .text_color(primary)
                                .text_align(TextAlign::Right)
                                .child(value),
                        )
                };
                let divider = || {
                    div().h(px(1.)).w_full().bg(crate::skin::mixed_color(
                        mode,
                        "--td-component-border",
                        0.6,
                        "--td-bg-color-container",
                    ))
                };
                let slider = |state: &Entity<SliderState>, pressed: &Rc<Cell<bool>>| {
                    crate::skin::colored_playback_slider(state, pressed, !enabled, mode, cx, None)
                        .into_any_element()
                };
                div()
                    .px(px(if narrow { 14. } else { 20. }))
                    .py(px(if narrow { 14. } else { 16. }))
                    .rounded(px(10.))
                    .bg(card_bg)
                    .border_1()
                    .border_color(panel_border)
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(color(mode, "--td-border-level-1-color"))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(
                                        self.t("settings.equalizer.bandParams")
                                            .replace("{index}", &(index + 1).to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(format_frequency(band.frequency)),
                            ),
                    )
                    .child(row(
                        self.t("settings.equalizer.frequency"),
                        slider(
                            &self.effects_ui.frequency,
                            &self.effects_ui.frequency_pressed,
                        ),
                        format_frequency(band.frequency),
                    ))
                    .child(divider())
                    .child(row(
                        self.t("settings.equalizer.qFactor"),
                        slider(&self.effects_ui.q, &self.effects_ui.q_pressed),
                        format!("{:.2}", band.q),
                    ))
                    .child(divider())
                    .child(row(
                        self.t("settings.equalizer.filterType"),
                        Select::new(&self.effects_ui.kind)
                            .accessibility_label(self.t("settings.equalizer.filterType"))
                            .disabled(!enabled)
                            .w_full()
                            .into_any_element(),
                        String::new(),
                    ))
            });

        let user_preset = self.eq.current_preset().is_some_and(|p| !p.is_default);
        let preset_controls = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(8.))
            .child(
                div().w(px(if narrow { 260. } else { 180. })).child(
                    Select::new(&self.effects_ui.preset)
                        .accessibility_label(self.t("settings.equalizer.selectPreset"))
                        .placeholder(self.t("settings.equalizer.selectPreset"))
                        .w_full(),
                ),
            )
            .when(user_preset, |row| {
                row.child(
                    Button::new("eq-save-preset")
                        .label(self.t("settings.equalizer.savePreset"))
                        .primary()
                        .ghost()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.eq_save_current(window, cx)),
                        ),
                )
                .child(
                    Button::new("eq-delete-preset")
                        .label(self.t("settings.equalizer.deletePreset"))
                        .danger()
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_effects_dialog(DialogKind::Delete, String::new(), window, cx)
                        })),
                )
            });
        let actions = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(px(8.))
            .child(
                Button::new("eq-save-as")
                    .label(self.t("settings.equalizer.saveAsPreset"))
                    .primary()
                    .outline()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_effects_dialog(DialogKind::SaveAs, String::new(), window, cx)
                    })),
            )
            .child(
                Button::new("eq-export")
                    .label(self.t("settings.equalizer.exportConfig"))
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| this.eq_export(cx))),
            )
            .child(
                Button::new("eq-import")
                    .label(self.t("settings.equalizer.importConfig"))
                    .outline()
                    .on_click(cx.listener(|this, _, window, cx| this.eq_import(window, cx))),
            )
            .child(
                Button::new("eq-vault")
                    .label(self.t("settings.equalizer.downloadPresets"))
                    .outline()
                    .on_click(|_, _, cx| cx.open_url(PRESET_VAULT_URL)),
            );
        let controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(preset_controls)
            .child(actions);

        let global = div()
            .px(px(if narrow { 14. } else { 16. }))
            .py(px(12.))
            .rounded(px(8.))
            .bg(card_bg)
            .border_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .mb(px(8.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.t("settings.equalizer.globalGain")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(secondary)
                            .child(format!("{:.1}dB", self.eq.gains.global)),
                    ),
            )
            .child(crate::skin::colored_playback_slider(
                &self.effects_ui.global,
                &self.effects_ui.global_pressed,
                !enabled,
                mode,
                cx,
                None,
            ));

        div().text_color(primary).child(header).child(
            div()
                .flex()
                .flex_col()
                .gap(px(if narrow { 12. } else { 16. }))
                .child(plot)
                .child(sliders)
                .when_some(params, |column, panel| column.child(panel))
                .child(controls)
                .child(global),
        )
    }

    fn band_slider(
        &self,
        index: usize,
        disabled: bool,
        brand: Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let state = &self.effects_ui.bands[index];
        let pressed = &self.effects_ui.band_pressed[index];
        let percentage = state.read(cx).percentage().end.clamp(0., 1.);
        let thumb = color(mode, "--td-bg-color-container");
        gpui_kit::base::Slider::new(state)
            .vertical()
            .disabled(disabled)
            .w(px(16.))
            .h_full()
            .relative()
            .flex()
            .justify_center()
            .when(disabled, |s| s.opacity(0.4))
            .child(
                gpui_kit::base::SliderIndicator::new(state)
                    .relative()
                    .w(px(4.))
                    .h_full()
                    .rounded(px(2.))
                    .bg(color(mode, "--td-bg-color-component"))
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
                            .left(px(-6.))
                            .bottom(relative(percentage))
                            .mb(px(-8.))
                            .size(px(16.))
                            .rounded(px(8.))
                            .bg(thumb)
                            .border_2()
                            .border_color(brand),
                    ),
            )
            .child(crate::skin::slider_pointer(
                state,
                pressed,
                Axis::Vertical,
                disabled,
            ))
    }

    /// The response curve over the spectrum (`visualizer-container` and its canvas).
    fn response_plot(&self, narrow: bool) -> impl IntoElement {
        let mode = self.ui_colors();
        let brand = color(mode, "--td-brand-color");
        let text = color(mode, "--td-text-color-primary");
        let background = color(mode, "--td-bg-color-page");
        let bar_start = color(mode, "--td-brand-color-5");
        let bar_end = color(mode, "--td-brand-color-7");
        let bands = self.eq.gains.bands.clone();
        let global = self.eq.gains.global;
        let selected = self.effects_ui.selected;
        let spectrum = self.effects_ui.shown.clone();
        let height = if narrow { 180. } else { 250. };
        div()
            .w_full()
            .h(px(height))
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(crate::skin::mixed_color(
                mode,
                "--td-brand-color",
                0.24,
                "--td-border-level-1-color",
            ))
            .bg(background)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let style = PlotStyle {
                            brand,
                            text,
                            background,
                            bar_start,
                            bar_end,
                        };
                        paint_response(
                            bounds, window, cx, &style, &bands, global, selected, &spectrum,
                        );
                    },
                )
                .size_full(),
            )
    }

    fn audio_effect_panel(&self, narrow: bool, cx: &mut Context<Self>) -> Div {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let effects = self.audio_effects.clone();
        let card = |id: &'static str| {
            div()
                .id(id)
                .flex_1()
                .min_w_0()
                .p(px(if narrow { 14. } else { 16. }))
                .rounded(px(8.))
                .border_1()
                .border_color(color(mode, "--td-component-border"))
                .bg(color(mode, "--td-bg-color-container"))
                .flex()
                .flex_col()
                .gap(px(16.))
        };
        let card_header = |title: String, status: String, switch: AnyElement| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .pb(px(8.))
                .border_b_1()
                .border_color(color(mode, "--td-component-stroke"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(16.))
                                .text_color(primary)
                                .child(title),
                        )
                        .when(narrow, |c| {
                            c.child(
                                div()
                                    .mt(px(4.))
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(status),
                            )
                        }),
                )
                .child(switch)
        };

        let surround_disabled = !effects.surround_enabled;
        let mut modes = div()
            .flex()
            .rounded(px(6.))
            .border_1()
            .border_color(color(mode, "--td-component-border"))
            .bg(color(mode, "--td-bg-color-secondarycontainer"))
            .overflow_hidden();
        for (value, key) in SURROUND_MODES {
            let active = effects.surround_mode == value;
            modes = modes.child(
                gpui_kit::base::Button::new(SharedString::from(format!("surround-{key}")))
                    .accessibility_label(self.t(key))
                    .flex_1()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(if active { primary } else { secondary })
                    .when(surround_disabled, |b| b.opacity(0.72))
                    .when(active, |b| {
                        b.bg(color(mode, "--td-bg-color-component-active"))
                            .border_1()
                            .border_color(brand)
                    })
                    .child(self.t(key))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.audio_effects.surround_enabled {
                            this.audio_effects.surround_mode = value;
                            this.save_audio_effects();
                            cx.notify();
                        }
                    })),
            );
        }
        let surround_status = if effects.surround_enabled {
            SURROUND_MODES
                .iter()
                .find(|(m, _)| *m == effects.surround_mode)
                .map(|(_, key)| self.t(key))
                .unwrap_or_default()
        } else {
            self.t("settings.equalizer.off")
        };
        let surround = card("effect-surround")
            .child(card_header(
                self.t("settings.audioEffect.surround"),
                surround_status,
                Switch::new("surround-enabled")
                    .accessibility_label(self.t("settings.audioEffect.surround"))
                    .checked(effects.surround_enabled)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.audio_effects.surround_enabled = *checked;
                        this.save_audio_effects();
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(secondary)
                            .child(self.t("settings.audioEffect.surroundSimulation")),
                    )
                    .child(modes),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(secondary)
                    .text_align(TextAlign::Center)
                    .child(self.t("settings.audioEffect.surroundInfo")),
            );

        let percent = format!("{:.0}", (effects.balance * 100.).abs());
        let balance_text = if effects.balance == 0. {
            self.t("settings.audioEffect.balanceCenter")
        } else if effects.balance < 0. {
            self.t("settings.audioEffect.balanceLeft")
                .replace("{percent}", &percent)
        } else {
            self.t("settings.audioEffect.balanceRight")
                .replace("{percent}", &percent)
        };
        let badge = |text: &'static str, active: bool| {
            div()
                .size(px(32.))
                .rounded(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .bg(color(mode, "--td-bg-color-container"))
                .border_1()
                .border_color(if active {
                    brand
                } else {
                    color(mode, "--td-component-border")
                })
                .text_color(if active { brand } else { secondary })
                .opacity(if active { 1. } else { 0.4 })
                .child(text)
        };
        let balance_enabled = effects.balance_enabled;
        let balance = card("effect-balance")
            .child(card_header(
                self.t("settings.audioEffect.balance"),
                if balance_enabled {
                    balance_text
                } else {
                    self.t("settings.equalizer.off")
                },
                Switch::new("balance-enabled")
                    .accessibility_label(self.t("settings.audioEffect.balance"))
                    .checked(balance_enabled)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.audio_effects.balance_enabled = *checked;
                        this.apply_audio_effects();
                        this.save_audio_effects();
                        cx.notify();
                    }))
                    .into_any_element(),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(12.))
                            .text_color(secondary)
                            .child("Left")
                            .child("Right"),
                    )
                    .child(crate::skin::colored_playback_slider(
                        &self.effects_ui.balance,
                        &self.effects_ui.balance_pressed,
                        !balance_enabled,
                        mode,
                        cx,
                        None,
                    )),
            )
            .child(
                div()
                    .mt(px(10.))
                    .flex()
                    .justify_around()
                    .items_center()
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(8.))
                    .bg(color(mode, "--td-bg-color-secondarycontainer"))
                    .child(badge("L", effects.balance <= 0.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(color(mode, "--td-text-color-placeholder"))
                            .child("Center"),
                    )
                    .child(badge("R", effects.balance >= 0.)),
            )
            .child(
                div().flex().justify_center().child(
                    Button::new("balance-center")
                        .label(self.t("settings.audioEffect.centerCalibration"))
                        .outline()
                        .disabled(!balance_enabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.audio_effects.balance = 0.;
                            this.apply_audio_effects();
                            this.save_audio_effects();
                            cx.notify();
                        })),
                ),
            );

        div()
            .text_color(primary)
            .child(
                div().flex().justify_end().mb(px(14.)).child(
                    Button::new("effects-reset-all")
                        .label(self.t("settings.audioEffect.resetAll"))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.audio_effects = AudioEffects::default();
                            this.apply_audio_effects();
                            this.save_audio_effects();
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .max_w(px(900.))
                    .when(narrow, |grid| grid.flex_col().max_w_full())
                    .child(surround)
                    .child(balance),
            )
    }
}

type PlotBand = equalizer::Band;

struct PlotStyle {
    brand: Hsla,
    text: Hsla,
    background: Hsla,
    bar_start: Hsla,
    bar_end: Hsla,
}

fn frequency_x(frequency: f64, left: f32, width: f32) -> f32 {
    let (min, max) = (20f64.log10(), 20000f64.log10());
    left + (((frequency.log10() - min) / (max - min)) as f32) * width.max(1.)
}
fn db_y(db: f64, top: f32, height: f32) -> f32 {
    top + (((24. - db) / 48.) as f32) * height.max(1.)
}

/// Background, grid, spectrum bars, response curve and band markers
/// (`drawBackground`, `drawSpectrum`, `drawEQCurve`, `drawBandMarkers`).
#[allow(clippy::too_many_arguments)]
fn paint_response(
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
    style: &PlotStyle,
    bands: &[PlotBand],
    global: f64,
    selected: Option<usize>,
    spectrum: &[f64],
) {
    let PlotStyle {
        brand,
        text,
        background,
        bar_start,
        bar_end,
    } = *style;
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let origin = bounds.origin;
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let rect =
        |x: f32, y: f32, w: f32, h: f32| Bounds::new(at(x, y), size(px(w.max(0.)), px(h.max(0.))));
    let plot_left = PLOT_INSET;
    let plot_top = PLOT_INSET;
    let plot_width = (width - PLOT_INSET * 2.).max(1.);
    let plot_height = (height - PLOT_INSET * 2.).max(1.);

    // The page colour fading into a faint brand tint towards the bottom.
    window.paint_quad(fill(bounds, background));
    window.paint_quad(fill(
        rect(0., height * 0.48, width, height * 0.52),
        linear_gradient(
            180.,
            linear_color_stop(background.opacity(0.), 0.),
            linear_color_stop(brand.opacity(0.045), 1.),
        )
        .color_space(ColorSpace::Srgb),
    ));

    let grid = text.opacity(0.055);
    let sub_grid = text.opacity(0.035);
    let label_color = text.opacity(0.38);
    let label = |window: &mut Window, cx: &mut App, value: String, x: f32, y: f32, center: bool| {
        let run = TextRun {
            len: value.len(),
            font: Font {
                family: "Menlo".into(),
                ..Font::default()
            },
            color: label_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            window
                .text_system()
                .shape_line(SharedString::from(value), px(10.), &[run], None);
        let x = if center {
            x - f32::from(line.width()) / 2.
        } else {
            x
        };
        let _ = line.paint(at(x, y - 6.), px(12.), TextAlign::Left, None, window, cx);
    };
    for frequency in GRID_FREQS {
        let x = frequency_x(frequency, plot_left, plot_width).round() + 0.5;
        let major = frequency == 1000. || frequency == 10000.;
        window.paint_quad(fill(
            rect(x, plot_top, if major { 0.8 } else { 0.5 }, plot_height),
            if major { grid } else { sub_grid },
        ));
        label(
            window,
            cx,
            format_frequency(frequency),
            x,
            height - 7.,
            true,
        );
    }
    for db in GRID_DB {
        let y = db_y(f64::from(db), plot_top, plot_height).round() + 0.5;
        if db == 0 {
            let mut path = PathBuilder::stroke(px(1.15)).dash_array(&[px(6.), px(7.)]);
            path.move_to(at(plot_left, y));
            path.line_to(at(plot_left + plot_width, y));
            if let Ok(path) = path.build() {
                window.paint_path(path, brand.opacity(0.34));
            }
        } else {
            let major = db % 12 == 0;
            window.paint_quad(fill(
                rect(plot_left, y, plot_width, if major { 0.7 } else { 0.5 }),
                if major { grid } else { sub_grid },
            ));
        }
        if db % 6 == 0 {
            let value = if db > 0 {
                format!("+{db}")
            } else {
                db.to_string()
            };
            label(window, cx, value, 5., y, false);
        }
    }
    window.paint_quad(outline(
        rect(
            plot_left + 0.5,
            plot_top + 0.5,
            plot_width - 1.,
            plot_height - 1.,
        ),
        text.opacity(0.045),
        BorderStyle::Solid,
    ));

    // Spectrum bars.
    let bar_count = effects::SPECTRUM_BANDS;
    let bar_slot = plot_width / bar_count as f32;
    let bar_width = (bar_slot * 0.55).max(1.);
    let baseline = height - PLOT_INSET;
    let max_bar = plot_height * 0.48;
    let has_signal = spectrum.iter().any(|v| *v > SILENCE_DB + 3.);
    for (i, level) in spectrum.iter().enumerate().take(bar_count) {
        let normalized = ((level - SILENCE_DB) / 80.).clamp(0., 1.) as f32;
        let floor = if has_signal {
            0.
        } else {
            0.018 + ((i as f32 * 1.73).sin() + 1.) * 0.007
        };
        let bar_height = (floor * max_bar).max(normalized.powf(0.68) * max_bar);
        if bar_height < 0.6 {
            continue;
        }
        let x = plot_left + i as f32 * bar_slot + (bar_slot - bar_width) / 2.;
        let top = baseline - bar_height;
        let (low, high) = if has_signal {
            (0.02, 0.28)
        } else {
            (0.025, 0.07)
        };
        window.paint_quad(
            fill(
                rect(x, top, bar_width, bar_height),
                linear_gradient(
                    0.,
                    linear_color_stop(bar_end.opacity(high), 0.),
                    linear_color_stop(bar_start.opacity(low), 1.),
                )
                .color_space(ColorSpace::Srgb),
            )
            .corner_radii(px((bar_width / 2.).min(2.5))),
        );
    }

    // Shared configuration preview: same coefficients and fixed reference on
    // desktop and mobile; only mapping to the drawing frame belongs here.
    let points: Vec<(f32, f32)> = equalizer::response_curve(&equalizer::Gains {
        global,
        bands: bands.to_vec(),
    })
    .iter()
    .map(|p| {
        (
            frequency_x(p.frequency, plot_left, plot_width),
            db_y(p.db.clamp(-24., 24.), plot_top, plot_height),
        )
    })
    .collect();
    let zero_y = db_y(0., plot_top, plot_height);
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        let mut area = PathBuilder::fill();
        area.move_to(at(first.0, zero_y));
        for (x, y) in &points {
            area.line_to(at(*x, *y));
        }
        area.line_to(at(last.0, zero_y));
        area.close();
        if let Ok(path) = area.build() {
            window.paint_path(path, brand.opacity(0.09));
        }
    }
    for (stroke, alpha) in [(8., 0.12), (2.2, 0.92)] {
        let mut curve = PathBuilder::stroke(px(stroke));
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 {
                curve.move_to(at(*x, *y));
            } else {
                curve.line_to(at(*x, *y));
            }
        }
        if let Ok(path) = curve.build() {
            window.paint_path(path, brand.opacity(alpha));
        }
    }

    // Band markers.
    for (i, band) in bands.iter().enumerate() {
        let x = frequency_x(band.frequency, plot_left, plot_width);
        let effective = if band.kind.has_gain() { band.gain } else { 0. };
        let y = db_y(effective.clamp(-24., 24.), plot_top, plot_height);
        let circle = |radius: f32, fill_color: Hsla, border: f32, border_color: Hsla| {
            quad(
                rect(x - radius, y - radius, radius * 2., radius * 2.),
                px(radius),
                fill_color,
                px(border),
                border_color,
                BorderStyle::Solid,
            )
        };
        if selected == Some(i) {
            window.paint_quad(circle(11., transparent_black(), 3., brand.opacity(0.34)));
            window.paint_quad(circle(6.2, brand.opacity(0.86), 1.6, text.opacity(0.86)));
            window.paint_quad(circle(2.2, text.opacity(0.7), 0., transparent_black()));
        } else {
            window.paint_quad(circle(6., transparent_black(), 1.2, brand.opacity(0.28)));
            window.paint_quad(circle(3.2, brand.opacity(0.42), 0.8, text.opacity(0.18)));
        }
    }
}

impl EffectsDialog {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .name
            .as_ref()
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default();
        let kind = self.kind.clone();
        let closed = self
            .parent
            .update(cx, |parent, cx| {
                parent.eq_dialog_confirmed(kind, name, window, cx)
            })
            .unwrap_or(true);
        if closed {
            window.close_dialog(cx);
        }
    }
}

impl Render for EffectsDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let (title, confirm_label, danger) = match self.kind {
            DialogKind::SaveAs => (
                self.t("settings.equalizer.saveNewPresetTitle"),
                self.t("common.confirm"),
                false,
            ),
            DialogKind::TextImport(_) => (
                self.t("settings.equalizer.textImportPresetTitle"),
                self.t("common.confirm"),
                false,
            ),
            DialogKind::Delete => (
                self.t("settings.equalizer.deletePresetTitle"),
                self.t("settings.equalizer.deletePreset"),
                true,
            ),
        };
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt(px(16.))
                    .text_color(secondary)
                    .when_some(self.name.clone(), |body, input| {
                        body.child(gpui_kit::component::input::Input::new(&input))
                    })
                    .when(self.name.is_none(), |body| body.child(self.body.clone())),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(24.))
                    .child(
                        Button::new("effects-dialog-cancel")
                            .label(self.t("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child({
                        let confirm = Button::new("effects-dialog-confirm")
                            .label(confirm_label)
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx)));
                        if danger {
                            confirm.danger()
                        } else {
                            confirm.primary()
                        }
                    }),
            )
    }
}

fn visible_playback(this: &Mio) -> bool {
    this.navigation.current() == Page::Appearance
        && !this.full_player_open
        && this.settings_ui.category == settings_page::Category::Playlist
}
