//! Source-aware online metadata and playback plugin; Subsonic IDs never enter this path.
mod backup_restore;
use crate::song::Song;
use crate::storage::Storage;
pub use backup_restore::{BackupPluginSnapshot, PreparedBackupPlugins};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use mio_music_sdk::{AggregateSearch, MusicItem};
use mio_plugin::{Plugin, PluginSource};
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Online {
    pub id: String,
    pub plugin: Arc<Plugin>,
    store: Storage,
    songs: Arc<Mutex<BTreeMap<String, MusicItem>>>,
    search_gate: Mutex<()>,
    /// Paging state of the current "all sources" query; guarded by `search_gate`.
    aggregate: Mutex<Option<AggregateSearch>>,
}
#[derive(Clone, Copy)]
pub enum OnlinePageKind {
    Playlist,
    Chart,
    Album,
    Singer,
}
pub struct SingerReference {
    pub source: String,
    pub id: String,
    pub name: String,
}
fn valid_singer(item: &MusicItem) -> bool {
    item.source.len() <= 4096
        && item.singer.len() <= 4096
        && item.singer_id.as_deref().is_some_and(|id| {
            !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
        })
}
fn metadata_id(item: &MusicItem) -> Option<String> {
    let raw = item
        .songmid
        .as_str()
        .filter(|id| id.len() <= 512)
        .map(str::to_owned)
        .or_else(|| item.songmid.as_u64().map(|id| id.to_string()))?;
    Some(song_id(&item.source, &raw))
}
fn singer_from(items: &[MusicItem], id: &str) -> Option<SingerReference> {
    let item = items
        .iter()
        .find(|item| metadata_id(item).as_deref() == Some(id))?;
    if !valid_singer(item) {
        return None;
    }
    Some(SingerReference {
        source: item.source.clone(),
        id: item.singer_id.clone()?,
        name: item.singer.clone(),
    })
}
fn singer_ids(items: &[MusicItem], sources: &[String]) -> Vec<String> {
    items
        .iter()
        .filter(|item| valid_singer(item) && sources.contains(&item.source))
        .filter_map(metadata_id)
        .collect()
}
pub struct SearchPage {
    pub songs: Vec<Song>,
    pub total: usize,
    /// Whether another page can be requested from the same query.
    pub has_more: bool,
    /// Aggregate pages are re-deduplicated as a whole, so they replace the list instead of
    /// extending it.
    pub replace: bool,
}
/// Provider work prepared off-thread; publishing is a separate owner decision.
#[derive(Clone)]
pub struct PreparedSourceSearch {
    items: Vec<MusicItem>,
    source: String,
    total: usize,
    has_more: bool,
    declared_pages: Option<usize>,
    description: String,
}
impl PreparedSourceSearch {
    /// Admit one original recognition row without searching or refetching a collection.
    pub fn from_recognition(row: &crate::recognition::Match) -> Result<Self, String> {
        let item = &row.item;
        row.validate()?;
        bounded_metadata_size(item)?;
        Ok(Self {
            items: vec![item.clone()],
            source: "wy".into(),
            total: 1,
            has_more: false,
            declared_pages: None,
            description: String::new(),
        })
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Project the already validated collection into the requested playback order.
    pub fn playlist_play(mut self, query: &str, shuffle: bool) -> Result<Self, String> {
        if query.len() > 1024 || query.contains('\0') {
            return Err("invalid_input: playlist filter".into());
        }
        let query = query.trim().to_lowercase();
        if !query.is_empty() {
            self.items.retain(|item| {
                item.name.to_lowercase().contains(&query)
                    || item.singer.to_lowercase().contains(&query)
                    || item.album_name.to_lowercase().contains(&query)
            });
        }
        if self.items.is_empty() {
            return Err("not_found: matching playlist songs".into());
        }
        if shuffle {
            for i in (1..self.items.len()).rev() {
                let j = (uuid::Uuid::new_v4().as_u128() % (i + 1) as u128) as usize;
                self.items.swap(i, j);
            }
        }
        self.total = self.items.len();
        Ok(self)
    }
    pub fn reported_total(&self) -> usize {
        self.total
    }
    pub fn declared_pages(&self) -> Option<usize> {
        self.declared_pages
    }
    pub fn has_more(&self) -> bool {
        self.has_more
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn singer_of(&self, id: &str) -> Option<SingerReference> {
        singer_from(&self.items, id)
    }
    pub fn singer_ids(&self, sources: &[String]) -> Vec<String> {
        singer_ids(&self.items, sources)
    }
    /// Preserve the exact session-owned metadata when a shared cache evicts it.
    pub fn selected(&self, id: &str) -> Option<Self> {
        let item = self
            .items
            .iter()
            .find(|item| {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|v| v.to_string()));
                raw.is_some_and(|raw| song_id(&self.source, &raw) == id)
            })?
            .clone();
        Some(Self {
            items: vec![item],
            source: self.source.clone(),
            total: 1,
            has_more: false,
            declared_pages: None,
            description: String::new(),
        })
    }
    /// Combine only already admitted bounded pages; use the first row for duplicate IDs.
    pub fn combined(pages: &[Self]) -> Result<Self, String> {
        let source = pages
            .first()
            .ok_or("not_found: no loaded songs")?
            .source
            .clone();
        let mut bytes = 0usize;
        let mut count = 0usize;
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for page in pages {
            bytes += page.metadata_bytes()?;
            count += page.items.len();
            if page.source != source || bytes > 8 * 1024 * 1024 || count > 2000 {
                return Err("capacity_exceeded: loaded songs".into());
            }
            for item in &page.items {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|v| v.to_string()))
                    .ok_or("invalid_data: song ID")?;
                if seen.insert(raw) {
                    items.push(item.clone());
                }
            }
        }
        Ok(Self {
            total: items.len(),
            items,
            source,
            has_more: false,
            declared_pages: None,
            description: String::new(),
        })
    }
    pub fn into_items(self) -> Vec<MusicItem> {
        self.items
    }
    pub fn item_count(&self) -> usize {
        self.items.len()
    }
    pub fn metadata_bytes(&self) -> Result<usize, String> {
        bounded_metadata_size(&self.items)
    }
}
fn bounded_metadata_size(value: &impl serde::Serialize) -> Result<usize, String> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= 2 * 1024 * 1024)
                .ok_or_else(|| std::io::Error::other("Online page exceeds mobile limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, value)
        .map_err(|_| "capacity_exceeded: online metadata page")?;
    Ok(count.0)
}
/// Session-owned aggregate metadata, independent of the evictable playback cache.
#[derive(Default)]
pub struct SearchRanking {
    items: Vec<MusicItem>,
    ids: HashSet<String>,
    bytes: usize,
}
impl PreparedSourceSearch {
    pub fn ranking(&self) -> Result<SearchRanking, String> {
        let mut ranking = SearchRanking::default();
        for item in &self.items {
            ranking.insert(item.clone())?;
        }
        Ok(ranking)
    }
}
impl SearchRanking {
    pub fn singer_of(&self, id: &str) -> Option<SingerReference> {
        singer_from(&self.items, id)
    }
    pub fn singer_ids(&self, sources: &[String]) -> Vec<String> {
        singer_ids(&self.items, sources)
    }
    pub fn selected(&self, id: &str) -> Option<PreparedSourceSearch> {
        let item = self
            .items
            .iter()
            .find(|item| metadata_id(item).as_deref() == Some(id))?
            .clone();
        Some(PreparedSourceSearch {
            source: item.source.clone(),
            items: vec![item],
            total: 1,
            has_more: false,
            declared_pages: None,
            description: String::new(),
        })
    }
    fn insert(&mut self, item: MusicItem) -> Result<(), String> {
        let raw = item
            .songmid
            .as_str()
            .map(str::to_owned)
            .or_else(|| item.songmid.as_u64().map(|id| id.to_string()))
            .ok_or("在线歌曲缺少 ID")?;
        let id = song_id(&item.source, &raw);
        if self.ids.contains(&id) {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&item)
            .map_err(|_| "无效在线元数据")?
            .len();
        let next = self.bytes.checked_add(bytes).ok_or("搜索元数据超过容量")?;
        if self.items.len() >= 2000 || next > 8 * 1024 * 1024 {
            return Err("capacity_exceeded: search ranking exceeds mobile limit".into());
        }
        self.bytes = next;
        self.ids.insert(id);
        self.items.push(item);
        Ok(())
    }
    pub fn append(&mut self, page: SearchRanking) -> Result<(), String> {
        for item in page.items {
            self.insert(item)?;
        }
        Ok(())
    }
    pub fn rank(
        &self,
        query: &str,
        sources: &[String],
        songs: &[Song],
    ) -> Result<Vec<Song>, String> {
        let groups: Vec<_> = sources
            .iter()
            .map(|source| {
                (
                    source.clone(),
                    self.items
                        .iter()
                        .filter(|item| item.source == *source)
                        .cloned()
                        .collect(),
                )
            })
            .collect();
        let ranked = mio_music_sdk::dedupe_and_sort_songs(&groups, query);
        let by_id: std::collections::HashMap<_, _> = songs.iter().map(|s| (&s.id, s)).collect();
        ranked
            .into_iter()
            .map(|item| {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|id| id.to_string()))
                    .ok_or("在线歌曲缺少 ID")?;
                by_id
                    .get(&song_id(&item.source, &raw))
                    .map(|s| (*s).clone())
                    .ok_or_else(|| "搜索元数据已失效".into())
            })
            .collect()
    }
}
/// One playlist search result card (`PlaylistCardItem` of the original).
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistCard {
    pub id: String,
    pub source: String,
    pub title: String,
    pub description: String,
    pub cover: Option<String>,
    pub author: String,
    /// Song count when the source reports one.
    pub song_count: Option<u64>,
    /// An album card of a singer page rather than a playlist.
    pub album: bool,
}
/// One page of a playlist's contents.
pub struct PlaylistDetailPage {
    pub songs: Vec<Song>,
    pub total: usize,
    pub has_more: bool,
    pub title: String,
    pub cover: Option<String>,
    pub description: String,
    pub author: String,
}
/// Profile shown in a singer page header.
pub struct SingerProfile {
    pub name: String,
    pub desc: String,
    pub avatar: Option<String>,
    pub music_count: i64,
    pub album_count: i64,
}
/// One album card of a singer page.
#[derive(Clone, Debug, PartialEq)]
pub struct SingerAlbumCard {
    pub id: String,
    pub name: String,
    pub author: String,
    pub cover: Option<String>,
    pub count: i64,
    pub description: String,
}
pub struct SingerAlbumsPage {
    pub cards: Vec<SingerAlbumCard>,
    pub total: usize,
    pub has_more: bool,
}
pub struct PlaylistPage {
    pub cards: Vec<PlaylistCard>,
    /// Source-reported total for one source; the number of cards for "all sources".
    pub total: usize,
}

