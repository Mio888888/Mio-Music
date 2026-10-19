//! Bounded row intents. Capture identifies the displayed song; later writes
//! stay with its original runtime/source and use absolute favorite state.
use crate::{api::FavoriteIntent, catalog::SavedPlayback, runtime};
use mio_core::app_core::{AppCore, FavoriteTarget};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static HANDLES: AtomicUsize = AtomicUsize::new(0);
static WORKERS: AtomicUsize = AtomicUsize::new(0);
struct Permit(&'static AtomicUsize);
impl Permit {
    fn acquire(counter: &'static AtomicUsize) -> Result<Self, String> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "busy: favorite operations occupied")?;
        Ok(Self(counter))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub(crate) enum Context {
    Local(u64),
    Search(crate::discovery::Guard),
    Saved(SavedPlayback),
}
impl Context {
    pub(crate) fn validate(&self, core: &AppCore) -> Result<(), String> {
        match self {
            Self::Local(revision) if core.local_revision() == *revision => Ok(()),
            Self::Search(guard) => guard.validate(core),
            Self::Saved(context) => context.validate(core),
            _ => Err("catalog_changed: refresh songs".into()),
        }
    }
}

pub(crate) struct Intent {
    target: FavoriteTarget,
    lease: runtime::RuntimeLease,
    context: Context,
    closed: Arc<AtomicBool>,
    operation: Mutex<()>,
    english: bool,
    _permit: Permit,
}
pub(crate) fn begin(
    lease: runtime::RuntimeLease,
    context: Context,
    capture: impl FnOnce() -> Result<(FavoriteTarget, bool), String>,
) -> Result<FavoriteIntent, String> {
    let permit = Permit::acquire(&HANDLES)?;
    lease.ensure_open()?;
    let (target, english) = capture()?;
    lease.ensure_open()?;
    Ok(FavoriteIntent {
        intent: Intent {
            target,
            lease,
            context,
            english,
            closed: Arc::new(AtomicBool::new(false)),
            operation: Mutex::new(()),
            _permit: permit,
        },
    })
}
impl Intent {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn validate(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        let context = self.context.clone();
        let closed = self.closed.clone();
        self.lease.inspect(move |core| {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            context.validate(core)
        })?
    }
    pub(crate) fn state(&self) -> Result<bool, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "busy: favorite operation running")?;
        let _worker = Permit::acquire(&WORKERS)?;
        self.validate()?;
        let liked = self
            .target
            .read_state()
            .map_err(|_| "favorite_unavailable: cannot read favorites")?;
        self.validate()?;
        Ok(liked)
    }
    pub(crate) fn set(&self, liked: bool) -> Result<bool, String> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| "busy: favorite operation running")?;
        let _worker = Permit::acquire(&WORKERS)?;
        self.validate()?;
        // This is write admission. Subsequent close/source changes cannot
        // redirect this immutable target or misreport a committed write.
        self.target
            .save_state(liked, self.english)
            .map_err(|_| "favorite_save_failed: previous favorites retained".into())
    }
}
impl Drop for Intent {
    fn drop(&mut self) {
        self.close();
    }
}
