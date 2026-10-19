//! Contents of a playlist found through online search. A page of its own rather than a Subsonic
//! album: its songs, cover and paging come from the online SDK, never from the Subsonic server.
use super::*;
use crate::navigation::OnlinePlaylistRef;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct PlaylistPlayAll {
    shuffle: bool,
    queue_revision: u64,
    filter: String,
}

pub(super) struct OnlinePlaylistView {
    pub songs: Remote<Song>,
    pub total: usize,
    pub page: usize,
    pub has_more: bool,
    pub loading_more: bool,
    pub more_error: Option<String>,
    pub play_all: Option<PlaylistPlayAll>,
    /// Header details; seeded from the search card and refreshed by the first page.
    pub header: Option<OnlinePlaylistRef>,
    pub scroll: UniformListScrollHandle,
    revision: u64,
    cancel: Option<Arc<AtomicBool>>,
}

impl OnlinePlaylistView {
    /// Changes whenever another playlist is opened; paging only appends within one revision.
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn invalidate(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let revision = self.revision.wrapping_add(1);
        *self = Self::new();
        self.revision = revision;
    }

    pub fn new() -> Self {
        Self {
            songs: Remote::Idle,
            total: 0,
            page: 0,
            has_more: false,
            loading_more: false,
            more_error: None,
            play_all: None,
            header: None,
            scroll: UniformListScrollHandle::new(),
            revision: 0,
            cancel: None,
        }
    }
}

impl Mio {
    pub(super) fn open_online_playlist(
        &mut self,
        card: crate::online::PlaylistCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_context_actions(window, cx);
        self.song_view.reset_list();
        let reference = OnlinePlaylistRef {
            id: card.id,
            source: card.source,
            title: card.title,
            cover: card.cover,
            author: card.author,
            description: card.description,
            album: card.album,
        };
        self.navigation.push_online_playlist(reference.clone());
        self.restore_online_playlist(reference, window, cx);
    }

    /// (Re)load from the first page; the original keeps list pages out of KeepAlive.
    pub(super) fn restore_online_playlist(
        &mut self,
        reference: OnlinePlaylistRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.song_view
            .filter
            .update(cx, |input, cx| input.set_value("", window, cx));
        let state = &mut self.song_view.online_playlist;
        state.revision += 1;
        if let Some(cancel) = state.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        state.songs = Remote::Idle;
        state.total = 0;
        state.page = 0;
        state.has_more = false;
        state.loading_more = false;
        state.more_error = None;
        state.play_all = None;
        state.header = Some(reference);
        state
            .scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.load_online_playlist(false, cx);
        cx.notify();
    }

