//! Download entry points and the download manager page. The queue itself is `crate::download`.
use super::*;
use crate::download::{ClearScope, Downloads, Enqueued, Status, TaskView};
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::notification::Notification;

pub(super) struct DownloadUi {
    pub manager: Option<Arc<Downloads>>,
    ticking: bool,
    /// 0 active, 1 completed, 2 failed.
    pub tab: u8,
    /// The header's gear is on and the "同时下载数" row is shown.
    pub settings_open: bool,
    /// Last quality chosen per source, kept in the preferences file.
    pub preferred: std::collections::BTreeMap<String, String>,
    pub tags: crate::tags::TagOptions,
    /// The header's "搜索任务" box; filters the current tab by song or artist.
    pub search: Entity<InputState>,
}

impl DownloadUi {
    pub fn new(search: Entity<InputState>) -> Self {
        Self {
            search,
            manager: None,
            ticking: false,
            tab: 0,
            settings_open: false,
            preferred: Default::default(),
            tags: Default::default(),
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024. && index < units.len() - 1 {
        value /= 1024.;
        index += 1;
    }
    format!("{value:.1} {}", units[index])
}

/// `formatSpeed` of the original.
fn format_speed(speed: f64) -> String {
    if speed <= 0. {
        return "0 B/s".into();
    }
    let units = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = speed;
    let mut index = 0;
    while value >= 1024. && index < units.len() - 1 {
        value /= 1024.;
        index += 1;
    }
    format!("{value:.1} {}", units[index])
}

/// Original download card/empty-state glass: CSS tokens are 6px blur and 180% saturation.
fn download_glass(mode: crate::skin::Colors, start: f32, end: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let corners = Corners::all(px(16.));
            #[cfg(target_os = "macos")]
            window.paint_backdrop(bounds, corners, 6., 1.8);
            window.paint_quad(
                fill(
                    bounds,
                    linear_gradient(
                        165.,
                        linear_color_stop(
                            color(mode, "--td-bg-color-container").opacity(start),
                            0.,
                        ),
                        linear_color_stop(color(mode, "--td-bg-color-container").opacity(end), 1.),
                    )
                    .color_space(ColorSpace::Srgb),
                )
                .corner_radii(corners),
            );
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

fn download_progress_gradient(start: Hsla, end: Hsla, brightness: f32) -> Background {
    let brighten = |color: Hsla| {
        let rgb = color.to_rgb();
        Hsla::from(Rgba {
            r: (rgb.r * brightness).min(1.),
            g: (rgb.g * brightness).min(1.),
            b: (rgb.b * brightness).min(1.),
            a: rgb.a,
        })
    };
    linear_gradient(
        90.,
        linear_color_stop(brighten(start), 0.),
        linear_color_stop(brighten(end), 1.),
    )
    .color_space(ColorSpace::Srgb)
}

/// Opens a folder in the file manager.
pub(super) fn open_directory(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Opens the file manager at a downloaded file.
fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(parent) = path.parent() {
        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
    }
}

impl Mio {
    /// The queue, created on first use with its records file next to the app's other data.
    /// Called at startup too, so tasks left from the last run come back right away.
    pub(super) fn download_manager(&mut self) -> Arc<Downloads> {
        let records = self.store.as_ref().map(Storage::downloads_path);
        let manager = self
            .song_view
            .downloads
            .manager
            .get_or_insert_with(|| {
                Arc::new(Downloads::new(
                    std::path::PathBuf::from(&self.directories.download_dir),
                    records,
                ))
            })
            .clone();
        manager.set_tag_options(self.song_view.downloads.tags.clone());
        manager.set_filename_template(&self.app_settings.filename_template);
        if let Some(online) = &self.online {
            manager.set_online(online.clone());
        }
        manager
    }

    /// Repaint while anything is queued or running; stops by itself when the queue is idle.
    /// Loads the saved task list (once) and hands the queue the current plugin, so tasks left
    /// queued from the last run continue and the page shows them.
    pub(super) fn attach_downloads(&mut self, cx: &mut Context<Self>) {
        let manager = self.download_manager();
        if manager.active_count() > 0 {
            self.ensure_download_ticker(cx);
        }
    }

    pub(super) fn ensure_download_ticker(&mut self, cx: &mut Context<Self>) {
        if self.song_view.downloads.ticking {
            return;
        }
        self.song_view.downloads.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        let active = this
                            .song_view
                            .downloads
                            .manager
                            .as_ref()
                            .is_some_and(|m| m.active_count() > 0);
                        if !active {
                            this.song_view.downloads.ticking = false;
                        }
                        active
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    /// Adds online songs to the download queue at `quality` (a song that does not offer it gets
    /// the nearest one it has, like the original's `calculateBestQuality`) and describes what
    /// happened.
    pub(super) fn enqueue_downloads(
        &mut self,
        songs: Vec<Song>,
        quality: &str,
        cx: &mut Context<Self>,
    ) -> Notification {
        let songs: Vec<Song> = songs.into_iter().filter(crate::online::is_online).collect();
        let Some(online) = self.online.clone() else {
            return Notification::error(self.t("download.getUrlFailed"));
        };
        if songs.is_empty() {
            return Notification::error(self.t("download.unknownError"));
        }
        let manager = self.download_manager();
        let (mut added, mut duplicates, mut already, mut adjusted) =
            (0usize, 0usize, 0usize, 0usize);
        let mut first_error: Option<String> = None;
        let mut adjusted_to = String::new();
        for song in &songs {
            let Some(item) = online
                .item_json(&song.id)
                .and_then(|json| serde_json::from_value::<mio_music_sdk::MusicItem>(json).ok())
            else {
                first_error = first_error.or(Some(self.t("download.unknownError")));
                continue;
            };
            let offered: Vec<String> =
                crate::quality::choices(&item, &online.source_qualities(&item.source))
                    .into_iter()
                    .map(|c| c.quality)
                    .collect();
            let Some(chosen) = crate::quality::best_available(&offered, quality) else {
                first_error = first_error.or(Some(self.t("download.getUrlFailed")));
                continue;
            };
            if chosen != quality {
                adjusted += 1;
                adjusted_to = chosen.clone();
            }
            match manager.enqueue(&online, item, chosen) {
                Ok(Enqueued::Added) => added += 1,
                Ok(Enqueued::Duplicate) => duplicates += 1,
                Ok(Enqueued::AlreadyDone) => already += 1,
                Err(error) => first_error = first_error.or(Some(error)),
            }
        }
        self.ensure_download_ticker(cx);
        cx.notify();
        let note = |message: String| {
            if adjusted == 0 {
                return message;
            }
            let name = crate::quality::display_key(&adjusted_to)
                .map(|key| self.t(key))
                .unwrap_or(adjusted_to.clone());
            if songs.len() == 1 {
                format!(
                    "{message} · {}",
                    self.t("download.qualityUnavailable")
                        .replace("{name}", &name)
                )
            } else if self.english {
                format!("{message} · {adjusted} songs adjusted to a lower quality")
            } else {
                format!("{message} · {adjusted} 首音质不可用，已自动调整")
            }
        };
        if songs.len() == 1 {
            if added == 1 {
                Notification::success(note(self.t("download.queueViewTip")))
            } else if duplicates == 1 {
                Notification::warning(self.t("download.duplicateDownload"))
            } else if already == 1 {
                Notification::warning(self.t("download.alreadyDownloaded"))
            } else {
                Notification::error(
                    first_error.unwrap_or_else(|| self.t("download.downloadFailed")),
                )
            }
        } else if added > 0 {
            Notification::success(note(
                self.t("music.list.startDownloadCount")
                    .replace("{count}", &added.to_string()),
            ))
        } else if duplicates > 0 {
            Notification::warning(self.t("download.duplicateDownload"))
        } else if already > 0 {
            Notification::warning(self.t("download.alreadyDownloaded"))
        } else {
            Notification::error(first_error.unwrap_or_else(|| self.t("download.downloadFailed")))
        }
    }

    fn task_status_text(&self, task: &TaskView) -> String {
        match &task.status {
            Status::Queued => if self.english { "Waiting" } else { "等待中" }.into(),
            Status::Paused => match task.total {
                Some(total) if total > 0 => format!(
                    "{} · {}%",
                    self.t("common.paused"),
                    (task.received.saturating_mul(100) / total).min(100),
                ),
                _ => self.t("common.paused"),
            },
            Status::Resolving => self
                .t("download.gettingUrl")
                .trim_end_matches('：')
                .to_owned(),
            Status::Downloading => {
                let progress = task.total.filter(|total| *total > 0).map(|total| {
                    format!("{}%", (task.received.saturating_mul(100) / total).min(100))
                });
                // `formatRemaining`: nothing for an unknown or infinite time.
                let remaining = task
                    .remaining
                    .filter(|seconds| seconds.is_finite() && *seconds > 0.)
                    .map(|seconds| {
                        if seconds < 60. {
                            self.t("common.unitSecond")
                                .replace("{seconds}", &seconds.round().to_string())
                        } else {
                            self.t("common.unitMinuteSecond")
                                .replace("{m}", &((seconds / 60.).floor() as u64).to_string())
                                .replace("{s}", &(seconds % 60.).round().to_string())
                        }
                    });
                progress
                    .into_iter()
                    .chain(std::iter::once(format_speed(task.speed)))
                    .chain(remaining)
                    .collect::<Vec<_>>()
                    .join(" · ")
            }
            Status::Done if task.missing => if self.english {
                "File moved or deleted"
            } else {
                "文件已被移动或删除"
            }
            .into(),
            Status::Done if task.skipped => if self.english {
                "Already exists, skipped"
            } else {
                "文件已存在，已跳过"
            }
            .into(),
            Status::Done => match &task.tag_error {
                Some(_) => format!(
                    "{} · {}",
                    self.t("download.alreadyDownloaded"),
                    self.t("download.tagWriteFailed")
                ),
                None => self.t("download.alreadyDownloaded"),
            },
            Status::Failed(message) => message.clone(),
            Status::Cancelled => if self.english {
                "Cancelled"
            } else {
                "已取消"
            }
            .into(),
        }
    }

    pub(super) fn downloads_page(
        &self,
        narrow: bool,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let tasks: Vec<TaskView> = self
            .song_view
            .downloads
            .manager
            .as_ref()
            .map(|m| m.snapshot())
            .unwrap_or_default();
        let bucket = |task: &TaskView| match task.status {
            Status::Queued | Status::Resolving | Status::Downloading | Status::Paused => 0u8,
            Status::Done => 1,
            Status::Failed(_) | Status::Cancelled => 2,
        };
        let counts = [0u8, 1, 2].map(|b| tasks.iter().filter(|t| bucket(t) == b).count());
        let tab = self.song_view.downloads.tab;
        let settings_open = self.song_view.downloads.settings_open;
        let concurrent = self
            .song_view
            .downloads
            .manager
            .as_ref()
            .map(|m| m.max_concurrent())
            .unwrap_or(3);
        let keyword = self
            .song_view
            .downloads
            .search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let searching = !keyword.is_empty();
        let mut visible: Vec<&TaskView> = tasks
            .iter()
            .filter(|t| bucket(t) == tab)
            .filter(|t| {
                !searching
                    || t.title.to_lowercase().contains(&keyword)
                    || t.artist.to_lowercase().contains(&keyword)
            })
            .collect();
        if tab == 0 {
            // Transferring first, then waiting, then paused; oldest first within each.
            let order = |t: &TaskView| match t.status {
                Status::Resolving | Status::Downloading => 0u8,
                Status::Queued => 1,
                _ => 2,
            };
            visible.sort_by_key(|t| order(t));
        } else {
            visible.reverse();
        }
        // `.tab-pill`: a rounded pill with a status dot and a count badge.
        let primary_text = color(mode, "--td-text-color-primary");
        let brand = color(mode, "--td-brand-color");
        let mut tabs = Some(
            div()
                .id("dl-tabs")
                .test_support()
                .flex()
                .when(narrow, |tabs| {
                    tabs.overflow_x_scroll()
                        .overflow_y_hidden()
                        .restrict_scroll_to_axis()
                        .pb(px(4.))
                })
                .gap(px(8.))
                .mb(px(if narrow { 16. } else { 0. }))
                .flex_shrink_0()
                .children(
                    [
                        (
                            "download-tab-active",
                            "download.tabActive",
                            "--td-brand-color",
                        ),
                        (
                            "download-tab-done",
                            "download.tabCompleted",
                            "--td-success-color",
                        ),
                        (
                            "download-tab-failed",
                            "download.tabFailed",
                            "--td-error-color",
                        ),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (id, key, dot))| {
                        let selected = tab as usize == index;
                        div()
                            .id(["dl-tab1", "dl-tab2", "dl-tab3"][index])
                            .test_support()
                            .child(
                                gpui_kit::base::Button::new(id)
                                    .aria_selected(selected)
                                    .accessibility_label(self.t(key))
                                    .flex_shrink_0()
                                    .when(narrow, |tab| tab.min_h(px(44.)))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    // 16px in the stylesheet, less the ~1.3px the labels come out wider by.
                                    .px(px(if narrow { 18. } else { 15.35 }))
                                    .py(px(8.))
                                    .rounded(px(999.))
                                    .text_size(px(13.))
                                    .line_height(px(16.))
                                    .bg(if selected {
                                        brand.opacity(0.15)
                                    } else {
                                        primary_text.opacity(0.06)
                                    })
                                    .hover(move |style| {
                                        style.bg(if selected {
                                            brand.opacity(0.15)
                                        } else {
                                            primary_text.opacity(0.10)
                                        })
                                    })
                                    .text_color(if selected {
                                        brand
                                    } else {
                                        color(mode, "--td-text-color-secondary")
                                    })
                                    .child(div().size(px(8.)).rounded_full().bg(color(mode, dot)))
                                    .child(self.t(key))
                                    .when(counts[index] > 0, |pill| {
                                        pill.child(
                                            div()
                                                .px(px(7.))
                                                .py(px(1.))
                                                .rounded(px(999.))
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .bg(if selected {
                                                    brand.opacity(0.20)
                                                } else {
                                                    primary_text.opacity(0.10)
                                                })
                                                .child(counts[index].to_string()),
                                        )
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.song_view.downloads.tab = index as u8;
                                        if index == 1
                                            && let Some(manager) = &this.song_view.downloads.manager
                                        {
                                            manager.validate_files();
                                        }
                                        cx.notify();
                                    })),
                            )
                    }),
                ),
        );
        let mut list = div()
            .id("download-list")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .when(narrow, |list| list.gap(px(10.)).pb(px(12.)));
        if visible.is_empty() {
            list = list.child(
                div()
                    .relative()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .py(px(if narrow { 48. } else { 60. }))
                    .px(px(20.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(primary_text.opacity(0.08))
                    .child(download_glass(mode, 0.5, 0.35))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(
                        div()
                            .mb(px(16.))
                            .text_color(color(mode, "--td-text-color-placeholder"))
                            .opacity(0.5)
                            .child(glyph("xiazai").size(px(48.))),
                    )
                    .child(div().text_size(px(14.)).child(self.t(if searching {
                        "download.noMatch"
                    } else {
                        "download.empty"
                    }))),
            );
        }
        let visible_is_empty = visible.is_empty();
        for task in visible {
            let id = task.id.clone();
            let active = task.status.is_active();
            let paused = task.status == Status::Paused;
            // "Show in folder" only for a file that is still there.
            let done_path = (task.status == Status::Done && !task.missing)
                .then(|| task.path.clone())
                .flatten();
            let fraction = match (&task.status, task.total) {
                (_, Some(total)) if total > 0 && task.status != Status::Done => {
                    (task.received as f32 / total as f32).clamp(0., 1.)
                }
                (Status::Done, _) => 1.,
                _ => 0.,
            };
            let now = Instant::now();
            let width_motion = window.use_keyed_state(
                (
                    ElementId::from(SharedString::from(id.clone())),
                    "download-width",
                ),
                cx,
                |_, _| player_motion::Tween::new(fraction),
            );
            let reduce_motion = cx.reduce_motion();
            let fraction = width_motion.update(cx, |motion, _| {
                motion.target(fraction, now, 0.3, [0.25, 0.1, 0.25, 1.], reduce_motion);
                if motion.active(now, 0.3) {
                    window.request_animation_frame();
                }
                motion.value(now, 0.3, [0.25, 0.1, 0.25, 1.])
            });
            let hover = window.use_keyed_state(
                (
                    ElementId::from(SharedString::from(id.clone())),
                    "download-hover",
                ),
                cx,
                |_, _| false,
            );
            let hovered = *hover.read(cx);
            let hover_amount = transition(
                (SharedString::from(id.clone()), "download-glass-hover"),
                if hovered { 1_f32 } else { 0. },
                Transition::new(Duration::from_millis(280))
                    .ease(|t| player_motion::easing(t, [0.16, 1., 0.3, 1.])),
                window,
                cx,
            );
            let downloading = task.status == Status::Downloading;
            let animate = downloading && !cx.reduce_motion();
            let pulse = || {
                Animation::new(Duration::from_secs(2))
                    .repeat()
                    .with_easing(|t| {
                        player_motion::easing(
                            if t < 0.5 { t * 2. } else { (1. - t) * 2. },
                            [0.42, 0., 0.58, 1.],
                        )
                    })
            };
            let failed = matches!(task.status, Status::Failed(_));
            let status_color = color(
                mode,
                match task.status {
                    Status::Done => "--td-success-color",
                    Status::Failed(_) => "--td-error-color",
                    Status::Paused | Status::Queued => "--td-warning-color",
                    Status::Cancelled => "--td-text-color-placeholder",
                    _ => "--td-brand-color",
                },
            );
            let (gradient_start, gradient_end) = if failed {
                (
                    status_color,
                    mixed_color(mode, "--td-error-color", 0.7, "--td-error-color-hover"),
                )
            } else if task.status == Status::Done {
                (
                    status_color,
                    mixed_color(mode, "--td-success-color", 0.7, "--td-success-color-hover"),
                )
            } else {
                (
                    brand,
                    mixed_color(mode, "--td-brand-color", 0.7, "--td-brand-color-8"),
                )
            };
            let action = |id: String, name: &str, label: String, danger: bool| {
                gpui_kit::base::Button::new(SharedString::from(id))
                    .accessibility_label(label.clone())
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                    })
                    .size(px(if narrow { 44. } else { 32. }))
                    .cursor_pointer()
                    .flex_shrink_0()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(primary_text.opacity(0.06))
                    .text_color(color(
                        mode,
                        if danger {
                            "--td-error-color"
                        } else {
                            "--td-text-color-secondary"
                        },
                    ))
                    .hover(move |style| {
                        style
                            .bg(if danger {
                                color(mode, "--td-error-color").opacity(0.12)
                            } else {
                                primary_text.opacity(0.12)
                            })
                            .text_color(color(
                                mode,
                                if danger {
                                    "--td-error-color"
                                } else {
                                    "--td-text-color-primary"
                                },
                            ))
                    })
                    // The reference's shuaxin/wenjianjia classes have no font glyph;
                    // reuse its existing Refresh/Folder SVG assets for those actions.
                    .child(
                        match name {
                            "shuaxin" => icon("Refresh"),
                            "wenjianjia" => icon("Folder"),
                            _ => glyph(name),
                        }
                        .size(px(if narrow { 16. } else { 14. })),
                    )
            };
            let actions = div()
                .flex()
                .items_center()
                .gap(px(if narrow { 8. } else { 6. }))
                .when(narrow, |actions| {
                    actions
                        .w_full()
                        .flex_wrap()
                        .justify_end()
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(primary_text.opacity(0.06))
                })
                .when(active, |row| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-pause-{id}"),
                            "zanting",
                            self.t("common.pause"),
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = &this.song_view.downloads.manager {
                                manager.pause(&id);
                            }
                            cx.notify();
                        })),
                    )
                })
                .when(paused, |row| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-resume-{id}"),
                            "bofang",
                            self.t("common.continue_"),
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = this.song_view.downloads.manager.clone() {
                                manager.resume(&id);
                                this.ensure_download_ticker(cx);
                            }
                            cx.notify();
                        })),
                    )
                })
                .when(failed, |row| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-retry-{id}"),
                            "shuaxin",
                            self.t("common.retry"),
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = this.song_view.downloads.manager.clone() {
                                manager.retry(&id);
                                this.ensure_download_ticker(cx);
                            }
                            cx.notify();
                        })),
                    )
                })
                .when(active || paused || failed, |row| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-cancel-{id}"),
                            "guanbi",
                            self.t("common.cancel"),
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = &this.song_view.downloads.manager {
                                manager.cancel(&id);
                            }
                            cx.notify();
                        })),
                    )
                })
                .when_some(done_path, |row, path| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-reveal-{id}"),
                            "wenjianjia",
                            self.t("common.openLocation"),
                            false,
                        )
                        .on_click(move |_, _, _| reveal(&path)),
                    )
                })
                .when(!active && !paused, |row| {
                    let id = id.clone();
                    row.child(
                        action(
                            format!("download-remove-{id}"),
                            "shanchu",
                            self.t("common.delete"),
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = &this.song_view.downloads.manager {
                                manager.remove(&id);
                            }
                            cx.notify();
                        })),
                    )
                });
            list = list.child(
                div()
                    .id(SharedString::from(format!("download-card-{id}")))
                    .relative()
                    .top(px(if narrow { 0. } else { -2. * hover_amount }))
                    .shadow(vec![
                        BoxShadow::new(
                            px(0.),
                            px(3. + 3. * hover_amount),
                            black().opacity(0.08 + 0.04 * hover_amount),
                        )
                        .blur_radius(px(12. + 8. * hover_amount)),
                    ])
                    .on_hover(move |hovered, _, cx| {
                        hover.update(cx, |state, cx| {
                            *state = *hovered;
                            cx.notify();
                        });
                    })
                    .w_full()
                    .px(px(12.))
                    .py(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .border_1()
                    .rounded(px(16.))
                    .border_color(primary_text.opacity(0.10 + 0.08 * hover_amount))
                    .child(download_glass(mode, 0.60, 0.48))
                    .when(narrow, |row| {
                        row.flex_col()
                            .items_start()
                            .p(px(14.))
                            .gap(px(10.))
                            .rounded(px(16.))
                    })
                    .child(
                        div()
                            .flex_1()
                            .when(narrow, |column| column.w_full().flex_none())
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(if narrow { 10. } else { 4. }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(
                                        div()
                                            .size(px(8.))
                                            .flex_shrink_0()
                                            .rounded_full()
                                            .bg(status_color)
                                            .when(paused, |dot| dot.opacity(0.6))
                                            .when(downloading, |dot| {
                                                dot.shadow(vec![
                                                    BoxShadow::new(
                                                        px(0.),
                                                        px(0.),
                                                        brand.opacity(0.5),
                                                    )
                                                    .blur_radius(px(6.)),
                                                ])
                                            })
                                            .map(|dot| {
                                                if animate {
                                                    dot.with_animation(
                                                        SharedString::from(format!(
                                                            "download-dot-{id}"
                                                        )),
                                                        pulse(),
                                                        |dot, amount| {
                                                            dot.opacity(1. - 0.5 * amount)
                                                        },
                                                    )
                                                    .into_any_element()
                                                } else {
                                                    dot.into_any_element()
                                                }
                                            }),
                                    )
                                    .text_size(px(if narrow { 15. } else { 14. }))
                                    .when(narrow, |title| title.line_height(px(21.)))
                                    .text_color(color(mode, "--td-text-color-primary"))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .when(!narrow, |title| title.truncate())
                                            .child(task.title.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(12.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(
                                        div()
                                            .when(!narrow, |artist| {
                                                artist.max_w(px(150.)).truncate()
                                            })
                                            .child(task.artist.clone()),
                                    )
                                    .child("·")
                                    .child(
                                        div()
                                            .px(px(5.))
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(primary_text.opacity(0.08))
                                            .bg(color(mode, "--td-bg-color-secondarycontainer")
                                                .opacity(0.85))
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(task.quality.to_uppercase()),
                                    )
                                    .child("·")
                                    .child(
                                        div()
                                            .text_color(color(mode, "--td-text-color-placeholder"))
                                            .child(format!(
                                                "{} / {}",
                                                format_bytes(task.received),
                                                format_bytes(task.total.unwrap_or(0))
                                            )),
                                    ),
                            )
                            .when(narrow && task.status == Status::Downloading, |column| {
                                column.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(10.))
                                        .text_size(px(12.))
                                        .text_color(brand)
                                        .child(format_speed(task.speed))
                                        .children(
                                            task.remaining
                                                .filter(|s| s.is_finite() && *s > 0.)
                                                .map(|s| {
                                                    let label = if s < 60. {
                                                        self.t("common.unitSecond").replace(
                                                            "{seconds}",
                                                            &s.round().to_string(),
                                                        )
                                                    } else {
                                                        self.t("common.unitMinuteSecond")
                                                            .replace(
                                                                "{m}",
                                                                &(s / 60.).floor().to_string(),
                                                            )
                                                            .replace(
                                                                "{s}",
                                                                &(s % 60.).round().to_string(),
                                                            )
                                                    };
                                                    div()
                                                        .text_color(color(
                                                            mode,
                                                            "--td-text-color-placeholder",
                                                        ))
                                                        .child(label)
                                                }),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .h(px(if narrow { 8. } else { 6. }))
                                    .w_full()
                                    .rounded_full()
                                    .overflow_hidden()
                                    .bg(primary_text.opacity(0.08))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(fraction))
                                            .rounded_full()
                                            .bg(download_progress_gradient(
                                                gradient_start,
                                                gradient_end,
                                                1.,
                                            ))
                                            .map(|fill| {
                                                if animate {
                                                    fill.with_animation(
                                                        SharedString::from(format!(
                                                            "download-shimmer-{id}"
                                                        )),
                                                        pulse(),
                                                        move |fill, amount| {
                                                            fill.bg(download_progress_gradient(
                                                                gradient_start,
                                                                gradient_end,
                                                                1. + 0.2 * amount,
                                                            ))
                                                        },
                                                    )
                                                    .into_any_element()
                                                } else {
                                                    fill.into_any_element()
                                                }
                                            }),
                                    ),
                            ),
                    )
                    .when(!narrow || failed, |row| {
                        row.child(
                            div()
                                .when(narrow, |status| status.w_full().line_height(px(16.8)))
                                .when(!narrow, |status| status.max_w(px(260.)).truncate())
                                .flex_shrink_0()
                                .text_size(px(12.))
                                .text_color(color(
                                    mode,
                                    if failed {
                                        "--td-error-color"
                                    } else {
                                        "--td-text-color-secondary"
                                    },
                                ))
                                .child(self.task_status_text(task)),
                        )
                    })
                    .child(actions),
            );
        }
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                20.
            }))
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                20.
            }))
            .pb(px(if narrow { 0. } else { 20. }))
            .bg(color(mode, "--td-bg-color-container"))
            .child({
                // `.toolbar-btn`: a pill with an optional glyph and a label.
                let toolbar = |id: &'static str,
                               glyph_name: Option<&'static str>,
                               label: Option<String>,
                               active: bool,
                               disabled: bool| {
                    let tooltip = label
                        .clone()
                        .unwrap_or_else(|| self.t("common.titleBar.settings"));
                    // Wide windows keep the toolbar to icons so the tabs fit beside it.
                    let label = label.filter(|_| narrow);
                    let has_label = label.is_some();
                    gpui_kit::base::Button::new(id)
                        .accessibility_label(tooltip.clone())
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                                .build(window, cx)
                        })
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        // 12px in the stylesheet; the CJK labels come out about 2.4px wider here
                        // than in the browser, split over the two sides.
                        .px(px(if narrow {
                            14.
                        } else if has_label {
                            10.4
                        } else {
                            12.
                        }))
                        .when(!has_label && !narrow, |button| button.h(px(29.)))
                        .when(narrow, |button| button.min_h(px(44.)))
                        .py(px(6.))
                        .rounded(px(999.))
                        .border_1()
                        .text_size(px(13.))
                        .line_height(px(16.))
                        .when(disabled, |b| b.opacity(0.5))
                        .border_color(if active {
                            brand.opacity(0.25)
                        } else {
                            primary_text.opacity(0.10)
                        })
                        .bg(if active {
                            brand.opacity(0.12)
                        } else {
                            primary_text.opacity(0.04)
                        })
                        .text_color(if active {
                            brand
                        } else {
                            color(mode, "--td-text-color-secondary")
                        })
                        .hover(move |style| {
                            style
                                .bg(primary_text.opacity(0.08))
                                .border_color(primary_text.opacity(0.18))
                                .text_color(primary_text)
                        })
                        .children(glyph_name.map(|name| glyph(name).size(px(14.))))
                        .children(label)
                };
                div()
                    .id("dl-header")
                    .test_support()
                    .flex()
                    .items_center()
                    .when(narrow, |header| header.flex_col().items_stretch())
                    .justify_between()
                    .gap(px(12.))
                    .mb(px(if narrow { 12. } else { 16. }))
                    .flex_shrink_0()
                    .child(match tabs.take() {
                        // Wide windows trade the page title for the status tabs.
                        Some(tabs) if !narrow => {
                            div().flex_1().min_w_0().child(tabs).into_any_element()
                        }
                        other => {
                            tabs = other;

                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .id("dl-indicator")
                                        .test_support()
                                        .size(px(10.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(brand)
                                        .shadow(vec![BoxShadow {
                                            color: brand.opacity(0.4),
                                            offset: point(px(0.), px(0.)),
                                            blur_radius: px(8.),
                                            spread_radius: px(0.),
                                            inset: false,
                                        }]),
                                )
                                .child(
                                    div()
                                        .id("dl-title")
                                        .test_support()
                                        .text_size(rems(if narrow { 1.5 } else { 1.75 }))
                                        .font_weight(FontWeight::BOLD)
                                        .line_height(relative(1.3))
                                        .child(self.t("download.title")),
                                )
                                .into_any_element()
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .when(narrow, |bar| bar.w_full().flex_wrap())
                            .gap(px(8.))
                            .flex_shrink_0()
                            .child(
                                // The reference gives the search box its own full-width row on narrow windows.
                                div()
                                    .id("dl-search")
                                    .test_support()
                                    .relative()
                                    .when(narrow, |search| search.w_full())
                                    .when(!narrow, |search| search.w(px(180.)))
                                    .child(
                                        // `Input::h` is for multi-line inputs; the single-line
                                        // height is the style's.
                                        Styled::h(
                                            Input::new(&self.song_view.downloads.search)
                                                .aria_label(self.t("download.searchTask"))
                                                .appearance(false),
                                            px(if narrow { 44. } else { 34. }),
                                        )
                                        .pl(px(34.))
                                        .pr(px(14.))
                                        .rounded(px(999.))
                                        .border_1()
                                        .border_color(primary_text.opacity(0.12))
                                        .bg(color(mode, "--td-bg-color-component").opacity(0.6))
                                        .text_size(px(if narrow { 16. } else { 13. })),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(12.))
                                            .top(px(if narrow { 15. } else { 9.75 }))
                                            .text_color(color(mode, "--td-text-color-placeholder"))
                                            .child(icon("Search").size(px(14.))),
                                    ),
                            )
                            .when(tab == 0, |bar| {
                                bar.child(
                                    div().id("dl-start").test_support().child(
                                        toolbar(
                                            "download-start-all",
                                            Some("bofang"),
                                            Some(self.t("download.startAll")),
                                            false,
                                            !tasks.iter().any(|t| t.status == Status::Paused),
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                if let Some(manager) =
                                                    this.song_view.downloads.manager.clone()
                                                {
                                                    manager.resume_all();
                                                    this.ensure_download_ticker(cx);
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                                )
                                .child(
                                    div().id("dl-pause").test_support().child(
                                        toolbar(
                                            "download-pause-all",
                                            Some("zanting"),
                                            Some(self.t("download.pauseAll")),
                                            false,
                                            !tasks.iter().any(|t| t.status.is_active()),
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                if let Some(manager) =
                                                    &this.song_view.downloads.manager
                                                {
                                                    manager.pause_all();
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                                )
                            })
                            .when(!visible_is_empty, |bar| {
                                bar.child(
                                    toolbar(
                                        "download-clear-records",
                                        Some("shanchu"),
                                        Some(self.t(if tab == 0 {
                                            "download.clearQueue"
                                        } else {
                                            "download.clearRecords"
                                        })),
                                        false,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some(manager) = &this.song_view.downloads.manager
                                            {
                                                manager.clear(match tab {
                                                    0 => ClearScope::Queue,
                                                    1 => ClearScope::Completed,
                                                    _ => ClearScope::Failed,
                                                });
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                            .child(
                                div().id("dl-gear").test_support().child(
                                    toolbar(
                                        "download-settings",
                                        Some("shezhi"),
                                        None,
                                        settings_open,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.song_view.downloads.settings_open =
                                                !this.song_view.downloads.settings_open;
                                            cx.notify();
                                        },
                                    )),
                                ),
                            ),
                    )
            })
            .when(settings_open, |page| {
                let step = |id: &'static str, label: &'static str, delta: i32, enabled: bool| {
                    gpui_kit::base::Button::new(id)
                        .accessibility_label(label)
                        .size(px(if narrow { 44. } else { 32. }))
                        .rounded(px(999.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(color(mode, "--td-border-level-1-color"))
                        .bg(color(mode, "--td-bg-color-component"))
                        .text_size(px(16.))
                        .text_color(color(mode, "--td-text-color-primary"))
                        .hover(|style| style.bg(color(mode, "--td-bg-color-component-hover")))
                        .when(!enabled, |button| button.opacity(0.4))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(manager) = this.song_view.downloads.manager.clone() {
                                let next = (manager.max_concurrent() as i32 + delta)
                                    .clamp(1, crate::download::MAX_CONCURRENT as i32);
                                manager.set_max_concurrent(next as usize);
                            }
                            cx.notify();
                        }))
                };
                page.child(
                    div()
                        .id("download-settings-popup")
                        .flex()
                        .when(narrow, |popup| popup.flex_col().gap(px(10.)))
                        .items_center()
                        .justify_between()
                        .flex_shrink_0()
                        .px(px(16.))
                        .py(px(12.))
                        .mb(px(12.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(color(mode, "--td-border-level-1-color"))
                        .bg(color(mode, "--td-bg-color-component").opacity(0.7))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(self.t("download.concurrentDownloads")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(step("download-concurrent-minus", "-", -1, concurrent > 1))
                                .child(
                                    div()
                                        .id("download-concurrent-value")
                                        .min_w(px(20.))
                                        .text_size(px(16.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_center()
                                        .child(concurrent.to_string()),
                                )
                                .child(step(
                                    "download-concurrent-plus",
                                    "+",
                                    1,
                                    concurrent < crate::download::MAX_CONCURRENT,
                                )),
                        ),
                )
            })
            .children(tabs)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(12.))
                    .bg(transparent_black())
                    .child(list),
            )
    }
}
