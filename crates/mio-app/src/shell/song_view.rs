use super::*;
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant};

pub(super) struct SongView {
    pub sort: song_sort::Sort,
    pub hovered_sort: Option<song_sort::Column>,
    pub cover_hovered: bool,
    pub multi: bool,
    pub selected: std::collections::HashSet<String>,
    pub filter: Entity<InputState>,
    pub scroll: UniformListScrollHandle,
    pub search_scroll: UniformListScrollHandle,
    visible: std::cell::RefCell<Option<VisibleSongs>>,
    /// Filtered rows of the online playlist: (revision, loaded count, query, rows).
    online_visible: std::cell::RefCell<Option<(u64, usize, String, Arc<Vec<Song>>)>>,
    pub album_covers: album_covers::AlbumCovers,
    pub search_covers: album_covers::AlbumCovers,
    pub album_qualities: std::collections::HashMap<String, crate::song::SongQuality>,
    pub search_qualities: std::collections::HashMap<String, crate::song::SongQuality>,
    pub search_query: String,
    pub search_playlists: bool,
    pub search_online: bool,
    pub playlists: playlist_search::PlaylistSearch,
    pub online_playlist: online_playlist::OnlinePlaylistView,
    pub singer: singer_page::SingerView,
    pub downloads: downloads_page::DownloadUi,
    /// `None` with `search_online` is the aggregate "all sources" search.
    pub search_source: Option<String>,
    pub search_has_more: bool,
    pub search_loading_more: bool,
    pub search_offset: usize,
    pub search_page_error: Option<String>,
    pub backdrop: Entity<album_backdrop::AlbumBackdrop>,
}
struct VisibleSongs {
    album: bool,
    revision: u64,
    query: String,
    songs: Arc<Vec<Song>>,
    sorted: Option<(song_sort::Sort, Arc<Vec<Song>>)>,
}

impl SongView {
    pub fn reset_list(&mut self) {
        self.reset_selection();
        self.sort = Default::default();
        self.hovered_sort = None;
        self.cover_hovered = false;
    }
    pub fn reset_selection(&mut self) {
        self.multi = false;
        self.selected.clear();
    }
    pub fn new(
        english: bool,
        window: &mut Window,
        cx: &mut Context<Mio>,
    ) -> (Self, [Subscription; 3]) {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("请输入"));
        let download_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(i18n::text(english, "download.searchTask"))
        });
        let download_search_subscription =
            cx.subscribe_in(&download_search, window, |_, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            });
        let backdrop = cx.new(|_| album_backdrop::AlbumBackdrop::default());
        let backdrop_subscription = cx.observe(&backdrop, |_, _, cx| cx.notify());
        let scroll = UniformListScrollHandle::new();
        let subscription = cx.subscribe_in(&filter, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let visible = this.filtered_songs(cx);
                if !this.song_view.selected.is_empty() {
                    let visible: std::collections::HashSet<&str> =
                        visible.iter().map(|song| song.id.as_str()).collect();
                    this.song_view
                        .selected
                        .retain(|id| visible.contains(id.as_str()));
                }
                let scroll = if this.navigation.current() == Page::OnlinePlaylist {
                    &this.song_view.online_playlist.scroll
                } else {
                    &this.song_view.scroll
                };
                scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(0.)));
                cx.notify();
            }
        });
        (
            Self {
                sort: Default::default(),
                hovered_sort: None,
                cover_hovered: false,
                multi: false,
                selected: Default::default(),
                filter,
                scroll,
                search_scroll: UniformListScrollHandle::new(),
                visible: Default::default(),
                online_visible: Default::default(),
                album_covers: Default::default(),
                search_covers: Default::default(),
                album_qualities: Default::default(),
                search_qualities: Default::default(),
                search_query: String::new(),
                search_playlists: false,
                search_online: true,
                playlists: playlist_search::PlaylistSearch::new(),
                online_playlist: online_playlist::OnlinePlaylistView::new(),
                singer: singer_page::SingerView::new(),
                downloads: downloads_page::DownloadUi::new(download_search),
                search_source: None,
                search_has_more: false,
                search_loading_more: false,
                search_offset: 0,
                search_page_error: None,
                backdrop,
            },
            [
                subscription,
                backdrop_subscription,
                download_search_subscription,
            ],
        )
    }
}

impl Mio {
    pub(super) fn filtered_songs(&self, cx: &App) -> Arc<Vec<Song>> {
        if self.navigation.current() == Page::OnlinePlaylist {
            let Remote::Ready(songs) = &self.song_view.online_playlist.songs else {
                return Arc::new(vec![]);
            };
            let query = self.song_view.filter.read(cx).value().trim().to_lowercase();
            let revision = self.song_view.online_playlist.revision();
            let mut cache = self.song_view.online_visible.borrow_mut();
            if let Some((cached_revision, loaded, cached_query, rows)) = cache.as_ref()
                && *cached_revision == revision
                && *loaded == songs.len()
                && *cached_query == query
            {
                return rows.clone();
            }
            let rows: Arc<Vec<Song>> = Arc::new(
                songs
                    .iter()
                    .filter(|song| {
                        query.is_empty()
                            || song.title.to_lowercase().contains(&query)
                            || song.artist.to_lowercase().contains(&query)
                            || song.album.to_lowercase().contains(&query)
                    })
                    .cloned()
                    .collect(),
            );
            *cache = Some((revision, songs.len(), query, rows.clone()));
            return rows;
        }
        let album = self.navigation.current().is_song_detail();
        let state = if album {
            &self.album_songs
        } else {
            &self.songs
        };
        let Remote::Ready(songs) = state else {
            return Arc::new(vec![]);
        };
        let query = if album {
            self.song_view.filter.read(cx).value().trim().to_lowercase()
        } else {
            String::new()
        };
        let revision = if album {
            self.album_song_revision
        } else {
            self.song_revision
        };
        let mut visible = self.song_view.visible.borrow_mut();
        if let Some(cached) = visible.as_ref()
            && cached.album == album
            && cached.revision == revision
            && cached.query == query
        {
            return cached.songs.clone();
        }
        let songs: Arc<Vec<Song>> = Arc::new(
            songs
                .iter()
                .filter(|song| {
                    query.is_empty()
                        || song.title.to_lowercase().contains(&query)
                        || song.artist.to_lowercase().contains(&query)
                        || song.album.to_lowercase().contains(&query)
                })
                .cloned()
                .collect(),
        );
        *visible = Some(VisibleSongs {
            album,
            revision,
            query,
            songs: songs.clone(),
            sorted: None,
        });
        songs
    }

