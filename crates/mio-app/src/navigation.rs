#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Discover,
    // The reference route exists without a visible navigation entry.
    #[allow(dead_code)]
    Recent,
    Recognize,
    // Reference route exists, but no visible navigation or login capsule mounts it.
    #[allow(dead_code)]
    Profile,
    Welcome,
    Playlists,
    Local,
    LocalTagEditor,
    Downloads,
    MusicSettings,
    PluginSettings,
    Appearance,
    Search,
    Playlist,
    /// A playlist from an online source; unrelated to Subsonic albums and local playlists.
    OnlinePlaylist,
    /// A singer found through an online song; its songs and albums come from the online SDK.
    Singer,
}
/// What the history entry of a singer page remembers; everything else is fetched on entry.
#[derive(Clone, Debug, PartialEq)]
pub struct SingerRef {
    pub id: String,
    pub source: String,
    /// Name of the song's artist, shown until (and when) the source has no profile.
    pub name: String,
}
/// What the history entry of an online playlist remembers; its songs are fetched on entry.
#[derive(Clone, Debug, PartialEq)]
pub struct OnlinePlaylistRef {
    pub id: String,
    pub source: String,
    pub title: String,
    pub cover: Option<String>,
    pub author: String,
    pub description: String,
    /// The ID is an album's: the singer page routes album cards through the list page.
    pub album: bool,
}
impl Page {
    pub fn is_song_detail(self) -> bool {
        matches!(self, Self::Playlist)
    }
}
enum Entry {
    Page(Page),
    Playlist(crate::library::Playlist),
    OnlinePlaylist(OnlinePlaylistRef),
    Singer(SingerRef),
}
impl Entry {
    fn page(&self) -> Page {
        match self {
            Self::Page(page) => *page,
            Self::Playlist(_) => Page::Playlist,
            Self::OnlinePlaylist(_) => Page::OnlinePlaylist,
            Self::Singer(_) => Page::Singer,
        }
    }
}
pub struct Navigation {
    entries: Vec<Entry>,
    cursor: usize,
}
impl Default for Navigation {
    fn default() -> Self {
        Self::new()
    }
}
impl Navigation {
    pub fn new() -> Self {
        Self {
            entries: vec![Entry::Page(Page::Discover)],
            cursor: 0,
        }
    }
    pub fn current(&self) -> Page {
        self.entries[self.cursor].page()
    }
    pub fn push(&mut self, page: Page) {
        if page == self.current() {
            return;
        }
        self.push_entry(Entry::Page(page));
    }
    pub fn playlist(&self) -> Option<&crate::library::Playlist> {
        match &self.entries[self.cursor] {
            Entry::Playlist(playlist) => Some(playlist),
            _ => None,
        }
    }
    pub fn push_playlist(&mut self, playlist: crate::library::Playlist) {
        if self
            .playlist()
            .is_some_and(|current| current.id == playlist.id)
        {
            self.entries[self.cursor] = Entry::Playlist(playlist);
            return;
        }
        self.push_entry(Entry::Playlist(playlist));
    }
    pub fn online_playlist(&self) -> Option<&OnlinePlaylistRef> {
        match &self.entries[self.cursor] {
            Entry::OnlinePlaylist(playlist) => Some(playlist),
            _ => None,
        }
    }
    pub fn push_online_playlist(&mut self, playlist: OnlinePlaylistRef) {
        if self
            .online_playlist()
            .is_some_and(|current| current == &playlist)
        {
            return;
        }
        self.push_entry(Entry::OnlinePlaylist(playlist));
    }
    pub fn singer(&self) -> Option<&SingerRef> {
        match &self.entries[self.cursor] {
            Entry::Singer(singer) => Some(singer),
            _ => None,
        }
    }
    pub fn push_singer(&mut self, singer: SingerRef) {
        if self.singer().is_some_and(|current| current == &singer) {
            return;
        }
        self.push_entry(Entry::Singer(singer));
    }
    fn push_entry(&mut self, entry: Entry) {
        self.entries.truncate(self.cursor + 1);
        self.entries.push(entry);
        self.cursor += 1;
    }
    pub fn back(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn forward(&mut self) {
        if self.can_forward() {
            self.cursor += 1;
        }
    }
    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_history_discards_forward_branch() {
        let mut nav = Navigation::new();
        nav.back();
        assert_eq!(nav.current(), Page::Discover);
        nav.push(Page::Local);
        nav.push(Page::Downloads);
        nav.back();
        assert_eq!(nav.current(), Page::Local);
        assert!(nav.can_forward());
        nav.push(Page::Playlists);
        assert!(!nav.can_forward());
        nav.back();
        nav.forward();
        assert_eq!(nav.current(), Page::Playlists);
        nav.push(Page::Playlists);
        nav.back();
        assert_eq!(nav.current(), Page::Local);
    }
}
