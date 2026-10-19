//! The equalizer's data and rules (`store/Equalizer.ts` of the original): the ten bands, the
//! built-in and user presets, importing and exporting the configuration (JSON and the
//! "Preamp / Filter n: ON PK Fc … Hz Gain … dB Q …" text form), and what is persisted.
use crate::effects::{
    BandParams, EQ_FREQS, FilterType, NUM_EQ_BANDS, clamp_freq, clamp_gain, clamp_q,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

pub const CONFIG_VERSION: u32 = 3;
pub const FLAT_PRESET_ID: &str = "flat";
const LOG_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub frequency: f64,
    pub gain: f64,
    pub q: f64,
    #[serde(rename = "type")]
    pub kind: FilterType,
    pub enabled: bool,
}

impl Band {
    fn flat(index: usize) -> Self {
        Self {
            frequency: EQ_FREQS[index],
            gain: 0.,
            q: FilterType::Peak.default_q(),
            kind: FilterType::Peak,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gains {
    pub global: f64,
    pub bands: Vec<Band>,
}

impl Gains {
    pub fn flat() -> Self {
        Self {
            global: 0.,
            bands: (0..NUM_EQ_BANDS).map(Band::flat).collect(),
        }
    }
    /// What the audio thread gets for these gains.
    pub fn band_params(&self) -> Vec<BandParams> {
        self.bands
            .iter()
            .map(|band| BandParams {
                kind: band.kind,
                frequency: band.frequency,
                gain: if band.kind.has_gain() { band.gain } else { 0. },
                q: band.q,
                active: band.enabled,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub gains: Gains,
}

/// The desktop and mobile configuration preview share this fixed reference.
/// This is a configured response, independent of playback/bypass or source rate.
pub const RESPONSE_RATE: f64 = 44100.0;
pub const RESPONSE_POINTS: usize = 300;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsePoint {
    pub frequency: f64,
    pub db: f64,
}
pub fn response_curve(gains: &Gains) -> [ResponsePoint; RESPONSE_POINTS] {
    let coeffs: Vec<_> = gains
        .bands
        .iter()
        .take(NUM_EQ_BANDS)
        .filter(|band| band.enabled)
        .map(|band| {
            crate::effects::Coeffs::new(
                band.kind,
                RESPONSE_RATE,
                clamp_freq(band.frequency),
                if band.kind.has_gain() {
                    clamp_gain(band.gain)
                } else {
                    0.0
                },
                clamp_q(band.q),
            )
        })
        .collect();
    let min_log = 20f64.log10();
    let span = 20000f64.log10() - min_log;
    std::array::from_fn(|index| {
        let frequency = 10f64.powf(min_log + index as f64 / (RESPONSE_POINTS - 1) as f64 * span);
        let db = coeffs.iter().fold(clamp_gain(gains.global), |total, c| {
            total + c.response_db(frequency, RESPONSE_RATE)
        });
        // Exact notch zeros may be -infinity. Keep transport and drawing finite;
        // both views clip the displayed response to their +/-24 dB frame.
        ResponsePoint {
            frequency,
            db: if db.is_nan() {
                0.0
            } else {
                db.clamp(-240.0, 240.0)
            },
        }
    })
}

/// `DEFAULT_PRESET_DATA`.
const DEFAULT_PRESETS: [(&str, &str, [f64; NUM_EQ_BANDS]); 8] = [
    ("flat", "Flat", [0., 0., 0., 0., 0., 0., 0., 0., 0., 0.]),
    ("pop", "Pop", [4., 3., 2., 0., -2., -2., 0., 2., 3., 4.]),
    ("rock", "Rock", [5., 4., 3., 1., -1., -1., 1., 3., 4., 5.]),
    ("jazz", "Jazz", [3., 2., 1., 2., -2., -2., 0., 2., 3., 4.]),
    (
        "classical",
        "Classical",
        [4., 3., 2., 1., -1., -1., 0., 2., 3., 4.],
    ),
    (
        "bass_boost",
        "Bass Boost",
        [6., 5., 4., 2., 0., 0., 0., 0., 0., 0.],
    ),
    (
        "vocal_boost",
        "Vocal Boost",
        [-2., -2., -1., 1., 4., 4., 2., 1., 0., -1.],
    ),
    (
        "treble_boost",
        "Treble Boost",
        [0., 0., 0., 0., 0., 1., 3., 5., 6., 7.],
    ),
];

pub fn default_presets() -> Vec<Preset> {
    DEFAULT_PRESETS
        .iter()
        .map(|(id, name, gains)| Preset {
            id: (*id).into(),
            name: (*name).into(),
            is_default: true,
            gains: Gains {
                global: 0.,
                bands: gains
                    .iter()
                    .enumerate()
                    .map(|(i, gain)| Band {
                        gain: *gain,
                        ..Band::flat(i)
                    })
                    .collect(),
            },
        })
        .collect()
}

fn is_default_id(id: &str) -> bool {
    DEFAULT_PRESETS.iter().any(|(default, _, _)| *default == id)
}

/// `LEGACY_PRESET_NAME_TO_ID`.
fn legacy_default_id(name: &str) -> Option<&'static str> {
    let name = name.trim();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let bare = name.split('(').next().unwrap_or("").trim();
    DEFAULT_PRESETS
        .iter()
        .find(|(_, english, _)| *english == bare && (bare == name || name.ends_with(')')))
        .map(|(id, _, _)| *id)
}

/// The locale key of a built-in preset's name (`PRESET_LABEL_KEYS`).
pub fn preset_locale_key(id: &str) -> Option<&'static str> {
    Some(match id {
        "flat" => "settings.equalizer.presetFlat",
        "pop" => "settings.equalizer.presetPop",
        "rock" => "settings.equalizer.presetRock",
        "jazz" => "settings.equalizer.presetJazz",
        "classical" => "settings.equalizer.presetClassical",
        "bass_boost" => "settings.equalizer.presetBassBoost",
        "vocal_boost" => "settings.equalizer.presetVocalBoost",
        "treble_boost" => "settings.equalizer.presetTrebleBoost",
        _ => return None,
    })
}

fn valid_preset_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn sanitize_id_part(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in value.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
            if c == '-' {
                if !dash {
                    out.push('-');
                }
                dash = true;
            } else {
                out.push(c);
                dash = false;
            }
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn unique_user_id(name: &str, used: &mut HashSet<String>) -> String {
    let base = sanitize_id_part(name);
    let base = if base.is_empty() {
        "preset".into()
    } else {
        base
    };
    let mut id = format!("user-{base}");
    let mut index = 2;
    while used.contains(&id) || is_default_id(&id) {
        id = format!("user-{base}-{index}");
        index += 1;
    }
    used.insert(id.clone());
    id
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}

fn normalize_bands(value: &Value) -> Vec<Band> {
    let Some(list) = value.as_array() else {
        return Gains::flat().bands;
    };
    // The v2 format: a bare array of gains.
    if list.first().is_some_and(Value::is_number) {
        return list
            .iter()
            .enumerate()
            .map(|(index, gain)| Band {
                gain: clamp_gain(number(gain).unwrap_or(0.)),
                ..Band::flat(index.min(NUM_EQ_BANDS - 1))
            })
            .collect();
    }
    list.iter()
        .enumerate()
        .map(|(index, item)| {
            let fallback = Band::flat(index.min(NUM_EQ_BANDS - 1));
            let Some(record) = item.as_object() else {
                return fallback;
            };
            let kind = record
                .get("type")
                .and_then(|v| serde_json::from_value::<FilterType>(v.clone()).ok())
                .unwrap_or(FilterType::Peak);
            Band {
                frequency: clamp_freq(
                    record
                        .get("frequency")
                        .and_then(number)
                        .unwrap_or(fallback.frequency),
                ),
                gain: clamp_gain(record.get("gain").and_then(number).unwrap_or(0.)),
                q: clamp_q(record.get("q").and_then(number).unwrap_or(1.414)),
                kind,
                enabled: record
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }
        })
        .collect()
}

fn normalize_gains(value: &Value, strict: bool, label: &str) -> Result<Gains, String> {
    if value
        .as_array()
        .is_some_and(|a| a.first().is_some_and(Value::is_number))
    {
        return Ok(Gains {
            global: 0.,
            bands: normalize_bands(value),
        });
    }
    if let Some(record) = value.as_object() {
        let global = match record.get("global").and_then(number) {
            Some(n) if (-24. ..=24.).contains(&n) => n,
            Some(n) => {
                if strict {
                    return Err(format!("{label}.global 超出 -24..24 dB 范围"));
                }
                clamp_gain(n)
            }
            None => return Err(format!("{label}.global 不是有效数字")),
        };
        return Ok(Gains {
            global,
            bands: normalize_bands(record.get("bands").unwrap_or(&Value::Null)),
        });
    }
    if strict {
        return Err(format!("{label} 格式无效"));
    }
    Ok(Gains::flat())
}

/// What is persisted (`EqualizerPersistedState`) and what an exported file holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub version: u32,
    pub enabled: bool,
    pub current_preset_id: String,
    pub gains: Gains,
    pub user_presets: Vec<Preset>,
    pub logs: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            enabled: false,
            current_preset_id: FLAT_PRESET_ID.into(),
            gains: Gains::flat(),
            user_presets: Vec::new(),
            logs: Vec::new(),
        }
    }
}

fn normalize_user_presets(
    record: &serde_json::Map<String, Value>,
    strict: bool,
) -> Result<Vec<Preset>, String> {
    let (list, from_user_presets) = match (record.get("userPresets"), record.get("presets")) {
        (Some(Value::Array(list)), _) => (list.as_slice(), true),
        (Some(_), _) if strict => return Err("userPresets 必须是数组".into()),
        (_, Some(Value::Array(list))) => (list.as_slice(), false),
        (_, Some(_)) if strict => return Err("presets 必须是数组".into()),
        _ => (&[][..], false),
    };
    let mut used = HashSet::new();
    let mut result = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let Some(item) = item.as_object() else {
            if strict {
                return Err(format!("预设 #{} 格式无效", index + 1));
            }
            continue;
        };
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        if name.is_empty() {
            if strict {
                return Err(format!("预设 #{} 缺少名称", index + 1));
            }
            continue;
        }
        if item.get("isDefault").is_some_and(|v| !v.is_boolean()) {
            if strict {
                return Err(format!("预设 \"{name}\" 的 isDefault 必须是布尔值"));
            }
            continue;
        }
        let raw_id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        let has_default_id = is_default_id(&raw_id);
        let marked_default = item.get("isDefault").and_then(Value::as_bool) == Some(true);
        let legacy_builtin = !from_user_presets
            && raw_id.is_empty()
            && legacy_default_id(&name).is_some()
            && !item.contains_key("originalGains");
        if from_user_presets && (marked_default || has_default_id) {
            if strict {
                return Err(format!("用户预设 \"{name}\" 不能标记为默认预设"));
            }
            continue;
        }
        if !from_user_presets && (has_default_id || legacy_builtin) {
            continue;
        }
        if marked_default {
            if strict {
                return Err(format!("预设 \"{name}\" 使用了无效的默认预设身份"));
            }
            continue;
        }
        let gains = normalize_gains(
            item.get("gains").unwrap_or(&Value::Null),
            strict,
            &format!("预设 \"{name}\".gains"),
        )?;
        let require_id = strict && (from_user_presets || !raw_id.is_empty());
        let id = if raw_id.is_empty() {
            if require_id {
                return Err(format!("预设 \"{name}\" 缺少有效 id"));
            }
            unique_user_id(&name, &mut used)
        } else if !valid_preset_id(&raw_id) {
            if require_id {
                return Err(format!("预设 \"{name}\" 的 id 格式无效"));
            }
            unique_user_id(&name, &mut used)
        } else if is_default_id(&raw_id) {
            if require_id {
                return Err(format!("预设 \"{name}\" 使用了保留的默认预设 id"));
            }
            unique_user_id(&name, &mut used)
        } else if used.contains(&raw_id) {
            if require_id {
                return Err(format!("预设 id \"{raw_id}\" 重复"));
            }
            unique_user_id(&name, &mut used)
        } else {
            used.insert(raw_id.clone());
            raw_id
        };
        result.push(Preset {
            id,
            name,
            is_default: false,
            gains,
        });
    }
    Ok(result)
}

/// `hydrateEqualizerState`: a configuration from a saved file (lenient) or an import (strict).
pub fn hydrate(raw: &Value, strict: bool) -> Result<State, String> {
    let Some(record) = raw.as_object() else {
        if strict {
            return Err("配置根节点必须是对象".into());
        }
        return Ok(State::default());
    };
    let user_presets = normalize_user_presets(record, strict)?;
    let ids: HashSet<String> = DEFAULT_PRESETS
        .iter()
        .map(|(id, _, _)| (*id).to_owned())
        .chain(user_presets.iter().map(|p| p.id.clone()))
        .collect();
    let mut current = None;
    if let Some(id) = record
        .get("currentPresetId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if ids.contains(id) {
            current = Some(id.to_owned());
        } else if strict {
            return Err(format!("当前预设 id \"{id}\" 不存在"));
        }
    }
    if current.is_none() {
        if let Some(name) = record
            .get("currentPreset")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(id) = legacy_default_id(name).filter(|id| ids.contains(*id)) {
                current = Some(id.to_owned());
            } else if let Some(preset) = user_presets.iter().find(|p| p.name == name) {
                current = Some(preset.id.clone());
            } else if strict {
                return Err(format!("当前预设 \"{name}\" 不存在"));
            }
        }
    }
    let current = current.unwrap_or_else(|| FLAT_PRESET_ID.into());
    let gains = match record.get("gains") {
        None | Some(Value::Null) => {
            if strict {
                normalize_gains(&Value::Null, true, "gains")?
            } else {
                find_preset(&current, &user_presets)
                    .map(|p| p.gains)
                    .unwrap_or_else(Gains::flat)
            }
        }
        Some(value) => normalize_gains(value, strict, "gains")?,
    };
    let logs = record
        .get("logs")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .take(LOG_LIMIT)
                .collect()
        })
        .unwrap_or_default();
    Ok(State {
        version: CONFIG_VERSION,
        enabled: record
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        current_preset_id: current,
        gains,
        user_presets,
        logs,
    })
}