    fn display_songs(&self, cx: &App) -> Arc<Vec<Song>> {
        let songs = self.filtered_songs(cx);
        let sort = self.song_view.sort;
        if self.navigation.current() != Page::Playlist
            || sort == song_sort::Sort::Default
            || songs.is_empty()
        {
            return songs;
        }
        let mut visible = self.song_view.visible.borrow_mut();
        if let Some((cached_sort, sorted)) = visible.as_ref().and_then(|v| v.sorted.as_ref())
            && *cached_sort == sort
        {
            return sorted.clone();
        }
        let mut sorted = songs.as_ref().clone();
        sort.apply(&mut sorted);
        let sorted = Arc::new(sorted);
        if let Some(visible) = visible.as_mut() {
            visible.sorted = Some((sort, sorted.clone()));
        }
        sorted
    }

    pub(super) fn song_list(
        &self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let rem = if narrow {
            16.
        } else {
            f32::from(window.rem_size())
        };
        let album = self.navigation.current().is_song_detail();
        let state = if album {
            &self.album_songs
        } else {
            &self.songs
        };
        let songs = self.display_songs(cx);
        let compact = self.song_view.scroll.0.borrow().base_handle.offset().y < px(-100.);
        let mut page = div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(!narrow, |page| page.p(px(20.)));
        if album {
            page = page.child(self.album_header(narrow, compact, rem, window, cx));
        } else {
            page = page.child(
                div()
                    .p(px(16.))
                    .text_size(px(1.5 * rem))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t("common.search")),
            );
        }
        let mut content = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(color(mode, "--song-list-content-bg"))
            .when(!narrow, |content| content.rounded(px(12.)));
        if matches!(state, Remote::Loading | Remote::Idle) {
            content = content.child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(16.))
                    .text_size(px(14.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(
                        gpui_kit::component::spinner::Spinner::new()
                            .color(color(mode, "--td-brand-color")),
                    )
                    .child(self.t("common.loading")),
            );
        } else {
            content = content.child(if self.song_view.multi {
                self.batch_song_columns(narrow, cx)
            } else {
                self.song_columns(narrow, cx).into_any_element()
            });
            if let Remote::Failed(error) = state {
                content = content.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .child(error.clone())
                        .child(
                            Button::new("retry-song-list")
                                .label(self.t("music.playlistCategory.reload"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.navigation.current() == Page::Playlist {
                                        this.load_local_playlist(cx);
                                        return;
                                    }
                                    this.load_songs(
                                        if album {
                                            this.selected_album.as_ref().map(|a| a.id.clone())
                                        } else {
                                            None
                                        },
                                        cx,
                                    );
                                })),
                        ),
                );
            } else if songs.is_empty() {
                content = content.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(14.))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(self.t(
                            if album && !self.song_view.filter.read(cx).value().is_empty() {
                                "music.list.noMatchSongs"
                            } else {
                                "music.list.noSongs"
                            },
                        )),
                );
            } else {
                let rows = songs.clone();
                content = content.child(
                    uniform_list(
                        "song-view-rows",
                        songs.len(),
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.song_row(&rows[index], index, narrow, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(if album {
                        &self.song_view.scroll
                    } else {
                        &self.song_view.search_scroll
                    })
                    .size_full()
                    .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify())),
                );
            }
        }
        page.child(content)
    }

    pub(super) fn album_header(
        &self,
        narrow: bool,
        compact: bool,
        rem: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let online = self.navigation.current() == Page::OnlinePlaylist;
        let online_state = &self.song_view.online_playlist;
        let online_header = online_state.header.as_ref().filter(|_| online);
        let album = self.selected_album.as_ref().filter(|_| !online);
        let cover_image = if self.navigation.current() == Page::Playlist {
            self.local_playlists.selected_cover.clone()
        } else {
            album
                .and_then(|album| album.cover_art.as_ref())
                .and_then(|id| self.song_view.album_covers.images.get(id))
                .and_then(|image| image.clone())
                .or_else(|| {
                    album
                        .and_then(|album| album.cover_art.as_ref())
                        .and_then(|id| self.album_covers.images.get(id))
                        .and_then(|image| image.clone())
                })
        };
        let cover = online_header
            .and_then(|header| header.cover.clone())
            .map(ImageSource::from)
            .or_else(|| cover_image.clone().map(ImageSource::from))
            .unwrap_or_else(|| {
                self.local_playlists
                    .selected
                    .as_ref()
                    .filter(|_| self.navigation.current() == Page::Playlist)
                    .map(|p| p.cover_img_url.as_str())
                    .filter(|url| {
                        !url.is_empty() && *url != "default-cover" && !url.starts_with("data:")
                    })
                    .map(|url| ImageSource::from(url.to_owned()))
                    .unwrap_or_else(|| ImageSource::from("images/default-cover.png"))
            });
        // Share the img asset cache: URLs and the default image also contribute
        // a backdrop, without fetching or decoding a second copy.
        let cover_image = cover_image.or_else(|| match &cover {
            ImageSource::Resource(resource) => window
                .use_asset::<gpui_kit::ImgResourceLoader>(resource, cx)
                .and_then(|image| image.ok()),
            _ => None,
        });
        let cover_frame = std::rc::Rc::new(std::cell::Cell::new(0));
        let available = if online {
            self.online.is_some()
                && online_state.play_all.is_none()
                && matches!(&online_state.songs, Remote::Ready(songs) if !songs.is_empty())
        } else {
            matches!(&self.album_songs, Remote::Ready(songs) if !songs.is_empty() && songs.iter().all(|song| self.local_song_playable(&song.id)))
        };
        let total = if online {
            online_state.total
        } else {
            match &self.album_songs {
                Remote::Ready(songs) => songs.len(),
                _ => 0,
            }
        };
        let title = online_header
            .map(|header| header.title.clone())
            .or_else(|| album.map(|a| a.name.clone()))
            .unwrap_or_else(|| self.t("music.list.title"));
        let desc = online_header
            .map(|header| {
                if header.description.is_empty() {
                    format!("By {}", header.source)
                } else {
                    header.description.clone()
                }
            })
            .or_else(|| {
                album
                    .filter(|a| !a.artist.is_empty())
                    .map(|a| a.artist.clone())
            })
            .unwrap_or_else(|| {
                if self.navigation.current() == Page::Playlist {
                    "By local".into()
                } else {
                    "By subsonic".into()
                }
            });
        let medium = !narrow && window.viewport_size().width <= px(1120.);
        let side = if narrow {
            if compact { 52. } else { 112. }
        } else if medium {
            160.
        } else {
            192.
        };
        let information = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(narrow, |title| {
                        title
                            .whitespace_normal()
                            .line_clamp(if compact { 1 } else { 2 })
                    })
                    .text_size(px(if narrow {
                        if compact {
                            rem
                        } else {
                            (f32::from(window.viewport_size().width) * 0.055)
                                .clamp(1.2 * rem, 1.55 * rem)
                        }
                    } else if compact {
                        25.
                    } else {
                        34.
                    }))
                    .line_height(relative(if narrow { 1.12 } else { 1. }))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .mb(px(if narrow { 0.35 * rem } else { 0.5 * rem }))
                    .child(title),
            )
            .when(!compact, |info| {
                info.child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .text_ellipsis()
                        .line_clamp(if narrow { 1 } else { 2 })
                        .text_size(px(if narrow { 0.8 * rem } else { rem }))
                        .line_height(relative(if narrow { 1.35 } else { 1.5 }))
                        .mb(px(if narrow { 0.25 * rem } else { 0.5 * rem }))
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(desc),
                )
            })
            .when(!compact || narrow, |info| {
                info.child(
                    div()
                        .text_size(px(if narrow { 12. } else { 0.875 * rem }))
                        .pt(px(if narrow { 0. } else { 5. }))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(
                            self.t("common.songCount")
                                .replace("{count}", &total.to_string()),
                        ),
                )
            });
        let local = self.navigation.current() == Page::Playlist;
        let opacity = gpui_kit::base::motion::transition(
            ("playlist-cover-edit", "opacity"),
            if local && self.song_view.cover_hovered {
                1_f32
            } else {
                0.
            },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(200))
                .ease(|t| player_motion::easing(t, [0.25, 0.1, 0.25, 1.])),
            window,
            cx,
        );
        let corners = Corners::all(px(if narrow { 12. } else { 0.5 * rem }));
        let zoom = 1. + 0.05 * opacity;
        let image = img(cover)
            .id("detail-cover-image")
            .size(px(side))
            .rounded(corners.top_left)
            .image_scale(zoom)
            .on_frame({
                let frame = cover_frame.clone();
                move |index| frame.set(index)
            })
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                img("images/default-cover.png")
                    .size(px(side))
                    .rounded(corners.top_left)
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            });
        let cover = div()
            .size(px(side))
            .flex_shrink_0()
            .relative()
            .overflow_hidden()
            .rounded(corners.top_left)
            .child(image)
            .when(local, |cover| {
                cover.child(
                    div()
                        .absolute()
                        .inset_0()
                        .size_full()
                        .bg(black().opacity(0.5))
                        .rounded(corners.top_left)
                        .opacity(opacity)
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .p(px(8.))
                        .text_color(white())
                        .child(icon("Playlist-Edit-Cover").size(px(24.)).mb(px(4.)))
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(relative(1.2))
                                .font_weight(FontWeight::MEDIUM)
                                .text_center()
                                .child(self.t("music.list.editCover")),
                        ),
                )
            });
        let cover = if local {
            gpui_kit::base::Button::new("edit-local-playlist-cover")
                .accessibility_label(self.t("music.list.editCover"))
                .disabled(self.favorites.busy || self.store.is_none())
                .size(px(side))
                .flex_shrink_0()
                .child(cover)
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.song_view.cover_hovered = *hovered;
                    cx.notify();
                }))
                .on_click(cx.listener(|this, _, window, cx| this.choose_playlist_cover(window, cx)))
                .into_any_element()
        } else {
            cover.into_any_element()
        };
        let play = Button::new("album-play-all")
            .primary()
            .accessibility_label(self.t("common.playAll"))
            .when(!narrow || !compact, |button| {
                button.label(self.t("common.playAll"))
            })
            .icon(glyph("bofang"))
            .disabled(!available)
            .h(px(if narrow { 44. } else { 36. }))
            .rounded(px(if narrow { 999. } else { 8. }))
            .min_w(px(if narrow { 0. } else { 120. }))
            .when(narrow && !compact, |button| button.flex_1())
            .when(narrow && compact, |button| {
                button.w(px(44.)).flex_shrink_0()
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if online {
                    this.confirm_online_playlist_play(false, window, cx)
                } else {
                    this.confirm_album_play(false, window, cx)
                }
            }));
        let shuffle = Button::new("album-shuffle-all")
            .outline()
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(transparent_black())
                    .foreground(color(mode, "--td-text-color-primary"))
                    .hover(color(mode, "--td-bg-color-container-hover"))
                    .active(color(mode, "--td-bg-color-container-active")),
            )
            .border_color(color(mode, "--td-component-border"))
            .accessibility_label(self.t("common.shufflePlay"))
            .when(!narrow || !compact, |button| {
                button.label(self.t("common.shufflePlay"))
            })
            .icon(icon("Shuffle"))
            .disabled(!available)
            .h(px(if narrow { 44. } else { 36. }))
            .rounded(px(if narrow { 999. } else { 8. }))
            .min_w(px(if narrow { 0. } else { 120. }))
            .when(narrow && !compact, |button| button.flex_1())
            .when(narrow && compact, |button| {
                button.w(px(44.)).flex_shrink_0()
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if online {
                    this.confirm_online_playlist_play(true, window, cx)
                } else {
                    this.confirm_album_play(true, window, cx)
                }
            }));
        let search_input = Styled::h(
            Input::new(&self.song_view.filter)
                .cleanable(true)
                .aria_label(self.t("common.searchPlaylist")),
            px(if narrow { 44. } else { 36. }),
        )
        .rounded(px(999.))
        .prefix(icon("Search").size(px(16.)))
        .bg(color(mode, "--td-bg-color-container").opacity(if narrow { 0.88 } else { 0.84 }))
        .w_full()
        .text_size(px(0.875 * rem));
        let search = div()
            .relative()
            .min_w_0()
            .h(px(if narrow { 44. } else { 36. }))
            .when(cfg!(target_os = "macos"), |search| {
                search.child(
                    canvas(
                        |_, _, _| (),
                        |bounds, _, window, _| {
                            #[cfg(target_os = "macos")]
                            window.paint_backdrop(bounds, Corners::all(px(999.)), 12., 1.);
                            #[cfg(not(target_os = "macos"))]
                            let _ = (bounds, window);
                        },
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
            })
            .child(search_input);
        let more = || {
            Button::new("album-more-actions")
                .custom(
                    ButtonCustomVariant::new(cx)
                        .foreground(color(mode, "--td-text-color-primary"))
                        .color(color(mode, "--td-bg-color-container"))
                        .hover(color(mode, "--td-bg-color-component-hover"))
                        .active(color(mode, "--td-bg-color-component-active")),
                )
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .icon(icon("TD-Ellipsis"))
                .accessibility_label(if self.english {
                    "More actions"
                } else {
                    "更多操作"
                })
                .size(px(if narrow { 44. } else { 36. }))
                .rounded(px(if narrow { 999. } else { 8. }))
                .flex_shrink_0()
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    if online {
                        this.online_playlist_more_actions(event.position(), window, cx)
                    } else {
                        this.song_more_actions(event.position(), window, cx)
                    }
                }))
        };
        let primary_actions = div()
            .flex()
            .justify_end()
            .items_center()
            .gap(px(if narrow { 8. } else { 0.75 * rem }))
            .child(play)
            .child(shuffle)
            .when(!narrow || compact, |row| row.child(more()));
        let actions = div()
            .flex()
            .flex_col()
            .gap(px(if narrow { 8. } else { 0.75 * rem }))
            .child(primary_actions)
            .when(!narrow || !compact, |actions| {
                actions.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .when(narrow, |row| row.child(more()))
                        .when(!narrow, |row| row.justify_end())
                        .child(
                            div()
                                .min_w_0()
                                .when(narrow, |field| field.flex_1())
                                .when(!narrow, |field| field.w(px(260.)))
                                .child(search),
                        ),
                )
            });
        let corners = if narrow && compact {
            Corners::all(px(14.))
        } else if narrow {
            Corners {
                top_left: px(0.),
                top_right: px(0.),
                bottom_left: px(20.),
                bottom_right: px(20.),
            }
        } else {
            Corners::all(px(0.75 * rem))
        };
        let container = color(mode, "--td-bg-color-container");
        let header = div()
            .relative()
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .bg(container)
            .mb(px(if narrow { 10. } else { 20. }))
            .rounded(px(if narrow && compact {
                14.
            } else if narrow {
                0.
            } else {
                0.75 * rem
            }))
            .when(narrow && !compact, |header| header.rounded_b(px(20.)))
            .p(px(if narrow {
                if compact { 8. } else { 12. }
            } else {
                if compact { rem } else { 1.5 * rem }
            }))
            .when(narrow, |header| {
                header
                    .pt(px(if compact { 8. } else { 12. }))
                    .when(compact, |header| header.pl(px(62.)))
            })
            .shadow(vec![if narrow {
                BoxShadow::new(
                    px(0.),
                    px(if compact { 10. } else { 14. }),
                    rgb(0x0f172a).opacity(0.12).into(),
                )
                .blur_radius(px(if compact { 24. } else { 34. }))
            } else {
                BoxShadow::new(px(0.), px(2.), black().opacity(0.08)).blur_radius(px(8.))
            }])
            .when(narrow && cfg!(target_os = "macos"), |header| {
                header.bg(transparent_black()).child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            #[cfg(target_os = "macos")]
                            window.paint_backdrop(bounds, corners, 14., 1.8);
                            window.paint_quad(fill(bounds, container).corner_radii(corners));
                        },
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
            })
            .child(album_backdrop::Layer {
                state: self.song_view.backdrop.clone(),
                cover: cover_image,
                frame: cover_frame,
                narrow,
                dark: self.mode == Mode::Dark
                    || matches!(
                        window.appearance(),
                        WindowAppearance::Dark | WindowAppearance::VibrantDark
                    ),
                corners,
            });
        if narrow && compact {
            header
                .flex()
                .items_center()
                .gap(px(12.))
                .child(cover)
                .child(information)
                .child(actions)
                .into_any_element()
        } else if narrow {
            header
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(cover)
                        .child(information),
                )
                .child(actions)
                .into_any_element()
        } else if medium {
            header
                .min_h(px(if compact { 120. } else { 220. }))
                .flex()
                .items_center()
                .gap(px(rem * 1.5))
                .child(cover)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(rem))
                        .child(information)
                        .child(actions),
                )
                .into_any_element()
        } else {
            header
                .h(px(if compact { 120. } else { 240. }))
                .flex()
                .items_center()
                .gap(px(rem * 1.5))
                .child(cover)
                .child(information)
                .child(actions.w(px(420.)).min_w(px(240.)))
                .into_any_element()
        }
    }

    fn song_checkbox(
        &self,
        id: impl Into<ElementId>,
        label: String,
        checked: bool,
    ) -> gpui_kit::base::Checkbox {
        let colors = self.ui_colors();
        gpui_kit::base::Checkbox::new(id)
            .accessibility_label(label)
            .checked(checked)
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
            .child(
                gpui_kit::base::CheckboxIndicator::new()
                    .checked(checked)
                    .size(px(16.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(color(colors, "--td-border-level-2-color"))
                    .bg(transparent_black())
                    .flex()
                    .items_center()
                    .justify_center()
                    .styles(|styles| {
                        styles.checked(|style| {
                            style
                                .bg(color(colors, "--td-brand-color"))
                                .border_color(color(colors, "--td-brand-color"))
                        })
                    })
                    .when(checked, |indicator| {
                        indicator.child(
                            icon("TD-Check")
                                .size(px(12.))
                                .text_color(color(colors, "--td-text-color-anti")),
                        )
                    }),
            )
    }

    fn batch_song_columns(&self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.ui_colors();
        let songs = self.filtered_songs(cx);
        let selected: Vec<_> = songs
            .iter()
            .filter(|song| self.song_view.selected.contains(&song.id))
            .cloned()
            .collect();
        let all = !songs.is_empty() && selected.len() == songs.len();
        let playable = !selected.is_empty()
            && selected
                .iter()
                .all(|song| self.local_song_playable(&song.id));
        let count = self
            .t("music.songList.selectedCount")
            .replace("{count}", &selected.len().to_string());
        let left = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(if narrow { 6. } else { 8. }))
            .child(
                self.song_checkbox("select-all-songs", self.t("music.songList.selectAll"), all)
                    .child(self.t("music.songList.selectAll"))
                    .pl(px(if narrow { 4. } else { 15. }))
                    .text_size(px(12.))
                    .on_change({
                        let activate = cx.listener(|this, _: &ClickEvent, _, cx| {
                            let songs = this.filtered_songs(cx);
                            let all = !songs.is_empty()
                                && songs
                                    .iter()
                                    .all(|song| this.song_view.selected.contains(&song.id));
                            if all {
                                this.song_view.selected.clear();
                            } else {
                                this.song_view.selected =
                                    songs.iter().map(|song| song.id.clone()).collect();
                            }
                            cx.notify();
                        });
                        move |_, event, window, cx| activate(event, window, cx)
                    }),
            )
            .child(
                control(
                    "play-selected-songs",
                    "TD-PlayCircle",
                    self.t("music.songList.play"),
                    colors,
                    cx,
                )
                .icon(glyph("bofang").size(px(14.)))
                .disabled(!playable)
                .size(px(if narrow { 44. } else { 32. }))
                .rounded(px(if narrow { 999. } else { 8. }))
                .bg(color(colors, "--td-brand-color"))
                .text_color(color(colors, "--td-text-color-anti"))
                .on_click(cx.listener(|this, _, window, cx| {
                    let songs: Vec<_> = this
                        .filtered_songs(cx)
                        .iter()
                        .filter(|song| this.song_view.selected.contains(&song.id))
                        .cloned()
                        .collect();
                    if songs.is_empty()
                        || songs.iter().any(|song| !this.local_song_playable(&song.id))
                    {
                        return;
                    }
                    let qualities = songs
                        .iter()
                        .filter_map(|song| {
                            this.display_song_quality(&song.id)
                                .map(|q| (song.id.clone(), q))
                        })
                        .collect();
                    let count = songs.len();
                    this.replace_player_queue(songs);
                    this.player.queue_qualities = qualities;
                    this.play_index(0, cx);
                    window.push_notification(
                        gpui_kit::component::notification::Notification::success(
                            this.t("music.list.replacedPlaylist")
                                .replace("{count}", &count.to_string()),
                        ),
                        cx,
                    );
                })),
            )
            .child(
                Button::new("download-selected-songs")
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(color(colors, "--td-bg-color-component-disabled"))
                            .foreground(color(colors, "--td-text-color-primary").opacity(0.35)),
                    )
                    .label(self.t("music.songList.batchDownload"))
                    .disabled(true)
                    .h(px(if narrow { 44. } else { 32. }))
                    .rounded(px(if narrow { 999. } else { 8. }))
                    .px(px(12.))
                    .text_size(px(12.)),
            );
        let right = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(if narrow { 6. } else { 8. }))
            .child(div().text_size(px(12.)).child(count))
            .when(self.navigation.current() == Page::Playlist, |row| {
                let disabled = self.song_view.selected.is_empty()
                    || self.favorites.busy
                    || self.store.is_none();
                let error = color(
                    colors,
                    if disabled {
                        "--td-error-color-disabled"
                    } else {
                        "--td-error-color"
                    },
                );
                row.child(
                    gpui_kit::base::Button::new("remove-selected-songs")
                        .accessibility_label(self.t("music.songList.removeFromPlaylist"))
                        .disabled(disabled)
                        .w(px(if narrow { 44. } else { 40. }))
                        .h(px(if narrow { 44. } else { 32. }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(if narrow { 999. } else { 8. }))
                        .border_1()
                        .border_color(error)
                        .text_color(error)
                        .when(!disabled, |button| {
                            button.hover(|button| {
                                button
                                    .border_color(color(colors, "--td-error-color-hover"))
                                    .text_color(color(colors, "--td-error-color-hover"))
                            })
                        })
                        .child(icon("TD-Delete").size(px(14.)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            let songs = this
                                .filtered_songs(cx)
                                .iter()
                                .filter(|song| this.song_view.selected.contains(&song.id))
                                .cloned()
                                .collect();
                            this.remove_local_playlist_songs(songs, true, window, cx);
                        })),
                )
            })
            .child(
                Button::new("finish-selecting-songs")
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .foreground(color(colors, "--td-text-color-primary"))
                            .hover(color(colors, "--td-bg-color-component-hover")),
                    )
                    .border_1()
                    .border_color(color(colors, "--td-border-level-2-color"))
                    .rounded(px(if narrow { 999. } else { 8. }))
                    .label(self.t("music.songList.done"))
                    .h(px(if narrow { 44. } else { 32. }))
                    .px(px(12.))
                    .text_size(px(12.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.song_view.reset_selection();
                        cx.notify();
                    })),
            );
        div()
            .w_full()
            .flex_shrink_0()
            .min_h(px(if narrow { 56. } else { 48. }))
            .px(px(if narrow { 4. } else { 10. }))
            .when(narrow, |row| row.py(px(6.)))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .overflow_hidden()
            .bg(color(colors, "--song-list-header-bg"))
            .border_b_1()
            .border_color(color(colors, "--song-list-header-border"))
            .text_color(color(colors, "--song-list-header-text"))
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn sortable_song_header(
        &self,
        column: song_sort::Column,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use song_sort::{Column, Sort};
        let (id, key) = match column {
            Column::Title => ("sort-song-title", "music.songList.title"),
            Column::Album => ("sort-song-album", "music.songList.album"),
            Column::Duration => ("sort-song-duration", "music.songList.duration"),
        };
        let title = self.t(key);
        let show_title = !(narrow && column == Column::Duration);
        if self.navigation.current() != Page::Playlist {
            return div()
                .when(show_title, |cell| cell.child(title))
                .into_any_element();
        }
        let colors = self.ui_colors();
        let active = self.song_view.sort.column() == Some(column);
        let state = if active {
            self.song_view.sort
        } else {
            Sort::Default
        };
        let show_hint = active || self.song_view.hovered_sort == Some(column);
        let hint = self.t(state.label_key());
        let hint_text = column == Column::Title || (column == Column::Album && !active);
        let icon_name = if !active {
            "TD-Time"
        } else if state.descending() {
            "TD-Arrow-Down"
        } else {
            "TD-Arrow-Up"
        };
        gpui_kit::base::Button::new(id)
            .accessibility_label(if column == Column::Title {
                format!("{title} · {hint}")
            } else {
                title.clone()
            })
            .w_full()
            .min_w_0()
            .h(px(if narrow { 44. } else { 40. }))
            .flex()
            .items_center()
            .when(column == Column::Duration, |cell| cell.justify_center())
            .when(column != Column::Duration, |cell| cell.justify_start())
            .overflow_hidden()
            .font_weight(FontWeight::NORMAL)
            .text_size(px(12.))
            .line_height(px(18.))
            .text_color(color(colors, "--song-list-header-text"))
            .hover(|cell| cell.text_color(color(colors, "--song-list-title-hover")))
            .when(show_title, |cell| cell.child(title))
            .when(show_hint, |cell| {
                cell.child(
                    div()
                        .ml(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .flex_shrink_0()
                        .text_color(color(
                            colors,
                            if active {
                                "--song-list-title-hover"
                            } else {
                                "--song-list-header-text"
                            },
                        ))
                        .child(
                            icon(icon_name)
                                .size(px(14.))
                                .when(hint_text, |icon| icon.mr(px(2.4))),
                        )
                        .when(hint_text, |hint_row| hint_row.child(hint.clone())),
                )
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.song_view.hovered_sort = hovered.then_some(column);
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_context_actions(window, cx);
                this.song_view.sort = this.song_view.sort.cycle(column);
                cx.notify();
            }))
            .into_any_element()
    }

    fn song_columns(&self, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        div()
            .w_full()
            .flex_shrink_0()
            .h(px(if narrow { 44. } else { 40. }))
            .px(px(if narrow { 4. } else { 10. }))
            .flex()
            .items_center()
            .overflow_hidden()
            .bg(color(mode, "--song-list-header-bg"))
            .border_b_1()
            .border_color(color(mode, "--song-list-header-border"))
            .text_size(px(12.))
            .text_color(color(mode, "--song-list-header-text"))
            .child(
                div()
                    .w(px(if narrow { 44. } else { 50. }))
                    .flex_shrink_0()
                    .text_center()
                    .child("#"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(if narrow { 4. } else { 10. }))
                    .child(self.sortable_song_header(song_sort::Column::Title, narrow, cx)),
            )
            .when(!narrow, |row| {
                row.child(
                    div()
                        .w(px(200.))
                        .flex_shrink_0()
                        .px(px(10.))
                        .child(self.sortable_song_header(song_sort::Column::Album, narrow, cx)),
                )
            })
            .child(
                div()
                    .w(px(if narrow { 44. } else { 60. }))
                    .flex_shrink_0()
                    .text_center()
                    .child(self.t("music.songList.liked")),
            )
            .child(
                div()
                    .w(px(if narrow { 52. } else { 80. }))
                    .flex_shrink_0()
                    .text_center()
                    .child(self.sortable_song_header(song_sort::Column::Duration, narrow, cx)),
            )
    }

    pub(super) fn online_song_labels(&self, song: &Song) -> (Option<String>, Option<String>) {
        let Some(online) = self.online.as_ref() else {
            return (None, None);
        };
        let Some(item) = online.item(&song.id) else {
            return (None, None);
        };
        let name = mio_music_sdk::provider(&item.source).map(|provider| provider.name().to_owned());
        let qualities = online.source_qualities(&item.source);
        let quality = crate::quality::highest_supported(&item, &qualities).map(|quality| {
            crate::quality::display_key(quality)
                .map(|key| self.t(key))
                .unwrap_or_else(|| quality.to_owned())
        });
        (name, quality)
    }

    pub(super) fn online_song_badges(&self, song: &Song) -> AnyElement {
        let (plugin, quality) = self.online_song_labels(song);
        let mode = self.ui_colors();
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_shrink_0()
            .children(
                [
                    (
                        quality,
                        "--song-list-quality-bg",
                        "--song-list-quality-color",
                    ),
                    (plugin, "--song-list-source-bg", "--song-list-source-color"),
                ]
                .into_iter()
                .filter_map(|(label, bg, fg)| {
                    label.map(|label| {
                        div()
                            .px(px(6.))
                            .py(px(2.))
                            .rounded(px(4.))
                            .text_size(px(10.))
                            .line_height(px(10.))
                            .bg(color(mode, bg))
                            .text_color(color(mode, fg))
                            .child(label)
                    })
                }),
            )
            .into_any_element()
    }

    pub(super) fn online_song_accessibility_label(&self, song: &Song) -> String {
        let (plugin, quality) = self.online_song_labels(song);
        [
            Some(song.title.clone()),
            quality,
            plugin,
            Some(song.artist.clone()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    pub(super) fn song_row_thumbnail(&self, song: &Song) -> AnyElement {
        img(self.song_row_cover(song))
            .size(px(40.))
            .rounded(px(4.))
            .object_fit(ObjectFit::Cover)
            .flex_shrink_0()
            .mr(px(8.))
            .with_fallback(|| {
                img("images/default-cover.png")
                    .size(px(40.))
                    .rounded(px(4.))
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            })
            .into_any_element()
    }

    pub(super) fn song_row_cover(&self, song: &Song) -> ImageSource {
        let cover_art = self
            .online
            .as_ref()
            .and_then(|online| online.item(&song.id))
            .map(|item| item.img)
            .filter(|url| !url.is_empty())
            .or_else(|| song.cover_art.clone());
        let cached = cover_art.as_ref().and_then(|id| {
            self.song_view
                .album_covers
                .images
                .get(id)
                .or_else(|| self.song_view.search_covers.images.get(id))
                .and_then(Clone::clone)
        });
        cached
            .map(ImageSource::from)
            .or_else(|| {
                cover_art
                    .as_ref()
                    .filter(|url| {
                        reqwest::Url::parse(url).is_ok_and(|url| {
                            matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
                        })
                    })
                    .cloned()
                    .map(ImageSource::from)
            })
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"))
    }

    fn song_row(
        &self,
        song: &Song,
        index: usize,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let playable = self.local_song_playable(&song.id);
        let song_source = if self.navigation.current() == Page::Playlist {
            self.local_song_source(&song.id)
        } else {
            "Subsonic".to_owned()
        };
        let playing = playable
            && self
                .player
                .song()
                .is_some_and(|active| active.id == song.id);
        let song_to_play = song.clone();
        let cover = self.song_row_cover(song);
        let title = song.title.clone();
        let qualities = if self.navigation.current().is_song_detail() {
            &self.song_view.album_qualities
        } else {
            &self.song_view.search_qualities
        };
        let track_quality = qualities.get(&song.id).copied();
        let (plugin_label, online_quality) = self.online_song_labels(song);
        let is_online = self
            .online
            .as_ref()
            .and_then(|online| online.item(&song.id))
            .is_some();
        let song_source = plugin_label.unwrap_or(song_source);
        let quality = if is_online {
            online_quality
        } else {
            track_quality.map(|quality| {
                self.t(match quality {
                    crate::song::SongQuality::Lossless => "quality.displayFlac",
                    crate::song::SongQuality::High => "quality.display320k",
                    crate::song::SongQuality::Standard => "quality.display128k",
                })
            })
        };
        let activate = Rc::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
            if this.song_view.multi {
                if !this.song_view.selected.remove(&song_to_play.id) {
                    this.song_view.selected.insert(song_to_play.id.clone());
                }
                cx.notify();
                return;
            }
            if !this.local_song_playable(&song_to_play.id) {
                return;
            }
            let index = this.enqueue_player_first(song_to_play.clone());
            if let Some(quality) = track_quality {
                this.player
                    .queue_qualities
                    .insert(song_to_play.id.clone(), quality);
            }
            this.play_index(index, cx);
        }));
        let row_group = SharedString::from(format!("song-row-group-{index}"));
        let play = gpui_kit::base::Button::new(SharedString::from(format!("track-play-{index}")))
            .disabled(!playable)
            .accessibility_label(format!(
                "{} · {}",
                self.t("music.songList.play"),
                song.title
            ))
            .size(px(if narrow { 44. } else { 32. }))
            .rounded_full()
            .text_color(color(mode, "--td-brand-color"))
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .when(!narrow && !playing, |button| {
                button.child(
                    div()
                        .text_size(px(14.))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .group_hover(row_group.clone(), |style| style.opacity(0.))
                        .child(format!("{:02}", index + 1)),
                )
            })
            .child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!narrow && !playing, |play| play.opacity(0.))
                    .group_hover(row_group.clone(), |style| style.opacity(1.))
                    .child(glyph("bofang").text_size(px(16.))),
            )
            .on_click({
                let activate = activate.clone();
                move |event, window, cx| activate(event, window, cx)
            });
        div()
            .id(SharedString::from(format!("track-row-{index}")))
            .group(row_group.clone())
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let song = song.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.song_context_actions(song.clone(), event.position, window, cx);
                    }
                }),
            )
            .w_full()
            .h(px(64.))
            .flex()
            .items_center()
            .min_w_0()
            .px(px(if narrow { 4. } else { 10. }))
            .py(px(8.))
            .border_b_1()
            .border_color(color(mode, "--song-list-item-border"))
            .bg(if playing {
                color(mode, "--song-list-item-playing")
            } else {
                transparent_black()
            })
            .hover(move |row| {
                row.bg(if playing {
                    color(mode, "--song-list-item-playing")
                } else {
                    color(mode, "--song-list-item-hover")
                })
            })
            .child(
                div()
                    .w(px(if narrow { 44. } else { 50. }))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(if self.song_view.multi {
                        self.song_checkbox(
                            ("select-song", index),
                            format!("{} · {}", self.t("common.batchSelect"), song.title),
                            self.song_view.selected.contains(&song.id),
                        )
                        .on_change({
                            let activate = cx.listener({
                                let id = song.id.clone();
                                move |this, _, _, cx| {
                                    if !this.song_view.selected.remove(&id) {
                                        this.song_view.selected.insert(id.clone());
                                    }
                                    cx.notify();
                                }
                            });
                            move |_, event, window, cx| activate(event, window, cx)
                        })
                        .into_any_element()
                    } else {
                        play.into_any_element()
                    }),
            )
            .child(
                gpui_kit::base::Button::new(SharedString::from(format!("track-title-{index}")))
                    .disabled(!playable && !self.song_view.multi)
                    .accessibility_label(match &quality {
                        Some(quality) => {
                            format!(
                                "{} · {} · {} · {}",
                                song.title, quality, song_source, song.artist
                            )
                        }
                        None => format!("{} · {} · {}", song.title, song_source, song.artist),
                    })
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .pl(px(if narrow { 4. } else { 10. }))
                    .gap(px(10.))
                    .child(
                        img(cover)
                            .size(px(40.))
                            .object_fit(ObjectFit::Cover)
                            .rounded(px(4.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_size(px(14.))
                                    .line_height(px(16.8))
                                    .font_weight(FontWeight::MEDIUM)
                                    .mb(px(4.))
                                    .text_color(color(
                                        mode,
                                        if playing {
                                            "--td-brand-color"
                                        } else {
                                            "--song-list-title-color"
                                        },
                                    ))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .text_size(px(12.))
                                    .line_height(px(14.4))
                                    .text_color(color(mode, "--song-list-artist-color"))
                                    .when_some(quality, |row, quality| {
                                        row.child(
                                            div()
                                                .flex_shrink_0()
                                                .px(px(6.))
                                                .py(px(2.))
                                                .rounded(px(4.))
                                                .text_size(px(10.))
                                                .line_height(px(10.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .bg(color(mode, "--song-list-quality-bg"))
                                                .text_color(color(
                                                    mode,
                                                    "--song-list-quality-color",
                                                ))
                                                .child(quality),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .px(px(6.))
                                            .py(px(2.))
                                            .rounded(px(4.))
                                            .text_size(px(10.))
                                            .line_height(px(10.))
                                            .bg(color(mode, "--song-list-source-bg"))
                                            .text_color(color(mode, "--song-list-source-color"))
                                            .child(song_source.clone()),
                                    )
                                    .child(
                                        div().flex_1().min_w_0().overflow_hidden().flex().child(
                                            self.song_artist_label(
                                                song,
                                                SharedString::from(format!("song-singer-{index}")),
                                                cx,
                                            ),
                                        ),
                                    ),
                            ),
                    )
                    .on_click(move |event, window, cx| activate(event, window, cx)),
            )
            .when(!narrow, |row| {
                row.child(
                    div()
                        .w(px(200.))
                        .flex_shrink_0()
                        .px(px(10.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(px(12.))
                        .text_color(color(mode, "--song-list-album-color"))
                        .child(song.album.clone()),
                )
            })
            .child(
                div()
                    .w(px(if narrow { 44. } else { 60. }))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        self.favorite_button(
                            SharedString::from(format!("track-like-{index}")),
                            playable.then_some(song),
                            true,
                            cx,
                        )
                        .size(px(if narrow { 44. } else { 32. })),
                    ),
            )
            .child(
                div()
                    .w(px(if narrow { 52. } else { 80. }))
                    .h(px(44.))
                    .relative()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(color(mode, "--td-text-color-placeholder"))
                    .when(!narrow, |cell| {
                        cell.child(
                            div()
                                .group_hover(row_group.clone(), |duration| duration.opacity(0.))
                                .child(format!(
                                    "{:02}:{:02}",
                                    song.duration / 60,
                                    song.duration % 60
                                )),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap(px(if narrow { 0. } else { 2. }))
                            .when(!narrow, |actions| {
                                actions
                                    .absolute()
                                    .size_full()
                                    .opacity(0.)
                                    .group_hover(row_group.clone(), |actions| actions.opacity(1.))
                            })
                            .child(
                                control(
                                    SharedString::from(format!("track-download-{index}")),
                                    "TD-Download",
                                    self.t("music.songList.download"),
                                    mode,
                                    cx,
                                )
                                .disabled(true)
                                .w(px(if narrow { 26. } else { 28. }))
                                .h(px(if narrow {
                                    44.
                                } else {
                                    28.
                                })),
                            )
                            .child(
                                control(
                                    SharedString::from(format!("track-add-{index}")),
                                    "Add",
                                    self.t("music.songList.addToPlaylist"),
                                    mode,
                                    cx,
                                )
                                .icon(glyph("zengjia").size(px(14.)))
                                .disabled(!playable || self.store.is_none() || self.favorites.busy)
                                .w(px(if narrow { 26. } else { 28. }))
                                .h(px(if narrow { 44. } else { 28. }))
                                .on_click(cx.listener({
                                    let song = song.clone();
                                    move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_add_to_playlist(vec![song.clone()], window, cx);
                                    }
                                })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn confirm_album_play(&self, shuffle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let songs = self.filtered_songs(cx);
        if songs.is_empty() || !songs.iter().all(|song| self.local_song_playable(&song.id)) {
            return;
        }
        let revision = self.album_song_revision;
        let queue_revision = self.player.queue_revision;
        let album = self.selected_album.as_ref().map(|album| album.id.clone());
        let view = cx.entity().downgrade();
        let title = self.t(if shuffle {
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
            .replace(
                "{title}",
                self.selected_album
                    .as_ref()
                    .map(|a| a.name.as_str())
                    .unwrap_or(""),
            )
            .replace("{count}", &songs.len().to_string());
        let confirm = self.t("music.list.confirmReplace");
        let cancel = self.t("common.cancel");
        let close = self.t("common.titleBar.close");
        let colors = self.ui_colors();
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let brand = color(colors, "--td-brand-color");
        let cancel_style = ButtonCustomVariant::new(cx)
            .color(primary.opacity(0.06))
            .foreground(primary)
            .hover(primary.opacity(0.12))
            .active(primary.opacity(0.12));
        let confirm_style = ButtonCustomVariant::new(cx)
            .color(brand.opacity(0.16))
            .foreground(brand)
            .hover(brand.opacity(0.22))
            .active(brand.opacity(0.22));
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let gutter = (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.);
            let view = view.clone();
            let album = album.clone();
            let songs = songs.clone();
            dialog
                .w(px(if narrow {
                    (f32::from(window.viewport_size().width) - 2. * gutter).min(440.)
                } else {
                    (f32::from(window.viewport_size().width) - 32.).min(420.)
                }))
                .margin_top((window.viewport_size().height - px(250.)) / 2.)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(22.))
                .p(px(if narrow { 16. } else { 28. }))
                .when(narrow, |dialog| dialog.pt(px(12.)))
                .gap(px(0.))
                .bg(primary.opacity(0.02))
                .border_color(white().opacity(0.4))
                .overlay_tint(if narrow {
                    color(colors, "--mobile-scrim")
                } else {
                    black().opacity(0.45)
                })
                .when(cfg!(target_os = "macos"), |dialog| {
                    dialog
                        .overlay_filter(
                            if narrow { 12. } else { 8. },
                            if narrow { 1.8 } else { 1.4 },
                        )
                        .surface_filter(6.56, 1.8, px(22.))
                })
                .close_button(false)
                // The source confirms only an activated button; Enter on the dialog is inert.
                .on_ok(|_, _, _| false)
                .title(
                    div()
                        .flex()
                        .flex_col()
                        .when(narrow, |header| {
                            header.child(
                                div()
                                    .w(px(38.))
                                    .h(px(4.))
                                    .flex_shrink_0()
                                    .mb(px(12.))
                                    .mx_auto()
                                    .rounded_full()
                                    .bg(rgb(0x787880).opacity(0.36)),
                            )
                        })
                        .mb(px(if narrow { 6. } else { 10. }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .h(px(if narrow { 44. } else { 42. }))
                                .child(
                                    div()
                                        .size(px(if narrow { 38. } else { 42. }))
                                        .flex_shrink_0()
                                        .rounded(px(if narrow { 12. } else { 13. }))
                                        .border_1()
                                        .border_color(primary.opacity(0.08))
                                        .bg(linear_gradient(
                                            135.,
                                            linear_color_stop(brand.opacity(0.18), 0.),
                                            linear_color_stop(rgb(0x8c50ff).opacity(0.12), 1.),
                                        ))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(brand)
                                        .child(
                                            if shuffle {
                                                icon("Shuffle")
                                            } else {
                                                glyph("bofang")
                                            }
                                            .size(px(20.)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_size(px(17.))
                                        .line_height(relative(1.2))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(title.clone()),
                                )
                                .child(
                                    gpui_kit::base::Button::new("close-album-confirm")
                                        .accessibility_label(close.clone())
                                        .size(px(if narrow { 44. } else { 30. }))
                                        .flex_shrink_0()
                                        .rounded(px(if narrow { 999. } else { 9. }))
                                        .border_1()
                                        .border_color(primary.opacity(0.08))
                                        .bg(primary.opacity(0.03))
                                        .text_color(secondary)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|button| {
                                            button
                                                .bg(rgb(0xff5050).opacity(0.15))
                                                .border_color(rgb(0xff5050).opacity(0.25))
                                                .text_color(color(colors, "--td-error-color"))
                                        })
                                        .child(icon("WindowClose").size(px(13.)))
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                ),
                        ),
                )
                .content({
                    let body = body.clone();
                    move |content, _, _| {
                        content
                            .font_weight(FontWeight::NORMAL)
                            .text_size(px(14.))
                            .line_height(relative(1.7))
                            .text_color(secondary)
                            .child(body.clone())
                    }
                })
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(if narrow { 8. } else { 10. }))
                        .mt(px(if narrow { 20. } else { 24. }))
                        .when(narrow, |footer| footer.flex_col_reverse())
                        .child(
                            Button::new("cancel-album-play")
                                .label(cancel.clone())
                                .border_1()
                                .custom(cancel_style)
                                .border_color(primary.opacity(0.08))
                                .min_w(px(92.))
                                .h(px(if narrow { 44. } else { 38. }))
                                .px(px(18.))
                                .rounded(px(if narrow { 999. } else { 12. }))
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(narrow, |button| button.w_full())
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("confirm-album-play")
                                .label(confirm.clone())
                                .border_1()
                                .custom(confirm_style)
                                .border_color(brand.opacity(0.32))
                                .min_w(px(92.))
                                .h(px(if narrow { 44. } else { 38. }))
                                .px(px(18.))
                                .rounded(px(if narrow { 999. } else { 12. }))
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(narrow, |button| button.w_full())
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if this.album_song_revision != revision
                                            || this.player.queue_revision != queue_revision
                                            || this.selected_album.as_ref().map(|a| a.id.clone())
                                                != album
                                        {
                                            return;
                                        }
                                        let mut songs = songs.as_ref().clone();
                                        if shuffle {
                                            for i in (1..songs.len()).rev() {
                                                let j = (uuid::Uuid::new_v4().as_u128()
                                                    % (i + 1) as u128)
                                                    as usize;
                                                songs.swap(i, j);
                                            }
                                        }
                                        let qualities = songs
                                            .iter()
                                            .filter_map(|song| {
                                                this.song_view
                                                    .album_qualities
                                                    .get(&song.id)
                                                    .map(|quality| (song.id.clone(), *quality))
                                            })
                                            .collect();
                                        this.replace_player_queue(songs);
                                        this.player.queue_qualities = qualities;
                                        this.play_index(0, cx);
                                    });
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }
}
