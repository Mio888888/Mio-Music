//! Music-list plugins: an installable script that answers search / playlist / album / discovery /
//! singer requests for one or more sources, in place of the built-in adapters.
//!
//! A list plugin is a script in the same bounded QuickJS host as playback plugins (HTTP, timers,
//! `md5`/`sha1`, base64; no filesystem, process or module loader). It exports:
//!
//! ```js
//! module.exports = { listPlugin: {
//!   name, version, author, description,
//!   sources: { tx: { name: 'QQ 音乐', icon: '🎵', capabilities: ['search', 'searchPlaylist', ...] } },
//!   async search(source, { keyword, page, limit }) { return SearchResult },
//!   async searchPlaylist(source, { keyword, page, limit }) { return PlaylistResult },
//!   async playlistDetail(source, { id, page, limit }) { return PlaylistDetail },
//!   async albumDetail(source, { id, page, limit }) { return PlaylistDetail },
//!   async discoveryCategories(source) { return DiscoveryCategories },
//!   async discoveryPlaylists(source, { category, page }) { return DiscoveryPlaylistPage },
//!   async discoveryCharts(source) { return DiscoveryChart[] },
//!   async chartDetail(source, { id, page }) { return PlaylistDetail },
//!   async singerInfo(source, { id }) { return SingerInfo | null },
//!   async singerSongs(source, { id, page, limit }) { return SearchResult },
//!   async singerAlbums(source, { id, page, limit }) { return SingerAlbumPage },
//! } };
//! ```
//!
//! Returned values have the camelCase shape of the matching `mio-music-sdk` type. The host
//! stamps every song and result with the source the call was for, so a plugin cannot answer as
//! another source.
use crate::runtime::{Kind, Logs, PluginEnv, execute_as, execute_env, push_log, validate_url};
use mio_music_sdk::{
    Capabilities, DiscoveryCategories, DiscoveryChart, DiscoveryPlaylistPage, MusicItem,
    MusicListProvider, Origin, PlaylistDetail, PlaylistResult, SearchResult, SingerAlbumPage,
    SingerInfo,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

const SOURCE_IDS: &[&str] = &["kw", "wy", "kg", "tx", "mg", "bd", "git"];
const CAPABILITIES: &[&str] = &[
    "search",
    "searchPlaylist",
    "playlistDetail",
    "albumDetail",
    "discovery",
    "singer",
    "lyrics",
    "comments",
    "cover",
    "play",
    "download",
    "testConnection",
    "userPlaylists",
];
const QUALITIES: &[&str] = &[
    "128k",
    "320k",
    "flac",
    "flac24bit",
    "hires",
    "atmos",
    "atmos_plus",
    "master",
];
const FIELD_KINDS: &[&str] = &["text", "password", "number", "switch", "select", "textarea"];
const MAX_FIELDS: usize = 32;

/// One field of a plugin's configuration form, as the author declared it.
#[derive(Clone, Debug)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    /// `text`, `password`, `number`, `switch`, `select` or `textarea`.
    pub kind: String,
    pub default: Value,
    pub required: bool,
    pub placeholder: String,
    pub description: String,
    /// `(label, value)` pairs of a `select`.
    pub options: Vec<(String, String)>,
}
/// A page of results is never larger than this, whatever the plugin returns.
const MAX_ITEMS: usize = 500;

#[derive(Clone, Debug)]
pub struct ListSource {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub capabilities: Vec<String>,
    /// Qualities the source plays at (with the `play` capability), best last.
    pub qualities: Vec<String>,
}

#[derive(Clone)]
pub struct ListPlugin {
    code: Arc<str>,
    pub name: String,
    /// What the interface shows instead of `name` (which identifies the plugin), in the
    /// interface language; empty when the script declares none.
    pub display_name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub sources: Vec<ListSource>,
    /// The configuration form the author declared (may be empty).
    pub config: Vec<ConfigField>,
    /// Whether the plugin asks to provide playback / download itself by default.
    pub default_play: bool,
    pub default_download: bool,
    pub legacy_service: bool,
    env: Option<PluginEnv>,
    logs: Logs,
}

/// `true` when the script declares itself a list plugin (cheap check before loading it).
pub fn looks_like_list_plugin(code: &str) -> bool {
    code.contains("listPlugin") || code.contains("configSchema") || code.contains("getPlaylists")
}

