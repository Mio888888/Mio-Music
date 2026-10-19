//! Source-bound discovery and playlist paging; provider work never holds owner.
use crate::{api::*, catalog::SnapshotPermit, runtime};
use mio_core::{
    app_core::AppCore,
    online::{Online, OnlinePageKind},
    song::Song,
};
use mio_music_sdk::MusicListProvider;
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
static WORKERS: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct Worker;
impl Worker {
    pub(crate) fn acquire() -> Result<Self, String> {
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "busy: discovery workers occupied")?;
        Ok(Self)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
#[derive(Clone)]
pub(crate) struct Guard {
    pub(crate) online: Arc<Online>,
    pub(crate) provider: Arc<dyn MusicListProvider>,
    pub(crate) source: String,
    pub(crate) cancelled: Arc<AtomicBool>,
    parent: Option<Arc<AtomicBool>>,
}
impl Guard {
    pub(crate) fn new(
        online: Arc<Online>,
        provider: Arc<dyn MusicListProvider>,
        source: String,
    ) -> Self {
        Self {
            online,
            provider,
            source,
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: None,
        }
    }
    pub(crate) fn fork(&self) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: Some(self.cancelled.clone()),
            ..self.clone()
        }
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self
                .parent
                .as_ref()
                .is_some_and(|p| p.load(Ordering::Acquire))
    }
    pub(crate) fn validate(&self, core: &AppCore) -> Result<(), String> {
        if self.is_cancelled() {
            return Err("cancelled".into());
        }
        if !core
            .online
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &self.online))
            || !mio_music_sdk::provider(&self.source)
                .is_some_and(|v| Arc::ptr_eq(&v, &self.provider))
        {
            return Err("source_changed: refresh discovery".into());
        }
        Ok(())
    }
    pub(crate) fn check(&self, lease: &runtime::RuntimeLease) -> Result<(), String> {
        let guard = self.clone();
        lease.inspect(move |core| guard.validate(core))?
    }
}
pub(crate) fn sources() -> Result<Vec<DiscoverySource>, String> {
    let ((online, preferred), lease) = runtime::inspect_with_lease(|core| {
        (
            core.online.clone(),
            core.preferences.app.selected_source.clone(),
        )
    })?;
    let mut sources: Vec<_> = online
        .map(|o| o.sources())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| mio_music_sdk::capabilities(&s.id).discovery)
        .map(|s| DiscoverySource {
            id: s.id,
            name: s.name,
        })
        .collect();
    if sources.len() > 32
        || sources
            .iter()
            .any(|s| s.id.len() > 4096 || s.name.len() > 1024)
    {
        return Err("capacity_exceeded: discovery sources".into());
    }
    sources.sort_by_key(|s| s.id != preferred);
    lease.ensure_open()?;
    Ok(sources)
}
pub(crate) struct Session {
    guard: Guard,
    lease: runtime::RuntimeLease,
    category: String,
    charts: bool,
    state: Mutex<Browse>,
    _permit: SnapshotPermit,
}
#[derive(Default)]
struct Browse {
    page: usize,
    cards: Vec<DiscoveryCard>,
    more: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(source: String, category: String, charts: bool) -> Result<Session, String> {
    if source.is_empty()
        || source.len() > 4096
        || category.len() > 128
        || category.chars().any(char::is_control)
    {
        return Err("invalid_input: discovery source or category".into());
    }
    let permit = SnapshotPermit::acquire()?;
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let online = online.ok_or("source_unavailable")?;
    if !online.sources().iter().any(|s| s.id == source) {
        return Err("source_unavailable".into());
    }
    let provider =
        mio_music_sdk::provider_with(&source, |c| c.discovery).ok_or("source_unavailable")?;
    let guard = Guard {
        online,
        provider,
        source,
        cancelled: Arc::new(AtomicBool::new(false)),
        parent: None,
    };
    guard.check(&lease)?;
    Ok(Session {
        guard,
        lease,
        category,
        charts,
        state: Mutex::new(Browse {
            more: true,
            ..Default::default()
        }),
        _permit: permit,
    })
}
fn text(value: &str, max: usize) -> Result<(), String> {
    if value.len() > max || value.contains('\0') {
        return Err("capacity_exceeded: discovery text".into());
    }
    Ok(())
}
pub(crate) fn card_bytes(card: &DiscoveryCard) -> Result<usize, String> {
    text(&card.id, 512)?;
    text(&card.title, 2048)?;
    text(&card.author, 2048)?;
    text(&card.description, 8192)?;
    if card.id.is_empty() {
        return Err("invalid_data: missing playlist ID".into());
    }
    if let Some(cover) = &card.cover {
        text(cover, 4096)?;
    }
    Ok(card.id.len()
        + card.title.len()
        + card.author.len()
        + card.description.len()
        + card.cover.as_ref().map_or(0, String::len))
}
impl Session {
    pub(crate) fn close(&self) {
        self.guard.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn categories(&self) -> Result<DiscoveryCategories, String> {
        let _worker = Worker::acquire()?;
        self.guard.check(&self.lease)?;
        let input = self.guard.provider.discovery_categories()?;
        self.guard.check(&self.lease)?;
        if input.source != self.guard.source || input.groups.len() > 32 || input.hot.len() > 256 {
            return Err("invalid_data: discovery categories".into());
        }
        let mut count = 0;
        let mut bytes = input
            .groups
            .iter()
            .try_fold(0usize, |n, g| text(&g.name, 1024).map(|_| n + g.name.len()))?;
        let mut category =
            |item: mio_music_sdk::DiscoveryCategory| -> Result<DiscoveryCategory, String> {
                count += 1;
                if count > 1000
                    || item.source != self.guard.source
                    || item.id.len() > 128
                    || item.id.chars().any(char::is_control)
                {
                    return Err("invalid_data: discovery category".into());
                }
                text(&item.name, 1024)?;
                bytes += item.id.len() + item.name.len();
                if bytes > 256 * 1024 {
                    return Err("capacity_exceeded: discovery categories".into());
                }
                Ok(DiscoveryCategory {
                    id: item.id,
                    name: item.name,
                })
            };
        let hot = input
            .hot
            .into_iter()
            .map(&mut category)
            .collect::<Result<_, _>>()?;
        let mut groups = Vec::new();
        for group in input.groups {
            text(&group.name, 1024)?;
            let items = group
                .list
                .into_iter()
                .map(&mut category)
                .collect::<Result<_, _>>()?;
            groups.push(DiscoveryGroup {
                name: group.name,
                items,
            });
        }
        self.guard.check(&self.lease)?;
        Ok(DiscoveryCategories { hot, groups })
    }
    pub(crate) fn next(&self) -> Result<DiscoveryPage, String> {
        let _worker = Worker::acquire()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: discovery page running")?;
        self.guard.check(&self.lease)?;
        if !state.more || state.page >= 100_000 {
            return Err("end_of_results".into());
        }
        let page = state.page + 1;
        let (cards, more) = if self.charts {
            let input = self.guard.provider.discovery_charts()?;
            if input.len() > 500 {
                return Err("capacity_exceeded: charts".into());
            }
            let mut cards = Vec::new();
            for item in input {
                if item.source != self.guard.source {
                    return Err("invalid_data: chart source".into());
                }
                cards.push(DiscoveryCard {
                    id: item.id,
                    title: item.name,
                    author: String::new(),
                    description: item.update_frequency,
                    cover: (!item.img.is_empty()).then_some(item.img),
                    song_count: None,
                    play_count: None,
                    chart: true,
                });
            }
            (cards, false)
        } else {
            let input = self
                .guard
                .provider
                .discovery_playlists(&self.category, page)?;
            if input.source != self.guard.source || input.page != page || input.list.len() > 200 {
                return Err("invalid_data: discovery playlist page".into());
            }
            let mut cards = Vec::new();
            for item in input.list {
                if item.source != self.guard.source {
                    return Err("invalid_data: playlist source".into());
                }
                let id = item
                    .id
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.id.as_u64().map(|v| v.to_string()))
                    .ok_or("invalid_data: playlist ID")?;
                cards.push(DiscoveryCard {
                    id,
                    title: item.name,
                    author: item.author,
                    description: item.desc,
                    cover: (!item.img.is_empty()).then_some(item.img),
                    song_count: item.total.as_u64(),
                    play_count: item
                        .play_count
                        .as_u64()
                        .or_else(|| item.play_count.as_str().and_then(|v| v.parse().ok())),
                    chart: false,
                });
            }
            (cards, input.has_more)
        };
        self.guard.check(&self.lease)?;
        let mut seen: HashSet<_> = state.cards.iter().map(|c| c.id.clone()).collect();
        let mut next = state.cards.clone();
        let mut bytes = next
            .iter()
            .try_fold(0usize, |n, c| card_bytes(c).map(|m| n + m))?;
        let mut added = 0;
        for card in cards {
            let size = card_bytes(&card)?;
            if seen.insert(card.id.clone()) {
                bytes += size;
                next.push(card);
                added += 1;
            }
        }
        if next.len() > 2000 || bytes > 4 * 1024 * 1024 {
            return Err("capacity_exceeded: discovery results".into());
        }
        state.cards = next;
        state.page = page;
        state.more = more && added > 0;
        Ok(DiscoveryPage {
            cards: state.cards.clone(),
            has_more: state.more,
        })
    }
    pub(crate) fn open(&self, id: String) -> Result<OnlinePlaylistSession, String> {
        self.guard.check(&self.lease)?;
        let card = self
            .state
            .try_lock()
            .map_err(|_| "busy: discovery page running")?
            .cards
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or("not_found: discovery card")?;
        if !card.chart && !self.guard.provider.capabilities().playlist_detail {
            return Err("unsupported: playlist details".into());
        }
        let kind = if card.chart {
            OnlinePageKind::Chart
        } else {
            OnlinePageKind::Playlist
        };
        PlaylistSession::begin(self.guard.fork(), self.lease.clone(), card.id, kind)
    }
}
#[derive(Default)]
struct Details {
    description: String,
    songs: Vec<Song>,
    pages: Vec<mio_core::online::PreparedSourceSearch>,
    page: usize,
    more: bool,
    total: u32,
    bytes: usize,
    items: usize,
}
pub(crate) struct PlaylistSession {
    guard: Guard,
    lease: runtime::RuntimeLease,
    id: String,
    kind: OnlinePageKind,
    state: Mutex<Details>,
    _permit: SnapshotPermit,
}
impl Drop for PlaylistSession {
    fn drop(&mut self) {
        self.close();
    }
}
impl PlaylistSession {
    pub(crate) fn begin(
        guard: Guard,
        lease: runtime::RuntimeLease,
        id: String,
        kind: OnlinePageKind,
    ) -> Result<OnlinePlaylistSession, String> {
        let permit = SnapshotPermit::acquire()?;
        guard.check(&lease)?;
        Ok(OnlinePlaylistSession {
            session: Self {
                guard,
                lease,
                id,
                kind,
                state: Mutex::new(Details {
                    more: true,
                    ..Default::default()
                }),
                _permit: permit,
            },
        })
    }
    pub(crate) fn close(&self) {
        self.guard.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn next(&self) -> Result<OnlinePlaylistPage, String> {
        let _worker = Worker::acquire()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?;
        self.guard.check(&self.lease)?;
        if !state.more || state.page >= 100_000 {
            return Err("end_of_results".into());
        }
        let page = state.page + 1;
        let prepared = self.guard.online.prepare_online_page(
            &self.guard.provider,
            &self.guard.source,
            &self.id,
            self.kind,
            page,
            &self.guard.cancelled,
        )?;
        let bytes = prepared.metadata_bytes()?;
        if state.bytes + bytes > 8 * 1024 * 1024 || state.items + prepared.item_count() > 2000 {
            return Err("capacity_exceeded: playlist results".into());
        }
        let description = prepared.description().to_owned();
        let retained = prepared.clone();
        self.guard.check(&self.lease)?;
        let mut keep: HashSet<_> = state.songs.iter().map(|s| s.id.clone()).collect();
        let guard = self.guard.clone();
        let fetched = self.lease.inspect(move |core| {
            guard.validate(core)?;
            keep.extend(core.player.queue.iter().map(|s| s.id.clone()));
            keep.extend(core.player.song().map(|s| s.id.clone()));
            guard
                .online
                .publish_source_search(prepared, &keep, &guard.cancelled)
        })??;
        self.guard.check(&self.lease)?;
        let mut seen: HashSet<_> = state.songs.iter().map(|s| s.id.clone()).collect();
        let mut added = 0;
        for song in fetched.songs {
            if seen.insert(song.id.clone()) {
                state.songs.push(song);
                added += 1;
            }
        }
        if !description.is_empty() {
            state.description = description;
        }
        state.page = page;
        state.items += retained.item_count();
        state.pages.push(retained);
        state.bytes += bytes;
        state.more = fetched.has_more && added > 0;
        state.total = fetched.total.min(u32::MAX as usize) as u32;
        let qualities = self.guard.online.source_qualities(&self.guard.source);
        let mut labels = Vec::with_capacity(state.songs.len());
        let mut label_bytes = 0usize;
        for song in &state.songs {
            let selected = state
                .pages
                .iter()
                .find_map(|page| page.selected(&song.id))
                .ok_or("not_found: playlist metadata")?;
            let item = selected
                .into_items()
                .into_iter()
                .next()
                .ok_or("not_found: playlist metadata")?;
            let quality = mio_core::quality::highest_supported(&item, &qualities)
                .map(|q| mio_core::quality::display_key(q).unwrap_or(q).to_owned())
                .unwrap_or_default();
            let source = self.guard.provider.name().to_owned();
            label_bytes += song.id.len() + source.len() + quality.len();
            if label_bytes > 4 * 1024 * 1024 {
                return Err("capacity_exceeded: playlist labels".into());
            }
            labels.push(SearchRowLabel {
                id: song.id.clone(),
                source,
                quality,
            });
        }
        self.guard.check(&self.lease)?;
        Ok(OnlinePlaylistPage {
            labels,
            description: state.description.clone(),
            tracks: state.songs.iter().map(runtime::track).collect(),
            singer_ids: if self.guard.provider.capabilities().singer {
                let sources = [self.guard.source.clone()];
                state
                    .pages
                    .iter()
                    .flat_map(|page| page.singer_ids(&sources))
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            },
            total: state.total,
            has_more: state.more,
        })
    }
    pub(crate) fn open_singer(&self, id: String) -> Result<SingerSession, String> {
        self.guard.check(&self.lease)?;
        let reference = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?
            .pages
            .iter()
            .find_map(|page| page.singer_of(&id))
            .ok_or("singer_unavailable")?;
        if reference.source != self.guard.source || !self.guard.provider.capabilities().singer {
            return Err("singer_unavailable".into());
        }
        crate::singer::begin(
            self.lease.clone(),
            self.guard.online.clone(),
            crate::singer::Reference {
                source: reference.source,
                id: reference.id,
                provider: self.guard.provider.clone(),
            },
            reference.name,
        )
    }
    pub(crate) fn begin_complete_download(&self) -> Result<OnlineDownloadCollection, String> {
        self.guard.check(&self.lease)?;
        crate::collection_download::begin(
            self.guard.fork(),
            self.lease.clone(),
            self.id.clone(),
            self.kind,
        )
    }
    pub(crate) fn prepare_download(&self, id: String) -> Result<DownloadChoice, String> {
        if id.is_empty() || id.len() > 4096 {
            return Err("invalid_input: song ID".into());
        }
        self.guard.check(&self.lease)?;
        // Resolve from the exact retained pages, even after shared-cache eviction.
        let selected = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?
            .pages
            .iter()
            .find_map(|page| page.selected(&id))
            .ok_or("not_found: playlist song")?;
        let mut items = selected.into_items();
        if items.len() != 1 {
            return Err("invalid_data: song metadata".into());
        }
        let item = items.pop().ok_or("not_found: playlist song")?;
        if item.source != self.guard.source {
            return Err("source_changed: song metadata".into());
        }
        let title = item.name.clone();
        let value = serde_json::to_value(item).map_err(|_| "invalid_data: song metadata")?;
        let guard = self.guard.clone();
        crate::downloads::begin_guarded(self.lease.clone(), self.guard.clone(), || {
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                core.prepare_download(guard.online, value)
                    .map(|request| (request, title))
            })?
        })
    }
    pub(crate) fn prepare_selected_save(
        &self,
        ids: Vec<String>,
    ) -> Result<OnlineSaveSession, String> {
        crate::catalog::selection_ids(&ids)?;
        self.guard.check(&self.lease)?;
        let prepared = mio_core::online::PreparedSourceSearch::combined(
            &self
                .state
                .try_lock()
                .map_err(|_| "busy: playlist page running")?
                .pages,
        )?;
        let selected = ids
            .iter()
            .map(|id| {
                prepared
                    .selected(id)
                    .ok_or("not_found: selected song".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let prepared = mio_core::online::PreparedSourceSearch::combined(&selected)?;
        crate::online_save::begin(self.guard.clone(), self.lease.clone(), prepared)
    }
    pub(crate) fn prepare_save(&self) -> Result<OnlineSaveSession, String> {
        self.guard.check(&self.lease)?;
        let prepared = mio_core::online::PreparedSourceSearch::combined(
            &self
                .state
                .try_lock()
                .map_err(|_| "busy: playlist page running")?
                .pages,
        )?;
        crate::online_save::begin(self.guard.clone(), self.lease.clone(), prepared)
    }
    pub(crate) fn prepare_favorite(&self, id: String) -> Result<FavoriteIntent, String> {
        if id.is_empty() || id.len() > 4096 {
            return Err("invalid_input: song ID".into());
        }
        self.guard.check(&self.lease)?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?;
        let song = state
            .songs
            .iter()
            .find(|song| song.id == id)
            .cloned()
            .ok_or("not_found: playlist song")?;
        let selected = state
            .pages
            .iter()
            .find_map(|page| page.selected(&id))
            .ok_or("not_found: playlist metadata")?;
        let item = selected
            .into_items()
            .into_iter()
            .next()
            .ok_or("not_found: playlist metadata")?;
        let row = mio_core::library::PlaylistSong {
            song,
            source: self.guard.source.clone(),
            identity: None,
            quality: None,
            online: Some(serde_json::to_value(item).map_err(|_| "invalid_data: song metadata")?),
        };
        drop(state);
        let context = crate::favorites::Context::Search(self.guard.clone());
        let guard = self.guard.clone();
        crate::favorites::begin(self.lease.clone(), context, || {
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                let target = core
                    .saved_favorite_target(&row)
                    .ok_or("favorite_unavailable: metadata unavailable")?;
                Ok((target, core.preferences.english()))
            })?
        })
    }
    pub(crate) fn play_filtered(&self, query: String, shuffle: bool) -> Result<(), String> {
        self.guard.check(&self.lease)?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?;
        let first = state
            .pages
            .first()
            .ok_or("incomplete_collection: no playlist pages")?;
        let total = first.reported_total();
        let pages = first
            .declared_pages()
            .ok_or("incomplete_collection: missing pagination")?;
        if state.more
            || total == 0
            || total > 2000
            || pages == 0
            || pages > 400
            || state.pages.len() != pages
            || state.page != pages
            || state.songs.len() != total
            || state.total as usize != total
        {
            return Err("incomplete_collection: load all playlist pages before playback".into());
        }
        let mut seen = HashSet::new();
        for (index, page) in state.pages.iter().enumerate() {
            if page.reported_total() != total
                || page.declared_pages() != Some(pages)
                || page.has_more() != (index + 1 < pages)
                || page.item_count() == 0
                || page.source() != self.guard.source
            {
                return Err("incomplete_collection: playlist pagination changed".into());
            }
            for item in page.clone().into_items() {
                let raw = item
                    .songmid
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| item.songmid.as_u64().map(|id| id.to_string()))
                    .ok_or("invalid_data: song ID")?;
                if item.source != self.guard.source
                    || raw.is_empty()
                    || raw.len() > 512
                    || raw.chars().any(char::is_control)
                    || !seen.insert(raw)
                {
                    return Err("incomplete_collection: duplicate or invalid song identity".into());
                }
            }
        }
        let prepared = mio_core::online::PreparedSourceSearch::combined(&state.pages)?;
        if seen.len() != total || prepared.item_count() != total {
            return Err("incomplete_collection: missing playlist songs".into());
        }
        let prepared = prepared.playlist_play(&query, shuffle)?;
        drop(state);
        self.lease
            .execute(runtime::Action::PlayDiscovery(self.guard.clone(), prepared))
    }
    pub(crate) fn play_all(&self) -> Result<(), String> {
        self.guard.check(&self.lease)?;
        let prepared = mio_core::online::PreparedSourceSearch::combined(
            &self
                .state
                .try_lock()
                .map_err(|_| "busy: playlist page running")?
                .pages,
        )?;
        self.lease
            .execute(runtime::Action::PlayDiscovery(self.guard.clone(), prepared))
    }
    pub(crate) fn play(&self, id: String) -> Result<(), String> {
        self.guard.check(&self.lease)?;
        let prepared = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?
            .pages
            .iter()
            .rev()
            .find_map(|page| page.selected(&id))
            .ok_or("not_found: playlist song")?;
        self.lease
            .execute(runtime::Action::PlayDiscovery(self.guard.clone(), prepared))
    }
    pub(crate) fn append(&self, id: String) -> Result<(), String> {
        self.guard.check(&self.lease)?;
        let prepared = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist page running")?
            .pages
            .iter()
            .rev()
            .find_map(|page| page.selected(&id))
            .ok_or("not_found: playlist song")?;
        self.lease.execute(runtime::Action::AppendDiscovery(
            self.guard.clone(),
            prepared,
        ))
    }
}
