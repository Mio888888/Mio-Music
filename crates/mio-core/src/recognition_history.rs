//! Original recognition-history.json wire format with bounded snapshot writes.
use crate::{recognition::Match, storage::Storage};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
const FILE: &str = "recognition-history.json";
const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_ROWS: usize = 20;

fn raw(root: &Path) -> Result<Option<Vec<u8>>, String> {
    let path = root.join(FILE);
    let info = match fs::symlink_metadata(&path) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("recognition_history_unreadable".into()),
    };
    if !info.is_file() || info.len() > MAX_BYTES as u64 {
        return Err("recognition_history_unreadable".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|_| "recognition_history_unreadable")?
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "recognition_history_unreadable")?;
    if bytes.len() > MAX_BYTES {
        return Err("recognition_history_unreadable".into());
    }
    Ok(Some(bytes))
}

pub struct Snapshot {
    root: PathBuf,
    original: Option<Vec<u8>>,
    rows: Result<Vec<Match>, String>,
}
impl Snapshot {
    pub fn read(store: &Storage) -> Result<Self, String> {
        let original = raw(store.root())?;
        let rows = match &original {
            None => Ok(Vec::new()),
            Some(bytes) => serde_json::from_slice::<Vec<Match>>(bytes)
                .map_err(|_| "recognition_history_invalid".to_owned())
                .and_then(|rows| {
                    validate_rows(&rows)?;
                    Ok(rows)
                }),
        };
        Ok(Self {
            root: store.root().to_owned(),
            original,
            rows,
        })
    }
    pub fn rows(&self) -> Result<&[Match], String> {
        self.rows.as_deref().map_err(Clone::clone)
    }
    pub fn append(
        &mut self,
        store: &Storage,
        incoming: &[Match],
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        validate_rows(incoming)?;
        let mut next = incoming.to_vec();
        next.extend(
            self.rows()?
                .iter()
                .take(MAX_ROWS.saturating_sub(next.len()))
                .cloned(),
        );
        self.write(store, next, cancelled)
    }
    /// Explicit clear may replace a malformed but bounded original snapshot.
    pub fn clear(&mut self, store: &Storage, cancelled: &AtomicBool) -> Result<(), String> {
        self.write(store, Vec::new(), cancelled)
    }
    fn write(
        &mut self,
        store: &Storage,
        rows: Vec<Match>,
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        validate_rows(&rows)?;
        let mut bytes =
            serde_json::to_vec_pretty(&rows).map_err(|_| "recognition_history_invalid")?;
        bytes.push(b'\n');
        if bytes.len() > MAX_BYTES {
            return Err("recognition_history_invalid".into());
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if store.root() != self.root || raw(store.root())? != self.original {
            return Err("recognition_history_changed".into());
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        // Same atomic temp-file replacement as desktop. The original-byte
        // comparison is not an external-process filesystem CAS.
        store
            .save(FILE, &rows)
            .map_err(|_| "recognition_history_not_saved")?;
        self.original = Some(bytes);
        self.rows = Ok(rows);
        Ok(())
    }
}
fn validate_rows(rows: &[Match]) -> Result<(), String> {
    if rows.len() > MAX_ROWS || rows.iter().any(|row| row.validate().is_err()) {
        return Err("recognition_history_invalid".into());
    }
    Ok(())
}
