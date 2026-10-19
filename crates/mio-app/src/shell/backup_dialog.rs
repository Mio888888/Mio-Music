//! The "备份 & 恢复" dialog (`BackupRestore.vue` / `S3ConfigDialog.vue` / `S3Backup.ts`):
//! recommended providers, the S3 connection, and encrypted backup/restore of playlists, settings
//! and the music-source plugin. The dialog entity lives as long as the window, like the
//! original's store, so the connection and passwords survive closing and opening it again.
use super::*;
use crate::backup::{self, S3Config, Saved};
use gpui_kit::component::{
    WindowExt, button::ButtonCustomVariant, notification::Notification, switch::Switch,
};

const HI168_URL: &str = "https://www.hi168.com";
const CSTCLOUD_URL: &str = "https://data.cstcloud.cn/";

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Recommend,
    Config,
    Ops,
}

#[derive(Clone, Copy, PartialEq)]
enum Busy {
    Idle,
    Connecting,
    BackingUp,
    Restoring,
}

pub(super) struct BackupDialog {
    parent: WeakEntity<Mio>,
    store: Option<Storage>,
    english: bool,
    colors: crate::skin::Colors,
    tab: Tab,
    busy: Busy,
    connected: bool,
    error: Option<String>,
    last_backup: Option<String>,
    path_style: bool,
    endpoint: Entity<InputState>,
    region: Entity<InputState>,
    bucket: Entity<InputState>,
    access_key: Entity<InputState>,
    secret: Entity<InputState>,
    password: Entity<InputState>,
    restore_password: Entity<InputState>,
    max_backups: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl Mio {
    /// Whether the S3 connection of the backup dialog is up (the dot on the top-bar button).
    pub(super) fn backup_connected(&self, cx: &App) -> bool {
        self.backup_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.read(cx).connected)
    }

    pub(super) fn open_backup_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let english = self.english;
        let colors = self.ui_colors();
        let dialog_entity = match &self.backup_dialog {
            Some(entity) => entity.clone(),
            None => {
                let parent = cx.weak_entity();
                let store = self.store.clone();
                let entity = cx.new(|cx| BackupDialog::new(parent, store, english, window, cx));
                self.backup_dialog = Some(entity.clone());
                entity
            }
        };
        dialog_entity.update(cx, |dialog, cx| {
            dialog.english = english;
            dialog.colors = colors;
            cx.notify();
        });
        window.open_dialog(cx, {
            let content = dialog_entity.clone();
            move |dialog, window, _| {
                let narrow = window.viewport_size().width <= px(768.);
                let width = f32::from(window.viewport_size().width);
                let busy = content.clone();
                dialog
                    .w(px((width - 32.).min(500.)))
                    .margin_top(((window.viewport_size().height - px(560.)).max(px(24.))) / 2.)
                    .rounded(px(if narrow { 18. } else { 22. }))
                    .p(px(0.))
                    .gap(px(0.))
                    .bg(color(colors, "--td-text-color-primary").opacity(0.02))
                    .border_color(white().opacity(0.4))
                    .overlay_tint(black().opacity(0.45))
                    .when(cfg!(target_os = "macos"), |dialog| {
                        dialog.overlay_filter(8., 1.4).surface_filter(
                            6.56,
                            1.8,
                            px(if narrow { 18. } else { 22. }),
                        )
                    })
                    .close_button(false)
                    .on_ok(|_, _, _| false)
                    .on_cancel(move |_, _, cx| busy.read(cx).busy == Busy::Idle)
                    .child(content.clone())
            }
        });
    }
}

