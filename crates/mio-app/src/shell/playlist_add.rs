use super::*;
use crate::library::{self, Playlist, PlaylistDestination, PlaylistTrack};
use gpui_kit::component::{
    WindowExt, button::ButtonCustomVariant, notification::Notification, spinner::Spinner,
};
use std::path::PathBuf;

struct PlaylistAdd {
    parent: WeakEntity<Mio>,
    /// Set when opened from a heart: adds the "My favorites" row for this song.
    favorite: Option<Song>,
    /// The original item of an online song, saved with its source identity.
    online_item: Option<serde_json::Value>,
    path: PathBuf,
    identity: SessionIdentity,
    tracks: Vec<PlaylistTrack>,
    playlists: Remote<Playlist>,
    search: Entity<InputState>,
    name: Entity<InputState>,
    create: bool,
    busy: bool,
    english: bool,
    colors: crate::skin::Colors,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    pub(super) fn open_add_to_playlist(
        &mut self,
        songs: Vec<Song>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_playlist_chooser(songs, None, window, cx);
    }

    /// Hearts: choose "My favorites" or a playlist for one song, local or online.
    pub(super) fn open_song_target(
        &mut self,
        song: Song,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_playlist_chooser(vec![song.clone()], Some(song), window, cx);
    }

    fn open_playlist_chooser(
        &mut self,
        songs: Vec<Song>,
        favorite: Option<Song>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if songs.is_empty() {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let local = songs.iter().all(crate::local_music::is_local);
        if !local && songs.iter().any(crate::local_music::is_local) {
            return;
        }
        // Online songs only come through a heart, one at a time.
        let online_item = if local {
            None
        } else if favorite.is_some() && songs.len() == 1 {
            let Some(item) = self
                .online
                .as_ref()
                .and_then(|online| online.item_json(&songs[0].id))
            else {
                return;
            };
            Some(item)
        } else {
            return;
        };
        if local && songs.iter().any(|song| !self.local_song_playable(&song.id)) {
            return;
        }
        let identity = SessionIdentity::local();
        let tracks = songs
            .into_iter()
            .map(|song| {
                let qualities = if self.navigation.current() == Page::Search {
                    &self.song_view.search_qualities
                } else {
                    &self.song_view.album_qualities
                };
                let quality = qualities
                    .get(&song.id)
                    .or_else(|| self.favorites.qualities.get(&song.id))
                    .copied();
                PlaylistTrack { song, quality }
            })
            .collect();
        let english = self.english;
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let mio = cx.entity();
        let editor = cx.new(|cx| {
            let search = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(i18n::text(english, "common.searchPlaylist"))
            });
            let name = cx.new(|cx| {
                InputState::new(window, cx).placeholder(i18n::text(english, "common.playlistName"))
            });
            let search_events = cx
                .subscribe(&search, |_: &mut PlaylistAdd, _, _: &InputEvent, cx| {
                    cx.notify()
                });
            let name_events = cx.subscribe_in(
                &name,
                window,
                |this: &mut PlaylistAdd, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) && this.create {
                        this.submit(
                            PlaylistDestination::New(this.name.read(cx).value().to_string()),
                            window,
                            cx,
                        );
                    }
                    cx.notify();
                },
            );
            // Re-render when favorites change underneath the open dialog.
            let parent_events = cx.observe(&mio, |_: &mut PlaylistAdd, _, cx| cx.notify());
            PlaylistAdd {
                parent,
                favorite,
                online_item,
                path,
                identity,
                tracks,
                playlists: Remote::Idle,
                search,
                name,
                create: false,
                busy: false,
                english,
                colors,
                scroll: ScrollHandle::new(),
                _subscriptions: vec![search_events, name_events, parent_events],
            }
        });
        editor.update(cx, |this, cx| this.load(cx));
        window.open_dialog(cx, move |dialog, window, cx| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            let state = editor.read(cx);
            let query = state.search.read(cx).value().trim().to_lowercase();
            let count = match &state.playlists {
                Remote::Ready(list) => list
                    .iter()
                    .filter(|p| p.name.to_lowercase().contains(&query))
                    .count(),
                _ => 0,
            };
            let base = if narrow {
                if state.create { 332. } else { 228. }
            } else {
                236.
            } + if state.favorite.is_some() { 64. } else { 0. };
            let body = match &state.playlists {
                Remote::Ready(list) if !state.busy => {
                    if list.is_empty() {
                        if narrow { 188. } else { 179. }
                    } else if count == 0 {
                        88.
                    } else {
                        count as f32 * 68. - 4.
                    }
                }
                _ => 110.,
            };
            let height = (base + body + 2.).min(f32::from(window.viewport_size().height) - 48.);
            let cancel = editor.clone();
            dialog
                .w(px(
                    (width - if narrow { 2. * gutter } else { 32. }).min(440.)
                ))
                .margin_top((window.viewport_size().height - px(height)).max(px(24.)) / 2.)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(22.))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(colors, "--td-text-color-primary").opacity(0.02))
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
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel.read(cx).busy)
                .child(editor.clone())
        });
    }
}

