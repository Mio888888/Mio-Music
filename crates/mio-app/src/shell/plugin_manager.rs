//! Installed plugins, explicit selection, uninstall confirmation and selected plugin logs.
use super::*;
use gpui_kit::component::{
    WindowExt, button::ButtonVariants, notification::Notification, switch::Switch,
};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Cran,
    Lx,
}

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Local,
    Online,
}

struct AddPlugin {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    kind: Kind,
    method: Method,
    /// Installing a music-list plugin: no Cran/LX kind, and the import goes to the list plugins.
    list: bool,
}

/// One row of a list plugin's configuration form.
struct ConfigRow {
    field: crate::list_plugins::StoredField,
    input: Option<Entity<InputState>>,
    flag: bool,
    choice: String,
}

/// The configuration form a list plugin's author declared, with its current values.
struct ListPluginConfig {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    id: String,
    name: String,
    rows: Vec<ConfigRow>,
    testing: bool,
    connection: Option<bool>,
    connection_values: Vec<String>,
    can_test: bool,
    _subscriptions: Vec<Subscription>,
}

struct ConfirmUninstall {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    name: String,
    id: String,
}

struct PluginLog {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    name: String,
    id: String,
    lines: Vec<(String, String)>,
}

fn dialog_options(
    dialog: gpui_kit::component::dialog::Dialog,
    colors: crate::skin::Colors,
    window: &Window,
    max_width: f32,
) -> gpui_kit::component::dialog::Dialog {
    let narrow = window.viewport_size().width <= px(768.);
    let width = f32::from(window.viewport_size().width);
    let gutter = (width * 0.04).clamp(16., 24.);
    dialog
        .w(px((width - 2. * gutter).min(max_width)))
        .margin_top(window.viewport_size().height * 0.15)
        .rounded(px(if narrow { 22. } else { 6. }))
        .p(px(0.))
        .gap(px(0.))
        .bg(color(colors, "--td-bg-color-container"))
        .border_color(color(colors, "--td-border-level-1-color"))
        .overlay_tint(color(colors, "--td-mask-active"))
        .close_button(false)
        .on_ok(|_, _, _| false)
}

