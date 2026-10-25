//! Installed music-list plugins (`mio_plugin::ListPlugin`): installation, enabling and the
//! registration of their providers, without built-in source fallbacks.
//! Kept apart from the playback plugins of `online.rs`: a list
//! plugin is not "the active plugin": several may be enabled at once. A plugin may also be
//! allowed to provide playback / download addresses itself (`provide_play`, `provide_download`),
//! in which case the playback plugin is not asked for that source any more.
use crate::storage::Storage;
use mio_plugin::{ListPlugin, PluginEnv};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

const FILE: &str = "list-plugins.json";
const FILE_BYTES: u64 = 16 * 1024 * 1024;
const PLUGIN_COUNT: usize = 16;
static GATE: Mutex<()> = Mutex::new(());
struct Registration {
    live: Weak<AtomicBool>,
    providers: Vec<Weak<dyn mio_music_sdk::MusicListProvider>>,
}
static STORAGE_LEASES: Mutex<BTreeMap<PathBuf, Vec<Registration>>> = Mutex::new(BTreeMap::new());

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSource {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
}

/// One field of the author's configuration form (see `mio_plugin::ConfigField`).
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredField {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub default: Value,
    pub required: bool,
    pub placeholder: String,
    pub description: String,
    pub options: Vec<(String, String)>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredListPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// What the interface shows instead of `name` (which identifies the plugin), in the
    /// interface language; empty falls back to `name`.
    #[serde(default)]
    pub display_name: String,
    pub sources: Vec<StoredSource>,
    pub enabled: bool,
    #[serde(default)]
    pub config_schema: Vec<StoredField>,
    /// The user's values of the form, by field key.
    #[serde(default)]
    pub config: Map<String, Value>,
    /// What the plugin keeps between calls (login state, caches): `cerumusic.storage`.
    #[serde(default)]
    pub storage: Map<String, Value>,
    /// Let the plugin provide playback / download addresses for its sources.
    #[serde(default)]
    pub provide_play: bool,
    #[serde(default)]
    pub provide_download: bool,
    /// What the plugin declares it can provide.
    #[serde(default)]
    pub can_play: bool,
    #[serde(default)]
    pub can_download: bool,
    code: String,
}

impl StoredListPlugin {
    /// The plugin's title in the interface language.
    pub fn title(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }

    /// Ships with the application: always installed, and switched off rather than removed.
    pub fn bundled(&self) -> bool {
        crate::plugin_catalog::is_bundled(&self.name)
    }

