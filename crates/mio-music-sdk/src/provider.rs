//! The unified music-list provider contract and its registry.
//!
//! Sources are provided only by installed, enabled external list plugins. All list API entry
//! points (`search_source`, `playlist_detail`, `discovery_*`, `singer_*`…) use this registry;
//! missing sources or capabilities never fall back to built-in adapters.
use crate::{
    DiscoveryCategories, DiscoveryChart, DiscoveryPlaylistPage, PlaylistDetail, PlaylistResult,
    SearchResult, SingerAlbumPage, SingerInfo,
};
use std::sync::{Arc, RwLock};

/// What a provider can answer. Callers check these before offering the matching UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub search: bool,
    pub search_playlist: bool,
    pub playlist_detail: bool,
    pub album_detail: bool,
    pub discovery: bool,
    pub singer: bool,
    pub lyrics: bool,
    pub comments: bool,
    pub cover: bool,
    /// The provider resolves playback addresses itself (`music_url`).
    pub play: bool,
    /// The provider resolves download addresses itself (`music_url` with `download`).
    pub download: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Builtin,
    /// The ID of the installed list plugin that provides the source.
    Plugin(String),
}

fn unsupported<T>(what: &str) -> Result<T, String> {
    Err(format!("该来源的{what}尚未接入"))
}

/// One source's music-list API. Every method has an "unsupported" default so a provider only
/// implements what its capabilities declare. Calls are blocking and run on worker threads.
pub trait MusicListProvider: Send + Sync {
    /// The source ID songs of this provider carry in `MusicItem::source`.
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// Optional display icon declared by the external plugin.
    fn plugin_name(&self) -> &str {
        self.name()
    }
    fn icon(&self) -> Option<&str> {
        None
    }
    fn origin(&self) -> Origin;
    fn capabilities(&self) -> Capabilities;

    fn search(&self, _keyword: &str, _page: usize, _limit: usize) -> Result<SearchResult, String> {
        unsupported("搜索")
    }
    fn search_playlist(
        &self,
        _keyword: &str,
        _page: usize,
        _limit: usize,
    ) -> Result<PlaylistResult, String> {
        unsupported("歌单搜索")
    }
    fn playlist_detail(
        &self,
        _id: &str,
        _page: usize,
        _limit: usize,
    ) -> Result<PlaylistDetail, String> {
        unsupported("歌单详情")
    }
    fn album_detail(
        &self,
        _id: &str,
        _page: usize,
        _limit: usize,
    ) -> Result<PlaylistDetail, String> {
        unsupported("专辑详情")
    }
    fn lyrics(&self, _song: &serde_json::Value) -> Result<serde_json::Value, String> {
        unsupported("歌词")
    }
    fn comments(
        &self,
        _song: &serde_json::Value,
        _page: u64,
        _hot: bool,
        _cursor: &str,
    ) -> Result<serde_json::Value, String> {
        unsupported("评论")
    }
    fn cover(&self, _song: &serde_json::Value) -> Result<String, String> {
        unsupported("封面")
    }
    /// Qualities this provider can resolve, best last; empty when it does not play.
    fn qualities(&self) -> Vec<String> {
        Vec::new()
    }
    /// The playback (or download) address of a song at `quality`; `song` is its `MusicItem`.
    fn music_url(
        &self,
        _song: &serde_json::Value,
        _quality: &str,
        _download: bool,
    ) -> Result<String, String> {
        unsupported("播放地址")
    }
    fn discovery_categories(&self) -> Result<DiscoveryCategories, String> {
        unsupported("发现分类")
    }
    fn discovery_playlists(
        &self,
        _category: &str,
        _page: usize,
    ) -> Result<DiscoveryPlaylistPage, String> {
        unsupported("发现歌单")
    }
    fn discovery_charts(&self) -> Result<Vec<DiscoveryChart>, String> {
        unsupported("榜单")
    }
    fn chart_detail(&self, _id: &str, _page: usize) -> Result<PlaylistDetail, String> {
        unsupported("榜单详情")
    }
    fn singer_info(&self, _id: &str) -> Result<Option<SingerInfo>, String> {
        unsupported("歌手页")
    }
    fn singer_songs(&self, _id: &str, _page: usize, _limit: usize) -> Result<SearchResult, String> {
        unsupported("歌手歌曲")
    }
    fn singer_albums(
        &self,
        _id: &str,
        _page: usize,
        _limit: usize,
    ) -> Result<SingerAlbumPage, String> {
        unsupported("歌手专辑")
    }
}