pub fn find_preset(id: &str, user_presets: &[Preset]) -> Option<Preset> {
    default_presets()
        .into_iter()
        .find(|p| p.id == id)
        .or_else(|| user_presets.iter().find(|p| p.id == id).cloned())
}

/// `parseTextPreset`: an AutoEq / Equalizer APO style listing.
pub fn parse_text_preset(text: &str) -> Option<Gains> {
    use regex::Regex;
    use std::sync::OnceLock;
    const NUM: &str = r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)";
    static PATTERNS: OnceLock<(Regex, Regex)> = OnceLock::new();
    let (preamp_pattern, filter_pattern) = PATTERNS.get_or_init(|| {
        (
            Regex::new(&format!(r"(?i)^Preamp\s*:\s*({NUM})\s*dB\b")).unwrap(),
            Regex::new(&format!(
                r"(?i)^Filter\s+(\d+)\s*:\s*(ON|OFF)\s+(.+?)\s+Fc\s*({NUM})\s*Hz\s+Gain\s*({NUM})\s*dB\s+Q\s*({NUM})\s*$"
            ))
            .unwrap(),
        )
    });
    let filter_kind = |name: &str| -> Option<FilterType> {
        let compact: String = name
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        Some(match compact.as_str() {
            "pk" | "peak" | "peaking" | "peakeq" | "peakingeq" | "bell" => FilterType::Peak,
            "ls" | "lsc" | "lowshelf" | "lowshelving" => FilterType::LowShelf,
            "hs" | "hsc" | "highshelf" | "highshelving" => FilterType::HighShelf,
            "lp" | "lpf" | "lowpass" | "lowpassfilter" => FilterType::LowPass,
            "hp" | "hpf" | "highpass" | "highpassfilter" => FilterType::HighPass,
            "no" | "notch" | "bandreject" | "bandstop" => FilterType::Notch,
            _ => return None,
        })
    };
    let mut bands = Gains::flat().bands;
    let mut global = 0.;
    let mut parsed_any = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.starts_with("preamp") && !lower[6..].starts_with(|c: char| c.is_alphanumeric()) {
            let captures = preamp_pattern.captures(line)?;
            global = clamp_gain(captures[1].parse().ok()?);
            continue;
        }
        if !(lower.starts_with("filter") && !lower[6..].starts_with(|c: char| c.is_alphanumeric()))
        {
            continue;
        }
        let captures = filter_pattern.captures(line)?;
        let index = captures[1].parse::<usize>().ok()?.checked_sub(1)?;
        if index >= NUM_EQ_BANDS {
            continue;
        }
        let kind = filter_kind(&captures[3])?;
        let frequency: f64 = captures[4].parse().ok()?;
        let gain: f64 = captures[5].parse().ok()?;
        let q: f64 = captures[6].parse().ok()?;
        bands[index] = Band {
            frequency: clamp_freq(frequency),
            gain: if kind.has_gain() {
                clamp_gain(gain)
            } else {
                0.
            },
            q: clamp_q(q),
            kind,
            enabled: captures[2].eq_ignore_ascii_case("ON"),
        };
        parsed_any = true;
    }
    parsed_any.then_some(Gains { global, bands })
}