    /// Every required field has a value, so the plugin's sources are offered.
    pub fn configured(&self) -> bool {
        configured(self)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    plugins: Vec<StoredListPlugin>,
}

impl Registry {
    fn read(store: &Storage) -> Result<Self, String> {
        use std::io::Read;
        let file = match std::fs::File::open(store.root().join(FILE)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err("无法读取列表插件".into()),
        };
        let mut bytes = Vec::new();
        file.take(FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取列表插件")?;
        if bytes.len() as u64 > FILE_BYTES {
            return Err("列表插件文件超限".into());
        }
        let registry: Self = serde_json::from_slice(&bytes).map_err(|_| "列表插件文件损坏")?;
        if registry.plugins.len() > PLUGIN_COUNT
            || registry
                .plugins
                .iter()
                .any(|p| uuid::Uuid::parse_str(&p.id).is_err() || p.code.len() > 1024 * 1024)
        {
            return Err("列表插件文件无效或超限".into());
        }
        Ok(registry)
    }

    fn save(&self, store: &Storage) -> Result<(), String> {
        if serde_json::to_vec_pretty(self)
            .map_err(|_| "列表插件数据无效")?
            .len()
            .checked_add(1)
            .is_none_or(|n| n > FILE_BYTES as usize)
        {
            return Err("列表插件文件超限，原状态已保留".into());
        }
        store
            .save(FILE, self)
            .map_err(|_| "无法保存列表插件，原状态已保留".into())
    }
}

fn describe(plugin: &ListPlugin, id: String, code: String, enabled: bool) -> StoredListPlugin {
    let declares = |name: &str| {
        plugin
            .sources
            .iter()
            .any(|s| s.capabilities.iter().any(|c| c == name))
    };
    StoredListPlugin {
        id,
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        author: plugin.author.clone(),
        description: plugin.description.clone(),
        display_name: plugin.display_name.clone(),
        sources: plugin
            .sources
            .iter()
            .map(|s| StoredSource {
                id: s.id.clone(),
                name: s.name.clone(),
                capabilities: s.capabilities.clone(),
            })
            .collect(),
        enabled,
        config_schema: plugin
            .config
            .iter()
            .map(|f| StoredField {
                key: f.key.clone(),
                label: f.label.clone(),
                kind: f.kind.clone(),
                default: f.default.clone(),
                required: f.required,
                placeholder: f.placeholder.clone(),
                description: f.description.clone(),
                options: f.options.clone(),
            })
            .collect(),
        config: Map::new(),
        storage: Map::new(),
        provide_play: plugin.default_play && declares("play"),
        provide_download: plugin.default_download && declares("download"),
        can_play: declares("play"),
        can_download: declares("download"),
        code,
    }
}

/// The configuration the plugin sees: the user's values over the fields' defaults.
fn effective_config(stored: &StoredListPlugin) -> Value {
    let mut config = Map::new();
    for field in &stored.config_schema {
        if !field.default.is_null() {
            config.insert(field.key.clone(), field.default.clone());
        }
    }
    for (key, value) in &stored.config {
        config.insert(key.clone(), value.clone());
    }
    Value::Object(config)
}

/// Whether every required field has a value. A plugin registers its sources only then, so a
/// bundled plugin nobody has set up yet stays out of the source lists instead of failing there.
fn configured(stored: &StoredListPlugin) -> bool {
    stored
        .config_schema
        .iter()
        .filter(|f| f.required)
        .all(|field| {
            let value = stored.config.get(&field.key).unwrap_or(&field.default);
            match value {
                Value::Null => false,
                Value::String(text) => !text.trim().is_empty(),
                _ => true,
            }
        })
}

/// Enabled and configured: the plugins whose providers are registered.
fn active(stored: &StoredListPlugin) -> bool {
    stored.enabled && configured(stored)
}

/// The new script's description of an installed plugin, keeping what the user configured and
/// what the plugin stored.
fn replace(existing: &mut StoredListPlugin, plugin: &ListPlugin, code: String, enabled: bool) {
    let mut next = describe(plugin, existing.id.clone(), code, enabled);
    next.config = std::mem::take(&mut existing.config);
    next.storage = std::mem::take(&mut existing.storage);
    next.provide_play = existing.provide_play && next.can_play;
    next.provide_download = existing.provide_download && next.can_download;
    *existing = next;
}

/// Tells list plugins the interface language (`cerumusic.locale`). It applies to the scripts run
/// from now on; [`apply_language`] also refreshes what is stored.
pub fn set_language(english: bool) {
    mio_plugin::set_english(english);
}

/// Re-reads each installed plugin's display text (title, description, source names and form
/// labels) from its script in the current language, keeping its configuration and state.
/// Returns whether anything changed.
fn relocalize(registry: &mut Registry) -> bool {
    let mut changed = false;
    for stored in &mut registry.plugins {
        let Ok(plugin) = ListPlugin::load(stored.code.clone()) else {
            continue;
        };
        let next = describe(
            &plugin,
            stored.id.clone(),
            stored.code.clone(),
            stored.enabled,
        );
        let text = |s: &StoredListPlugin| {
            serde_json::to_value((
                &s.display_name,
                &s.description,
                &s.sources,
                &s.config_schema,
            ))
        };
        if text(stored).ok() != text(&next).ok() {
            stored.display_name = next.display_name;
            stored.description = next.description;
            stored.sources = next.sources;
            stored.config_schema = next.config_schema;
            changed = true;
        }
    }
    changed
}

/// The interface language changed: tells the plugins, refreshes the stored display text and
/// re-registers the active plugins so their source names follow.
pub fn apply_language(store: &Storage, english: bool) -> Result<(), String> {
    set_language(english);
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    if relocalize(&mut registry) {
        registry.save(store)?;
    }
    register_all(store, &registry);
    Ok(())
}

/// Adds the bundled plugins that are missing and updates older copies, keeping each one's
/// configuration and enabled state. Returns whether the registry changed.
fn seed_bundled(registry: &mut Registry) -> bool {
    let mut changed = false;
    for bundled in crate::plugin_catalog::plugins() {
        let plugin = match ListPlugin::load(bundled.script.to_owned()) {
            Ok(plugin) => plugin,
            Err(error) => {
                eprintln!("bundled list plugin {} not loaded: {error}", bundled.name);
                continue;
            }
        };
        let version = crate::plugin_catalog::version_key(&plugin.version);
        let full = registry.plugins.len() >= PLUGIN_COUNT;
        match registry.plugins.iter_mut().find(|p| p.name == plugin.name) {
            Some(existing) if crate::plugin_catalog::version_key(&existing.version) < version => {
                let enabled = existing.enabled;
                replace(existing, &plugin, bundled.script.to_owned(), enabled);
                changed = true;
            }
            Some(_) => {}
            None if !full => {
                let id = uuid::Uuid::new_v4().to_string();
                registry
                    .plugins
                    .push(describe(&plugin, id, bundled.script.to_owned(), true));
                changed = true;
            }
            None => {}
        }
    }
    changed
}

fn refuse_bundled(stored: &StoredListPlugin) -> Result<(), String> {
    if crate::plugin_catalog::is_bundled(&stored.name) {
        Err("自带插件不能卸载，可以停用".into())
    } else {
        Ok(())
    }
}

/// Writes a plugin's store back to the registry file (called from plugin calls).
fn persist_storage(store: &Storage, id: &str, storage: &Map<String, Value>, live: &AtomicBool) {
    let Ok(_gate) = GATE.lock() else { return };
    if !live.load(Ordering::Acquire) {
        return;
    }
    let Ok(mut registry) = Registry::read(store) else {
        return;
    };
    if let Some(plugin) = registry.plugins.iter_mut().find(|p| p.id == id) {
        plugin.storage = storage.clone();
        let _ = registry.save(store);
    }
}

fn invalidate_storage_locked(store: &Storage) -> Vec<Weak<dyn mio_music_sdk::MusicListProvider>> {
    let mut leases = STORAGE_LEASES.lock().unwrap_or_else(|e| e.into_inner());
    let mut providers = Vec::new();
    for registration in leases.remove(store.root()).unwrap_or_default() {
        if let Some(live) = registration.live.upgrade() {
            live.store(false, Ordering::Release);
        }
        providers.extend(registration.providers);
    }
    providers
}

/// Prevent in-flight providers from saving login/cache state after owner exit.
pub fn invalidate_storage(store: &Storage) {
    let _gate = GATE.lock().unwrap_or_else(|e| e.into_inner());
    mio_music_sdk::unregister_matching(&invalidate_storage_locked(store));
}

fn env_for(store: &Storage, stored: &StoredListPlugin, live: Arc<AtomicBool>) -> PluginEnv {
    let (store, id) = (store.clone(), stored.id.clone());
    PluginEnv {
        config: effective_config(stored),
        storage: Arc::new(Mutex::new(stored.storage.clone())),
        persist: Arc::new(move |map| persist_storage(&store, &id, map, &live)),
    }
}

/// Replaces every registered plugin provider with those of the enabled stored plugins. Plugins
/// that no longer load are skipped (and stay stored, so the list shows them).
fn register_all(store: &Storage, registry: &Registry) {
    mio_music_sdk::unregister_matching(&invalidate_storage_locked(store));
    for stored in registry.plugins.iter().filter(|p| active(p)) {
        match ListPlugin::load(stored.code.clone()) {
            Ok(plugin) => {
                let live = Arc::new(AtomicBool::new(true));
                let env = env_for(store, stored, live.clone());
                let plugin = Arc::new(plugin.with_env(env));
                let providers =
                    plugin.providers(&stored.id, stored.provide_play, stored.provide_download);
                STORAGE_LEASES
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .entry(store.root().to_path_buf())
                    .or_default()
                    .push(Registration {
                        live: Arc::downgrade(&live),
                        providers: providers.iter().map(Arc::downgrade).collect(),
                    });
                for provider in providers {
                    let _ = mio_music_sdk::register(provider);
                }
            }
            Err(error) => eprintln!("list plugin {} not loaded: {error}", stored.name),
        }
    }
}

/// At startup: installs or updates the bundled plugins, then registers the active plugins.
pub fn restore(store: &Storage) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    // The language may have changed since the plugins were stored: refresh their display text.
    let changed = seed_bundled(&mut registry) | relocalize(&mut registry);
    if changed && let Err(error) = registry.save(store) {
        // Register what is already installed; the next start retries the bundled plugins.
        eprintln!("bundled list plugins not saved: {error}");
        registry = Registry::read(store)?;
    }
    register_all(store, &registry);
    Ok(())
}

pub fn installed(store: &Storage) -> Result<Vec<StoredListPlugin>, String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    Ok(Registry::read(store)?.plugins)
}