impl PlaylistAdd {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        self.playlists = Remote::Loading;
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { library::list_playlists(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.playlists = match result {
                    Ok(list) => Remote::Ready(list),
                    Err(_) => Remote::Failed(this.t("common.failed")),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn submit(
        &mut self,
        destination: PlaylistDestination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !matches!(self.playlists, Remote::Ready(_)) {
            return;
        }
        if matches!(&destination,PlaylistDestination::New(name) if name.trim().is_empty()) {
            return;
        }
        // Online songs are appended to an existing playlist only.
        if self.online_item.is_some() && !matches!(destination, PlaylistDestination::Existing(_)) {
            return;
        }
        let started = self
            .parent
            .update(cx, |parent, cx| {
                if parent.favorites.busy
                    || self.tracks.iter().any(|track| {
                        crate::local_music::is_local(&track.song)
                            && !parent.local_song_playable(&track.song.id)
                    })
                {
                    return false;
                }
                parent.favorites.busy = true;
                cx.notify();
                true
            })
            .unwrap_or(false);
        if !started {
            window.push_notification(Notification::error(self.t("common.addFailed")), cx);
            return;
        }
        self.busy = true;
        cx.notify();
        let path = self.path.clone();
        let identity = self.identity.clone();
        let tracks = self.tracks.clone();
        if let (Some(item), PlaylistDestination::Existing(destination_id)) =
            (self.online_item.clone(), &destination)
        {
            let destination_id = destination_id.clone();
            let name = match &self.playlists {
                Remote::Ready(list) => list
                    .iter()
                    .find(|playlist| playlist.id == destination_id)
                    .map(|playlist| playlist.name.clone())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            cx.spawn_in(window, async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let source = item
                            .get("source")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .ok_or(())?;
                        let mut cursor =
                            library::LibraryCursor::open_mobile(&path).map_err(|_| ())?;
                        cursor
                            .append_online_songs(
                                &destination_id,
                                &[source.as_str()],
                                &[item],
                                || true,
                            )
                            .map_err(|_| ())
                    })
                    .await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    let _ = this.parent.update(cx, |parent, cx| {
                        parent.favorites.busy = false;
                        cx.notify();
                    });
                    match result {
                        Ok(added) => {
                            let _ = this.parent.update(cx, |parent, cx| {
                                parent.load_playlists(cx);
                                cx.notify();
                            });
                            window.close_dialog(cx);
                            window.push_notification(
                                Notification::success(
                                    this.t("common.addedSongsToPlaylist")
                                        .replace("{count}", &added.to_string())
                                        .replace("{name}", &name),
                                ),
                                cx,
                            );
                        }
                        Err(()) => window
                            .push_notification(Notification::error(this.t("common.addFailed")), cx),
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    library::add_playlist_songs(&path, &identity, destination, &tracks)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let _ = this.parent.update(cx, |parent, cx| {
                    parent.favorites.busy = false;
                    cx.notify();
                });
                match result {
                    Ok((playlist, favorites)) => {
                        let _ = this.parent.update(cx, |parent, cx| {
                            parent.favorites.snapshot = Some(favorites);
                            for track in &this.tracks {
                                if let Some(quality) = track.quality {
                                    parent.remember_favorite_quality(&track.song.id, quality);
                                }
                            }
                            parent.load_playlists(cx);
                            if parent.navigation.current() == Page::Playlist
                                && parent
                                    .local_playlists
                                    .selected
                                    .as_ref()
                                    .is_some_and(|p| p.id == playlist.id)
                            {
                                parent.load_local_playlist(cx);
                            }
                            cx.notify();
                        });
                        window.close_dialog(cx);
                        window.push_notification(
                            Notification::success(
                                this.t("common.addedSongsToPlaylist")
                                    .replace("{count}", &this.tracks.len().to_string())
                                    .replace("{name}", &playlist.name),
                            ),
                            cx,
                        );
                    }
                    Err(_) => window
                        .push_notification(Notification::error(this.t("common.addFailed")), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn field(
        &self,
        state: &Entity<InputState>,
        search: bool,
        narrow: bool,
        window: &Window,
        cx: &App,
    ) -> Input {
        let primary = color(self.colors, "--td-text-color-primary");
        let brand = color(self.colors, "--td-brand-color");
        let focused = state.read(cx).focus_handle(cx).is_focused(window);
        Input::new(state)
            .appearance(false)
            .disabled(self.busy)
            .min_h(px(if narrow { 44. } else { 39. }))
            .max_h(px(if narrow { 44. } else { 39. }))
            .border_1()
            .rounded(px(if search { 11. } else { 10. }))
            .border_color(if focused { brand } else { primary.opacity(0.1) })
            .bg(
                color(self.colors, "--td-bg-color-component").opacity(if focused {
                    0.65
                } else {
                    0.5
                }),
            )
            .text_color(primary)
            .text_size(px(if narrow { 16. } else { 13. }))
            .px(px(12.))
            .when(search, |input| {
                input.prefix(icon("Search").size(px(15.))).cleanable(true)
            })
            .when(focused, |input| {
                input.shadow(vec![BoxShadow {
                    color: brand.opacity(0.12),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            })
    }
}

impl Render for PlaylistAdd {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let placeholder = color(self.colors, "--td-text-color-placeholder");
        let brand = color(self.colors, "--td-brand-color");
        let outline = ButtonCustomVariant::new(cx)
            .color(transparent_black())
            .foreground(primary)
            .hover(primary.opacity(0.03))
            .active(primary.opacity(0.06));
        let save = ButtonCustomVariant::new(cx)
            .color(brand)
            .foreground(white())
            .hover(color(self.colors, "--td-brand-color-hover"))
            .active(color(self.colors, "--td-brand-color-active"));
        let query = self.search.read(cx).value().trim().to_lowercase();
        let favorite_row = self.favorite.clone().map(|song| {
            let (liked, busy) = self
                .parent
                .upgrade()
                .map(|parent| {
                    let parent = parent.read(cx);
                    (parent.song_liked(&song), parent.favorites.busy)
                })
                .unwrap_or((false, true));
            gpui_kit::base::Button::new("add-playlist-favorite")
                .accessibility_label(self.t("music.songList.myFavoriteMusic"))
                .w_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(12.))
                .min_h(px(56.))
                .mb(px(10.))
                .rounded(px(if narrow { 14. } else { 13. }))
                .bg(primary.opacity(0.06))
                .hover(|row| row.bg(primary.opacity(0.12)))
                .disabled(busy || self.busy)
                .child(
                    icon(if liked { "TD-Heart-Filled" } else { "TD-Heart" })
                        .size(px(20.))
                        .text_color(if liked {
                            color(self.colors, "--td-error-color")
                        } else {
                            secondary
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(self.t("music.songList.myFavoriteMusic")),
                        )
                        .when(liked, |col| {
                            col.child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(placeholder)
                                    .child(self.t("play.liked")),
                            )
                        }),
                )
                .when(liked, |row| {
                    row.child(icon("TD-Check").size(px(16.)).text_color(brand))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(song) = this.favorite.clone() {
                        let _ = this.parent.update(cx, |parent, cx| {
                            parent.toggle_favorite(song, window, cx);
                        });
                    }
                }))
        });
        let mut body = div()
            .id("add-playlist-list")
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .max_h(
                (if narrow {
                    (window.viewport_size().height * 0.82).min(px(680.))
                        - px(if self.create { 334. } else { 230. })
                } else {
                    window.viewport_size().height - px(286.)
                } - px(if self.favorite.is_some() { 66. } else { 0. }))
                .max(px(64.)),
            )
            .flex()
            .flex_col()
            .gap(px(4.));
        match &self.playlists {
            Remote::Ready(list) if !self.busy => {
                let filtered: Vec<_> = list
                    .iter()
                    .filter(|p| p.name.to_lowercase().contains(&query))
                    .collect();
                if filtered.is_empty() {
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .p(px(36.))
                            .gap(px(12.))
                            .text_color(secondary)
                            .text_size(px(13.))
                            .when(list.is_empty(), |empty| {
                                empty.child(icon("PlaylistAddCover").size(px(32.)).opacity(0.3))
                            })
                            .child(self.t(if list.is_empty() {
                                "common.noPlaylistYet"
                            } else {
                                "common.noMatchPlaylist"
                            }))
                            .when(list.is_empty(), |empty| {
                                empty.child(
                                    Button::new("empty-add-create")
                                        .label(self.t("common.createPlaylist"))
                                        .custom(save)
                                        .h(px(if narrow { 44. } else { 35. }))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.create = true;
                                            cx.notify();
                                        })),
                                )
                            }),
                    );
                } else {
                    for playlist in filtered {
                        let id = playlist.id.clone();
                        let cover = if playlist.cover_img_url.is_empty()
                            || playlist.cover_img_url == "default-cover"
                        {
                            "images/default-cover.png".to_owned()
                        } else {
                            playlist.cover_img_url.clone()
                        };
                        body = body.child(
                            gpui_kit::base::Button::new(SharedString::from(format!(
                                "add-playlist-{}",
                                playlist.id
                            )))
                            .accessibility_label(format!(
                                "{} · {}",
                                playlist.name,
                                self.t("common.songCountShort")
                                    .replace("{count}", &playlist.song_count.to_string())
                            ))
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(12.))
                            .py(px(10.))
                            .min_h(px(64.))
                            .rounded(px(if narrow { 14. } else { 13. }))
                            .bg(primary.opacity(0.06))
                            .hover(|row| row.bg(primary.opacity(0.12)))
                            .child(
                                img(cover)
                                    .size(px(44.))
                                    .rounded(px(10.))
                                    .object_fit(ObjectFit::Cover)
                                    .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_ellipsis()
                                            .child(playlist.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .text_size(px(11.5))
                                            .text_color(placeholder)
                                            .child(self.t("common.songCountShort").replace(
                                                "{count}",
                                                &playlist.song_count.to_string(),
                                            ))
                                            .when(!playlist.source.is_empty(), |meta| {
                                                meta.child(
                                                    div()
                                                        .px(px(6.))
                                                        .rounded(px(5.))
                                                        .bg(primary.opacity(0.04))
                                                        .text_size(px(10.5))
                                                        .child(playlist.source.clone()),
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                icon("ChevronRight")
                                    .size(px(14.))
                                    .text_color(placeholder)
                                    .opacity(0.4),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.submit(
                                        PlaylistDestination::Existing(id.clone()),
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        );
                    }
                }
            }
            Remote::Failed(message) => {
                body = body.child(
                    div()
                        .p(px(36.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.))
                        .child(message.clone())
                        .child(
                            Button::new("retry-add-playlists")
                                .label(self.t("common.retry"))
                                .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                        ),
                )
            }
            _ => {
                body = body.child(
                    div()
                        .p(px(48.))
                        .flex()
                        .justify_center()
                        .child(Spinner::new()),
                )
            }
        }
        let mut footer = div()
            .flex()
            .gap(px(8.))
            .items_center()
            .when(narrow, |form| form.flex_col().items_stretch());
        if self.online_item.is_some() {
            // Online songs join an existing playlist; there is no create-and-add.
            footer = footer.hidden();
        } else if self.create {
            footer = footer
                .child(
                    self.field(&self.name, false, narrow, window, cx)
                        .flex_1()
                        .min_w_0(),
                )
                .child(
                    Button::new("create-and-add-playlist")
                        .label(self.t("common.createAndAdd"))
                        .custom(save)
                        .h(px(if narrow { 44. } else { 35. }))
                        .rounded(px(if narrow { 12. } else { 11. }))
                        .disabled(
                            self.busy
                                || self.name.read(cx).value().trim().is_empty()
                                || !matches!(self.playlists, Remote::Ready(_)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.submit(
                                PlaylistDestination::New(this.name.read(cx).value().to_string()),
                                window,
                                cx,
                            )
                        })),
                )
                .child(
                    Button::new("cancel-create-and-add")
                        .label(self.t("common.cancel"))
                        .custom(outline)
                        .h(px(if narrow { 44. } else { 35. }))
                        .rounded(px(if narrow { 12. } else { 11. }))
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create = false;
                            this.name
                                .update(cx, |state, cx| state.set_value("", window, cx));
                            cx.notify();
                        })),
                );
        } else {
            footer = footer.child(
                Button::new("start-create-and-add")
                    .w_full()
                    .label(self.t("common.createPlaylist"))
                    .icon(icon("PlaylistCreate").size(px(14.)))
                    .custom(outline)
                    .disabled(self.busy)
                    .h(px(if narrow { 44. } else { 39. }))
                    .rounded(px(if narrow { 12. } else { 11. }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.create = true;
                        cx.notify();
                    })),
            );
        }
        div()
            .p(px(if narrow { 16. } else { 28. }))
            .when(narrow, |root| root.pt(px(12.)))
            .flex()
            .flex_col()
            .min_h_0()
            .text_color(primary)
            .font_weight(FontWeight::NORMAL)
            .when(narrow, |root| {
                root.child(
                    div()
                        .w(px(38.))
                        .h(px(4.))
                        .mx_auto()
                        .mb(px(12.))
                        .rounded(px(999.))
                        .bg(rgb(0x787880).opacity(0.36)),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(if narrow { 14. } else { 18. }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .size(px(if narrow { 38. } else { 42. }))
                                    .rounded(px(13.))
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
                                    .child(
                                        icon("PlaylistAddCover").size(px(22.)).text_color(brand),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(17.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(self.t("common.addToPlaylistTitle")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(primary.opacity(0.55))
                                            .child(self.t("common.songCount").replace(
                                                "{count}",
                                                &self.tracks.len().to_string(),
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        gpui_kit::base::Button::new("close-add-playlist")
                            .accessibility_label(self.t("common.titleBar.close"))
                            .disabled(self.busy)
                            .size(px(if narrow { 44. } else { 30. }))
                            .rounded(px(if narrow { 12. } else { 9. }))
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
                                    .text_color(color(self.colors, "--td-error-color"))
                            })
                            .child(icon("WindowClose").size(px(13.)))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(
                div()
                    .mb(px(14.))
                    .child(self.field(&self.search, true, narrow, window, cx)),
            )
            .children(favorite_row)
            .child(body)
            .child(
                div()
                    .mt(px(if narrow { 12. } else { 14. }))
                    .pt(px(if narrow { 12. } else { 14. }))
                    .border_t_1()
                    .border_color(primary.opacity(0.06))
                    .child(footer),
            )
    }
}
