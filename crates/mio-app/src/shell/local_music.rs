//! `views/music/local.vue`: directory management, scan, filter and virtual local song rows.
use super::*;
use crate::local_music::{Library, Track};
use gpui_kit::component::{
    WindowExt,
    menu::{DropdownMenu, PopupMenuItem},
    notification::Notification,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub(super) struct LocalMusic {
    pub library: Option<Library>,
    selected: HashSet<String>,
    multi: bool,
    pub(super) busy: bool,
    error: Option<String>,
    filter: Entity<InputState>,
    scroll: UniformListScrollHandle,
    mobile_search: bool,
    pub(super) editor: Option<Entity<super::local_tag_editor::TagEditor>>,
    covers: HashMap<String, Option<Arc<RenderImage>>>,
    cover_queue: VecDeque<Track>,
    cover_loading: Option<String>,
    cover_revision: u64,
    pub(super) retired_covers: Vec<Arc<RenderImage>>,
    /// Bumped with every replaced library; keys the filtered-track cache.
    library_revision: u64,
    visible: std::cell::RefCell<Option<(u64, String, Arc<Vec<Track>>)>>,
}
impl LocalMusic {
    pub(super) fn set_library(&mut self, library: Library) {
        self.library = Some(library);
        self.library_revision = self.library_revision.wrapping_add(1);
    }
    pub fn new(english: bool, window: &mut Window, cx: &mut Context<Mio>) -> (Self, Subscription) {
        let filter = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(i18n::text(english, "music.local.searchPlaceholder"))
        });
        let subscription = cx.subscribe_in(&filter, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.local_music
                    .scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(point(px(0.), px(0.)));
                cx.notify();
            }
        });
        (
            Self {
                library: None,
                selected: HashSet::new(),
                multi: false,
                busy: false,
                error: None,
                filter,
                scroll: UniformListScrollHandle::new(),
                mobile_search: false,
                editor: None,
                covers: HashMap::new(),
                cover_queue: VecDeque::new(),
                cover_loading: None,
                cover_revision: 0,
                retired_covers: vec![],
                library_revision: 0,
                visible: Default::default(),
            },
            subscription,
        )
    }
}

struct DirectoryDialog {
    parent: WeakEntity<Mio>,
    directories: Vec<PathBuf>,
    colors: crate::skin::Colors,
    english: bool,
}