/// Snapshot the selected plugin, without holding the registry lock during network calls.
/// Preview configuration and login cache stay private to this operation.
pub fn service_plugin(
    store: &Storage,
    id: &str,
    config: Option<Map<String, Value>>,
) -> Result<Arc<ListPlugin>, String> {
    let mut stored = {
        let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
        Registry::read(store)?
            .plugins
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("列表插件不存在")?
    };
    if let Some(config) = config {
        stored.config = config;
        stored.storage.clear();
    }
    let env = PluginEnv {
        config: effective_config(&stored),
        storage: Arc::new(Mutex::new(stored.storage)),
        persist: Arc::new(|_| {}),
    };
    Ok(Arc::new(ListPlugin::load(stored.code)?.with_env(env)))
}

/// Installs (or updates, when a plugin of the same name is already installed) a list plugin and
/// enables it. The script is run once to read its metadata.
pub fn install(store: &Storage, code: String) -> Result<StoredListPlugin, String> {
    let plugin = ListPlugin::load(code.clone())?;
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    let stored = match registry.plugins.iter_mut().find(|p| p.name == plugin.name) {
        Some(existing) => {
            replace(existing, &plugin, code, true);
            existing.clone()
        }
        None => {
            if registry.plugins.len() >= PLUGIN_COUNT {
                return Err(format!("最多安装 {PLUGIN_COUNT} 个列表插件"));
            }
            let stored = describe(&plugin, uuid::Uuid::new_v4().to_string(), code, true);
            registry.plugins.push(stored.clone());
            stored
        }
    };
    registry.save(store)?;
    register_all(store, &registry);
    Ok(stored)
}

