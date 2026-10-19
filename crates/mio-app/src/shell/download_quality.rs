//! Quality choice before a download, like the original's `createQualityDialog`: a scrollable
//! list of the qualities a song offers, with sizes, the last choice for that source highlighted.
//! Batch actions open a separate choice for each song, as the original does.
use super::*;
use crate::quality::{self, Choice};
use gpui_kit::component::{WindowExt, notification::Notification};
use std::{cell::RefCell, rc::Rc};

type QualityDialogs = Rc<RefCell<Vec<(Song, String, Vec<Choice>, String)>>>;

impl Mio {
    /// Entry point of every download button: asks for a quality first.
    pub(super) fn download_songs(
        &mut self,
        songs: Vec<Song>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let songs: Vec<Song> = songs.into_iter().filter(crate::online::is_online).collect();
        if songs.is_empty() {
            return;
        }
        let Some(online) = self.online.clone() else {
            window.push_notification(Notification::error(self.t("download.getUrlFailed")), cx);
            return;
        };
        let mut dialogs = Vec::new();
        for song in songs {
            let Some(item) = online
                .item_json(&song.id)
                .and_then(|json| serde_json::from_value::<mio_music_sdk::MusicItem>(json).ok())
            else {
                window.push_notification(Notification::error(self.t("download.unknownError")), cx);
                return;
            };
            let options = quality::choices(&item, &online.source_qualities(&item.source));
            if options.is_empty() {
                window.push_notification(Notification::error(self.t("download.getUrlFailed")), cx);
                return;
            }
            let remembered = self
                .song_view
                .downloads
                .preferred
                .get(&item.source)
                .cloned()
                .or_else(|| self.selected_music_quality())
                .unwrap_or_default();
            dialogs.push((song, item.source, options, remembered));
        }
        // Original batch handlers call downloadSingleSong for every loaded song;
        // each choice keeps that song's qualities and sizes, and closing skips only it.
        self.next_download_quality(Rc::new(RefCell::new(dialogs)), window, cx);
    }

    fn next_download_quality(
        &self,
        pending: QualityDialogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Expose the last song first, like the original's overlapping dialogs, without
        // GPUI's stacked-layer offset shrinking long batches outside the narrow viewport.
        let next = pending.borrow_mut().pop();
        if let Some((song, source, options, remembered)) = next {
            self.open_download_quality(
                vec![song],
                source,
                options,
                remembered,
                pending,
                window,
                cx,
            );
        }
    }

    fn open_download_quality(
        &self,
        songs: Vec<Song>,
        source: String,
        options: Vec<Choice>,
        remembered: String,
        pending: QualityDialogs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let title = self.t("download.selectQuality");
        let close = self.t("common.titleBar.close");
        let colors = self.ui_colors();
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let brand = color(colors, "--td-brand-color");
        // (quality, display name, size, is the remembered one)
        let rows: Vec<(String, String, Option<String>, bool)> = options
            .into_iter()
            .map(|choice| {
                let name = quality::display_key(&choice.quality)
                    .map(|key| self.t(key))
                    .unwrap_or_else(|| choice.quality.clone());
                let active = choice.quality == remembered;
                (choice.quality, name, choice.size, active)
            })
            .collect();
        let finish = Rc::new(move |window: &mut Window, cx: &mut App| {
            window.close_dialog(cx);
            let _ = view.update(cx, |this, cx| {
                this.next_download_quality(pending.clone(), window, cx);
            });
        });
        let view = cx.entity().downgrade();
        let options = Arc::new(rows);
        let songs = Arc::new(songs);
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let gutter = (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.);
            let view = view.clone();
            let options = options.clone();
            let songs = songs.clone();
            let finish = finish.clone();
            let cancel = finish.clone();
            let close_choice = finish.clone();
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
                .on_cancel(move |_, window, cx| {
                    cancel(window, cx);
                    false
                })
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
                                        .child(icon("Download").size(px(20.))),
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
                                    gpui_kit::base::Button::new("close-download-quality")
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
                                        .on_click(move |_, window, cx| close_choice(window, cx)),
                                ),
                        ),
                )
                .content({
                    let options = options.clone();
                    let songs = songs.clone();
                    let view = view.clone();
                    let source = source.clone();
                    let finish = finish.clone();
                    move |content, _, _| {
                        content.child(
                            div()
                                .id("download-quality-list")
                                .max_h(px(320.))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .children(options.iter().enumerate().map(
                                    |(index, (quality, name, size, active))| {
                                        let quality = quality.clone();
                                        let finish = finish.clone();
                                        let songs = songs.clone();
                                        let view = view.clone();
                                        let source = source.clone();
                                        gpui_kit::base::Button::new(SharedString::from(format!(
                                            "download-quality-{index}"
                                        )))
                                        .accessibility_label(name.clone())
                                        .aria_selected(*active)
                                        .w_full()
                                        .px(px(14.))
                                        .py(px(12.))
                                        .rounded(px(12.))
                                        .border_1()
                                        .border_color(if *active {
                                            brand.opacity(0.3)
                                        } else {
                                            primary.opacity(0.08)
                                        })
                                        .bg(if *active {
                                            brand.opacity(0.14)
                                        } else {
                                            primary.opacity(0.06)
                                        })
                                        .hover(move |button| button.bg(primary.opacity(0.12)))
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(16.))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.))
                                                .child(
                                                    div()
                                                        .text_size(px(14.))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(if *active {
                                                            brand
                                                        } else {
                                                            primary
                                                        })
                                                        .child(name.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.))
                                                        .text_color(secondary)
                                                        .child(quality.clone()),
                                                ),
                                        )
                                        .when_some(size.clone(), |row, size| {
                                            row.child(
                                                div()
                                                    .flex_shrink_0()
                                                    .text_size(px(12.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(secondary)
                                                    .child(size),
                                            )
                                        })
                                        .on_click(
                                            move |_, window, cx| {
                                                let note = view
                                                    .update(cx, |this, cx| {
                                                        this.song_view.downloads.preferred.insert(
                                                            source.clone(),
                                                            quality.clone(),
                                                        );
                                                        this.save_preferences();
                                                        this.enqueue_downloads(
                                                            songs.as_ref().clone(),
                                                            &quality,
                                                            cx,
                                                        )
                                                    })
                                                    .ok();
                                                finish(window, cx);
                                                if let Some(note) = note {
                                                    window.push_notification(note, cx);
                                                }
                                            },
                                        )
                                    },
                                )),
                        )
                    }
                })
                .footer(div())
        });
    }
}