/// What `white-space: normal` does to source text: runs of space, tab and line breaks become one
/// space and the ends are trimmed. GPUI's `line_clamp` counts soft wraps only, so an intro with
/// hard line breaks would otherwise ignore the original's two-line clamp. U+3000 and other
/// non-CSS whitespace are kept.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c') {
            pending = !out.is_empty();
        } else {
            if pending {
                out.push(' ');
                pending = false;
            }
            out.push(ch);
        }
    }
    out
}

/// `unescapeHtml` of the original: only numeric `&#NNN;` references are decoded.
fn unescape_numeric_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("&#") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && tail.as_bytes().get(digits) == Some(&b';') {
            if let Some(c) = tail[..digits].parse::<u32>().ok().and_then(char::from_u32) {
                out.push(c);
                rest = &tail[digits + 1..];
                continue;
            }
        }
        out.push_str("&#");
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn playlist_card(item: mio_music_sdk::PlaylistItem) -> PlaylistCard {
    let text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    PlaylistCard {
        id: text(&item.id),
        source: item.source,
        title: collapse_whitespace(&unescape_numeric_entities(&item.name)),
        description: collapse_whitespace(&item.desc),
        cover: (!item.img.is_empty()).then_some(item.img),
        author: collapse_whitespace(&item.author),
        song_count: text(&item.total)
            .trim()
            .replace(',', "")
            .parse::<u64>()
            .ok(),
        album: false,
    }
}
const METADATA_MAX_SONGS: usize = 2048;
const METADATA_MAX_BYTES: usize = 2 * 1024 * 1024;

fn over_limit(songs: &BTreeMap<String, MusicItem>) -> Result<bool, String> {
    Ok(songs.len() > METADATA_MAX_SONGS
        || serde_json::to_vec_pretty(songs)
            .map_err(|_| "无效在线元数据")?
            .len()
            > METADATA_MAX_BYTES)
}

/// Song ID in the app: source-namespaced so plugin resolution never guesses the origin.
pub fn song_id(source: &str, raw: &str) -> String {
    format!("mio-online:{source}:{}", URL_SAFE_NO_PAD.encode(raw))
}

