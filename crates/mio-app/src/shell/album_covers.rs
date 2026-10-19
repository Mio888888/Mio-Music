use super::*;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub(super) struct AlbumCovers {
    revision: u64,
    queue: VecDeque<String>,
    pub images: HashMap<String, Option<Arc<RenderImage>>>,
    pub retired: Vec<Arc<RenderImage>>,
}

impl AlbumCovers {
    pub fn replace(&mut self, albums: &[Album]) {
        self.replace_ids(albums.iter().filter_map(|album| album.cover_art.clone()));
    }

    pub fn replace_ids(&mut self, ids: impl IntoIterator<Item = String>) {
        self.revision += 1;
        self.retired
            .extend(self.images.drain().filter_map(|(_, image)| image));
        self.queue.clear();
        // Bound distinct decoded covers even for an oversized server response.
        for id in ids {
            if self.queue.len() == 64 {
                break;
            }
            if !self.queue.contains(&id) {
                self.queue.push_back(id);
            }
        }
        // Keep busy until an old request completes, including across source changes.
    }
}

impl Mio {
    /// Covers of the Subsonic library were fetched here through its client; that client is gone
    /// (Subsonic is a list plugin now, whose songs carry directly loadable cover URLs).
    pub(super) fn pump_album_covers(&mut self, _cx: &mut Context<Self>) {}
}
