//! Singer page reached from a song's artist name: profile, songs and albums from the online SDK,
//! mirroring `views/music/singer.vue`. Like the online playlist page it is kept out of
//! KeepAlive: history navigation reloads it.
use super::*;
use crate::navigation::SingerRef;
use crate::online::{PlaylistCard, SingerAlbumCard, SingerProfile};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SingerTab {
    Songs,
    Albums,
}

pub(super) struct SingerView {
    pub header: Option<SingerRef>,
    /// `None` until the profile arrives, or for sources without one.
    pub profile: Option<SingerProfile>,
    pub songs: Remote<Song>,
    pub total: usize,
    pub page: usize,
    pub has_more: bool,
    pub loading_more: bool,
    pub more_error: Option<String>,
    pub albums: Remote<SingerAlbumCard>,
    pub tab: SingerTab,
    pub scroll: UniformListScrollHandle,
    pub albums_scroll: ScrollHandle,
    revision: u64,
    cancel: Option<Arc<AtomicBool>>,
}

impl SingerView {
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
            header: None,
            profile: None,
            songs: Remote::Idle,
            total: 0,
            page: 0,
            has_more: false,
            loading_more: false,
            more_error: None,
            albums: Remote::Idle,
            tab: SingerTab::Songs,
            scroll: UniformListScrollHandle::new(),
            albums_scroll: ScrollHandle::new(),
            revision: 0,
            cancel: None,
        }
    }
}

impl Mio {
    /// The singer page the artist of this song leads to, if its source gave a singer ID.
    pub(super) fn singer_of_song(&self, song: &Song) -> Option<SingerRef> {
        let (source, id) = self.online.as_ref()?.singer_of(&song.id)?;
        Some(SingerRef {
            id,
            source,
            name: song.artist.clone(),
        })
    }

