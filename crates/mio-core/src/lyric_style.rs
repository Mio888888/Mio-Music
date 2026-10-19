//! Bounded projection of the existing desktop lyric typography preferences.
use crate::storage::AppSettings;
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub family: String,
    pub rate: f32,
    pub weight: u32,
}
impl Config {
    pub fn from_app(app: &AppSettings) -> Self {
        Self {
            family: if valid_family(&app.lyric_font_family) {
                app.lyric_font_family.clone()
            } else {
                "lyricfont".into()
            },
            rate: if app.lyric_font_rate.is_finite() {
                app.lyric_font_rate.clamp(0.1, 2.0)
            } else {
                1.0
            },
            weight: app.lyric_font_weight.clamp(100, 900),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !valid_family(&self.family)
            || !self.rate.is_finite()
            || !(0.1..=2.0).contains(&self.rate)
            || !(100..=900).contains(&self.weight)
        {
            return Err("invalid_input: lyric typography".into());
        }
        Ok(())
    }
}
fn valid_family(value: &str) -> bool {
    value.len() <= 1024
        && value.chars().count() <= 256
        && !value.chars().any(char::is_control)
        && !value.trim().is_empty()
        && value.split(',').count() <= 8
}
