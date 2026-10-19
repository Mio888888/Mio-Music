//! Network playlist reads use one provider snapshot and never persist partial pages.
use mio_music_sdk::{MusicItem, MusicListProvider};
use std::borrow::Cow;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_PAGES: usize = 400;
const PAGE_SIZE: usize = 30;
const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum Error {
    Input,
    Unavailable,
    Incomplete,
    Cancelled,
}

pub struct Request {
    source: String,
    id: String,
    provider: Arc<dyn MusicListProvider>,
}

impl Request {
    pub fn prepare(source: &str, input: &str) -> Result<Self, Error> {
        let input = input.trim();
        if input.is_empty() || input.len() > 4096 || input.chars().any(char::is_control) {
            return Err(Error::Input);
        }
        let numeric = input.bytes().all(|byte| byte.is_ascii_digit());
        let pattern = match source {
            "wy" => r"(?i)(?:music\.163\.com/.*[?&]id=|playlist\?id=|playlist/|id=)(\d+)",
            "tx" => {
                r"(?i)(?:y\.qq\.com/n/ryqq/playlist/|music\.qq\.com/.*[?&]id=|playlist[?&]id=|[?&]id=)(\d+)"
            }
            "kw" => r"(?i)(?:kuwo\.cn/playlist_detail/|kuwo\.cn/.*[?&]pid=|[?&](?:pid|id)=)(\d+)",
            "mg" => r"(?i)[?&]id=(\d+)",
            "bd" => r"(?i)[?&]playlistId=(\d+)",
            "kg" => "",
            _ => return Err(Error::Input),
        };
        let id = if source == "kg" || numeric {
            input.to_owned()
        } else {
            regex::Regex::new(pattern)
                .ok()
                .and_then(|pattern| {
                    pattern
                        .captures(input)
                        .map(|captures| captures[1].to_owned())
                })
                .ok_or(Error::Input)?
        };
        let provider = mio_music_sdk::provider_with(source, |caps| caps.playlist_detail)
            .ok_or(Error::Unavailable)?;
        Ok(Self {
            source: source.to_owned(),
            id,
            provider,
        })
    }

    /// Same immutable provider used for every page and cover lookup.
    pub fn provider(&self) -> Arc<dyn MusicListProvider> {
        self.provider.clone()
    }

    pub fn read(&self, cancelled: &AtomicBool) -> Result<(Vec<serde_json::Value>, String), Error> {
        let mut items: Vec<MusicItem> = Vec::new();
        let mut seen = HashSet::new();
        let mut expected = None;
        let mut cover = String::new();
        let mut bytes = 0usize;
        let mut complete = false;
        for page in 1..=MAX_PAGES {
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let detail = self
                .provider
                .playlist_detail(&self.id, page, PAGE_SIZE)
                .map_err(|_| Error::Unavailable)?;
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            if detail.total < 0
                || detail.list.len() > MAX_PAGES * PAGE_SIZE
                || expected.is_some_and(|old| detail.total as usize != old)
            {
                return Err(Error::Incomplete);
            }
            if detail.total > 0 {
                let total = usize::try_from(detail.total).map_err(|_| Error::Incomplete)?;
                if total > MAX_PAGES * PAGE_SIZE || expected.is_some_and(|old| old != total) {
                    return Err(Error::Incomplete);
                }
                expected = Some(total);
            }
            if valid_cover(&detail.info.img) {
                cover = detail.info.img
            }
            if detail.list.is_empty() {
                complete = expected.is_none_or(|total| items.len() == total);
                break;
            }
            for item in detail.list {
                let raw = item
                    .songmid
                    .as_str()
                    .map(Cow::Borrowed)
                    .or_else(|| item.songmid.as_u64().map(|raw| Cow::Owned(raw.to_string())))
                    .filter(|raw| !raw.is_empty())
                    .ok_or(Error::Incomplete)?;
                if raw.len() > 512
                    || raw.chars().any(char::is_control)
                    || item.source != self.source
                    || !seen.insert(raw.into_owned())
                    || [&item.name, &item.singer, &item.album_name]
                        .iter()
                        .any(|v| v.len() > 65536 || v.contains('\0'))
                    || item.img.len() > 4096
                    || item.img.contains('\0')
                {
                    return Err(Error::Incomplete);
                }
                bytes = bytes
                    .checked_add(serialized_size(&item, 2 * 1024 * 1024)?)
                    .ok_or(Error::Incomplete)?;
                if bytes > MAX_BYTES || items.len() >= MAX_PAGES * PAGE_SIZE {
                    return Err(Error::Incomplete);
                }
                items.push(item);
            }
            if let Some(total) = expected {
                if items.len() > total {
                    return Err(Error::Incomplete);
                }
                if items.len() == total {
                    complete = true;
                    break;
                }
            }
        }
        if !complete {
            return Err(Error::Incomplete);
        }
        // Cover lookups stay on this same provider snapshot, outside registry locks.
        if self.provider.capabilities().cover {
            for batch in items.chunks_mut(12) {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Error::Cancelled);
                }
                let covers_ok = std::thread::scope(|scope| {
                    let mut handles = Vec::new();
                    for item in batch.iter_mut().filter(|item| item.img.is_empty()) {
                        let provider = &self.provider;
                        handles.push(scope.spawn(move || {
                            if cancelled.load(Ordering::Acquire) {
                                return;
                            }
                            if let Ok(row) = serde_json::to_value(&*item) {
                                if let Ok(url) = provider.cover(&row) {
                                    if valid_cover(&url) {
                                        item.img = url
                                    }
                                }
                            }
                        }));
                    }
                    handles
                        .into_iter()
                        .fold(true, |ok, handle| handle.join().is_ok() && ok)
                });
                if !covers_ok {
                    return Err(Error::Unavailable);
                }
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let songs = items
            .iter()
            .map(|item| serde_json::to_value(item).map_err(|_| Error::Incomplete))
            .collect::<Result<Vec<_>, _>>()?;
        if serialized_size(&songs, MAX_BYTES)? > MAX_BYTES {
            return Err(Error::Incomplete);
        }
        Ok((
            songs,
            if valid_cover(&cover) {
                cover
            } else {
                String::new()
            },
        ))
    }
}

fn valid_cover(url: &str) -> bool {
    url.len() <= 4096
        && reqwest::Url::parse(url).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

// Count encoded bytes without first allocating an unbounded serialization buffer.
fn serialized_size(value: &impl serde::Serialize, limit: usize) -> Result<usize, Error> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| std::io::Error::other("playlist capacity"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|_| Error::Incomplete)?;
    Ok(counter.bytes)
}