impl BackupDialog {
    fn new(
        parent: WeakEntity<Mio>,
        store: Option<Storage>,
        english: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved = Saved::load(store.as_ref());
        let input = |placeholder: &str,
                     value: &str,
                     masked: bool,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            let placeholder = placeholder.to_owned();
            let value = value.to_owned();
            cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                if masked {
                    state = state.masked(true);
                }
                if !value.is_empty() {
                    state.set_value(&value, window, cx);
                }
                state
            })
        };
        let endpoint = input(
            "https://s3.amazonaws.com",
            &saved.endpoint,
            false,
            window,
            cx,
        );
        let region = input("auto", &saved.region, false, window, cx);
        let bucket = input("my-bucket", &saved.bucket, false, window, cx);
        let access_key = input("AKIA...", &saved.access_key_id, true, window, cx);
        let secret = input("••••••••", "", true, window, cx);
        let password = input(
            &i18n::text(english, "backup.encryptBackupPlaceholder"),
            "",
            true,
            window,
            cx,
        );
        let restore_password = input(
            &i18n::text(english, "backup.restorePassword"),
            "",
            true,
            window,
            cx,
        );
        let max_backups = input("10", &saved.max_backups.to_string(), false, window, cx);
        let mut subscriptions = Vec::new();
        for state in [
            &endpoint,
            &region,
            &bucket,
            &access_key,
            &secret,
            &password,
            &restore_password,
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                |this: &mut BackupDialog, _, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        // Editing the connection invalidates "已连接".
                        cx.notify();
                    }
                    let _ = this;
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &max_backups,
            window,
            |this: &mut BackupDialog, _, event, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.persist(cx);
                }
                cx.notify();
            },
        ));
        Self {
            parent,
            store,
            english,
            colors: crate::skin::Colors::from(Mode::Light),
            tab: Tab::Recommend,
            busy: Busy::Idle,
            connected: false,
            error: None,
            last_backup: saved.last_backup_time,
            path_style: saved.path_style,
            endpoint,
            region,
            bucket,
            access_key,
            secret,
            password,
            restore_password,
            max_backups,
            _subscriptions: subscriptions,
        }
    }

    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn text(&self, state: &Entity<InputState>, cx: &App) -> String {
        state.read(cx).value().trim().to_owned()
    }

    fn config(&self, cx: &App) -> S3Config {
        let region = self.text(&self.region, cx);
        S3Config {
            endpoint: self.text(&self.endpoint, cx),
            region: if region.is_empty() {
                "auto".into()
            } else {
                region
            },
            access_key_id: self.text(&self.access_key, cx),
            secret_access_key: self.text(&self.secret, cx),
            bucket: self.text(&self.bucket, cx),
            path_style: self.path_style,
        }
    }

    fn config_valid(&self, cx: &App) -> bool {
        let config = self.config(cx);
        !config.endpoint.is_empty()
            && !config.access_key_id.is_empty()
            && !config.secret_access_key.is_empty()
            && !config.bucket.is_empty()
    }

    fn max_backups_value(&self, cx: &App) -> u32 {
        // The original clamps to 1–100 and falls back to 10 for anything that is not a number.
        self.text(&self.max_backups, cx)
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(10)
            .clamp(1, 100)
    }

    /// `saveConfig`: the address, region, key id, bucket and count — never the secret.
    fn persist(&mut self, cx: &App) {
        let Some(store) = &self.store else { return };
        let config = self.config(cx);
        let saved = Saved {
            endpoint: config.endpoint,
            region: config.region,
            access_key_id: config.access_key_id,
            bucket: config.bucket,
            max_backups: self.max_backups_value(cx),
            last_backup_time: self.last_backup.clone(),
            path_style: self.path_style,
        };
        let _ = saved.save(store);
    }

    fn status_text(&self) -> String {
        self.t(match self.busy {
            Busy::Connecting => "backup.connecting",
            Busy::BackingUp => "backup.backingUp",
            Busy::Restoring => "backup.restoring",
            Busy::Idle if self.connected => "backup.connected",
            Busy::Idle => "backup.disconnected",
        })
    }

    fn test_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy != Busy::Idle || !self.config_valid(cx) {
            return;
        }
        self.busy = Busy::Connecting;
        self.error = None;
        let config = self.config(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { backup::test_connection(&config) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = Busy::Idle;
                match result {
                    Ok(()) => {
                        this.connected = true;
                        this.persist(cx);
                        this.tab = Tab::Ops;
                    }
                    Err(message) => {
                        this.connected = false;
                        this.error = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn run_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy != Busy::Idle {
            return;
        }
        if !self.connected {
            self.error = Some(self.t("backup.connectFirst"));
            cx.notify();
            return;
        }
        let password = self.password.read(cx).value().to_string();
        if password.is_empty() {
            self.error = Some(self.t("backup.setPassword"));
            cx.notify();
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let Some((preferences, directories)) = self
            .parent
            .update(cx, |parent, _| {
                (parent.preferences_snapshot(), parent.directories.clone())
            })
            .ok()
        else {
            return;
        };
        self.busy = Busy::BackingUp;
        self.error = None;
        let config = self.config(cx);
        let max_backups = self.max_backups_value(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let payload = backup::collect(&store, &preferences, &directories)?;
                    backup::backup(&config, &password, max_backups, &payload)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = Busy::Idle;
                match result {
                    Ok(done) => {
                        this.last_backup = Some(done.timestamp);
                        this.persist(cx);
                        window.push_notification(
                            Notification::success(if this.english {
                                format!("Backup uploaded ({} plugin)", done.plugins)
                            } else {
                                format!("备份已上传（音源插件 {} 个）", done.plugins)
                            }),
                            cx,
                        );
                    }
                    Err(message) => this.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn run_restore(&mut self, overwrite: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy != Busy::Idle {
            return;
        }
        if !self.connected {
            self.error = Some(self.t("backup.connectFirst"));
            cx.notify();
            return;
        }
        let password = self.restore_password.read(cx).value().to_string();
        if password.is_empty() {
            self.error = Some(self.t("backup.inputRestorePassword"));
            cx.notify();
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        self.busy = Busy::Restoring;
        self.error = None;
        let config = self.config(cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let data = backup::fetch_latest(&config, &password)?;
                    backup::apply(&store, &data, overwrite)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = Busy::Idle;
                match result {
                    Ok(done) => {
                        window.push_notification(
                            Notification::success(if this.english {
                                format!(
                                    "Restored {} playlists and {} plugins. Restarting…",
                                    done.playlists, done.plugins
                                )
                            } else {
                                format!(
                                    "已恢复 {} 个歌单、{} 个音源插件，正在重启…",
                                    done.playlists, done.plugins
                                )
                            }),
                            cx,
                        );
                        // The original reloads its page; the settings, plugin and library are
                        // read once at startup here, so the application starts again.
                        let _ = this.parent.update(cx, |parent, _| parent.save_session());
                        match crate::updater::relaunch() {
                            Ok(()) => cx.quit(),
                            Err(_) => {
                                this.error = Some(
                                    if this.english {
                                        "Restored. Please restart the application."
                                    } else {
                                        "已恢复，请手动重启应用以生效。"
                                    }
                                    .into(),
                                );
                            }
                        }
                    }
                    Err(message) => this.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn format_time(iso: &str) -> String {
        chrono::DateTime::parse_from_rfc3339(iso)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%Y/%-m/%-d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|_| iso.to_owned())
    }
}

impl Render for BackupDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let brand = color(colors, "--td-brand-color");
        let success = color(colors, "--td-success-color");
        let error_color = color(colors, "--td-error-color");
        let busy = self.busy != Busy::Idle;
        let narrow = window.viewport_size().width <= px(768.);
        let field_bg = color(colors, "--td-bg-color-component").opacity(0.5);

        let label = |text: String| {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(primary.opacity(0.75))
                .child(text)
        };
        let field = |state: &Entity<InputState>, toggle: bool| {
            let mut input = Input::new(state)
                .disabled(busy)
                .appearance(false)
                .border_1()
                .h(px(if narrow { 44. } else { 39. }))
                .px(px(14.))
                .rounded(px(11.))
                .bg(field_bg)
                .border_color(primary.opacity(0.1))
                .text_size(px(if narrow { 16. } else { 14. }));
            if toggle {
                input = input.mask_toggle();
            }
            input
        };
        let hint = |text: String| {
            div()
                .flex()
                .items_start()
                .gap(px(6.))
                .text_size(px(11.5))
                .line_height(relative(1.5))
                .text_color(secondary)
                .child(div().mt(px(2.)).child(icon("BackupCloud").size(px(13.))))
                .child(div().flex_1().child(text))
        };
        let solid_style = |fill: Hsla, cx: &mut Context<Self>| {
            ButtonCustomVariant::new(cx)
                .color(fill)
                .foreground(white())
                .hover(fill.opacity(0.9))
                .active(fill.opacity(0.8))
        };
        let connect_style = solid_style(brand, cx);
        let backup_style = solid_style(brand, cx);
        let overwrite_style = solid_style(error_color, cx);
        let outline = ButtonCustomVariant::new(cx)
            .color(transparent_black())
            .foreground(primary)
            .hover(primary.opacity(0.04))
            .active(primary.opacity(0.08));

        // ---- header
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .mb(px(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(42.))
                            .flex_shrink_0()
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
                            .text_color(brand)
                            .child(icon("BackupCloud").size(px(22.))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .line_height(relative(1.2))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.t("backup.backupAndRestore")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .child(div().size(px(6.)).rounded_full().bg(
                                        if self.connected {
                                            success
                                        } else {
                                            color(colors, "--td-text-color-disabled")
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(primary.opacity(0.55))
                                            .child(self.status_text()),
                                    ),
                            ),
                    ),
            )
            .child(
                gpui_kit::base::Button::new("close-backup-dialog")
                    .accessibility_label(self.t("common.titleBar.close"))
                    .disabled(busy)
                    .size(px(30.))
                    .rounded(px(9.))
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
                            .text_color(error_color)
                    })
                    .child(icon("WindowClose").size(px(13.)))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            );

        // ---- tabs
        let tab =
            |id: &'static str, text: String, value: Tab, disabled: bool, cx: &mut Context<Self>| {
                let active = self.tab == value;
                gpui_kit::base::Button::new(id)
                    .disabled(disabled)
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(14.))
                    .py(px(7.))
                    .rounded(px(8.))
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if active { primary } else { secondary })
                    .when(active, |button| {
                        button
                            .bg(color(colors, "--td-bg-color-container"))
                            .shadow(vec![BoxShadow {
                                color: black().opacity(0.08),
                                offset: point(px(0.), px(1.)),
                                blur_radius: px(3.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                    })
                    .when(disabled, |button| button.opacity(0.5))
                    .child(text)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = value;
                        cx.notify();
                    }))
            };
        let tabs = div()
            .flex()
            .gap(px(3.))
            .p(px(3.))
            .mb(px(18.))
            .rounded(px(11.))
            .border_1()
            .border_color(primary.opacity(0.05))
            .bg(primary.opacity(0.05))
            .child(tab(
                "backup-tab-recommend",
                self.t("backup.tabRecommend"),
                Tab::Recommend,
                false,
                cx,
            ))
            .child(tab(
                "backup-tab-config",
                self.t("backup.tabConfig"),
                Tab::Config,
                false,
                cx,
            ))
            .child(tab(
                "backup-tab-ops",
                self.t("backup.tabOps"),
                Tab::Ops,
                !self.connected,
                cx,
            ));

        let error_box = self.error.as_ref().map(|message| {
            div()
                .mt(px(12.))
                .flex()
                .items_start()
                .gap(px(8.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(10.))
                .border_1()
                .border_color(error_color.opacity(0.25))
                .bg(error_color.opacity(0.08))
                .text_size(px(12.5))
                .line_height(relative(1.45))
                .text_color(error_color)
                .child(div().flex_1().child(message.clone()))
        });

        // ---- recommend tab
        let provider = |id: &'static str,
                        title: String,
                        url: String,
                        tags: Vec<String>,
                        desc: String,
                        link: &'static str| {
            gpui_kit::base::Button::new(id)
                .w_full()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(10.))
                .p(px(14.))
                .rounded(px(14.))
                .border_1()
                .border_color(primary.opacity(0.08))
                .bg(primary.opacity(0.03))
                .hover(move |card| card.bg(primary.opacity(0.06)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(6.))
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(success)
                                .bg(success.opacity(0.12))
                                .child(self.t("backup.free")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(title),
                                )
                                .child(div().text_size(px(11.)).text_color(secondary).child(url)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(tags.into_iter().map(|tag| {
                            div()
                                .px(px(8.))
                                .py(px(3.))
                                .rounded(px(6.))
                                .bg(primary.opacity(0.05))
                                .text_size(px(11.))
                                .text_color(secondary)
                                .child(tag)
                        })),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(relative(1.5))
                        .text_color(secondary)
                        .text_left()
                        .child(desc),
                )
                .on_click(move |_, _, cx| cx.open_url(link))
        };
        let recommend = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(relative(1.5))
                    .text_color(secondary)
                    .child(self.t("backup.recommendIntro")),
            )
            .child(provider(
                "backup-provider-hi168",
                self.t("backup.hi168.title"),
                self.t("backup.hi168.url"),
                vec![
                    format!(
                        "500 GB {}",
                        self.t("backup.hi168.storage").replace("500 GB ", "")
                    ),
                    format!(
                        "10 GB {}",
                        self.t("backup.hi168.traffic").replace("10 GB ", "")
                    ),
                    self.t("backup.hi168.s3Compatible"),
                ],
                self.t("backup.hi168.desc"),
                HI168_URL,
            ))
            .child(provider(
                "backup-provider-cstcloud",
                self.t("backup.cstcloud.title"),
                self.t("backup.cstcloud.url"),
                vec![
                    format!(
                        "20 GB {}",
                        self.t("backup.cstcloud.storage").replace("20 GB ", "")
                    ),
                    self.t("backup.cstcloud.s3Compatible"),
                    self.t("backup.cstcloud.research"),
                ],
                self.t("backup.cstcloud.desc"),
                CSTCLOUD_URL,
            ))
            .child(hint(self.t("backup.recommendHint")));

        // ---- config tab
        let connect_label = match (self.busy, self.connected) {
            (Busy::Connecting, _) => self.t("backup.connecting"),
            (_, true) => self.t("backup.connectedRetest"),
            _ => self.t("backup.testConnection"),
        };
        let config_tab = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label(self.t("backup.endpoint")))
                    .child(field(&self.endpoint, false)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.region")))
                            .child(field(&self.region, false)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.bucket")))
                            .child(field(&self.bucket, false)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.accessKeyId")))
                            .child(field(&self.access_key, true)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.secretAccessKey")))
                            .child(field(&self.secret, true)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(label(
                                if self.english {
                                    "Path-style addressing"
                                } else {
                                    "路径风格（Path-style）"
                                }
                                .to_owned(),
                            ))
                            .child(div().text_size(px(11.5)).text_color(secondary).child(
                                if self.english {
                                    "On: host/bucket/key. Off: bucket.host/key."
                                } else {
                                    "开：host/bucket/key；关：bucket.host/key（虚拟主机风格）"
                                },
                            )),
                    )
                    .child(
                        Switch::new("backup-path-style")
                            .checked(self.path_style)
                            .disabled(busy)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.path_style = *checked;
                                this.connected = false;
                                this.persist(cx);
                                cx.notify();
                            })),
                    ),
            )
            .children(error_box.map(IntoElement::into_any_element))
            .child(
                Button::new("backup-connect")
                    .label(connect_label)
                    .loading(self.busy == Busy::Connecting)
                    .disabled(busy || !self.config_valid(cx))
                    .custom(connect_style)
                    .w_full()
                    .h(px(40.))
                    .rounded(px(11.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _, window, cx| this.test_connection(window, cx))),
            );

        // ---- operations tab
        let ops_card = |icon_name: &'static str, tint: Hsla, title: String, desc: String| {
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(
                    div()
                        .size(px(40.))
                        .flex_shrink_0()
                        .rounded(px(12.))
                        .bg(tint.opacity(0.14))
                        .text_color(tint)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(icon_name).size(px(20.))),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(div().text_size(px(12.)).text_color(secondary).child(desc)),
                )
        };
        let card = || {
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .p(px(14.))
                .rounded(px(14.))
                .border_1()
                .border_color(primary.opacity(0.08))
                .bg(primary.opacity(0.03))
        };
        let backup_label = if self.busy == Busy::BackingUp {
            self.t("backup.backingUp")
        } else {
            self.t("backup.encryptBackup")
        };
        let restore_label = if self.busy == Busy::Restoring {
            self.t("backup.restoring")
        } else {
            self.t("backup.mergeRestore")
        };
        let password_empty = self.password.read(cx).value().is_empty();
        let restore_empty = self.restore_password.read(cx).value().is_empty();
        let ops = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.backupPassword")))
                            .child(field(&self.password, true)),
                    )
                    .child(
                        div()
                            .w(px(120.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(label(self.t("backup.maxBackups")))
                            .child(field(&self.max_backups, false)),
                    ),
            )
            .child(
                card()
                    .child(ops_card(
                        "Cloud",
                        brand,
                        self.t("backup.backupToCloud"),
                        self.t("backup.backupToCloudDesc"),
                    ))
                    .children(self.last_backup.as_ref().map(|time| {
                        div()
                            .text_size(px(11.5))
                            .text_color(secondary)
                            .child(format!(
                                "{}{}",
                                self.t("backup.lastBackup"),
                                Self::format_time(time)
                            ))
                    }))
                    .child(
                        Button::new("backup-run")
                            .label(backup_label)
                            .loading(self.busy == Busy::BackingUp)
                            .disabled(busy || password_empty)
                            .custom(backup_style)
                            .w_full()
                            .h(px(40.))
                            .rounded(px(11.))
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.run_backup(window, cx)),
                            ),
                    ),
            )
            .child(
                card()
                    .child(ops_card(
                        "TD-Download",
                        success,
                        self.t("backup.restoreFromCloud"),
                        self.t("backup.restoreFromCloudDesc"),
                    ))
                    .child(field(&self.restore_password, true))
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .child(
                                Button::new("backup-merge")
                                    .label(restore_label)
                                    .loading(self.busy == Busy::Restoring)
                                    .disabled(busy || restore_empty)
                                    .custom(outline)
                                    .border_1()
                                    .border_color(primary.opacity(0.12))
                                    .flex_1()
                                    .h(px(40.))
                                    .rounded(px(11.))
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_restore(false, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("backup-overwrite")
                                    .label(self.t("backup.overwriteRestore"))
                                    .disabled(busy || restore_empty)
                                    .custom(overwrite_style)
                                    .flex_1()
                                    .h(px(40.))
                                    .rounded(px(11.))
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.run_restore(true, window, cx)
                                    })),
                            ),
                    ),
            )
            .children(error_box_ops(self.error.as_ref(), error_color))
            .child(hint(self.t("backup.opsHint")));

        div()
            .id("backup-dialog-content")
            .w_full()
            .max_h(window.viewport_size().height * 0.9)
            .overflow_y_scroll()
            .p(px(28.))
            .text_color(primary)
            .font_weight(FontWeight::NORMAL)
            .child(header)
            .child(tabs)
            .child(match self.tab {
                Tab::Recommend => recommend.into_any_element(),
                Tab::Config => config_tab.into_any_element(),
                Tab::Ops => ops.into_any_element(),
            })
    }
}

fn error_box_ops(message: Option<&String>, error_color: Hsla) -> Option<AnyElement> {
    message.map(|message| {
        div()
            .flex()
            .items_start()
            .px(px(12.))
            .py(px(10.))
            .rounded(px(10.))
            .border_1()
            .border_color(error_color.opacity(0.25))
            .bg(error_color.opacity(0.08))
            .text_size(px(12.5))
            .line_height(relative(1.45))
            .text_color(error_color)
            .child(div().flex_1().child(message.clone()))
            .into_any_element()
    })
}
