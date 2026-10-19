use super::*;
use crate::library::{self, Playlist};
use gpui_kit::component::{
    WindowExt,
    button::ButtonCustomVariant,
    input::{Textarea, TextareaState},
    notification::Notification,
};
use std::path::PathBuf;

struct PlaylistEditor {
    parent: WeakEntity<Mio>,
    path: PathBuf,
    id: Option<String>,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    english: bool,
    colors: crate::skin::Colors,
    busy: bool,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    pub(super) fn open_playlist_editor(
        &mut self,
        playlist: Option<Playlist>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let english = self.english;
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let editing = playlist.is_some();
        let editor = cx.new(|cx| {
            let name = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(i18n::text(
                        english,
                        "music.songlist.playlistNamePlaceholder",
                    ))
                    .validate(move |value, _| {
                        if editing {
                            value.chars().count() <= 50
                        } else {
                            value.encode_utf16().count() <= 50
                        }
                    })
            });
            let description = cx.new(|cx| {
                let mut state = TextareaState::new(window, cx)
                    .placeholder(i18n::text(
                        english,
                        "music.songlist.playlistDescPlaceholder",
                    ))
                    .rows(3);
                if editing {
                    state.set_auto_grow(3, 6, cx);
                }
                state
            });
            if let Some(playlist) = &playlist {
                name.update(cx, |state, cx| state.set_value(&playlist.name, window, cx));
                description.update(cx, |state, cx| {
                    state.set_value(&playlist.description, window, cx)
                });
            }
            let name_events = cx.subscribe_in(
                &name,
                window,
                |this: &mut PlaylistEditor, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) && this.id.is_none() {
                        this.submit(window, cx);
                    }
                    cx.notify();
                },
            );
            let description_events = cx.subscribe_in(
                &description,
                window,
                |this: &mut PlaylistEditor, _, event, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = this.description.read(cx).value();
                        if value.encode_utf16().count() > 200 {
                            let mut units = 0;
                            let shortened: String = value
                                .chars()
                                .take_while(|ch| {
                                    units += ch.len_utf16();
                                    units <= 200
                                })
                                .collect();
                            this.description
                                .update(cx, |state, cx| state.set_value(shortened, window, cx));
                        }
                    }
                    cx.notify();
                },
            );
            PlaylistEditor {
                parent,
                path,
                id: playlist.map(|playlist| playlist.id),
                name,
                description,
                english,
                colors,
                busy: false,
                _subscriptions: vec![name_events, description_events],
            }
        });
        window.open_dialog(cx, {
            let editor = editor.clone();
            move |dialog, window, _| {
                let narrow = window.viewport_size().width <= px(768.);
                let width = f32::from(window.viewport_size().width);
                let gutter = (width * 0.04).clamp(16., 24.);
                let cancel_editor = editor.clone();
                dialog
                    .w(px((width - if narrow { 2. * gutter } else { 32. })
                        .min(if editing { 500. } else { 440. })))
                    .margin_top(
                        (window.viewport_size().height - px(if editing { 360. } else { 352. }))
                            .max(px(24.))
                            / 2.,
                    )
                    .when(narrow && !editing, |dialog| dialog.bottom_offset(px(12.)))
                    .rounded(px(if narrow && !editing {
                        18.
                    } else if editing {
                        6.
                    } else {
                        22.
                    }))
                    .p(px(0.))
                    .gap(px(0.))
                    .when(!editing, |dialog| {
                        dialog
                            .bg(color(colors, "--td-text-color-primary").opacity(0.02))
                            .border_color(white().opacity(0.4))
                    })
                    .overlay_tint(if narrow && !editing {
                        color(colors, "--mobile-scrim")
                    } else {
                        black().opacity(0.45)
                    })
                    .when(cfg!(target_os = "macos") && !editing, |dialog| {
                        dialog
                            .overlay_filter(
                                if narrow { 12. } else { 8. },
                                if narrow { 1.8 } else { 1.4 },
                            )
                            .surface_filter(6.56, 1.8, px(if narrow { 18. } else { 22. }))
                    })
                    .close_button(false)
                    .on_ok(|_, _, _| false)
                    .on_cancel(move |_, _, cx| !cancel_editor.read(cx).busy)
                    .child(editor.clone())
            }
        });
        let name = editor.read(cx).name.clone();
        name.update(cx, |state, cx| state.focus(window, cx));
    }
}