    pub(super) fn load_online_playlist(&mut self, more: bool, cx: &mut Context<Self>) {
        let state = &self.song_view.online_playlist;
        if more && (state.loading_more || !state.has_more || state.more_error.is_some()) {
            return;
        }
        let Some(reference) = state.header.clone() else {
            return;
        };
        let Some(online) = self.online.clone() else {
            self.song_view.online_playlist.songs = Remote::Failed("请先导入可用在线插件".into());
            return;
        };
        let page = if more { state.page + 1 } else { 1 };
        let keep = self.online_keep_ids();
        let state = &mut self.song_view.online_playlist;
        if more {
            state.loading_more = true;
            state.more_error = None;
        } else {
            state.songs = Remote::Loading;
        }
        let revision = state.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel = Some(cancel.clone());
        let task = cx.background_executor().spawn(async move {
            online.playlist_detail(
                &reference.source,
                &reference.id,
                page,
                reference.album,
                &keep,
                cancel,
            )
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.song_view.online_playlist;
                if state.revision != revision {
                    return;
                }
                state.cancel = None;
                state.loading_more = false;
                match result {
                    Ok(found) => {
                        state.total = found.total;
                        if more {
                            if let Remote::Ready(songs) = &mut state.songs {
                                let before = songs.len();
                                let mut ids: std::collections::HashSet<String> =
                                    songs.iter().map(|s| s.id.clone()).collect();
                                songs.extend(
                                    found.songs.into_iter().filter(|s| ids.insert(s.id.clone())),
                                );
                                // Stop automatic paging on a non-progressing response, but keep it retryable.
                                let added = songs.len() > before;
                                state.has_more = found.has_more && added;
                                if found.has_more && !added {
                                    state.has_more = true;
                                    state.more_error = Some(
                                        if this.english {
                                            "The source returned no new songs; retry loading."
                                        } else {
                                            "来源未返回新歌曲，请重试加载。"
                                        }
                                        .into(),
                                    );
                                }
                            }
                        } else {
                            state.has_more = found.has_more && !found.songs.is_empty();
                            state.songs = Remote::Ready(found.songs);
                            if let Some(header) = &mut state.header {
                                if !found.title.is_empty() {
                                    header.title = found.title;
                                }
                                if found.cover.is_some() {
                                    header.cover = found.cover;
                                }
                                // The search card already has the nickname; the detail
                                // response only carries the account ID.
                                if header.author.is_empty() && !found.author.is_empty() {
                                    header.author = found.author;
                                }
                                if !found.description.is_empty() {
                                    header.description = found.description;
                                }
                            }
                        }
                        if state.more_error.is_none() {
                            state.page = page;
                        }
                    }
                    Err(error) => {
                        if more {
                            state.more_error = Some(error);
                        } else {
                            state.songs = Remote::Failed(error);
                        }
                    }
                }
                this.continue_online_playlist_play(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn continue_online_playlist_play(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.song_view.online_playlist;
        let Some(pending) = &state.play_all else {
            return;
        };
        if self.navigation.current() != Page::OnlinePlaylist
            || self.player.queue_revision != pending.queue_revision
            || self.online.is_none()
            || state.more_error.is_some()
        {
            state.play_all = None;
            return;
        }
        if state.loading_more {
            return;
        }
        if state.has_more {
            self.load_online_playlist(true, cx);
            cx.notify();
            return;
        }
        let pending = state.play_all.take().unwrap();
        let Remote::Ready(songs) = &state.songs else {
            return;
        };
        let mut songs: Vec<Song> = songs
            .iter()
            .filter(|song| {
                pending.filter.is_empty()
                    || song.title.to_lowercase().contains(&pending.filter)
                    || song.artist.to_lowercase().contains(&pending.filter)
                    || song.album.to_lowercase().contains(&pending.filter)
            })
            .cloned()
            .collect();
        let mut ids = std::collections::HashSet::new();
        songs.retain(|song| ids.insert(song.id.clone()));
        if songs.is_empty() {
            return;
        }
        if pending.shuffle {
            for i in (1..songs.len()).rev() {
                let j = (uuid::Uuid::new_v4().as_u128() % (i + 1) as u128) as usize;
                songs.swap(i, j);
            }
        }
        self.replace_player_queue(songs);
        self.play_index(0, cx);
        cx.notify();
    }

    pub(super) fn online_playlist_page(
        &self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let state = &self.song_view.online_playlist;
        let compact = state.scroll.0.borrow().base_handle.offset().y < px(-100.);
        let rem = if narrow {
            16.
        } else {
            f32::from(window.rem_size())
        };
        let playlist_header = self.album_header(narrow, compact, rem, window, cx);
        let mut content = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(!narrow, |content| content.rounded(px(12.)))
            .bg(color(mode, "--song-list-content-bg"));
        match &state.songs {
            Remote::Idle | Remote::Loading if narrow => {
                content = content.child(
                    div()
                        .size_full()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .children((0..12).map(|_| playlist_song_skeleton())),
                );
            }
            Remote::Idle | Remote::Loading => {
                content = content.child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .text_size(px(14.))
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(
                            gpui_kit::component::spinner::Spinner::new()
                                .color(color(mode, "--td-brand-color")),
                        )
                        .child(self.t("common.loading")),
                );
            }
            Remote::Failed(error) => {
                content = content
                    .child(self.search_message(&self.t("music.search.searchFailed"), error, mode))
                    .child(
                        Button::new("online-playlist-retry")
                            .label(self.t("common.retry"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(reference) =
                                    this.song_view.online_playlist.header.clone()
                                {
                                    this.restore_online_playlist(reference, window, cx);
                                }
                            })),
                    );
            }
            Remote::Ready(songs) if songs.is_empty() => {
                content = content.child(self.search_message(
                    &self.t("music.list.noSongs"),
                    &self.t("music.search.tryOther"),
                    mode,
                ));
            }
            Remote::Ready(_) => {
                let rows = self.filtered_songs(cx);
                let count = rows.len();
                content = content.child(
                    uniform_list(
                        "online-playlist-rows",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.online_song_row(&rows[index], index, narrow, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&state.scroll)
                    .flex_1()
                    .min_h_0()
                    .on_scroll_wheel(cx.listener(|_, _, window, cx| {
                        cx.defer_in(window, |this, _, cx| {
                            let scroll = this.song_view.online_playlist.scroll.0.borrow();
                            let bottom = -scroll.base_handle.offset().y
                                + scroll.base_handle.bounds().size.height;
                            let full = scroll.base_handle.max_offset().y
                                + scroll.base_handle.bounds().size.height;
                            drop(scroll);
                            cx.notify();
                            if bottom >= full - px(100.) {
                                this.load_online_playlist(true, cx);
                            }
                        });
                    })),
                );
                if state.loading_more && narrow {
                    content = content.child(playlist_song_skeleton());
                } else if state.loading_more {
                    content = content.child(
                        div()
                            .h(px(44.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(gpui_kit::component::spinner::Spinner::new()),
                    );
                } else if let Some(error) = &state.more_error {
                    content = content.child(
                        Button::new("online-playlist-page-retry")
                            .label(self.t("music.search.pageRetry").replace("{error}", error))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.song_view.online_playlist.more_error = None;
                                this.load_online_playlist(true, cx);
                            })),
                    );
                }
            }
        }
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(!narrow, |page| page.p(px(20.)))
            .bg(color(mode, "--search-bg"))
            .child(playlist_header)
            .child(content)
    }

    /// Fetch remaining pages before replacing the queue with the complete filtered list.
    pub(super) fn confirm_online_playlist_play(
        &self,
        shuffle: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &self.song_view.online_playlist;
        let Remote::Ready(songs) = &state.songs else {
            return;
        };
        if songs.is_empty() || self.online.is_none() || state.play_all.is_some() {
            return;
        }
        let count = state.total.max(songs.len());
        let filter = self.song_view.filter.read(cx).value().trim().to_lowercase();
        let title = state
            .header
            .as_ref()
            .map(|h| h.title.clone())
            .unwrap_or_default();
        let revision = state.revision;
        let queue_revision = self.player.queue_revision;
        let heading = self.t(if shuffle {
            "music.list.shufflePlaylist"
        } else {
            "music.list.playPlaylist"
        });
        let body = self
            .t(if shuffle {
                "music.list.shuffleConfirm"
            } else {
                "music.list.replaceConfirm"
            })
            .replace("{title}", &title)
            .replace("{count}", &count.to_string());
        self.confirm_replace_queue(
            shuffle,
            heading,
            body,
            std::rc::Rc::new(move |this, cx| {
                // Stale if the page was reloaded or the queue changed behind the dialog.
                if this.song_view.online_playlist.revision != revision
                    || this.player.queue_revision != queue_revision
                {
                    return;
                }
                this.song_view.online_playlist.more_error = None;
                this.song_view.online_playlist.play_all = Some(PlaylistPlayAll {
                    shuffle,
                    queue_revision,
                    filter: filter.clone(),
                });
                this.continue_online_playlist_play(cx);
            }),
            window,
            cx,
        );
    }

    /// One song of an online playlist: number, title, artist - album, like, download, duration.
    pub(super) fn online_song_row(
        &self,
        song: &Song,
        index: usize,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let playing = self
            .player
            .song()
            .is_some_and(|active| active.id == song.id);
        let album_suffix = if song.album.is_empty() {
            String::new()
        } else {
            format!(" - {}", song.album)
        };
        let duration = if song.duration == 0 {
            "--:--".to_owned()
        } else {
            format!("{}:{:02}", song.duration / 60, song.duration % 60)
        };
        let play_song = song.clone();
        let menu_song = song.clone();
        let download_song = song.clone();
        let download = control(
            SharedString::from(format!("online-song-download-{index}")),
            "Download",
            self.t("music.songList.download"),
            mode,
            cx,
        )
        .disabled(self.online.is_none())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.download_songs(vec![download_song.clone()], window, cx);
        }));
        div()
            .id(SharedString::from(format!("online-song-{index}")))
            .role(gpui_kit::Role::Button)
            .aria_label(self.online_song_accessibility_label(song))
            .w_full()
            .h(px(if narrow { 72. } else { 56. }))
            .px(px(if narrow { 10. } else { 12. }))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(4.))
            .min_w_0()
            .cursor_pointer()
            .bg(if playing {
                color(mode, "--song-list-item-playing")
            } else {
                transparent_black()
            })
            .hover(move |el| el.bg(color(mode, "--theme-hover-bg")))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.online.is_none() {
                    return;
                }
                let index = this.enqueue_player_first(play_song.clone());
                this.play_index(index, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.song_context_actions(menu_song.clone(), event.position, window, cx);
                }),
            )
            .child(
                div()
                    .w(px(if narrow { 26. } else { 32. }))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(color(mode, "--theme-text-secondary"))
                    .child((index + 1).to_string()),
            )
            .child(self.song_row_thumbnail(song))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(if narrow { 15. } else { 14. }))
                            .text_color(color(mode, "--theme-text-primary"))
                            .child(song.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .overflow_hidden()
                            .text_size(px(12.))
                            .text_color(color(mode, "--theme-text-secondary"))
                            .items_center()
                            .gap(px(4.))
                            .when(!narrow, |row| row.child(self.online_song_badges(song)))
                            .child(self.song_artist_label(
                                song,
                                SharedString::from(format!("online-song-singer-{index}")),
                                cx,
                            ))
                            .child(div().min_w_0().truncate().child(album_suffix)),
                    )
                    .when(narrow, |info| {
                        info.child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .child(self.online_song_badges(song)),
                        )
                    }),
            )
            .child(self.favorite_button(
                SharedString::from(format!("online-song-like-{index}")),
                Some(song),
                true,
                cx,
            ))
            .child(download)
            .child(
                div()
                    .ml(px(8.))
                    .w(px(44.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(color(mode, "--theme-text-secondary"))
                    .child(duration),
            )
            .into_any_element()
    }
}

fn playlist_song_skeleton() -> impl IntoElement {
    use gpui_kit::component::skeleton::Skeleton;
    div()
        .h(px(72.))
        .w_full()
        .flex_shrink_0()
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(Skeleton::new().w(px(16.)).h(px(12.)).flex_shrink_0())
        .child(
            Skeleton::new()
                .size(px(40.))
                .rounded(px(4.))
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(Skeleton::new().w(relative(0.8)).h(px(15.)))
                .child(Skeleton::new().secondary().w(relative(0.6)).h(px(12.)))
                .child(Skeleton::new().secondary().w(relative(0.5)).h(px(14.))),
        )
        .child(Skeleton::new().w(px(28.)).h(px(12.)).flex_shrink_0())
}