fn at_time(at_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

impl Mio {
    pub(super) fn open_add_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let content = cx.new(|_| AddPlugin {
            parent,
            colors,
            english: self.english,
            kind: Kind::Cran,
            method: Method::Local,
            list: false,
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog_options(dialog, colors, window, 520.).child(content.clone())
        });
    }

    /// The install dialog of music-list plugins: file or URL, no plugin kind to choose.
    fn open_add_list_plugin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let content = cx.new(|_| AddPlugin {
            parent,
            colors,
            english: self.english,
            kind: Kind::Cran,
            method: Method::Local,
            list: true,
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog_options(dialog, colors, window, 520.).child(content.clone())
        });
    }

    fn finish_list_install(
        &mut self,
        result: Result<crate::list_plugins::StoredListPlugin, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.online_busy = false;
        match result {
            Ok(plugin) => {
                window.push_notification(
                    Notification::success(
                        self.t("settings.plugin.installSuccess")
                            .replace("{name}", plugin.title()),
                    ),
                    cx,
                );
                self.online_error = None;
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(
                        self.t("settings.plugin.installFailed")
                            .replace("{msg}", &error),
                    ),
                    cx,
                );
            }
        }
        self.list_providers_changed(cx);
    }

    pub(super) fn import_list_plugin_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if self.english {
                    "Install list plugin"
                } else {
                    "安装音乐列表插件"
                }
                .into(),
            ),
        });
        self.online_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = match selection.await {
                Ok(Ok(Some(files))) => match files.into_iter().next() {
                    Some(path) => Some(
                        cx.background_executor()
                            .spawn(async move { crate::list_plugins::import_file(&store, &path) })
                            .await,
                    ),
                    None => None,
                },
                Ok(Ok(None)) => None,
                _ => Some(Err("无法选择插件文件".to_string())),
            };
            let _ = this.update_in(cx, |this, window, cx| match result {
                Some(result) => this.finish_list_install(result, window, cx),
                None => {
                    this.online_busy = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn import_list_plugin_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let url = self.plugin_url.read(cx).value().trim().to_string();
        if url.is_empty() {
            return;
        }
        self.plugin_url
            .update(cx, |field, cx| field.set_value("", window, cx));
        self.online_busy = true;
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { crate::list_plugins::import_url(&store, &url) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_list_install(result, window, cx)
            });
        })
        .detach();
    }

    pub(super) fn refresh_installed_plugins(&mut self) {
        if let Some(store) = &self.store {
            if let Ok(list) = crate::list_plugins::installed(store) {
                self.list_plugins = list;
            }
            match crate::online::Online::installed(store) {
                Ok(plugins) => self.installed_plugins = plugins,
                Err(error) => self.online_error = Some(error),
            }
        }
    }

    fn open_uninstall_confirm(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self
            .installed_plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
        else {
            return;
        };
        if self.online_busy || window.has_active_dialog(cx) {
            return;
        }
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let content = cx.new(|_| ConfirmUninstall {
            parent,
            colors,
            english: self.english,
            name,
            id,
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog_options(dialog, colors, window, 480.).child(content.clone())
        });
    }

    fn plugin_log_lines(&self, id: &str) -> Vec<(String, String)> {
        if let Some(online) = self.online.as_ref().filter(|o| o.id == id) {
            return online
                .plugin
                .logs()
                .into_iter()
                .map(|line| (at_time(line.at_ms), line.text))
                .collect();
        }
        self.installed_plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| {
                p.logs
                    .iter()
                    .map(|(time, text)| (at_time(*time), text.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn open_plugin_log(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self
            .installed_plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
        else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let lines = self.plugin_log_lines(&id);
        let content = cx.new(|_| PluginLog {
            parent,
            colors,
            english: self.english,
            name,
            id,
            lines,
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog_options(dialog, colors, window, 900.).child(content.clone())
        });
    }

    fn invalidate_online_search(&mut self) {
        if self.song_view.search_online {
            self.song_revision += 1;
        }
        if let Some(cancel) = &self.online_search_cancel {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.online_search_pending = None;
        if self.song_view.search_online {
            self.songs = Remote::Idle;
        }
        self.online_total = 0;
        self.song_view.search_loading_more = false;
        self.song_view.search_offset = 0;
        self.song_view.search_has_more = false;
        self.song_view.search_qualities.clear();
        self.song_view.search_page_error = None;
    }

    /// Invalidates views belonging to the old plugin without changing playback or its queue.
    fn active_plugin_changed(&mut self, cx: &mut Context<Self>) {
        self.invalidate_online_search();
        self.song_view.online_playlist.invalidate();
        self.song_view.singer.invalidate();
        self.online_discovery.invalidate();
        self.restart_playlist_search(cx);
        if self.player.song().is_some_and(crate::online::is_online) {
            self.lyrics_revision += 1;
            self.lyrics = if self.full_player_open {
                Remote::Loading
            } else {
                Remote::Idle
            };
            if self.full_player_open {
                self.load_lyrics(cx);
            }
        }
        if self.online.is_none()
            && let Some(manager) = &self.song_view.downloads.manager
        {
            manager.clear_online();
        }
        self.attach_downloads(cx);
        self.restore_online_session(cx);
        if self.song_view.search_online && self.online.is_some() {
            self.load_online_search(false, cx);
        }
        self.ensure_online_discovery(cx);
        cx.notify();
    }

    pub(super) fn refresh_online_plugin(&mut self, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        self.online_error = None;
        if self.online.is_some() {
            // Keep the active client's live logs when only refreshing the installed list.
            self.refresh_installed_plugins();
            cx.notify();
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        self.online_busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { crate::online::Online::restore(&store) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.online_busy = false;
                this.refresh_installed_plugins();
                match result {
                    Ok(Some(client)) => {
                        this.online = Some(client);
                        this.active_plugin_changed(cx);
                    }
                    Ok(None) => {}
                    Err(error) => this.online_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn activate_online_plugin(&mut self, id: String, cx: &mut Context<Self>) {
        if self.online_busy || self.online.as_ref().is_some_and(|o| o.id == id) {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        self.online_busy = true;
        self.online_error = None;
        let existing = self.online.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::online::Online::activate(store, &id, existing) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.online_busy = false;
                match result {
                    Ok(client) => {
                        this.online = Some(client);
                        this.refresh_installed_plugins();
                        this.active_plugin_changed(cx);
                    }
                    Err(error) => this.online_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn uninstall_online_plugin(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let Some(name) = self
            .installed_plugins
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
        else {
            return;
        };
        self.online_busy = true;
        let existing = self.online.clone();
        let selected_active = existing.as_ref().is_some_and(|o| o.id == id);
        let task = cx.background_executor().spawn(async move {
            crate::online::Online::uninstall_selected(&store, &id, existing.as_ref())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.online_busy = false;
                match result {
                    Ok(()) => {
                        this.online_error = None;
                        this.refresh_installed_plugins();
                        if selected_active {
                            this.online = None;
                            this.active_plugin_changed(cx);
                        }
                        window.push_notification(
                            Notification::success(
                                this.t("settings.plugin.uninstallSuccess")
                                    .replace("{name}", &name),
                            ),
                            cx,
                        );
                    }
                    Err(error) => window.push_notification(
                        Notification::error(
                            this.t("settings.plugin.uninstallFailed")
                                .replace("{msg}", &error),
                        ),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Lists, discovery and singer pages read the providers again after a list plugin changed.
    fn list_providers_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_installed_plugins();
        self.sync_virtual_online(cx);
        self.invalidate_online_search();
        self.invalidate_comments();
        if self.player.song().is_some_and(crate::online::is_online) {
            self.lyrics_revision = self.lyrics_revision.wrapping_add(1);
            self.lyrics = if self.full_player_open {
                Remote::Loading
            } else {
                Remote::Idle
            };
            if self.full_player_open {
                self.load_lyrics(cx);
            }
        }
        self.song_view.online_playlist.invalidate();
        self.song_view.singer.invalidate();
        self.online_discovery.invalidate();
        self.restart_playlist_search(cx);
        self.ensure_online_discovery(cx);
        cx.notify();
    }

    /// Runs one list-plugin store operation off the UI thread, then reports and refreshes.
    fn list_plugin_task(
        &mut self,
        done: String,
        window: &mut Window,
        cx: &mut Context<Self>,
        job: impl FnOnce(&Storage) -> Result<(), String> + Send + 'static,
    ) {
        if self.online_busy {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        self.online_busy = true;
        let task = cx.background_executor().spawn(async move { job(&store) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.online_busy = false;
                match result {
                    Ok(()) => {
                        window.push_notification(Notification::success(done), cx);
                        this.online_error = None;
                    }
                    Err(error) => {
                        window.push_notification(Notification::error(error.clone()), cx);
                        this.online_error = Some(error);
                    }
                }
                this.list_providers_changed(cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn open_list_plugin_config(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(plugin) = self.list_plugins.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let colors = self.ui_colors();
        let parent = cx.weak_entity();
        let english = self.english;
        let content = cx.new(|cx| {
            let rows: Vec<ConfigRow> = plugin
                .config_schema
                .iter()
                .map(|field| {
                    let current = plugin
                        .config
                        .get(&field.key)
                        .or(if field.default.is_null() {
                            None
                        } else {
                            Some(&field.default)
                        });
                    let text = match current {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(serde_json::Value::Number(n)) => n.to_string(),
                        _ => String::new(),
                    };
                    let flag = current
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let input = matches!(
                        field.kind.as_str(),
                        "text" | "password" | "number" | "textarea"
                    )
                    .then(|| {
                        let placeholder = field.placeholder.clone();
                        let masked = field.kind == "password";
                        cx.new(|cx| {
                            let mut state = InputState::new(window, cx).placeholder(placeholder);
                            if masked {
                                state = state.masked(true);
                            }
                            if !text.is_empty() {
                                state.set_value(&text, window, cx);
                            }
                            state
                        })
                    });
                    ConfigRow {
                        field: field.clone(),
                        input,
                        flag,
                        choice: text,
                    }
                })
                .collect();
            let subscriptions = rows
                .iter()
                .filter_map(|row| row.input.as_ref())
                .map(|input| {
                    cx.subscribe(
                        input,
                        |this: &mut ListPluginConfig, _, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Change) {
                                this.connection = None;
                                cx.notify();
                            }
                        },
                    )
                })
                .collect();
            ListPluginConfig {
                parent,
                colors,
                english,
                id: plugin.id.clone(),
                name: plugin.title().to_owned(),
                rows,
                testing: false,
                connection: None,
                connection_values: Vec::new(),
                _subscriptions: subscriptions,
                can_test: plugin
                    .sources
                    .iter()
                    .any(|s| s.capabilities.iter().any(|c| c == "testConnection")),
            }
        });
        window.open_dialog(cx, move |dialog, window, _| {
            dialog_options(dialog, colors, window, 560.).child(content.clone())
        });
    }

    fn save_list_plugin_config(
        &mut self,
        id: String,
        values: serde_json::Map<String, serde_json::Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let done = if self.english {
            "Saved"
        } else {
            "已保存配置"
        }
        .to_string();
        self.list_plugin_task(done, window, cx, move |store| {
            crate::list_plugins::set_config(store, &id, values)
        });
    }

    fn clear_list_plugin_storage(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let done = if self.english {
            "Cleared"
        } else {
            "已清除登录状态与缓存"
        }
        .to_string();
        self.list_plugin_task(done, window, cx, move |store| {
            crate::list_plugins::clear_storage(store, &id)
        });
    }

    fn set_list_plugin_provide(
        &mut self,
        id: String,
        play: Option<bool>,
        download: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let done = if self.english { "Updated" } else { "已更新" }.to_string();
        self.list_plugin_task(done, window, cx, move |store| {
            crate::list_plugins::set_provide(store, &id, play, download)
        });
    }

    /// Keeps the playback client in step with the list plugins: with no playback plugin active,
    /// a plugin that provides playback itself needs a (script-less) client; without one it goes.
    fn sync_virtual_online(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let provides = !crate::list_plugins::play_sources().is_empty();
        match (&self.online, provides) {
            (None, true) => {
                if let Ok(client) = crate::online::Online::virtual_client(store, None) {
                    self.online = Some(client);
                    self.active_plugin_changed(cx);
                }
            }
            (Some(online), false) if online.is_virtual() => {
                self.online = None;
                self.active_plugin_changed(cx);
            }
            _ => {}
        }
    }

    fn toggle_list_plugin(
        &mut self,
        id: String,
        enable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let done = match (enable, self.english) {
            (true, true) => "Enabled",
            (true, false) => "已启用",
            (false, true) => "Disabled",
            (false, false) => "已停用",
        }
        .to_string();
        self.list_plugin_task(done, window, cx, move |store| {
            crate::list_plugins::set_enabled(store, &id, enable)
        });
    }

    fn uninstall_list_plugin(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let done = if self.english {
            "Uninstalled"
        } else {
            "已卸载"
        }
        .to_string();
        self.list_plugin_task(done, window, cx, move |store| {
            crate::list_plugins::uninstall(store, &id)
        });
    }

    /// Installed external music-list plugins, with enable/uninstall controls.
    fn list_plugin_group(&mut self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let en = self.english;
        let busy = self.online_busy;
        let tr = move |zh: &str, en_text: &str| if en { en_text } else { zh }.to_string();
        let tag = |text: String, fill: Hsla, text_color: Hsla| {
            div()
                .px(px(8.))
                .py(px(1.))
                .rounded(px(10.))
                .text_size(px(12.))
                .bg(fill)
                .text_color(text_color)
                .child(text)
        };
        let mut group = self
            .setting_group("list-plugin-settings", None, narrow)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(8.))
                    .mb(px(8.))
                    .child(
                        div()
                            .text_size(px(if narrow { 16. } else { 18. }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("音乐列表插件", "Music list plugins")),
                    )
                    .child(
                        Button::new("list-plugin-add")
                            .label(tr("安装列表插件", "Install list plugin"))
                            .primary()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_add_list_plugin(window, cx)
                            })),
                    ),
            );
        for plugin in self.list_plugins.clone() {
            let enabled = plugin.enabled;
            // Bundled plugins are always installed; they are switched off, not removed.
            let bundled = plugin.bundled();
            // Without its required fields (a server address) a plugin offers no sources yet.
            let waiting = enabled && !plugin.configured();
            let toggle_id = plugin.id.clone();
            let remove_id = plugin.id.clone();
            let sources: Vec<String> = plugin.sources.iter().map(|s| s.name.clone()).collect();
            let (state_fill, state_text) = if enabled && !waiting {
                (
                    color(mode, "--td-brand-color"),
                    color(mode, "--td-text-color-anti"),
                )
            } else {
                (color(mode, "--td-bg-color-component"), primary)
            };
            group = group.child(
                div()
                    .p(px(16.))
                    .mb(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(color(mode, "--td-border-level-1-color"))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(plugin.title().to_owned()),
                            )
                            .when(!plugin.version.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(secondary)
                                        .child(format!("v{}", plugin.version)),
                                )
                            })
                            .when(bundled, |row| {
                                row.child(tag(
                                    tr("内置", "Built-in"),
                                    color(mode, "--td-bg-color-component"),
                                    secondary,
                                ))
                            })
                            .child(tag(
                                if waiting {
                                    tr("待配置", "Needs setup")
                                } else if enabled {
                                    tr("已启用", "Enabled")
                                } else {
                                    tr("已停用", "Disabled")
                                },
                                state_fill,
                                state_text,
                            )),
                    )
                    .when(!plugin.description.is_empty(), |card| {
                        card.child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(20.))
                                .text_color(secondary)
                                .child(plugin.description.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(sources.into_iter().map(|source| {
                                tag(source, color(mode, "--td-bg-color-component"), primary)
                            })),
                    )
                    .when(plugin.can_play || plugin.can_download, |card| {
                        let mut row = div().flex().flex_wrap().gap(px(16.)).items_center();
                        if plugin.can_play {
                            let id = plugin.id.clone();
                            row = row.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        Switch::new(SharedString::from(format!(
                                            "list-plugin-play-{}",
                                            plugin.id
                                        )))
                                        .checked(plugin.provide_play)
                                        .disabled(busy)
                                        .on_click(
                                            cx.listener(move |this, checked: &bool, window, cx| {
                                                this.set_list_plugin_provide(
                                                    id.clone(),
                                                    Some(*checked),
                                                    None,
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(primary)
                                            .child(tr("由本插件提供播放", "Provide playback")),
                                    ),
                            );
                        }
                        if plugin.can_download {
                            let id = plugin.id.clone();
                            row = row.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        Switch::new(SharedString::from(format!(
                                            "list-plugin-download-{}",
                                            plugin.id
                                        )))
                                        .checked(plugin.provide_download)
                                        .disabled(busy)
                                        .on_click(
                                            cx.listener(move |this, checked: &bool, window, cx| {
                                                this.set_list_plugin_provide(
                                                    id.clone(),
                                                    None,
                                                    Some(*checked),
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(primary)
                                            .child(tr("由本插件提供下载", "Provide downloads")),
                                    ),
                            );
                        }
                        card.child(row)
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .pt(px(4.))
                            .when(!plugin.config_schema.is_empty(), |row| {
                                let id = plugin.id.clone();
                                row.child(
                                    Button::new(SharedString::from(format!(
                                        "list-plugin-config-{}",
                                        plugin.id
                                    )))
                                    .label(tr("配置", "Configure"))
                                    .primary()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_list_plugin_config(id.clone(), window, cx)
                                    })),
                                )
                            })
                            .when(
                                plugin
                                    .sources
                                    .iter()
                                    .any(|s| s.capabilities.iter().any(|c| c == "userPlaylists")),
                                |row| {
                                    row.child(
                                        Button::new(SharedString::from(format!(
                                            "list-plugin-import-{}",
                                            plugin.id
                                        )))
                                        .label(tr("导入歌单", "Import playlists"))
                                        .outline()
                                        .disabled(busy || !enabled)
                                        .on_click(
                                            cx.listener({
                                                let id = plugin.id.clone();
                                                move |this, _, window, cx| {
                                                    this.open_plugin_playlists(
                                                        id.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                }
                                            }),
                                        ),
                                    )
                                },
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "list-plugin-toggle-{}",
                                    plugin.id
                                )))
                                .label(if enabled {
                                    tr("停用", "Disable")
                                } else {
                                    tr("启用", "Enable")
                                })
                                .outline()
                                .disabled(busy)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.toggle_list_plugin(
                                            toggle_id.clone(),
                                            !enabled,
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            )
                            .when(!bundled, |row| {
                                row.child(
                                    Button::new(SharedString::from(format!(
                                        "list-plugin-remove-{}",
                                        plugin.id
                                    )))
                                    .label(tr("卸载", "Uninstall"))
                                    .outline()
                                    .danger()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.uninstall_list_plugin(remove_id.clone(), window, cx)
                                    })),
                                )
                            }),
                    ),
            );
        }
        if self.list_plugins.is_empty() {
            // The same empty state as the playback plugins above.
            group = group.child(
                div()
                    .py(px(32.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(tr("暂无列表插件", "No list plugins")),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(secondary)
                            .text_center()
                            .child(tr(
                                "点击右上角“安装列表插件”，从文件或 URL 安装",
                                "Use \"Install list plugin\" to install from a file or URL",
                            )),
                    ),
            );
        }
        group.into_any_element()
    }

    pub(super) fn plugin_groups(
        &mut self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let busy = self.online_busy;
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(8.))
            .mb(px(16.))
            .child(
                div()
                    .text_size(px(if narrow { 16. } else { 18. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t("settings.plugin.title")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("plugin-add")
                            .label(self.t("settings.plugin.addPlugin"))
                            .primary()
                            .disabled(busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_add_plugin(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("plugin-refresh")
                            .label(self.t("common.refresh"))
                            .outline()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_online_plugin(cx))),
                    ),
            );
        let mut group = self
            .setting_group("plugin-settings", None, narrow)
            .child(header);

        let state_block = |title: String, hint: Option<String>, tone: Hsla| {
            div()
                .py(px(32.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone)
                        .child(title),
                )
                .children(hint.map(|hint| {
                    div()
                        .text_size(px(13.))
                        .text_color(secondary)
                        .text_center()
                        .child(hint)
                }))
        };

        if busy && self.installed_plugins.is_empty() {
            group = group.child(state_block(
                self.t("settings.plugin.loading"),
                None,
                secondary,
            ));
        } else if let (true, Some(error)) = (self.installed_plugins.is_empty(), &self.online_error)
        {
            group = group.child(
                state_block(
                    self.t("settings.plugin.loadError"),
                    Some(error.clone()),
                    color(mode, "--td-error-color"),
                )
                .child(
                    Button::new("plugin-retry")
                        .label(self.t("common.retry"))
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| this.refresh_online_plugin(cx))),
                ),
            );
        } else if !self.installed_plugins.is_empty() {
            for plugin in self.installed_plugins.clone() {
                let active = self.online.as_ref().is_some_and(|o| o.id == plugin.id);
                let log_id = plugin.id.clone();
                let uninstall_id = plugin.id.clone();
                let activate_id = plugin.id.clone();
                let sources: Vec<String> = plugin
                    .sources
                    .iter()
                    .map(|s| s.name.clone())
                    .filter(|n| !n.is_empty())
                    .collect();
                let description = if plugin.description.is_empty() {
                    self.t("settings.plugin.noDescription")
                } else {
                    plugin.description.clone()
                };
                let tag = |text: String, fill: Hsla, text_color: Hsla| {
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .rounded(px(10.))
                        .text_size(px(12.))
                        .bg(fill)
                        .text_color(text_color)
                        .child(text)
                };
                group = group.child(
                    div()
                        .p(px(16.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(color(mode, "--td-brand-color-3"))
                        .bg(color(mode, "--td-brand-color-1"))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_wrap()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(16.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(plugin.name.clone()),
                                )
                                .when(!plugin.version.is_empty(), |row| {
                                    row.child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(secondary)
                                            .child(format!("v{}", plugin.version)),
                                    )
                                })
                                .when(active, |row| {
                                    row.child(tag(
                                        self.t("settings.plugin.currentUsing"),
                                        brand,
                                        color(mode, "--td-text-color-anti"),
                                    ))
                                }),
                        )
                        .when(!plugin.author.is_empty(), |card| {
                            card.child(
                                div().text_size(px(13.)).text_color(secondary).child(
                                    self.t("settings.plugin.author")
                                        .replace("{name}", &plugin.author),
                                ),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(20.))
                                .text_color(secondary)
                                .child(description),
                        )
                        .when(!sources.is_empty(), |card| {
                            card.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(secondary)
                                            .child(self.t("settings.plugin.supportedSources")),
                                    )
                                    .children(sources.into_iter().map(|source| {
                                        tag(source, color(mode, "--td-bg-color-component"), primary)
                                    })),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .pt(px(4.))
                                .when(!active, |row| {
                                    row.child(
                                        Button::new(SharedString::from(format!(
                                            "plugin-use-{}",
                                            plugin.id
                                        )))
                                        .label(self.t("settings.plugin.use"))
                                        .primary()
                                        .disabled(busy)
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.activate_online_plugin(activate_id.clone(), cx)
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "plugin-logs-{}",
                                        plugin.id
                                    )))
                                    .label(self.t("settings.plugin.logs"))
                                    .outline()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_plugin_log(log_id.clone(), window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "plugin-uninstall-{}",
                                        plugin.id
                                    )))
                                    .label(self.t("settings.plugin.uninstall"))
                                    .outline()
                                    .danger()
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_uninstall_confirm(
                                            uninstall_id.clone(),
                                            window,
                                            cx,
                                        )
                                    })),
                                ),
                        ),
                );
            }
        } else {
            group = group.child(state_block(
                self.t("settings.plugin.noPlugins"),
                Some(self.t("settings.plugin.noPluginsHint")),
                primary,
            ));
        }
        if !self.installed_plugins.is_empty()
            && let Some(error) = &self.online_error
        {
            group = group.child(
                div()
                    .mt(px(12.))
                    .text_color(color(mode, "--td-error-color"))
                    .child(error.clone()),
            );
        }
        vec![group.into_any_element(), self.list_plugin_group(narrow, cx)]
    }
}

fn radio(
    id: &'static str,
    label: String,
    selected: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    let button = Button::new(id)
        .label(label)
        .on_click(move |_, window, cx| on_click(window, cx));
    if selected {
        button.primary()
    } else {
        button.outline()
    }
}

impl AddPlugin {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let method = self.method;
        let Some(parent) = self.parent.upgrade() else {
            return;
        };
        match method {
            Method::Local => {
                window.close_dialog(cx);
                let list = self.list;
                parent.update(cx, |parent, cx| {
                    if list {
                        parent.import_list_plugin_file(window, cx)
                    } else {
                        parent.import_online_plugin(window, cx)
                    }
                });
            }
            Method::Online => {
                let url = parent
                    .read(cx)
                    .plugin_url
                    .read(cx)
                    .value()
                    .trim()
                    .to_owned();
                if url.is_empty() {
                    window.push_notification(
                        Notification::warning(self.t("settings.plugin.enterUrl")),
                        cx,
                    );
                    return;
                }
                if reqwest::Url::parse(&url).is_err() {
                    window.push_notification(
                        Notification::warning(self.t("settings.plugin.invalidUrl")),
                        cx,
                    );
                    return;
                }
                window.close_dialog(cx);
                let list = self.list;
                parent.update(cx, |parent, cx| {
                    if list {
                        parent.import_list_plugin_url(window, cx)
                    } else {
                        parent.import_online_url(window, cx)
                    }
                });
            }
        }
    }
}

impl Render for AddPlugin {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let this = cx.entity();
        let set = |apply: fn(&mut AddPlugin, bool), flag: bool| {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                this.update(cx, |dialog, cx| {
                    apply(dialog, flag);
                    cx.notify();
                });
            }
        };
        let url_input = self
            .parent
            .upgrade()
            .map(|parent| parent.read(cx).plugin_url.clone());
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if self.list {
                        if self.english {
                            "Install music list plugin".to_string()
                        } else {
                            "安装音乐列表插件".to_string()
                        }
                    } else {
                        self.t("settings.plugin.addPlugin")
                    }),
            )
            .when(!self.list, |dialog| {
                dialog.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(secondary)
                                .child(self.t("settings.plugin.pluginTypeTip")),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(radio(
                                    "plugin-kind-cran",
                                    self.t("settings.plugin.cranPlugin"),
                                    self.kind == Kind::Cran,
                                    set(|d, _| d.kind = Kind::Cran, true),
                                ))
                                .child(radio(
                                    "plugin-kind-lx",
                                    self.t("settings.plugin.lxPlugin"),
                                    self.kind == Kind::Lx,
                                    set(|d, _| d.kind = Kind::Lx, true),
                                )),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(secondary)
                            .child(self.t("settings.plugin.selectImportMethod")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(radio(
                                "plugin-method-local",
                                self.t("settings.plugin.localImport"),
                                self.method == Method::Local,
                                set(|d, _| d.method = Method::Local, true),
                            ))
                            .child(radio(
                                "plugin-method-online",
                                self.t("settings.plugin.onlineImport"),
                                self.method == Method::Online,
                                set(|d, _| d.method = Method::Online, true),
                            )),
                    )
                    .child(match (self.method, url_input) {
                        (Method::Online, Some(input)) => div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(Input::new(&input).h(px(40.)).mask_toggle())
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(self.t("settings.plugin.onlineUrlTip")),
                            )
                            .into_any_element(),
                        _ => div()
                            .text_size(px(12.))
                            .text_color(secondary)
                            .child(self.t("settings.plugin.localImportTip"))
                            .into_any_element(),
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("plugin-add-cancel")
                            .label(self.t("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("plugin-add-confirm")
                            .label(self.t("common.confirm"))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            )
    }
}

impl ListPluginConfig {
    fn t(&self, zh: &str, en: &str) -> String {
        if self.english { en } else { zh }.to_string()
    }

    fn fingerprint(&self, cx: &Context<Self>) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| match row.field.kind.as_str() {
                "switch" => row.flag.to_string(),
                "select" => row.choice.clone(),
                _ => row
                    .input
                    .as_ref()
                    .map(|input| input.read(cx).value().trim().to_string())
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn values(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut values = serde_json::Map::new();
        for index in 0..self.rows.len() {
            let row = &self.rows[index];
            let key = row.field.key.clone();
            let label = row.field.label.clone();
            let required = row.field.required;
            let text = row
                .input
                .as_ref()
                .map(|input| input.read(cx).value().trim().to_string())
                .unwrap_or_default();
            let warn =
                |this: &Self, what: &str, en: &str, window: &mut Window, cx: &mut Context<Self>| {
                    window.push_notification(
                        Notification::warning(format!("{}：{label}", this.t(what, en))),
                        cx,
                    );
                };
            match row.field.kind.as_str() {
                "switch" => {
                    values.insert(key, serde_json::Value::Bool(row.flag));
                }
                "select" => {
                    if required && row.choice.is_empty() {
                        warn(self, "必填项未填写", "Required", window, cx);
                        return None;
                    }
                    values.insert(key, serde_json::Value::String(row.choice.clone()));
                }
                "number" => {
                    if text.is_empty() {
                        if required {
                            warn(self, "必填项未填写", "Required", window, cx);
                            return None;
                        }
                        continue;
                    }
                    let Some(number) = text.parse::<f64>().ok().filter(|n| n.is_finite()) else {
                        warn(self, "数字格式无效", "Invalid number", window, cx);
                        return None;
                    };
                    let value = if number.fract() == 0.0 && number.abs() < 9e15 {
                        serde_json::Value::from(number as i64)
                    } else {
                        serde_json::Value::from(number)
                    };
                    values.insert(key, value);
                }
                _ => {
                    if required && text.is_empty() {
                        warn(self, "必填项未填写", "Required", window, cx);
                        return None;
                    }
                    values.insert(key, serde_json::Value::String(text));
                }
            }
        }
        Some(values)
    }

    fn test_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        let Some(values) = self.values(window, cx) else {
            return;
        };
        let Some(store) = self
            .parent
            .upgrade()
            .and_then(|parent| parent.read(cx).store.clone())
        else {
            return;
        };
        self.testing = true;
        self.connection = None;
        self.connection_values = self.fingerprint(cx);
        let id = self.id.clone();
        let task = cx.background_executor().spawn(async move {
            crate::list_plugins::service_plugin(&store, &id, Some(values))?.test_connection()
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.testing = false;
                this.connection = Some(result.is_ok());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        let Some(values) = self.values(window, cx) else {
            return;
        };
        window.close_dialog(cx);
        let id = self.id.clone();
        let _ = self.parent.update(cx, |parent, cx| {
            parent.save_list_plugin_config(id, values, window, cx)
        });
    }
}

impl Render for ListPluginConfig {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let this = cx.entity();
        let mut form = div().flex().flex_col().gap(px(16.));
        for (index, row) in self.rows.iter().enumerate() {
            let label = if row.field.required {
                format!("{} *", row.field.label)
            } else {
                row.field.label.clone()
            };
            let control: AnyElement = match row.field.kind.as_str() {
                "switch" => {
                    let this = this.clone();
                    Switch::new(SharedString::from(format!("list-config-switch-{index}")))
                        .checked(row.flag)
                        .on_click(move |checked: &bool, _, cx| {
                            let checked = *checked;
                            this.update(cx, |dialog, cx| {
                                dialog.rows[index].flag = checked;
                                dialog.connection = None;
                                cx.notify();
                            });
                        })
                        .into_any_element()
                }
                "select" => div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(row.field.options.iter().map(|(option_label, value)| {
                        let selected = row.choice == *value;
                        let this = this.clone();
                        let value = value.clone();
                        let button =
                            Button::new(SharedString::from(format!("list-config-{index}-{value}")))
                                .label(option_label.clone())
                                .on_click(move |_, _, cx| {
                                    let value = value.clone();
                                    this.update(cx, |dialog, cx| {
                                        dialog.rows[index].choice = value;
                                        dialog.connection = None;
                                        cx.notify();
                                    });
                                });
                        if selected {
                            button.primary()
                        } else {
                            button.outline()
                        }
                    }))
                    .into_any_element(),
                _ => match &row.input {
                    Some(input) => Input::new(input)
                        .h(px(40.))
                        .mask_toggle()
                        .into_any_element(),
                    None => div().into_any_element(),
                },
            };
            form = form.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(control)
                    .when(!row.field.description.is_empty(), |field| {
                        field.child(
                            div()
                                .text_size(px(12.))
                                .text_color(secondary)
                                .child(row.field.description.clone()),
                        )
                    }),
            );
        }
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!(
                        "{} · {}",
                        self.name,
                        self.t("配置", "Configuration")
                    )),
            )
            .child(form)
            .when(self.can_test, |root| {
                root.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            Button::new("list-config-test")
                                .label(self.t(
                                    if self.testing {
                                        "验证中…"
                                    } else {
                                        "测试连接"
                                    },
                                    if self.testing {
                                        "Checking…"
                                    } else {
                                        "Test connection"
                                    },
                                ))
                                .outline()
                                .disabled(self.testing)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.test_connection(window, cx)
                                })),
                        )
                        .when_some(
                            self.connection
                                .filter(|_| self.connection_values == self.fingerprint(cx)),
                            |row, success| {
                                row.child(self.t(
                                    if success {
                                        "连接成功"
                                    } else {
                                        "连接失败或插件未提供验证方法"
                                    },
                                    if success {
                                        "Connected"
                                    } else {
                                        "Connection failed or method unavailable"
                                    },
                                ))
                            },
                        )
                        .child(div().text_size(px(12.)).text_color(secondary).child(self.t(
                            "验证当前填写内容；保存后才应用",
                            "Checks current values; Save to apply",
                        ))),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        Button::new("list-config-clear")
                            .disabled(self.testing)
                            .label(self.t("清除登录状态与缓存", "Clear login state & cache"))
                            .outline()
                            .danger()
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.close_dialog(cx);
                                let id = this.id.clone();
                                let _ = this.parent.update(cx, |parent, cx| {
                                    parent.clear_list_plugin_storage(id, window, cx)
                                });
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                Button::new("list-config-cancel")
                                    .label(self.t("取消", "Cancel"))
                                    .outline()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new("list-config-save")
                                    .disabled(self.testing)
                                    .label(self.t("保存", "Save"))
                                    .primary()
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.save(window, cx)),
                                    ),
                            ),
                    ),
            )
    }
}

