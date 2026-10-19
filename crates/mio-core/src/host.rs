//! The host supplies a live system-language query without pulling UI APIs into core.
use std::sync::OnceLock;

static SYSTEM_LANGUAGE: OnceLock<fn() -> bool> = OnceLock::new();

pub fn set_system_language_resolver(resolve: fn() -> bool) {
    let _ = SYSTEM_LANGUAGE.set(resolve);
}

pub fn system_is_english() -> bool {
    if let Some(resolve) = SYSTEM_LANGUAGE.get() {
        return resolve();
    }
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
        .is_some_and(|language| !language.to_ascii_lowercase().starts_with("zh"))
}