impl ListPlugin {
    pub fn load(code: String) -> Result<Self, String> {
        if code.is_empty() || code.len() > 1024 * 1024 {
            return Err("插件文件为空或超过 1MiB".into());
        }
        // Keep the stored script intact; adapt its CommonJS exports inside the bounded host.
        let comment = |key: &str| {
            regex_lite::Regex::new(&format!(r"@{key}\s+([^\r\n]+)"))
                .ok()
                .and_then(|r| {
                    r.captures(&code)
                        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_owned()))
                })
                .unwrap_or_default()
        };
        let fallback = json!({"name":comment("name"),"version":comment("version"),"author":comment("author"),"description":comment("description")});
        let code = format!(
            "{code}\n;if(!module.exports.listPlugin){{if(!module.exports.configSchema&&typeof configSchema!=='undefined')module.exports.configSchema=configSchema;if(!module.exports.pluginInfo&&typeof pluginInfo!=='undefined')module.exports.pluginInfo=pluginInfo;if(!module.exports.sources&&typeof sources!=='undefined')module.exports.sources=sources;}}module.exports=__adaptLegacyService(module.exports,{fallback});"
        );
        let logs: Logs = Default::default();
        let meta = execute_as(
            Kind::List,
            &code,
            "var t=(v,n)=>typeof v==='string'?v.slice(0,n):''; return {name:t(__list.name,256), displayName:t(__list.displayName,256), version:t(__list.version,64), author:t(__list.author,256), description:t(__list.description,1000), icon:t(__list.icon,65536), sources: __list.sources||{}, config: Array.isArray(__list.config)?__list.config:[], defaults: __list.defaults||{}, legacyService:__list.__legacyService===true};",
            Arc::new(AtomicBool::new(false)),
            &logs,
        )?;
        let text = |key: &str| {
            meta.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_owned()
        };
        let name = text("name");
        if name.is_empty() {
            return Err("列表插件缺少名称".into());
        }
        let sources: Vec<ListSource> = meta
            .get("sources")
            .and_then(Value::as_object)
            .ok_or("列表插件未声明来源")?
            .iter()
            .filter(|(id, _)| {
                SOURCE_IDS.contains(&id.as_str()) || mio_music_sdk::is_custom_source_id(id)
            })
            .map(|(id, data)| ListSource {
                id: id.clone(),
                name: data
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .unwrap_or(id)
                    .to_owned(),
                icon: data
                    .get("icon")
                    .or_else(|| meta.get("icon"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 65536)
                    .map(str::to_owned),
                capabilities: data
                    .get("capabilities")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .filter(|c| CAPABILITIES.contains(c))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                qualities: data
                    .get("qualities")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .filter(|q| QUALITIES.contains(q))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .filter(|s| !s.capabilities.is_empty())
            .collect();
        if sources.is_empty() {
            return Err("列表插件没有声明有效来源及能力".into());
        }
        push_log(
            &logs,
            format!(
                "[info] 列表插件已加载：{name}，来源 {}",
                sources
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        );
        Ok(Self {
            code: code.into(),
            name,
            display_name: text("displayName"),
            version: text("version"),
            author: text("author"),
            description: text("description"),
            sources,
            config: parse_config(meta.get("config")),
            default_play: flag(meta.get("defaults"), "play"),
            default_download: flag(meta.get("defaults"), "download"),
            legacy_service: meta
                .get("legacyService")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            env: None,
            logs,
        })
    }

    /// The plugin with its configuration values and persistent store attached; every call runs
    /// with them.
    pub fn with_env(mut self, env: PluginEnv) -> Self {
        self.env = Some(env);
        self
    }

    /// What the plugin wrote with `console.*` and what the host noted about it, oldest first.
    pub fn logs(&self) -> Vec<crate::LogLine> {
        self.logs
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Optional service operation; an absent method is reported, never treated as success.
    pub fn test_connection(&self) -> Result<(), String> {
        let source = &self
            .sources
            .iter()
            .find(|s| s.capabilities.iter().any(|c| c == "testConnection"))
            .ok_or("插件未提供连接验证")?
            .id;
        let result: Value = self.call("testConnection", true, source, json!({}))?;
        if result == Value::Bool(true)
            || result.get("success").and_then(Value::as_bool) == Some(true)
        {
            Ok(())
        } else {
            Err("连接验证失败".into())
        }
    }

    /// The user's server playlists, separate from public playlist search/discovery.
    pub fn user_playlists(&self, source: &str, page: usize) -> Result<PlaylistResult, String> {
        if !self
            .sources
            .iter()
            .any(|s| s.id == source && s.capabilities.iter().any(|c| c == "userPlaylists"))
        {
            return Err("插件未声明该来源".into());
        }
        let mut result: PlaylistResult = self.call(
            "getPlaylists",
            true,
            source,
            json!({"page": page, "limit": 100}),
        )?;
        if result.list.len()
            > if self.legacy_service {
                10_000
            } else {
                MAX_ITEMS
            }
        {
            return Err("插件歌单单页超限".into());
        }
        result.source = source.to_owned();
        for item in &mut result.list {
            item.source = source.to_owned();
        }
        Ok(result)
    }

    /// Legacy services return a whole SongList array, with original song sources/extra fields.
    pub fn legacy_playlist_songs(&self, source: &str, id: &str) -> Result<Vec<Value>, String> {
        if !self.legacy_service || !self.sources.iter().any(|s| s.id == source) {
            return Err("不是所选旧服务来源".into());
        }
        let songs: Vec<Value> = self.call("importSongs", true, source, json!({"id": id}))?;
        if songs.len() > 40_000 {
            return Err("歌单歌曲数量超限".into());
        }
        for song in &songs {
            let source = song
                .get("source")
                .and_then(Value::as_str)
                .ok_or("歌曲来源无效")?;
            if !mio_music_sdk::is_implemented(source)
                && !self
                    .sources
                    .iter()
                    .any(|s| s.id == source && s.capabilities.iter().any(|c| c == "play"))
            {
                return Err("歌曲来源不可用，请先安装对应音源或列表插件".into());
            }
        }
        Ok(songs)
    }

    /// One provider per declared source, for [`mio_music_sdk::register`]. `plugin_id` is the
    /// installed plugin's ID, used to unregister them again.
    ///
    /// `play` / `download` say whether the user lets the plugin provide playback / download
    /// addresses; without them the `play` / `download` capabilities are not offered even when
    /// the plugin declares them, and the source's songs keep resolving through the playback
    /// plugin.
    pub fn providers(
        self: &Arc<Self>,
        plugin_id: &str,
        play: bool,
        download: bool,
    ) -> Vec<Arc<dyn MusicListProvider>> {
        self.sources
            .iter()
            .map(|source| {
                let mut caps = capabilities_of(&source.capabilities);
                caps.play &= play;
                caps.download &= download;
                Arc::new(ListProvider {
                    plugin: self.clone(),
                    plugin_id: plugin_id.to_owned(),
                    id: source.id.clone(),
                    name: source.name.clone(),
                    icon: source.icon.clone(),
                    caps,
                    qualities: source.qualities.clone(),
                }) as Arc<dyn MusicListProvider>
            })
            .collect()
    }

    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        capability: bool,
        source: &str,
        args: Value,
    ) -> Result<T, String> {
        if !capability {
            return Err("插件未声明该能力".into());
        }
        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some()
            && matches!(method, "lyrics" | "comments" | "cover")
        {
            eprintln!("[plugin] call source={source} capability={method}");
        }
        let source = serde_json::to_string(source).map_err(|_| "无效来源")?;
        let action = format!(
            "if(typeof __list.{method}!=='function')throw Error('插件缺少 {method}'); return await __list.{method}({source}, {args});"
        );
        let value = execute_env(
            Kind::List,
            &self.code,
            &action,
            Arc::new(AtomicBool::new(false)),
            &self.logs,
            self.env.clone(),
        )
        .inspect_err(|error| {
            push_log(&self.logs, format!("[error] {method} 失败：{error}"));
        })?;
        serde_json::from_value(value).map_err(|error| {
            push_log(&self.logs, format!("[error] {method} 返回值无效：{error}"));
            "插件返回的数据格式无效".to_owned()
        })
    }
}

fn capabilities_of(list: &[String]) -> Capabilities {
    let has = |name: &str| list.iter().any(|c| c == name);
    Capabilities {
        search: has("search"),
        search_playlist: has("searchPlaylist"),
        playlist_detail: has("playlistDetail"),
        album_detail: has("albumDetail"),
        discovery: has("discovery"),
        singer: has("singer"),
        lyrics: has("lyrics"),
        comments: has("comments"),
        cover: has("cover"),
        play: has("play"),
        download: has("download"),
    }
}

fn flag(defaults: Option<&Value>, key: &str) -> bool {
    defaults
        .and_then(|d| d.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The author's form declaration, bounded and normalised: unknown kinds become `text`, fields
/// without a key or with a duplicate key are dropped.
fn parse_config(value: Option<&Value>) -> Vec<ConfigField> {
    let mut seen = std::collections::HashSet::new();
    let mut fields = Vec::new();
    for raw in value.and_then(Value::as_array).into_iter().flatten() {
        if fields.len() >= MAX_FIELDS {
            break;
        }
        let text = |key: &str, max: usize| {
            raw.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(max)
                .collect::<String>()
        };
        let key = text("key", 64);
        if key.is_empty()
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
            || !seen.insert(key.clone())
        {
            continue;
        }
        let kind = text("type", 16);
        let kind = if FIELD_KINDS.contains(&kind.as_str()) {
            kind
        } else {
            "text".to_owned()
        };
        let options = raw
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(32)
            .filter_map(|o| {
                let value = match o.get("value")? {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => return None,
                };
                let label = o
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(&value)
                    .chars()
                    .take(64)
                    .collect();
                Some((label, value))
            })
            .collect();
        fields.push(ConfigField {
            label: {
                let label = text("label", 64);
                if label.is_empty() { key.clone() } else { label }
            },
            key,
            kind,
            default: raw.get("default").cloned().unwrap_or(Value::Null),
            required: raw
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            placeholder: text("placeholder", 128),
            description: text("description", 256),
            options,
        });
    }
    fields
}

struct ListProvider {
    plugin: Arc<ListPlugin>,
    plugin_id: String,
    id: String,
    name: String,
    icon: Option<String>,
    caps: Capabilities,
    qualities: Vec<String>,
}

fn stamp_songs(list: &mut Vec<MusicItem>, source: &str) {
    list.truncate(MAX_ITEMS);
    for song in list {
        song.source = source.to_owned();
    }
}

impl ListProvider {
    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        capability: bool,
        args: Value,
    ) -> Result<T, String> {
        self.plugin.call(method, capability, &self.id, args)
    }
}

impl MusicListProvider for ListProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn plugin_name(&self) -> &str {
        &self.plugin.name
    }
    fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }
    fn origin(&self) -> Origin {
        Origin::Plugin(self.plugin_id.clone())
    }
    fn capabilities(&self) -> Capabilities {
        self.caps
    }

    fn lyrics(&self, song: &Value) -> Result<Value, String> {
        self.call("lyrics", self.caps.lyrics, json!({"song": song}))
    }
    fn comments(&self, song: &Value, page: u64, hot: bool, cursor: &str) -> Result<Value, String> {
        self.call(
            "comments",
            self.caps.comments,
            json!({"song": song,"page":page,"limit":30,"hot":hot,"cursor":cursor}),
        )
    }
    fn cover(&self, song: &Value) -> Result<String, String> {
        let url: String = self.call("cover", self.caps.cover, json!({"song":song}))?;
        if !url.is_empty() {
            validate_url(&url)?;
        }
        Ok(url)
    }

    fn qualities(&self) -> Vec<String> {
        self.qualities.clone()
    }
    fn music_url(&self, song: &Value, quality: &str, download: bool) -> Result<String, String> {
        let capability = if download {
            self.caps.download
        } else {
            self.caps.play
        };
        if !capability {
            return Err("插件未提供该能力".into());
        }
        let source = serde_json::to_string(&self.id).map_err(|_| "无效来源")?;
        let args = json!({"song": song, "quality": quality});
        let pick = if download {
            "var f = __list.downloadUrl || __list.musicUrl;"
        } else {
            "var f = __list.musicUrl;"
        };
        let action = format!(
            "{pick} if(typeof f!=='function')throw Error('插件缺少 musicUrl'); return await f.call(__list, {source}, {args});"
        );
        let value = execute_env(
            Kind::List,
            &self.plugin.code,
            &action,
            Arc::new(AtomicBool::new(false)),
            &self.plugin.logs,
            self.plugin.env.clone(),
        )
        .inspect_err(|error| {
            push_log(
                &self.plugin.logs,
                format!("[error] 解析 {} 失败：{error}", self.id),
            );
        })?;
        let url = value
            .as_str()
            .or_else(|| value.get("url").and_then(Value::as_str))
            .ok_or("插件没有返回播放地址")?;
        validate_url(url)?;
        Ok(url.to_owned())
    }

    fn search(&self, keyword: &str, page: usize, limit: usize) -> Result<SearchResult, String> {
        let mut result: SearchResult = self.call(
            "search",
            self.caps.search,
            json!({"keyword": keyword, "page": page, "limit": limit}),
        )?;
        stamp_songs(&mut result.list, &self.id);
        result.source = self.id.clone();
        Ok(result)
    }
    fn search_playlist(
        &self,
        keyword: &str,
        page: usize,
        limit: usize,
    ) -> Result<PlaylistResult, String> {
        let mut result: PlaylistResult = self.call(
            "searchPlaylist",
            self.caps.search_playlist,
            json!({"keyword": keyword, "page": page, "limit": limit}),
        )?;
        result.list.truncate(MAX_ITEMS);
        for item in &mut result.list {
            item.source = self.id.clone();
        }
        result.source = self.id.clone();
        Ok(result)
    }
    fn playlist_detail(
        &self,
        id: &str,
        page: usize,
        limit: usize,
    ) -> Result<PlaylistDetail, String> {
        let mut detail: PlaylistDetail = self.call(
            "playlistDetail",
            self.caps.playlist_detail,
            json!({"id": id, "page": page, "limit": limit}),
        )?;
        stamp_songs(&mut detail.list, &self.id);
        detail.source = self.id.clone();
        Ok(detail)
    }
    fn album_detail(&self, id: &str, page: usize, limit: usize) -> Result<PlaylistDetail, String> {
        let mut detail: PlaylistDetail = self.call(
            "albumDetail",
            self.caps.album_detail,
            json!({"id": id, "page": page, "limit": limit}),
        )?;
        stamp_songs(&mut detail.list, &self.id);
        detail.source = self.id.clone();
        Ok(detail)
    }
    fn discovery_categories(&self) -> Result<DiscoveryCategories, String> {
        let mut result: DiscoveryCategories =
            self.call("discoveryCategories", self.caps.discovery, json!({}))?;
        result.source = self.id.clone();
        for category in result
            .hot
            .iter_mut()
            .chain(result.groups.iter_mut().flat_map(|g| g.list.iter_mut()))
        {
            category.source = self.id.clone();
        }
        Ok(result)
    }
    fn discovery_playlists(
        &self,
        category: &str,
        page: usize,
    ) -> Result<DiscoveryPlaylistPage, String> {
        let mut result: DiscoveryPlaylistPage = self.call(
            "discoveryPlaylists",
            self.caps.discovery,
            json!({"category": category, "page": page}),
        )?;
        result.list.truncate(MAX_ITEMS);
        for item in &mut result.list {
            item.source = self.id.clone();
        }
        result.source = self.id.clone();
        Ok(result)
    }
    fn discovery_charts(&self) -> Result<Vec<DiscoveryChart>, String> {
        let mut charts: Vec<DiscoveryChart> =
            self.call("discoveryCharts", self.caps.discovery, json!({}))?;
        charts.truncate(MAX_ITEMS);
        for chart in &mut charts {
            chart.source = self.id.clone();
        }
        Ok(charts)
    }
    fn chart_detail(&self, id: &str, page: usize) -> Result<PlaylistDetail, String> {
        let mut detail: PlaylistDetail = self.call(
            "chartDetail",
            self.caps.discovery,
            json!({"id": id, "page": page}),
        )?;
        stamp_songs(&mut detail.list, &self.id);
        detail.source = self.id.clone();
        Ok(detail)
    }
    fn singer_info(&self, id: &str) -> Result<Option<SingerInfo>, String> {
        let mut info: Option<SingerInfo> =
            self.call("singerInfo", self.caps.singer, json!({"id": id}))?;
        if let Some(info) = &mut info {
            info.source = self.id.clone();
        }
        Ok(info)
    }
    fn singer_songs(&self, id: &str, page: usize, limit: usize) -> Result<SearchResult, String> {
        let mut result: SearchResult = self.call(
            "singerSongs",
            self.caps.singer,
            json!({"id": id, "page": page, "limit": limit}),
        )?;
        stamp_songs(&mut result.list, &self.id);
        result.source = self.id.clone();
        Ok(result)
    }
    fn singer_albums(
        &self,
        id: &str,
        page: usize,
        limit: usize,
    ) -> Result<SingerAlbumPage, String> {
        let mut result: SingerAlbumPage = self.call(
            "singerAlbums",
            self.caps.singer,
            json!({"id": id, "page": page, "limit": limit}),
        )?;
        result.list.truncate(MAX_ITEMS);
        result.source = self.id.clone();
        Ok(result)
    }
}
