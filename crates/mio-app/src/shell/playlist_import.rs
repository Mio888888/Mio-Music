use super::*;
use crate::library::{self, Favorites, Playlist, PlaylistImportError, PlaylistTrack};
use gpui_kit::component::{WindowExt, notification::Notification};

impl Mio {
    pub(super) fn open_playlist_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.favorites.busy || self.store.is_none() {
            return;
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        window.open_dialog(cx, move |dialog, window, _cx| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            let primary = color(colors, "--td-text-color-primary");
            let secondary = color(colors, "--td-text-color-secondary");
            let brand = color(colors, "--td-brand-color");
            let text = |key| i18n::text(english, key);
            let mut options = div().flex().flex_col().gap(px(8.));
            for (index, glyph_name, title, description) in [
                (
                    0,
                    "liebiao",
                    "music.songlist.fromPlayingList",
                    "music.songlist.fromPlayingListDesc",
                ),
                (
                    1,
                    "daoru",
                    "music.songlist.fromLocalFile",
                    "music.songlist.fromLocalFileDesc",
                ),
                (
                    2,
                    "wangluo",
                    "music.songlist.fromNetwork",
                    "music.songlist.fromNetworkDesc",
                ),
            ] {
                let parent = parent.clone();
                options = options.child(
                    gpui_kit::base::Button::new(("playlist-import-option", index as usize))
                        .accessibility_label(text(title))
                        .w_full()
                        .min_h(px(60.))
                        .px(px(if narrow { 14. } else { 16. }))
                        .py(px(if narrow { 12. } else { 14. }))
                        .rounded(px(if narrow { 14. } else { 13. }))
                        .border_1()
                        .border_color(primary.opacity(0.08))
                        .bg(primary.opacity(0.05))
                        .flex()
                        .items_center()
                        .hover(|row| {
                            row.bg(primary.opacity(0.1))
                                .border_color(primary.opacity(0.14))
                        })
                        .when(index == 0, |row| {
                            row.child(glyph(glyph_name).size(px(20.8)).text_color(brand))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .ml(px(14.))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .line_height(relative(1.3))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(text(title)),
                                )
                                .child(
                                    div()
                                        .mt(px(3.))
                                        .text_size(px(12.))
                                        .line_height(relative(1.4))
                                        .text_color(secondary)
                                        .child(text(description)),
                                )
                                .when(index == 2, |column| {
                                    column.child(
                                        div().mt(px(4.)).flex().items_start().child(
                                            div()
                                                .px(px(6.))
                                                .py(px(1.))
                                                .rounded(px(4.))
                                                .border_1()
                                                .border_color(rgb(0xedb500).opacity(0.2))
                                                .bg(rgb(0xedb500).opacity(0.12))
                                                .text_color(color(colors, "--td-warning-color"))
                                                .text_size(px(10.))
                                                .child(text("common.experimental")),
                                        ),
                                    )
                                }),
                        )
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = parent.update(cx, |parent, cx| {
                                if index == 0 {
                                    parent.import_playlist_queue(window, cx);
                                } else if index == 1 {
                                    parent.import_playlist_file(window, cx);
                                } else if index == 2 {
                                    parent.open_network_import(window, cx);
                                }
                            });
                        }),
                );
            }
            let content = div()
                .p(px(if narrow { 16. } else { 28. }))
                .when(narrow, |root| root.pt(px(12.)))
                .flex()
                .flex_col()
                .text_color(primary)
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
                        .mb(px(if narrow { 16. } else { 20. }))
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
                                            icon("PlaylistImport").size(px(22.)).text_color(brand),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(17.))
                                        .line_height(relative(1.2))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(text("music.songlist.selectImportMethod")),
                                ),
                        )
                        .child(
                            gpui_kit::base::Button::new("close-playlist-import")
                                .accessibility_label(text("common.titleBar.close"))
                                .size(px(if narrow { 44. } else { 30. }))
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
                                        .text_color(color(colors, "--td-error-color"))
                                })
                                .child(icon("WindowClose").size(px(13.)))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        ),
                )
                .child(options);
            dialog
                .w(px((width - if narrow { 2. * gutter } else { 32. })
                    .min(if narrow { 440. } else { 500. })))
                .margin_top((window.viewport_size().height - px(352.)).max(px(24.)) / 2.)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(22.))
                .p(px(0.))
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
                .on_ok(|_, _, _| false)
                .child(content)
        });
    }

    fn import_playlist_queue(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.player.queue.iter().any(crate::online::is_online) {
            window.push_notification(
                Notification::error(if self.english {
                    "Importing a queue containing online songs is not supported yet"
                } else {
                    "暂不支持将含在线歌曲的队列导入歌单"
                }),
                cx,
            );
            return;
        }
        if self.favorites.busy {
            return;
        }
        if self.player.queue.is_empty() {
            window.push_notification(
                Notification::warning(self.t("music.songlist.playingListEmpty")),
                cx,
            );
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let Some(identity) = None::<SessionIdentity> else {
            return;
        };
        let tracks: Vec<_> = self
            .player
            .queue
            .iter()
            .map(|song| PlaylistTrack {
                song: song.clone(),
                quality: self
                    .player
                    .queue_qualities
                    .get(&song.id)
                    .or_else(|| self.favorites.qualities.get(&song.id))
                    .copied(),
            })
            .collect();
        let description = self
            .t("music.songlist.fromPlaylistImport")
            .replace("{count}", &tracks.len().to_string());
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    library::import_current_queue(&path, &identity, &tracks, &description)
                        .map_err(|_| PlaylistImportError::Save)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_playlist_import(result, true, window, cx)
            });
        })
        .detach();
    }

    fn import_playlist_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.favorites.busy {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("music.songlist.fromLocalFile").into()),
        });
        let description = self.t("music.songlist.fromFileImport");
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = match selection.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(file) = paths.into_iter().next() {
                        Some(
                            cx.background_executor()
                                .spawn(async move {
                                    library::import_playlist_file(&path, &file, &description)
                                })
                                .await,
                        )
                    } else {
                        None
                    }
                }
                Ok(Ok(None)) => None,
                _ => Some(Err(PlaylistImportError::Save)),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if let Some(result) = result {
                    this.finish_playlist_import(result, false, window, cx);
                } else {
                    this.favorites.busy = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn finish_playlist_import(
        &mut self,
        result: std::result::Result<(Playlist, Favorites, usize), PlaylistImportError>,
        queue: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.favorites.busy = false;
        match result {
            Ok((playlist, favorites, count)) => {
                self.favorites.snapshot = Some(favorites);
                self.load_playlists(cx);
                window.push_notification(
                    Notification::success(
                        self.t(if queue {
                            "music.songlist.fromPlaylistSuccess"
                        } else {
                            "music.songlist.fromFileSuccess"
                        })
                        .replace("{count}", &count.to_string())
                        .replace("{name}", &playlist.name),
                    ),
                    cx,
                );
            }
            Err(error) => {
                let message = self.t(match error {
                    PlaylistImportError::Parse => "music.songlist.parseFileFailed",
                    PlaylistImportError::Format => "music.songlist.fileFormatError",
                    PlaylistImportError::Save if queue => "music.songlist.fromPlaylistFailed",
                    PlaylistImportError::Save => "common.failed",
                });
                window.push_notification(Notification::error(message), cx);
            }
        }
        cx.notify();
    }
}
