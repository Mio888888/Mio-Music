//! Comments are fetched only by the enabled external list plugin.
use crate::MusicItem;
use serde_json::Value;

pub fn get(item: &MusicItem, page: u64, hot: bool, cursor: &str) -> Result<Value, String> {
    if !(1..=10000).contains(&page)
        || cursor.len() > 256
        || !cursor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("无效评论分页参数".into());
    }
    let provider =
        crate::provider_with(&item.source, |c| c.comments).ok_or("该来源的外部插件未提供评论")?;
    get_with(&provider, item, page, hot, cursor)
}

/// Resolve using the same provider captured with the song metadata.
pub fn get_with(
    provider: &std::sync::Arc<dyn crate::MusicListProvider>,
    item: &MusicItem,
    page: u64,
    hot: bool,
    cursor: &str,
) -> Result<Value, String> {
    if !(1..=10000).contains(&page)
        || cursor.len() > 256
        || !cursor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        || provider.id() != item.source
        || !provider.capabilities().comments
    {
        return Err("无效评论来源或分页参数".into());
    }
    let song = serde_json::to_value(item).map_err(|_| "无效歌曲信息")?;
    let result = provider.comments(&song, page, hot, cursor)?;
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= 4 * 1024 * 1024)
                .ok_or_else(|| std::io::Error::other("comments exceed limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), &result).map_err(|_| "插件评论数据过大")?;
    if !result["comments"]
        .as_array()
        .is_some_and(|rows| rows.len() <= 300)
    {
        return Err("插件评论列表无效或过大".into());
    }
    Ok(result)
}

/// HTTP(S) comment media only, without embedded credentials.
pub fn image_url(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 4096 {
        return None;
    }
    let url = reqwest::Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| value.to_owned())
}
