//! Local source preference menus bound to their original owner and providers.
use crate::{
    api::{SourceOption, SourceSelectionSnapshot},
    catalog::SnapshotPermit,
    runtime,
};
use mio_core::{app_core::AppCore, online::Online};
use mio_music_sdk::MusicListProvider;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct Guard {
    lease: runtime::RuntimeLease,
    online: Option<Arc<Online>>,
    source_revision: u64,
    selection_revision: u64,
    closed: Arc<AtomicBool>,
}
impl Guard {
    fn validate(&self, core: &AppCore) -> Result<(), String> {
        self.lease.ensure_open()?;
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if self.lease.source_revision() != self.source_revision
            || !match (&self.online, &core.online) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
        {
            return Err("source_changed: refresh sources".into());
        }
        if core.source_selection_revision() != self.selection_revision {
            return Err("selection_changed: refresh sources".into());
        }
        Ok(())
    }
}
pub(crate) struct Commit {
    guard: Guard,
    source: String,
    provider: Arc<dyn MusicListProvider>,
    quality: Option<(String, bool, std::collections::BTreeMap<String, String>)>,
}
impl Commit {
    pub(crate) fn apply(self, core: &mut AppCore) -> Result<(), String> {
        self.guard.validate(core)?;
        if !mio_music_sdk::provider(&self.source)
            .is_some_and(|provider| Arc::ptr_eq(&provider, &self.provider))
        {
            return Err("source_changed: refresh sources".into());
        }
        if let Some((quality, all_sources, expected)) = self.quality {
            core.select_music_quality(
                self.guard.selection_revision,
                &expected,
                &quality,
                all_sources,
            )
        } else {
            core.select_music_source(self.guard.selection_revision, &self.source)
        }
    }
}
pub(crate) struct Session {
    guard: Guard,
    snapshot: SourceSelectionSnapshot,
    providers: Vec<Arc<dyn MusicListProvider>>,
    quality_map: std::collections::BTreeMap<String, String>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(source_revision: u64, selection_revision: u64) -> Result<Session, String> {
    let permit = SnapshotPermit::acquire()?;
    let ((online, selected, actual_selection, quality_map, selected_quality), lease) =
        runtime::inspect_with_lease(|core| {
            (
                core.online.clone(),
                core.preferences.app.selected_source.clone(),
                core.source_selection_revision(),
                core.preferences.download_quality.clone(),
                core.preferences.app.selected_quality.clone(),
            )
        })?;
    if lease.source_revision() != source_revision || actual_selection != selection_revision {
        return Err("source_changed: refresh sources".into());
    }
    let guard = Guard {
        lease,
        online,
        source_revision,
        selection_revision,
        closed: Arc::new(AtomicBool::new(false)),
    };
    let sources = guard
        .online
        .as_ref()
        .map(|o| o.sources())
        .unwrap_or_default();
    if sources.len() > 32 {
        return Err("capacity_exceeded: sources".into());
    }
    let mut options = Vec::new();
    let mut providers = Vec::new();
    let mut bytes = 0;
    for source in sources {
        if source.id.is_empty()
            || source.id.len() > 4096
            || source.name.len() > 1024
            || source.id.chars().any(char::is_control)
            || source.name.chars().any(char::is_control)
            || source.qualities.len() > 64
            || source
                .qualities
                .iter()
                .any(|q| q.len() > 128 || q.chars().any(char::is_control))
            || options.iter().any(|s: &SourceOption| s.id == source.id)
        {
            return Err("invalid_data: source metadata".into());
        }
        let provider = mio_music_sdk::provider(&source.id).ok_or("source_unavailable")?;
        // Decode/URL validation belongs to the renderer; bound plugin text here.
        let icon = provider
            .icon()
            .filter(|icon| icon.len() <= 350_000 && !icon.chars().any(char::is_control))
            .map(str::to_owned);
        bytes += source.id.len() + source.name.len() + icon.as_ref().map_or(0, String::len);
        if bytes > 2 * 1024 * 1024 {
            return Err("capacity_exceeded: source metadata".into());
        }
        options.push(SourceOption {
            preferred_quality: quality_map
                .get(&source.id)
                .filter(|q| source.qualities.contains(q))
                .or(Some(&selected_quality)
                    .filter(|q| source.id == selected && source.qualities.contains(q)))
                .or_else(|| mio_core::quality::default_quality(&source.qualities))
                .cloned(),
            qualities: source.qualities,
            plugin_name: provider.plugin_name().chars().take(1024).collect(),
            id: source.id,
            name: source.name,
            icon,
        });
        providers.push(provider);
    }
    let check = guard.clone();
    guard.lease.inspect(move |core| check.validate(core))??;
    Ok(Session {
        guard,
        snapshot: SourceSelectionSnapshot {
            sources: options,
            selected,
        },
        providers,
        quality_map,
        _permit: permit,
    })
}
impl Session {
    pub(crate) fn close(&self) {
        self.guard.closed.store(true, Ordering::Release);
    }
    pub(crate) fn snapshot(&self) -> Result<SourceSelectionSnapshot, String> {
        let guard = self.guard.clone();
        self.guard
            .lease
            .inspect(move |core| guard.validate(core))??;
        Ok(self.snapshot.clone())
    }
    pub(crate) fn select(&self, source: String) -> Result<(), String> {
        let index = self
            .snapshot
            .sources
            .iter()
            .position(|s| s.id == source)
            .ok_or("source_unavailable")?;
        self.guard
            .lease
            .execute(runtime::Action::SelectSource(Commit {
                guard: self.guard.clone(),
                source,
                provider: self.providers[index].clone(),
                quality: None,
            }))
    }
    pub(crate) fn select_quality(&self, quality: String, all_sources: bool) -> Result<(), String> {
        let index = self
            .snapshot
            .sources
            .iter()
            .position(|s| s.id == self.snapshot.selected)
            .or_else(|| (!self.snapshot.sources.is_empty()).then_some(0))
            .ok_or("source_unavailable")?;
        let source = &self.snapshot.sources[index];
        if !source.qualities.contains(&quality)
            || (all_sources
                && self
                    .snapshot
                    .sources
                    .iter()
                    .any(|s| !s.qualities.contains(&quality)))
        {
            return Err("invalid_input: unavailable quality".into());
        }
        self.guard
            .lease
            .execute(runtime::Action::SelectSource(Commit {
                guard: self.guard.clone(),
                source: source.id.clone(),
                provider: self.providers[index].clone(),
                quality: Some((quality, all_sources, self.quality_map.clone())),
            }))
    }
}