/// Inverse of [`song_id`]: `(source, raw songmid)`.
pub fn parse_song_id(id: &str) -> Option<(String, String)> {
    let rest = id.strip_prefix("mio-online:")?;
    let (source, encoded) = rest.split_once(':')?;
    let raw = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded).ok()?).ok()?;
    (!source.is_empty() && !raw.is_empty()).then(|| (source.to_owned(), raw))
}
const PLUGIN_REGISTRY: &str = "online-plugins.json";
const REGISTRY_BYTES: usize = 24 * 1024 * 1024;
const PLUGIN_COUNT: usize = 16;
static PLUGIN_GATE: Mutex<()> = Mutex::new(());

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub sources: Vec<PluginSource>,
    pub logs: Vec<(u64, String)>,
    code: String,
}
impl InstalledPlugin {
    fn validated(client: &Online, code: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: client.plugin.name.clone(),
            version: client.plugin.version.clone(),
            author: client.plugin.author.clone(),
            description: client.plugin.description.clone(),
            sources: client.plugin.sources.clone(),
            logs: client
                .plugin
                .logs()
                .into_iter()
                .map(|line| (line.at_ms, line.text))
                .collect(),
            code,
        }
    }
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct PluginRegistry {
    plugins: Vec<InstalledPlugin>,
    active: Option<String>,
}
impl PluginRegistry {
    fn read(store: &Storage) -> Result<Self, String> {
        Self::read_mode(store, true)
    }
    fn read_mode(store: &Storage, migrate: bool) -> Result<Self, String> {
        let path = store.root().join(PLUGIN_REGISTRY);
        match std::fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(REGISTRY_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "无法读取插件列表")?;
                if bytes.len() > REGISTRY_BYTES {
                    return Err("插件列表超限".into());
                }
                let registry: Self = serde_json::from_slice(&bytes).map_err(|_| "插件列表损坏")?;
                registry.validate()?;
                Ok(registry)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !migrate {
                    return Ok(Self::default());
                }
                let Some(code) = store
                    .load_online::<String>("online-plugin.json")
                    .map_err(|_| "无法读取旧插件")?
                else {
                    return Ok(Self::default());
                };
                let client = Online::from_code(store.clone(), code.clone(), None)?;
                let plugin = InstalledPlugin::validated(&client, code);
                let registry = Self {
                    active: Some(plugin.id.clone()),
                    plugins: vec![plugin],
                };
                registry.save(store)?;
                Ok(registry)
            }
            Err(_) => Err("无法读取插件列表".into()),
        }
    }
    fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        if self.plugins.len() > PLUGIN_COUNT
            || self.plugins.iter().any(|p| {
                uuid::Uuid::parse_str(&p.id).is_err()
                    || !ids.insert(&p.id)
                    || p.code.is_empty()
                    || p.code.len() > 1024 * 1024
                    || p.logs.len() > 500
                    || p.logs.iter().any(|(_, line)| line.chars().count() > 2000)
            })
            || self.active.as_ref().is_some_and(|id| !ids.contains(id))
        {
            return Err("插件列表无效或超限".into());
        }
        Ok(())
    }
    fn save(&self, store: &Storage) -> Result<(), String> {
        self.validate()?;
        if serde_json::to_vec_pretty(self)
            .map_err(|_| "无法序列化插件列表")?
            .len()
            + 1
            > REGISTRY_BYTES
        {
            return Err("插件列表超限".into());
        }
        store
            .save(PLUGIN_REGISTRY, self)
            .map_err(|_| "无法保存插件列表，原状态已保留".into())
    }
    fn capture_logs(&mut self, existing: Option<&Arc<Online>>) {
        if let Some(client) = existing {
            if let Some(plugin) = self.plugins.iter_mut().find(|p| p.id == client.id) {
                plugin.logs = client
                    .plugin
                    .logs()
                    .into_iter()
                    .map(|line| (line.at_ms, line.text))
                    .collect();
            }
        }
    }
}

pub enum PlaybackChange {
    Install(String),
    Activate(String),
    Remove(String),
}

pub struct PreparedPlaybackChange {
    before: Vec<u8>,
    registry: PluginRegistry,
    online: Arc<Online>,
}

