//! Cover lookup via enabled external list plugins; no built-in source requests.
use crate::MusicItem;
use std::sync::atomic::{AtomicBool, Ordering};

/// Songs looked up at the same time, as in the original's `COVER_BATCH_SIZE`.
const BATCH: usize = 12;

pub fn get(item: &MusicItem) -> Result<String, String> {
    let provider =
        crate::provider_with(&item.source, |c| c.cover).ok_or("该来源的外部插件未提供封面")?;
    get_with(&provider, item)
}

/// Resolve through an already captured provider without consulting the registry.
pub fn get_with(
    provider: &std::sync::Arc<dyn crate::MusicListProvider>,
    item: &MusicItem,
) -> Result<String, String> {
    if provider.id() != item.source || !provider.capabilities().cover {
        return Err("封面来源不可用".into());
    }
    let song = serde_json::to_value(item).map_err(|_| "歌曲信息无效")?;
    let url = provider.cover(&song)?;
    if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
        eprintln!(
            "[cover] source={} resolved={}",
            item.source,
            !url.is_empty()
        );
    }
    if url.is_empty() {
        return Ok(url);
    }
    let parsed = reqwest::Url::parse(&url).map_err(|_| "插件封面地址无效")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("插件封面地址无效".into());
    }
    Ok(url)
}

fn lookup(item: &MusicItem) -> Option<String> {
    get(item).ok().filter(|url| !url.is_empty())
}

/// Fills `img` of songs that have none, a batch at a time. Failures leave the song as it was.
pub fn fill_missing_pics(items: &mut [MusicItem], cancelled: &AtomicBool) {
    let missing: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.img.is_empty() && crate::capabilities(&item.source).cover)
        .map(|(index, _)| index)
        .collect();
    for batch in missing.chunks(BATCH) {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let found: Vec<(usize, Option<String>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|&index| {
                    let item = &items[index];
                    scope.spawn(move || (index, lookup(item)))
                })
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect()
        });
        for (index, url) in found {
            if let Some(url) = url {
                items[index].img = url;
            }
        }
    }
}
