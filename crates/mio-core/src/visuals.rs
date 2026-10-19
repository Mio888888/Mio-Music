//! Shared projection of desktop visual switches; mobile policy never overwrites them.
use crate::storage::Preferences;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Auto,
    Reduced,
    Full,
}
pub fn deserialize_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Mode, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub immersive_lyric_color: bool,
    pub cover_layout: bool,
    pub show_left_panel: bool,
    pub auto_hide_bottom: bool,
    pub pause_transition: bool,
    pub cover_dynamic_theme: bool,
    pub background: bool,
    pub blur_lyrics: bool,
    pub spectrum: bool,
    pub jump_lyrics: bool,
    pub mode: Mode,
}
impl Config {
    pub fn from_preferences(value: &Preferences) -> Self {
        Self {
            immersive_lyric_color: value.immersive_lyric_color,
            cover_layout: value.cover_layout,
            show_left_panel: value.show_left_panel,
            auto_hide_bottom: value.auto_hide_bottom,
            pause_transition: value.pause_transition,
            cover_dynamic_theme: value.app.cover_dynamic_theme,
            background: value.background.enabled,
            blur_lyrics: value.blur_lyrics,
            spectrum: value.audio_visualization,
            jump_lyrics: value.app.jump_lyric,
            mode: value.app.mobile_visual_mode,
        }
    }
    pub fn apply(self, value: &mut Preferences) {
        value.immersive_lyric_color = self.immersive_lyric_color;
        value.cover_layout = self.cover_layout;
        value.show_left_panel = self.show_left_panel;
        value.auto_hide_bottom = self.auto_hide_bottom;
        value.pause_transition = self.pause_transition;
        value.app.cover_dynamic_theme = self.cover_dynamic_theme;
        value.background.enabled = self.background;
        value.blur_lyrics = self.blur_lyrics;
        value.audio_visualization = self.spectrum;
        value.app.jump_lyric = self.jump_lyrics;
        value.app.mobile_visual_mode = self.mode;
    }
}
