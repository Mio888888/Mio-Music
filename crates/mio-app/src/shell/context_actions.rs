use super::*;

type Handler = Rc<dyn Fn(&mut Mio, &mut Window, &mut Context<Mio>)>;

struct Item {
    label: String,
    icon: &'static str,
    danger: bool,
    disabled: bool,
    handler: Option<Handler>,
}

impl Item {
    fn action(
        label: String,
        icon: &'static str,
        handler: impl Fn(&mut Mio, &mut Window, &mut Context<Mio>) + 'static,
    ) -> Self {
        Self {
            label,
            icon,
            danger: false,
            disabled: false,
            handler: Some(Rc::new(handler)),
        }
    }
    fn unavailable(label: String, icon: &'static str) -> Self {
        Self {
            label,
            icon,
            danger: false,
            disabled: true,
            handler: None,
        }
    }
    fn separator() -> Self {
        Self::unavailable(String::new(), "")
    }
}

pub(super) struct ContextActions {
    position: Point<Pixels>,
    width: f32,
    items: Vec<Item>,
    selected: Option<usize>,
    previous_focus: Option<FocusHandle>,
    page: Page,
    album_revision: u64,
    song_revision: u64,
    playlist: Option<String>,
}

impl Mio {
    pub(super) fn local_track_context_actions(
        &mut self,
        track: crate::local_music::Track,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let play = track.song.clone();
        let add = track.song.clone();
        let append = track.clone();
        let favorite = track.song.clone();
        let mut edit = Item::action(
            self.t("music.local.contextEditTag"),
            "bianji",
            move |this, window, cx| this.open_local_tag_editor(track.clone(), window, cx),
        );
        edit.disabled = self.local_music.busy;
        let mut adding = Item::action(
            self.t("music.local.contextAddToLocal"),
            "zengjia",
            move |this, window, cx| this.open_add_to_playlist(vec![add.clone()], window, cx),
        );
        adding.disabled = self.favorites.busy || self.store.is_none();
        let mut liking = Item::action(self.t("play.like"), "TD-Heart", move |this, window, cx| {
            this.toggle_favorite(favorite.clone(), window, cx)
        });
        liking.disabled = self.favorites.busy || self.favorites.snapshot.is_none();
        self.show_context_actions(
            position,
            210.,
            vec![
                Item::action(
                    self.t("music.local.contextPlay"),
                    "bofang",
                    move |this, _, cx| {
                        if this.local_song_playable(&play.id) {
                            let index = this.enqueue_player_first(play.clone());
                            this.play_index(index, cx);
                        }
                    },
                ),
                Item::action(
                    self.t("music.local.contextJoinPlaylist"),
                    "zengjia",
                    move |this, _, cx| this.append_local_tracks(vec![append.clone()], cx),
                ),
                adding,
                liking,
                edit,
            ],
            window,
            cx,
        );
    }

