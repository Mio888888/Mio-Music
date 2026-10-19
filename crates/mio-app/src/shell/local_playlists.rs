use super::*;
use crate::library::{self, Playlist, PlaylistSong};
use gpui_kit::component::{WindowExt, notification::Notification};
use std::collections::HashMap;

pub(super) struct LocalPlaylists {
    list: Remote<Playlist>,
    pub(super) list_revision: u64,
    pub(super) play_revision: u64,
    pub(super) scroll: ScrollHandle,
    pub selected: Option<Playlist>,
    pub(super) songs: HashMap<String, PlaylistSong>,
    pub(super) covers: playlist_cover::CoverCache,
    pub(super) selected_cover: Option<Arc<RenderImage>>,
    pub(super) retired_covers: Vec<Arc<RenderImage>>,
    pub(super) cover_order: std::collections::VecDeque<String>,
    pub(super) cover_busy: bool,
    pub(super) degraded_covers: std::collections::HashSet<String>,
    pub(super) visible_cover_ids: std::collections::HashSet<String>,
    pub(super) cover_view_key: Option<(Page, Size<Pixels>, Point<Pixels>, u64)>,
}
impl Default for LocalPlaylists {
    fn default() -> Self {
        Self {
            list: Remote::Idle,
            list_revision: 0,
            play_revision: 0,
            scroll: ScrollHandle::new(),
            selected: None,
            songs: HashMap::new(),
            covers: HashMap::new(),
            selected_cover: None,
            retired_covers: Vec::new(),
            cover_order: Default::default(),
            cover_busy: false,
            degraded_covers: Default::default(),
            visible_cover_ids: Default::default(),
            cover_view_key: None,
        }
    }
}

