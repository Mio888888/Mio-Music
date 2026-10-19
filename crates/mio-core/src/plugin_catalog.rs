//! The music-list plugins shipped inside the application. Each is the same `*.list.js` script a
//! user could import from a file; [`crate::list_plugins::restore`] installs any that are missing
//! and updates older copies at startup, so they are always present as ordinary installed plugins
//! (configurable, switchable) without an install step.
use std::sync::OnceLock;

const SCRIPTS: &[&str] = &[
    include_str!("../../../plugins/songloft.list.js"),
    include_str!("../../../plugins/subsonic.list.js"),
    include_str!("../../../plugins/navidrome.list.js"),
    include_str!("../../../plugins/jellyfin.list.js"),
    include_str!("../../../plugins/emby.list.js"),
    include_str!("../../../plugins/plex.list.js"),
];

/// A bundled plugin: its script and the `@name` it installs under.
pub struct Bundled {
    pub name: String,
    pub script: &'static str,
}

/// A `@key value` line of the script's header comment.
fn header(script: &str, key: &str) -> String {
    let tag = format!("@{key} ");
    script
        .lines()
        .take_while(|line| {
            let line = line.trim();
            line.is_empty() || line.starts_with("/*") || line.starts_with('*')
        })
        .find_map(|line| {
            line.trim()
                .trim_start_matches('*')
                .trim()
                .strip_prefix(tag.as_str())
        })
        .unwrap_or("")
        .trim()
        .to_owned()
}

/// The bundled plugins, in display order.
pub fn plugins() -> &'static [Bundled] {
    static BUNDLED: OnceLock<Vec<Bundled>> = OnceLock::new();
    BUNDLED.get_or_init(|| {
        SCRIPTS
            .iter()
            .map(|script| Bundled {
                name: header(script, "name"),
                script,
            })
            .collect()
    })
}

/// Whether an installed plugin of this name ships with the application. Installs match plugins
/// by name, so this is how a bundled plugin is recognized after installation.
pub fn is_bundled(name: &str) -> bool {
    plugins().iter().any(|p| p.name == name)
}

/// The numeric parts of a version, for ordering (`1.2.10` is newer than `1.2.9`).
pub(crate) fn version_key(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}
