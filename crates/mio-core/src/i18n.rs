use serde_json::Value;
use std::{collections::HashMap, sync::OnceLock};
static ZH: OnceLock<HashMap<String, String>> = OnceLock::new();
static EN: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Every string leaf under its dotted key, so a lookup during rendering is one hash probe.
fn flatten(source: &str) -> HashMap<String, String> {
    fn walk(prefix: &str, value: &Value, out: &mut HashMap<String, String>) {
        match value {
            Value::String(text) => {
                out.insert(prefix.to_owned(), text.clone());
            }
            Value::Object(map) => {
                for (key, value) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, value, out);
                }
            }
            _ => {}
        }
    }
    let root: Value = serde_json::from_str(source).expect("locale");
    let mut out = HashMap::new();
    walk("", &root, &mut out);
    out
}

pub fn text(english: bool, key: &str) -> String {
    let zh = ZH.get_or_init(|| flatten(include_str!("../locales/zh-CN.json")));
    let en = EN.get_or_init(|| flatten(include_str!("../locales/en-US.json")));
    if english { en.get(key) } else { None }
        .or_else(|| zh.get(key))
        .cloned()
        .unwrap_or_else(|| key.into())
}
#[cfg(test)]
mod tests {
    #[test]
    fn source_locales_without_assistant() {
        assert_eq!(
            super::text(false, "music.find.configSubsonic"),
            "配置 Subsonic"
        );
        assert_eq!(
            super::text(true, "music.find.configSubsonic"),
            "Configure Subsonic"
        );
        let zh: serde_json::Value =
            serde_json::from_str(include_str!("../locales/zh-CN.json")).unwrap();
        assert!(zh.get("ai").is_none());
    }
}