pub fn set_enabled(store: &Storage, id: &str, enabled: bool) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    let plugin = registry
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("列表插件不存在")?;
    plugin.enabled = enabled;
    registry.save(store)?;
    register_all(store, &registry);
    Ok(())
}

pub fn uninstall(store: &Storage, id: &str) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    refuse_bundled(
        registry
            .plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or("列表插件不存在")?,
    )?;
    registry.plugins.retain(|p| p.id != id);
    registry.save(store)?;
    mio_music_sdk::unregister_plugin(id);
    Ok(())
}

/// `true` when the script declares itself a music-list plugin.
pub fn is_list_plugin(code: &str) -> bool {
    mio_plugin::looks_like_list_plugin(code)
}

/// Saves the configuration form's values (only declared fields are kept) and re-registers.
pub fn set_config(store: &Storage, id: &str, values: Map<String, Value>) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    let plugin = registry
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("列表插件不存在")?;
    let keep: Map<String, Value> = values
        .into_iter()
        .filter(|(key, _)| plugin.config_schema.iter().any(|f| f.key == *key))
        .collect();
    plugin.config = keep;
    // A changed configuration invalidates what the plugin cached for the old one.
    plugin.storage.clear();
    registry.save(store)?;
    register_all(store, &registry);
    Ok(())
}

