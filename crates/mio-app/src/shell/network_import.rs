//! "Import from network" of the playlist import dialog (`songlist.vue`
//! `handleNetworkPlaylistImport`): the user picks a platform and pastes a playlist link or ID, the
//! whole playlist is read through the built-in sources and saved as a new local playlist. No
//! plugin is involved.
use super::*;
use crate::library;
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant, spinner::Spinner};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

/// Platform, dot color and name key, in the original's order.
const PLATFORMS: [(&str, u32, &str); 6] = [
    ("wy", 0xe60026, "music.songlist.netease"),
    ("tx", 0x31c27a, "music.songlist.qq"),
    ("kw", 0xf5a623, "music.songlist.kuwo"),
    ("bd", 0x5b9df5, "music.songlist.bodo"),
    ("kg", 0x2ca2f9, "music.songlist.kugou"),
    ("mg", 0xff6c37, "music.songlist.migu"),
];

/// Pages read at most from one playlist, so a source that never reports its end cannot loop.
const MAX_PAGES: usize = 400;

/// The playlist ID in a pasted link, or the input itself when it is only digits
/// (`resolvePlaylistId`). Kugou takes the input as typed.
fn resolve_playlist_id(input: &str, platform: &str) -> Option<String> {
    let numeric = !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit());
    let pattern = match platform {
        "wy" => r"(?i)(?:music\.163\.com/.*[?&]id=|playlist\?id=|playlist/|id=)(\d+)",
        "tx" => {
            r"(?i)(?:y\.qq\.com/n/ryqq/playlist/|music\.qq\.com/.*[?&]id=|playlist[?&]id=|[?&]id=)(\d+)"
        }
        "kw" => r"(?i)(?:kuwo\.cn/playlist_detail/|kuwo\.cn/.*[?&]pid=|[?&](?:pid|id)=)(\d+)",
        "kg" => return Some(input.to_owned()),
        "mg" => r"(?i)[?&]id=(\d+)",
        "bd" => r"(?i)[?&]playlistId=(\d+)",
        _ => return numeric.then(|| input.to_owned()),
    };
    regex::Regex::new(pattern)
        .ok()
        .and_then(|re| re.captures(input).map(|c| c[1].to_owned()))
        .or_else(|| numeric.then(|| input.to_owned()))
}

enum Outcome {
    Success { count: usize, name: String },
    Warning(String),
    Error(String),
}

struct NetworkImport {
    parent: WeakEntity<Mio>,
    path: PathBuf,
    platform: usize,
    input: Entity<InputState>,
    busy: bool,
    outcome: Option<Outcome>,
    cancelled: Arc<AtomicBool>,
    english: bool,
    colors: crate::skin::Colors,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    pub(super) fn open_network_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let english = self.english;
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let editor = cx.new(|cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx).placeholder(i18n::text(
                    english,
                    "music.songlist.networkImportPlaceholder",
                ))
            });
            let events = cx.subscribe_in(
                &input,
                window,
                |this: &mut NetworkImport, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
                    }
                    cx.notify();
                },
            );
            NetworkImport {
                parent,
                path,
                platform: 0,
                input,
                busy: false,
                outcome: None,
                cancelled: Arc::new(AtomicBool::new(false)),
                english,
                colors,
                _subscriptions: vec![events],
            }
        });
        window.open_dialog(cx, move |dialog, window, _cx| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            let cancel = editor.clone();
            dialog
                .w(px((width - if narrow { 2. * gutter } else { 32. })
                    .min(if narrow { 440. } else { 500. })))
                .margin_top((window.viewport_size().height - px(330.)).max(px(24.)) / 2.)
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
                // Closing while the playlist is being read is ignored, as in the original.
                .on_cancel(move |_, _, cx| !cancel.read(cx).busy)
                .child(editor.clone())
        });
    }
}

