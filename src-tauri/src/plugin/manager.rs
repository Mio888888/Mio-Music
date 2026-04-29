use super::engine::PluginEngine;
use super::types::LoadedPlugin;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    engines: Arc<RwLock<HashMap<String, PluginEngine>>>,
    plugins_dir: PathBuf,
}

impl PluginManager {
    pub fn new(app_data_dir: &std::path::Path) -> Self {
        let plugins_dir = app_data_dir.join("plugins");
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            engines: Arc::new(RwLock::new(HashMap::new())),
            plugins_dir,
        }
    }

    /// Initialize: load all plugins from the plugins directory.
    pub async fn initialize(&self) -> Result<Vec<LoadedPlugin>, String> {
        fs::create_dir_all(&self.plugins_dir)
            .map_err(|e| format!("创建插件目录失败: {}", e))?;

        let mut plugins = self.plugins.write().await;
        let mut engines = self.engines.write().await;
        plugins.clear();
        engines.clear();

        let entries = fs::read_dir(&self.plugins_dir)
            .map_err(|e| format!("读取插件目录失败: {}", e))?;

        let mut result = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Parse plugin ID from filename (format: {id}-{name})
            let parts: Vec<&str> = file_name.splitn(2, '-').collect();
            if parts.len() < 2 { continue; }
            let plugin_id = parts[0].to_string();
            let plugin_name = parts[1..].join("-");

            match PluginEngine::from_file(&path) {
                Ok(engine) => {
                    let loaded = LoadedPlugin {
                        plugin_id: plugin_id.clone(),
                        plugin_name,
                        plugin_info: engine.info().clone(),
                        supported_sources: engine.sources().to_vec(),
                    };
                    engines.insert(plugin_id.clone(), engine);
                    plugins.insert(plugin_id.clone(), loaded.clone());
                    result.push(loaded);
                }
                Err(e) => {
                    eprintln!("[PluginManager] 加载插件 {} 失败: {}", file_name, e);
                }
            }
        }

        Ok(result)
    }

    /// Add a plugin from code string.
    pub async fn add_plugin(
        &self,
        plugin_code: &str,
        plugin_name: &str,
        target_plugin_id: Option<String>,
    ) -> Result<LoadedPlugin, String> {
        // Validate plugin by parsing it
        let engine = PluginEngine::new(plugin_code)?;
        let info = engine.info().clone();

        // Ensure plugins dir exists
        fs::create_dir_all(&self.plugins_dir)
            .map_err(|e| e.to_string())?;

        // Determine plugin ID
        let plugin_id = if let Some(id) = target_plugin_id {
            id
        } else {
            // Check if same-named plugin exists
            let plugins = self.plugins.read().await;
            if let Some(existing) = plugins.values().find(|p| p.plugin_info.name == info.name) {
                if existing.plugin_info.version == info.version {
                    return Err(format!("插件 \"{} v{}\" 已存在", info.name, info.version));
                }
                existing.plugin_id.clone()
            } else {
                uuid::Uuid::new_v4().to_string().replace('-', "")
            }
        };

        // Remove old plugin file if updating
        self.remove_plugin_file(&plugin_id).await;

        // Write plugin file
        let safe_name = plugin_name.replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
        let file_path = self.plugins_dir.join(format!("{}-{}", plugin_id, safe_name));
        fs::write(&file_path, plugin_code)
            .map_err(|e| format!("写入插件文件失败: {}", e))?;

        // Reload engine
        let engine = PluginEngine::from_file(&file_path)?;
        let loaded = LoadedPlugin {
            plugin_id: plugin_id.clone(),
            plugin_name: safe_name,
            plugin_info: engine.info().clone(),
            supported_sources: engine.sources().to_vec(),
        };

        {
            let mut engines = self.engines.write().await;
            engines.insert(plugin_id.clone(), engine);
        }
        {
            let mut plugins = self.plugins.write().await;
            plugins.insert(plugin_id.clone(), loaded.clone());
        }

        Ok(loaded)
    }

    /// Remove a plugin.
    pub async fn uninstall_plugin(&self, plugin_id: &str) -> Result<(), String> {
        self.remove_plugin_file(plugin_id).await;
        {
            let mut engines = self.engines.write().await;
            engines.remove(plugin_id);
        }
        {
            let mut plugins = self.plugins.write().await;
            plugins.remove(plugin_id);
        }
        Ok(())
    }

    /// Get all loaded plugins info.
    pub async fn get_plugins_list(&self) -> Vec<LoadedPlugin> {
        let plugins = self.plugins.read().await;
        plugins.values().cloned().collect()
    }

    /// Get a specific plugin engine for calling methods.
    #[allow(dead_code)]
    pub fn get_engine(&self, plugin_id: &str) -> Option<PluginEngine> {
        // Synchronous access - we need to block in a tokio context
        // For now, use try_read
        let engines = self.engines.try_read().ok()?;
        engines.get(plugin_id).cloned()
    }

    /// Call a plugin method (placeholder - requires JS engine integration).
    pub fn call_plugin_method(
        &self,
        _plugin_id: &str,
        method: &str,
        _args_json: &str,
    ) -> Result<String, String> {
        Err(format!("插件方法 {} 暂未实现（需要JS引擎集成）", method))
    }

    async fn remove_plugin_file(&self, plugin_id: &str) {
        if let Ok(entries) = fs::read_dir(&self.plugins_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{}-", plugin_id)) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}
