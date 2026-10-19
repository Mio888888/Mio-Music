//! Search owns cancellation and pagination, while provider work stays off owner.
use crate::{
    api::{SearchPage, SearchRowLabel, SearchSource, SearchSourceState},
    discovery::Guard,
    runtime,
};
use mio_core::{online::Online, song::Song};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

static HANDLES: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct State {
    sources: Vec<SourceState>,
    songs: Vec<Song>,
    ranking: mio_core::online::SearchRanking,
    displayed_ids: HashSet<String>,
}

struct SourceState {
    source: SearchSource,
    qualities: Vec<String>,
    page: usize,
    total: u32,
    more: bool,
    failed: bool,
}

pub(crate) struct Session {
    online: Arc<Online>,
    lease: runtime::RuntimeLease,
    query: String,
    cancelled: Arc<AtomicBool>,
    guards: HashMap<String, Guard>,
    aggregate: bool,
    state: Mutex<State>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancel();
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) fn sources() -> Result<Vec<SearchSource>, String> {
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let result = online
        .map(|o| {
            o.sources()
                .into_iter()
                .filter(|s| mio_music_sdk::capabilities(&s.id).search)
                .map(|s| SearchSource {
                    id: s.id,
                    name: s.name,
                })
                .collect()
        })
        .unwrap_or_default();
    lease.ensure_open()?;
    Ok(result)
}

pub(crate) fn begin(query: String, source: String) -> Result<Session, String> {
    let query = query.trim().to_owned();
    if query.is_empty() || query.len() > 1024 || (source.len() > 4096 || source.contains('\0')) {
        return Err("invalid_input: search query or source".into());
    }
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let online = online.ok_or("source_unavailable: install and enable a music source plugin")?;
    let sources: Vec<_> = online
        .sources()
        .into_iter()
        .filter(|s| {
            (source.is_empty() || s.id == source) && mio_music_sdk::capabilities(&s.id).search
        })
        .collect();
    if sources.is_empty() {
        return Err("source_unavailable: search source unavailable".into());
    }
    if sources.len() > 32
        || sources.iter().any(|s| {
            s.id.len() > 4096
                || s.name.len() > 1024
                || s.qualities.len() > 32
                || s.qualities
                    .iter()
                    .any(|q| q.len() > 128 || q.contains('\0'))
        })
    {
        return Err("capacity_exceeded: too many or oversized search sources".into());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let guards: HashMap<_, _> = sources
        .iter()
        .map(|source| {
            let provider = mio_music_sdk::provider_with(&source.id, |c| c.search)
                .ok_or("source_unavailable")?;
            let mut guard = Guard::new(online.clone(), provider, source.id.clone());
            guard.cancelled = cancelled.clone();
            Ok((source.id.clone(), guard))
        })
        .collect::<Result<_, String>>()?;
    let admission: Vec<_> = guards.values().cloned().collect();
    lease.inspect(move |core| admission.iter().try_for_each(|guard| guard.validate(core)))??;
    HANDLES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 8).then_some(n + 1)
        })
        .map_err(|_| "busy: too many retained searches")?;
    Ok(Session {
        online,
        lease,
        query,
        cancelled,
        guards,
        aggregate: source.is_empty(),
        state: Mutex::new(State {
            sources: sources
                .into_iter()
                .map(|s| SourceState {
                    qualities: s.qualities,
                    source: SearchSource {
                        id: s.id,
                        name: s.name,
                    },
                    page: 0,
                    total: 0,
                    more: true,
                    failed: false,
                })
                .collect(),
            songs: Vec::new(),
            ranking: Default::default(),
            displayed_ids: HashSet::new(),
        }),
    })
}

