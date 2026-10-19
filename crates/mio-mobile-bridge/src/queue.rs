//! Original-owner, revision-bound queue controls.
use crate::runtime;
use mio_core::{
    app_core::{AppCore, QueueEdit},
    online::Online,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
static HANDLES: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone)]
pub(crate) struct Guard {
    revision: u64,
    local: u64,
    online: Option<Arc<Online>>,
    closed: Arc<AtomicBool>,
}
impl Guard {
    pub(crate) fn capture(core: &AppCore, revision: u64) -> Result<Self, String> {
        let guard = Self {
            revision,
            local: core.local_revision(),
            online: core.online.clone(),
            closed: Arc::new(AtomicBool::new(false)),
        };
        guard.validate(core)?;
        Ok(guard)
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    pub(crate) fn online(&self) -> Option<&Arc<Online>> {
        self.online.as_ref()
    }
    pub(crate) fn validate(&self, core: &AppCore) -> Result<(), String> {
        if self.cancelled() {
            return Err("cancelled".into());
        }
        let same = match (&self.online, &core.online) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same
            || core.player.queue_revision != self.revision
            || core.local_revision() != self.local
        {
            return Err("queue_changed: reopen queue".into());
        }
        Ok(())
    }
    pub(crate) fn preflight(&self, core: &AppCore, op: &Operation) -> Result<(), String> {
        self.validate(core)?;
        if let Operation::Play(index, id) = op {
            if core.player.queue.get(*index).map(|s| s.id.as_str()) != Some(id.as_str()) {
                return Err("queue_changed: song differs".into());
            }
        }
        Ok(())
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}
pub(crate) enum Operation {
    Play(usize, String),
    Edit(QueueEdit),
    Mode,
}
pub(crate) struct Session {
    guard: Guard,
    lease: runtime::RuntimeLease,
    reader: Mutex<()>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) fn begin(revision: u64, source: u64) -> Result<Session, String> {
    HANDLES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 4).then_some(n + 1)
        })
        .map_err(|_| "busy: too many queue sessions")?;
    let result = (|| {
        let (guard, lease) = runtime::inspect_with_lease(move |core| {
            let guard = Guard {
                revision,
                local: core.local_revision(),
                online: core.online.clone(),
                closed: Arc::new(AtomicBool::new(false)),
            };
            guard.validate(core)?;
            Ok::<_, String>(guard)
        })?;
        if lease.source_revision() != source {
            return Err("queue_changed: reopen queue".into());
        }
        Ok(Session {
            guard: guard?,
            lease,
            reader: Mutex::new(()),
        })
    })();
    if result.is_err() {
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
    result
}
impl Session {
    pub(crate) fn page(&self, offset: u32) -> Result<crate::api::QueuePage, String> {
        let _reader = self.reader.try_lock().map_err(|_| "busy: queue reader")?;
        let guard = self.guard.clone();
        self.lease.inspect(move |core| {
            guard.validate(core)?;
            let total = u32::try_from(core.player.queue.len())
                .map_err(|_| "capacity_exceeded: queue count")?;
            if offset > total || offset % 64 != 0 {
                return Err("invalid_input: queue page offset".into());
            }
            let mut bytes = 0usize;
            let mut items = Vec::with_capacity(64);
            for song in core.player.queue.iter().skip(offset as usize).take(64) {
                for text in [
                    &song.id,
                    &song.title,
                    &song.artist,
                    &song.album,
                    song.cover_art.as_deref().unwrap_or(""),
                ] {
                    bytes = bytes
                        .checked_add(text.len())
                        .filter(|n| *n <= 256 * 1024)
                        .ok_or("capacity_exceeded: queue page text")?;
                }
                items.push(runtime::track(song));
            }
            Ok(crate::api::QueuePage {
                offset,
                total,
                items,
            })
        })?
    }
    pub(crate) fn locate(&self, id: String) -> Result<Option<u32>, String> {
        valid_id(&id)?;
        let guard = self.guard.clone();
        self.lease.inspect(move |core| {
            guard.validate(core)?;
            core.player
                .queue
                .iter()
                .position(|song| song.id == id)
                .map(|index| {
                    u32::try_from(index).map_err(|_| "capacity_exceeded: queue index".into())
                })
                .transpose()
        })?
    }
    pub(crate) fn recent(&self) -> Result<crate::api::RecentPage, String> {
        let guard = self.guard.clone();
        self.lease.inspect(move |core| {
            guard.validate(core)?;
            let total = u32::try_from(core.player.queue.len())
                .map_err(|_| "capacity_exceeded: queue count")?;
            let mut bytes = 0usize;
            let mut items = Vec::with_capacity(core.player.queue.len().min(200));
            for (index, song) in core.player.queue.iter().enumerate().rev().take(200) {
                for text in [
                    &song.id,
                    &song.title,
                    &song.artist,
                    &song.album,
                    song.cover_art.as_deref().unwrap_or(""),
                ] {
                    bytes = bytes
                        .checked_add(text.len())
                        .filter(|n| *n <= 2 * 1024 * 1024)
                        .ok_or("capacity_exceeded: recent text")?;
                }
                items.push(crate::api::RecentTrack {
                    index: index as u32,
                    track: runtime::track(song),
                });
            }
            Ok(crate::api::RecentPage { items, total })
        })?
    }
    pub(crate) fn close(&self) {
        self.guard.closed.store(true, Ordering::Release);
    }
    fn execute(&self, op: Operation) -> Result<(), String> {
        if self.guard.cancelled() {
            return Err("cancelled".into());
        }
        self.lease
            .execute(runtime::Action::Queue(self.guard.clone(), op))
    }
    pub(crate) fn play(&self, index: u32, id: String) -> Result<(), String> {
        valid_id(&id)?;
        self.execute(Operation::Play(index as usize, id))
    }
    pub(crate) fn remove(&self, id: String) -> Result<(), String> {
        valid_id(&id)?;
        self.execute(Operation::Edit(QueueEdit::Remove(id)))
    }
    pub(crate) fn move_song(&self, from: u32, boundary: u32) -> Result<(), String> {
        self.execute(Operation::Edit(QueueEdit::Move {
            from: from as usize,
            boundary: boundary as usize,
        }))
    }
    pub(crate) fn clear(&self) -> Result<(), String> {
        self.execute(Operation::Edit(QueueEdit::Clear))
    }
    pub(crate) fn mode(&self) -> Result<(), String> {
        self.execute(Operation::Mode)
    }
}
fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 4096 || id.contains('\0') {
        return Err("invalid_input: queue song".into());
    }
    Ok(())
}
pub(crate) fn apply(core: &mut AppCore, guard: Guard, op: Operation) -> Result<(), String> {
    guard.preflight(core, &op)?;
    match op {
        Operation::Play(index, id) => {
            if core.player.queue.get(index).map(|s| s.id.as_str()) != Some(id.as_str()) {
                return Err("queue_changed: song differs".into());
            }
            core.play_queue_index(index, || guard.cancelled())
        }
        Operation::Edit(edit) => core.edit_queue(edit),
        Operation::Mode => core.cycle_play_mode(),
    }
}
