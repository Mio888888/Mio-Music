//! The two folders of the original's storage settings (`directory_commands.rs`): where played
//! songs are cached and where downloads go. Kept in `directories.json` as
//! `{cacheDir, downloadDir}`; the defaults are `<data>/cache` and the system Downloads folder.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directories {
    pub cache_dir: String,
    pub download_dir: String,
}

impl Directories {
    /// The defaults (`default_dirs`).
    pub fn defaults(data_root: &Path) -> Self {
        Self {
            cache_dir: data_root.join("cache").to_string_lossy().into_owned(),
            download_dir: crate::download::default_dir()
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// What `directories.json` holds, or the defaults; blank entries fall back too.
    pub fn load(store: Option<&crate::storage::Storage>) -> Self {
        let root = store
            .map(|s| s.root().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let defaults = Self::defaults(&root);
        let saved = store.and_then(|s| s.load::<Directories>("directories.json").ok().flatten());
        match saved {
            Some(saved) => Self {
                cache_dir: if saved.cache_dir.trim().is_empty() {
                    defaults.cache_dir
                } else {
                    saved.cache_dir
                },
                download_dir: if saved.download_dir.trim().is_empty() {
                    defaults.download_dir
                } else {
                    saved.download_dir
                },
            },
            None => defaults,
        }
    }

    /// `save_directories`: written, and both folders are created.
    pub fn save(&self, store: &crate::storage::Storage) -> Result<(), String> {
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.download_dir).map_err(|e| e.to_string())?;
        store
            .save("directories.json", self)
            .map_err(|e| e.to_string())
    }
}
