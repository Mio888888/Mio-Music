//! The version block of the About category (`AboutSection.vue` + `useAppUpdater.ts`): the
//! startup-check switch, the check button and the update card with its release notes,
//! download progress and restart.
use super::*;
use crate::updater::{self, Update};
use gpui_kit::component::{WindowExt, notification::Notification, switch::Switch};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Downloaded,
    UpToDate,
    Error,
}

#[derive(Default)]
pub(super) struct UpdateUi {
    status: UpdateStatus,
    update: Option<Update>,
    error: String,
    received: u64,
    total: u64,
    notes_expanded: bool,
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1024. / 1024.)
}

impl Mio {
    /// `checkForUpdate`.
    pub(super) fn check_for_update(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update_ui.status,
            UpdateStatus::Checking | UpdateStatus::Downloading
        ) {
            return;
        }
        self.update_ui = UpdateUi {
            status: UpdateStatus::Checking,
            ..Default::default()
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { updater::check() })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Some(update)) => {
                        this.update_ui.update = Some(update);
                        this.update_ui.status = UpdateStatus::Available;
                    }
                    Ok(None) => this.update_ui.status = UpdateStatus::UpToDate,
                    Err(error) => {
                        this.update_ui.error = error;
                        this.update_ui.status = UpdateStatus::Error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The startup check, when "应用启动时检查更新" is on. A missing channel stays silent.
    pub(super) fn check_for_update_at_startup(&mut self, cx: &mut Context<Self>) {
        if self.app_settings.auto_update && updater::endpoint().is_some() {
            self.check_for_update(cx);
        }
    }

    /// `downloadAndInstall`: download with progress, check the signature, install.
    fn download_and_install(&mut self, cx: &mut Context<Self>) {
        let Some(update) = self.update_ui.update.clone() else {
            return;
        };
        self.update_ui.status = UpdateStatus::Downloading;
        self.update_ui.error.clear();
        self.update_ui.received = 0;
        self.update_ui.total = 0;
        cx.notify();
        let progress = Arc::new(Mutex::new((0u64, 0u64)));
        let reporter = progress.clone();
        let task = cx.background_executor().spawn(async move {
            let bytes = updater::download(&update, move |received, total| {
                if let Ok(mut p) = reporter.lock() {
                    *p = (received, total.unwrap_or(0));
                }
            })?;
            updater::install(&bytes)
        });
        cx.spawn(async move |this, cx| {
            // Progress is read while the transfer runs.
            let finished = std::sync::atomic::AtomicBool::new(false);
            let watcher = async {
                while !finished.load(std::sync::atomic::Ordering::Relaxed) {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(200))
                        .await;
                    let (received, total) = progress.lock().map(|p| *p).unwrap_or((0, 0));
                    if this
                        .update(cx, |this, cx| {
                            this.update_ui.received = received;
                            this.update_ui.total = total;
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            };
            let work = async {
                let result = task.await;
                finished.store(true, std::sync::atomic::Ordering::Relaxed);
                result
            };
            let (result, _) = futures_lite_join(work, watcher).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.update_ui.status = UpdateStatus::Downloaded;
                        this.update_ui.received = this.update_ui.total.max(this.update_ui.received);
                    }
                    Err(error) => {
                        this.update_ui.error = error;
                        this.update_ui.status = UpdateStatus::Error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn restart_to_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_session();
        match updater::relaunch() {
            Ok(()) => cx.quit(),
            Err(_) => window.push_notification(
                Notification::error(self.t("settings.about.restartFailed")),
                cx,
            ),
        }
    }

    pub(super) fn update_section(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let success = color(mode, "--td-success-color");
        let status = self.update_ui.status;
        let busy = matches!(status, UpdateStatus::Checking | UpdateStatus::Downloading);
        let card = || {
            div()
                .mt(px(16.))
                .p(px(16.))
                .rounded(px(10.))
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .bg(color(mode, "--td-bg-color-container"))
                .flex()
                .flex_col()
                .gap(px(10.))
        };
        let badge = |text: String, tone: Hsla| {
            div()
                .px(px(8.))
                .py(px(1.))
                .rounded(px(999.))
                .text_size(px(12.))
                .text_color(tone)
                .bg(tone.opacity(0.12))
                .child(text)
        };
        let mut section = div().flex().flex_col().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            Switch::new("auto-update")
                                .accessibility_label(self.t("settings.about.checkUpdateOnStart"))
                                .checked(self.app_settings.auto_update)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.app_settings.auto_update = *checked;
                                    this.save_preferences();
                                    cx.notify();
                                })),
                        )
                        .child(self.t("settings.about.checkUpdateOnStart")),
                )
                .child(
                    Button::new("check-update")
                        .label(if status == UpdateStatus::Checking {
                            self.t("settings.about.checking")
                        } else {
                            self.t("settings.about.checkUpdate")
                        })
                        .primary()
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.check_for_update(cx))),
                ),
        );
        match status {
            UpdateStatus::UpToDate => {
                section = section.child(
                    card().child(
                        div()
                            .text_color(success)
                            .child(format!("✓ {}", self.t("settings.about.upToDate"))),
                    ),
                );
            }
            UpdateStatus::Available => {
                let update = self.update_ui.update.clone().unwrap_or(Update {
                    version: String::new(),
                    notes: String::new(),
                    url: String::new(),
                    signature: String::new(),
                });
                let long_notes = update.notes.lines().count() > 8 || update.notes.len() > 500;
                let expanded = self.update_ui.notes_expanded;
                let mut body = card().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().text_color(brand).child("↑"))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(primary)
                                .child(self.t("settings.about.newVersionFound")),
                        )
                        .child(badge(
                            self.t("settings.about.versionBadge")
                                .replace("{version}", &update.version),
                            brand,
                        )),
                );
                if !update.notes.trim().is_empty() {
                    body = body
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(secondary)
                                .child(self.t("settings.about.releaseNotes")),
                        )
                        .child(
                            div()
                                .when(long_notes && !expanded, |d| {
                                    d.max_h(px(160.)).overflow_hidden()
                                })
                                .text_size(px(13.))
                                .text_color(primary)
                                .child(gpui_kit::component::text::markdown(update.notes.clone())),
                        );
                    if long_notes {
                        body = body.child(
                            Button::new("update-notes-toggle")
                                .label(if expanded {
                                    self.t("settings.about.hideFullNotes")
                                } else {
                                    self.t("settings.about.showFullNotes")
                                })
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.update_ui.notes_expanded = !this.update_ui.notes_expanded;
                                    cx.notify();
                                })),
                        );
                    }
                }
                section = section.child(
                    body.child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                Button::new("update-install")
                                    .label(self.t("settings.about.downloadAndInstall"))
                                    .primary()
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.download_and_install(cx)),
                                    ),
                            )
                            .child(
                                Button::new("update-later")
                                    .label(self.t("settings.about.remindLater"))
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.update_ui = UpdateUi::default();
                                        cx.notify();
                                    })),
                            ),
                    ),
                );
            }
            UpdateStatus::Downloading => {
                let (received, total) = (self.update_ui.received, self.update_ui.total);
                let percent = if total > 0 {
                    ((received as f64 / total as f64) * 100.).round().min(100.) as u32
                } else {
                    0
                };
                let version = self
                    .update_ui
                    .update
                    .as_ref()
                    .map(|u| u.version.clone())
                    .unwrap_or_default();
                section = section.child(
                    card()
                        .child(div().text_color(primary).child(format!(
                            "↓ {} v{version}...",
                            self.t("settings.about.downloading")
                        )))
                        .child(
                            div()
                                .h(px(10.))
                                .w_full()
                                .rounded(px(5.))
                                .bg(color(mode, "--td-bg-color-component"))
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(percent as f32 / 100.))
                                        .rounded(px(5.))
                                        .bg(brand),
                                ),
                        )
                        .child(div().text_size(px(12.)).text_color(secondary).child(
                            if total > 0 {
                                format!(
                                    "{percent}%   {} MB / {} MB",
                                    megabytes(received),
                                    megabytes(total)
                                )
                            } else {
                                format!("{} MB", megabytes(received))
                            },
                        )),
                );
            }
            UpdateStatus::Downloaded => {
                let version = self
                    .update_ui
                    .update
                    .as_ref()
                    .map(|u| u.version.clone())
                    .unwrap_or_default();
                section = section.child(
                    card()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(div().text_color(success).child("✓"))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(self.t("settings.about.updateDownloaded")),
                                )
                                .child(badge(
                                    self.t("settings.about.versionBadge")
                                        .replace("{version}", &version),
                                    success,
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(secondary)
                                .child(self.t("settings.about.restartHint")),
                        )
                        .child(
                            div().child(
                                Button::new("update-restart")
                                    .label(self.t("settings.about.restartNow"))
                                    .primary()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.restart_to_install(window, cx)
                                    })),
                            ),
                        ),
                );
            }
            UpdateStatus::Error => {
                section = section.child(
                    card().child(
                        div()
                            .text_color(color(mode, "--td-error-color"))
                            .child(format!("! {}", self.update_ui.error)),
                    ),
                );
            }
            UpdateStatus::Idle | UpdateStatus::Checking => {}
        }
        let _ = narrow;
        section.into_any_element()
    }
}

/// Runs two futures to completion together, returning both outputs.
async fn futures_lite_join<A, B>(a: A, b: B) -> (A::Output, B::Output)
where
    A: std::future::Future,
    B: std::future::Future,
{
    let mut a = Box::pin(a);
    let mut b = Box::pin(b);
    let mut a_out = None;
    let mut b_out = None;
    std::future::poll_fn(|cx| {
        use std::task::Poll;
        if a_out.is_none()
            && let Poll::Ready(v) = a.as_mut().poll(cx)
        {
            a_out = Some(v);
        }
        if b_out.is_none()
            && let Poll::Ready(v) = b.as_mut().poll(cx)
        {
            b_out = Some(v);
        }
        if a_out.is_some() && b_out.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    (a_out.unwrap(), b_out.unwrap())
}
