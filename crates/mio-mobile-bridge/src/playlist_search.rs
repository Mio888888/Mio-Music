//! Bounded playlist search over captured providers; details reuse discovery guards.
use crate::{
    api::{
        DiscoveryCard, OnlinePlaylistSession, PlaylistSearchItem, PlaylistSearchPage, SearchSource,
        SearchSourceState,
    },
    catalog::SnapshotPermit,
    discovery::{Guard, PlaylistSession, Worker, card_bytes},
    runtime,
};
use mio_core::online::OnlinePageKind;
use mio_music_sdk::PlaylistItem;
use std::{
    collections::HashSet,
    sync::{Mutex, atomic::Ordering},
};

#[derive(Clone)]
struct Source {
    guard: Guard,
    name: String,
    page: usize,
    total: u32,
    more: bool,
    failed: bool,
}
#[derive(Clone, Default)]
struct State {
    sources: Vec<Source>,
    items: Vec<PlaylistSearchItem>,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    query: String,
    aggregate: bool,
    guards: Vec<Guard>,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn sources() -> Result<Vec<SearchSource>, String> {
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let sources: Vec<_> = online
        .map(|o| o.sources())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            let c = mio_music_sdk::capabilities(&s.id);
            c.search || c.search_playlist
        })
        .map(|s| SearchSource {
            id: s.id,
            name: s.name,
        })
        .collect();
    if sources.len() > 32
        || sources
            .iter()
            .any(|s| s.id.len() > 4096 || s.name.len() > 1024)
    {
        return Err("capacity_exceeded: playlist search sources".into());
    }
    lease.ensure_open()?;
    Ok(sources)
}
pub(crate) fn begin(query: String, source: String) -> Result<Session, String> {
    let query = query.trim().to_owned();
    if query.is_empty()
        || query.len() > 1024
        || query.contains('\0')
        || source.len() > 4096
        || source.contains('\0')
    {
        return Err("invalid_input: playlist search query or source".into());
    }
    let permit = SnapshotPermit::acquire()?;
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let online = online.ok_or("source_unavailable")?;
    let available = online.sources();
    if !source.is_empty() && !available.iter().any(|s| s.id == source) {
        return Err("source_unavailable".into());
    }
    let mut sources = Vec::new();
    for s in available
        .into_iter()
        .filter(|s| source.is_empty() || s.id == source)
    {
        if let Some(provider) = mio_music_sdk::provider_with(&s.id, |c| c.search_playlist) {
            if s.id.len() > 4096 || s.name.len() > 1024 || sources.len() >= 32 {
                return Err("capacity_exceeded: playlist search sources".into());
            }
            sources.push(Source {
                guard: Guard::new(online.clone(), provider, s.id),
                name: s.name,
                page: 0,
                total: 0,
                more: true,
                failed: false,
            });
        }
    }
    if sources.is_empty() {
        return Err("unsupported: playlist search".into());
    }
    let session = Session {
        lease,
        query,
        aggregate: source.is_empty(),
        guards: sources.iter().map(|s| s.guard.clone()).collect(),
        state: Mutex::new(State {
            sources,
            items: Vec::new(),
        }),
        _permit: permit,
    };
    session.check()?;
    Ok(session)
}
fn count(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_str()
            .filter(|s| s.len() <= 32)
            .and_then(|s| s.parse().ok())
    })
}
fn project(item: PlaylistItem, source: &str) -> Result<PlaylistSearchItem, String> {
    if !item.source.is_empty() && item.source != source {
        return Err("invalid_data: playlist source".into());
    }
    let id = item
        .id
        .as_str()
        .map(str::to_owned)
        .or_else(|| item.id.as_u64().map(|v| v.to_string()))
        .ok_or("invalid_data: playlist identifier")?;
    let card = DiscoveryCard {
        id,
        title: item.name,
        author: item.author,
        description: item.desc,
        cover: (!item.img.is_empty()).then_some(item.img),
        song_count: count(&item.total),
        play_count: count(&item.play_count),
        chart: false,
    };
    card_bytes(&card)?;
    Ok(PlaylistSearchItem {
        source: source.to_owned(),
        card,
    })
}
fn bounded(items: &[PlaylistSearchItem]) -> Result<(), String> {
    let bytes = items.iter().try_fold(0usize, |n, item| {
        card_bytes(&item.card).map(|m| n + m + item.source.len())
    })?;
    if items.len() > 2000 || bytes > 4 * 1024 * 1024 {
        return Err("capacity_exceeded: playlist search results".into());
    }
    Ok(())
}
impl Session {
    pub(crate) fn close(&self) {
        for guard in &self.guards {
            guard.cancelled.store(true, Ordering::Release);
        }
    }
    fn check(&self) -> Result<(), String> {
        let guards = self.guards.clone();
        self.lease
            .inspect(move |core| guards.iter().try_for_each(|g| g.validate(core)))?
    }
    pub(crate) fn next(&self, retry_failed: bool) -> Result<PlaylistSearchPage, String> {
        let _worker = Worker::acquire()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist search running")?;
        self.check()?;
        let mut next = state.clone();
        for source in &mut next.sources {
            if if retry_failed {
                !source.failed
            } else {
                source.failed || !source.more
            } {
                continue;
            }
            if source.page >= 100_000 {
                return Err("capacity_exceeded: playlist search pages".into());
            }
            let page = source.page + 1;
            self.check()?;
            let result = source.guard.provider.search_playlist(
                &self.query,
                page,
                if self.aggregate { 20 } else { 30 },
            );
            self.check()?;
            let projected = result.and_then(|result| {
                if (!result.source.is_empty() && result.source != source.guard.source)
                    || result.list.len() > 200
                    || result.total < 0
                {
                    return Err("invalid_data: playlist search page".into());
                }
                let cards = result
                    .list
                    .into_iter()
                    .map(|item| project(item, &source.guard.source))
                    .collect::<Result<Vec<_>, _>>()?;
                bounded(&cards)?;
                Ok((cards, result.total.min(u32::MAX as i64) as u32))
            });
            match projected {
                Err(_) => {
                    source.failed = true;
                }
                Ok((cards, total)) => {
                    let before = next.items.len();
                    let mut seen: HashSet<_> = next
                        .items
                        .iter()
                        .map(|i| (i.source.clone(), i.card.id.clone()))
                        .collect();
                    next.items.extend(
                        cards
                            .into_iter()
                            .filter(|i| seen.insert((i.source.clone(), i.card.id.clone()))),
                    );
                    bounded(&next.items)?;
                    source.page = page;
                    source.total = total;
                    source.failed = false;
                    source.more = !self.aggregate
                        && next.items.len() > before
                        && next.items.len() < total as usize;
                }
            }
        }
        if self.aggregate {
            let grouped: Vec<_> = next
                .sources
                .iter()
                .map(|s| {
                    (
                        s.guard.source.clone(),
                        next.items
                            .iter()
                            .filter(|i| i.source == s.guard.source)
                            .map(|i| PlaylistItem {
                                id: i.card.id.clone().into(),
                                name: i.card.title.clone(),
                                img: i.card.cover.clone().unwrap_or_default(),
                                source: i.source.clone(),
                                desc: i.card.description.clone(),
                                author: i.card.author.clone(),
                                total: i.card.song_count.map(Into::into).unwrap_or_default(),
                                play_count: i.card.play_count.map(Into::into).unwrap_or_default(),
                            })
                            .collect(),
                    )
                })
                .collect();
            let order: Vec<_> = next
                .sources
                .iter()
                .map(|s| s.guard.source.as_str())
                .collect();
            next.items = mio_music_sdk::dedupe_and_sort_playlists(&grouped, &self.query, &order)
                .into_iter()
                .map(|i| {
                    let source = i.source.clone();
                    project(i, &source)
                })
                .collect::<Result<_, _>>()?;
        }
        self.check()?;
        let response = PlaylistSearchPage {
            items: next.items.clone(),
            total: if self.aggregate {
                next.items.len() as u32
            } else {
                next.sources[0].total
            },
            has_more: next.sources.iter().any(|s| s.more && !s.failed),
            sources: next
                .sources
                .iter()
                .map(|s| SearchSourceState {
                    id: s.guard.source.clone(),
                    name: s.name.clone(),
                    total: s.total,
                    has_more: s.more,
                    failed: s.failed,
                })
                .collect(),
        };
        *state = next;
        Ok(response)
    }
    pub(crate) fn open(&self, source: String, id: String) -> Result<OnlinePlaylistSession, String> {
        self.check()?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: playlist search running")?;
        if !state
            .items
            .iter()
            .any(|i| i.source == source && i.card.id == id)
        {
            return Err("not_found: playlist search result".into());
        }
        let guard = self
            .guards
            .iter()
            .find(|g| g.source == source)
            .ok_or("source_unavailable")?;
        if !guard.provider.capabilities().playlist_detail {
            return Err("unsupported: playlist details".into());
        }
        PlaylistSession::begin(
            guard.fork(),
            self.lease.clone(),
            id,
            OnlinePageKind::Playlist,
        )
    }
}