static OVERRIDES: RwLock<Vec<Arc<dyn MusicListProvider>>> = RwLock::new(Vec::new());

/// A source ID a plugin may introduce besides the original's seven: 2–16 lower-case letters,
/// digits and `_`, starting with a letter.
pub fn is_custom_source_id(id: &str) -> bool {
    (2..=16).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Registers a plugin's provider, replacing one that an earlier registration made for the same
/// source. Only the seven known source IDs are accepted: songs, quality names and the UI refer to
/// those.
pub fn register(provider: Arc<dyn MusicListProvider>) -> Result<(), String> {
    if !crate::IMPLEMENTED_SOURCES.contains(&provider.id()) && !is_custom_source_id(provider.id()) {
        return Err(format!("无效的来源 ID {}", provider.id()));
    }
    let mut list = OVERRIDES.write().map_err(|_| "来源注册表不可用")?;
    list.retain(|p| p.id() != provider.id());
    list.push(provider);
    Ok(())
}

/// Publish a prepared set only after the caller's durable commit succeeds.
/// The callback must perform bounded storage work, never provider/network calls.
pub fn replace_plugins(
    previous: &[std::sync::Weak<dyn MusicListProvider>],
    providers: Vec<Arc<dyn MusicListProvider>>,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if providers
        .iter()
        .any(|p| !crate::IMPLEMENTED_SOURCES.contains(&p.id()) && !is_custom_source_id(p.id()))
    {
        return Err("无效的来源 ID".into());
    }
    let mut list = OVERRIDES.write().map_err(|_| "来源注册表不可用")?;
    commit()?;
    list.retain(|p| {
        !previous
            .iter()
            .any(|old| old.upgrade().is_some_and(|old| Arc::ptr_eq(p, &old)))
    });
    for provider in providers {
        list.retain(|p| p.id() != provider.id());
        list.push(provider);
    }
    Ok(())
}

/// Remove only the actual registered instances owned by a retiring host.
pub fn unregister_matching(previous: &[std::sync::Weak<dyn MusicListProvider>]) {
    if let Ok(mut list) = OVERRIDES.write() {
        list.retain(|p| {
            !previous
                .iter()
                .any(|old| old.upgrade().is_some_and(|old| Arc::ptr_eq(p, &old)))
        });
    }
}

/// Drops every provider the given plugin registered; its sources become unavailable.
pub fn unregister_plugin(plugin_id: &str) {
    if let Ok(mut list) = OVERRIDES.write() {
        list.retain(|p| p.origin() != Origin::Plugin(plugin_id.to_owned()));
    }
}

/// The enabled external provider that declares the requested capability.
pub fn provider_with(
    source: &str,
    has: fn(&Capabilities) -> bool,
) -> Option<Arc<dyn MusicListProvider>> {
    provider(source).filter(|p| has(&p.capabilities()))
}

/// The enabled external provider answering for `source`.
pub fn provider(source: &str) -> Option<Arc<dyn MusicListProvider>> {
    OVERRIDES
        .read()
        .ok()?
        .iter()
        .find(|p| p.id() == source)
        .cloned()
}

/// Only capabilities declared by the enabled external provider are available.
pub fn capabilities(source: &str) -> Capabilities {
    provider(source)
        .map(|p| p.capabilities())
        .unwrap_or_default()
}

/// Registered external sources in registration order.
pub fn providers() -> Vec<(String, String, Origin)> {
    OVERRIDES
        .read()
        .map(|list| {
            list.iter()
                .map(|p| (p.id().to_owned(), p.name().to_owned(), p.origin()))
                .collect()
        })
        .unwrap_or_default()
}
