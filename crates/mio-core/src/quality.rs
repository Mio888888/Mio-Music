//! Download quality choices, following the original `utils/quality.ts`.
use mio_music_sdk::MusicItem;

/// Best first, as `QUALITY_ORDER`.
pub const ORDER: [&str; 8] = [
    "master",
    "atmos_plus",
    "atmos",
    "hires",
    "flac24bit",
    "flac",
    "320k",
    "128k",
];

/// Unconfigured playback prefers 128k, otherwise the provider's first rendition.
/// Callers retain explicit preferences instead of replacing them with this default.
pub fn default_quality(qualities: &[String]) -> Option<&String> {
    qualities
        .iter()
        .find(|quality| quality.as_str() == "128k")
        .or_else(|| qualities.first())
}

fn rank(quality: &str) -> usize {
    ORDER
        .iter()
        .position(|q| *q == quality)
        .unwrap_or(ORDER.len())
}

/// `quality.display*` locale key of a quality, if it has one.
pub fn display_key(quality: &str) -> Option<&'static str> {
    Some(match quality {
        "128k" => "quality.display128k",
        "320k" => "quality.display320k",
        "flac" => "quality.displayFlac",
        "flac24bit" => "quality.displayFlac24bit",
        "hires" => "quality.displayHires",
        "atmos" => "quality.displayAtmos",
        "atmos_plus" => "quality.displayAtmosPlus",
        "master" => "quality.displayMaster",
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub quality: String,
    /// File size the source reports for this quality.
    pub size: Option<String>,
}

/// A row badge requires a real intersection; absent metadata/support shows no quality.
pub fn highest_supported<'a>(item: &'a MusicItem, plugin: &[String]) -> Option<&'a str> {
    item.types
        .as_ref()?
        .iter()
        .filter(|quality| plugin.contains(quality))
        .min_by_key(|quality| rank(quality))
        .map(String::as_str)
}

/// Qualities of one song to pick from: those the song lists (with sizes), narrowed to what the
/// plugin declares for its source (all of them if that would leave none, as the original does),
/// best first. A song that lists none falls back to the plugin's qualities, without sizes.
pub fn choices(item: &MusicItem, plugin: &[String]) -> Vec<Choice> {
    let listed = item.types.as_deref().unwrap_or(&[]);
    let mut all: Vec<Choice> = if listed.is_empty() {
        plugin
            .iter()
            .map(|q| Choice {
                quality: q.clone(),
                size: None,
            })
            .collect()
    } else {
        listed
            .iter()
            .map(|q| Choice {
                quality: q.clone(),
                size: item
                    .types_map
                    .as_ref()
                    .and_then(|m| m.get(q))
                    .map(|info| info.size.clone())
                    .filter(|s| !s.is_empty()),
            })
            .collect()
    };
    if !plugin.is_empty() && !listed.is_empty() {
        let narrowed: Vec<Choice> = all
            .iter()
            .filter(|c| plugin.iter().any(|p| *p == c.quality))
            .cloned()
            .collect();
        if !narrowed.is_empty() {
            all = narrowed;
        }
    }
    all.sort_by_key(|c| rank(&c.quality));
    all.dedup_by(|a, b| a.quality == b.quality);
    all
}

/// `calculateBestQuality`: the wanted quality if offered, else the nearest lower one, else the
/// lowest offered.
pub fn best_available(available: &[String], wanted: &str) -> Option<String> {
    if available.is_empty() {
        return None;
    }
    if available.iter().any(|q| q == wanted) {
        return Some(wanted.to_owned());
    }
    let target = rank(wanted);
    if target == ORDER.len() {
        return available.iter().min_by_key(|q| rank(q)).cloned();
    }
    let lower = available
        .iter()
        .filter(|q| rank(q) != ORDER.len() && rank(q) >= target)
        .min_by_key(|q| rank(q));
    lower
        .or_else(|| available.iter().max_by_key(|q| rank(q)))
        .cloned()
}

/// What the music source settings say about playback quality: the quality chosen per source
/// (`sourceQualityMap`) and the selected source with its `selectQuality`.
#[derive(Default, Clone)]
pub struct Preference {
    pub per_source: std::collections::BTreeMap<String, String>,
    pub selected_source: Option<String>,
    pub selected_quality: Option<String>,
}

static PREFERENCE: std::sync::Mutex<Option<Preference>> = std::sync::Mutex::new(None);

pub fn set_preference(preference: Preference) {
    if let Ok(mut current) = PREFERENCE.lock() {
        *current = Some(preference);
    }
}

pub fn preference() -> Preference {
    PREFERENCE
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .unwrap_or_default()
}
