use crate::{api::*, runtime};
use mio_core::{
    list_plugins,
    online::{Online, PlaybackChange, PreparedPlaybackChange},
    storage::Storage,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

static WRITERS: AtomicUsize = AtomicUsize::new(0);
static READERS: AtomicUsize = AtomicUsize::new(0);
struct Permit(&'static AtomicUsize);
impl Permit {
    fn acquire(counter: &'static AtomicUsize, max: usize) -> Result<Self, String> {
        counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < max).then_some(n + 1)
            })
            .map_err(|_| "busy: plugin operation already running")?;
        Ok(Self(counter))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) enum Prepared {
    Received(
        Box<Prepared>,
        runtime::RuntimeLease,
        u64,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    Playback(PreparedPlaybackChange),
    List(list_plugins::PreparedChange, Arc<Online>),
}
impl Prepared {
    pub(crate) fn commit(self, store: &Storage) -> Result<Arc<Online>, String> {
        match self {
            Self::Received(prepared, lease, revision, closed) => {
                lease.ensure_open()?;
                if closed.load(Ordering::Acquire) {
                    return Err("cancelled".into());
                }
                if lease.source_revision() != revision {
                    return Err("source_changed".into());
                }
                return prepared.commit(store);
            }
            Self::Playback(prepared) => Online::commit_change(store, prepared),
            Self::List(prepared, online) => {
                list_plugins::commit_change(store, prepared)?;
                Ok(online)
            }
        }
        .map_err(|error| {
            if error.starts_with("storage_changed:") {
                "storage_changed: refresh plugin list and retry".into()
            } else {
                "storage_unavailable: plugin change could not be saved".into()
            }
        })
    }
}

fn id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        Err("invalid_input: plugin identifier".into())
    } else {
        Ok(())
    }
}

fn mutate(
    expected_revision: Option<u64>,
    build: impl FnOnce(Storage, Option<Arc<Online>>) -> Result<Prepared, String>,
) -> Result<(), String> {
    let _permit = Permit::acquire(&WRITERS, 1)?;
    let ((store, existing), lease) =
        runtime::inspect_with_lease(|core| (core.store.clone(), core.online.clone()))?;
    if expected_revision.is_some_and(|r| r != lease.source_revision()) {
        return Err("source_changed: reopen configuration and retry".into());
    }
    let prepared = build(store, existing.clone())
        .map_err(|_| "plugin_invalid: validate plugin or configuration and retry")?;
    lease.execute(runtime::Action::Plugins(prepared, existing))
}

pub(crate) fn logs(plugin: String) -> Result<Vec<String>, String> {
    id(&plugin)?;
    let _permit = Permit::acquire(&READERS, 4)?;
    let (store, online) =
        runtime::inspect_with_lease(|core| (core.store.clone(), core.online.clone()))?.0;
    if let Some(online) = online.filter(|online| online.id == plugin) {
        return Ok(online
            .plugin
            .logs()
            .into_iter()
            .map(|line| line.text)
            .collect());
    }
    Online::installed_current(&store)
        .map_err(|_| "storage_unavailable: cannot read plugin logs".into())
        .map(|items| {
            items
                .into_iter()
                .find(|item| item.id == plugin)
                .map(|item| item.logs.into_iter().map(|(_, text)| text).collect())
                .unwrap_or_default()
        })
}

pub(crate) fn installed() -> Result<Vec<PluginInfo>, String> {
    let _permit = Permit::acquire(&READERS, 4)?;
    let ((store, online), lease) =
        runtime::inspect_with_lease(|core| (core.store.clone(), core.online.clone()))?;
    let revision = lease.source_revision();
    let active = online.as_ref().map(|o| o.id.clone()).unwrap_or_default();
    let mut items: Vec<_> = Online::installed_current(&store)
        .map_err(|_| "storage_unavailable: cannot read plugins")?
        .into_iter()
        .map(|p| PluginInfo {
            revision,
            active: p.id == active,
            id: p.id,
            kind: PluginKind::Playback,
            name: p.name,
            version: p.version,
            author: p.author,
            description: p.description,
            sources: p
                .sources
                .into_iter()
                .map(|s| SearchSource {
                    id: s.id,
                    name: s.name,
                })
                .collect(),
            enabled: true,
            can_play: true,
            can_download: true,
            provide_play: true,
            provide_download: true,
            fields: Vec::new(),
            bundled: false,
            configured: true,
        })
        .collect();
    for p in
        list_plugins::installed(&store).map_err(|_| "storage_unavailable: cannot read plugins")?
    {
        let (bundled, configured) = (p.bundled(), p.configured());
        if p.config_schema.len() > 128 {
            return Err("capacity_exceeded: plugin form too large".into());
        }
        let fields = p
            .config_schema
            .into_iter()
            .map(|f| {
                let value = p.config.get(&f.key).unwrap_or(&f.default);
                PluginField {
                    value_json: value.to_string(),
                    key: f.key,
                    label: f.label,
                    kind: f.kind,
                    required: f.required,
                    placeholder: f.placeholder,
                    description: f.description,
                    options: f
                        .options
                        .into_iter()
                        .map(|(label, value)| PluginOption { value, label })
                        .collect(),
                }
            })
            .collect();
        items.push(PluginInfo {
            revision,
            id: p.id,
            kind: PluginKind::List,
            name: p.name,
            version: p.version,
            author: p.author,
            description: p.description,
            sources: p
                .sources
                .into_iter()
                .map(|s| SearchSource {
                    id: s.id,
                    name: s.name,
                })
                .collect(),
            enabled: p.enabled,
            active: false,
            can_play: p.can_play,
            can_download: p.can_download,
            provide_play: p.provide_play,
            provide_download: p.provide_download,
            fields,
            bundled,
            configured,
        });
    }
    let same = lease.inspect(move |core| match (&core.online, &online) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    })?;
    if !same || revision != lease.source_revision() {
        return Err("source_changed: refresh plugin list".into());
    }
    Ok(items)
}

