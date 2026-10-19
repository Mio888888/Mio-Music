//! Complete, source-bound collection snapshot, independent of UI pages/cache.
use crate::{
    api::*,
    catalog::SnapshotPermit,
    discovery::{Guard, Worker},
    runtime,
};
use mio_core::online::OnlinePageKind;
use std::{
    collections::HashSet,
    sync::{Mutex, atomic::Ordering},
};

pub(crate) struct Session {
    guard: Guard,
    lease: runtime::RuntimeLease,
    id: String,
    kind: OnlinePageKind,
    rows: Mutex<Option<Result<Vec<serde_json::Value>, String>>>,
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
    id: String,
    kind: OnlinePageKind,
) -> Result<OnlineDownloadCollection, String> {
    let permit = SnapshotPermit::acquire()?;
    guard.check(&lease)?;
    Ok(OnlineDownloadCollection {
        session: Session {
            guard,
            lease,
            id,
            kind,
            rows: Mutex::new(None),
            _permit: permit,
        },
    })
}
impl Session {
    pub(crate) fn close(&self) {
        self.guard.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn load(&self) -> Result<u32, String> {
        let mut saved = self.rows.try_lock().map_err(|_| "busy: collection read")?;
        if let Some(rows) = &*saved {
            return rows
                .as_ref()
                .map(|rows| rows.len() as u32)
                .map_err(Clone::clone);
        }
        let _worker = Worker::acquire()?;
        let result = self.read();
        let count = result
            .as_ref()
            .map(|rows| rows.len() as u32)
            .map_err(Clone::clone);
        *saved = Some(result);
        count
    }
    fn read(&self) -> Result<Vec<serde_json::Value>, String> {
        let mut rows = Vec::new();
        let mut seen = HashSet::new();
        let mut expected = None;
        let mut expected_pages = None;
        let mut bytes = 0usize;
        for page in 1..=400 {
            self.guard.check(&self.lease)?;
            let prepared = self
                .guard
                .online
                .prepare_complete_online_page(
                    &self.guard.provider,
                    &self.guard.source,
                    &self.id,
                    self.kind,
                    page,
                    &self.guard.cancelled,
                )
                .map_err(|_| "collection_read_failed: reopen before retry")?;
            self.guard.check(&self.lease)?;
            let total = prepared.reported_total();
            let pages = prepared
                .declared_pages()
                .ok_or("incomplete_collection: missing pagination")?;
            if expected_pages.is_some_and(|old| old != pages) {
                return Err("incomplete_collection: page count changed".into());
            }
            expected_pages = Some(pages);
            if total > 12000 || expected.is_some_and(|old| old != total) {
                return Err("incomplete_collection: total changed or exceeds limit".into());
            }
            expected = Some(total);
            bytes = bytes
                .checked_add(prepared.metadata_bytes()?)
                .filter(|n| *n <= 32 * 1024 * 1024)
                .ok_or("capacity_exceeded: collection metadata")?;
            let more = prepared.has_more();
            for item in prepared.into_items() {
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
                if rows.len() >= 12000 {
                    return Err("capacity_exceeded: collection songs".into());
                }
                rows.push(serde_json::to_value(item).map_err(|_| "invalid_data: song metadata")?);
            }
            if rows.len() > total {
                return Err("incomplete_collection: count exceeds total".into());
            }
            if !more {
                if rows.len() != total {
                    return Err("incomplete_collection: missing songs".into());
                }
                self.guard.check(&self.lease)?;
                return Ok(rows);
            }
        }
        Err("incomplete_collection: page limit".into())
    }
    pub(crate) fn prepare_download(&self, index: u32) -> Result<DownloadChoice, String> {
        self.guard.check(&self.lease)?;
        let value = self
            .rows
            .try_lock()
            .map_err(|_| "busy: collection read")?
            .as_ref()
            .ok_or("not_ready: read collection first")?
            .as_ref()
            .map_err(Clone::clone)?
            .get(index as usize)
            .cloned()
            .ok_or("not_found: collection song")?;
        let title = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let guard = self.guard.clone();
        crate::downloads::begin_guarded(self.lease.clone(), self.guard.clone(), || {
            self.lease.inspect(move |core| {
                guard.validate(core)?;
                core.prepare_download(guard.online, value)
                    .map(|request| (request, title))
            })?
        })
    }
}
