//! Playlist tab of the search page: one source pages 30 at a time, "all sources" shows one
//! de-duplicated page. Mirrors `useSearchPlaylists.ts` and the grid of `views/music/search.vue`.
use super::*;
use crate::online::PlaylistCard;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct PlaylistSearch {
    pub results: Remote<PlaylistCard>,
    pub total: usize,
    pub page: usize,
    pub has_more: bool,
    pub loading_more: bool,
    pub more_error: Option<String>,
    pub scroll: ScrollHandle,
    /// Query, source and plugin the current results belong to.
    key: Option<(String, Option<String>, usize)>,
    revision: u64,
    cancel: Option<Arc<AtomicBool>>,
}

impl PlaylistSearch {
    pub fn new() -> Self {
        Self {
            results: Remote::Idle,
            total: 0,
            page: 0,
            has_more: false,
            loading_more: false,
            more_error: None,
            scroll: ScrollHandle::new(),
            key: None,
            revision: 0,
            cancel: None,
        }
    }
    fn reset(&mut self) {
        self.revision += 1;
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.results = Remote::Idle;
        self.total = 0;
        self.page = 0;
        self.has_more = false;
        self.loading_more = false;
        self.more_error = None;
        self.key = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
    }
}

impl Mio {
    /// A new query or source: drop results, and fetch now only if the playlist tab is showing
    /// (the original searches the active tab and fetches the other when it is opened).
    pub(super) fn restart_playlist_search(&mut self, cx: &mut Context<Self>) {
        let key = self.playlist_search_key();
        let state = &self.song_view.playlists;
        // Returning to the page with the same query, source and plugin keeps what was loaded.
        let unchanged =
            state.key.as_ref() == Some(&key) && !matches!(state.results, Remote::Failed(_));
        if !unchanged {
            self.song_view.playlists.reset();
        }
        if self.song_view.search_playlists {
            self.ensure_playlist_search(cx);
        }
        cx.notify();
    }

    fn playlist_search_key(&self) -> (String, Option<String>, usize) {
        (
            self.song_view.search_query.trim().to_string(),
            self.song_view.search_source.clone(),
            self.online
                .as_ref()
                .map_or(0, |online| Arc::as_ptr(online) as usize),
        )
    }

    /// Fetch the playlist tab on first view of a query.
    pub(super) fn ensure_playlist_search(&mut self, cx: &mut Context<Self>) {
        if matches!(self.song_view.playlists.results, Remote::Idle) {
            self.load_playlist_search(false, cx);
        }
    }