impl Online {
    /// A mobile reader does not perform legacy registry migrations.
    pub fn installed_current(store: &Storage) -> Result<Vec<InstalledPlugin>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut plugins = PluginRegistry::read_mode(store, false)?.plugins;
        for plugin in &mut plugins {
            plugin.code.clear();
        }
        Ok(plugins)
    }

    pub fn refreshed(store: Storage, existing: Option<Arc<Self>>) -> Result<Arc<Self>, String> {
        let Some(existing) = existing else {
            return Self::virtual_client(store, None);
        };
        Ok(Arc::new(Self {
            id: existing.id.clone(),
            plugin: existing.plugin.clone(),
            store,
            songs: existing.songs.clone(),
            search_gate: Mutex::new(()),
            aggregate: Mutex::new(None),
        }))
    }

    pub fn prepare_change(
        store: Storage,
        existing: Option<Arc<Self>>,
        change: PlaybackChange,
    ) -> Result<PreparedPlaybackChange, String> {
        let mut registry = {
            let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
            PluginRegistry::read_mode(&store, false)?
        };
        let before = serde_json::to_vec(&registry).map_err(|_| "插件数据无效")?;
        registry.capture_logs(existing.as_ref());
        let online = match change {
            PlaybackChange::Install(code) => {
                if code.is_empty()
                    || code.len() > 1024 * 1024
                    || crate::list_plugins::is_list_plugin(&code)
                {
                    return Err("播放插件文件无效或超限".into());
                }
                let mut online = Self::from_code(store, code.clone(), existing)?;
                let id = if let Some(p) = registry.plugins.iter().find(|p| p.code == code) {
                    p.id.clone()
                } else {
                    if registry.plugins.len() >= PLUGIN_COUNT {
                        return Err("插件数量超限".into());
                    }
                    let plugin = InstalledPlugin::validated(&online, code);
                    let id = plugin.id.clone();
                    registry.plugins.push(plugin);
                    id
                };
                registry.active = Some(id.clone());
                Arc::get_mut(&mut online).ok_or("插件状态不可用")?.id = id;
                online
            }
            change => {
                match change {
                    PlaybackChange::Activate(id) => {
                        if !registry.plugins.iter().any(|p| p.id == id) {
                            return Err("插件不存在".into());
                        }
                        registry.active = Some(id);
                    }
                    PlaybackChange::Remove(id) => {
                        if !registry.plugins.iter().any(|p| p.id == id) {
                            return Err("插件不存在".into());
                        }
                        registry.plugins.retain(|p| p.id != id);
                        if registry.active.as_ref() == Some(&id) {
                            registry.active = None;
                        }
                    }
                    _ => unreachable!(),
                }
                if let Some(id) = &registry.active {
                    let code = registry
                        .plugins
                        .iter()
                        .find(|p| &p.id == id)
                        .ok_or("插件不存在")?
                        .code
                        .clone();
                    let mut online = Self::from_code(store, code, existing)?;
                    Arc::get_mut(&mut online).ok_or("插件状态不可用")?.id = id.clone();
                    online
                } else {
                    Self::virtual_client(store, existing)?
                }
            }
        };
        registry.validate()?;
        Ok(PreparedPlaybackChange {
            before,
            registry,
            online,
        })
    }

    /// Owner validates its current source before this disk-only commit.
    pub fn commit_change(
        store: &Storage,
        prepared: PreparedPlaybackChange,
    ) -> Result<Arc<Self>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let current = PluginRegistry::read_mode(store, false)?;
        if serde_json::to_vec(&current).map_err(|_| "插件数据无效")? != prepared.before {
            return Err("storage_changed: 播放插件已变化，请重试".into());
        }
        prepared.registry.save(store)?;
        Ok(prepared.online)
    }
}
impl Online {
    /// Backup metadata is read from the registry without executing installed plugin code.
    pub fn backup_plugins(store: &Storage) -> Result<Vec<serde_json::Value>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        Ok(PluginRegistry::read(store)?
            .plugins
            .into_iter()
            .map(|plugin| {
                serde_json::json!({
                    "id": plugin.id, "name": plugin.name, "code": plugin.code, "config": null,
                })
            })
            .collect())
    }
    /// Merge valid imported plugins in one atomic commit, leaving selection and existing code intact.
    pub fn merge_plugins<C: AsRef<str>>(
        store: &Storage,
        codes: impl Iterator<Item = C>,
    ) -> Result<u32, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut registry = PluginRegistry::read(store)?;
        let mut added = 0;
        for code in codes {
            let code = code.as_ref();
            if code.is_empty()
                || code.len() > 1024 * 1024
                || registry.plugins.iter().any(|plugin| plugin.code == code)
            {
                continue;
            }
            let Ok(client) = Self::from_code(store.clone(), code.to_owned(), None) else {
                continue;
            };
            if registry.plugins.len() >= PLUGIN_COUNT {
                return Err("最多安装 16 个插件，原插件列表已保留".into());
            }
            registry
                .plugins
                .push(InstalledPlugin::validated(&client, code.to_owned()));
            added += 1;
        }
        if added > 0 {
            registry.save(store)?;
        }
        Ok(added)
    }
    pub fn installed(store: &Storage) -> Result<Vec<InstalledPlugin>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut plugins = PluginRegistry::read(store)?.plugins;
        // The UI only needs display metadata and saved logs, not a second copy of all code.
        for plugin in &mut plugins {
            plugin.code.clear();
        }
        Ok(plugins)
    }
    pub fn restore(store: &Storage) -> Result<Option<Arc<Self>>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let registry = PluginRegistry::read(store)?;
        let Some(id) = registry.active else {
            // No playback plugin: music-list plugins may provide playback themselves.
            if crate::list_plugins::play_sources().is_empty() {
                return Ok(None);
            }
            return Self::virtual_client(store.clone(), None).map(Some);
        };
        let plugin = registry
            .plugins
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("插件不存在")?;
        let mut client = Self::from_code(store.clone(), plugin.code, None)?;
        Arc::get_mut(&mut client).unwrap().id = id;
        Ok(Some(client))
    }
    pub fn activate(
        store: Storage,
        id: &str,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut registry = PluginRegistry::read(&store)?;
        let code = registry
            .plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or("插件不存在")?
            .code
            .clone();
        let mut client = Self::from_code(store.clone(), code, existing.clone())?;
        Arc::get_mut(&mut client).unwrap().id = id.to_owned();
        registry.capture_logs(existing.as_ref());
        registry.active = Some(id.to_owned());
        registry.save(&store)?;
        Ok(client)
    }
    pub fn uninstall_selected(
        store: &Storage,
        id: &str,
        existing: Option<&Arc<Self>>,
    ) -> Result<(), String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut registry = PluginRegistry::read(store)?;
        if !registry.plugins.iter().any(|p| p.id == id) {
            return Err("插件不存在".into());
        }
        registry.capture_logs(existing);
        registry.plugins.retain(|p| p.id != id);
        if registry.active.as_deref() == Some(id) {
            registry.active = None;
        }
        registry.save(store)
    }
    fn install(
        store: Storage,
        code: String,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        let _gate = PLUGIN_GATE.lock().map_err(|_| "插件列表不可用")?;
        let mut client = Self::from_code(store.clone(), code.clone(), existing.clone())?;
        let mut registry = PluginRegistry::read(&store)?;
        registry.capture_logs(existing.as_ref());
        let id = if let Some(plugin) = registry.plugins.iter().find(|p| p.code == code) {
            plugin.id.clone()
        } else {
            if registry.plugins.len() >= PLUGIN_COUNT {
                return Err("最多安装 16 个插件".into());
            }
            let plugin = InstalledPlugin::validated(&client, code);
            let id = plugin.id.clone();
            registry.plugins.push(plugin);
            id
        };
        Arc::get_mut(&mut client).unwrap().id = id;
        registry.save(&store)?;
        Ok(client)
    }
    fn from_code(
        store: Storage,
        code: String,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        let plugin = Arc::new(Plugin::load(code)?);
        let songs = Self::songs_of(&store, existing)?;
        Ok(Arc::new(Self {
            id: String::new(),
            plugin,
            store,
            songs,
            search_gate: Mutex::new(()),
            aggregate: Mutex::new(None),
        }))
    }
    /// The song metadata cache: shared with the client it replaces, else read from disk.
    fn songs_of(
        store: &Storage,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Mutex<BTreeMap<String, MusicItem>>>, String> {
        let songs = if let Some(existing) = existing {
            existing.songs.clone()
        } else {
            Arc::new(Mutex::new(
                store
                    .load_online::<BTreeMap<String, MusicItem>>("online-songs.json")
                    .map_err(|_| "无法读取在线歌曲元数据")?
                    .unwrap_or_default(),
            ))
        };
        let snapshot = songs.lock().map_err(|_| "在线元数据不可用")?;
        if over_limit(&snapshot)? {
            return Err("在线歌曲元数据超限".into());
        }
        drop(snapshot);
        Ok(songs)
    }
    /// The client used when no playback plugin is active but music-list plugins provide playback
    /// themselves: it has no script, only the sources those plugins provide.
    pub fn virtual_client(
        store: Storage,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        let songs = Self::songs_of(&store, existing)?;
        Ok(Arc::new(Self {
            id: String::new(),
            plugin: Arc::new(Plugin::placeholder()),
            store,
            songs,
            search_gate: Mutex::new(()),
            aggregate: Mutex::new(None),
        }))
    }
    /// `true` for [`Self::virtual_client`]: there is no playback plugin behind it.
    pub fn is_virtual(&self) -> bool {
        self.plugin.sources.is_empty()
    }
    /// Reads a plugin script from a file (at most 1 MiB, UTF-8).
    pub fn read_plugin_file(path: &Path) -> Result<String, String> {
        let file = std::fs::File::open(path).map_err(|_| "无法打开插件文件")?;
        let mut code = String::new();
        file.take(1024 * 1024 + 1)
            .read_to_string(&mut code)
            .map_err(|_| "插件不是有效 UTF-8")?;
        if code.len() > 1024 * 1024 {
            return Err("插件文件超过 1MiB".into());
        }
        Ok(code)
    }
    pub fn import(
        store: Storage,
        path: &Path,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        Self::install_playback(store, Self::read_plugin_file(path)?, existing)
    }
    /// A playback plugin install refuses a music-list plugin, which has its own install.
    fn install_playback(
        store: Storage,
        code: String,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        if crate::list_plugins::is_list_plugin(&code) {
            return Err("这是音乐列表插件，请在“音乐列表插件”中安装".into());
        }
        Self::install(store, code, existing)
    }
    pub fn import_url(
        store: Storage,
        raw: &str,
        existing: Option<Arc<Self>>,
    ) -> Result<Arc<Self>, String> {
        Self::install_playback(store, Self::download_plugin(raw)?, existing)
    }
    /// Downloads a plugin script (HTTP/HTTPS, at most 1 MiB, UTF-8).
    pub fn download_plugin(raw: &str) -> Result<String, String> {
        let url = mio_plugin::runtime_url(raw)?;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|_| "无法创建插件下载客户端")?;
        let started = std::time::Instant::now();
        let response = client.get(url).send().map_err(|error| {
            eprintln!(
                "plugin download failed: timeout={}, connect={}, elapsed_ms={}",
                error.is_timeout(),
                error.is_connect(),
                started.elapsed().as_millis()
            );
            "插件下载失败或超时"
        })?;
        if !response.status().is_success() {
            return Err(format!("插件下载返回 HTTP {}", response.status().as_u16()));
        }
        let mut code = Vec::new();
        response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut code)
            .map_err(|error| {
                let timeout = error.kind() == std::io::ErrorKind::TimedOut
                    || error
                        .get_ref()
                        .and_then(|source| source.downcast_ref::<reqwest::Error>())
                        .is_some_and(reqwest::Error::is_timeout);
                eprintln!(
                    "plugin body download failed: timeout={timeout}, received_bytes={}, elapsed_ms={}",
                    code.len(),
                    started.elapsed().as_millis()
                );
                if timeout {
                    "插件下载超时"
                } else {
                    "插件下载失败"
                }
            })?;
        if code.len() > 1024 * 1024 {
            return Err("插件文件超过 1MiB".into());
        }
        String::from_utf8(code).map_err(|_| "插件不是有效 UTF-8".into())
    }
    /// The sources the enabled list plugins register, in plugin-management order, that can
    /// play: through the list plugin itself or the active playback plugin. Names come from the
    /// list plugin; built-in SDK adapters alone do not make a source available.
    pub fn sources(&self) -> Vec<PluginSource> {
        let provided = crate::list_plugins::play_sources();
        let mut out: Vec<PluginSource> = Vec::new();
        for (id, name, origin) in mio_music_sdk::providers() {
            if !matches!(origin, mio_music_sdk::Origin::Plugin(_))
                || out.iter().any(|s| s.id == id)
                || !mio_music_sdk::is_implemented(&id)
            {
                continue;
            }
            let playable = provided
                .iter()
                .chain(&self.plugin.sources)
                .find(|s| s.id == id);
            if let Some(source) = playable {
                out.push(PluginSource {
                    name,
                    ..source.clone()
                });
            }
        }
        out
    }
    /// Playlist search. One source pages by `page` (30 per page, as the original); `None` searches
    /// every enabled source once (20 each) and de-duplicates, without paging.
    pub fn search_playlists(
        &self,
        source: Option<&str>,
        query: &str,
        page: usize,
        cancelled: Arc<AtomicBool>,
    ) -> Result<PlaylistPage, String> {
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let page = match source {
            Some(source) => {
                if !self.sources().iter().any(|s| s.id == source) {
                    return Err("当前插件不支持该来源".into());
                }
                let result = mio_music_sdk::search_playlist_source(source, query, page, 30)?;
                PlaylistPage {
                    total: result.total.max(0) as usize,
                    cards: result.list.into_iter().map(playlist_card).collect(),
                }
            }
            None => {
                let sources = self.sources();
                let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
                let cards: Vec<PlaylistCard> = mio_music_sdk::search_playlists_all(query, &ids)?
                    .into_iter()
                    .map(playlist_card)
                    .collect();
                PlaylistPage {
                    total: cards.len(),
                    cards,
                }
            }
        };
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        Ok(page)
    }
    /// Searches one source (`source = Some`) or every enabled source (`None`). `more` continues
    /// the previous query: `page` for a single source, the stored paging state for all sources.
    pub fn search(
        &self,
        source: Option<&str>,
        query: &str,
        page: usize,
        more: bool,
        keep: &HashSet<String>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<SearchPage, String> {
        // A search that was replaced while queued must not wait for the one in front of it: the
        // slow request ahead can take tens of seconds, and the replacement is already queued
        // behind it in the UI.
        let _gate = loop {
            match self.search_gate.try_lock() {
                Ok(gate) => break gate,
                Err(std::sync::TryLockError::Poisoned(_)) => return Err("在线搜索不可用".into()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if cancelled.load(Ordering::Relaxed) {
                        return Err("搜索已取消".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                }
            }
        };
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let (items, total, has_more, replace) = match source {
            Some(source) => {
                if !self.sources().iter().any(|s| s.id == source) {
                    return Err("当前插件不支持该来源".into());
                }
                let result = mio_music_sdk::search_source(source, query, page, 30)?;
                let has_more = (page as i64) < result.all_page && !result.list.is_empty();
                (result.list, result.total.max(0) as usize, has_more, false)
            }
            None => {
                let mut state = self.aggregate.lock().map_err(|_| "在线搜索不可用")?;
                if more {
                    let current = state
                        .as_mut()
                        .filter(|s| s.query() == query.trim())
                        .ok_or("聚合搜索状态已失效，请重新搜索")?;
                    current.next_page()?;
                } else {
                    let sources = self.sources();
                    let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
                    *state = Some(AggregateSearch::start(query, &ids)?);
                }
                let current = state.as_ref().ok_or("聚合搜索状态已失效，请重新搜索")?;
                let songs = current.songs().to_vec();
                (songs.clone(), songs.len(), current.has_more(), true)
            }
        };
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let songs = self.publish_fetched(items, source, keep, &cancelled)?;
        Ok(SearchPage {
            songs,
            total,
            has_more,
            replace,
        })
    }
    /// Fetch one independently pageable source without durable metadata writes.
    pub fn prepare_source_search(
        &self,
        provider: &Arc<dyn mio_music_sdk::MusicListProvider>,
        source: &str,
        query: &str,
        page: usize,
        cancelled: &AtomicBool,
    ) -> Result<PreparedSourceSearch, String> {
        let _gate = loop {
            if cancelled.load(Ordering::Acquire) {
                return Err("搜索已取消".into());
            }
            match self.search_gate.try_lock() {
                Ok(gate) => break gate,
                Err(std::sync::TryLockError::Poisoned(_)) => return Err("在线搜索不可用".into()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(40))
                }
            }
        };
        if !self.sources().iter().any(|s| s.id == source) {
            return Err("当前插件不支持该来源".into());
        }
        if provider.id() != source || !provider.capabilities().search {
            return Err("source_unavailable".into());
        }
        let result = provider.search(query, page, 30)?;
        if result.source != source || result.list.iter().any(|item| item.source != source) {
            return Err("invalid_data: search source".into());
        }
        if result.list.len() > 200 {
            return Err("capacity_exceeded: search page".into());
        }
        bounded_metadata_size(&result.list)?;
        let has_more = (page as i64) < result.all_page && !result.list.is_empty();
        let mut items = result.list;
        // Bound parallelism and use the exact same provider for missing artwork.
        if provider.capabilities().cover {
            for batch in items.chunks_mut(12) {
                if cancelled.load(Ordering::Acquire) {
                    return Err("cancelled".into());
                }
                std::thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter_mut()
                        .filter(|item| item.img.is_empty())
                        .map(|item| {
                            scope.spawn(move || {
                                if let Ok(url) = mio_music_sdk::get_cover_with(provider, item) {
                                    if url.len() <= 4096 {
                                        item.img = url;
                                    }
                                }
                            })
                        })
                        .collect();
                    for handle in handles {
                        let _ = handle.join();
                    }
                });
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        bounded_metadata_size(&items)?;
        Ok(PreparedSourceSearch {
            items,
            source: source.to_owned(),
            total: result.total.max(0) as usize,
            has_more,
            declared_pages: None,
            description: String::new(),
        })
    }

    /// Fetch from the captured provider; no registry replacement is consulted.
    /// Mobile admits the bounded metadata separately on its original owner.
    pub fn prepare_online_page(
        &self,
        provider: &Arc<dyn mio_music_sdk::MusicListProvider>,
        source: &str,
        id: &str,
        kind: OnlinePageKind,
        page: usize,
        cancelled: &AtomicBool,
    ) -> Result<PreparedSourceSearch, String> {
        self.prepare_online_page_inner(provider, source, id, kind, page, cancelled, false)
    }
    /// Whole-collection reads reject invalid totals/page counts instead of clamping them.
    pub fn prepare_complete_online_page(
        &self,
        provider: &Arc<dyn mio_music_sdk::MusicListProvider>,
        source: &str,
        id: &str,
        kind: OnlinePageKind,
        page: usize,
        cancelled: &AtomicBool,
    ) -> Result<PreparedSourceSearch, String> {
        self.prepare_online_page_inner(provider, source, id, kind, page, cancelled, true)
    }
    fn prepare_online_page_inner(
        &self,
        provider: &Arc<dyn mio_music_sdk::MusicListProvider>,
        source: &str,
        id: &str,
        kind: OnlinePageKind,
        page: usize,
        cancelled: &AtomicBool,
        strict: bool,
    ) -> Result<PreparedSourceSearch, String> {
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if provider.id() != source || id.is_empty() || id.len() > 512 || page == 0 || page > 100_000
        {
            return Err("invalid_input: online page".into());
        }
        let (items, total, all_page, actual_source, description) = match kind {
            OnlinePageKind::Singer => {
                let result = provider.singer_songs(id, page, 30)?;
                (
                    result.list,
                    result.total,
                    result.all_page,
                    result.source,
                    String::new(),
                )
            }
            _ => {
                let result = match kind {
                    OnlinePageKind::Chart => provider.chart_detail(id, page)?,
                    OnlinePageKind::Album => provider.album_detail(id, page, 30)?,
                    _ => provider.playlist_detail(id, page, 30)?,
                };
                (
                    result.list,
                    result.total,
                    result.all_page,
                    result.source,
                    collapse_whitespace(&result.info.desc),
                )
            }
        };
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if strict
            && (total < 0
                || all_page < 0
                || all_page > 400
                || (items.is_empty() && (page as i64) < all_page)
                || (total == 0 && !items.is_empty())
                || (all_page > 0 && page as i64 > all_page))
        {
            return Err("incomplete_collection: invalid pagination".into());
        }
        if actual_source != source
            || items.len() > 200
            || items.iter().any(|item| item.source != source)
        {
            return Err("invalid_data: online page source or size".into());
        }
        if description.len() > 16384 || description.contains('\0') {
            return Err("capacity_exceeded: playlist description".into());
        }
        bounded_metadata_size(&items)?;
        let has_more = (page as i64) < all_page && !items.is_empty();
        Ok(PreparedSourceSearch {
            items,
            source: source.to_owned(),
            total: total.max(0) as usize,
            has_more,
            declared_pages: Some(all_page.max(0) as usize),
            description,
        })
    }

    /// No provider or cover requests run here. Caller validates runtime/source
    /// on its owner before invoking this bounded durable metadata commit.
    pub fn publish_source_search(
        &self,
        prepared: PreparedSourceSearch,
        keep: &HashSet<String>,
        cancelled: &AtomicBool,
    ) -> Result<SearchPage, String> {
        let songs = self.publish(prepared.items, Some(&prepared.source), keep, cancelled)?;
        Ok(SearchPage {
            songs,
            total: prepared.total,
            has_more: prepared.has_more,
            replace: false,
        })
    }

    /// Fetched pages run on background workers. Resolve covers before publishing, never while
    /// holding metadata locks or restoring the persisted registry on the UI thread.
    fn publish_fetched(
        &self,
        items: Vec<MusicItem>,
        only_source: Option<&str>,
        keep: &HashSet<String>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<Song>, String> {
        let items = self.prepare_pics(items, cancelled)?;
        self.publish(items, only_source, keep, cancelled)
    }

    fn prepare_pics(
        &self,
        mut items: Vec<MusicItem>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<MusicItem>, String> {
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        // Reuse successful plugin lookups across pages and later searches.
        {
            let metadata = self.songs.lock().map_err(|_| "在线元数据不可用")?;
            for item in items.iter_mut().filter(|item| item.img.is_empty()) {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|id| id.to_string()));
                if let Some(saved) = raw.and_then(|raw| metadata.get(&song_id(&item.source, &raw)))
                {
                    item.img = saved.img.clone();
                }
            }
        }
        mio_music_sdk::fill_missing_pics(&mut items, cancelled);
        if cancelled.load(Ordering::Acquire) {
            return Err("搜索已取消".into());
        }
        Ok(items)
    }

    /// Turns SDK items into app songs and persists their metadata so they can be resolved later.
    /// `only_source` rejects items from any other source. When the cache is over budget only
    /// entries nothing references (`keep`, or this batch) are evicted.
    fn publish(
        &self,
        items: Vec<MusicItem>,
        only_source: Option<&str>,
        keep: &HashSet<String>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<Song>, String> {
        let mut metadata = self.songs.lock().map_err(|_| "在线元数据不可用")?;
        let mut next = metadata.clone();
        let mut songs = Vec::new();
        let mut fresh = HashSet::new();
        for item in items {
            let raw = item
                .songmid
                .as_str()
                .map(str::to_owned)
                .or_else(|| item.songmid.as_u64().map(|v| v.to_string()))
                .ok_or("在线歌曲缺少 ID")?;
            let enabled = self.sources().iter().any(|s| s.id == item.source);
            if raw.is_empty()
                || raw.len() > 512
                || !enabled
                || only_source.is_some_and(|s| s != item.source)
            {
                return Err("无效在线歌曲来源".into());
            }
            let id = song_id(&item.source, &raw);
            let duration = item
                .interval
                .split(':')
                .try_fold(0u64, |a, b| {
                    b.parse::<u64>()
                        .ok()
                        .and_then(|b| a.checked_mul(60)?.checked_add(b))
                })
                .unwrap_or(0);
            songs.push(Song {
                id: id.clone(),
                title: item.name.clone(),
                artist: item.singer.clone(),
                album: item.album_name.clone(),
                duration,
                cover_art: (!item.img.is_empty()).then(|| item.img.clone()),
            });
            fresh.insert(id.clone());
            next.insert(id, item);
        }
        // Over budget: drop only metadata nothing references (queue, active song, shown results,
        // this page). Queued/restorable songs are never evicted; if they alone exceed the budget
        // the search fails explicitly.
        if over_limit(&next)? {
            next.retain(|id, _| fresh.contains(id) || keep.contains(id));
            if over_limit(&next)? {
                return Err("在线歌曲缓存已满，现有歌曲信息已保留".into());
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        self.store
            .save("online-songs.json", &next)
            .map_err(|_| "无法保存在线歌曲元数据")?;
        *metadata = next;
        Ok(songs)
    }
    /// One page of a playlist's songs (30 per page), published like search results. `album` IDs
    /// come from a singer page and go through the source's album call.
    pub fn playlist_detail(
        &self,
        source: &str,
        id: &str,
        page: usize,
        album: bool,
        keep: &HashSet<String>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<PlaylistDetailPage, String> {
        if !self.sources().iter().any(|s| s.id == source) {
            return Err("当前插件不支持该来源".into());
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let detail = if let Some(chart) = id.strip_prefix("__chart__:") {
            mio_music_sdk::discovery_chart_detail(source, chart, page)?
        } else if album {
            mio_music_sdk::album_detail(source, id, page)?
        } else {
            mio_music_sdk::playlist_detail(source, id, page)?
        };
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let songs = self.publish_fetched(detail.list, Some(source), keep, &cancelled)?;
        Ok(PlaylistDetailPage {
            songs,
            total: detail.total.max(0) as usize,
            has_more: (page as i64) < detail.all_page,
            title: collapse_whitespace(&unescape_numeric_entities(&detail.info.name)),
            cover: (!detail.info.img.is_empty()).then_some(detail.info.img),
            description: collapse_whitespace(&detail.info.desc),
            author: collapse_whitespace(&detail.info.author),
        })
    }
    /// Source and singer ID of an online song, when the source gave one; local and Subsonic
    /// songs never have a singer page.
    pub fn singer_of(&self, id: &str) -> Option<(String, String)> {
        let items = self.songs.lock().ok()?;
        let item = items.get(id)?;
        let singer = item.singer_id.as_deref().filter(|s| !s.is_empty())?;
        Some((item.source.clone(), singer.to_string()))
    }
    /// Bound the mobile navigation reference before cloning cached provider data.
    pub fn bounded_singer_of(&self, id: &str) -> Result<Option<(String, String)>, String> {
        let items = self.songs.lock().map_err(|_| "metadata_unavailable")?;
        let Some(item) = items.get(id) else {
            return Ok(None);
        };
        let Some(singer) = item.singer_id.as_deref().filter(|s| !s.trim().is_empty()) else {
            return Ok(None);
        };
        if item.source.len() > 4096 || singer.len() > 512 || singer.chars().any(char::is_control) {
            return Err("capacity_exceeded: singer reference".into());
        }
        Ok(Some((item.source.clone(), singer.to_owned())))
    }
    fn singer_source(&self, source: &str) -> Result<(), String> {
        if !self.sources().iter().any(|s| s.id == source) {
            return Err("当前插件不支持该来源".into());
        }
        if !mio_music_sdk::is_singer_implemented(source) {
            return Err("该来源没有歌手页".into());
        }
        Ok(())
    }
    /// `None` for sources that publish no singer profile.
    pub fn singer_info(&self, source: &str, id: &str) -> Result<Option<SingerProfile>, String> {
        self.singer_source(source)?;
        Ok(
            mio_music_sdk::singer_info(source, id)?.map(|info| SingerProfile {
                name: unescape_numeric_entities(&info.name),
                desc: collapse_whitespace(&info.desc),
                avatar: (!info.avatar.is_empty()).then_some(info.avatar),
                music_count: info.music_count,
                album_count: info.album_count,
            }),
        )
    }
    /// One page (30) of a singer's songs, published like search results.
    pub fn singer_songs(
        &self,
        source: &str,
        id: &str,
        page: usize,
        keep: &HashSet<String>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<SearchPage, String> {
        self.singer_source(source)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let result = mio_music_sdk::singer_songs(source, id, page, 30)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let has_more = (page as i64) < result.all_page && !result.list.is_empty();
        let songs = self.publish_fetched(result.list, Some(source), keep, &cancelled)?;
        Ok(SearchPage {
            songs,
            total: result.total.max(0) as usize,
            has_more,
            replace: false,
        })
    }
    pub fn singer_albums(
        &self,
        source: &str,
        id: &str,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Vec<SingerAlbumCard>, String> {
        Ok(self.singer_albums_page(source, id, 1, cancelled)?.cards)
    }
    pub fn singer_albums_page(
        &self,
        source: &str,
        id: &str,
        page: usize,
        cancelled: Arc<AtomicBool>,
    ) -> Result<SingerAlbumsPage, String> {
        self.singer_source(source)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let result = mio_music_sdk::singer_albums(source, id, page, 30)?;
        if cancelled.load(Ordering::Relaxed) {
            return Err("搜索已取消".into());
        }
        let has_more = (page as i64) < result.all_page && !result.list.is_empty();
        let total = result.total.max(0) as usize;
        let cards = result
            .list
            .into_iter()
            .map(|album| SingerAlbumCard {
                id: album.id,
                name: unescape_numeric_entities(&album.name),
                author: album.author,
                cover: (!album.img.is_empty()).then_some(album.img),
                count: album.count,
                description: collapse_whitespace(&album.desc.unwrap_or_default()),
            })
            .collect();
        Ok(SingerAlbumsPage {
            cards,
            total,
            has_more,
        })
    }
    pub fn resolve(&self, id: &str, cancelled: Arc<AtomicBool>) -> Result<String, String> {
        self.resolve_quality(id, None, cancelled)
    }
    /// Metadata used by song rows without serializing on every render.
    pub fn item(&self, id: &str) -> Option<MusicItem> {
        self.songs.lock().ok()?.get(id).cloned()
    }
    /// Bound metadata before cloning for a mobile presentation session.
    pub fn bounded_item(&self, id: &str) -> Result<Option<MusicItem>, String> {
        let items = self.songs.lock().map_err(|_| "online_unavailable")?;
        let Some(item) = items.get(id) else {
            return Ok(None);
        };
        bounded_metadata_size(item)?;
        Ok(Some(item.clone()))
    }
    /// The stored MusicItem of an online song, as the original app persists it.
    pub fn item_json(&self, id: &str) -> Option<serde_json::Value> {
        let items = self.songs.lock().ok()?;
        items
            .get(id)
            .and_then(|item| serde_json::to_value(item).ok())
    }
    /// Makes songs read back from the library (favorites) resolvable again. Entries already
    /// known are left alone. Returns false when the metadata cache has no room.
    pub fn register(&self, rows: Vec<serde_json::Value>, keep: &HashSet<String>) -> bool {
        let items: Vec<MusicItem> = rows
            .into_iter()
            .filter_map(|row| serde_json::from_value(row).ok())
            .filter(|item: &MusicItem| {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|v| v.to_string()));
                let known = raw.is_some_and(|raw| {
                    self.songs
                        .lock()
                        .map(|songs| songs.contains_key(&song_id(&item.source, &raw)))
                        .unwrap_or(false)
                });
                !known && self.sources().iter().any(|s| s.id == item.source)
            })
            .collect();
        if items.is_empty() {
            return true;
        }
        let never = AtomicBool::new(false);
        self.publish(items, None, keep, &never).is_ok()
    }
    /// Qualities the plugin declares for a source.
    pub fn source_qualities(&self, source: &str) -> Vec<String> {
        self.sources()
            .into_iter()
            .find(|s| s.id == source)
            .map(|s| s.qualities)
            .unwrap_or_default()
    }
    /// The quality playback uses for a song: the one the settings ask for, or the nearest the
    /// plugin declares and the song offers (`getSongQuality` + `calculateBestQuality`); without
    /// a preference, 128k when offered, else the plugin's first.
    pub fn playback_quality(&self, item: &MusicItem) -> Option<String> {
        let all_sources = self.sources();
        let declared = &all_sources.iter().find(|s| s.id == item.source)?.qualities;
        if declared.is_empty() {
            return None;
        }
        let listed = item.types.as_deref().unwrap_or(&[]);
        let mut available: Vec<String> = declared
            .iter()
            .filter(|q| listed.is_empty() || listed.contains(q))
            .cloned()
            .collect();
        if available.is_empty() {
            available = declared.clone();
        }
        // `getSongQuality`: the quality of the song's own source, else the selected source's.
        let preference = crate::quality::preference();
        let sources = &all_sources;
        let selected = preference
            .selected_source
            .as_deref()
            .and_then(|id| sources.iter().find(|s| s.id == id))
            .or_else(|| sources.first());
        let user_quality = selected.and_then(|source| {
            let declared = &source.qualities;
            preference
                .per_source
                .get(&source.id)
                .filter(|q| declared.contains(q))
                .or(preference
                    .selected_quality
                    .as_ref()
                    .filter(|q| declared.contains(q)))
                .or_else(|| crate::quality::default_quality(declared))
                .cloned()
        });
        let wanted = preference
            .per_source
            .get(&item.source)
            .filter(|q| !q.is_empty())
            .cloned()
            .or(user_quality);
        if let Some(wanted) = wanted {
            if let Some(best) = crate::quality::best_available(&available, &wanted) {
                return Some(best);
            }
        }
        crate::quality::default_quality(declared).cloned()
    }
    /// The audio cache's key of a song at the quality playback uses: `{source}_{songmid}_{quality}`
    /// (`buildCacheKey` of the original).
    pub fn cache_key(&self, id: &str) -> Option<String> {
        let mut parts = id.splitn(3, ':');
        if parts.next()? != "mio-online" {
            return None;
        }
        let (source, raw) = (parts.next()?, parts.next()?);
        let item = self.songs.lock().ok()?.get(id).cloned()?;
        let quality = self.playback_quality(&item)?;
        Some(format!("{source}_{raw}_{quality}"))
    }
    /// Playback address of a song at `quality`; `None` keeps playback's default (128k if offered).
    pub fn resolve_quality(
        &self,
        id: &str,
        quality: Option<&str>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        let song = self
            .songs
            .lock()
            .map_err(|_| "在线元数据不可用")?
            .get(id)
            .cloned()
            .ok_or("缺少在线歌曲信息，请重新搜索")?;
        if !id.starts_with(&format!("mio-online:{}:", song.source)) {
            return Err("歌曲来源不匹配".into());
        }
        self.resolve_item(&song, quality, cancelled)
    }
    /// Playback address for a song given as its MusicItem, independent of the metadata cache
    /// (downloads restored from disk carry their own copy).
    pub fn resolve_item(
        &self,
        item: &MusicItem,
        quality: Option<&str>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.resolve_for(item, quality, cancelled, false)
    }
    /// Download address of a song: from the list plugin when it provides downloads for the
    /// source, otherwise the same as the playback address.
    pub fn resolve_download(
        &self,
        item: &MusicItem,
        quality: Option<&str>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<String, String> {
        self.resolve_for(item, quality, cancelled, true)
    }
    fn resolve_for(
        &self,
        item: &MusicItem,
        quality: Option<&str>,
        cancelled: Arc<AtomicBool>,
        download: bool,
    ) -> Result<String, String> {
        if !mio_music_sdk::is_implemented(&item.source) {
            return Err("歌曲来源不匹配".into());
        }
        let sources = self.sources();
        let source = sources
            .iter()
            .find(|s| s.id == item.source)
            .ok_or("当前插件不支持这首歌曲")?;
        let quality = match quality {
            Some(wanted) => source
                .qualities
                .iter()
                .find(|q| q.as_str() == wanted)
                .ok_or("插件未声明所选音质")?,
            None => {
                let chosen = self.playback_quality(item).ok_or("插件未声明音质")?;
                source
                    .qualities
                    .iter()
                    .find(|q| **q == chosen)
                    .ok_or("插件未声明音质")?
            }
        };
        if item.source == "tx" && std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
            eprintln!("[playback] plugin_input source=tx quality={quality}");
        }
        let song = serde_json::to_value(item).map_err(|_| "无效歌曲信息")?;
        // A list plugin that provides this source's playback / download answers instead of
        // the playback plugin, which is then not asked at all.
        let owner = mio_music_sdk::provider_with(&item.source, |c| c.play)
            .filter(|p| matches!(p.origin(), mio_music_sdk::Origin::Plugin(_)));
        let download_owner = if download {
            mio_music_sdk::provider_with(&item.source, |c| c.download)
                .filter(|p| matches!(p.origin(), mio_music_sdk::Origin::Plugin(_)))
        } else {
            None
        };
        if let Some(provider) = download_owner.or(owner) {
            if cancelled.load(Ordering::Relaxed) {
                return Err("解析已取消".into());
            }
            return provider.music_url(&song, quality, download);
        }
        self.plugin.resolve(&item.source, &song, quality, cancelled)
    }
}
pub fn is_online(song: &Song) -> bool {
    song.id.starts_with("mio-online:")
}
