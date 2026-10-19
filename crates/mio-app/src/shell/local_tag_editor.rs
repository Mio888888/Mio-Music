//! `LocalTagEditorPage.vue`: four editable tags, read-only path, explicit save and back.
use super::*;
use crate::local_music::{TagValues, Track};
use gpui_kit::component::{WindowExt, notification::Notification};

pub(super) struct TagEditor {
    parent: WeakEntity<Mio>,
    track: Track,
    inputs: [Entity<InputState>; 4],
    busy: bool,
    loaded: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    pub(super) fn open_local_tag_editor(
        &mut self,
        track: Track,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.local_music.busy {
            return;
        }
        let english = self.english;
        let parent = cx.weak_entity();
        let editor = cx.new(|cx| {
            let values = [
                track.song.title.clone(),
                track.song.artist.clone(),
                track.song.album.clone(),
                track.year.to_string(),
            ];
            let labels = ["songName", "artist", "album", "year"];
            let inputs: [Entity<InputState>; 4] = std::array::from_fn(|i| {
                cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder(i18n::text(
                        english,
                        &format!("music.tagEditor.{}", labels[i]),
                    ));
                    state.set_value(values[i].clone(), window, cx);
                    state
                })
            });
            let subscriptions = inputs
                .iter()
                .map(|input| {
                    cx.subscribe(input, |_: &mut TagEditor, _, _: &InputEvent, cx| {
                        cx.notify()
                    })
                })
                .collect();
            TagEditor {
                parent,
                track,
                inputs,
                busy: false,
                loaded: false,
                error: None,
                _subscriptions: subscriptions,
            }
        });
        let focus = editor.read(cx).inputs[0].clone();
        self.local_music.editor = Some(editor.clone());
        self.go(Page::LocalTagEditor, cx);
        let path = editor.read(cx).track.path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::local_music::refresh_track(&path) });
        cx.spawn_in(window, async move |_, cx| {
            let result = task.await;
            let _ = editor.update_in(cx, |editor, window, cx| {
                match result {
                    Ok(track) => {
                        let values = [
                            track.song.title.clone(),
                            track.song.artist.clone(),
                            track.song.album.clone(),
                            track.year.to_string(),
                        ];
                        for (input, value) in editor.inputs.iter().zip(values) {
                            input.update(cx, |state, cx| state.set_value(value, window, cx));
                        }
                        editor.track = track;
                        editor.loaded = true;
                        if editor.parent.upgrade().is_some_and(|parent| {
                            let parent = parent.read(cx);
                            parent.navigation.current() == Page::LocalTagEditor
                                && parent.local_music.editor.as_ref().is_some_and(|current| {
                                    current.entity_id() == cx.entity().entity_id()
                                })
                        }) {
                            focus.update(cx, |state, cx| state.focus(window, cx));
                        }
                    }
                    Err(message) => editor.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn local_tag_editor_page(&self, _cx: &mut Context<Self>) -> AnyElement {
        self.local_music
            .editor
            .clone()
            .map(|e| e.into_any_element())
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl TagEditor {
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.loaded {
            return;
        }
        let year_text = self.inputs[3].read(cx).value();
        let year = year_text.trim();
        let year = if year.is_empty() {
            Some(0)
        } else {
            year.parse::<u32>().ok().filter(|y| *y <= 2099)
        };
        let Some(year) = year else {
            self.error = Some("年份须为 0–2099 的整数".into());
            cx.notify();
            return;
        };
        let values = TagValues {
            title: self.inputs[0].read(cx).value().to_string(),
            artist: self.inputs[1].read(cx).value().to_string(),
            album: self.inputs[2].read(cx).value().to_string(),
            year,
        };
        let Some(parent) = self.parent.upgrade() else {
            return;
        };
        if parent.read(cx).local_music.busy {
            return;
        }
        let Some(library) = parent.read(cx).local_music.library.clone() else {
            return;
        };
        let Some(store) = parent.read(cx).store.clone() else {
            return;
        };
        let track = self.track.clone();
        let english = parent.read(cx).english;
        self.busy = true;
        self.error = None;
        parent.update(cx, |this, cx| {
            this.local_music.busy = true;
            cx.notify();
        });
        let task = cx
            .background_executor()
            .spawn(async move { library.edit_tags(&track, &values, &store) });
        let parent = self.parent.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let mut updated = None;
            let mut error = None;
            let _ = parent.update_in(cx, |parent, window, cx| {
                parent.local_music.busy = false;
                match result {
                    Ok(result) => {
                        if !parent.session_restored
                            && parent
                                .player
                                .song()
                                .is_some_and(|s| s.id == result.track.song.id)
                            && matches!(
                                parent.player.status,
                                Status::Playing | Status::Paused | Status::Loading
                            )
                        {
                            // Reopen the replaced file with its new byte length before any seek.
                            parent.player.reload_local(result.track.path.clone());
                        }
                        updated = Some(result.track);
                        parent.local_music.set_library(result.library);
                        parent.local_library_changed(cx);
                        if let Some(message) = result.index_error {
                            error = Some(message.clone());
                            window.push_notification(Notification::error(message), cx);
                        } else {
                            window.push_notification(
                                Notification::success(i18n::text(
                                    english,
                                    "music.tagEditor.saveSuccess",
                                )),
                                cx,
                            );
                        }
                    }
                    Err(message) => {
                        error = Some(message.clone());
                        window.push_notification(
                            Notification::error(format!(
                                "{}: {message}",
                                i18n::text(english, "music.tagEditor.saveFailed")
                            )),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                this.error = error;
                if let Some(track) = updated {
                    this.track = track;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for TagEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parent) = self.parent.upgrade() else {
            return div().into_any_element();
        };
        let parent_view = parent.read(cx);
        let colors = parent_view.ui_colors();
        let english = parent_view.english;
        let narrow = window.viewport_size().width <= px(768.);
        let parent = self.parent.clone();
        let mut header = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .when(narrow, |h| h.flex_col().items_start())
            .child(
                Button::new("local-tags-back")
                    .label(i18n::text(english, "common.back"))
                    .ghost()
                    .disabled(self.busy)
                    .when(narrow, |b| b.min_h(px(44.)).rounded(px(12.)))
                    .on_click(move |_, window, cx| {
                        let _ =
                            parent.update(cx, |this, cx| this.navigate_history(false, window, cx));
                    }),
            )
            .child(
                div()
                    .text_size(px(if narrow {
                        (f32::from(window.viewport_size().width) * 0.09).clamp(32., 41.6)
                    } else {
                        20.
                    }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(i18n::text(english, "music.tagEditor.title")),
            );
        header = header.flex_shrink_0();
        let mut form = div()
            .w_full()
            .max_w(px(if narrow { 768. } else { 600. }))
            .flex()
            .flex_col()
            .gap(px(14.));
        for (i, key) in ["songName", "artist", "album", "year"].iter().enumerate() {
            form = form.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(if narrow { 14. } else { 13. }))
                            .text_color(color(colors, "--td-text-color-secondary"))
                            .child(i18n::text(english, &format!("music.tagEditor.{key}"))),
                    )
                    .child(
                        Input::new(&self.inputs[i])
                            .disabled(self.busy || !self.loaded)
                            .when(narrow, |input| input.min_h(px(44.))),
                    ),
            );
        }
        form = form.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(color(colors, "--td-text-color-secondary"))
                        .child(i18n::text(english, "music.tagEditor.filePath")),
                )
                .child(
                    div()
                        .p(px(if narrow { 12. } else { 8. }))
                        .rounded(px(6.))
                        .bg(color(colors, "--td-bg-color-page"))
                        .text_size(px(12.))
                        .text_color(color(colors, "--td-text-color-placeholder"))
                        .child(self.track.path.to_string_lossy().into_owned()),
                ),
        );
        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_size(px(13.))
                    .text_color(color(colors, "--td-error-color"))
                    .child(error.clone()),
            );
        }
        form = form.child(
            Button::new("local-tags-save")
                .label(i18n::text(
                    english,
                    if self.busy {
                        "common.loading"
                    } else {
                        "music.tagEditor.saveTag"
                    },
                ))
                .primary()
                .disabled(self.busy || !self.loaded)
                .mt(px(10.))
                .when(narrow, |b| b.w_full().min_h(px(44.)).rounded(px(12.)))
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
        );
        div()
            .id("local-tag-editor-page")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(if narrow {
                (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.)
            } else {
                20.
            }))
            .pt(px(if narrow {
                (f32::from(window.viewport_size().width) * 0.035).clamp(14., 22.)
            } else {
                20.
            }))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .text_color(color(colors, "--td-text-color-primary"))
            .child(header)
            .child(form)
            .into_any_element()
    }
}