/// The file `exportConfig` writes (`mio-eq-config.json`).
pub fn export_config(state: &State) -> Value {
    let defaults = default_presets();
    let mut presets = defaults.clone();
    presets.extend(state.user_presets.clone());
    serde_json::json!({
        "version": CONFIG_VERSION,
        "enabled": state.enabled,
        "currentPresetId": state.current_preset_id,
        "gains": state.gains,
        "presets": presets,
        "defaultPresets": defaults,
        "userPresets": state.user_presets,
    })
}

/// Ten bands is a fixed shape; fix up whatever a file held.
pub fn fit_bands(gains: &mut Gains) {
    gains.bands.truncate(NUM_EQ_BANDS);
    while gains.bands.len() < NUM_EQ_BANDS {
        gains.bands.push(Band::flat(gains.bands.len()));
    }
}

impl State {
    pub fn add_log(&mut self, message: String) {
        self.logs.insert(0, message);
        self.logs.truncate(LOG_LIMIT);
    }
    pub fn presets(&self) -> Vec<Preset> {
        let mut all = default_presets();
        all.extend(self.user_presets.clone());
        all
    }
    pub fn current_preset(&self) -> Option<Preset> {
        find_preset(&self.current_preset_id, &self.user_presets)
    }
    pub fn name_taken(&self, name: &str, exclude: Option<&str>) -> bool {
        let name = name.trim().to_lowercase();
        !name.is_empty()
            && self
                .presets()
                .iter()
                .any(|p| Some(p.id.as_str()) != exclude && p.name.trim().to_lowercase() == name)
    }
    pub fn apply_preset(&mut self, id: &str) -> bool {
        let Some(preset) = find_preset(id, &self.user_presets) else {
            return false;
        };
        self.current_preset_id = preset.id.clone();
        self.gains = preset.gains;
        self.add_log(format!("Preset switched: {}", preset.name));
        true
    }
    pub fn set_band_gain(&mut self, index: usize, gain: f64) {
        if let Some(band) = self.gains.bands.get_mut(index) {
            band.gain = clamp_gain(gain);
        }
    }
    pub fn set_band_frequency(&mut self, index: usize, frequency: f64) {
        if let Some(band) = self.gains.bands.get_mut(index) {
            band.frequency = clamp_freq(frequency);
        }
    }
    pub fn set_band_q(&mut self, index: usize, q: f64) {
        if let Some(band) = self.gains.bands.get_mut(index) {
            band.q = clamp_q(q);
        }
    }
    pub fn set_band_type(&mut self, index: usize, kind: FilterType) {
        if let Some(band) = self.gains.bands.get_mut(index) {
            band.kind = kind;
            band.q = kind.default_q();
            if !kind.has_gain() {
                band.gain = 0.;
            }
        }
    }
    /// `createUserPresetFromGains`; the error is the locale key to show.
    pub fn create_user_preset(
        &mut self,
        name: &str,
        gains: Gains,
        apply: bool,
    ) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() {
            return Err("settings.equalizer.textImportNameRequired");
        }
        if self.name_taken(name, None) {
            return Err("settings.equalizer.presetExists");
        }
        let mut used: HashSet<String> = self.user_presets.iter().map(|p| p.id.clone()).collect();
        let id = unique_user_id(name, &mut used);
        self.user_presets.push(Preset {
            id: id.clone(),
            name: name.to_owned(),
            is_default: false,
            gains: gains.clone(),
        });
        self.current_preset_id = id;
        if apply {
            self.gains = gains;
        }
        self.add_log(format!("Saved new preset \"{name}\""));
        Ok(())
    }
    pub fn update_user_preset(&mut self, id: &str) -> bool {
        let gains = self.gains.clone();
        let Some(preset) = self.user_presets.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        preset.gains = gains;
        let name = preset.name.clone();
        self.add_log(format!("Updated preset \"{name}\""));
        true
    }
    pub fn delete_user_preset(&mut self, id: &str) -> bool {
        let Some(index) = self.user_presets.iter().position(|p| p.id == id) else {
            return false;
        };
        let removed = self.user_presets.remove(index);
        if self.current_preset_id == id {
            self.apply_preset(FLAT_PRESET_ID);
        }
        self.add_log(format!("Deleted preset: {}", removed.name));
        true
    }
}
