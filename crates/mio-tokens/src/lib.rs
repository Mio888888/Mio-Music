use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Deserialize)]
struct SourceTokens {
    themes: BTreeMap<String, BTreeMap<String, String>>,
}
static TOKENS: OnceLock<SourceTokens> = OnceLock::new();

fn tokens() -> &'static SourceTokens {
    TOKENS.get_or_init(|| {
        serde_json::from_str(include_str!("../tokens.json")).expect("generated CSS tokens")
    })
}

pub fn raw(mode: Mode, key: &str) -> Option<&'static str> {
    tokens()
        .themes
        .get(mode.name())?
        .get(key)
        .map(String::as_str)
}

/// Resolve only whole-value var() aliases. Complex CSS expressions stay explicit errors.
pub fn resolve(mode: Mode, key: &str) -> Result<&'static str, String> {
    let mut visited = BTreeSet::new();
    let mut key = key;
    loop {
        if !visited.insert(key.to_owned()) {
            return Err(format!("CSS token cycle: {key}"));
        }
        let value = raw(mode, key).ok_or_else(|| format!("Missing CSS token: {key}"))?;
        match value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) {
            Some(alias) if alias.starts_with("--") && !alias.contains(',') => key = alias.trim(),
            _ => return Ok(value),
        }
    }
}

/// RGBA channels in 0..1, preserving original alpha.
pub fn color(mode: Mode, key: &str) -> Result<[f32; 4], String> {
    // Every colour token is parsed once per mode: rendering asks for them on every frame.
    static PARSED: [OnceLock<std::collections::HashMap<&'static str, [f32; 4]>>; 2] =
        [OnceLock::new(), OnceLock::new()];
    let parsed = PARSED[mode as usize].get_or_init(|| {
        tokens()
            .themes
            .get(mode.name())
            .into_iter()
            .flat_map(|theme| theme.keys())
            .filter_map(|key| parse_color(mode, key).ok().map(|rgba| (key.as_str(), rgba)))
            .collect()
    });
    match parsed.get(key) {
        Some(rgba) => Ok(*rgba),
        None => parse_color(mode, key),
    }
}

fn parse_color(mode: Mode, key: &str) -> Result<[f32; 4], String> {
    let value = resolve(mode, key)?;
    if value == "transparent" {
        return Ok([0.0; 4]);
    }
    if let Some(hex) = value.strip_prefix('#') {
        let expanded = match hex.len() {
            3 | 4 => hex.chars().flat_map(|ch| [ch, ch]).collect::<String>(),
            6 | 8 => hex.to_owned(),
            _ => return Err(format!("Invalid hex color: {value}")),
        };
        let mut rgba = [1.0; 4];
        for (index, channel) in expanded.as_bytes().chunks(2).enumerate() {
            rgba[index] =
                u8::from_str_radix(std::str::from_utf8(channel).map_err(|e| e.to_string())?, 16)
                    .map_err(|e| e.to_string())? as f32
                    / 255.0;
        }
        return Ok(rgba);
    }
    if let Some(body) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
        .and_then(|v| v.strip_suffix(')'))
    {
        let parts = body
            .split(',')
            .map(|v| v.trim().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if parts.len() == 3 || parts.len() == 4 {
            return Ok([
                parts[0] / 255.0,
                parts[1] / 255.0,
                parts[2] / 255.0,
                *parts.get(3).unwrap_or(&1.0),
            ]);
        }
    }
    Err(format!("Unsupported CSS color: {key} = {value}"))
}

pub fn rem_size(width: f32) -> f32 {
    if width <= 768.0 {
        16.0
    } else {
        (width * 0.014).clamp(15.0, 18.0)
    }
}
pub fn player_height(height: f32) -> f32 {
    (height * 0.1).clamp(70.0, 80.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_tokens_and_viewport_rules() {
        assert_eq!(resolve(Mode::Light, "--td-brand-color").unwrap(), "#03de6d");
        assert_eq!(resolve(Mode::Dark, "--td-brand-color").unwrap(), "#00a74d");
        assert_eq!(
            color(Mode::Light, "--td-text-color-primary").unwrap(),
            [0., 0., 0., 0.9]
        );
        assert_eq!(
            resolve(Mode::Light, "--motion-duration-standard").unwrap(),
            "280ms"
        );
        for mode in [Mode::Light, Mode::Dark] {
            for key in [
                "--td-bg-color-page",
                "--td-bg-color-container",
                "--td-bg-color-secondarycontainer",
                "--td-border-level-1-color",
                "--td-text-color-secondary",
                "--player-bg-idle",
                "--player-text-idle",
                "--player-btn-bg-idle",
            ] {
                color(mode, key).unwrap_or_else(|error| panic!("{error}"));
            }
        }
        assert_eq!(rem_size(768.), 16.);
        assert_eq!(rem_size(800.), 15.);
        assert!((rem_size(1200.) - 16.8).abs() < 0.0001);
        assert_eq!(rem_size(1440.), 18.);
        assert_eq!(player_height(600.), 70.);
        assert_eq!(player_height(800.), 80.);
        assert!(resolve(Mode::Light, "--missing").is_err());
    }
}
