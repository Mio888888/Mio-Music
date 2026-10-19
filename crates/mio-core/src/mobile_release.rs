//! Read-only mobile Release notices. Installation stays in the system browser.
use serde::Deserialize;
use std::{io::Read, time::Duration};

const ENDPOINT: &str = "https://api.github.com/repos/Mio888888/Mio-Music/releases/latest";
const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_NOTES: usize = 32 * 1024;

#[derive(Clone)]
pub struct Release {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub notes_truncated: bool,
    pub newer: bool,
}
#[derive(Deserialize)]
struct Response {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
    published_at: String,
}

pub fn installed_version(value: &str) -> Result<semver::Version, String> {
    if value.len() > 128 || value.is_empty() {
        return Err("invalid_version".into());
    }
    semver::Version::parse(value).map_err(|_| "invalid_version".into())
}

pub fn check(
    installed: &semver::Version,
    cancelled: &dyn Fn() -> Result<(), String>,
) -> Result<Release, String> {
    cancelled()?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("mio-music-mobile/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "release_unavailable")?;
    // No credentials, custom endpoint, artifact download or installation.
    let mut response = client
        .get(ENDPOINT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .map_err(|_| "release_network")?;
    cancelled()?;
    match response.status().as_u16() {
        200 => (),
        403 | 429 => return Err("release_rate_limited".into()),
        404 => return Err("release_not_public".into()),
        _ => return Err("release_unavailable".into()),
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err("release_invalid".into());
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        cancelled()?;
        let count = response.read(&mut chunk).map_err(|_| "release_network")?;
        if count == 0 {
            break;
        }
        if bytes
            .len()
            .checked_add(count)
            .is_none_or(|n| n > MAX_RESPONSE)
        {
            return Err("release_invalid".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    cancelled()?;
    // Derived deserialization rejects duplicate known fields. Unknown GitHub
    // metadata is skipped and never becomes a URL or an executable asset.
    let release: Response = serde_json::from_slice(&bytes).map_err(|_| "release_invalid")?;
    if release.draft
        || release.prerelease
        || release.tag_name.len() > 128
        || !release
            .tag_name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".+-".contains(&c))
        || release.published_at.len() > 128
        || chrono::DateTime::parse_from_rfc3339(&release.published_at).is_err()
    {
        return Err("release_invalid".into());
    }
    let version = installed_version(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "release_invalid")?;
    if !version.pre.is_empty() {
        return Err("release_invalid".into());
    }
    let mut notes = release.body.unwrap_or_default();
    let notes_truncated = notes.len() > MAX_NOTES;
    if notes_truncated {
        let mut end = MAX_NOTES;
        while !notes.is_char_boundary(end) {
            end -= 1;
        }
        notes.truncate(end);
    }
    // Render only plain text; discard invisible control and bidi override marks.
    notes.retain(|c| {
        (!c.is_control() || matches!(c, '\n' | '\t'))
            && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    });
    cancelled()?;
    Ok(Release {
        newer: version.cmp_precedence(installed).is_gt(),
        version: version.to_string(),
        tag: release.tag_name,
        notes,
        notes_truncated,
    })
}

impl crate::app_core::AppCore {
    /// Owner-only compare-and-save. Publish only after the atomic file save.
    pub fn save_auto_update(&mut self, expected: bool, enabled: bool) -> Result<(), String> {
        if self.preferences.app.auto_update != expected {
            return Err("settings_changed".into());
        }
        if expected == enabled {
            return Ok(());
        }
        let mut next = self.preferences.clone();
        next.app.auto_update = enabled;
        self.store
            .save("preferences.json", &next)
            .map_err(|_| "settings_not_saved")?;
        self.preferences = next;
        Ok(())
    }
}
