//! Music-list contracts and routing for installed external plugins.
mod aggregate;
mod comments;
pub use comments::{
    get as get_comments, get_with as get_comments_with, image_url as comment_image_url,
};
mod discovery;
mod lyric;
mod pic;
mod provider;
mod types;

pub use aggregate::{
    AGGREGATE_PAGE_SIZE, AGGREGATE_PLAYLIST_PAGE_SIZE, AggregateSearch, dedupe_and_sort_playlists,
    dedupe_and_sort_songs, search_playlists_all,
};
pub use discovery::{
    DISCOVERY_SOURCES, DiscoveryCategories, DiscoveryCategory, DiscoveryCategoryGroup,
    DiscoveryChart, DiscoveryPlaylistPage,
};
pub use lyric::{
    Lyric, decode_payload as decode_lyric_payload, get as get_lyric,
    ttml::{Found as FoundLyric, TimedLine, TimedWord, find as find_lyric, parse as parse_ttml},
};
pub use provider::{
    Capabilities, MusicListProvider, Origin, capabilities, is_custom_source_id, provider,
    provider_with, providers, register, replace_plugins, unregister_matching, unregister_plugin,
};
pub use types::{
    MusicItem, PlaylistDetail, PlaylistInfo, PlaylistItem, PlaylistResult, QualityInfo,
    SearchResult, SingerAlbum, SingerAlbumPage, SingerInfo,
};

/// Sources of the original, in its aggregate order; each is answered by one
/// [`MusicListProvider`] (see the `provider` module).
pub const IMPLEMENTED_SOURCES: &[&str] = &["kw", "wy", "kg", "tx", "mg", "bd", "git"];

pub use pic::{fill_missing_pics, get as get_cover, get_with as get_cover_with};

pub fn is_implemented(source: &str) -> bool {
    let caps = capabilities(source);
    caps.search || caps.play
}

pub fn is_playlist_implemented(source: &str) -> bool {
    capabilities(source).search_playlist
}

pub fn is_singer_implemented(source: &str) -> bool {
    capabilities(source).singer
}

type Shared = std::sync::Arc<dyn MusicListProvider>;

fn require(source: &str, has: fn(&Capabilities) -> bool, missing: &str) -> Result<Shared, String> {
    provider_with(source, has).ok_or_else(|| missing.to_string())
}

fn check_page(page: usize) -> Result<(), String> {
    if page == 0 {
        return Err("无效歌单参数".into());
    }
    Ok(())
}

fn check_id(id: &str) -> Result<&str, String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 128 || id == "0" {
        return Err("无效歌手参数".into());
    }
    Ok(id)
}

/// Playlist contents (the original `getPlaylistDetail`), 30 songs per page. `id` is a playlist ID
/// as returned by [`search_playlist_source`].
pub fn playlist_detail(source: &str, id: &str, page: usize) -> Result<PlaylistDetail, String> {
    check_page(page)?;
    require(source, |c| c.playlist_detail, "该来源的歌单详情尚未接入")?
        .playlist_detail(id, page, 30)
}

/// Album contents for sources whose original singer page lists albums. Album IDs use their
/// own detail endpoints, rather than being interpreted as unrelated playlist IDs.
pub fn album_detail(source: &str, id: &str, page: usize) -> Result<PlaylistDetail, String> {
    check_page(page)?;
    require(source, |c| c.album_detail, "该来源的专辑详情尚未接入")?.album_detail(id, page, 30)
}

fn check_search(keyword: &str, page: usize, limit: usize) -> Result<(), String> {
    if keyword.trim().is_empty() || keyword.len() > 1024 || page == 0 || !(1..=30).contains(&limit)
    {
        return Err("无效搜索参数".into());
    }
    Ok(())
}

/// Single-source playlist search (the original `searchPlaylist`). `page` is 1-based.
pub fn search_playlist_source(
    source: &str,
    keyword: &str,
    page: usize,
    limit: usize,
) -> Result<PlaylistResult, String> {
    check_search(keyword, page, limit)?;
    require(source, |c| c.search_playlist, "该来源的歌单搜索尚未接入")?
        .search_playlist(keyword, page, limit)
}

/// Kuwo search, kept for existing callers; see [`search_source`].
pub fn search(keyword: &str, page: usize, limit: usize) -> Result<SearchResult, String> {
    search_source("kw", keyword, page, limit)
}

/// Single-source song search (the original `search` SDK method). `page` is 1-based and `limit`
/// is 1..=30, the largest page the app requests.
pub fn search_source(
    source: &str,
    keyword: &str,
    page: usize,
    limit: usize,
) -> Result<SearchResult, String> {
    check_search(keyword, page, limit)?;
    require(source, |c| c.search, "该来源的搜索尚未接入")?.search(keyword, page, limit)
}

fn discovery_provider(source: &str) -> Result<Shared, String> {
    require(source, |c| c.discovery, "该来源没有发现接口")
}

pub fn discovery_categories(source: &str) -> Result<DiscoveryCategories, String> {
    discovery_provider(source)?.discovery_categories()
}

/// One provider page. Empty category means all.
pub fn discovery_playlists(
    source: &str,
    category: &str,
    page: usize,
) -> Result<DiscoveryPlaylistPage, String> {
    if page == 0 || page > 100_000 || category.len() > 128 || category.chars().any(char::is_control)
    {
        return Err("无效发现歌单参数".into());
    }
    discovery_provider(source)?.discovery_playlists(category, page)
}

pub fn discovery_charts(source: &str) -> Result<Vec<DiscoveryChart>, String> {
    discovery_provider(source)?.discovery_charts()
}

/// A page of chart songs. Chart IDs are separate from playlist IDs on kw/kg/tx/mg.
pub fn discovery_chart_detail(
    source: &str,
    id: &str,
    page: usize,
) -> Result<PlaylistDetail, String> {
    if page == 0 || page > 100_000 || id.is_empty() || id.len() > 40 {
        return Err("无效榜单参数".into());
    }
    discovery_provider(source)?.chart_detail(id, page)
}

fn singer_provider(source: &str) -> Result<Shared, String> {
    require(source, |c| c.singer, "该来源没有歌手页")
}

/// Profile of one singer; `None` for sources without a profile endpoint.
pub fn singer_info(source: &str, id: &str) -> Result<Option<SingerInfo>, String> {
    singer_provider(source)?.singer_info(check_id(id)?)
}

pub fn singer_songs(
    source: &str,
    id: &str,
    page: usize,
    limit: usize,
) -> Result<SearchResult, String> {
    let id = check_id(id)?;
    if page == 0 || !(1..=30).contains(&limit) {
        return Err("无效歌手参数".into());
    }
    singer_provider(source)?.singer_songs(id, page, limit)
}

pub fn singer_albums(
    source: &str,
    id: &str,
    page: usize,
    limit: usize,
) -> Result<SingerAlbumPage, String> {
    let id = check_id(id)?;
    if page == 0 || !(1..=30).contains(&limit) {
        return Err("无效歌手参数".into());
    }
    singer_provider(source)?.singer_albums(id, page, limit)
}
