use serde::{Deserialize, Serialize};

// A malformed new background field must not erase older, valid preferences.
pub fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BackgroundConfig, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value::<BackgroundConfig>(value)
        .map(BackgroundConfig::validated)
        .unwrap_or_else(|_| {
            eprintln!("Invalid background preferences; using default background settings");
            BackgroundConfig::default()
        }))
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    #[default]
    Auto,
    Performance,
    Quality,
    Custom,
}
impl Preset {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Performance, Self::Quality, Self::Custom];
    pub fn key(self) -> &'static str {
        match self {
            Self::Auto => "presetAuto",
            Self::Performance => "presetPerformance",
            Self::Quality => "presetQuality",
            Self::Custom => "presetCustom",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackgroundConfig {
    pub preset: Preset,
    pub enabled: bool,
    pub audio_response: bool,
    pub render_scale: f32,
    pub flow_speed: f32,
    pub static_mode: bool,
    pub fps: u32,
}
impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            preset: Preset::Auto,
            enabled: true,
            audio_response: true,
            render_scale: 0.5,
            flow_speed: 1.,
            static_mode: false,
            fps: 30,
        }
    }
}
impl BackgroundConfig {
    pub fn validated(mut self) -> Self {
        let bounded = |value: f32, min: f32, max: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        self.render_scale = bounded(self.render_scale, 0.1, 1., 0.5);
        self.flow_speed = bounded(self.flow_speed, 0.1, 5., 1.);
        self.fps = self.fps.clamp(15, 60);
        self
    }
    pub fn apply_preset(&mut self, preset: Preset) {
        // Original Auto/Custom only change the mode; reset below restores defaults.
        *self = match preset {
            Preset::Performance => Self {
                preset,
                audio_response: false,
                render_scale: 0.3,
                flow_speed: 0.5,
                static_mode: true,
                fps: 15,
                ..Self::default()
            },
            Preset::Quality => Self {
                preset,
                render_scale: 0.8,
                flow_speed: 1.5,
                fps: 60,
                ..Self::default()
            },
            _ => Self { preset, ..*self },
        };
    }
    /// Used by the macOS background renderer's fallback.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn degraded(self) -> Self {
        Self {
            preset: Preset::Custom,
            audio_response: false,
            render_scale: 0.3,
            flow_speed: 0.5,
            static_mode: true,
            fps: 15,
            ..self
        }
    }
}