impl Mio {
    pub(super) fn remove_local_playlist_songs(
        &mut self,
        songs: Vec<Song>,
        batch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.navigation.current() != Page::Playlist
            || self.favorites.busy
            || songs.is_empty()
            || songs
                .iter()
                .any(|song| !self.local_playlists.songs.contains_key(&song.id))
        {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let Some(playlist) = self.local_playlists.selected.clone() else {
            return;
        };
        let revision = self.album_song_revision;
        let song_ids: Vec<_> = songs.iter().map(|song| song.id.clone()).collect();
        let success = if batch {
            self.t("music.list.removedSongs")
                .replace("{count}", &songs.len().to_string())
        } else {
            self.t("music.list.removedFromPlaylist")
                .replace("{name}", &songs[0].title)
        };
        let failure = self.t(if batch {
            "music.list.batchRemoveFailed"
        } else {
            "music.list.removeFailed"
        });
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let id = playlist.id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { library::remove_playlist_songs(&path, &id, &song_ids) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.favorites.busy = false;
                match result {
                    Ok(snapshot) => {
                        this.favorites.snapshot = Some(snapshot);
                        let current = this.navigation.current() == Page::Playlist
                            && this
                                .local_playlists
                                .selected
                                .as_ref()
                                .is_some_and(|p| p.id == playlist.id);
                        if current && batch && this.album_song_revision == revision {
                            this.song_view.reset_selection();
                        }
                        this.load_playlists(cx);
                        if current {
                            this.close_context_actions(window, cx);
                            this.load_local_playlist(cx);
                        }
                        window.push_notification(Notification::success(success), cx);
                    }
                    Err(_) => window.push_notification(Notification::error(failure), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn load_playlists(&mut self, cx: &mut Context<Self>) {
        self.local_playlists.list_revision += 1;
        let revision = self.local_playlists.list_revision;
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            self.local_playlists.list = Remote::Failed(self.t("common.failed"));
            return;
        };
        self.local_playlists.list = Remote::Loading;
        let covers = self
            .local_playlists
            .covers
            .iter()
            .filter(|(id, _)| !self.local_playlists.degraded_covers.contains(*id))
            .map(|(id, cover)| (id.clone(), cover.clone()))
            .collect();
        let renderer = cx.svg_renderer();
        let task = cx.background_executor().spawn(async move {
            library::list_playlists(&path).map(|playlists| {
                let covers = playlist_cover::cache_covers(&playlists, covers, &renderer);
                (playlists, covers)
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.local_playlists.list_revision != revision {
                    return;
                }
                this.local_playlists.list = match result {
                    Ok((playlists, covers)) => {
                        for (_, (_, image)) in
                            std::mem::replace(&mut this.local_playlists.covers, covers)
                        {
                            if let Some(image) = image
                                && !this.local_playlists.covers.values().any(|(_, new)| {
                                    new.as_ref().is_some_and(|new| Arc::ptr_eq(new, &image))
                                })
                            {
                                this.local_playlists.retired_covers.push(image);
                            }
                        }
                        this.local_playlists
                            .cover_order
                            .retain(|id| this.local_playlists.covers.contains_key(id));
                        for id in this.local_playlists.covers.keys() {
                            if !this.local_playlists.cover_order.contains(id) {
                                this.local_playlists.cover_order.push_back(id.clone());
                            }
                        }
                        Remote::Ready(playlists)
                    }
                    Err(_) => Remote::Failed(this.t("common.failed")),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn local_playlists_changed(&mut self, cx: &mut Context<Self>) {
        self.local_playlists.list_revision += 1;
        self.local_playlists.list = Remote::Idle;
        match self.navigation.current() {
            Page::Playlists => self.load_playlists(cx),
            Page::Playlist
                if self
                    .local_playlists
                    .selected
                    .as_ref()
                    .is_some_and(|playlist| playlist.favorite) =>
            {
                self.load_local_playlist(cx)
            }
            _ => {}
        }
    }

    pub(super) fn open_local_playlist(
        &mut self,
        playlist: Playlist,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigation.push_playlist(playlist.clone());
        self.restore_local_playlist(playlist, window, cx);
    }

    pub(super) fn restore_local_playlist(
        &mut self,
        playlist: Playlist,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_context_actions(window, cx);
        self.song_view.reset_list();
        self.song_view
            .filter
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.song_view
            .scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), px(0.)));
        self.local_playlists.selected = Some(playlist);
        self.local_playlists
            .retired_covers
            .extend(self.local_playlists.selected_cover.take());
        self.load_local_playlist(cx);
    }

    pub(super) fn load_local_playlist(&mut self, cx: &mut Context<Self>) {
        let Some(playlist) = self.local_playlists.selected.clone() else {
            return;
        };
        self.album_song_revision += 1;
        let revision = self.album_song_revision;
        self.album_songs = Remote::Loading;
        self.local_playlists.songs.clear();
        self.song_view.album_qualities.clear();
        self.song_view.album_covers.replace_ids([]);
        self.song_view.backdrop.update(cx, |state, _| state.clear());
        self.selected_album = Some(Album {
            id: playlist.id.clone(),
            name: playlist.name.clone(),
            artist: playlist.description.clone(),
            cover_art: None,
            song_count: playlist.song_count as u64,
        });
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            self.album_songs = Remote::Failed(self.t("common.failed"));
            cx.notify();
            return;
        };
        let playlist_id = playlist.id.clone();
        let cached_cover = self
            .local_playlists
            .covers
            .get(&playlist.id)
            .filter(|_| !self.local_playlists.degraded_covers.contains(&playlist.id))
            .cloned();
        let renderer = cx.svg_renderer();
        let task = cx.background_executor().spawn(async move {
            library::load_playlist(&path, &playlist.id).map(|detail| {
                let cover = match cached_cover {
                    Some((url, image)) if url == detail.playlist.cover_img_url => image,
                    _ => playlist_cover::decode_url(&detail.playlist.cover_img_url, &renderer),
                };
                (detail, cover)
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.album_song_revision != revision
                    || !this
                        .local_playlists
                        .selected
                        .as_ref()
                        .is_some_and(|p| p.id == playlist_id)
                {
                    return;
                }
                match result {
                    Ok((detail, cover)) => {
                        if let Some(old) =
                            std::mem::replace(&mut this.local_playlists.selected_cover, cover)
                            && this
                                .local_playlists
                                .selected_cover
                                .as_ref()
                                .is_none_or(|new| !Arc::ptr_eq(new, &old))
                        {
                            this.local_playlists.retired_covers.push(old);
                        }
                        let identity = None::<SessionIdentity>;
                        let mut songs = Vec::with_capacity(detail.songs.len());
                        // Online rows carry their MusicItem; make them resolvable for playback.
                        let online_items: Vec<serde_json::Value> = detail
                            .songs
                            .iter()
                            .filter_map(|row| row.online.clone())
                            .collect();
                        if !online_items.is_empty()
                            && let Some(online) = this.online.clone()
                        {
                            let keep = this.online_keep_ids();
                            cx.background_executor()
                                .spawn(async move {
                                    online.register(online_items, &keep);
                                })
                                .detach();
                        }
                        for mut row in detail.songs {
                            // A cover ID is scoped to its server just like the stream ID.
                            if !row.playable(identity.as_ref()) {
                                row.song.cover_art = None;
                            }
                            if let Some(quality) = row.quality {
                                if row.playable(identity.as_ref()) {
                                    this.remember_favorite_quality(&row.song.id, quality);
                                }
                                this.song_view
                                    .album_qualities
                                    .insert(row.song.id.clone(), quality);
                            }
                            songs.push(row.song.clone());
                            this.local_playlists.songs.insert(row.song.id.clone(), row);
                        }
                        this.song_view
                            .album_covers
                            .replace_ids(songs.iter().filter_map(|song| song.cover_art.clone()));
                        this.selected_album = Some(Album {
                            id: detail.playlist.id.clone(),
                            name: detail.playlist.name.clone(),
                            artist: detail.playlist.description.clone(),
                            cover_art: None,
                            song_count: songs.len() as u64,
                        });
                        this.local_playlists.selected = Some(detail.playlist);
                        this.song_view
                            .selected
                            .retain(|id| songs.iter().any(|song| &song.id == id));
                        this.album_songs = Remote::Ready(songs);
                        this.pump_album_covers(cx);
                    }
                    Err(_) => this.album_songs = Remote::Failed(this.t("common.failed")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn local_song_playable(&self, id: &str) -> bool {
        if id.starts_with("mio-local:") {
            if self.navigation.current() == Page::Playlist
                && !self
                    .local_playlists
                    .songs
                    .get(id)
                    .is_some_and(|row| row.playable(None))
            {
                return false;
            }
            return self
                .local_music
                .library
                .as_ref()
                .and_then(|l| l.path(id))
                .is_some();
        }
        if id.starts_with("mio-online:") {
            // Online rows play through the plugin, whichever Subsonic source is active.
            return self.online.is_some()
                && (self.navigation.current() != Page::Playlist
                    || self.local_playlists.songs.contains_key(id));
        }
        if self.navigation.current() != Page::Playlist {
            return true;
        }
        let identity = None::<SessionIdentity>;
        self.local_playlists
            .songs
            .get(id)
            .is_some_and(|row| row.playable(identity.as_ref()))
    }

    pub(super) fn local_song_source(&self, id: &str) -> String {
        self.local_playlists
            .songs
            .get(id)
            .map(|row| {
                if row.source == "subsonic" {
                    "Subsonic".to_owned()
                } else {
                    row.source.clone()
                }
            })
            .unwrap_or_else(|| "local".to_owned())
    }

    pub(super) fn play_local_playlist(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let identity = None::<SessionIdentity>;
        self.local_playlists.play_revision += 1;
        let revision = self.local_playlists.play_revision;
        let queue_revision = self.player.queue_revision;
        let generation = self.player.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { library::load_playlist(&path, &id) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.local_playlists.play_revision != revision
                    || this.player.queue_revision != queue_revision
                    || this.player.generation != generation
                {
                    return;
                }
                match result {
                    Ok(detail) if detail.songs.is_empty() => {
                        window.push_notification(
                            Notification::warning(this.t("music.songlist.noSongs")),
                            cx,
                        );
                    }
                    Ok(detail)
                        if detail.songs.iter().all(|row| {
                            (row.playable(identity.as_ref())
                                && (!crate::local_music::is_local(&row.song)
                                    || this
                                        .local_music
                                        .library
                                        .as_ref()
                                        .and_then(|l| l.path(&row.song.id))
                                        .is_some()))
                                || (row.online.is_some() && this.online.is_some())
                        }) =>
                    {
                        if let Some(online) = this.online.clone() {
                            let items = detail
                                .songs
                                .iter()
                                .filter_map(|row| row.online.clone())
                                .collect();
                            let keep = this.online_keep_ids();
                            online.register(items, &keep);
                        }
                        let qualities = detail
                            .songs
                            .iter()
                            .filter_map(|row| {
                                row.quality.map(|quality| (row.song.id.clone(), quality))
                            })
                            .collect();
                        this.replace_player_queue(
                            detail.songs.into_iter().map(|row| row.song).collect(),
                        );
                        this.player.queue_qualities = qualities;
                        this.play_index(0, cx);
                    }
                    Ok(_) => this.local_playlist_play_error(window, cx),
                    Err(_) => {
                        window.push_notification(
                            Notification::error(this.t("music.songlist.playFailed")),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn local_playlist_play_error(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(
            Notification::warning(if self.english {
                "This playlist needs its matching music source."
            } else {
                "请先启用歌单歌曲对应的音乐源。"
            }),
            cx,
        );
    }

    pub(super) fn playlists_page(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width);
        let narrow = width <= 768.;
        let rem = if narrow {
            16.
        } else {
            f32::from(window.rem_size())
        };
        let gutter = if narrow {
            (width * 0.04).clamp(16., 24.)
        } else {
            2. * rem
        };
        let mode = self.ui_colors();
        let count = match &self.local_playlists.list {
            Remote::Ready(items) => items.len(),
            _ => 0,
        };
        let actions = div()
            .id("pl-actions")
            .test_support()
            .flex()
            .gap(px(if narrow { 8. } else { 0.75 * rem }))
            .when(narrow, |row| row.w_full())
            .child(
                div()
                    .id("pl-create")
                    .test_support()
                    .when(narrow, |w| w.flex_1())
                    .child(
                        Button::new("create-local-playlist")
                            .outline()
                            .icon(glyph("zengjia").size(px(14.)))
                            .label(self.t("music.songlist.newPlaylist"))
                            .disabled(self.store.is_none())
                            .when(!narrow, |button| button.px(px(8.8)))
                            .h(px(if narrow { 44. } else { 32. }))
                            .when(narrow, |button| button.flex_1().rounded(px(999.)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_playlist_editor(None, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .id("pl-import")
                    .test_support()
                    .when(narrow, |w| w.flex_1())
                    .child(
                        Button::new("import-local-playlist")
                            .primary()
                            .label(self.t("common.import"))
                            .disabled(self.store.is_none() || self.favorites.busy)
                            .when(!narrow, |button| button.px(px(15.3)))
                            .h(px(if narrow { 44. } else { 32. }))
                            .when(narrow, |button| button.flex_1().rounded(px(999.)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_playlist_import(window, cx)
                            })),
                    ),
            );
        let mut actions = Some(actions);
        let mut body = div()
            .id("local-playlists-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.local_playlists.scroll)
            .px(px(gutter))
            .pt(px(if narrow { 14. } else { rem }))
            .pb(px(3. * rem));
        body = body
            // Wide windows drop the title; actions join the section row.
            .when(narrow, |body| {
                body.child(
                    div()
                        .id("pl-header")
                        .test_support()
                        .flex()
                        .justify_between()
                        .items_start()
                        .mb(px(if narrow { 1.5 * rem } else { 2. * rem }))
                        .when(narrow, |header| header.flex_col().gap(px(rem)))
                        .child(
                            div()
                                .id("pl-left")
                                .test_support()
                                .text_size(px(if narrow {
                                    (width * 0.09).clamp(2. * rem, 2.6 * rem)
                                } else {
                                    1.875 * rem
                                }))
                                .line_height(relative(if narrow { 1.1 } else { 1.5 }))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(!narrow, |title| {
                                    title
                                        .border_l(px(8.))
                                        .border_color(color(mode, "--td-brand-color-3"))
                                        .pl(px(12.))
                                        .rounded(px(8.))
                                })
                                .child(self.t("music.songlist.title")),
                        )
                        .when(narrow, |header| header.children(actions.take())),
                )
            })
            .child(
                div()
                    .id("pl-section-header")
                    .test_support()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(if narrow { rem } else { 1.5 * rem }))
                    .child(
                        div()
                            .text_size(px(if narrow { 1.1 * rem } else { 1.25 * rem }))
                            .when(!narrow, |title| title.line_height(px(24.)))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                self.t("music.songlist.myPlaylists")
                                    .replace("{count}", &count.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(0.75 * rem))
                            // Wide: the actions sit here, before an icon-only refresh.
                            .children(actions.take())
                            .child({
                                let refresh = Button::new("refresh-local-playlists")
                                    .disabled(matches!(self.local_playlists.list, Remote::Loading))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.load_playlists(cx)),
                                    );
                                if narrow {
                                    refresh
                                        .ghost()
                                        .label(self.t("common.refresh"))
                                        .h(px(44.))
                                        .rounded(px(999.))
                                } else {
                                    refresh
                                        .ghost()
                                        .accessibility_label(self.t("common.refresh"))
                                        .tooltip(self.t("common.refresh"))
                                        .icon(icon("Refresh"))
                                        .size(px(32.))
                                        .rounded(px(6.))
                                }
                            }),
                    ),
            );
        match &self.local_playlists.list {
            Remote::Idle | Remote::Loading => {
                body = body.child(
                    div()
                        .py(px(4. * rem))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .child(gpui_kit::component::spinner::Spinner::new())
                        .child(self.t("common.loading")),
                )
            }
            Remote::Failed(message) => {
                body = body.child(div().py(px(4. * rem)).text_center().child(message.clone()))
            }
            Remote::Ready(playlists) if playlists.is_empty() => {
                body = body.child(
                    div()
                        .id("pl-empty")
                        .test_support()
                        .py(px(if narrow { 3. * rem } else { 4. * rem }))
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .mt(px(1.5 * rem))
                                .mb(px(0.5 * rem))
                                .text_size(px(1.125 * rem))
                                .line_height(px(22.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.t("music.songlist.emptyTitle")),
                        )
                        .child(
                            div()
                                .mb(px(2. * rem))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(self.t("music.songlist.emptyDesc")),
                        )
                        .child(
                            Button::new("empty-create-local-playlist")
                                .icon(glyph("zengjia"))
                                .primary()
                                .disabled(self.store.is_none())
                                .h(px(if narrow { 44. } else { 32. }))
                                .label(self.t("music.songlist.createPlaylist"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_playlist_editor(None, window, cx)
                                })),
                        ),
                );
            }
            Remote::Ready(playlists) => {
                let gap = if narrow { 8. } else { rem };
                let content_width =
                    (width - if narrow { 0. } else { 15. * rem } - 2. * gutter).max(1.);
                let columns = if narrow {
                    1
                } else {
                    ((content_width + gap) / (168. + gap)).floor().max(1.) as u16
                };
                let card_width = (content_width - gap * (columns - 1) as f32) / columns as f32;
                body = body.child(div().grid().grid_cols(columns).gap(px(gap)).children(
                    playlists.iter().map(|playlist| {
                        self.local_playlist_card(playlist, narrow, rem, card_width, cx)
                    }),
                ));
            }
        }
        body
    }

    fn local_playlist_card(
        &self,
        playlist: &Playlist,
        narrow: bool,
        rem: f32,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let cover =
            if playlist.cover_img_url.is_empty() || playlist.cover_img_url == "default-cover" {
                "images/default-cover.png".to_owned()
            } else {
                playlist.cover_img_url.clone()
            };
        let cover = if cover.starts_with("data:") {
            playlist_cover::InlineCover {
                parent: cx.weak_entity(),
                id: playlist.id.clone(),
                url: cover,
                radius: if narrow { 8. } else { 0.75 * rem },
            }
            .into_any_element()
        } else {
            img(ImageSource::from(cover))
                .size_full()
                .rounded_t(px(if narrow { 8. } else { 0.75 * rem }))
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        };
        let group = SharedString::from(format!("playlist-card-{}", playlist.id));
        let open = playlist.clone();
        let open_name = playlist.clone();
        let open_detail = playlist.clone();
        let edit = playlist.clone();
        let delete = playlist.clone();
        let play_id = playlist.id.clone();
        if narrow {
            let menu = playlist.clone();
            return div()
                .id(group.clone())
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let playlist = playlist.clone();
                        move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.playlist_context_actions(
                                playlist.clone(),
                                event.position,
                                window,
                                cx,
                            );
                        }
                    }),
                )
                .min_w_0()
                .flex()
                .items_center()
                .p(px(10.))
                .rounded(px(12.))
                .bg(color(mode, "--td-bg-color-container"))
                .child(
                    gpui_kit::base::Button::new(SharedString::from(format!(
                        "playlist-open-{}",
                        playlist.id
                    )))
                    .accessibility_label(playlist.name.clone())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(60.))
                            .flex_shrink_0()
                            .rounded(px(8.))
                            .overflow_hidden()
                            .child(cover),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(15.))
                                    .line_height(relative(1.3))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(playlist.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(format!(
                                        "{} · {}",
                                        playlist.source.to_uppercase(),
                                        self.t("music.local.totalSongs")
                                            .replace("{count}", &playlist.song_count.to_string())
                                    )),
                            )
                            .when(playlist.favorite, |info| {
                                info.child(
                                    div()
                                        .truncate()
                                        .text_size(px(10.))
                                        .text_color(color(mode, "--td-brand-color"))
                                        .child(self.t("music.songlist.myFavorite")),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_local_playlist(open.clone(), window, cx)
                    })),
                )
                .child(div().w(px(4.)).flex_shrink_0())
                .child(
                    control(
                        SharedString::from(format!("playlist-play-{}", playlist.id)),
                        "TD-PlayCircle",
                        self.t("music.songlist.playPlaylist"),
                        mode,
                        cx,
                    )
                    .size(px(44.))
                    .flex_shrink_0()
                    .disabled(playlist.song_count == 0)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.play_local_playlist(play_id.clone(), window, cx)
                    })),
                )
                .child(
                    control(
                        SharedString::from(format!("playlist-more-{}", playlist.id)),
                        "TD-Ellipsis",
                        if self.english {
                            "More actions".to_owned()
                        } else {
                            "更多操作".to_owned()
                        },
                        mode,
                        cx,
                    )
                    .size(px(44.))
                    .flex_shrink_0()
                    .on_click(cx.listener(
                        move |this, event: &ClickEvent, window, cx| {
                            this.playlist_context_actions(
                                menu.clone(),
                                event.position(),
                                window,
                                cx,
                            )
                        },
                    )),
                )
                .into_any_element();
        }
        div()
            .id(group.clone())
            .group(group.clone())
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let playlist = playlist.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.playlist_context_actions(playlist.clone(), event.position, window, cx);
                    }
                }),
            )
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(if narrow { 16. } else { 0.75 * rem }))
            // Wide cards sit on a similar tone as the page: a lighter surface and a hairline
            // keep each card's edge visible.
            .bg(color(
                mode,
                if narrow || mode.mode != Mode::Dark {
                    "--td-bg-color-container"
                } else {
                    "--td-bg-color-secondarycontainer"
                },
            ))
            .when(!narrow, |card| {
                card.border_1()
                    .border_color(color(mode, "--td-component-stroke"))
            })
            .shadow(vec![
                BoxShadow::new(
                    px(0.),
                    px(if narrow { 8. } else { 2. }),
                    black().opacity(0.08),
                )
                .blur_radius(px(if narrow { 24. } else { 8. })),
            ])
            .child(
                gpui_kit::base::Button::new(SharedString::from(format!(
                    "playlist-cover-{}",
                    playlist.id
                )))
                .accessibility_label(playlist.name.clone())
                .w_full()
                .h(px(if narrow {
                    width
                } else {
                    (width * 0.58).clamp(88., 130.)
                }))
                .relative()
                .overflow_hidden()
                .child(cover)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_t(px(if narrow { 16. } else { 0.75 * rem }))
                        .bg(black().opacity(if narrow { 0.18 } else { 0.4 }))
                        .opacity(if narrow { 1. } else { 0. })
                        .group_hover(group, |overlay| overlay.opacity(1.))
                        .child(
                            glyph("bofang")
                                .text_size(px(if narrow { 2. * rem } else { 2.25 * rem }))
                                .text_color(white()),
                        ),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_local_playlist(open.clone(), window, cx)
                })),
            )
            .child(
                div()
                    .px(px(0.75 * rem))
                    .pt(px(0.625 * rem))
                    .pb(px(0.25 * rem))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        gpui_kit::base::Button::new(SharedString::from(format!(
                            "playlist-name-{}",
                            playlist.id
                        )))
                        .accessibility_label(playlist.name.clone())
                        .flex()
                        .items_start()
                        .gap(px(6.))
                        .w_full()
                        .mb(px(0.25 * rem))
                        .when(narrow, |row| row.min_h(px(44.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .line_clamp(1)
                                .text_size(px(if narrow { 0.95 * rem } else { 0.875 * rem }))
                                .line_height(relative(if narrow { 1.25 } else { 1.4 }))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(playlist.name.clone()),
                        )
                        .when(playlist.favorite, |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .mt(px(1.))
                                    .px(px(4.))
                                    .h(px(20.))
                                    .rounded(px(3.))
                                    .border_1()
                                    .border_color(color(mode, "--td-error-color-3"))
                                    .bg(color(mode, "--td-error-color-light"))
                                    .text_color(color(mode, "--td-error-color"))
                                    .text_size(px(10.))
                                    .line_height(px(18.))
                                    .child(self.t("music.songlist.myFavorite")),
                            )
                        })
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_local_playlist(open_name.clone(), window, cx)
                            },
                        )),
                    )
                    .when(!narrow, |info| {
                        info.child(
                            div()
                                .flex_1()
                                .line_clamp(1)
                                .text_size(px(0.72 * rem))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .mb(px(0.3 * rem))
                                .child(if playlist.description.is_empty() {
                                    self.t("music.songlist.noDescription")
                                } else {
                                    playlist.description.clone()
                                }),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(0.5 * rem))
                            .text_size(px(if narrow { 0.7 * rem } else { 0.68 * rem }))
                            .text_color(color(mode, "--td-text-color-placeholder"))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(mode, "--td-brand-color"))
                                    .child(playlist.source.to_uppercase()),
                            )
                            .when(!playlist.create_date.is_empty(), |meta| {
                                meta.child(
                                    self.t("music.songlist.createdAt")
                                        .replace("{date}", &playlist.create_date),
                                )
                            }),
                    ),
            )
            .child(
                // Wide cards: a hairline separates the actions, which share the width evenly.
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mx(px(0.5 * rem))
                    .mt(px(0.25 * rem))
                    .px(px(0.25 * rem))
                    .py(px(0.4 * rem))
                    .border_t_1()
                    .border_color(color(mode, "--td-component-stroke"))
                    .child(
                        control(
                            SharedString::from(format!("playlist-play-{}", playlist.id)),
                            "TD-PlayCircle",
                            self.t("music.songlist.playPlaylist"),
                            mode,
                            cx,
                        )
                        .size(px(28.))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.play_local_playlist(play_id.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        control(
                            SharedString::from(format!("playlist-view-{}", playlist.id)),
                            "TD-ViewList",
                            self.t("music.songlist.viewDetail"),
                            mode,
                            cx,
                        )
                        .size(px(28.))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_local_playlist(open_detail.clone(), window, cx)
                            },
                        )),
                    )
                    .child(
                        control(
                            SharedString::from(format!("playlist-edit-{}", playlist.id)),
                            "TD-Edit2",
                            self.t("music.songlist.editPlaylist"),
                            mode,
                            cx,
                        )
                        .size(px(28.))
                        .disabled(self.store.is_none())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_playlist_editor(Some(edit.clone()), window, cx)
                            },
                        )),
                    )
                    .child(
                        control(
                            SharedString::from(format!("playlist-delete-{}", playlist.id)),
                            "TD-Delete",
                            self.t("music.songlist.deletePlaylist"),
                            mode,
                            cx,
                        )
                        .size(px(28.))
                        .disabled(self.store.is_none() || self.favorites.busy)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.confirm_delete_playlist(delete.clone(), window, cx);
                            },
                        )),
                    ),
            )
            .into_any_element()
    }
}
