//! Bounded immutable restore preparation and owner-only settings application.
//! Preparation itself performs no code execution, network access, or writes.
use crate::library::{BackupSummary, PreparedBackup};
use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

pub const MAX_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ENCRYPTED_BYTES: usize = (MAX_DOCUMENT_BYTES + 49).div_ceil(3) * 4 + 64;
const MAX_NODES: usize = 500_000;

impl crate::app_core::AppCore {
    /// Only the settings this backup format can modify participate in admission.
    pub fn backup_settings_snapshot(&self) -> Value {
        let cache = self.paths.cache.to_string_lossy();
        let downloads = self.paths.downloads.to_string_lossy();
        crate::backup::settings_json(
            &self.preferences,
            &crate::backup::Directories {
                cache_dir: &cache,
                download_dir: &downloads,
            },
        )
    }

    /// Owner-only, single-file save, followed by publishing the same download/cache
    /// configuration. Library and plugin transactions have separate outcomes.
    pub fn restore_backup_settings(
        &mut self,
        expected: &Value,
        incoming: &Value,
        mode: crate::backup::Mode,
    ) -> Result<(), String> {
        if &self.backup_settings_snapshot() != expected {
            return Err("settings_changed: reopen backup".into());
        }
        validate_settings(incoming)?;
        let mut next = self.preferences.clone();
        let cache = self.paths.cache.to_string_lossy();
        let downloads = self.paths.downloads.to_string_lossy();
        let directories = crate::backup::Directories {
            cache_dir: &cache,
            download_dir: &downloads,
        };
        crate::backup::apply_settings(&mut next, &directories, incoming, mode);
        if crate::backup::settings_json(&next, &directories) == *expected {
            return Ok(());
        }
        let output = crate::storage_settings::Config::from_preferences(&next);
        output.validate()?;
        crate::lyric_style::Config::from_app(&next.app).validate()?;
        let cache = output
            .auto_cache
            .then(|| (self.paths.cache.clone(), output.cache_limit));
        let manager = self.downloads.clone();
        crate::audio_cache::configure_when(cache, || {
            manager.configure_restore_output_when(&output, &next.tag_write_options, || {
                self.store.save("preferences.json", &next).map_err(|_| {
                    "storage_unavailable: backup settings could not be saved".to_owned()
                })
            })
        })?;
        self.preferences = next;
        Ok(())
    }
}
pub struct Plugin {
    id: String,
    name: String,
    code: String,
}
impl Plugin {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn code(&self) -> &str {
        &self.code
    }
}
pub struct PreparedRestore {
    timestamp: String,
    library: PreparedBackup,
    settings: Value,
    plugins: Vec<Plugin>,
}
impl PreparedRestore {
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    pub fn summary(&self) -> BackupSummary {
        self.library.summary()
    }
    pub fn settings(&self) -> &Value {
        &self.settings
    }
    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }
    pub fn library(&self) -> &PreparedBackup {
        &self.library
    }
    pub fn from_payload(
        payload: crate::backup::Payload,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let document = Value::Object(
            [
                ("version".into(), Value::from(3)),
                ("timestamp".into(), Value::String(timestamp)),
                ("settings".into(), payload.settings),
                ("plugins".into(), Value::Array(payload.plugins)),
                ("playlists".into(), payload.playlists),
            ]
            .into_iter()
            .collect(),
        );
        Self::from_document(document, check)
    }

    pub fn document_bytes(
        &self,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Vec<u8>, String> {
        #[derive(serde::Serialize)]
        struct SavedPlugin<'a> {
            id: &'a str,
            name: &'a str,
            code: &'a str,
            config: Option<()>,
        }
        #[derive(serde::Serialize)]
        struct Document<'a> {
            version: u8,
            timestamp: &'a str,
            playlists: &'a PreparedBackup,
            settings: &'a Value,
            plugins: Vec<SavedPlugin<'a>>,
        }
        struct Limited<'a> {
            bytes: Vec<u8>,
            check: &'a dyn Fn() -> Result<(), String>,
        }
        impl std::io::Write for Limited<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                (self.check)().map_err(std::io::Error::other)?;
                if self
                    .bytes
                    .len()
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > MAX_DOCUMENT_BYTES)
                {
                    return Err(std::io::Error::other("document limit"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let document = Document {
            version: 3,
            timestamp: self.timestamp(),
            playlists: &self.library,
            settings: self.settings(),
            plugins: self
                .plugins
                .iter()
                .map(|p| SavedPlugin {
                    id: p.id(),
                    name: p.name(),
                    code: p.code(),
                    config: None,
                })
                .collect(),
        };
        let mut output = Limited {
            bytes: Vec::new(),
            check,
        };
        if serde_json::to_writer(&mut output, &document).is_err() {
            check()?;
            return Err("capacity_exceeded: backup document".into());
        }
        Ok(output.bytes)
    }

    pub fn from_encrypted(
        body: String,
        password: &str,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        check()?;
        if body.len() > MAX_ENCRYPTED_BYTES || password.is_empty() || password.len() > 4096 {
            return Err("invalid_input: backup envelope or password limit".into());
        }
        let plain = crate::backup::decode_envelope(&body, password)
            .map_err(|_| "invalid_data: backup password or encrypted data")?;
        drop(body);
        check()?;
        if plain.len() > MAX_DOCUMENT_BYTES {
            return Err("capacity_exceeded: backup document".into());
        }
        let value = parse_json(&plain, check)?;
        drop(plain);
        Self::from_document(value, check)
    }

    pub(crate) fn from_document(
        value: Value,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        check()?;
        let Value::Object(mut object) = value else {
            return Err("invalid_data: backup document".into());
        };
        if object.remove("version").and_then(|v| v.as_u64()) != Some(3) {
            return Err("invalid_data: backup version".into());
        }
        let timestamp = object
            .remove("timestamp")
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|v| v.len() <= 128 && chrono::DateTime::parse_from_rfc3339(v).is_ok())
            .ok_or("invalid_data: backup timestamp")?;
        let settings = object
            .remove("settings")
            .ok_or("invalid_data: missing backup settings")?;
        validate_settings(&settings)?;
        let plugins = object
            .remove("plugins")
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let mut plugins = match plugins {
            Value::Array(v) if v.len() <= 16 => v,
            _ => return Err("capacity_exceeded: backup plugins".into()),
        };
        let mut seen = std::collections::HashSet::new();
        let mut prepared = Vec::new();
        for value in plugins.drain(..) {
            check()?;
            let get = |key: &str, max: usize| -> Result<String, String> {
                value
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty() && v.len() <= max && !v.contains('\0'))
                    .map(str::to_owned)
                    .ok_or_else(|| "invalid_data: backup plugin field".into())
            };
            let id = get("id", 128)?;
            if !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !seen.insert(id.clone())
            {
                return Err("invalid_data: duplicate or invalid backup plugin".into());
            }
            prepared.push(Plugin {
                id,
                name: get("name", 1024)?,
                code: get("code", 1024 * 1024)?,
            });
        }
        let library = PreparedBackup::prepare(
            object
                .remove("playlists")
                .ok_or("invalid_data: missing backup playlists")?,
            check,
        )?;
        check()?;
        Ok(Self {
            timestamp,
            library,
            settings,
            plugins: prepared,
        })
    }
}