impl NetworkImport {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let input = self.input.read(cx).value().trim().to_owned();
        if input.is_empty() {
            self.outcome = Some(Outcome::Warning(self.t("music.songlist.invalidLink")));
            cx.notify();
            return;
        }
        let (source, _, name_key) = PLATFORMS[self.platform];
        let platform_name = self.t(name_key);
        let Some(id) = resolve_playlist_id(&input, source) else {
            self.outcome = Some(Outcome::Error(
                self.t("music.songlist.unrecognizedLink")
                    .replace("{platform}", &platform_name),
            ));
            cx.notify();
            return;
        };
        self.busy = true;
        self.outcome = None;
        self.cancelled.store(false, Ordering::Relaxed);
        cx.notify();
        let path = self.path.clone();
        let cancelled = self.cancelled.clone();
        let name = self
            .t("music.songlist.importedFrom")
            .replace("{platform}", &platform_name);
        let description_template = self.t("music.songlist.fromNetworkImport");
        let failed_template = self
            .t("music.songlist.fetchDetailFailed")
            .replace("{platform}", &platform_name);
        let empty_message = self.t("music.songlist.emptyPlaylist");
        let save_failed = self
            .t("music.songlist.importFailed")
            .replace("{error}", &self.t("common.failed"));
        let platform_name_for_description = platform_name.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    read_playlist(source, &id, &cancelled)
                        .map_err(|error| format!("{failed_template}：{error}"))
                        .and_then(|(songs, cover)| {
                            if songs.is_empty() {
                                return Ok(Err(empty_message));
                            }
                            let description = description_template
                                .replace("{platform}", &platform_name_for_description)
                                .replace("{count}", &songs.len().to_string());
                            library::import_network_playlist(
                                &path,
                                &name,
                                &description,
                                source,
                                (!cover.is_empty()).then_some(cover.as_str()),
                                &songs,
                            )
                            .map(|(playlist, favorites, count)| Ok((playlist, favorites, count)))
                            .map_err(|_| save_failed)
                        })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Err(message) => this.outcome = Some(Outcome::Error(message)),
                    Ok(Err(message)) => this.outcome = Some(Outcome::Warning(message)),
                    Ok(Ok((playlist, favorites, count))) => {
                        let _ = this.parent.update(cx, |parent, cx| {
                            parent.favorites.snapshot = Some(favorites);
                            parent.load_playlists(cx);
                            cx.notify();
                        });
                        this.outcome = Some(Outcome::Success {
                            count,
                            name: playlist.name.clone(),
                        });
                        // The original closes the dialog 1.5 s after the success message.
                        cx.spawn_in(window, async move |_, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(1500))
                                .await;
                            let _ = cx.update(|window, cx| window.close_dialog(cx));
                        })
                        .detach();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// Every song of the playlist, page after page (`total` songs or until a page is empty), with
