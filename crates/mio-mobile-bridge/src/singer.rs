//! Original-provider artist navigation, sharing bounded content sessions.
use crate::{
    api::*,
    catalog::SnapshotPermit,
    discovery::{Guard, PlaylistSession, Worker, card_bytes},
    runtime,
};
use mio_core::{
    online::{Online, OnlinePageKind},
    song::Song,
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, atomic::Ordering},
};

pub(crate) struct Reference {
    pub(crate) source: String,
    pub(crate) id: String,
    pub(crate) provider: Arc<dyn mio_music_sdk::MusicListProvider>,
}
pub(crate) fn reference(online: &Online, song: &Song) -> Result<Option<Reference>, String> {
    let Some((source, id)) = online.bounded_singer_of(&song.id)? else {
        return Ok(None);
    };
    if song.artist.len() > 4096 {
        return Err("capacity_exceeded: singer name".into());
    }
    Ok(
        mio_music_sdk::provider_with(&source, |c| c.singer).map(|provider| Reference {
            source,
            id,
            provider,
        }),
    )
}

pub(crate) fn begin(
    lease: runtime::RuntimeLease,
    online: Arc<Online>,
    reference: Reference,
    name: String,
) -> Result<SingerSession, String> {
    let permit = SnapshotPermit::acquire()?;
    let Reference {
        source,
        id,
        provider,
    } = reference;
    let guard = Guard::new(online, provider, source);
    guard.check(&lease)?;
    Ok(SingerSession {
        session: Session {
            guard,
            lease,
            id,
            name,
            albums: Mutex::new(Albums {
                more: true,
                ..Default::default()
            }),
            _permit: permit,
        },
    })
}
#[derive(Default)]
struct Albums {
    page: usize,
    cards: Vec<DiscoveryCard>,
    more: bool,
    total: u32,
}
pub(crate) struct Session {
    guard: Guard,
    lease: runtime::RuntimeLease,
    id: String,
    name: String,
    albums: Mutex<Albums>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
impl Session {
    /// Explicitly reopen the same artist on the original runtime/provider after UI suspension.
    pub(crate) fn reopen(&self) -> Result<SingerSession, String> {
        begin(
            self.lease.clone(),
            self.guard.online.clone(),
            Reference {
                source: self.guard.source.clone(),
                id: self.id.clone(),
                provider: self.guard.provider.clone(),
            },
            self.name.clone(),
        )
    }
    pub(crate) fn close(&self) {
        self.guard.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn info(&self) -> Result<SingerProfile, String> {
        self.guard.check(&self.lease)?;
        Ok(SingerProfile {
            name: self.name.clone(),
            description: String::new(),
            avatar: None,
            song_count: None,
            album_count: None,
            album_details: self.guard.provider.capabilities().album_detail,
        })
    }
    pub(crate) fn profile(&self) -> Result<SingerProfile, String> {
        let _worker = Worker::acquire()?;
        self.guard.check(&self.lease)?;
        let profile = self.guard.provider.singer_info(&self.id)?;
        self.guard.check(&self.lease)?;
        let Some(profile) = profile else {
            return Ok(SingerProfile {
                name: self.name.clone(),
                description: String::new(),
                avatar: None,
                song_count: None,
                album_count: None,
                album_details: self.guard.provider.capabilities().album_detail,
            });
        };
        if profile.source != self.guard.source
            || profile.id != self.id
            || profile.name.len() > 4096
            || profile.desc.len() > 64 * 1024
            || profile.avatar.len() > 4096
        {
            return Err("invalid_data: singer profile".into());
        }
        Ok(SingerProfile {
            name: if profile.name.is_empty() {
                self.name.clone()
            } else {
                profile.name
            },
            description: profile.desc,
            avatar: (!profile.avatar.is_empty()).then_some(profile.avatar),
            song_count: (profile.music_count >= 0).then_some(profile.music_count as u64),
            album_count: (profile.album_count >= 0).then_some(profile.album_count as u64),
            album_details: self.guard.provider.capabilities().album_detail,
        })
    }
    pub(crate) fn songs(&self) -> Result<OnlinePlaylistSession, String> {
        PlaylistSession::begin(
            self.guard.fork(),
            self.lease.clone(),
            self.id.clone(),
            OnlinePageKind::Singer,
        )
    }
    pub(crate) fn next_albums(&self) -> Result<SingerAlbumsPage, String> {
        let _worker = Worker::acquire()?;
        let mut state = self
            .albums
            .try_lock()
            .map_err(|_| "busy: singer albums loading")?;
        self.guard.check(&self.lease)?;
        if !state.more || state.page >= 100_000 {
            return Err("end_of_results".into());
        }
        let page = state.page + 1;
        let result = self.guard.provider.singer_albums(&self.id, page, 30)?;
        self.guard.check(&self.lease)?;
        if result.source != self.guard.source || result.list.len() > 200 {
            return Err("invalid_data: singer albums".into());
        }
        let more = (page as i64) < result.all_page && !result.list.is_empty();
        let mut seen: HashSet<_> = state.cards.iter().map(|c| c.id.clone()).collect();
        let mut bytes = state
            .cards
            .iter()
            .try_fold(0usize, |n, c| card_bytes(c).map(|s| n + s))?;
        let mut added = Vec::new();
        for album in result.list {
            let card = DiscoveryCard {
                id: album.id,
                title: album.name,
                author: album.author,
                description: album.desc.unwrap_or_default(),
                cover: (!album.img.is_empty()).then_some(album.img),
                song_count: (album.count >= 0).then_some(album.count as u64),
                play_count: None,
                chart: false,
            };
            let size = card_bytes(&card)?;
            if seen.insert(card.id.clone()) {
                bytes += size;
                added.push(card);
            }
            if state.cards.len() + added.len() > 2000 || bytes > 4 * 1024 * 1024 {
                return Err("capacity_exceeded: singer albums".into());
            }
        }
        let progressed = !added.is_empty();
        self.guard.check(&self.lease)?;
        state.cards.extend(added);
        state.page = page;
        state.more = more && progressed;
        state.total = result.total.max(0).min(u32::MAX as i64) as u32;
        Ok(SingerAlbumsPage {
            cards: state.cards.clone(),
            total: state.total,
            has_more: state.more,
        })
    }
    pub(crate) fn open_album(&self, id: String) -> Result<OnlinePlaylistSession, String> {
        self.guard.check(&self.lease)?;
        if !self.guard.provider.capabilities().album_detail {
            return Err("unsupported: album details".into());
        }
        let state = self
            .albums
            .try_lock()
            .map_err(|_| "busy: singer albums loading")?;
        let card = state
            .cards
            .iter()
            .find(|c| c.id == id)
            .ok_or("not_found: singer album")?;
        PlaylistSession::begin(
            self.guard.fork(),
            self.lease.clone(),
            card.id.clone(),
            OnlinePageKind::Album,
        )
    }
}
