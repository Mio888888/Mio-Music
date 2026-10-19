//! Cross-source song search: concurrent fetch, de-duplication and relevance ordering.
//!
//! Ported from the original frontend (`composables/useSearchSongs.ts`, `utils/search/{normalize,
//! similarity,deduplicate}.ts`) and `music_sdk/client.rs::search_music`. String handling follows
//! JavaScript semantics where it is observable: edit distance counts UTF-16 code units, the binary
//! insertion sort places equal scores before the element it lands on, and `Array.sort()` of
//! artist parts compares UTF-16 code units.
use crate::{MusicItem, SearchResult};
use regex::Regex;
use serde_json::Value;
use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

/// Per-source page size of the aggregate view (`AGGREGATE_PAGE_SIZE`).
pub const AGGREGATE_PAGE_SIZE: usize = 20;
const DUPLICATE_DURATION_TOLERANCE_SECONDS: f64 = 3.0;

// ─── normalize ─────────────────────────────────────────────

static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(?:amp|quot|apos|lt|gt);").unwrap());
static NOISE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\p{P}\p{S}\x{FEFF}]+").unwrap());
// `\b` is ASCII-only in JavaScript, so Rust's Unicode word boundary is disabled.
static ARTIST_SEPARATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?iu)\s*(?:[/／\\|、，,;；+＆&×]|\s+x\s+|(?-u:\b)(?:feat|featuring|ft|with)\.?(?-u:\b))\s*",
    )
    .unwrap()
});

fn nfkc_lower(value: &str) -> String {
    let lower = value.to_lowercase();
    icu_normalizer::ComposingNormalizer::new_nfkc()
        .normalize(&lower)
        .into_owned()
}

fn normalize_search_text(value: &str) -> String {
    let lowered = nfkc_lower(value);
    let text = ENTITY.replace_all(&lowered, "");
    NOISE.replace_all(&text, "").into_owned()
}

fn normalize_artist_text(value: &str) -> String {
    let text = ENTITY.replace_all(&nfkc_lower(value), "").into_owned();
    let mut parts: Vec<String> = ARTIST_SEPARATOR
        .split(&text)
        .map(normalize_search_text)
        .filter(|part| !part.is_empty())
        .collect();
    parts.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    parts.join("&")
}

/// `parseDurationSeconds` for string intervals ("mm:ss", "hh:mm:ss", or plain seconds).
fn parse_duration_seconds(interval: &str) -> Option<f64> {
    let text = interval.trim();
    if text.is_empty() {
        return None;
    }
    if text.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return text.parse::<f64>().ok().filter(|v| v.is_finite());
    }
    let parts: Vec<f64> = text
        .split(':')
        .map(|part| {
            let part = part.trim();
            if part.is_empty() {
                Some(0.0) // Number('') === 0
            } else {
                part.parse::<f64>().ok().filter(|v| v.is_finite())
            }
        })
        .collect::<Option<_>>()?;
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    Some(parts.iter().fold(0.0, |total, part| total * 60.0 + part))
}

// ─── similarity ────────────────────────────────────────────

fn levenshtein(a: &[u16], b: &[u16]) -> usize {
    if a.is_empty() || b.is_empty() {
        return a.len().max(b.len());
    }
    let (a, b) = if a.len() > b.len() { (b, a) } else { (a, b) };
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut last = row[0];
        row[0] = last + 1;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let next = (row[j] + 1).min(row[j - 1] + 1).min(last + cost);
            last = row[j];
            row[j] = next;
        }
    }
    row[b.len()]
}

fn similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a: Vec<u16> = a.encode_utf16().collect();
    let b: Vec<u16> = b.encode_utf16().collect();
    let max = a.len().max(b.len());
    if max == 0 {
        return 1.0;
    }
    1.0 - levenshtein(&a, &b) as f64 / max as f64
}