/// Lets the plugin provide playback / download addresses itself, or not.
pub fn set_provide(
    store: &Storage,
    id: &str,
    play: Option<bool>,
    download: Option<bool>,
) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    let plugin = registry
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("列表插件不存在")?;
    if let Some(play) = play {
        plugin.provide_play = play && plugin.can_play;
    }
    if let Some(download) = download {
        plugin.provide_download = download && plugin.can_download;
    }
    registry.save(store)?;
    register_all(store, &registry);
    Ok(())
}

/// Forgets what the plugin stored (login state, caches).
pub fn clear_storage(store: &Storage, id: &str) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    let mut registry = Registry::read(store)?;
    let plugin = registry
        .plugins
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("列表插件不存在")?;
    plugin.storage.clear();
    registry.save(store)?;
    register_all(store, &registry);
    Ok(())
}

fn require_list(code: String) -> Result<String, String> {
    if is_list_plugin(&code) {
        Ok(code)
    } else {
        Err("这不是音乐列表插件或旧服务插件".into())
    }
}

/// Installs a list plugin from a file.
pub fn import_file(store: &Storage, path: &std::path::Path) -> Result<StoredListPlugin, String> {
    install(
        store,
        require_list(crate::online::Online::read_plugin_file(path)?)?,
    )
}

/// Installs a list plugin from a URL.
pub fn import_url(store: &Storage, raw: &str) -> Result<StoredListPlugin, String> {
    install(
        store,
        require_list(crate::online::Online::download_plugin(raw)?)?,
    )
}

/// Sources whose playback a list plugin provides (the user allowed it): what the playback
/// plugin is not asked for any more. Read from the SDK registry, so it follows toggles at once.
pub fn play_sources() -> Vec<mio_plugin::PluginSource> {
    mio_music_sdk::providers()
        .into_iter()
        .filter(|(_, _, origin)| matches!(origin, mio_music_sdk::Origin::Plugin(_)))
        .filter_map(|(id, name, _)| {
            let provider = mio_music_sdk::provider(&id)?;
            let caps = provider.capabilities();
            if !caps.play {
                return None;
            }
            let mut qualities = provider.qualities();
            if qualities.is_empty() {
                qualities.push("128k".into());
            }
            Some(mio_plugin::PluginSource {
                id,
                name,
                qualities,
            })
        })
        .collect()
}

/// A mobile edit is prepared without durable writes or provider registration.
pub enum Change {
    Install(String),
    Enabled(String, bool),
    Config(String, Map<String, Value>),
    Provide(String, bool, bool),
    Remove(String),
}

pub struct PreparedChange {
    before: Vec<u8>,
    registry: Registry,
    providers: Vec<Arc<dyn mio_music_sdk::MusicListProvider>>,
    leases: Vec<(
        Arc<AtomicBool>,
        Vec<Weak<dyn mio_music_sdk::MusicListProvider>>,
    )>,
}

fn registry_bytes(registry: &Registry) -> Result<Vec<u8>, String> {
    serde_json::to_vec(registry).map_err(|_| "列表插件数据无效".into())
}