fn validate_settings(settings: &Value) -> Result<(), String> {
    let object = settings
        .as_object()
        .filter(|m| m.len() <= 256)
        .ok_or("invalid_data: backup settings")?;
    // Bound even unknown legacy fields. They remain immutable and are never host paths.
    struct Limit(usize);
    impl std::io::Write for Limit {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(b.len())
                .filter(|n| *n <= 64 * 1024)
                .ok_or_else(|| std::io::Error::other("settings limit"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Limit(0), settings).map_err(|_| "capacity_exceeded: backup settings")?;
    for key in [
        "autoCacheMusic",
        "autoUpdate",
        "closeToTray",
        "hasConfiguredCloseBehavior",
        "routePreloadEnabled",
        "skipHiddenFiles",
    ] {
        if object.get(key).is_some_and(|v| !v.is_boolean()) {
            return Err("invalid_data: backup boolean setting".into());
        }
    }
    for key in ["filenameTemplate", "lyricFontFamily", "language"] {
        if object.get(key).is_some_and(|v| {
            v.as_str().is_none_or(|s| {
                s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control)
            })
        }) {
            return Err("invalid_data: backup text setting".into());
        }
    }
    if object
        .get("language")
        .and_then(Value::as_str)
        .is_some_and(|s| !matches!(s, "system" | "en-US" | "zh-CN"))
    {
        return Err("invalid_data: backup language".into());
    }
    if object
        .get("filenameTemplate")
        .and_then(Value::as_str)
        .is_some_and(|s| s.chars().count() > 256)
    {
        return Err("invalid_data: backup filename template".into());
    }
    if object
        .get("cacheSizeLimit")
        .is_some_and(|v| v.as_u64().is_none())
        || object
            .get("lyricFontWeight")
            .is_some_and(|v| v.as_u64().is_none_or(|n| !(100..=900).contains(&n)))
        || object
            .get("FullPlayLyricFontRate")
            .is_some_and(|v| v.as_f64().is_none_or(|n| !(0.1..=2.).contains(&n)))
    {
        return Err("invalid_data: backup numeric setting".into());
    }
    if let Some(tags) = object.get("tagWriteOptions") {
        let tags = tags
            .as_object()
            .ok_or("invalid_data: backup tag settings")?;
        for key in ["basicInfo", "cover", "lyrics", "downloadLyrics"] {
            if tags.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err("invalid_data: backup tag setting".into());
            }
        }
        if tags.get("lyricFormat").is_some_and(|v| {
            v.as_str()
                .is_none_or(|s| !matches!(s, "lrc" | "word-by-word"))
        }) {
            return Err("invalid_data: backup lyric format".into());
        }
    }
    if let Some(background) = object.get("backgroundRender") {
        let background = background
            .as_object()
            .ok_or("invalid_data: backup background settings")?;
        if let Some(full) = background.get("fullPlay") {
            let full = full
                .as_object()
                .ok_or("invalid_data: backup full-player settings")?;
            for key in ["enabled", "audioResponse", "staticMode"] {
                if full.get(key).is_some_and(|v| !v.is_boolean()) {
                    return Err("invalid_data: backup background setting".into());
                }
            }
            if full.get("preset").is_some_and(|v| {
                v.as_str()
                    .is_none_or(|s| !matches!(s, "auto" | "performance" | "quality" | "custom"))
            }) || full
                .get("renderScale")
                .is_some_and(|v| v.as_f64().is_none_or(|n| !(0.1..=1.).contains(&n)))
                || full
                    .get("flowSpeed")
                    .is_some_and(|v| v.as_f64().is_none_or(|n| !(0.1..=5.).contains(&n)))
                || full
                    .get("fps")
                    .is_some_and(|v| v.as_u64().is_none_or(|n| !(15..=60).contains(&n)))
            {
                return Err("invalid_data: backup background setting".into());
            }
        }
    }
    Ok(())
}

