use super::*;
use gpui_kit::component::{WindowExt, notification::Notification};

impl Mio {
    fn load_search_more(&mut self, cx: &mut Context<Self>) {
        if self.song_view.search_online {
            self.load_online_search(true, cx);
        }
        // Library (Subsonic) search paging is gone; online search handled above.
    }

    pub(super) fn search_page(
        &self,
        narrow: bool,
        width: f32,
        rem: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let songs = self.filtered_songs(cx);
        let unavailable = (self.song_view.search_playlists && !self.song_view.search_online)
            || (if self.song_view.search_online {
                self.online.is_none()
            } else {
                true
            });
        let mut page = div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                20.
            }))
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                20.
            }))
            .pb(px(if narrow { 0. } else { 20. }))
            .bg(color(mode, "--search-bg"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .mb(px(if narrow { 6. } else { 16. }))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .text_size(px(if narrow { 18. } else { 24. }))
                            .text_color(color(mode, "--search-title-color"))
                            .when(!narrow, |el| {
                                el.border_l(px(4.))
                                    .border_color(color(mode, "--td-brand-color"))
                                    .pl(px(8.))
                            })
                            .child(format!("{}\"", self.t("music.search.searchKeyword")))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(color(mode, "--search-keyword-color"))
                                    .child(self.song_view.search_query.clone()),
                            )
                            .child("\""),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(color(mode, "--search-info-color"))
                            .child(if unavailable {
                                String::new()
                            } else if self.song_view.search_playlists {
                                match &self.song_view.playlists.results {
                                    Remote::Ready(cards) => self
                                        .t(if self.song_view.search_source.is_some() {
                                            "music.search.foundPlaylists"
                                        } else {
                                            "music.search.foundAggregatePlaylists"
                                        })
                                        .replace(
                                            "{count}",
                                            &if self.song_view.search_source.is_some() {
                                                self.song_view.playlists.total
                                            } else {
                                                cards.len()
                                            }
                                            .to_string(),
                                        ),
                                    _ => String::new(),
                                }
                            } else {
                                if self.song_view.search_online {
                                    format!("{} 首", self.online_total)
                                } else {
                                    self.t("music.search.libraryLoaded")
                                        .replace("{count}", &songs.len().to_string())
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(if narrow { 16. } else { 20. }))
                    .border_b_1()
                    .border_color(color(mode, "--theme-border"))
                    .mb(px(if narrow { 0. } else { 12. }))
                    .children([false, true].into_iter().map(|playlists| {
                        let selected = self.song_view.search_playlists == playlists;
                        gpui_kit::base::Button::new(if playlists {
                            "search-tab-playlists"
                        } else {
                            "search-tab-songs"
                        })
                        .accessibility_label(self.t(if playlists {
                            "music.search.tabPlaylists"
                        } else {
                            "music.search.tabSongs"
                        }))
                        .aria_selected(selected)
                        .font_weight(if selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .px(px(0.))
                        .py(px(10.))
                        .text_size(px(if narrow { 15. } else { 14. }))
                        .text_color(color(
                            mode,
                            if selected {
                                "--td-brand-color"
                            } else {
                                "--theme-text-secondary"
                            },
                        ))
                        .border_b(px(2.))
                        .border_color(if selected {
                            color(mode, "--td-brand-color")
                        } else {
                            transparent_black()
                        })
                        .child(self.t(if playlists {
                            "music.search.tabPlaylists"
                        } else {
                            "music.search.tabSongs"
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.song_view.search_playlists = playlists;
                            if this.song_view.search_online {
                                if playlists {
                                    this.ensure_playlist_search(cx);
                                } else if matches!(this.songs, Remote::Idle) {
                                    // A search issued on the playlist tab left songs unloaded.
                                    this.load_online_search(false, cx);
                                }
                            }
                            cx.notify();
                        }))
                    })),
            );
        // "All" (aggregate) first, then one chip per source the plugin and this build both support,
        // The reference excludes Subsonic from this selector.
        let mut chips: Vec<(SharedString, String, bool, Option<String>)> = Vec::new();
        chips.push((
            "search-source-all".into(),
            self.t("music.search.allSources"),
            true,
            None,
        ));
        if let Some(online) = &self.online {
            for source in online.sources() {
                chips.push((
                    format!("search-source-{}", source.id).into(),
                    if source.name.is_empty() {
                        source.id.clone()
                    } else {
                        source.name.clone()
                    },
                    true,
                    Some(source.id.clone()),
                ));
            }
        }
        page = page.child(
            div()
                .id("search-source-filters")
                .flex()
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .when(narrow, |row| {
                    row.overflow_x_scroll()
                        .overflow_y_hidden()
                        .restrict_scroll_to_axis()
                        .pt(px(6.))
                        .pb(px(2.))
                })
                .when(!narrow, |row| row.flex_wrap())
                .gap(px(if narrow { 6. } else { 8. }))
                .mb(px(if narrow { 6. } else { 12. }))
                .children(chips.into_iter().map(|(id, label, online, source)| {
                    let selected = self.song_view.search_online == online
                        && self.song_view.search_source == source;
                    gpui_kit::base::Button::new(id)
                        .aria_selected(selected)
                        .flex_shrink_0()
                        .accessibility_label(label.clone())
                        .px(px(14.))
                        .py(px(if narrow { 6. } else { 4. }))
                        .rounded(px(16.))
                        .text_size(px(if narrow { 12. } else { 13. }))
                        .min_h(px(if narrow { 32. } else { 0. }))
                        .bg(color(
                            mode,
                            if selected {
                                "--td-brand-color"
                            } else {
                                "--theme-bg-primary"
                            },
                        ))
                        .text_color(if selected {
                            white()
                        } else {
                            color(mode, "--theme-text-secondary")
                        })
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.song_view.search_online != online
                                || this.song_view.search_source != source
                            {
                                this.song_view.search_online = online;
                                this.song_view.search_source = source.clone();
                                this.song_view
                                    .search_scroll
                                    .0
                                    .borrow()
                                    .base_handle
                                    .set_offset(point(px(0.), px(0.)));
                                this.songs = Remote::Idle;
                                this.song_revision += 1;
                                this.song_view.search_loading_more = false;
                                this.song_view.search_has_more = false;
                                this.load_songs(None, cx);
                            }
                            cx.notify();
                        }))
                })),
        );
        let mut content = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.))
            .bg(color(mode, "--search-content-bg"));
        if !unavailable && self.song_view.search_playlists && self.song_view.search_online {
            content = content.child(self.playlist_search_content(narrow, width, rem, mode, cx));
        } else if unavailable {
            let title = self.t(if self.song_view.search_playlists {
                "music.search.playlistsUnavailable"
            } else {
                "music.search.noOnlineSources"
            });
            let description = self.t(
                if self.song_view.search_playlists && !self.song_view.search_online {
                    "music.search.playlistCapability"
                } else {
                    "music.search.onlineUnavailable"
                },
            );
            content = content.child(self.search_message(&title, &description, mode));
        } else if let Remote::Failed(error) = &self.songs {
            content = content
                .child(self.search_message(&self.t("music.search.searchFailed"), error, mode))
                .child(
                    Button::new("search-retry")
                        .label(self.t("common.retry"))
                        .on_click(cx.listener(|this, _, _, cx| this.load_songs(None, cx))),
                );
        } else if matches!(self.songs, Remote::Loading | Remote::Idle) {
            content = content.child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .p(px(4.))
                    .children((0..10).map(|_| {
                        div()
                            .h(px(56.))
                            .p(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .h(px(12.))
                                    .w(relative(0.6))
                                    .rounded(px(4.))
                                    .bg(color(mode, "--theme-bg-tertiary")),
                            )
                            .child(
                                div()
                                    .h(px(10.))
                                    .w(relative(0.35))
                                    .rounded(px(4.))
                                    .bg(color(mode, "--theme-bg-tertiary")),
                            )
                    })),
            );
        } else if songs.is_empty() {
            content = content.child(self.search_message(
                &self.t("music.search.noSongResults"),
                &self.t("music.search.tryOther"),
                mode,
            ));
        } else {
            let count = songs.len();
            content = content.child(
                uniform_list(
                    "search-result-rows",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| this.search_result_row(&songs[index], index, narrow, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.song_view.search_scroll)
                .flex_1()
                .min_h_0()
                .on_scroll_wheel(cx.listener(|_, _, window, cx| {
                    cx.defer_in(window, |this, _, cx| {
                        let state = this.song_view.search_scroll.0.borrow();
                        let bottom =
                            -state.base_handle.offset().y + state.base_handle.bounds().size.height;
                        let full = state.base_handle.max_offset().y
                            + state.base_handle.bounds().size.height;
                        drop(state);
                        if bottom >= full - px(100.) {
                            this.load_search_more(cx);
                        }
                    });
                })),
            );
            if self.song_view.search_loading_more {
                content = content.child(
                    div()
                        .h(px(44.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(gpui_kit::component::spinner::Spinner::new()),
                );
            } else if let Some(error) = &self.song_view.search_page_error {
                content = content.child(
                    Button::new("search-page-retry")
                        .label(self.t("music.search.pageRetry").replace("{error}", error))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.song_view.search_page_error = None;
                            this.load_search_more(cx);
                        })),
                );
            }
        }
        page.child(content)
    }

    /// `restart_playlist_search` reached from inside `load_online_search`, which already notifies.
    fn restart_playlist_search_deferred(&mut self, cx: &mut Context<Self>) {
        self.restart_playlist_search(cx);
    }

    /// Online song IDs whose cached metadata must survive eviction: the queue, the active song and
    /// everything currently listed, because each can still be played or resumed.
    pub(super) fn online_keep_ids(&self) -> std::collections::HashSet<String> {
        let mut keep: std::collections::HashSet<String> = self
            .player
            .queue
            .iter()
            .chain(self.player.active_song.iter())
            .map(|s| s.id.clone())
            .collect();
        if let Some(song) = self.player.song() {
            keep.insert(song.id.clone());
        }
        if let Remote::Ready(songs) = &self.songs {
            keep.extend(songs.iter().map(|s| s.id.clone()));
        }
        if let Remote::Ready(songs) = &self.song_view.online_playlist.songs {
            keep.extend(songs.iter().map(|s| s.id.clone()));
        }
        if let Remote::Ready(songs) = &self.song_view.singer.songs {
            keep.extend(songs.iter().map(|s| s.id.clone()));
        }
        keep
    }

    pub(super) fn load_online_search(&mut self, more: bool, cx: &mut Context<Self>) {
        if self.online_busy
            || (more
                && (self.song_view.search_playlists
                    || self.song_view.search_loading_more
                    || !self.song_view.search_has_more
                    || self.song_view.search_page_error.is_some()))
        {
            return;
        }
        let Some(online) = self.online.clone() else {
            return;
        };
        // A replaced plugin may no longer offer the selected source; fall back to all sources.
        if let Some(source) = &self.song_view.search_source
            && !online.sources().iter().any(|s| &s.id == source)
        {
            self.song_view.search_source = None;
        }
        // Chips, tabs and queued restarts search what the page shows (the committed query), not
        // whatever has been typed into the box since. Entry points that mean "search the box"
        // commit it first.
        let committed = self.song_view.search_query.trim().to_string();
        let query = if committed.is_empty() {
            self.search.read(cx).value().trim().to_string()
        } else {
            committed
        };
        if query.is_empty() {
            return;
        }
        if !more {
            // Every new query or source invalidates the playlist tab too.
            self.song_view.search_query = query.clone();
            self.restart_playlist_search_deferred(cx);
        }
        if self.song_view.search_playlists && !more {
            // The playlist tab is showing: songs are fetched when their tab is opened.
            if let Some(cancel) = &self.online_search_cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            self.online_search_pending = None;
            self.song_revision += 1;
            self.songs = Remote::Idle;
            self.online_total = 0;
            self.song_view.search_offset = 0;
            self.song_view.search_has_more = false;
            self.song_view.search_loading_more = false;
            self.song_view.search_qualities.clear();
            self.song_view.search_page_error = None;
            cx.notify();
            return;
        }
        if self.online_search_busy {
            if !more {
                if let Some(cancel) = &self.online_search_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.online_search_pending = Some(false);
                self.song_revision += 1;
                self.songs = Remote::Loading;
                self.song_view.search_query = query;
                self.song_view.search_loading_more = false;
                cx.notify();
            }
            return;
        }
        if !more {
            self.song_revision += 1;
            self.songs = Remote::Loading;
            self.online_total = 0;
            self.song_view.search_query = query.clone();
            self.song_view.search_offset = 0;
            self.song_view.search_has_more = false;
            self.song_view.search_qualities.clear();
            self.song_view.search_page_error = None;
        }
        let revision = self.song_revision;
        let page = if more {
            self.song_view.search_offset + 1
        } else {
            1
        };
        let source = self.song_view.search_source.clone();
        let keep = self.online_keep_ids();
        self.song_view.search_loading_more = more;
        self.online_search_busy = true;
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.online_search_cancel = Some(cancel.clone());
        let task = cx.background_executor().spawn(async move {
            online.search(source.as_deref(), &query, page, more, &keep, cancel)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.online_search_busy = false;
                this.online_search_cancel = None;
                if this.song_revision == revision && this.song_view.search_online {
                    this.song_view.search_loading_more = false;
                    match result {
                        Ok(result) => {
                            this.online_total = result.total;
                            if more && !result.replace {
                                if let Remote::Ready(songs) = &mut this.songs {
                                    let before = songs.len();
                                    let mut ids: std::collections::HashSet<_> =
                                        songs.iter().map(|s| s.id.clone()).collect();
                                    songs.extend(
                                        result
                                            .songs
                                            .into_iter()
                                            .filter(|s| ids.insert(s.id.clone())),
                                    );
                                    // A page that added nothing ends paging instead of looping.
                                    this.song_view.search_has_more =
                                        result.has_more && songs.len() > before;
                                }
                            } else {
                                this.song_view.search_has_more = result.has_more;
                                this.songs = Remote::Ready(result.songs);
                            }
                            this.song_view.search_offset = page;
                            this.song_revision += 1;
                        }
                        Err(error) => {
                            if more {
                                this.song_view.search_page_error = Some(error)
                            } else {
                                this.songs = Remote::Failed(error)
                            }
                        }
                    }
                }
                if this.online_search_pending.take().is_some() && this.song_view.search_online {
                    if this.navigation.current() == Page::Search {
                        this.load_online_search(false, cx);
                    } else if matches!(this.songs, Remote::Loading) {
                        this.songs = Remote::Idle;
                        this.song_view.search_loading_more = false;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn import_online_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let url = self.plugin_url.read(cx).value().trim().to_string();
        if url.is_empty() {
            return;
        }
        self.plugin_url
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.online_busy = true;
        self.online_error = None;
        cx.notify();
        let existing = self.online.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::online::Online::import_url(store, &url, existing) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.online_busy = false;
                match result {
                    Ok(online) => {
                        window.push_notification(
                            Notification::success(
                                this.t("settings.plugin.installSuccess")
                                    .replace("{name}", &online.plugin.name),
                            ),
                            cx,
                        );
                        this.refresh_installed_plugins();
                        this.online_error = None;
                    }
                    Err(error) => {
                        window.push_notification(
                            Notification::error(
                                this.t("settings.plugin.installFailed")
                                    .replace("{msg}", &error),
                            ),
                            cx,
                        );
                        this.online_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn import_online_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("导入在线音乐插件".into()),
        });
        self.online_busy = true;
        let existing = self.online.clone();
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result =
                match selection.await {
                    Ok(Ok(Some(files))) => match files.into_iter().next() {
                        Some(path) => Some(
                            cx.background_executor()
                                .spawn(async move {
                                    crate::online::Online::import(store, &path, existing)
                                })
                                .await,
                        ),
                        None => None,
                    },
                    Ok(Ok(None)) => None,
                    _ => Some(Err("无法选择插件文件".into())),
                };
            let _ = this.update_in(cx, |this, window, cx| {
                this.online_busy = false;
                match result {
                    Some(Ok(online)) => {
                        window.push_notification(
                            Notification::success(
                                this.t("settings.plugin.installSuccess")
                                    .replace("{name}", &online.plugin.name),
                            ),
                            cx,
                        );
                        this.refresh_installed_plugins();
                        this.online_error = None;
                    }
                    Some(Err(error)) => {
                        window.push_notification(
                            Notification::error(
                                this.t("settings.plugin.installFailed")
                                    .replace("{msg}", &error),
                            ),
                            cx,
                        );
                        this.online_error = Some(error);
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn search_message(
        &self,
        title: &str,
        description: &str,
        mode: crate::skin::Colors,
    ) -> impl IntoElement + use<> {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .p(px(20.))
            .child(
                div()
                    .text_size(px(16.))
                    .text_color(color(mode, "--search-empty-title"))
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(color(mode, "--search-empty-text"))
                    .child(description.to_owned()),
            )
    }

    pub(super) fn search_result_row(
        &self,
        song: &Song,
        index: usize,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let song = song.clone();
        let title = song.title.clone();
        let album_suffix = if song.album.is_empty() {
            String::new()
        } else {
            format!(" - {}", song.album)
        };
        let artist = self.song_artist_label(
            &song,
            SharedString::from(format!("search-result-singer-{index}")),
            cx,
        );
        let duration = if song.duration == 0 {
            "--:--".to_owned()
        } else {
            format!("{}:{:02}", song.duration / 60, song.duration % 60)
        };
        gpui_kit::base::Button::new(SharedString::from(format!("search-result-{index}")))
            .accessibility_label(self.online_song_accessibility_label(&song))
            .w_full()
            .h(px(56.))
            .px(px(if narrow { 10. } else { 12. }))
            .py(px(if narrow { 8. } else { 10. }))
            .rounded(px(6.))
            .flex()
            .items_center()
            .min_w_0()
            .hover(move |el| el.bg(color(mode, "--theme-hover-bg")))
            .child(
                div()
                    .w(px(if narrow { 26. } else { 32. }))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(color(mode, "--theme-text-secondary"))
                    .child((index + 1).to_string()),
            )
            .child(self.song_row_thumbnail(&song))
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
                            .child(title),
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
                            .child(self.online_song_badges(&song))
                            .child(artist)
                            .child(div().min_w_0().truncate().child(album_suffix)),
                    ),
            )
            .child(
                div()
                    .ml(px(12.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(color(mode, "--theme-text-secondary"))
                    .child(duration),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(quality) = this.song_view.search_qualities.get(&song.id) {
                    this.player
                        .queue_qualities
                        .insert(song.id.clone(), *quality);
                }
                let index = this.enqueue_player_first(song.clone());
                this.play_index(index, cx);
            }))
            .into_any_element()
    }
}