/// the covers a source left out looked up, as `[MusicItem JSON]` plus the playlist's cover.
fn read_playlist(
    source: &str,
    id: &str,
    cancelled: &AtomicBool,
) -> Result<(Vec<serde_json::Value>, String), String> {
    let mut items = Vec::new();
    let mut cover = String::new();
    let mut total = usize::MAX;
    let mut page = 1;
    while items.len() < total && page <= MAX_PAGES {
        if cancelled.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
        let detail = mio_music_sdk::playlist_detail(source, id, page)?;
        if !detail.info.img.is_empty() {
            cover = detail.info.img.clone();
        }
        let received = detail.list.len();
        total = usize::try_from(detail.total)
            .ok()
            .filter(|total| *total > 0)
            .unwrap_or(received);
        items.extend(detail.list);
        page += 1;
        if received == 0 {
            break;
        }
    }
    mio_music_sdk::fill_missing_pics(&mut items, cancelled);
    let songs = items
        .iter()
        .map(|item| serde_json::to_value(item).map_err(|_| "歌曲数据无效".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((songs, cover))
}

impl Render for NetworkImport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let brand = color(self.colors, "--td-brand-color");
        let save = ButtonCustomVariant::new(cx)
            .color(brand)
            .foreground(white())
            .hover(color(self.colors, "--td-brand-color-hover"))
            .active(color(self.colors, "--td-brand-color-active"));
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let platform_name = self.t(PLATFORMS[self.platform].2);

        let mut grid = div().flex().flex_wrap().gap(px(6.)).mb(px(22.));
        for (index, (_, dot, name_key)) in PLATFORMS.iter().enumerate() {
            let active = index == self.platform;
            grid = grid.child(
                gpui_kit::base::Button::new(("network-import-platform", index))
                    .accessibility_label(self.t(name_key))
                    .disabled(self.busy)
                    .px(px(12.))
                    .py(px(if narrow { 11. } else { 8. }))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if active {
                        brand.opacity(0.35)
                    } else {
                        primary.opacity(0.08)
                    })
                    .bg(if active {
                        brand.opacity(0.14)
                    } else {
                        primary.opacity(0.05)
                    })
                    .hover(|card| {
                        if active {
                            card
                        } else {
                            card.bg(primary.opacity(0.1))
                                .border_color(primary.opacity(0.14))
                        }
                    })
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(8.)).rounded_full().bg(rgb(*dot)))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(primary)
                            .child(self.t(name_key)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.platform = index;
                        cx.notify();
                    })),
            );
        }

        let field = Input::new(&self.input)
            .appearance(false)
            .disabled(self.busy)
            .min_h(px(if narrow { 44. } else { 40. }))
            .max_h(px(if narrow { 44. } else { 40. }))
            .border_1()
            .rounded(px(11.))
            .border_color(if focused { brand } else { primary.opacity(0.1) })
            .bg(
                color(self.colors, "--td-bg-color-component").opacity(if focused {
                    0.65
                } else {
                    0.5
                }),
            )
            .text_color(primary)
            .text_size(px(if narrow { 16. } else { 14. }))
            .px(px(14.))
            .flex_1()
            .min_w_0()
            .when(focused, |input| {
                input.shadow(vec![BoxShadow {
                    color: brand.opacity(0.12),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            });

        let button = Button::new("start-network-import")
            .custom(save)
            .h(px(if narrow { 44. } else { 40. }))
            .rounded(px(11.))
            .disabled(self.busy)
            .when(self.busy, |button| button.child(Spinner::new()))
            .when(!self.busy, |button| {
                button.icon(icon("Download").size(px(14.)))
            })
            .label(self.t(if self.busy {
                "music.songlist.fetchingInfo"
            } else {
                "music.songlist.startImport"
            }))
            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));

        let outcome = self.outcome.as_ref().map(|outcome| {
            let (tone, glyph, text) = match outcome {
                Outcome::Success { count, name } => (
                    color(self.colors, "--td-success-color"),
                    "Check",
                    self.t("music.songlist.networkImportSuccess")
                        .replace("{count}", &count.to_string())
                        .replace("{name}", name),
                ),
                Outcome::Warning(message) => (
                    color(self.colors, "--td-warning-color"),
                    "Info",
                    message.clone(),
                ),
                Outcome::Error(message) => (
                    color(self.colors, "--td-error-color"),
                    "Info",
                    message.clone(),
                ),
            };
            div()
                .mt(px(14.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(10.))
                .bg(tone.opacity(0.1))
                .border_1()
                .border_color(tone.opacity(0.25))
                .text_color(tone)
                .text_size(px(13.))
                .flex()
                .items_start()
                .gap(px(8.))
                .child(icon(glyph).size(px(16.)).flex_shrink_0())
                .child(div().flex_1().min_w_0().child(text))
        });

        div()
            .p(px(if narrow { 16. } else { 28. }))
            .when(narrow, |root| root.pt(px(12.)))
            .flex()
            .flex_col()
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
                    .mb(px(if narrow { 16. } else { 20. }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                gpui_kit::base::Button::new("network-import-back")
                                    .accessibility_label(self.t("common.back"))
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
                                        button.bg(primary.opacity(0.1)).text_color(primary)
                                    })
                                    .child(icon("NavigateBefore").size(px(16.)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let parent = this.parent.clone();
                                        window.close_dialog(cx);
                                        let _ = parent.update(cx, |parent, cx| {
                                            parent.open_playlist_import(window, cx)
                                        });
                                    })),
                            )
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
                                    .child(icon("Earth").size(px(22.)).text_color(brand)),
                            )
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.t("music.songlist.importNetworkPlaylist")),
                            ),
                    )
                    .child(
                        gpui_kit::base::Button::new("close-network-import")
                            .accessibility_label(self.t("common.titleBar.close"))
                            .disabled(self.busy)
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
                                    .text_color(color(self.colors, "--td-error-color"))
                            })
                            .child(icon("WindowClose").size(px(13.)))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .child(grid)
            .child(
                div()
                    .mb(px(10.))
                    .text_size(px(13.))
                    .line_height(relative(1.5))
                    .text_color(secondary)
                    .child(
                        self.t("music.songlist.networkImportGuide")
                            .replace("{platform}", &platform_name),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .when(narrow, |row| row.flex_col())
                    .child(field)
                    .child(button),
            )
            .children(outcome)
    }
}