fn prepare_code(
    store: Storage,
    existing: Option<Arc<Online>>,
    code: String,
) -> Result<Prepared, String> {
    if code.is_empty() || code.len() > 1024 * 1024 {
        return Err("invalid_input".into());
    }
    if list_plugins::is_list_plugin(&code) {
        let online = Online::refreshed(store.clone(), existing)?;
        Ok(Prepared::List(
            list_plugins::prepare_change(&store, list_plugins::Change::Install(code))?,
            online,
        ))
    } else {
        Ok(Prepared::Playback(Online::prepare_change(
            store,
            existing,
            PlaybackChange::Install(code),
        )?))
    }
}
pub(crate) fn install_url(url: String) -> Result<(), String> {
    if url.len() > 4096 {
        return Err("invalid_input: plugin URL too long".into());
    }
    mutate(None, |store, existing| {
        prepare_code(store, existing, Online::download_plugin(&url)?)
    })
}
pub(crate) fn install_code(code: String, expected_revision: u64) -> Result<(), String> {
    mutate(Some(expected_revision), |store, existing| {
        prepare_code(store, existing, code)
    })
}
pub(crate) fn activate(plugin: String) -> Result<(), String> {
    id(&plugin)?;
    mutate(None, |store, existing| {
        Ok(Prepared::Playback(Online::prepare_change(
            store,
            existing,
            PlaybackChange::Activate(plugin),
        )?))
    })
}
fn list_change(change: list_plugins::Change) -> Result<(), String> {
    list_change_at(change, None)
}
fn list_change_at(change: list_plugins::Change, expected: Option<u64>) -> Result<(), String> {
    mutate(expected, |store, existing| {
        Ok(Prepared::List(
            list_plugins::prepare_change(&store, change)?,
            Online::refreshed(store, existing)?,
        ))
    })
}
pub(crate) fn enabled(plugin: String, enabled: bool) -> Result<(), String> {
    id(&plugin)?;
    list_change(list_plugins::Change::Enabled(plugin, enabled))
}
pub(crate) fn configure(plugin: String, values: String, revision: u64) -> Result<(), String> {
    id(&plugin)?;
    if values.len() > 64 * 1024 {
        return Err("invalid_input: plugin configuration too large".into());
    }
    let values = serde_json::from_str(&values)
        .map_err(|_| "invalid_input: plugin configuration must be an object")?;
    list_change_at(list_plugins::Change::Config(plugin, values), Some(revision))
}
pub(crate) fn provide(plugin: String, play: bool, download: bool) -> Result<(), String> {
    id(&plugin)?;
    list_change(list_plugins::Change::Provide(plugin, play, download))
}
pub(crate) fn remove(kind: PluginKind, plugin: String) -> Result<(), String> {
    id(&plugin)?;
    match kind {
        PluginKind::List => list_change(list_plugins::Change::Remove(plugin)),
        PluginKind::Playback => mutate(None, |store, existing| {
            Ok(Prepared::Playback(Online::prepare_change(
                store,
                existing,
                PlaybackChange::Remove(plugin),
            )?))
        }),
    }
}

/// LAN reception keeps the captured runtime and the same writer/validation path.
pub(crate) fn install_received(
    lease: runtime::RuntimeLease,
    value: String,
    is_url: bool,
    closed: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    let _permit = Permit::acquire(&WRITERS, 1)?;
    lease.ensure_open()?;
    if closed.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    let revision = lease.source_revision();
    let (store, existing) = lease.inspect(|core| (core.store.clone(), core.online.clone()))?;
    let code = if is_url {
        Online::download_plugin(&value)?
    } else {
        value
    };
    if closed.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    let prepared = prepare_code(store, existing.clone(), code)?;
    lease.execute(runtime::Action::Plugins(
        Prepared::Received(Box::new(prepared), lease.clone(), revision, closed),
        existing,
    ))
}
