//! A one-shot save of exact loaded metadata, independent of the shared cache.
use crate::{
    api::*,
    catalog::{SnapshotPermit, playlist_info},
    discovery::{Guard, Worker},
    runtime,
};
use mio_core::{library, online::PreparedSourceSearch};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, PartialEq, Eq)]
struct Form {
    name: String,
    description: String,
    cover: Option<String>,
}
enum Outcome {
    New(Form, Result<PlaylistInfo, String>),
    Append(String, Result<u32, String>),
}
pub(crate) struct Session {
    guards: Vec<Guard>,
    new_source: Option<String>,
    lease: runtime::RuntimeLease,
    path: PathBuf,
    items: Vec<serde_json::Value>,
    closed: Arc<AtomicBool>,
    outcome: Mutex<Option<Outcome>>,
    destinations: Mutex<Option<library::LibraryCursor>>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(
    guard: Guard,
    lease: runtime::RuntimeLease,
    prepared: PreparedSourceSearch,
) -> Result<OnlineSaveSession, String> {
    let permit = SnapshotPermit::acquire()?;
    guard.check(&lease)?;
    if prepared.source() != guard.source
        || prepared.item_count() == 0
        || prepared.item_count() > 2000
    {
        return Err("invalid_input: loaded songs".into());
    }
    let mut items = Vec::with_capacity(prepared.item_count());
    for item in prepared.into_items() {
        let raw = item
            .songmid
            .as_str()
            .map(str::to_owned)
            .or_else(|| item.songmid.as_u64().map(|v| v.to_string()))
            .ok_or("invalid_data: song ID")?;
        if item.source != guard.source
            || raw.is_empty()
            || raw.len() > 512
            || raw.chars().any(char::is_control)
        {
            return Err("invalid_data: song source or ID".into());
        }
        items.push(serde_json::to_value(item).map_err(|_| "invalid_data: song metadata")?);
    }
    let origin = guard.clone();
    let path = lease.inspect(move |core| {
        origin.validate(core)?;
        Ok::<_, String>(core.store.library_path())
    })??;
    Ok(OnlineSaveSession {
        session: Session {
            new_source: Some(guard.source.clone()),
            guards: vec![guard],
            lease,
            path,
            items,
            closed: Arc::new(AtomicBool::new(false)),
            outcome: Mutex::new(None),
            destinations: Mutex::new(None),
            _permit: permit,
        },
    })
}
/// Search selections may contain multiple providers. Retain each original Guard;
/// never derive any row's identity from the currently selected provider.
pub(crate) fn begin_append_selection(
    lease: runtime::RuntimeLease,
    selections: Vec<(Guard, PreparedSourceSearch)>,
) -> Result<OnlineSaveSession, String> {
    if selections.is_empty() || selections.len() > 200 {
        return Err("invalid_input: select 1..200 songs".into());
    }
    let permit = SnapshotPermit::acquire()?;
    let _worker = Worker::acquire()?;
    let mut guards: Vec<Guard> = Vec::new();
    let mut items = Vec::with_capacity(selections.len());
    let mut bytes = 0usize;
    for (guard, prepared) in selections {
        if prepared.source() != guard.source || prepared.item_count() != 1 {
            return Err("invalid_data: selected source".into());
        }
        bytes = bytes
            .checked_add(prepared.metadata_bytes()?)
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or("capacity_exceeded: selected search metadata")?;
        if let Some(previous) = guards
            .iter()
            .find(|previous| previous.source == guard.source)
        {
            if !Arc::ptr_eq(&previous.online, &guard.online)
                || !Arc::ptr_eq(&previous.provider, &guard.provider)
            {
                return Err("source_changed: selection providers differ".into());
            }
        } else {
            if guards.len() >= 32 {
                return Err("capacity_exceeded: selected sources".into());
            }
            guards.push(guard.clone());
        }
        let item = prepared
            .into_items()
            .pop()
            .ok_or("not_found: selected song")?;
        let raw = item
            .songmid
            .as_str()
            .map(str::to_owned)
            .or_else(|| item.songmid.as_u64().map(|id| id.to_string()))
            .ok_or("invalid_data: song ID")?;
        if item.source != guard.source
            || raw.is_empty()
            || raw.len() > 512
            || raw.chars().any(char::is_control)
        {
            return Err("invalid_data: song source or ID".into());
        }
        items.push(serde_json::to_value(item).map_err(|_| "invalid_data: song metadata")?);
    }
    let origins = guards.clone();
    let path = lease.inspect(move |core| {
        origins.iter().try_for_each(|guard| guard.validate(core))?;
        Ok::<_, String>(core.store.library_path())
    })??;
    Ok(OnlineSaveSession {
        session: Session {
            guards,
            new_source: None,
            lease,
            path,
            items,
            closed: Arc::new(AtomicBool::new(false)),
            outcome: Mutex::new(None),
            destinations: Mutex::new(None),
            _permit: permit,
        },
    })
}
impl Session {
    pub(crate) fn count(&self) -> u32 {
        self.items.len() as u32
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<(), String> {
        let guards = self.guards.clone();
        let closed = self.closed.clone();
        self.lease.inspect(move |core| {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            guards.iter().try_for_each(|guard| guard.validate(core))
        })?
    }
    pub(crate) fn destinations(&self, offset: u32, limit: u32) -> Result<PlaylistPage, String> {
        let (start, size) = crate::catalog::page_bounds(offset, limit)?;
        let _worker = Worker::acquire()?;
        self.check()?;
        let mut slot = self
            .destinations
            .lock()
            .map_err(|_| "unavailable: destination cursor")?;
        if slot.is_none() {
            *slot = Some(
                library::LibraryCursor::open_mobile(&self.path)
                    .map_err(crate::catalog::browse_error)?,
            );
        }
        let (rows, total) = slot
            .as_mut()
            .unwrap()
            .playlists(start, size)
            .map_err(crate::catalog::browse_error)?;
        self.check()?;
        Ok(PlaylistPage {
            items: crate::catalog::playlist_items(rows)?,
            offset,
            total: u32::try_from(total).map_err(|_| "capacity_exceeded: destinations")?,
            has_more: start + size < total,
        })
    }
    pub(crate) fn destination_cover(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        crate::catalog::identifier(&id)?;
        let _read = crate::catalog::ReadPermit::acquire()?;
        self.check()?;
        let data = self
            .destinations
            .try_lock()
            .map_err(|_| "busy: destination cover")?
            .as_mut()
            .ok_or("invalid_input: read destinations first")?
            .cover_data(&id)
            .map_err(crate::catalog::browse_error)?;
        let image = data
            .map(|data| mio_core::cover::saved_playlist_thumbnail(&data))
            .transpose()?;
        self.destinations
            .try_lock()
            .map_err(|_| "busy: destination cover")?
            .as_mut()
            .ok_or("invalid_input: read destinations first")?
            .check()
            .map_err(crate::catalog::browse_error)?;
        self.check()?;
        Ok(image)
    }
    pub(crate) fn append(&self, destination_id: String) -> Result<u32, String> {
        crate::catalog::identifier(&destination_id)?;
        if self.items.len() > 200 {
            return Err("invalid_input: select 1..200 songs".into());
        }
        let mut outcome = self.outcome.try_lock().map_err(|_| "busy: saving songs")?;
        if let Some(previous) = outcome.as_ref() {
            return match previous {
                Outcome::Append(id, result) if *id == destination_id => result.clone(),
                _ => Err("session_used: reopen save dialog".into()),
            };
        }
        let _worker = Worker::acquire()?;
        let result = (|| {
            self.check()?;
            let mut slot = self
                .destinations
                .lock()
                .map_err(|_| "unavailable: destination cursor")?;
            let cursor = slot
                .as_mut()
                .ok_or("invalid_input: read destinations first")?;
            let sources: Vec<_> = self
                .guards
                .iter()
                .map(|guard| guard.source.as_str())
                .collect();
            cursor
                .append_online_songs(&destination_id, &sources, &self.items, || {
                    self.check().is_ok()
                })
                .map(|n| n as u32)
                .map_err(crate::catalog::browse_error)
        })();
        *outcome = Some(Outcome::Append(destination_id, result.clone()));
        result
    }
    pub(crate) fn save(
        &self,
        name: String,
        description: String,
        cover: Option<String>,
    ) -> Result<PlaylistInfo, String> {
        let source = self
            .new_source
            .as_deref()
            .ok_or("invalid_input: search selection is append-only")?;
        if name.len() > 4096
            || description.len() > 16384
            || cover
                .as_ref()
                .is_some_and(|v| v.len() > 4096 || v.contains('\0'))
        {
            return Err("invalid_input: playlist form".into());
        }
        let name = name.trim().to_owned();
        let description = description.trim().to_owned();
        if name.is_empty()
            || name.encode_utf16().count() > 50
            || description.encode_utf16().count() > 200
            || name.contains('\0')
            || description.contains('\0')
        {
            return Err("invalid_input: playlist form".into());
        }
        let form = Form {
            name,
            description,
            cover,
        };
        let mut outcome = self
            .outcome
            .try_lock()
            .map_err(|_| "busy: saving loaded songs")?;
        if let Some(previous) = outcome.as_ref() {
            return match previous {
                Outcome::New(previous, result) if previous == &form => result.clone(),
                _ => Err("session_used: reopen save dialog".into()),
            };
        }
        let _worker = Worker::acquire()?;
        self.check()?;
        // The existing import transaction checks cancellation before opening and
        // before commit. Return its actual result; never retry an uncertain write.
        let result = library::save_network_playlist_when(
            &self.path,
            &form.name,
            &form.description,
            source,
            form.cover.as_deref(),
            &self.items,
            || self.check().is_err(),
        )
        .map(|(row, _)| playlist_info(row))
        .map_err(|_| "playlist_save_failed: close and reopen before retry".to_owned());
        *outcome = Some(Outcome::New(form, result.clone()));
        result
    }
}
