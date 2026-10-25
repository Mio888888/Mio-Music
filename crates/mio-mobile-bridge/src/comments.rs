//! Original-source comments, independently paged hot/latest with bounded admission.
use crate::{
    api::*,
    catalog::SnapshotPermit,
    discovery::{Guard, Worker},
    runtime,
};
use mio_core::online::Online;
use mio_music_sdk::{MusicItem, MusicListProvider};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, atomic::Ordering},
};

#[derive(Default)]
struct Tab {
    page: u64,
    total: u64,
    cursor: String,
    ids: HashSet<String>,
    ended: bool,
}
#[derive(Default)]
struct State {
    tabs: [Tab; 2],
    objects: usize,
    bytes: usize,
}
pub(crate) struct Session {
    guard: Guard,
    lease: runtime::RuntimeLease,
    item: MusicItem,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(
    lease: runtime::RuntimeLease,
    online: Arc<Online>,
    item: MusicItem,
    provider: Arc<dyn MusicListProvider>,
) -> Result<CommentsSession, String> {
    let permit = SnapshotPermit::acquire()?;
    if item.source.len() > 4096 || provider.id() != item.source || !provider.capabilities().comments
    {
        return Err("comments_unavailable".into());
    }
    let guard = Guard::new(online, provider, item.source.clone());
    guard.check(&lease)?;
    Ok(CommentsSession {
        session: Session {
            guard,
            lease,
            item,
            state: Mutex::new(State::default()),
            _permit: permit,
        },
    })
}
fn field(row: &Value, key: &str, max: usize) -> Result<String, String> {
    match &row[key] {
        Value::Null => Ok(String::new()),
        Value::String(s) if s.len() <= max && !s.contains('\0') => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(format!("invalid_data: comment {key}")),
    }
}
fn body(row: &Value, objects: &mut usize, bytes: &mut usize) -> Result<CommentBody, String> {
    *objects += 1;
    if *objects > 2000 || !row.is_object() {
        return Err("capacity_exceeded: comment objects".into());
    }
    let id = field(row, "id", 512)?;
    let name = field(row, "userName", 4096)?;
    let time = field(row, "timeStr", 256)?;
    let text = field(row, "text", 64 * 1024)?;
    let avatar = field(row, "avatar", 4096)?;
    if id.is_empty() {
        return Err("invalid_data: missing comment ID".into());
    }
    let mut images = Vec::new();
    if !row["images"].is_null() {
        let values = row["images"]
            .as_array()
            .filter(|v| v.len() <= 9)
            .ok_or("capacity_exceeded: comment images")?;
        for value in values {
            let value = value
                .as_str()
                .filter(|s| s.len() <= 4096)
                .ok_or("invalid_data: comment image")?;
            *bytes += value.len();
            if let Some(url) = mio_music_sdk::comment_image_url(value) {
                images.push(url);
            }
        }
    }
    *bytes += id.len() + name.len() + time.len() + text.len() + avatar.len();
    if *bytes > 8 * 1024 * 1024 {
        return Err("capacity_exceeded: comment text".into());
    }
    Ok(CommentBody {
        id,
        name,
        time,
        text,
        avatar: mio_music_sdk::comment_image_url(&avatar),
        images,
        liked_count: row["likedCount"]
            .as_u64()
            .or_else(|| row["likedCount"].as_str().and_then(|v| v.parse().ok()))
            .unwrap_or(0),
    })
}
impl Session {
    pub(crate) fn close(&self) {
        self.guard.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn reopen(&self) -> Result<CommentsSession, String> {
        begin(
            self.lease.clone(),
            self.guard.online.clone(),
            self.item.clone(),
            self.guard.provider.clone(),
        )
    }
    pub(crate) fn next(&self, hot: bool) -> Result<CommentsPage, String> {
        let _worker = Worker::acquire()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: comments loading")?;
        self.guard.check(&self.lease)?;
        let index = usize::from(!hot);
        let tab = &state.tabs[index];
        if tab.ended || tab.page >= 10000 {
            return Err("end_of_results".into());
        }
        let page = tab.page + 1;
        let data = mio_music_sdk::get_comments_with(
            &self.guard.provider,
            &self.item,
            page,
            hot,
            &tab.cursor,
        )?;
        self.guard.check(&self.lease)?;
        if data
            .get("source")
            .is_some_and(|v| v.as_str() != Some(&self.guard.source))
        {
            return Err("invalid_data: comment source".into());
        }
        let cursor = field(&data, "cursor", 256)?;
        if !cursor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("invalid_data: comment cursor".into());
        }
        let mut ids = tab.ids.clone();
        let mut objects = state.objects;
        let mut bytes = state.bytes;
        let mut rows = Vec::new();
        for row in data["comments"]
            .as_array()
            .ok_or("invalid_data: comments")?
        {
            let content = body(row, &mut objects, &mut bytes)?;
            let mut replies = Vec::new();
            if !row["reply"].is_null() {
                for reply in row["reply"]
                    .as_array()
                    .filter(|r| r.len() <= 100)
                    .ok_or("capacity_exceeded: comment replies")?
                {
                    replies.push(body(reply, &mut objects, &mut bytes)?);
                }
            }
            if ids.insert(content.id.clone()) {
                rows.push(Comment {
                    body: content,
                    replies,
                });
            }
        }
        let total = data["total"].as_u64().unwrap_or(0);
        let max_page = data["maxPage"].as_u64().unwrap_or(1).min(10000);
        let has_more = !rows.is_empty()
            && page < max_page
            && (ids.len() as u64) < total
            && objects < 2000
            && bytes < 8 * 1024 * 1024
            && (hot
                || data["cursorPaged"].as_bool() != Some(true)
                || (!cursor.is_empty() && cursor != tab.cursor));
        // A failed provider/parse/admission never advances either tab's cursor.
        self.guard.check(&self.lease)?;
        state.tabs[index] = Tab {
            page,
            total,
            cursor,
            ids,
            ended: !has_more,
        };
        state.objects = objects;
        state.bytes = bytes;
        Ok(CommentsPage {
            rows,
            total: state.tabs[index].total,
            has_more,
        })
    }
}