pub fn prepare_change(store: &Storage, change: Change) -> Result<PreparedChange, String> {
    let mut registry = {
        let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
        Registry::read(store)?
    };
    let before = registry_bytes(&registry)?;
    match change {
        Change::Install(code) => {
            if code.is_empty() || code.len() > 1024 * 1024 || !is_list_plugin(&code) {
                return Err("列表插件文件无效或超限".into());
            }
            let plugin = ListPlugin::load(code.clone())?;
            if let Some(existing) = registry.plugins.iter_mut().find(|p| p.name == plugin.name) {
                replace(existing, &plugin, code, true);
            } else {
                if registry.plugins.len() >= PLUGIN_COUNT {
                    return Err("列表插件数量超限".into());
                }
                registry.plugins.push(describe(
                    &plugin,
                    uuid::Uuid::new_v4().to_string(),
                    code,
                    true,
                ));
            }
        }
        Change::Remove(id) => {
            refuse_bundled(
                registry
                    .plugins
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or("列表插件不存在")?,
            )?;
            registry.plugins.retain(|p| p.id != id);
        }
        change => {
            let id = match &change {
                Change::Enabled(id, _) | Change::Config(id, _) | Change::Provide(id, _, _) => id,
                _ => unreachable!(),
            };
            let plugin = registry
                .plugins
                .iter_mut()
                .find(|p| &p.id == id)
                .ok_or("列表插件不存在")?;
            match change {
                Change::Enabled(_, enabled) => plugin.enabled = enabled,
                Change::Provide(_, play, download) => {
                    if play && !plugin.can_play || download && !plugin.can_download {
                        return Err("插件不支持该能力".into());
                    }
                    plugin.provide_play = play;
                    plugin.provide_download = download;
                }
                Change::Config(_, values) => {
                    if values.len() > 128
                        || serde_json::to_vec(&values).map_err(|_| "配置无效")?.len() > 64 * 1024
                    {
                        return Err("插件配置超限".into());
                    }
                    if values
                        .keys()
                        .any(|k| !plugin.config_schema.iter().any(|f| &f.key == k))
                    {
                        return Err("插件配置包含未知字段".into());
                    }
                    for field in &plugin.config_schema {
                        let value = values.get(&field.key).unwrap_or(&field.default);
                        if value.is_null() && !field.required {
                            continue;
                        }
                        let valid = match field.kind.as_str() {
                            "switch" => value.is_boolean(),
                            "number" => value.as_f64().is_some_and(f64::is_finite),
                            "select" => value.as_str().is_some_and(|v| {
                                (!field.required || !v.is_empty())
                                    && (v.is_empty()
                                        || field.options.iter().any(|(_, key)| key == v))
                            }),
                            "text" | "password" | "textarea" => value
                                .as_str()
                                .is_some_and(|v| !field.required || !v.trim().is_empty()),
                            _ => false,
                        };
                        if !valid {
                            return Err("插件配置字段无效或缺少必填项".into());
                        }
                    }
                    plugin.config = values;
                    plugin.storage.clear();
                }
                _ => unreachable!(),
            }
        }
    }
    if registry_bytes(&registry)?.len() > FILE_BYTES as usize {
        return Err("列表插件文件超限".into());
    }
    let mut providers = Vec::new();
    let mut leases = Vec::new();
    for stored in registry.plugins.iter().filter(|p| active(p)) {
        let live = Arc::new(AtomicBool::new(false));
        let plugin = Arc::new(ListPlugin::load(stored.code.clone())?.with_env(env_for(
            store,
            stored,
            live.clone(),
        )));
        let prepared = plugin.providers(&stored.id, stored.provide_play, stored.provide_download);
        if prepared
            .iter()
            .any(|p| !mio_music_sdk::is_custom_source_id(p.id()))
        {
            return Err("插件来源 ID 无效".into());
        }
        leases.push((live, prepared.iter().map(Arc::downgrade).collect()));
        providers.extend(prepared);
    }
    Ok(PreparedChange {
        before,
        registry,
        providers,
        leases,
    })
}

/// Owner commit: compare the complete registry, save, then publish prepared providers.
pub fn commit_change(store: &Storage, prepared: PreparedChange) -> Result<(), String> {
    let _gate = GATE.lock().map_err(|_| "列表插件不可用")?;
    if registry_bytes(&Registry::read(store)?)? != prepared.before {
        return Err("storage_changed: 列表插件已变化，请重试".into());
    }
    let previous: Vec<_> = STORAGE_LEASES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(store.root())
        .into_iter()
        .flatten()
        .flat_map(|p| p.providers.iter().cloned())
        .collect();
    mio_music_sdk::replace_plugins(&previous, prepared.providers, || {
        prepared.registry.save(store)?;
        let _ = invalidate_storage_locked(store);
        Ok(())
    })?;
    let mut leases = STORAGE_LEASES.lock().unwrap_or_else(|e| e.into_inner());
    let target = leases.entry(store.root().to_path_buf()).or_default();
    for (live, providers) in prepared.leases {
        live.store(true, Ordering::Release);
        target.push(Registration {
            live: Arc::downgrade(&live),
            providers,
        });
    }
    Ok(())
}