    fn show_context_actions(
        &mut self,
        position: Point<Pixels>,
        width: f32,
        items: Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_context_actions(window, cx);
        self.context_actions = Some(ContextActions {
            position,
            width,
            items,
            selected: None,
            previous_focus: window.focused(cx),
            page: self.navigation.current(),
            album_revision: self.album_song_revision,
            song_revision: self.song_revision,
            playlist: self.local_playlists.selected.as_ref().map(|p| p.id.clone()),
        });
        self.context_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_context_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.context_actions.take() {
            if self.context_focus.contains_focused(window, cx)
                && let Some(focus) = menu.previous_focus
            {
                focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    fn run_context_action(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &self.context_actions else {
            return;
        };
        let valid = menu.page == self.navigation.current()
            && menu.album_revision == self.album_song_revision
            && menu.song_revision == self.song_revision
            && menu.playlist == self.local_playlists.selected.as_ref().map(|p| p.id.clone());
        let handler = menu
            .items
            .get(index)
            .filter(|item| !item.disabled)
            .and_then(|item| item.handler.clone());
        self.close_context_actions(window, cx);
        if valid && let Some(handler) = handler {
            handler(self, window, cx);
        }
    }

    fn context_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.context_actions else {
            return;
        };
        match event.keystroke.key.as_str() {
            "escape" => self.close_context_actions(window, cx),
            "enter" | "space" => {
                if let Some(index) = menu.selected {
                    self.run_context_action(index, window, cx);
                }
            }
            "up" | "down" | "home" | "end" | "tab" => {
                let enabled: Vec<_> = menu
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| !item.disabled)
                    .map(|(i, _)| i)
                    .collect();
                if !enabled.is_empty() {
                    let current = menu
                        .selected
                        .and_then(|i| enabled.iter().position(|n| *n == i));
                    let backwards = event.keystroke.key == "up"
                        || (event.keystroke.key == "tab" && event.keystroke.modifiers.shift);
                    let index = match event.keystroke.key.as_str() {
                        "home" => 0,
                        "end" => enabled.len() - 1,
                        _ if backwards => current
                            .map(|i| (i + enabled.len() - 1) % enabled.len())
                            .unwrap_or(enabled.len() - 1),
                        _ => current.map(|i| (i + 1) % enabled.len()).unwrap_or(0),
                    };
                    menu.selected = Some(enabled[index]);
                    cx.notify();
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_context_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.context_actions else {
            return div().into_any_element();
        };
        let colors = self.ui_colors();
        let mut content = div()
            .id("mio-context-menu")
            .role(Role::Menu)
            .track_focus(&self.context_focus)
            .min_w(px(menu.width))
            .rounded(px(10.))
            .p(px(4.))
            .flex()
            .flex_col()
            .bg(color(colors, "--td-bg-color-container"))
            .text_color(color(colors, "--td-text-color-primary"))
            .shadow(vec![
                BoxShadow::new(px(0.), px(4.), black().opacity(0.15)).blur_radius(px(12.)),
                BoxShadow::new(px(0.), px(0.), color(colors, "--td-border-level-1-color"))
                    .spread_radius(px(1.)),
            ])
            .occlude()
            .on_key_down(cx.listener(Self::context_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(
                cx.listener(|this, _, window, cx| this.close_context_actions(window, cx)),
            );
        for (index, item) in menu.items.iter().enumerate() {
            if item.label.is_empty() {
                content = content.child(
                    div()
                        .h(px(1.))
                        .my(px(4.))
                        .mx(px(8.))
                        .bg(color(colors, "--td-border-level-1-color")),
                );
                continue;
            }
            let danger = item.danger;
            let selected = menu.selected == Some(index);
            content = content.child(
                gpui_kit::base::Button::new(("context-action", index))
                    .role(Role::MenuItem)
                    .accessibility_label(item.label.clone())
                    .disabled(item.disabled)
                    .focusable(false)
                    .w_full()
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .line_height(relative(1.5))
                    .flex()
                    .items_center()
                    .justify_start()
                    .font_weight(FontWeight::NORMAL)
                    .gap(px(8.))
                    .text_color(if item.disabled {
                        color(colors, "--td-text-color-primary").opacity(0.35)
                    } else {
                        color(
                            colors,
                            if danger {
                                "--td-error-color"
                            } else {
                                "--td-text-color-primary"
                            },
                        )
                    })
                    .when(selected, |row| {
                        row.bg(color(
                            colors,
                            if danger {
                                "--td-error-color-light"
                            } else {
                                "--td-bg-color-component-hover"
                            },
                        ))
                    })
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if let Some(menu) = &mut this.context_actions {
                            menu.selected = if *hovered { Some(index) } else { None };
                            cx.notify();
                        }
                    }))
                    .child(
                        if item.icon == "bofang" || item.icon == "zengjia" {
                            glyph(item.icon)
                        } else {
                            icon(item.icon)
                        }
                        .size(px(14.)),
                    )
                    .child(item.label.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.run_context_action(index, window, cx)
                    })),
            );
        }
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window()
                .child(content),
        )
        .with_priority(100)
        .into_any_element()
    }

    pub(super) fn playlist_context_actions(
        &mut self,
        playlist: crate::library::Playlist,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let play = playlist.id.clone();
        let view = playlist.clone();
        let edit = playlist.clone();
        let mut delete = Item::action(
            self.t("music.songlist.deletePlaylist"),
            "TD-Delete",
            move |this, window, cx| this.confirm_delete_playlist(playlist.clone(), window, cx),
        );
        delete.danger = true;
        delete.disabled = self.store.is_none() || self.favorites.busy;
        let mut editing = Item::action(
            self.t("music.songlist.editPlaylist"),
            "TD-Edit2",
            move |this, window, cx| this.open_playlist_editor(Some(edit.clone()), window, cx),
        );
        editing.disabled = self.store.is_none() || self.favorites.busy;
        self.show_context_actions(
            position,
            156.,
            vec![
                Item::action(
                    self.t("music.songlist.playPlaylist"),
                    "bofang",
                    move |this, window, cx| this.play_local_playlist(play.clone(), window, cx),
                ),
                Item::action(
                    self.t("music.songlist.viewDetail"),
                    "TD-ViewList",
                    move |this, window, cx| this.open_local_playlist(view.clone(), window, cx),
                ),
                Item::unavailable(self.t("music.songlist.downloadAll"), "TD-Download"),
                Item::separator(),
                editing,
                delete,
            ],
            window,
            cx,
        );
    }