struct Seed<'a> {
    nodes: &'a mut usize,
    check: &'a dyn Fn() -> Result<(), String>,
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if *self.nodes == 0 {
            return Err(D::Error::custom("node limit"));
        }
        *self.nodes -= 1;
        if *self.nodes % 256 == 0 {
            (self.check)().map_err(D::Error::custom)?;
        }
        d.deserialize_any(JsonVisitor(self))
    }
}
struct JsonVisitor<'a>(Seed<'a>);
impl<'de> Visitor<'de> for JsonVisitor<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded JSON without duplicate fields")
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("number"))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: Error>(self, v: String) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = a.next_element_seed(Seed {
            nodes: self.0.nodes,
            check: self.0.check,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = a.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate field"));
            }
            let value = a.next_value_seed(Seed {
                nodes: self.0.nodes,
                check: self.0.check,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
pub(crate) fn parse_json(
    bytes: &[u8],
    check: &dyn Fn() -> Result<(), String>,
) -> Result<Value, String> {
    check()?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err("capacity_exceeded: backup JSON".into());
    }
    let mut nodes = MAX_NODES;
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let value = Seed {
        nodes: &mut nodes,
        check,
    }
    .deserialize(&mut d)
    .map_err(|_| {
        check()
            .err()
            .unwrap_or_else(|| "invalid_data: backup JSON".into())
    })?;
    d.end().map_err(|_| "invalid_data: trailing backup JSON")?;
    check()?;
    Ok(value)
}