    /// The artist of a song: a link to the singer page when the source gave a singer ID (the
    /// original's `.singer-link`, which turns brand-coloured on hover), plain text otherwise.
    pub(super) fn song_artist_label(
        &self,
        song: &Song,
        id: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        match self.singer_of_song(song) {
            Some(singer) => div()
                .id(id.into())
                .min_w_0()
                .truncate()
                .cursor_pointer()
                .hover(move |el| el.text_color(color(mode, "--td-brand-color")))
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_singer(singer.clone(), window, cx);
                }))
                .child(song.artist.clone())
                .into_any_element(),
            None => div()
                .min_w_0()
                .truncate()
                .child(song.artist.clone())
                .into_any_element(),
        }
    }

    pub(super) fn open_singer(
        &mut self,
        reference: SingerRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_context_actions(window, cx);
        self.song_view.reset_list();
        // The full player covers the page; the original lets the route change decide.
        self.full_player_open = false;
        self.navigation.push_singer(reference.clone());
        self.restore_singer(reference, cx);
        cx.notify();
    }

    /// (Re)load from the first page.
    pub(super) fn restore_singer(&mut self, reference: SingerRef, cx: &mut Context<Self>) {
        let state = &mut self.song_view.singer;
        state.revision += 1;
        if let Some(cancel) = state.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        state.profile = None;
        state.songs = Remote::Idle;
        state.total = 0;
        state.page = 0;
        state.has_more = false;
        state.loading_more = false;
        state.more_error = None;
        state.albums = Remote::Idle;
        state.tab = SingerTab::Songs;
        state.header = Some(reference);
        state
            .scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        state.albums_scroll.set_offset(point(px(0.), px(0.)));
        self.load_singer_profile(cx);
        self.load_singer_songs(false, cx);
        cx.notify();
    }

    fn singer_cancel(&mut self) -> Arc<AtomicBool> {
        let state = &mut self.song_view.singer;
        state
            .cancel
            .get_or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    fn load_singer_profile(&mut self, cx: &mut Context<Self>) {
        let Some(reference) = self.song_view.singer.header.clone() else {
            return;
        };
        let Some(online) = self.online.clone() else {
            return;
        };
        let revision = self.song_view.singer.revision;
        let cancel = self.singer_cancel();
        let task = cx.background_executor().spawn(async move {
            if cancel.load(Ordering::Relaxed) {
                return Err("搜索已取消".to_string());
            }
            online.singer_info(&reference.source, &reference.id)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.song_view.singer;
                if state.revision != revision {
                    return;
                }
                // A failed profile keeps the song's artist name; the lists are independent.
                if let Ok(profile) = result {
                    state.profile = profile;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn load_singer_songs(&mut self, more: bool, cx: &mut Context<Self>) {
        let state = &self.song_view.singer;
        if more && (state.loading_more || !state.has_more || state.more_error.is_some()) {
            return;
        }
        let Some(reference) = state.header.clone() else {
            return;
        };
        let Some(online) = self.online.clone() else {
            self.song_view.singer.songs = Remote::Failed("请先导入可用在线插件".into());
            return;
        };
        let page = if more { state.page + 1 } else { 1 };
        let keep = self.online_keep_ids();
        let cancel = self.singer_cancel();
        let state = &mut self.song_view.singer;
        if more {
            state.loading_more = true;
            state.more_error = None;
        } else {
            state.songs = Remote::Loading;
        }
        let revision = state.revision;
        let task = cx.background_executor().spawn(async move {
            online.singer_songs(&reference.source, &reference.id, page, &keep, cancel)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.song_view.singer;
                if state.revision != revision {
                    return;
                }
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
                                // A page that added nothing ends paging instead of looping.
                                state.has_more = found.has_more && songs.len() > before;
                            }
                        } else {
                            state.has_more = found.has_more && !found.songs.is_empty();
                            state.songs = Remote::Ready(found.songs);
                        }
                        state.page = page;
                    }
                    Err(error) => {
                        if more {
                            state.more_error = Some(error);
                        } else {
                            state.songs = Remote::Failed(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_singer_albums(&mut self, cx: &mut Context<Self>) {
        // Fetched when the tab is first opened and kept for the page, like the original.
        if matches!(
            self.song_view.singer.albums,
            Remote::Loading | Remote::Ready(_)
        ) {
            return;
        }
        let Some(reference) = self.song_view.singer.header.clone() else {
            return;
        };
        let Some(online) = self.online.clone() else {
            self.song_view.singer.albums = Remote::Failed("请先导入可用在线插件".into());
            return;
        };
        let cancel = self.singer_cancel();
        self.song_view.singer.albums = Remote::Loading;
        let revision = self.song_view.singer.revision;
        let task = cx
            .background_executor()
            .spawn(async move { online.singer_albums(&reference.source, &reference.id, cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.song_view.singer;
                if state.revision != revision {
                    return;
                }
                state.albums = match result {
                    Ok(albums) => Remote::Ready(albums),
                    Err(error) => Remote::Failed(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn switch_singer_tab(&mut self, tab: SingerTab, cx: &mut Context<Self>) {
        self.song_view.singer.tab = tab;
        if tab == SingerTab::Albums {
            self.load_singer_albums(cx);
        }
        cx.notify();
    }

    /// An album card opens the list page with the album's ID, as the original routes it
    /// (`list/:id?type=album`). The SDK distinguishes album IDs from playlist IDs.
    fn open_singer_album(
        &mut self,
        album: &SingerAlbumCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = self.song_view.singer.header.clone() else {
            return;
        };
        self.open_online_playlist(
            PlaylistCard {
                id: album.id.clone(),
                source: reference.source,
                title: album.name.clone(),
                description: album.description.clone(),
                cover: album.cover.clone(),
                author: album.author.clone(),
                song_count: (album.count > 0).then_some(album.count as u64),
                album: true,
            },
            window,
            cx,
        );
    }

    /// Replaces the play queue with the loaded songs after confirmation, like `handlePlayAll`.
    fn confirm_singer_play_all(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &self.song_view.singer;
        let Remote::Ready(songs) = &state.songs else {
            return;
        };
        if songs.is_empty() || self.online.is_none() {
            return;
        }
        let songs = songs.clone();
        let name = state
            .profile
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| state.header.as_ref().map(|h| h.name.clone()))
            .unwrap_or_default();
        let revision = state.revision;
        let queue_revision = self.player.queue_revision;
        let body = self
            .t("music.singer.replaceConfirm")
            .replace("{name}", &name)
            .replace("{count}", &songs.len().to_string());
        self.confirm_replace_queue(
            false,
            self.t("common.playAll"),
            body,
            std::rc::Rc::new(move |this, cx| {
                // Stale if the page was reloaded or the queue changed behind the dialog.
                if this.song_view.singer.revision != revision
                    || this.player.queue_revision != queue_revision
                {
                    return;
                }
                this.replace_player_queue(songs.clone());
                this.play_index(0, cx);
            }),
            window,
            cx,
        );
    }

    pub(super) fn singer_page(
        &self,
        narrow: bool,
        width: f32,
        rem: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let state = &self.song_view.singer;
        let reference = state.header.clone();
        let name = state
            .profile
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty())
            .or_else(|| {
                reference
                    .as_ref()
                    .map(|r| r.name.clone())
                    .filter(|n| !n.is_empty())
            })
            .unwrap_or_else(|| self.t("music.singer.loading"));
        let avatar = state
            .profile
            .as_ref()
            .and_then(|p| p.avatar.clone())
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        let avatar_size = if narrow { 112. } else { 100. };
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");

        let mut info = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .when(narrow, |info| info.items_center().text_center())
            .child(
                div()
                    .mb(px(4.))
                    .text_size(px(if narrow {
                        (width * 0.09).clamp(32., 41.6)
                    } else {
                        22.
                    }))
                    .when(narrow, |name| name.line_height(relative(1.1)))
                    .font_weight(FontWeight::BOLD)
                    .text_color(primary)
                    .when(!narrow, |name| name.truncate())
                    .child(name.clone()),
            );
        if let Some(profile) = &state.profile {
            info = info.child(
                div()
                    .mb(px(4.))
                    .text_size(px(13.))
                    .text_color(secondary)
                    .child(format!(
                        "{} · {}",
                        self.t("music.singer.songCount")
                            .replace("{count}", &profile.music_count.to_string()),
                        self.t("music.singer.albumCount")
                            .replace("{count}", &profile.album_count.to_string()),
                    )),
            );
            if !profile.desc.is_empty() {
                info = info.child(
                    div()
                        .mt(px(4.))
                        .overflow_hidden()
                        .line_clamp(if narrow { 3 } else { 2 })
                        .text_size(px(if narrow { 13.6 } else { 12. }))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(profile.desc.clone()),
                );
            }
        }
        let can_play = matches!(&state.songs, Remote::Ready(songs) if !songs.is_empty())
            && self.online.is_some();
        if can_play {
            info = info.child(
                div().mt(px(16.)).child(
                    Button::new("singer-play-all")
                        .primary()
                        .accessibility_label(self.t("music.singer.playAll"))
                        .label(self.t("music.singer.playAll"))
                        .h(px(if narrow { 44. } else { 36. }))
                        .px(px(24.))
                        .rounded(px(if narrow { 12. } else { 20. }))
                        .text_size(px(14.))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_singer_play_all(window, cx)
                        })),
                ),
            );
        }
        let header = div()
            .flex_shrink_0()
            .relative()
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                24.
            }))
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                16.
            }))
            .pb(px(20.))
            .bg(linear_gradient(
                180.,
                linear_color_stop(brand.opacity(0.12), 0.),
                linear_color_stop(transparent_black(), 1.),
            ))
            .child(
                div().mb(px(if narrow { 8. } else { 12. })).child(
                    control(
                        "singer-back",
                        "NavigateBefore",
                        self.t("common.back"),
                        mode,
                        cx,
                    )
                    .when(narrow, |button| button.size(px(44.)).rounded_full())
                    .disabled(!self.navigation.can_back())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate_history(false, window, cx);
                    })),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(if narrow { 12. } else { 16. }))
                    .when(narrow, |meta| meta.flex_col().items_center())
                    .when(!narrow, |meta| meta.items_end())
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(avatar_size))
                            .rounded(px(avatar_size / 2.))
                            .overflow_hidden()
                            .bg(color(mode, "--td-bg-color-secondarycontainer"))
                            .child(
                                img(avatar)
                                    .size_full()
                                    .rounded(px(avatar_size / 2.))
                                    .object_fit(ObjectFit::Cover)
                                    .with_fallback(|| {
                                        img(ImageSource::from("images/default-cover.png"))
                                            .size_full()
                                            .object_fit(ObjectFit::Cover)
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .child(info),
            );

        let albums_count = state.profile.as_ref().map(|p| p.album_count).unwrap_or(0);
        let tabs = div()
            .flex_shrink_0()
            .flex()
            .gap(px(if narrow { 12. } else { 24. }))
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                24.
            }))
            .border_b_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .children(
                [SingerTab::Songs, SingerTab::Albums]
                    .into_iter()
                    .map(|tab| {
                        let selected = state.tab == tab;
                        let label = match tab {
                            SingerTab::Songs => {
                                format!("{}({})", self.t("music.singer.tabSongs"), state.total)
                            }
                            SingerTab::Albums => {
                                format!("{}({})", self.t("music.singer.tabAlbums"), albums_count)
                            }
                        };
                        gpui_kit::base::Button::new(match tab {
                            SingerTab::Songs => "singer-tab-songs",
                            SingerTab::Albums => "singer-tab-albums",
                        })
                        .accessibility_label(label.clone())
                        .aria_selected(selected)
                        .px(px(0.))
                        .py(px(10.))
                        .min_h(px(if narrow { 44. } else { 0. }))
                        .text_size(px(if narrow { 15. } else { 14. }))
                        .text_color(if selected { brand } else { secondary })
                        .border_b(px(2.))
                        .border_color(if selected { brand } else { transparent_black() })
                        .child(label)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.switch_singer_tab(tab, cx)),
                        )
                    }),
            );

        let loading = |this: &Self| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .text_size(px(13.))
                .text_color(color(this.ui_colors(), "--td-text-color-placeholder"))
                .child(gpui_kit::component::spinner::Spinner::new().color(brand))
                .child(this.t("common.loading"))
        };
        let mut content = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden();
        content = match state.tab {
            SingerTab::Songs => match &state.songs {
                Remote::Idle | Remote::Loading => content.child(loading(self)),
                Remote::Failed(error) => content
                    .child(self.search_message(&self.t("music.search.searchFailed"), error, mode))
                    .child(
                        Button::new("singer-retry")
                            .label(self.t("common.retry"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(reference) = this.song_view.singer.header.clone() {
                                    this.restore_singer(reference, cx);
                                }
                            })),
                    ),
                Remote::Ready(songs) if songs.is_empty() => content.child(self.search_message(
                    &self.t("music.list.noSongs"),
                    &self.t("music.search.tryOther"),
                    mode,
                )),
                Remote::Ready(songs) => {
                    let rows = Arc::new(songs.clone());
                    let count = rows.len();
                    let mut content = content.child(
                        uniform_list(
                            "singer-rows",
                            count,
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|index| {
                                        this.online_song_row(&rows[index], index, narrow, cx)
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .track_scroll(&state.scroll)
                        .flex_1()
                        .min_h_0()
                        .on_scroll_wheel(cx.listener(
                            |_, _, window, cx| {
                                cx.defer_in(window, |this, _, cx| {
                                    let scroll = this.song_view.singer.scroll.0.borrow();
                                    let bottom = -scroll.base_handle.offset().y
                                        + scroll.base_handle.bounds().size.height;
                                    let full = scroll.base_handle.max_offset().y
                                        + scroll.base_handle.bounds().size.height;
                                    drop(scroll);
                                    if bottom >= full - px(100.) {
                                        this.load_singer_songs(true, cx);
                                    }
                                });
                            },
                        )),
                    );
                    if state.loading_more {
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
                            Button::new("singer-page-retry")
                                .label(self.t("music.search.pageRetry").replace("{error}", error))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.song_view.singer.more_error = None;
                                    this.load_singer_songs(true, cx);
                                })),
                        );
                    } else if !state.has_more {
                        content = content.child(
                            div()
                                .flex_shrink_0()
                                .py(px(12.))
                                .flex()
                                .justify_center()
                                .text_size(px(13.))
                                .text_color(color(mode, "--td-text-color-placeholder"))
                                .child(self.t("music.singer.noMore")),
                        );
                    }
                    content
                }
            },
            SingerTab::Albums => match &state.albums {
                Remote::Idle | Remote::Loading => content.child(loading(self)),
                Remote::Failed(error) => content
                    .child(self.search_message(&self.t("music.search.searchFailed"), error, mode))
                    .child(
                        Button::new("singer-albums-retry")
                            .label(self.t("common.retry"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.song_view.singer.albums = Remote::Idle;
                                this.load_singer_albums(cx);
                            })),
                    ),
                Remote::Ready(albums) if albums.is_empty() => content.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(self.t("music.singer.noAlbums")),
                ),
                Remote::Ready(albums) => {
                    let gap = if narrow { 12. } else { 16. };
                    let padding = if narrow {
                        (width * 0.04).clamp(16., 24.)
                    } else {
                        24.
                    };
                    let page_padding = if narrow { 0. } else { 20. };
                    let sidebar = if narrow { 0. } else { 15. * rem };
                    let inner = (width - sidebar - 2. * page_padding - 2. * padding).max(1.);
                    let min_card = if narrow { 0. } else { 140. };
                    let columns = if narrow {
                        2
                    } else {
                        ((inner + gap) / (min_card + gap)).floor().max(1.) as u16
                    };
                    let card_width = (inner - gap * (columns - 1) as f32) / columns as f32;
                    content.child(
                        div()
                            .id("singer-album-grid")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&state.albums_scroll)
                            .p(px(padding))
                            .child(div().grid().grid_cols(columns).gap(px(gap)).children(
                                albums.iter().enumerate().map(|(index, album)| {
                                    self.singer_album_card(
                                        album, index, card_width, narrow, mode, cx,
                                    )
                                    .into_any_element()
                                }),
                            )),
                    )
                }
            },
        };
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(color(mode, "--search-bg"))
            .child(header)
            .child(tabs)
            .child(content)
    }

    fn singer_album_card(
        &self,
        album: &SingerAlbumCard,
        index: usize,
        width: f32,
        narrow: bool,
        mode: crate::skin::Colors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = album.clone();
        let cover = album
            .cover
            .clone()
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        div()
            .id(SharedString::from(format!("singer-album-{index}")))
            .w(px(width))
            .min_w_0()
            .flex()
            .flex_col()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_singer_album(&selected, window, cx);
            }))
            .child(
                div()
                    .w(px(width))
                    .h(px(width))
                    .rounded(px(if narrow { 16. } else { 8. }))
                    .overflow_hidden()
                    .bg(color(mode, "--td-bg-color-secondarycontainer"))
                    .child(
                        img(cover)
                            .size_full()
                            .rounded(px(if narrow { 16. } else { 8. }))
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(move || {
                                img(ImageSource::from("images/default-cover.png"))
                                    .size_full()
                                    .rounded(px(if narrow { 16. } else { 8. }))
                                    .object_fit(ObjectFit::Cover)
                                    .into_any_element()
                            }),
                    ),
            )
            .child(
                div()
                    .mt(px(6.))
                    .truncate()
                    .text_size(px(13.))
                    .text_color(color(mode, "--td-text-color-primary"))
                    .child(album.name.clone()),
            )
            // QQ Music's album list reports no song count (always 0); a "0 首" label would
            // be wrong, so the line is left out then.
            .when(album.count > 0, |card| {
                card.child(
                    div()
                        .mt(px(2.))
                        .text_size(px(12.))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(
                            self.t("music.songList.songsUnit")
                                .replace("{count}", &album.count.to_string()),
                        ),
                )
            })
    }
}