    pub(super) fn song_context_actions(
        &mut self,
        song: Song,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let online_key = crate::online::parse_song_id(&song.id);
        let is_online = online_key.is_some();
        // Online songs play through the plugin and need no Subsonic source.
        let playable = self.local_song_playable(&song.id)
            && (is_online || crate::local_music::is_local(&song));
        let quality = self.display_song_quality(&song.id);
        let play = song.clone();
        let add = song.clone();
        let favorite = song.clone();
        let download = song.clone();
        let liked = match &online_key {
            Some((source, raw)) => self
                .favorites
                .snapshot
                .as_ref()
                .is_some_and(|s| s.contains_online(source, raw)),
            None if crate::local_music::is_local(&song) => {
                self.favorites.snapshot.as_ref().is_some_and(|favorites| {
                    favorites.contains(&song.id, &SessionIdentity::local())
                })
            }
            None => None::<SessionIdentity>.is_some_and(|identity| {
                self.favorites
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.contains(&song.id, &identity))
            }),
        };
        let mut playing = Item::action(
            self.t("music.songList.play"),
            "bofang",
            move |this, _, cx| {
                if !this.local_song_playable(&play.id) {
                    return;
                }
                let index = this.enqueue_player_first(play.clone());
                if let Some(quality) = quality {
                    this.player.queue_qualities.insert(play.id.clone(), quality);
                }
                this.play_index(index, cx);
            },
        );
        playing.disabled = !playable;
        let mut adding = Item::action(
            self.t("music.songList.addToPlaylist"),
            "zengjia",
            move |this, window, cx| this.open_add_to_playlist(vec![add.clone()], window, cx),
        );
        // Adding to a playlist is built on Subsonic identity; online songs use favorites.
        adding.disabled = !playable || is_online || self.store.is_none() || self.favorites.busy;
        let mut liking = Item::action(
            self.t(if liked {
                "music.songList.cancelLike"
            } else {
                "music.songList.liked"
            }),
            "TD-Heart",
            move |this, window, cx| this.toggle_favorite(favorite.clone(), window, cx),
        );
        liking.disabled = !playable
            || self.store.is_none()
            || self.favorites.busy
            || self.favorites.snapshot.is_none();
        let mut items = vec![
            playing,
            adding,
            liking,
            Item::separator(),
            if is_online && self.online.is_some() {
                Item::action(
                    self.t("music.songList.download"),
                    "TD-Download",
                    move |this, window, cx| this.download_songs(vec![download.clone()], window, cx),
                )
            } else {
                Item::unavailable(self.t("music.songList.download"), "TD-Download")
            },
        ];
        if self.navigation.current() == Page::Playlist {
            items.push(Item::separator());
            items.push(Item::action(
                self.t("music.songList.moveToPosition"),
                "TD-Swap",
                // The original list.vue has no handler for this emitted event.
                |_, _, _| {},
            ));
            let remove_song = song.clone();
            let mut remove = Item::action(
                self.t("music.songList.removeFromPlaylist"),
                "TD-Delete",
                move |this, window, cx| {
                    this.remove_local_playlist_songs(vec![remove_song.clone()], false, window, cx)
                },
            );
            remove.danger = true;
            remove.disabled = self.store.is_none()
                || self.favorites.busy
                || !self.local_playlists.songs.contains_key(&song.id);
            items.push(remove);
        }
        self.show_context_actions(position, 180., items, window, cx);
    }

    pub(super) fn display_song_quality(&self, id: &str) -> Option<crate::song::SongQuality> {
        let page_quality = match self.navigation.current() {
            Page::Search => self.song_view.search_qualities.get(id),
            Page::Playlist if self.local_song_playable(id) => {
                self.song_view.album_qualities.get(id)
            }
            _ => None,
        };
        page_quality
            .or_else(|| self.favorites.qualities.get(id))
            .copied()
    }

    pub(super) fn song_more_actions(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_context_actions(
            position,
            156.,
            vec![
                Item::action(
                    self.t(if self.song_view.multi {
                        "common.cancelBatch"
                    } else {
                        "common.batchSelect"
                    }),
                    "TD-ViewList",
                    |this, _, cx| {
                        this.song_view.multi = !this.song_view.multi;
                        this.song_view.selected.clear();
                        cx.notify();
                    },
                ),
                match self.listed_online_songs() {
                    songs if !songs.is_empty() && self.online.is_some() => Item::action(
                        self.t("common.downloadAll"),
                        "TD-Download",
                        move |this, window, cx| this.download_songs(songs.clone(), window, cx),
                    ),
                    _ => Item::unavailable(self.t("common.downloadAll"), "TD-Download"),
                },
            ],
            window,
            cx,
        );
    }

    /// Online-source songs of the playlist or album being shown (loaded rows only).
    fn listed_online_songs(&self) -> Vec<Song> {
        match &self.album_songs {
            Remote::Ready(songs) => songs
                .iter()
                .filter(|song| crate::online::is_online(song))
                .cloned()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// "More" menu of an online playlist page: batch select is not offered there.
    pub(super) fn online_playlist_more_actions(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let songs = match &self.song_view.online_playlist.songs {
            Remote::Ready(songs) => songs.clone(),
            _ => Vec::new(),
        };
        let item = if songs.is_empty() || self.online.is_none() {
            Item::unavailable(self.t("common.downloadAll"), "TD-Download")
        } else {
            Item::action(
                self.t("common.downloadAll"),
                "TD-Download",
                move |this, window, cx| this.download_songs(songs.clone(), window, cx),
            )
        };
        self.show_context_actions(position, 156., vec![item], window, cx);
    }
}