/// `sortBySimilarity`: binary insertion by descending similarity; an equal score is inserted at the
/// probed index, i.e. before the earlier equal element.
fn sort_by_similarity<T>(items: Vec<T>, keyword: &str, text: impl Fn(&T) -> String) -> Vec<T> {
    if keyword.is_empty() || items.len() <= 1 {
        return items;
    }
    let keyword = keyword.to_lowercase();
    let mut sorted: Vec<(f64, T)> = Vec::with_capacity(items.len());
    for item in items {
        let score = similarity(&keyword, &text(&item).to_lowercase());
        let (mut left, mut right) = (0isize, sorted.len() as isize - 1);
        while left <= right {
            let mid = (left + right) / 2;
            let mid_score = sorted[mid as usize].0;
            if score == mid_score {
                left = mid;
                break;
            } else if score < mid_score {
                right = mid - 1;
            } else {
                left = mid + 1;
            }
        }
        sorted.insert(left as usize, (score, item));
    }
    sorted.into_iter().map(|(_, item)| item).collect()
}

// ─── de-duplication ────────────────────────────────────────

struct Candidate {
    song: MusicItem,
    source_order: usize,
    item_order: usize,
    relevance_rank: u8,
    duplicate_key: String,
    album_key: String,
    duration: Option<f64>,
}

/// JavaScript truthiness of a `songmid` value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn js_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn has_stable_identity(song: &MusicItem) -> bool {
    truthy(&song.songmid)
        || song.hash.as_deref().is_some_and(|h| !h.is_empty())
        || song
            .type_url
            .as_ref()
            .is_some_and(|v| v.as_object().is_some_and(|o| !o.is_empty()))
}

fn relevance_rank(song: &MusicItem, query: &str) -> u8 {
    if query.is_empty() {
        return 5;
    }
    let title = normalize_search_text(&song.name);
    let singer = normalize_search_text(&song.singer);
    let album = normalize_search_text(&song.album_name);
    if !title.is_empty() && title == query {
        0
    } else if !title.is_empty() && title.starts_with(query) {
        1
    } else if !title.is_empty() && title.contains(query) {
        2
    } else if !singer.is_empty() && singer.contains(query) {
        3
    } else if !album.is_empty() && album.contains(query) {
        4
    } else {
        5
    }
}

fn candidate(song: MusicItem, query: &str, source_order: usize, item_order: usize) -> Candidate {
    let title = normalize_search_text(&song.name);
    let singer = normalize_artist_text(&song.singer);
    let duplicate_key = if !title.is_empty() && !singer.is_empty() && has_stable_identity(&song) {
        format!("{title}|{singer}")
    } else {
        let identity = if truthy(&song.songmid) {
            js_text(&song.songmid)
        } else {
            song.hash.clone().unwrap_or_default()
        };
        format!(
            "unique:{source_order}:{item_order}:{}:{identity}",
            song.source
        )
    };
    Candidate {
        relevance_rank: relevance_rank(&song, query),
        album_key: normalize_search_text(&song.album_name),
        duration: parse_duration_seconds(&song.interval),
        song,
        source_order,
        item_order,
        duplicate_key,
    }
}

fn compare(a: &Candidate, b: &Candidate) -> Ordering {
    a.relevance_rank
        .cmp(&b.relevance_rank)
        .then(a.source_order.cmp(&b.source_order))
        .then(a.item_order.cmp(&b.item_order))
}

fn likely_duplicate(a: &Candidate, b: &Candidate) -> bool {
    if a.duplicate_key != b.duplicate_key || a.duplicate_key.starts_with("unique:") {
        return false;
    }
    if !a.album_key.is_empty() && !b.album_key.is_empty() && a.album_key != b.album_key {
        return false;
    }
    if let (Some(a), Some(b)) = (a.duration, b.duration) {
        return (a - b).abs() <= DUPLICATE_DURATION_TOLERANCE_SECONDS;
    }
    !a.album_key.is_empty() && !b.album_key.is_empty()
}

