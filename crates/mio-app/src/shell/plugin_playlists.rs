//! Import the selected service plugin's own playlists, never a registry fallback.
use super::*;
use crate::{library, list_plugins};
use gpui_kit::component::{WindowExt, button::ButtonVariants, notification::Notification};
use mio_music_sdk::PlaylistItem;
use mio_plugin::ListPlugin;

struct PluginPlaylists {
    parent: WeakEntity<Mio>,
    store: Storage,
    id: String,
    name: String,
    colors: crate::skin::Colors,
    english: bool,
    plugin: Option<Arc<ListPlugin>>,
    playlists: Vec<PlaylistItem>,
    busy: bool,
    importing: bool,
    failed: bool,
    message: String,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for PluginPlaylists {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Mio {
    pub(super) fn open_plugin_playlists(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(store) = self.store.clone() else {
            return;
        };
        let Some(plugin) = self.list_plugins.iter().find(|p| p.id == id && p.enabled) else {
            return;
        };
        let name = plugin.name.clone();
        let colors = self.ui_colors();
        let english = self.english;
        let parent = cx.weak_entity();
        let content = cx.new(|_| PluginPlaylists {
            parent,
            store,
            id,
            name,
            colors,
            english,
            plugin: None,
            playlists: Vec::new(),
            busy: false,
            importing: false,
            failed: false,
            message: String::new(),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        content.update(cx, |this, cx| this.load(window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let width = f32::from(window.viewport_size().width);
            let guard = content.clone();
            dialog
                .w(px((width - 32.).min(600.)))
                .margin_top(window.viewport_size().height * 0.1)
                .p(px(0.))
                .gap(px(0.))
                .rounded(px(22.))
                .bg(color(colors, "--td-bg-color-container"))
                .close_button(false)
                .on_ok(|_, _, _| false)
                .on_cancel(move |_, _, cx| !guard.read(cx).importing)
                .child(content.clone())
        });
    }
}

impl PluginPlaylists {
    fn t(&self, zh: &str, en: &str) -> String {
        if self.english { en } else { zh }.to_string()
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.failed = false;
        self.message = self.t("正在读取服务器歌单…", "Loading server playlists…");
        let store = self.store.clone();
        let id = self.id.clone();
        let cancelled = self.cancelled.clone();
        let task = cx.background_executor().spawn(async move {
            let deadline = std::time::Instant::now() + Duration::from_secs(120);
            let plugin = list_plugins::service_plugin(&store, &id, None)?;
            let mut playlists = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for source in plugin
                .sources
                .iter()
                .filter(|s| s.capabilities.iter().any(|c| c == "userPlaylists"))
                // The old getPlaylists() has no source argument and returns one shared list.
                .take(if plugin.legacy_service { 1 } else { usize::MAX })
            {
                if !source.capabilities.iter().any(|c| c == "playlistDetail") {
                    return Err("插件未提供歌单详情".to_string());
                }
                let mut received = 0;
                let mut expected = None;
                for page in 1..=100 {
                    if cancelled.load(std::sync::atomic::Ordering::Relaxed)
                        || std::time::Instant::now() >= deadline
                    {
                        return Err("读取已取消或超时".into());
                    }
                    let result = plugin.user_playlists(&source.id, page)?;
                    let total = usize::try_from(result.total).map_err(|_| "歌单总数无效")?;
                    if expected.is_some_and(|old| old != total) {
                        return Err("服务器歌单已变化，请重试".into());
                    }
                    expected = Some(total);
                    let count = result.list.len();
                    for item in result.list {
                        let id = match &item.id {
                            serde_json::Value::String(id) if !id.is_empty() => id.clone(),
                            serde_json::Value::Number(id) => id.to_string(),
                            _ => return Err("歌单 ID 无效".into()),
                        };
                        if !seen.insert((source.id.clone(), id)) || item.name.trim().is_empty() {
                            return Err("插件返回重复或无效歌单".into());
                        }
                        playlists.push(item);
                        if playlists.len() > 10_000 {
                            return Err("服务器歌单数量超限".into());
                        }
                    }
                    received += count;
                    if received == total {
                        break;
                    }
                    if received > total || count == 0 || page == 100 {
                        return Err("歌单分页不完整或超限，请重试".into());
                    }
                }
            }
            Ok::<_, String>((plugin, playlists))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok((plugin, playlists)) => {
                        this.plugin = Some(plugin);
                        this.playlists = playlists;
                        this.message = if this.playlists.is_empty() {
                            this.t("未找到服务器歌单", "No server playlists found")
                        } else {
                            String::new()
                        };
                    }
                    Err(_) => {
                        // Plugin errors may contain request URLs/credentials.
                        this.failed = true;
                        this.message = this.t(
                            "读取歌单失败，请检查插件配置后重试",
                            "Could not load playlists; check configuration and retry",
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn import(&mut self, item: PlaylistItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(plugin) = self.plugin.clone() else {
            return;
        };
        self.busy = true;
        self.importing = true;
        self.message = self.t("正在导入…", "Importing…");
        let path = self.store.library_path();
        let plugin_name = self.name.clone();
        let english = self.english;
        let cancelled = self.cancelled.clone();
        let task = cx.background_executor().spawn(async move {
            let deadline = std::time::Instant::now() + Duration::from_secs(120);
            let id = match &item.id {
                serde_json::Value::String(id) => id.clone(),
                serde_json::Value::Number(id) => id.to_string(),
                _ => return Err("歌单 ID 无效".to_string()),
            };
            let provider = plugin
                .providers("import", false, false)
                .into_iter()
                .find(|p| p.id() == item.source)
                .ok_or("插件来源不存在")?;
            let mut songs = Vec::new();
            let mut expected = None;
            if plugin.legacy_service {
                songs = plugin.legacy_playlist_songs(&item.source, &id)?;
            } else {
                for page in 1..=400 {
                    if cancelled.load(std::sync::atomic::Ordering::Relaxed)
                        || std::time::Instant::now() >= deadline
                    {
                        return Err("读取已取消或超时".into());
                    }
                    let detail = provider.playlist_detail(&id, page, 100)?;
                    let total = usize::try_from(detail.total).map_err(|_| "歌曲总数无效")?;
                    if expected.is_some_and(|old| old != total) {
                        return Err("服务器歌单已变化，请重试".into());
                    }
                    expected = Some(total);
                    let count = detail.list.len();
                    if total > 40_000 {
                        return Err("歌单歌曲数量超限".into());
                    }
                    for song in detail.list {
                        songs.push(serde_json::to_value(song).map_err(|_| "歌曲数据无效")?);
                    }
                    if songs.len() == total {
                        break;
                    }
                    if songs.len() > total || count == 0 || page == 400 {
                        return Err("歌曲分页不完整或超限".into());
                    }
                }
            }
            if songs.is_empty() {
                return Err("歌单没有可导入歌曲".into());
            }
            // The local schema keeps one row per song ID; preserve the first occurrence.
            let mut seen = std::collections::HashSet::new();
            songs.retain(|song| {
                let id = match &song["songmid"] {
                    serde_json::Value::String(id) => id.clone(),
                    serde_json::Value::Number(id) => id.to_string(),
                    _ => String::new(),
                };
                seen.insert(id)
            });
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("已取消".into());
            }
            let description = if english {
                format!("Imported from {plugin_name}, {} songs", songs.len())
            } else {
                format!("从{plugin_name}导入，共 {} 首", songs.len())
            };
            library::import_network_playlist(
                &path,
                &item.name,
                &description,
                "service",
                Some(&item.img),
                &songs,
            )
            .map_err(|_| "保存歌单失败".to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.importing = false;
                match result {
                    Ok((playlist, favorites, _)) => {
                        let count = playlist.song_count;
                        let _ = this.parent.update(cx, |parent, cx| {
                            parent.favorites.snapshot = Some(favorites);
                            parent.load_playlists(cx);
                            cx.notify();
                        });
                        this.message = format!("{} {count}", this.t("已导入歌曲：", "Songs imported:"));
                        window.push_notification(Notification::success(this.message.clone()), cx);
                    }
                    Err(_) => this.message = this.t("导入失败：歌单为空、分页不完整或保存失败；未创建部分歌单", "Import failed: empty/incomplete playlist or save failure; no partial playlist created"),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}

impl Render for PluginPlaylists {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = div()
            .id("plugin-playlist-rows")
            .flex()
            .flex_col()
            .gap(px(12.))
            .max_h((window.viewport_size().height * 0.55).max(px(100.)))
            .overflow_y_scroll();
        for (index, item) in self.playlists.iter().cloned().enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .when(!item.img.is_empty(), |row| {
                        row.child(img(item.img.clone()).size(px(44.)).rounded(px(6.)))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(item.name.clone())
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(color(self.colors, "--td-text-color-secondary"))
                                    .child(format!("{} {}", item.total, self.t("首", "songs"))),
                            )
                            .when(!item.desc.is_empty(), |column| {
                                column.child(div().text_size(px(12.)).child(item.desc.clone()))
                            }),
                    )
                    .child(
                        Button::new(("plugin-playlist-import", index))
                            .label(self.t("导入", "Import"))
                            .primary()
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.import(item.clone(), window, cx)
                            })),
                    ),
            );
        }
        div()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .text_color(color(self.colors, "--td-text-color-primary"))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!(
                        "{} · {}",
                        self.name,
                        self.t("导入歌单", "Import playlists")
                    )),
            )
            .child(rows)
            .when(!self.message.is_empty(), |root| {
                root.child(self.message.clone())
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .when(self.failed, |row| {
                        row.child(
                            Button::new("plugin-playlist-retry")
                                .label(self.t("重试", "Retry"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
                        )
                    })
                    .child(
                        Button::new("plugin-playlist-close")
                            .label(self.t("关闭", "Close"))
                            .outline()
                            .disabled(self.importing)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
    }
}