impl Render for ConfirmUninstall {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let text = |key: &str| i18n::text(self.english, key);
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(text("settings.plugin.confirmUninstallTitle")),
            )
            .child(
                div()
                    .mt(px(16.))
                    .line_height(px(22.))
                    .text_color(secondary)
                    .child(
                        text("settings.plugin.confirmUninstallBody").replace("{name}", &self.name),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(24.))
                    .child(
                        Button::new("plugin-uninstall-cancel")
                            .label(text("common.cancel"))
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("plugin-uninstall-confirm")
                            .label(text("settings.plugin.confirmUninstallBtn"))
                            .primary()
                            .danger()
                            .on_click(cx.listener(|this, _, window, cx| {
                                window.close_dialog(cx);
                                let _ = this.parent.update(cx, |parent, cx| {
                                    parent.uninstall_online_plugin(this.id.clone(), window, cx)
                                });
                            })),
                    ),
            )
    }
}

impl Render for PluginLog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let text = |key: &str| i18n::text(self.english, key);
        let console_bg = rgb(0x1e1e1e);
        let line_color = |line: &str| {
            if line.starts_with("[error]") {
                rgb(0xff6b6b)
            } else if line.starts_with("[warn]") {
                rgb(0xffc857)
            } else {
                rgb(0xd4d4d4)
            }
        };
        let empty = self.lines.is_empty();
        div()
            .p(px(24.))
            .text_color(primary)
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when(narrow, |header| {
                        header.flex_col().items_start().gap(px(12.))
                    })
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "{} · {}",
                                text("settings.plugin.pluginLog"),
                                self.name
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                Button::new("plugin-log-refresh")
                                    .label(text("common.refresh"))
                                    .outline()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let lines = this
                                            .parent
                                            .upgrade()
                                            .map(|parent| {
                                                parent.read(cx).plugin_log_lines(&this.id)
                                            })
                                            .unwrap_or_default();
                                        this.lines = lines;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("plugin-log-close")
                                    .label(text("common.close"))
                                    .outline()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("plugin-log-console")
                    .h(window.viewport_size().height * 0.5)
                    .overflow_y_scroll()
                    .p(px(14.))
                    .rounded(px(8.))
                    .bg(console_bg)
                    .font_family("Menlo")
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .when(empty, |console| {
                        console.child(
                            div()
                                .text_color(rgb(0x8a8a8a))
                                .child(text("settings.plugin.noLogs")),
                        )
                    })
                    .children(self.lines.iter().map(|(time, line)| {
                        div()
                            .flex()
                            .gap(px(10.))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(rgb(0x6a6a6a))
                                    .child(time.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(line_color(line))
                                    .child(line.clone()),
                            )
                    })),
            )
    }
}
