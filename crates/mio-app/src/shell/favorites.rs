use super::*;
use crate::library::{self, Favorites, LibraryError};
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant, notification::Notification};

#[derive(Default)]
pub(super) struct FavoriteState {
    pub(super) snapshot: Option<Favorites>,
    pub(super) busy: bool,
    pub qualities: std::collections::HashMap<String, crate::song::SongQuality>,
}

impl Mio {
    pub(super) fn load_favorites(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.favorites.busy {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        self.favorites.busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { library::load_favorites(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.favorites.busy = false;
                match result {
                    Ok(snapshot) => this.favorites.snapshot = Some(snapshot),
                    Err(error) => this.favorite_error(error, window, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn favorite_error(&self, error: LibraryError, window: &mut Window, cx: &mut Context<Self>) {
        let message = match (error, self.english) {
            (LibraryError::DifferentSource, false) => {
                "此歌曲 ID 已属于其他音乐源或旧数据，原收藏已保留。"
            }
            (LibraryError::DifferentSource, true) => {
                "This song ID belongs to another source or legacy data. The existing favorite was preserved."
            }
            (LibraryError::Unavailable, false) => "收藏读取或保存失败，原数据已保留。",
            (LibraryError::Unavailable, true) => {
                "Unable to read or save favorites. Existing data was preserved."
            }
        };
        window.push_notification(Notification::error(message), cx);
    }

    pub(super) fn favorite_button(
        &self,
        id: impl Into<ElementId>,
        song: Option<&Song>,
        row: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let mode = self.ui_colors();
        let identity = None::<SessionIdentity>;
        let online_key = song.and_then(|song| crate::online::parse_song_id(&song.id));
        let identity = if song.is_some_and(crate::local_music::is_local) {
            Some(SessionIdentity::local())
        } else {
            identity
        };
        let liked = match (&online_key, song) {
            (Some((source, raw)), _) => self
                .favorites
                .snapshot
                .as_ref()
                .is_some_and(|favorites| favorites.contains_online(source, raw)),
            (None, Some(song)) => identity.as_ref().is_some_and(|identity| {
                self.favorites
                    .snapshot
                    .as_ref()
                    .is_some_and(|favorites| favorites.contains(&song.id, identity))
            }),
            (None, None) => false,
        };
        let unavailable = self.favorites.snapshot.is_none() && !self.favorites.busy;
        let label = if unavailable {
            if self.english {
                "Reload favorites"
            } else {
                "重新读取收藏"
            }
            .to_owned()
        } else {
            self.t(if liked { "play.liked" } else { "play.like" })
        };
        let song = song.cloned();
        control(
            id,
            if liked {
                "TD-Heart-Filled"
            } else if row {
                "TD-Heart"
            } else {
                "FavoriteBorder"
            },
            label,
            mode,
            cx,
        )
        .when(liked, |button| {
            button.custom(
                ButtonCustomVariant::new(cx)
                    .foreground(if row {
                        color(mode, "--td-error-color")
                    } else {
                        rgb(0xff7878).into()
                    })
                    .hover(color(mode, "--td-bg-color-component-hover"))
                    .active(color(mode, "--td-bg-color-component-active")),
            )
        })
        .disabled(
            song.as_ref().is_none()
                || (if online_key.is_some() {
                    self.online.is_none()
                } else {
                    identity.is_none()
                })
                || self.favorites.busy
                || self.store.is_none(),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            if this.favorites.snapshot.is_none() {
                this.load_favorites(window, cx);
                return;
            }
            // Hearts open the chooser: "My favorites" or one of the user's playlists.
            if let Some(song) = &song {
                this.open_song_target(song.clone(), window, cx);
            }
        }))
    }

    pub(super) fn toggle_favorite(
        &mut self,
        song: Song,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.favorites.busy {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        if let Some((source, raw)) = crate::online::parse_song_id(&song.id) {
            self.toggle_online_favorite(song, source, raw, path, window, cx);
            return;
        }
        let identity = if crate::local_music::is_local(&song) {
            if !self.local_song_playable(&song.id) {
                return;
            }
            SessionIdentity::local()
        } else {
            let Some(identity) = None::<SessionIdentity> else {
                return;
            };
            identity
        };
        let Some(snapshot) = &self.favorites.snapshot else {
            return;
        };
        let liked = !snapshot.contains(&song.id, &identity);
        let quality = self
            .display_song_quality(&song.id)
            .or_else(|| self.player.queue_qualities.get(&song.id).copied());
        if let Some(quality) = quality {
            self.remember_favorite_quality(&song.id, quality);
        }
        let name = self.t("music.songList.myFavoriteMusic");
        let description = self.t("music.songList.collectedSongs");
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    library::set_favorite(
                        &path,
                        &identity,
                        &song,
                        quality,
                        liked,
                        &name,
                        &description,
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.favorites.busy = false;
                match result {
                    Ok(snapshot) => {
                        this.favorites.snapshot = Some(snapshot);
                        this.local_playlists_changed(cx);
                    }
                    Err(error) => this.favorite_error(error, window, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn remember_favorite_quality(
        &mut self,
        id: &str,
        quality: crate::song::SongQuality,
    ) {
        // Keep known metadata through a favorite removal/re-add and page reload.
        // SourceChanged clears this source-scoped cache; no guessed tier is stored.
        if self.favorites.qualities.len() >= 256 && !self.favorites.qualities.contains_key(id) {
            self.favorites.qualities.clear();
        }
        self.favorites.qualities.insert(id.to_owned(), quality);
    }

    /// Online songs are stored with their original MusicItem so they can be resolved later; no
    /// Subsonic identity is attached.
    fn toggle_online_favorite(
        &mut self,
        song: Song,
        source: String,
        raw: String,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = &self.favorites.snapshot else {
            return;
        };
        let Some(item) = self
            .online
            .as_ref()
            .and_then(|online| online.item_json(&song.id))
        else {
            window.push_notification(Notification::error(self.t("download.unknownError")), cx);
            return;
        };
        let liked = !snapshot.contains_online(&source, &raw);
        let name = self.t("music.songList.myFavoriteMusic");
        let description = self.t("music.songList.collectedSongs");
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    library::set_online_favorite(&path, &item, liked, &name, &description)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.favorites.busy = false;
                match result {
                    Ok(snapshot) => {
                        this.favorites.snapshot = Some(snapshot);
                        this.local_playlists_changed(cx);
                    }
                    Err(error) => this.favorite_error(error, window, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
