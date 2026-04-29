use super::types::{PluginInfo, PluginSource};
use std::path::Path;

/// Plugin code validator and metadata extractor.
/// Parses plugin JS code to extract metadata without executing it.
#[derive(Clone)]
pub struct PluginEngine {
    info: PluginInfo,
    sources: Vec<PluginSource>,
    code: String,
}

impl PluginEngine {
    /// Create a new engine by parsing plugin code metadata.
    pub fn new(plugin_code: &str) -> Result<Self, String> {
        let info = Self::extract_plugin_info(plugin_code)?;
        let sources = Self::extract_sources(plugin_code);

        Ok(Self {
            info,
            sources,
            code: plugin_code.to_string(),
        })
    }

    /// Create engine from file path.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let code = std::fs::read_to_string(path)
            .map_err(|e| format!("读取插件文件失败: {}", e))?;
        Self::new(&code)
    }

    pub fn info(&self) -> &PluginInfo { &self.info }
    pub fn sources(&self) -> &[PluginSource] { &self.sources }
    #[allow(dead_code)]
    pub fn code(&self) -> &str { &self.code }

    /// Extract pluginInfo from JS code by parsing the object literal.
    fn extract_plugin_info(code: &str) -> Result<PluginInfo, String> {
        // Look for pluginInfo = { ... } or module.exports = { pluginInfo: { ... } }
        let name = Self::extract_object_field(code, "name")
            .unwrap_or_else(|| "未知插件".to_string());
        let version = Self::extract_object_field(code, "version")
            .unwrap_or_else(|| "1.0.0".to_string());
        let author = Self::extract_object_field(code, "author")
            .unwrap_or_else(|| "未知作者".to_string());
        let description = Self::extract_object_field(code, "description")
            .unwrap_or_default();

        if name == "未知插件" {
            return Err("无法解析插件名称".to_string());
        }

        Ok(PluginInfo { name, version, author, description })
    }

    /// Extract a string field value from JS code.
    fn extract_object_field(code: &str, field: &str) -> Option<String> {
        // Match patterns like: name: "value" or name: 'value' or name: `value`
        let patterns = [
            format!(r#"{}\s*:\s*"([^"]*)""#, field),
            format!(r"{}\s*:\s*'([^']*)'", field),
            format!(r#"{}\s*:\s*`([^`]*)`"#, field),
        ];

        for pattern in &patterns {
            if let Some(caps) = regex_lite::Regex::new(pattern).ok()?.captures(code) {
                if let Some(m) = caps.get(1) {
                    return Some(m.as_str().to_string());
                }
            }
        }
        None
    }

    /// Extract source definitions from JS code.
    fn extract_sources(code: &str) -> Vec<PluginSource> {
        let mut sources = Vec::new();

        // Look for source definitions like: kw: { name: "酷我", qualities: [...] }
        let source_ids = ["kw", "kg", "tx", "wy", "mg", "mgt", "bd", "local"];
        let source_names = ["酷我音乐", "酷狗音乐", "QQ音乐", "网易云音乐", "咪咕音乐", "咪咕音乐", "百度音乐", "本地"];

        for (id, default_name) in source_ids.iter().zip(source_names.iter()) {
            if code.contains(&format!("\"{}\"", id)) || code.contains(&format!("'{}'", id)) {
                let qualities = Self::extract_qualities_for_source(code, id);
                sources.push(PluginSource {
                    name: default_name.to_string(),
                    qualities,
                });
            }
        }

        sources
    }

    /// Extract quality list for a specific source.
    fn extract_qualities_for_source(code: &str, source_id: &str) -> Vec<String> {
        let default_qualities = vec![
            "128k".to_string(), "320k".to_string(), "flac".to_string(),
        ];

        // Try to find quality definitions near the source id
        if let Ok(re) = regex_lite::Regex::new(&format!(
            r#"'{}'\s*:\s*\[([^\]]*)\]"#, source_id
        )) {
            if let Some(caps) = re.captures(code) {
                if let Some(m) = caps.get(1) {
                    let quals: Vec<String> = m.as_str()
                        .split(',')
                        .filter_map(|q| {
                            let q = q.trim().trim_matches(|c| c == '\'' || c == '"');
                            if q.is_empty() { None } else { Some(q.to_string()) }
                        })
                        .collect();
                    if !quals.is_empty() {
                        return quals;
                    }
                }
            }
        }

        default_qualities
    }
}