impl Mio {
    pub(super) fn load_local_music(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            self.local_music.error = Some("无法打开本地音乐数据目录".into());
            return;
        };
        self.local_music.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { Library::load(&store) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.local_music.busy = false;
                match result {
                    Ok(library) => {
                        this.local_music.set_library(library);
                        this.local_music.error = None;
                        this.restore_local_music_session(cx);
                        this.local_library_changed(cx);
                    }
                    Err(error) => this.local_music.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn restore_local_music_session(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.pending_local_session.take() else {
            return;
        };
        let qualities = session.online_queue_qualities();
        if let Some((queue, index, song, position, identity)) = session.restore_local() {
            self.player.queue = queue;
            self.player.queue_qualities = qualities;
            self.player.current = Some(index);
            self.player.duration = song.duration as f64;
            self.player.active_song = Some(song);
            self.player.position = position;
            self.player.status = Status::Paused;
            self.online_queue_identity = identity;
            self.session_restored = true;
            self.online_progress_pending = true;
            cx.notify();
        }
    }
    fn scan_local_music(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.local_music.busy {
            return;
        }
        let Some(library) = self.local_music.library.clone() else {
            self.load_local_music(cx);
            return;
        };
        if library.directories.is_empty() {
            window.push_notification(
                Notification::warning(self.t("music.local.selectScanDir")),
                cx,
            );
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let skip_hidden = self.app_settings.skip_hidden_files;
        self.local_music.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { library.scan(skip_hidden, &store) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.local_music.busy = false;
                match result {
                    Ok((library, result)) => {
                        this.local_music.set_library(library);
                        this.local_library_changed(cx);
                        this.local_music.error = None;
                        let mut text = this
                            .t("music.local.scanComplete")
                            .replace("{scanned}", &result.scanned.to_string())
                            .replace("{added}", &result.added.to_string());
                        if result.updated > 0 {
                            text.push_str(&format!(
                                ", {}",
                                this.t("music.local.scanUpdated")
                                    .replace("{updated}", &result.updated.to_string())
                            ));
                        }
                        if result.errors > 0 {
                            text.push_str(&format!(
                                ", {}",
                                this.t("music.local.scanErrors")
                                    .replace("{errors}", &result.errors.to_string())
                            ));
                        }
                        window.push_notification(
                            if result.errors > 0 {
                                Notification::warning(text)
                            } else {
                                Notification::success(text)
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.local_music.error = Some(error.clone());
                        window.push_notification(Notification::error(error), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn open_local_directories(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.local_music.busy {
            return;
        }
        let Some(library) = &self.local_music.library else {
            return;
        };
        let colors = self.ui_colors();
        let english = self.english;
        let parent = cx.entity().downgrade();
        let directories = library.directories.clone();
        let dialog = cx.new(|_| DirectoryDialog {
            parent,
            directories,
            colors,
            english,
        });
        let title = self.t("music.local.selectDirDialog");
        window.open_dialog(cx, move |view, window, _| {
            view.title(title.clone())
                .w(px((f32::from(window.viewport_size().width) - 32.).min(500.)))
                .child(dialog.clone())
        });
    }
    pub(super) fn visible_local_tracks(&self, cx: &App) -> Arc<Vec<Track>> {
        let query = self
            .local_music
            .filter
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        // Rendering runs on every player tick; filter and clone only when the inputs change.
        let revision = self.local_music.library_revision;
        let mut cache = self.local_music.visible.borrow_mut();
        if let Some((cached, cached_query, tracks)) = cache.as_ref()
            && *cached == revision
            && *cached_query == query
        {
            return tracks.clone();
        }
        let tracks: Arc<Vec<Track>> = Arc::new(
            self.local_music
                .library
                .as_ref()
                .map(|l| {
                    l.tracks
                        .iter()
                        .filter(|t| {
                            query.is_empty()
                                || [&t.song.title, &t.song.artist, &t.song.album]
                                    .iter()
                                    .any(|s| s.to_lowercase().contains(&query))
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
        );
        *cache = Some((revision, query, tracks.clone()));
        tracks
    }
    fn play_all_local(&mut self, cx: &mut Context<Self>) {
        let tracks = self.visible_local_tracks(cx);
        if tracks.is_empty() {
            return;
        }
        self.player
            .replace_queue(tracks.iter().map(|t| t.song.clone()).collect());
        self.online_queue_identity = None;
        self.play_index(0, cx);
    }
    pub(super) fn local_music_page(
        &self,
        narrow: bool,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = self.ui_colors();
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let tracks = self.visible_local_tracks(cx);
        let total = self
            .local_music
            .library
            .as_ref()
            .map(|l| l.tracks.len())
            .unwrap_or(0);
        let directories_button = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            let button = Button::new("local-directories").label(this.t("music.local.selectDir"));
            let button = if primary {
                button.primary().rounded(px(24.))
            } else {
                button.outline()
            };
            button
                .disabled(this.local_music.busy || this.local_music.library.is_none())
                .on_click(
                    cx.listener(|this, _, window, cx| this.open_local_directories(window, cx)),
                )
        };
        let mut page = div()
            .id("local-music-page")
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .gap(px(12.))
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                24.
            }))
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                24.
            }))
            .pb(px(if narrow { 0. } else { 24. }))
            .text_color(primary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    // Wide windows drop the heading row: its count and directory
                    // button move into the toolbar below.
                    .when(!narrow, |header| header.hidden())
                    .child(
                        div()
                            .flex()
                            .when(narrow, |heading| heading.flex_col().items_start())
                            .when(!narrow, |heading| heading.flex_wrap().items_baseline())
                            .gap(px(8.))
                            .min_w_0()
                            .child(
                                div()
                                    .when(!narrow, |title| {
                                        title
                                            .border_l(px(8.))
                                            .border_color(color(colors, "--td-brand-color-3"))
                                            .pl(px(12.))
                                    })
                                    .text_size(px(if narrow {
                                        (width * 0.08).clamp(28., 38.4)
                                    } else {
                                        28.
                                    }))
                                    .when(narrow, |title| title.line_height(relative(1.1)))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.t("music.local.title")),
                            )
                            .child(
                                div().text_size(px(12.)).text_color(secondary).child(
                                    self.t("music.local.totalSongs")
                                        .replace("{count}", &total.to_string()),
                                ),
                            ),
                    )
                    .when(narrow, |header| {
                        header.child(directories_button(self, cx, true))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Button::new("local-play-all")
                            .label(self.t("common.playAll"))
                            .primary()
                            .when(narrow, |b| b.flex_1().h(px(44.)).rounded(px(22.)))
                            .disabled(tracks.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.play_all_local(cx))),
                    )
                    .when(!narrow, |row| {
                        row.child(directories_button(self, cx, false))
                    })
                    .child(
                        Button::new("local-scan")
                            .accessibility_label(self.t("common.refresh"))
                            .tooltip(self.t("common.refresh"))
                            .icon(icon("Refresh"))
                            .outline()
                            .when(narrow, |b| b.size(px(44.)).rounded(px(22.)))
                            .disabled(self.local_music.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.scan_local_music(window, cx)
                                }),
                            ),
                    )
                    .child({
                        let entity = cx.entity().downgrade();
                        let play_label = self.t("common.playAll");
                        let append_label = self.t("music.local.addAllToPlaylist");
                        let batch_label = self.t(if self.local_music.multi {
                            "common.cancelBatch"
                        } else {
                            "common.batchSelect"
                        });
                        let clear_label = self.t("common.clearAll");
                        let empty = tracks.is_empty();
                        let clear_disabled = self.local_music.busy || total == 0;
                        Button::new("local-more")
                            .accessibility_label(if self.english {
                                "More actions"
                            } else {
                                "更多操作"
                            })
                            .label("⋯")
                            .outline()
                            .when(narrow, |b| b.size(px(44.)).rounded_full())
                            .dropdown_menu(move |menu, _, _| {
                                let play_entity = entity.clone();
                                let append_entity = entity.clone();
                                let batch_entity = entity.clone();
                                let clear_entity = entity.clone();
                                menu.item(
                                    PopupMenuItem::new(play_label.clone())
                                        .disabled(empty)
                                        .on_click(move |_, _, cx| {
                                            let _ = play_entity
                                                .update(cx, |this, cx| this.play_all_local(cx));
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(append_label.clone())
                                        .disabled(empty)
                                        .on_click(move |_, _, cx| {
                                            let _ = append_entity.update(cx, |this, cx| {
                                                let tracks =
                                                    this.visible_local_tracks(cx).as_ref().clone();
                                                this.append_local_tracks(tracks, cx);
                                            });
                                        }),
                                )
                                .item(PopupMenuItem::new(batch_label.clone()).on_click(
                                    move |_, _, cx| {
                                        let _ = batch_entity.update(cx, |this, cx| {
                                            this.local_music.multi = !this.local_music.multi;
                                            this.local_music.selected.clear();
                                            cx.notify();
                                        });
                                    },
                                ))
                                .item(
                                    PopupMenuItem::new(clear_label.clone())
                                        .disabled(clear_disabled)
                                        .on_click(move |_, window, cx| {
                                            let _ = clear_entity.update(cx, |this, cx| {
                                                this.confirm_clear_local_index(window, cx);
                                            });
                                        }),
                                )
                            })
                    })
                    .when(narrow, |row| {
                        row.child(
                            Button::new("local-search-toggle")
                                .accessibility_label(self.t("common.search"))
                                .icon(icon("Search"))
                                .size(px(44.))
                                .outline()
                                .rounded(px(22.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.local_music.mobile_search =
                                        !this.local_music.mobile_search;
                                    cx.notify();
                                })),
                        )
                    })
                    .when(!narrow, |row| {
                        row.child(div().flex_1())
                            .child(
                                div().text_size(px(12.)).text_color(secondary).child(
                                    self.t("music.local.totalSongs")
                                        .replace("{count}", &total.to_string()),
                                ),
                            )
                            .child(
                                Input::new(&self.local_music.filter)
                                    .w(px(240.))
                                    .cleanable(true),
                            )
                    }),
            );
        if !self.local_music.selected.is_empty() {
            page = page.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div().text_size(px(12.)).child(
                            self.t("music.local.selectedCount")
                                .replace("{count}", &self.local_music.selected.len().to_string()),
                        ),
                    )
                    .child(
                        Button::new("local-play-selected")
                            .label(self.t("music.local.playSelected"))
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let tracks = this.local_action_tracks(cx);
                                if !tracks.is_empty() {
                                    this.player.replace_queue(
                                        tracks.iter().map(|t| t.song.clone()).collect(),
                                    );
                                    this.online_queue_identity = None;
                                    this.play_index(0, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("local-add-playlist")
                            .label(self.t("common.addToPlaylist"))
                            .outline()
                            .disabled(self.favorites.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let songs = this
                                    .local_action_tracks(cx)
                                    .iter()
                                    .map(|t| t.song.clone())
                                    .collect();
                                this.open_add_to_playlist(songs, window, cx);
                            })),
                    )
                    .child(
                        Button::new("local-cancel-selection")
                            .label(self.t("music.local.cancelSelect"))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.local_music.selected.clear();
                                cx.notify();
                            })),
                    ),
            );
        }
        if narrow && self.local_music.mobile_search {
            page = page.child(Input::new(&self.local_music.filter).cleanable(true));
        }
        if let Some(error) = &self.local_music.error {
            page = page.child(
                div()
                    .text_size(px(13.))
                    .text_color(color(colors, "--td-error-color"))
                    .child(error.clone()),
            );
        }
        if self.local_music.busy {
            page = page.child(
                div()
                    .text_color(secondary)
                    .text_size(px(13.))
                    .child(self.t("common.loading")),
            );
        }
        if tracks.is_empty() {
            let searching = !self.local_music.filter.read(cx).value().trim().is_empty();
            return page.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child(self.t(if searching {
                                "music.local.noMatchMusic"
                            } else {
                                "music.local.emptyTitle"
                            })),
                    )
                    .child(
                        div()
                            .max_w(px(480.))
                            .text_size(px(14.))
                            .text_color(secondary)
                            .child(self.t(if searching {
                                "music.local.emptySearchHint"
                            } else {
                                "music.local.noMusicDir"
                            })),
                    )
                    .when(!searching, |empty| {
                        empty
                            .child(
                                Button::new("local-empty-directories")
                                    .label(self.t("music.local.selectDir"))
                                    .primary()
                                    .disabled(
                                        self.local_music.busy || self.local_music.library.is_none(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_local_directories(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(self.t("music.local.emptyScanTip")),
                            )
                    }),
            );
        }
        if !narrow {
            page = page.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .px(px(8.))
                    .gap(px(12.))
                    .text_size(px(12.))
                    .text_color(secondary)
                    .child(
                        Button::new("local-select-all")
                            .label(
                                if tracks
                                    .iter()
                                    .all(|t| self.local_music.selected.contains(&t.song.id))
                                {
                                    "☑"
                                } else {
                                    "☐"
                                },
                            )
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                let ids: HashSet<_> = this
                                    .visible_local_tracks(cx)
                                    .iter()
                                    .map(|t| t.song.id.clone())
                                    .collect();
                                if ids.iter().all(|id| this.local_music.selected.contains(id)) {
                                    this.local_music.selected.clear();
                                } else {
                                    this.local_music.selected = ids;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.t("music.local.colSong")),
                    )
                    .child(
                        div()
                            .w(px(180.))
                            .flex_shrink_0()
                            .child(self.t("music.local.colAlbum")),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .flex_shrink_0()
                            .child(self.t("music.local.colDuration")),
                    )
                    .child(
                        div()
                            .w(px(85.))
                            .flex_shrink_0()
                            .child(self.t("music.local.colSize")),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .flex_shrink_0()
                            .child(self.t("music.local.colActions")),
                    ),
            );
        }
        page.child(
            uniform_list(
                "local-music-rows",
                tracks.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    this.request_local_covers(range.clone().map(|i| tracks[i].clone()), cx);
                    range
                        .map(|i| this.local_track_row(&tracks[i], i, narrow, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.local_music.scroll)
            .size_full(),
        )
    }
    fn local_track_row(
        &self,
        track: &Track,
        index: usize,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = self.ui_colors();
        let secondary = color(colors, "--td-text-color-placeholder");
        let active = self.player.song().is_some_and(|s| s.id == track.song.id);
        let song = track.song.clone();
        let mut row = div()
            .id(("local-track", index))
            .tab_stop(true)
            .cursor_pointer()
            .w_full()
            .h(px(if narrow { 68. } else { 60. }))
            .px(px(8.))
            .rounded(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .min_w_0()
            .hover(|r| r.bg(color(colors, "--td-bg-color-component-hover")))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.local_music.multi {
                    this.toggle_local_selection(&song.id, cx);
                } else {
                    let index = this.enqueue_player_first(song.clone());
                    this.play_index(index, cx);
                }
            }))
            .on_key_down(cx.listener({
                let track = track.clone();
                move |this, e: &KeyDownEvent, window, cx| {
                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                        if this.local_music.multi {
                            this.toggle_local_selection(&track.song.id, cx);
                        } else {
                            let index = this.enqueue_player_first(track.song.clone());
                            this.play_index(index, cx);
                        }
                        cx.stop_propagation();
                    } else if e.keystroke.key == "f10" && e.keystroke.modifiers.shift {
                        this.local_track_context_actions(
                            track.clone(),
                            window.mouse_position(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let track = track.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        this.local_track_context_actions(track.clone(), event.position, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .when(!narrow || self.local_music.multi, |row| {
                row.child(
                    Button::new(("local-select", index))
                        .label(if self.local_music.selected.contains(&track.song.id) {
                            "☑"
                        } else {
                            "☐"
                        })
                        .ghost()
                        .on_click(cx.listener({
                            let id = track.song.id.clone();
                            move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_local_selection(&id, cx);
                            }
                        })),
                )
            })
            .child(self.favorite_button(("local-favorite", index), Some(&track.song), true, cx))
            .child(
                img(self
                    .local_music
                    .covers
                    .get(&track.cover_key())
                    .and_then(|image| image.clone())
                    .map(ImageSource::from)
                    .unwrap_or_else(|| ImageSource::from("images/default-cover.png")))
                .size(px(if narrow { 48. } else { 44. }))
                .object_fit(ObjectFit::Cover)
                .rounded(px(if narrow { 10. } else { 8. }))
                .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .text_color(if active {
                                color(colors, "--td-brand-color")
                            } else {
                                color(colors, "--td-text-color-primary")
                            })
                            .child(track.song.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(secondary)
                            .child(if track.song.artist.is_empty() {
                                self.t("common.unknown")
                            } else {
                                track.song.artist.clone()
                            }),
                    ),
            );
        if !narrow {
            row = row.child(
                div()
                    .w(px(180.))
                    .flex_shrink_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(secondary)
                    .child(if track.song.album.is_empty() {
                        self.t("common.unknownAlbum")
                    } else {
                        track.song.album.clone()
                    }),
            );
        }
        let duration = format!(
            "{:02}:{:02}",
            track.song.duration / 60,
            track.song.duration % 60
        );
        row.child(
            div()
                .w(px(if narrow { 48. } else { 60. }))
                .flex_shrink_0()
                .text_size(px(12.))
                .text_color(secondary)
                .child(duration),
        )
        .when(!narrow, |r| {
            r.child(
                div()
                    .w(px(85.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(secondary)
                    .child(format!("{:.2} MB", track.size as f64 / (1024. * 1024.))),
            )
            .child(
                div().w(px(60.)).flex_shrink_0().child(
                    Button::new(("local-edit-tags", index))
                        .label(self.t("music.local.edit"))
                        .ghost()
                        .disabled(self.local_music.busy)
                        .on_click(cx.listener({
                            let track = track.clone();
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_local_tag_editor(track.clone(), window, cx);
                            }
                        })),
                ),
            )
        })
    }
}

impl Mio {
    pub(super) fn local_library_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(library) = &self.local_music.library {
            self.local_music
                .selected
                .retain(|id| library.tracks.iter().any(|t| &t.song.id == id));
        }
        self.local_music.cover_revision += 1;
        self.local_music.cover_queue.clear();
        self.local_music.retired_covers.extend(
            self.local_music
                .covers
                .drain()
                .filter_map(|(_, cover)| cover),
        );
        if let Some(library) = &self.local_music.library {
            for song in &mut self.player.queue {
                if let Some(track) = library.tracks.iter().find(|t| t.song.id == song.id) {
                    *song = track.song.clone();
                }
            }
            if let Some(song) = &mut self.player.active_song
                && let Some(track) = library.tracks.iter().find(|t| t.song.id == song.id)
            {
                *song = track.song.clone();
            }
        }
        if self.player.song().is_some_and(crate::local_music::is_local) {
            self.cover = None;
            self.cover_background = None;
            self.cover_pixels = None;
            self.cover_palette = None;
            #[cfg(target_os = "macos")]
            self.background.invalidate_track();
            self.apply_theme(cx);
            self.load_local_player_cover(cx);
            self.save_session();
        }
        cx.notify();
    }
    pub(super) fn local_player_cover_key(&self) -> Option<String> {
        let song = self
            .player
            .song()
            .filter(|s| crate::local_music::is_local(s))?;
        self.local_music
            .library
            .as_ref()?
            .tracks
            .iter()
            .find(|t| t.song.id == song.id)
            .map(Track::cover_key)
    }
    pub(super) fn load_local_player_cover(&self, cx: &mut Context<Self>) {
        let Some(song) = self
            .player
            .song()
            .filter(|s| crate::local_music::is_local(s))
        else {
            return;
        };
        let Some(track) = self
            .local_music
            .library
            .as_ref()
            .and_then(|l| l.tracks.iter().find(|t| t.song.id == song.id))
            .cloned()
        else {
            return;
        };
        let key = track.cover_key();
        let generation = self.player.generation;
        let task = cx
            .background_executor()
            .spawn(async move { crate::local_music::embedded_cover(&track.path) });
        cx.spawn(async move |this, cx| {
            let bytes = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.player.generation == generation
                    && this.local_player_cover_key().as_ref() == Some(&key)
                    && let Some(bytes) = bytes
                {
                    this.decode_cover(bytes, generation, cx);
                }
            });
        })
        .detach();
    }
    fn request_local_covers(
        &mut self,
        tracks: impl Iterator<Item = Track>,
        cx: &mut Context<Self>,
    ) {
        for track in tracks {
            if self.local_music.cover_queue.len() >= 64 {
                break;
            }
            let key = track.cover_key();
            if !self.local_music.covers.contains_key(&key)
                && self.local_music.cover_loading.as_ref() != Some(&key)
                && !self
                    .local_music
                    .cover_queue
                    .iter()
                    .any(|t| t.cover_key() == key)
            {
                self.local_music.cover_queue.push_back(track);
            }
        }
        self.pump_local_covers(cx);
    }
    fn pump_local_covers(&mut self, cx: &mut Context<Self>) {
        if self.local_music.cover_loading.is_some() {
            return;
        }
        let Some(track) = self.local_music.cover_queue.pop_front() else {
            return;
        };
        let key = track.cover_key();
        let revision = self.local_music.cover_revision;
        self.local_music.cover_loading = Some(key.clone());
        let task = cx.background_executor().spawn(async move {
            let bytes = crate::local_music::embedded_cover(&track.path)?;
            let mut pixels = cover_palette::decode_image(bytes)?
                .thumbnail(256, 256)
                .to_rgba8();
            for pixel in pixels.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
        });
        cx.spawn(async move |this, cx| {
            let cover = task.await;
            let _ = this.update(cx, |this, cx| {
                this.local_music.cover_loading = None;
                if this.local_music.cover_revision == revision {
                    if this.local_music.covers.len() >= 64
                        && let Some(old) = this.local_music.covers.keys().next().cloned()
                        && let Some(Some(image)) = this.local_music.covers.remove(&old)
                    {
                        this.local_music.retired_covers.push(image);
                    }
                    this.local_music.covers.insert(key, cover);
                }
                this.pump_local_covers(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

impl DirectoryDialog {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }
    fn pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some(self.t("common.addFolder").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = selection.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    for path in paths {
                        let Ok(path) = path.canonicalize() else {
                            continue;
                        };
                        if path.is_dir() && !this.directories.contains(&path) {
                            this.directories.push(path);
                        }
                    }
                    cx.notify();
                }
                Ok(Ok(None)) => {}
                _ => window.push_notification(
                    Notification::error(this.t("settings.storage.selectDirFailed")),
                    cx,
                ),
            });
        })
        .detach();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(parent) = self.parent.upgrade() else {
            return;
        };
        let Some(mut library) = parent.read(cx).local_music.library.clone() else {
            return;
        };
        if parent.read(cx).local_music.busy {
            return;
        }
        let Some(store) = parent.read(cx).store.clone() else {
            return;
        };
        library.directories = self.directories.clone();
        parent.update(cx, |this, cx| {
            this.local_music.busy = true;
            cx.notify();
        });
        window.close_dialog(cx);
        let task = cx
            .background_executor()
            .spawn(async move { library.save(&store).map(|_| library) });
        let parent = parent.downgrade();
        cx.spawn_in(window, async move |_, cx| {
            let result = task.await;
            let _ = parent.update_in(cx, |this, window, cx| {
                this.local_music.busy = false;
                match result {
                    Ok(library) => {
                        this.local_music.set_library(library);
                        window.push_notification(
                            Notification::success(this.t("music.local.dirSaved")),
                            cx,
                        );
                    }
                    Err(error) => window.push_notification(Notification::error(error), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
impl Render for DirectoryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .text_size(px(14.))
            .child(
                div()
                    .text_color(color(self.colors, "--td-text-color-secondary"))
                    .child(self.t("music.local.selectDirTip")),
            );
        if self.directories.is_empty() {
            body = body
                .child(div().child(self.t("music.local.dirEmptyTitle")))
                .child(
                    div()
                        .text_size(px(12.))
                        .child(self.t("music.local.dirEmptyHint")),
                );
        }
        let mut list = div()
            .id("local-directory-list")
            .max_h(px(300.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(8.));
        for (index, path) in self.directories.iter().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .child(path.to_string_lossy().into_owned()),
                    )
                    .child(
                        Button::new(("local-dir-remove", index))
                            .label(self.t("common.delete"))
                            .outline()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.directories.remove(index);
                                cx.notify();
                            })),
                    ),
            );
        }
        body.child(list)
            .child(
                Button::new("local-dir-add")
                    .label(self.t("common.addFolder"))
                    .outline()
                    .on_click(cx.listener(|this, _, window, cx| this.pick(window, cx))),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("local-dir-cancel")
                            .label(self.t("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("local-dir-save")
                            .label(self.t("common.confirm"))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

impl Mio {
    fn toggle_local_selection(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.local_music.selected.remove(id) {
            self.local_music.selected.insert(id.to_owned());
        }
        cx.notify();
    }
    fn local_action_tracks(&self, cx: &App) -> Vec<Track> {
        self.visible_local_tracks(cx)
            .iter()
            .filter(|t| self.local_music.selected.contains(&t.song.id))
            .cloned()
            .collect()
    }
    pub(super) fn append_local_tracks(&mut self, tracks: Vec<Track>, cx: &mut Context<Self>) {
        let mut known: HashSet<_> = self.player.queue.iter().map(|s| s.id.clone()).collect();
        let songs = tracks
            .into_iter()
            .filter(|t| {
                self.local_music
                    .library
                    .as_ref()
                    .and_then(|l| l.path(&t.song.id))
                    .is_some()
            })
            .filter_map(|t| known.insert(t.song.id.clone()).then_some(t.song))
            .collect();
        self.player.append_to_queue(songs);
        self.save_session();
        cx.notify();
    }
    fn confirm_clear_local_index(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.local_music.busy {
            return;
        }
        let parent = cx.weak_entity();
        let title = self.t("common.clearAll");
        let hint = if self.english {
            "Clear scanned entries? Music files, folders and saved playlists stay on disk. Rescan to restore the entries."
        } else {
            "清空扫描索引？音乐文件、目录配置与歌单会保留，重新扫描可恢复条目。"
        };
        window.open_dialog(cx, move |dialog, _, _| {
            let parent = parent.clone();
            dialog
                .title(title.clone())
                .child(hint)
                .button_props(
                    gpui_kit::component::dialog::DialogButtonProps::default().show_cancel(true),
                )
                .on_ok(move |_, window, cx| {
                    let _ = parent.update(cx, |this, cx| this.clear_local_index(window, cx));
                    true
                })
        });
    }
    fn clear_local_index(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.local_music.busy {
            return;
        }
        let (Some(mut library), Some(store)) =
            (self.local_music.library.clone(), self.store.clone())
        else {
            return;
        };
        library.tracks.clear();
        self.local_music.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { library.save(&store).map(|_| library) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.local_music.busy = false;
                match result {
                    Ok(library) => {
                        this.local_music.set_library(library);
                        this.local_library_changed(cx);
                        window.push_notification(
                            Notification::success(this.t("music.local.indexCleared")),
                            cx,
                        );
                    }
                    Err(error) => window.push_notification(Notification::error(error), cx),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