    /// The selected source, if this plugin still offers it and the SDK can search its playlists.
    fn playlist_source_state(&self) -> Result<Option<String>, &'static str> {
        match &self.song_view.search_source {
            None => Ok(None),
            Some(source) if mio_music_sdk::is_playlist_implemented(source) => {
                Ok(Some(source.clone()))
            }
            Some(_) => Err("该来源的歌单搜索尚未接入"),
        }
    }

    pub(super) fn load_playlist_search(&mut self, more: bool, cx: &mut Context<Self>) {
        if !self.song_view.search_online || self.online_busy {
            return;
        }
        let state = &self.song_view.playlists;
        if more && (state.loading_more || !state.has_more || state.more_error.is_some()) {
            return;
        }
        if !more && matches!(state.results, Remote::Loading) {
            return;
        }
        let Some(online) = self.online.clone() else {
            return;
        };
        let query = self.song_view.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let Ok(source) = self.playlist_source_state() else {
            return;
        };
        let page = if more {
            self.song_view.playlists.page + 1
        } else {
            1
        };
        let key = self.playlist_search_key();
        let state = &mut self.song_view.playlists;
        if !more {
            state.reset();
            state.key = Some(key);
            state.results = Remote::Loading;
        } else {
            state.loading_more = true;
            state.more_error = None;
        }
        let revision = state.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel = Some(cancel.clone());
        let task = cx
            .background_executor()
            .spawn(async move { online.search_playlists(source.as_deref(), &query, page, cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.song_view.playlists;
                if state.revision != revision {
                    return;
                }
                state.cancel = None;
                state.loading_more = false;
                match result {
                    Ok(found) => {
                        let single = this.song_view.search_source.is_some();
                        let state = &mut this.song_view.playlists;
                        state.total = found.total;
                        if more {
                            if let Remote::Ready(cards) = &mut state.results {
                                let before = cards.len();
                                let mut seen: std::collections::HashSet<(String, String)> = cards
                                    .iter()
                                    .map(|c| (c.source.clone(), c.id.clone()))
                                    .collect();
                                cards.extend(
                                    found
                                        .cards
                                        .into_iter()
                                        .filter(|c| seen.insert((c.source.clone(), c.id.clone()))),
                                );
                                // A page that added nothing ends paging instead of looping.
                                state.has_more =
                                    single && cards.len() > before && cards.len() < state.total;
                                state.page = page;
                            }
                        } else {
                            state.has_more = single
                                && !found.cards.is_empty()
                                && found.cards.len() < state.total;
                            state.page = 1;
                            state.results = Remote::Ready(found.cards);
                        }
                    }
                    Err(error) => {
                        if more {
                            state.more_error = Some(error);
                        } else {
                            state.results = Remote::Failed(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Content of the playlist tab (online mode). `width`/`rem` size the responsive grid like
    /// CSS `repeat(auto-fill, minmax(180px, 1fr))` with a 12px gap inside 8px of padding.
    pub(super) fn playlist_search_content(
        &self,
        narrow: bool,
        width: f32,
        rem: f32,
        mode: crate::skin::Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = &self.song_view.playlists;
        let single = self.song_view.search_source.is_some();
        if let Err(message) = self.playlist_source_state() {
            return self
                .search_message(&self.t("music.search.playlistsUnavailable"), message, mode)
                .into_any_element();
        }
        match &state.results {
            Remote::Idle | Remote::Loading => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .text_size(px(14.))
                .text_color(color(mode, "--search-loading-text"))
                .child(
                    gpui_kit::component::spinner::Spinner::new()
                        .color(color(mode, "--td-brand-color")),
                )
                .child(self.t(if single {
                    "music.search.searchingPlaylists"
                } else {
                    "music.search.searchingAggregatePlaylists"
                }))
                .into_any_element(),
            Remote::Failed(error) => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(self.search_message(&self.t("music.search.searchFailed"), error, mode))
                .child(
                    Button::new("playlist-search-retry")
                        .label(self.t("common.retry"))
                        .on_click(cx.listener(|this, _, _, cx| this.restart_playlist_search(cx))),
                )
                .into_any_element(),
            Remote::Ready(cards) if cards.is_empty() => self
                .search_message(
                    &self.t("music.search.noPlaylistResults"),
                    &self.t("music.search.tryOther"),
                    mode,
                )
                .into_any_element(),
            Remote::Ready(cards) => {
                let gap = 12.;
                let padding = if narrow { 0. } else { 8. };
                let page_padding = if narrow {
                    (width * 0.04).clamp(16., 24.)
                } else {
                    20.
                };
                let sidebar = if narrow { 0. } else { 15. * rem };
                let content = (width - sidebar - 2. * page_padding - 2. * padding).max(1.);
                let columns = if narrow {
                    2
                } else {
                    ((content + gap) / (180. + gap)).floor().max(1.) as u16
                };
                let card_width = (content - gap * (columns - 1) as f32) / columns as f32;
                let mut grid = div()
                    .id("playlist-search-grid")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&state.scroll)
                    .p(px(padding))
                    .on_scroll_wheel(cx.listener(|_, _, window, cx| {
                        cx.defer_in(window, |this, _, cx| {
                            let handle = this.song_view.playlists.scroll.clone();
                            let bottom = -handle.offset().y + handle.bounds().size.height;
                            let full = handle.max_offset().y + handle.bounds().size.height;
                            if bottom >= full - px(100.) {
                                this.load_playlist_search(true, cx);
                            }
                        });
                    }))
                    .child(div().grid().grid_cols(columns).gap(px(gap)).children(
                        cards.iter().map(|card| {
                            self.playlist_card(card, card_width, mode, cx)
                                .into_any_element()
                        }),
                    ));
                if state.loading_more {
                    grid = grid.child(
                        div()
                            .h(px(44.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(gpui_kit::component::spinner::Spinner::new()),
                    );
                } else if let Some(error) = &state.more_error {
                    grid = grid.child(
                        Button::new("playlist-page-retry")
                            .label(self.t("music.search.pageRetry").replace("{error}", error))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.song_view.playlists.more_error = None;
                                this.load_playlist_search(true, cx);
                            })),
                    );
                }
                grid.into_any_element()
            }
        }
    }

    pub(super) fn playlist_card(
        &self,
        card: &PlaylistCard,
        width: f32,
        mode: crate::skin::Colors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = card.clone();
        let cover = card
            .cover
            .clone()
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        let id: SharedString = format!("playlist-card-{}-{}", card.source, card.id).into();
        let description = if card.description.is_empty() {
            self.t("music.search.featuredPlaylist")
        } else {
            card.description.clone()
        };
        div()
            .id(id)
            .relative()
            .w(px(width))
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.))
            .border_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .bg(color(mode, "--td-bg-color-container"))
            .cursor_pointer()
            .hover(|style| {
                style
                    .top(px(-2.))
                    .border_color(color(mode, "--td-brand-color"))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_online_playlist(selected.clone(), window, cx);
            }))
            .child(
                div().w_full().h(px(width - 2.)).overflow_hidden().child(
                    img(cover)
                        .size_full()
                        .rounded_t(px(12.))
                        .object_fit(ObjectFit::Cover)
                        .with_fallback(|| {
                            img(ImageSource::from("images/default-cover.png"))
                                .size_full()
                                .rounded_t(px(12.))
                                .object_fit(ObjectFit::Cover)
                                .into_any_element()
                        }),
                ),
            )
            .child(
                div()
                    .p(px(12.))
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .text_ellipsis()
                            .line_clamp(2)
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .mb(px(6.))
                            .text_color(color(mode, "--td-text-color-primary"))
                            .child(card.title.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .text_ellipsis()
                            .line_clamp(2)
                            .text_size(px(12.))
                            .text_color(color(mode, "--td-text-color-secondary"))
                            .child(description),
                    ),
            )
    }
}