/// `dedupeAndSortSongs`. `results` is ordered; the index of a source here is its `sourceOrder`.
/// `query` is the raw keyword (trimmed by the caller).
pub fn dedupe_and_sort_songs(results: &[(String, Vec<MusicItem>)], query: &str) -> Vec<MusicItem> {
    let normalized_query = normalize_search_text(query);
    let mut buckets: Vec<Vec<Candidate>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (source_order, (source, list)) in results.iter().enumerate() {
        for (item_order, song) in list.iter().enumerate() {
            let mut song = song.clone();
            if song.source.is_empty() {
                song.source = source.clone();
            }
            let candidate = candidate(song, &normalized_query, source_order, item_order);
            match index.get(&candidate.duplicate_key) {
                None => {
                    index.insert(candidate.duplicate_key.clone(), buckets.len());
                    buckets.push(vec![candidate]);
                }
                Some(&bucket) => {
                    let bucket = &mut buckets[bucket];
                    match bucket.iter().position(|c| likely_duplicate(c, &candidate)) {
                        None => bucket.push(candidate),
                        Some(at) => {
                            if compare(&candidate, &bucket[at]) == Ordering::Less {
                                bucket[at] = candidate;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut deduped: Vec<Candidate> = buckets.into_iter().flatten().collect();
    deduped.sort_by(compare);

    let mut out = Vec::with_capacity(deduped.len());
    let mut tier: Vec<Candidate> = Vec::new();
    let mut tier_rank = None;
    let flush = |tier: &mut Vec<Candidate>, out: &mut Vec<MusicItem>| {
        let sorted = sort_by_similarity(std::mem::take(tier), query, |c| {
            format!("{} {}", c.song.name, c.song.singer)
        });
        out.extend(sorted.into_iter().map(|c| c.song));
    };
    for candidate in deduped {
        if tier_rank.is_some_and(|rank| rank != candidate.relevance_rank) {
            flush(&mut tier, &mut out);
        }
        tier_rank = Some(candidate.relevance_rank);
        tier.push(candidate);
    }
    flush(&mut tier, &mut out);
    out
}

// ─── paging state ──────────────────────────────────────────

struct SourcePage {
    max_page: i64,
    fetched_page: i64,
}

/// State of one aggregate query: first page from every source, then page N+1 only from sources
/// that are not exhausted (the lx-music pattern the original follows).
pub struct AggregateSearch {
    query: String,
    pages: Vec<(String, SourcePage)>,
    songs: Vec<MusicItem>,
}

impl AggregateSearch {
    /// Searches page 1 of every source concurrently. A failing or empty source is dropped, as in
    /// the original; an error is returned only when *no* source answered.
    pub fn start(query: &str, sources: &[&str]) -> Result<Self, String> {
        let query = query.trim().to_string();
        // The caller's order (the plugins' order) fixes `sourceOrder` of the first page.
        let mut ordered: Vec<&str> = Vec::new();
        for source in sources.iter().copied() {
            if !ordered.contains(&source) && crate::capabilities(source).search {
                ordered.push(source);
            }
        }
        if query.is_empty() || ordered.is_empty() {
            return Err("无效搜索参数".into());
        }
        let outcomes = fetch_pages(&query, ordered.iter().map(|s| (*s, 1)).collect());
        let mut pages = Vec::new();
        let mut results = Vec::new();
        let mut last_error = None;
        for (source, outcome) in ordered.iter().zip(outcomes) {
            match outcome {
                Ok(result) if !result.list.is_empty() => {
                    pages.push((
                        (*source).to_string(),
                        SourcePage {
                            max_page: result.all_page.max(1),
                            fetched_page: 1,
                        },
                    ));
                    results.push(((*source).to_string(), result.list));
                }
                Ok(_) => {}
                Err(error) => last_error = Some(error),
            }
        }
        if results.is_empty()
            && let Some(error) = last_error
        {
            return Err(error);
        }
        let songs = dedupe_and_sort_songs(&results, &query);
        Ok(Self {
            query,
            pages,
            songs,
        })
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn songs(&self) -> &[MusicItem] {
        &self.songs
    }

    pub fn has_more(&self) -> bool {
        self.pages.iter().any(|(_, p)| p.fetched_page < p.max_page)
    }

    /// Fetches the next page of every non-exhausted source, merges with what is displayed and
    /// de-duplicates again. Sources keep the order of their first appearance in the *current
    /// displayed list*, then new sources, which is how the original rebuilds `sourceOrder`.
    pub fn next_page(&mut self) -> Result<&[MusicItem], String> {
        let active: Vec<(String, i64)> = self
            .pages
            .iter()
            .filter(|(_, p)| p.fetched_page < p.max_page)
            .map(|(s, p)| (s.clone(), p.fetched_page + 1))
            .collect();
        if active.is_empty() {
            return Ok(&self.songs);
        }
        let outcomes = fetch_pages(
            &self.query,
            active.iter().map(|(s, p)| (s.as_str(), *p)).collect(),
        );
        let mut fresh: Vec<(String, Vec<MusicItem>)> = Vec::new();
        for ((source, page), outcome) in active.iter().zip(outcomes) {
            let Some((_, info)) = self.pages.iter_mut().find(|(s, _)| s == source) else {
                continue;
            };
            match outcome {
                Ok(result) => {
                    info.fetched_page = *page;
                    info.max_page = result.all_page;
                    if !result.list.is_empty() {
                        fresh.push((source.clone(), result.list));
                    }
                }
                Err(_) => info.max_page = page - 1, // mark exhausted
            }
        }
        if !fresh.is_empty() {
            let mut merged: Vec<(String, Vec<MusicItem>)> = Vec::new();
            let mut push = |source: &str, songs: Vec<MusicItem>| match merged
                .iter_mut()
                .find(|(s, _)| s == source)
            {
                Some((_, list)) => list.extend(songs),
                None => merged.push((source.to_string(), songs)),
            };
            for song in std::mem::take(&mut self.songs) {
                push(&song.source.clone(), vec![song]);
            }
            for (source, list) in fresh {
                push(&source, list);
            }
            self.songs = dedupe_and_sort_songs(&merged, &self.query);
        }
        Ok(&self.songs)
    }
}

fn fetch_pages(query: &str, requests: Vec<(&str, i64)>) -> Vec<Result<SearchResult, String>> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = requests
            .iter()
            .map(|(source, page)| {
                scope.spawn(move || {
                    crate::search_source(source, query, *page as usize, AGGREGATE_PAGE_SIZE)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("在线搜索异常终止".into())))
            .collect()
    })
}

// ─── playlists ─────────────────────────────────────────────

/// Per-source page size of the aggregate playlist view (`AGGREGATE_PLAYLIST_PAGE_SIZE`).
pub const AGGREGATE_PLAYLIST_PAGE_SIZE: usize = 20;

fn value_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// `parsePlaylistSongCount`: a finite, non-negative number or numeric string.
fn known_song_count(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64().filter(|v| v.is_finite() && *v >= 0.0),
        Value::String(s) => {
            let text = s.trim().replace(',', "");
            let numeric = !text.is_empty()
                && text.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                && text.matches('.').count() <= 1
                && !text.starts_with('.')
                && !text.ends_with('.');
            numeric
                .then(|| text.parse::<f64>().ok())
                .flatten()
                .filter(|v| v.is_finite())
        }
        _ => None,
    }
}

fn playlist_rank(item: &crate::PlaylistItem, query: &str) -> u8 {
    if query.is_empty() {
        return 5;
    }
    let title = normalize_search_text(&item.name);
    let author = normalize_search_text(&item.author);
    let description = normalize_search_text(&item.desc);
    if !title.is_empty() && title == query {
        0
    } else if !title.is_empty() && title.starts_with(query) {
        1
    } else if !title.is_empty() && title.contains(query) {
        2
    } else if !author.is_empty() && author.contains(query) {
        3
    } else if !description.is_empty() && description.contains(query) {
        4
    } else {
        5
    }
}

/// `dedupeAndSortPlaylists`: drops playlists known to be empty, keeps the best candidate per
/// `source + id` (or `source + title + author`), orders by (relevance, source order, position).
/// `source_order` is the order sources are listed in; unknown sources use their result position.
pub fn dedupe_and_sort_playlists(
    results: &[(String, Vec<crate::PlaylistItem>)],
    query: &str,
    source_order: &[&str],
) -> Vec<crate::PlaylistItem> {
    struct Cand {
        item: crate::PlaylistItem,
        order: (u8, usize, usize),
    }
    let normalized = normalize_search_text(query);
    let mut best: Vec<(String, Cand)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (result_order, (source, list)) in results.iter().enumerate() {
        let source_rank = source_order
            .iter()
            .position(|s| s == source)
            .unwrap_or(result_order);
        for (item_order, item) in list.iter().enumerate() {
            if known_song_count(&item.total) == Some(0.0) {
                continue;
            }
            let mut item = item.clone();
            if item.source.is_empty() {
                item.source = source.clone();
            }
            let source_key = normalize_search_text(&item.source);
            let id_key = value_text(&item.id).trim().to_string();
            let key = if !source_key.is_empty() && !id_key.is_empty() {
                format!("id:{source_key}:{id_key}")
            } else {
                let title = normalize_search_text(&item.name);
                let author = normalize_search_text(&item.author);
                if !source_key.is_empty() && !title.is_empty() && !author.is_empty() {
                    format!("meta:{source_key}:{title}:{author}")
                } else {
                    format!("unique:{source_rank}:{item_order}")
                }
            };
            let cand = Cand {
                order: (playlist_rank(&item, &normalized), source_rank, item_order),
                item,
            };
            match index.get(&key) {
                Some(&at) => {
                    if cand.order < best[at].1.order {
                        best[at].1 = cand;
                    }
                }
                None => {
                    index.insert(key.clone(), best.len());
                    best.push((key, cand));
                }
            }
        }
    }
    let mut cands: Vec<Cand> = best.into_iter().map(|(_, c)| c).collect();
    cands.sort_by_key(|a| a.order);
    cands.into_iter().map(|c| c.item).collect()
}

/// "All sources" playlist search: page 1 of every listed source concurrently, then de-duplicated.
/// Sources that fail or have no playlist adapter are dropped; an error is returned only when every
/// attempted source failed.
pub fn search_playlists_all(
    query: &str,
    sources: &[&str],
) -> Result<Vec<crate::PlaylistItem>, String> {
    let query = query.trim();
    let attempted: Vec<&str> = sources
        .iter()
        .copied()
        .filter(|s| crate::is_playlist_implemented(s))
        .collect();
    if query.is_empty() || attempted.is_empty() {
        return Ok(Vec::new());
    }
    let outcomes: Vec<Result<crate::PlaylistResult, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = attempted
            .iter()
            .map(|source| {
                scope.spawn(move || {
                    crate::search_playlist_source(source, query, 1, AGGREGATE_PLAYLIST_PAGE_SIZE)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|_| Err("歌单搜索异常终止".into())))
            .collect()
    });
    let mut results = Vec::new();
    let mut last_error = None;
    let mut any_ok = false;
    for (source, outcome) in attempted.iter().zip(outcomes) {
        match outcome {
            Ok(result) => {
                any_ok = true;
                if !result.list.is_empty() {
                    results.push(((*source).to_string(), result.list));
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    if !any_ok && let Some(error) = last_error {
        return Err(error);
    }
    Ok(dedupe_and_sort_playlists(&results, query, sources))
}
