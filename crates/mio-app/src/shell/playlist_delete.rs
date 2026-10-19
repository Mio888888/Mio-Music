use super::*;
use crate::library::{self, Playlist};
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant, notification::Notification};
use std::path::PathBuf;

struct PlaylistDelete {
    parent: WeakEntity<Mio>,
    path: PathBuf,
    playlist: Playlist,
    colors: crate::skin::Colors,
    english: bool,
    busy: bool,
    scroll: ScrollHandle,
}

impl Mio {
    pub(super) fn confirm_delete_playlist(
        &mut self,
        playlist: Playlist,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.favorites.busy {
            return;
        }
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let confirmation = cx.new(|_| PlaylistDelete {
            parent,
            path,
            playlist,
            colors,
            english,
            busy: false,
            scroll: ScrollHandle::new(),
        });
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            let cancel = confirmation.clone();
            dialog
                .w(px((width - 2. * gutter).min(if narrow {
                    420.
                } else {
                    480.
                })))
                .margin_top(window.viewport_size().height * 0.2)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(
                    colors,
                    if narrow {
                        "--mobile-glass-bg-strong"
                    } else {
                        "--td-bg-color-container"
                    },
                ))
                .border_color(color(
                    colors,
                    if narrow {
                        "--mobile-glass-border"
                    } else {
                        "--td-border-level-1-color"
                    },
                ))
                .overlay_tint(color(
                    colors,
                    if narrow {
                        "--mobile-scrim"
                    } else {
                        "--td-mask-active"
                    },
                ))
                .when(cfg!(target_os = "macos"), |dialog| {
                    dialog
                        .overlay_filter(
                            if narrow { 12. } else { 5. },
                            if narrow { 1.8 } else { 1. },
                        )
                        .when(narrow, |dialog| dialog.surface_filter(12., 1.8, px(22.)))
                })
                .close_button(false)
                // TDesign's confirmOnEnter defaults to false.
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !cancel.read(cx).busy)
                .child(confirmation.clone())
        });
    }
}

impl PlaylistDelete {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let started = self
            .parent
            .update(cx, |parent, cx| {
                if parent.favorites.busy {
                    return false;
                }
                // Share the favorites write gate, including reads that could restore
                // a pre-delete snapshot. Invalidate a pending card-play request.
                parent.favorites.busy = true;
                parent.local_playlists.play_revision += 1;
                cx.notify();
                true
            })
            .unwrap_or(false);
        if !started {
            return;
        }
        self.busy = true;
        cx.notify();
        let path = self.path.clone();
        let id = self.playlist.id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { library::delete_playlist(&path, &id) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let successful = result.is_ok();
                let _ = this.parent.update(cx, |parent, cx| {
                    parent.favorites.busy = false;
                    if let Ok(favorites) = result {
                        parent.favorites.snapshot = Some(favorites);
                        if parent
                            .local_playlists
                            .selected
                            .as_ref()
                            .is_some_and(|p| p.id == this.playlist.id)
                        {
                            parent.local_playlists.selected = None;
                            parent.local_playlists.songs.clear();
                            if parent.navigation.current() == Page::Playlist {
                                parent.album_song_revision += 1;
                                parent.album_songs = Remote::Failed(parent.t("common.failed"));
                                parent.selected_album = None;
                            }
                        }
                        parent.load_playlists(cx);
                    }
                    cx.notify();
                });
                this.busy = false;
                window.close_dialog(cx);
                let message = this.t(if successful {
                    "music.songlist.deleteSuccess"
                } else {
                    "music.songlist.deleteFailed"
                });
                window.push_notification(
                    if successful {
                        Notification::success(message)
                    } else {
                        Notification::error(message)
                    },
                    cx,
                );
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for PlaylistDelete {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let surface = color(self.colors, "--td-bg-color-container");
        // A solid fill with a white label: the fixed red, not the cover-adjusted one.
        let danger = color(self.colors, "--td-error-color-6");
        let cancel_style = ButtonCustomVariant::new(cx)
            .color(if narrow {
                surface.opacity(0.7)
            } else {
                surface
            })
            .foreground(primary)
            .hover(color(self.colors, "--td-bg-color-container-hover"))
            .active(color(self.colors, "--td-bg-color-container-active"));
        let danger_style = ButtonCustomVariant::new(cx)
            .color(danger)
            .foreground(color(self.colors, "--td-font-white-1"))
            .hover(color(self.colors, "--td-error-color-hover"))
            .active(color(self.colors, "--td-error-color-active"));
        let body = self
            .t("music.songlist.deleteConfirm")
            .replace("{name}", &self.playlist.name);
        div()
            .relative()
            .p(px(if narrow { 16. } else { 32. }))
            .when(narrow, |div| div.pt(px(12.)))
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
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
                    .gap(px(8.))
                    .pr(px(28.))
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        icon("TD-ErrorCircle-Filled")
                            .size(px(24.))
                            .text_color(danger),
                    )
                    .child(self.t("music.songlist.confirmDelete")),
            )
            .child(
                gpui_kit::base::Button::new("close-delete-playlist")
                    .accessibility_label(self.t("common.titleBar.close"))
                    .disabled(self.busy)
                    .absolute()
                    .top(px(if narrow { 10. } else { 32. }))
                    .right(px(if narrow { 10. } else { 32. }))
                    .size(px(if narrow { 44. } else { 20. }))
                    .rounded(px(if narrow { 999. } else { 3. }))
                    .when(narrow, |button| button.bg(surface.opacity(0.6)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(secondary)
                    .hover(|button| button.bg(color(self.colors, "--td-bg-color-container-hover")))
                    .child(icon("WindowClose").size(px(20.)))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                div()
                    .id("playlist-delete-body")
                    .py(px(if narrow { 14. } else { 16. }))
                    .max_h((window.viewport_size().height * 0.76 - px(144.)).max(px(48.)))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .line_height(px(22.))
                    .text_color(secondary)
                    .child(body),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(if narrow { 16. } else { 8. }))
                    .pt(px(if narrow { 12. } else { 16. }))
                    .when(narrow, |div| div.flex_row_reverse())
                    .child(
                        Button::new("cancel-delete-playlist")
                            .label(self.t("common.cancel"))
                            .custom(cancel_style)
                            .outline()
                            .disabled(self.busy)
                            .h(px(if narrow { 44. } else { 32. }))
                            .rounded(px(if narrow { 12. } else { 3. }))
                            .when(narrow, |button| button.flex_1())
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("confirm-delete-playlist")
                            .label(self.t("common.delete"))
                            .custom(danger_style)
                            .disabled(self.busy)
                            .h(px(if narrow { 44. } else { 32. }))
                            .rounded(px(if narrow { 12. } else { 3. }))
                            .when(narrow, |button| button.flex_1())
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
