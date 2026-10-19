//! Inactive backup plugin merges, bound to a read-only registry snapshot.
use super::{InstalledPlugin, PLUGIN_COUNT, PLUGIN_GATE, PluginRegistry, Storage};

pub struct BackupPluginSnapshot {
    before: Vec<u8>,
    registry: PluginRegistry,
}
pub struct PreparedBackupPlugins {
    before: Vec<u8>,
    registry: PluginRegistry,
    added: u32,
    skipped: u32,
}
impl BackupPluginSnapshot {
    pub fn check(&self, store: &Storage) -> Result<(), String> {
        let _gate = PLUGIN_GATE
            .lock()
            .map_err(|_| "unavailable: plugin registry")?;
        let current =
            PluginRegistry::read_mode(store, false).map_err(|_| "unavailable: plugin registry")?;
        if serde_json::to_vec(&current).map_err(|_| "invalid_data: plugin registry")? != self.before
        {
            return Err("storage_changed: reopen backup".into());
        }
        Ok(())
    }

    /// Uses the captured registry only, with no migrations or code execution.
    pub fn backup_values(&self) -> Vec<serde_json::Value> {
        self.registry
            .plugins
            .iter()
            .map(|p| serde_json::json!({"id":p.id,"name":p.name,"code":p.code,"config":null}))
            .collect()
    }
    pub fn capture(store: &Storage) -> Result<Self, String> {
        let _gate = PLUGIN_GATE
            .lock()
            .map_err(|_| "unavailable: plugin registry")?;
        let registry =
            PluginRegistry::read_mode(store, false).map_err(|_| "unavailable: plugin registry")?;
        let before = serde_json::to_vec(&registry).map_err(|_| "invalid_data: plugin registry")?;
        Ok(Self { before, registry })
    }

    /// Validation executes imported plugin initialization, off the owner and only
    /// after the user's restore confirmation. No registry save or activation.
    pub fn prepare(
        mut self,
        codes: impl Iterator<Item = impl AsRef<str>>,
        check: &dyn Fn() -> Result<(), String>,
    ) -> Result<PreparedBackupPlugins, String> {
        let mut added = 0;
        let mut skipped = 0;
        for code in codes {
            check()?;
            if added + skipped >= 16 {
                return Err("capacity_exceeded: backup plugins".into());
            }
            let code = code.as_ref();
            if code.is_empty()
                || code.len() > 1024 * 1024
                || self.registry.plugins.iter().any(|p| p.code == code)
                || crate::list_plugins::is_list_plugin(code)
            {
                skipped += 1;
                continue;
            }
            let plugin = mio_plugin::Plugin::load(code.to_owned());
            check()?;
            let Ok(plugin) = plugin else {
                skipped += 1;
                continue;
            };
            if self.registry.plugins.len() >= PLUGIN_COUNT {
                return Err("capacity_exceeded: installed plugins".into());
            }
            // Same validated metadata and generated IDs as the desktop merge,
            // without constructing an Online client or reading its song cache.
            self.registry.plugins.push(InstalledPlugin {
                id: uuid::Uuid::new_v4().to_string(),
                name: plugin.name.clone(),
                version: plugin.version.clone(),
                author: plugin.author.clone(),
                description: plugin.description.clone(),
                sources: plugin.sources.clone(),
                code: code.to_owned(),
                logs: Vec::new(),
            });
            added += 1;
        }
        check()?;
        self.registry
            .validate()
            .map_err(|_| "invalid_data: merged plugin registry")?;
        Ok(PreparedBackupPlugins {
            before: self.before,
            registry: self.registry,
            added,
            skipped,
        })
    }
}
impl PreparedBackupPlugins {
    pub fn check(&self, store: &Storage) -> Result<(), String> {
        let _gate = PLUGIN_GATE
            .lock()
            .map_err(|_| "unavailable: plugin registry")?;
        self.check_locked(store)
    }
    fn check_locked(&self, store: &Storage) -> Result<(), String> {
        let current =
            PluginRegistry::read_mode(store, false).map_err(|_| "unavailable: plugin registry")?;
        if serde_json::to_vec(&current).map_err(|_| "invalid_data: plugin registry")? != self.before
        {
            return Err("storage_changed: reopen backup".into());
        }
        Ok(())
    }
    pub fn commit(self, store: &Storage) -> Result<(u32, u32), String> {
        let _gate = PLUGIN_GATE
            .lock()
            .map_err(|_| "unavailable: plugin registry")?;
        self.check_locked(store)?;
        if self.added > 0 {
            self.registry
                .save(store)
                .map_err(|_| "storage_unavailable: backup plugins could not be saved")?;
        }
        Ok((self.added, self.skipped))
    }
}