impl Session {
    pub(crate) fn prepare_selected_save(
        &self,
        ids: Vec<String>,
    ) -> Result<crate::api::OnlineSaveSession, String> {
        crate::catalog::selection_ids(&ids)?;
        self.check()?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page running")?;
        let mut selections = Vec::with_capacity(ids.len());
        for id in ids {
            if !state.displayed_ids.contains(&id) {
                return Err("not_found: selected search result".into());
            }
            let prepared = state
                .ranking
                .selected(&id)
                .ok_or("not_found: selected search metadata")?;
            let guard = self
                .guards
                .get(prepared.source())
                .ok_or("source_unavailable")?
                .clone();
            selections.push((guard, prepared));
        }
        drop(state);
        crate::online_save::begin_append_selection(self.lease.clone(), selections)
    }
    pub(crate) fn prepare_favorite(
        &self,
        id: String,
    ) -> Result<crate::api::FavoriteIntent, String> {
        if id.is_empty() || id.len() > 4096 {
            return Err("invalid_input: song identifier".into());
        }
        self.check()?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page running")?;
        let prepared = state
            .ranking
            .selected(&id)
            .ok_or("not_found: search song")?;
        let context = crate::favorites::Context::Search(
            self.guards
                .get(prepared.source())
                .ok_or("source_unavailable")?
                .clone(),
        );
        drop(state);
        let guard = context.clone();
        crate::favorites::begin(self.lease.clone(), context, || {
            let song = self
                .state
                .try_lock()
                .map_err(|_| "busy: search page running")?
                .songs
                .iter()
                .find(|song| song.id == id)
                .cloned()
                .ok_or("not_found: search song unavailable")?;
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                let target = core
                    .favorite_target(&song)
                    .ok_or("favorite_unavailable: metadata unavailable")?;
                Ok((target, core.preferences.english()))
            })?
        })
    }
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn check(&self) -> Result<HashSet<String>, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled: search replaced or closed".into());
        }
        let guards: Vec<_> = self.guards.values().cloned().collect();
        let keep = self.lease.inspect(move |core| {
            for guard in guards {
                guard.validate(core)?;
            }
            let mut keep: HashSet<String> =
                core.player.queue.iter().map(|s| s.id.clone()).collect();
            keep.extend(core.player.song().map(|s| s.id.clone()));
            Ok::<_, String>(keep)
        })?;
        let keep = keep.inspect_err(|_| self.cancel())?;
        Ok(keep)
    }

    // Each source keeps its own cursor. Retry only failed pages, while a
    // normal next-page request advances successful sources with more results.
    pub(crate) fn next(&self, retry_failed: bool) -> Result<SearchPage, String> {
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page already running")?;
        self.check()?;
        let targets: Vec<_> = state
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if retry_failed {
                    s.failed
                } else {
                    s.more && !s.failed
                }
            })
            .map(|(index, _)| index)
            .collect();
        if targets.is_empty() {
            return Err("end_of_results".into());
        }
        for index in targets {
            self.check()?;
            let cursor = &state.sources[index];
            let source_guard = self
                .guards
                .get(&cursor.source.id)
                .ok_or("source_unavailable")?
                .clone();
            let prepared = match self.online.prepare_source_search(
                &source_guard.provider,
                &cursor.source.id,
                &self.query,
                cursor.page + 1,
                &self.cancelled,
            ) {
                Ok(page) => page,
                Err(_) => {
                    // Lifecycle/source failures abort the whole obsolete session;
                    // provider failures leave this exact source cursor retryable.
                    self.check()?;
                    state.sources[index].failed = true;
                    continue;
                }
            };
            let ranking = match prepared.ranking() {
                Ok(ranking) => ranking,
                Err(_) => {
                    self.check()?;
                    state.sources[index].failed = true;
                    continue;
                }
            };
            let mut keep = self.check()?;
            keep.extend(state.songs.iter().map(|s| s.id.clone()));
            let expected = self.online.clone();
            let cancelled = self.cancelled.clone();
            let fetched = self.lease.inspect(move |core| {
                source_guard.validate(core)?;
                if !core
                    .online
                    .as_ref()
                    .is_some_and(|o| Arc::ptr_eq(o, &expected))
                {
                    return Err("source_changed: search source changed".into());
                }
                if cancelled.load(Ordering::Acquire) {
                    return Err("cancelled: search closed".into());
                }
                keep.extend(core.player.queue.iter().map(|s| s.id.clone()));
                keep.extend(core.player.song().map(|s| s.id.clone()));
                expected.publish_source_search(prepared, &keep, &cancelled)
            })?;
            let fetched = match fetched {
                Ok(page) => page,
                Err(_) => {
                    self.check()?;
                    state.sources[index].failed = true;
                    continue;
                }
            };
            self.check()?;
            let mut ids: HashSet<_> = state.songs.iter().map(|s| s.id.clone()).collect();
            for song in fetched.songs {
                if ids.insert(song.id.clone()) {
                    state.songs.push(song);
                }
            }
            let bytes = state.songs.iter().try_fold(0usize, |n, s| {
                [&s.id, &s.title, &s.artist, &s.album]
                    .iter()
                    .try_fold(n, |n, text| n.checked_add(text.len()))
                    .and_then(|n| n.checked_add(s.cover_art.as_ref().map_or(0, String::len)))
            });
            if state.songs.len() > 2000 || bytes.is_none_or(|n| n > 8 * 1024 * 1024) {
                self.cancel();
                return Err("capacity_exceeded: search results exceed mobile limit".into());
            }
            state
                .ranking
                .append(ranking)
                .inspect_err(|_| self.cancel())?;
            let cursor = &mut state.sources[index];
            cursor.page += 1;
            cursor.more = fetched.has_more;
            cursor.total = fetched.total.min(u32::MAX as usize) as u32;
            cursor.failed = false;
        }
        self.check()?;
        let displayed = if self.aggregate {
            let sources: Vec<_> = state.sources.iter().map(|s| s.source.id.clone()).collect();
            state.ranking.rank(&self.query, &sources, &state.songs)?
        } else {
            state.songs.clone()
        };
        self.check()?;
        let total = if self.aggregate {
            displayed.len() as u32
        } else {
            state
                .sources
                .iter()
                .fold(0u32, |n, s| n.saturating_add(s.total))
        };
        let singer_sources: Vec<_> = self
            .guards
            .values()
            .filter(|guard| guard.provider.capabilities().singer)
            .map(|guard| guard.source.clone())
            .collect();
        let singer_ids: HashSet<_> = state
            .ranking
            .singer_ids(&singer_sources)
            .into_iter()
            .collect();
        // Labels come from the same immutable ranking metadata as playback,
        // so shared-cache eviction cannot relabel an aggregate representative.
        let mut label_bytes = 0usize;
        let labels = displayed
            .iter()
            .map(|song| {
                let prepared = state
                    .ranking
                    .selected(&song.id)
                    .ok_or("not_found: search metadata")?;
                let source = state
                    .sources
                    .iter()
                    .find(|s| s.source.id == prepared.source())
                    .ok_or("source_unavailable")?;
                let item = prepared
                    .into_items()
                    .into_iter()
                    .next()
                    .ok_or("not_found: search metadata")?;
                let quality = mio_core::quality::highest_supported(&item, &source.qualities)
                    .map(|q| mio_core::quality::display_key(q).unwrap_or(q).to_owned())
                    .unwrap_or_default();
                label_bytes += song.id.len() + source.source.name.len() + quality.len();
                if label_bytes > 4 * 1024 * 1024 {
                    self.cancel();
                    return Err("capacity_exceeded: search row labels".into());
                }
                Ok(SearchRowLabel {
                    id: song.id.clone(),
                    source: source.source.name.clone(),
                    quality,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.check()?;
        state.displayed_ids = displayed.iter().map(|song| song.id.clone()).collect();
        Ok(SearchPage {
            labels,
            singer_ids: displayed
                .iter()
                .filter(|song| singer_ids.contains(&song.id))
                .map(|song| song.id.clone())
                .collect(),
            items: displayed.iter().map(runtime::track).collect(),
            total,
            has_more: state.sources.iter().any(|s| s.more && !s.failed),
            sources: state
                .sources
                .iter()
                .map(|s| SearchSourceState {
                    id: s.source.id.clone(),
                    name: s.source.name.clone(),
                    failed: s.failed,
                    has_more: s.more,
                    total: s.total,
                })
                .collect(),
        })
    }

    pub(crate) fn open_singer(&self, id: String) -> Result<crate::api::SingerSession, String> {
        self.check()?;
        let state = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page running")?;
        let reference = state.ranking.singer_of(&id).ok_or("singer_unavailable")?;
        let guard = self
            .guards
            .get(&reference.source)
            .ok_or("source_unavailable")?;
        if !guard.provider.capabilities().singer {
            return Err("singer_unavailable".into());
        }
        crate::singer::begin(
            self.lease.clone(),
            self.online.clone(),
            crate::singer::Reference {
                source: reference.source,
                id: reference.id,
                provider: guard.provider.clone(),
            },
            reference.name,
        )
    }
    pub(crate) fn play(&self, id: String) -> Result<(), String> {
        self.check()?;
        let prepared = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page running")?
            .ranking
            .selected(&id)
            .ok_or("not_found: search song")?;
        let guard = self
            .guards
            .get(prepared.source())
            .ok_or("source_unavailable")?
            .clone();
        self.lease
            .execute(runtime::Action::PlayDiscovery(guard, prepared))
    }
    pub(crate) fn append(&self, id: String) -> Result<(), String> {
        self.check()?;
        let prepared = self
            .state
            .try_lock()
            .map_err(|_| "busy: search page running")?
            .ranking
            .selected(&id)
            .ok_or("not_found: search song")?;
        let guard = self
            .guards
            .get(prepared.source())
            .ok_or("source_unavailable")?
            .clone();
        self.lease
            .execute(runtime::Action::AppendDiscovery(guard, prepared))
    }
}