impl PlaylistEditor {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name.read(cx).value().trim().to_owned();
        let description = self.description.read(cx).value().trim().to_owned();
        if name.is_empty() {
            window.push_notification(
                Notification::warning(self.t("music.songlist.nameEmpty")),
                cx,
            );
            return;
        }
        self.busy = true;
        cx.notify();
        let path = self.path.clone();
        let id = self.id.clone();
        let editing = id.is_some();
        cx.spawn_in(window, async move |this, cx| {
            let result =
                cx.background_executor()
                    .spawn(async move {
                        library::save_playlist(&path, id.as_deref(), &name, &description)
                    })
                    .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(saved) => {
                        let _ = this.parent.update(cx, |parent, cx| {
                            // Reload from committed data only. Song/queue/favorite state is untouched.
                            parent.load_playlists(cx);
                            if parent
                                .local_playlists
                                .selected
                                .as_ref()
                                .is_some_and(|playlist| playlist.id == saved.id)
                            {
                                parent.local_playlists.selected = Some(saved.clone());
                                if let Some(album) = &mut parent.selected_album {
                                    album.name = saved.name.clone();
                                    album.artist = saved.description.clone();
                                }
                            }
                            cx.notify();
                        });
                        window.close_dialog(cx);
                        window.push_notification(
                            Notification::success(this.t(if editing {
                                "music.songlist.updateSuccess"
                            } else {
                                "music.songlist.createSuccess"
                            })),
                            cx,
                        );
                    }
                    Err(_) => window.push_notification(
                        Notification::error(this.t(if editing {
                            "music.songlist.updateFailed"
                        } else {
                            "music.songlist.createFailed"
                        })),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for PlaylistEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editing = self.id.is_some();
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let brand = color(self.colors, "--td-brand-color");
        let empty = self.name.read(cx).value().trim().is_empty();
        let cancel_style = ButtonCustomVariant::new(cx)
            .color(transparent_black())
            .foreground(primary)
            .hover(primary.opacity(0.03))
            .active(primary.opacity(0.06));
        let save_style = ButtonCustomVariant::new(cx)
            .color(brand)
            .foreground(white())
            .hover(color(self.colors, "--td-brand-color-hover"))
            .active(color(self.colors, "--td-brand-color-active"));
        let name_focused = self.name.read(cx).focus_handle(cx).is_focused(window);
        let description_focused = self
            .description
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let field_bg = color(self.colors, "--td-bg-color-component").opacity(0.5);
        let field_radius = if editing { 3. } else { 11. };
        let mut name = Input::new(&self.name)
            .aria_label(self.t("music.songlist.playlistName"))
            .disabled(self.busy)
            .min_h(px(if editing {
                32.
            } else if narrow {
                44.
            } else {
                39.
            }))
            .max_h(px(if editing {
                32.
            } else if narrow {
                44.
            } else {
                39.
            }))
            .appearance(false)
            .border_1()
            .px(px(14.))
            .rounded(px(field_radius))
            .bg(field_bg)
            .border_color(primary.opacity(0.1))
            .when(name_focused, |input| {
                input
                    .border_color(brand)
                    .bg(color(self.colors, "--td-bg-color-component").opacity(0.65))
                    .shadow(vec![BoxShadow {
                        color: brand.opacity(0.12),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(3.),
                        inset: false,
                    }])
            })
            .text_size(px(if narrow && !editing { 16. } else { 14. }));
        if editing {
            name = name.cleanable(true);
        }
        div()
            .id("playlist-editor-content")
            .w_full()
            .max_h(window.viewport_size().height * if narrow && !editing { 0.82 } else { 0.9 })
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .p(px(if editing {
                24.
            } else if narrow {
                16.
            } else {
                28.
            }))
            .when(narrow && !editing, |body| body.pt(px(12.)))
            .text_color(primary)
            .font_weight(FontWeight::NORMAL)
            .when(narrow && !editing, |body| {
                body.child(
                    div()
                        .w(px(38.))
                        .h(px(4.))
                        .flex_shrink_0()
                        .mx_auto()
                        .mb(px(12.))
                        .rounded_full()
                        .bg(rgb(0x787880).opacity(0.36)),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .mb(px(if editing {
                        24.
                    } else if narrow {
                        16.
                    } else {
                        22.
                    }))
                    .when(!editing, |header| {
                        header.child(
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
                                .child(icon("PlaylistCreate").size(px(22.))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(if editing { 20. } else { 17. }))
                            .line_height(relative(1.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.t(if editing {
                                "music.songlist.editPlaylistInfo"
                            } else {
                                "music.songlist.createNewPlaylist"
                            })),
                    )
                    .child(
                        gpui_kit::base::Button::new("close-playlist-editor")
                            .accessibility_label(self.t("common.titleBar.close"))
                            .disabled(self.busy)
                            .size(px(if narrow && !editing { 44. } else { 30. }))
                            .rounded(px(if narrow && !editing { 999. } else { 9. }))
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
                                    .text_color(color(self.colors, "--td-error-color"))
                            })
                            .child(icon("WindowClose").size(px(13.)))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(if editing { 8. } else { 6. }))
                    .mb(px(if editing { 24. } else { 16. }))
                    .child(
                        div()
                            .line_height(relative(1.2))
                            .text_size(px(if editing { 14. } else { 12. }))
                            .font_weight(if editing {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(primary.opacity(if editing { 1. } else { 0.75 }))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(self.t("music.songlist.playlistName"))
                            .when(!editing, |label| {
                                label.child(
                                    div()
                                        .text_color(color(self.colors, "--td-error-color"))
                                        .child("*"),
                                )
                            }),
                    )
                    // The original uses show-word-limit, which TDesign Input does not expose.
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(if editing { 8. } else { 6. }))
                    .child(
                        div()
                            .line_height(relative(1.2))
                            .text_size(px(if editing { 14. } else { 12. }))
                            .font_weight(if editing {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .text_color(primary.opacity(if editing { 1. } else { 0.75 }))
                            .child(self.t("music.songlist.playlistDesc")),
                    )
                    .child(
                        Textarea::new(&self.description)
                            .aria_label(self.t("music.songlist.playlistDesc"))
                            .disabled(self.busy)
                            .when(!editing, |input| input.h(px(74.)))
                            .appearance(false)
                            .border_1()
                            .rounded(px(field_radius))
                            .bg(field_bg)
                            .border_color(primary.opacity(0.1))
                            .when(description_focused, |input| {
                                input
                                    .border_color(brand)
                                    .bg(color(self.colors, "--td-bg-color-component").opacity(0.65))
                                    .shadow(vec![BoxShadow {
                                        color: brand.opacity(0.12),
                                        offset: point(px(0.), px(0.)),
                                        blur_radius: px(0.),
                                        spread_radius: px(3.),
                                        inset: false,
                                    }])
                            })
                            .text_size(px(if narrow && !editing { 16. } else { 14. })),
                    )
                    .when(editing, |field| {
                        field.child(
                            div()
                                .text_right()
                                .text_size(px(12.))
                                .text_color(secondary)
                                .child(format!(
                                    "{}/200",
                                    self.description.read(cx).value().encode_utf16().count()
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(if narrow && !editing { 8. } else { 10. }))
                    .mt(px(if narrow && !editing { 20. } else { 24. }))
                    .when(narrow && !editing, |footer| footer.flex_col_reverse())
                    .child(
                        Button::new("cancel-playlist-editor")
                            .label(self.t("common.cancel"))
                            .disabled(self.busy)
                            .custom(cancel_style)
                            .border_1()
                            .border_color(primary.opacity(0.08))
                            .min_w(px(92.))
                            .h(px(if narrow && !editing { 44. } else { 38. }))
                            .px(px(18.))
                            .rounded(px(if narrow && !editing { 999. } else { 11. }))
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(narrow && !editing, |button| button.w_full())
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-playlist-editor")
                            .label(self.t(if editing {
                                "common.save"
                            } else {
                                "common.create"
                            }))
                            .disabled(self.busy)
                            .loading(self.busy)
                            .custom(save_style)
                            .when(empty && !editing, |button| button.opacity(0.45))
                            .min_w(px(92.))
                            .h(px(if narrow && !editing { 44. } else { 38. }))
                            .px(px(18.))
                            .rounded(px(if narrow && !editing { 999. } else { 11. }))
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .when(narrow && !editing, |button| button.w_full())
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
