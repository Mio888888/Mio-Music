//! The storage category of the settings page (`StorageSection.vue`, `DirectorySettings.vue`,
//! `MusicCache.vue`): the cache and download folders, the played-song cache with its limit and a
//! clear button, and the local scan's hidden-file switch. The file name template and the tag
//! options live next to these in `settings_sections.rs` and `tag_settings.rs`.
use super::*;
use crate::audio_cache;
use crate::directories::Directories;
use gpui_kit::component::{WindowExt, notification::Notification, switch::Switch};
use std::path::PathBuf;

/// The limits offered for the played-song cache.
const CACHE_LIMITS: [(&str, u64); 4] = [
    ("500 MB", 500 * 1024 * 1024),
    ("1 GB", 1024 * 1024 * 1024),
    ("2 GB", 2 * 1024 * 1024 * 1024),
    ("5 GB", 5 * 1024 * 1024 * 1024),
];

/// What the page shows about the folders; measured in the background.
#[derive(Default)]
pub(super) struct StorageInfo {
    /// Size of the cache folder being edited, of the download folder being edited.
    cache_dir_size: u64,
    download_dir_size: u64,
    /// Files and bytes in the saved cache folder.
    cache_files: u64,
    cache_bytes: u64,
    clearing: bool,
    saving: bool,
    /// The import of the original application's data is running / what it did last.
    importing: bool,
    import_result: Option<String>,
    import_restart: bool,
    /// Measure again the next time the category is drawn.
    pub dirty: bool,
}

impl StorageInfo {
    pub fn new() -> Self {
        Self {
            dirty: true,
            ..Default::default()
        }
    }
}

/// `formatSize` of the original.
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let index = ((bytes as f64).ln() / 1024f64.ln()).floor().max(0.) as usize;
    let index = index.min(units.len() - 1);
    format!(
        "{:.2} {}",
        bytes as f64 / 1024f64.powi(index as i32),
        units[index]
    )
}

#[derive(Clone, Copy)]
enum Confirm {
    ResetDirectories,
    ClearCache,
}

struct StorageDialog {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    action: Confirm,
}

impl Mio {
    /// Measures the folders and the cache in the background.
    fn refresh_storage_info(&mut self, cx: &mut Context<Self>) {
        let cache = PathBuf::from(&self.dir_edit.cache_dir);
        let download = PathBuf::from(&self.dir_edit.download_dir);
        let saved_cache = PathBuf::from(&self.directories.cache_dir);
        cx.spawn(async move |this, cx| {
            let (cache_size, download_size, (files, bytes)) = cx
                .background_executor()
                .spawn(async move {
                    (
                        audio_cache::directory_size(&cache),
                        audio_cache::directory_size(&download),
                        audio_cache::info(&saved_cache),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.storage_info.cache_dir_size = cache_size;
                this.storage_info.download_dir_size = download_size;
                this.storage_info.cache_files = files;
                this.storage_info.cache_bytes = bytes;
                cx.notify();
            });
        })
        .detach();
    }

    /// The cache configuration the audio path should use now (`player__set_cache_config`).
    pub(super) fn apply_cache_config(&self) {
        crate::image_cache::configure(PathBuf::from(&self.directories.cache_dir));
        audio_cache::configure(self.app_settings.auto_cache_music.then(|| {
            (
                PathBuf::from(&self.directories.cache_dir),
                self.app_settings.cache_size_limit,
            )
        }));
    }

    fn pick_directory(&mut self, cache: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.t("settings.storage.selectDirectory").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = match selection.await {
                Ok(Ok(Some(paths))) => paths,
                // Cancelled.
                Ok(Ok(None)) => return,
                _ => {
                    let _ = this.update_in(cx, |this, window, cx| {
                        window.push_notification(
                            Notification::error(this.t("settings.storage.selectDirFailed")),
                            cx,
                        );
                    });
                    return;
                }
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let value = path.to_string_lossy().into_owned();
                let key = if cache {
                    this.dir_edit.cache_dir = value;
                    "settings.storage.cacheDirSelected"
                } else {
                    this.dir_edit.download_dir = value;
                    "settings.storage.downloadDirSelected"
                };
                window.push_notification(Notification::success(this.t(key)), cx);
                this.refresh_storage_info(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the original application's data (see `legacy_import`) in the background.
    fn import_legacy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if self.storage_info.importing {
            return;
        }
        self.storage_info.importing = true;
        self.storage_info.import_result = None;
        self.storage_info.import_restart = false;
        let english = self.english;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let source = crate::legacy_import::find();
                    if !source.found() {
                        return Err("not-found".to_owned());
                    }
                    crate::legacy_import::import(&store, &source)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.storage_info.importing = false;
                let (message, restart) = match result {
                    Ok(report) if report.changed() => {
                        let mut parts: Vec<String> = Vec::new();
                        let mut add = |zh: String, en: String| parts.push(if english { en } else { zh });
                        if report.playlists_added > 0 {
                            add(
                                format!("歌单 {} 个", report.playlists_added),
                                format!("{} playlists", report.playlists_added),
                            );
                        }
                        if report.plugin {
                            add("音源插件".into(), "music-source plugin".into());
                        }
                        if report.settings {
                            add("设置".into(), "settings".into());
                        }
                        if report.hotkeys {
                            add("快捷键".into(), "hotkeys".into());
                        }
                        if report.directories {
                            add("目录".into(), "folders".into());
                        }
                        if report.downloads {
                            add("下载记录".into(), "download records".into());
                        }
                        if report.desktop_lyric {
                            add("桌面歌词位置".into(), "desktop-lyric position".into());
                        }
                        if report.equalizer {
                            add("均衡器".into(), "equalizer".into());
                        }
                        if report.audio_effects {
                            add("音效（声道平衡）".into(), "audio effects".into());
                        }
                        if report.local_tracks > 0 {
                            add(
                                format!("本地音乐 {} 首", report.local_tracks),
                                format!("{} local tracks", report.local_tracks),
                            );
                        }
                        let joined = parts.join(if english { ", " } else { "、" });
                        (
                            if english {
                                format!("Imported: {joined}. Restart to apply the settings.")
                            } else {
                                format!("已导入：{joined}。重启后设置生效。")
                            },
                            true,
                        )
                    }
                    Ok(report) => (
                        if english {
                            format!(
                                "Nothing new to import ({} playlists in the original already exist here).",
                                report.playlists_seen
                            )
                        } else {
                            format!(
                                "没有需要导入的新内容（旧版的 {} 个歌单已在这里）。",
                                report.playlists_seen
                            )
                        },
                        false,
                    ),
                    Err(error) if error == "not-found" => (
                        if english {
                            "The original application's data was not found on this computer."
                        } else {
                            "未在本机找到旧版应用的数据。"
                        }
                        .to_owned(),
                        false,
                    ),
                    Err(error) => (error, false),
                };
                this.storage_info.import_result = Some(message);
                this.storage_info.import_restart = restart;
                let _ = window;
                cx.notify();
            });
        })
        .detach();
    }

    fn open_storage_directory(&self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() || !path.is_dir() {
            window.push_notification(
                Notification::error(self.t("settings.storage.openDirFailed")),
                cx,
            );
            return;
        }
        super::downloads_page::open_directory(&path);
    }

    fn save_directories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if self.storage_info.saving {
            return;
        }
        self.storage_info.saving = true;
        let edited = self.dir_edit.clone();
        let result = edited.save(&store);
        self.storage_info.saving = false;
        match result {
            Ok(()) => {
                self.directories = edited;
                if let Some(manager) = &self.song_view.downloads.manager {
                    manager.set_dir(PathBuf::from(&self.directories.download_dir));
                }
                self.apply_cache_config();
                window.push_notification(
                    Notification::success(self.t("settings.storage.saveDirSuccess")),
                    cx,
                );
                self.refresh_storage_info(cx);
            }
            Err(_) => window.push_notification(
                Notification::error(self.t("settings.storage.saveSettingsFailed")),
                cx,
            ),
        }
        cx.notify();
    }

    fn reset_directories(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let defaults = Directories::defaults(store.root());
        match defaults.save(&store) {
            Ok(()) => {
                self.dir_edit = defaults.clone();
                self.directories = defaults;
                if let Some(manager) = &self.song_view.downloads.manager {
                    manager.set_dir(PathBuf::from(&self.directories.download_dir));
                }
                self.apply_cache_config();
                window.push_notification(
                    Notification::success(self.t("settings.storage.resetSuccess")),
                    cx,
                );
                self.refresh_storage_info(cx);
            }
            Err(_) => window.push_notification(
                Notification::error(self.t("settings.storage.resetFailed")),
                cx,
            ),
        }
        cx.notify();
    }

    fn clear_cache(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_info.clearing {
            return;
        }
        self.storage_info.clearing = true;
        let dir = PathBuf::from(&self.directories.cache_dir);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { audio_cache::clear(&dir) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.storage_info.clearing = false;
                let (message, success) = match result {
                    Ok(()) => ("settings.storage.clearSuccess", true),
                    Err(_) => ("settings.storage.clearCacheFailed", false),
                };
                let text = this.t(message);
                window.push_notification(
                    if success {
                        Notification::success(text)
                    } else {
                        Notification::error(text)
                    },
                    cx,
                );
                this.refresh_storage_info(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn open_storage_confirm(
        &mut self,
        action: Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let content = cx.new(|_| StorageDialog {
            parent,
            colors,
            english,
            action,
        });
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let width = f32::from(window.viewport_size().width);
            let gutter = (width * 0.04).clamp(16., 24.);
            dialog
                .w(px((width - 2. * gutter).min(if narrow {
                    420.
                } else {
                    480.
                })))
                .margin_top(window.viewport_size().height * 0.2)
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(colors, "--td-bg-color-container"))
                .border_color(color(colors, "--td-border-level-1-color"))
                .overlay_tint(color(colors, "--td-mask-active"))
                .close_button(false)
                .on_ok(|_, _, _| false)
                .child(content.clone())
        });
    }

    /// The path box of a directory block: a read-only field and its two buttons.
    fn directory_block(&self, cache: bool, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let (title, description, path, size, id, placeholder) = if cache {
            (
                self.t("settings.storage.cacheDirectory"),
                self.t("settings.storage.cacheDirectoryDesc"),
                self.dir_edit.cache_dir.clone(),
                self.storage_info.cache_dir_size,
                "cache",
                self.t("settings.storage.cacheDirPlaceholder"),
            )
        } else {
            (
                self.t("settings.storage.downloadDirectory"),
                self.t("settings.storage.downloadDirectoryDesc"),
                self.dir_edit.download_dir.clone(),
                self.storage_info.download_dir_size,
                "download",
                self.t("settings.storage.downloadDirPlaceholder"),
            )
        };
        let (tag_bg, tag_fg) = if cache {
            (
                color(mode, "--td-brand-color-1"),
                color(mode, "--td-brand-color"),
            )
        } else {
            (
                color(mode, "--td-success-color-1"),
                color(mode, "--td-success-color"),
            )
        };
        let open_path = path.clone();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color(mode, "--td-text-color-primary"))
                            .child(title),
                    )
                    .child(
                        div()
                            .px(px(8.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .rounded(px(3.))
                            .bg(tag_bg)
                            .text_color(tag_fg)
                            .text_size(px(12.))
                            .child(format_size(size)),
                    ),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(13.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(description),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .when(narrow, |row| row.flex_col())
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(3.))
                            .border_1()
                            .border_color(color(mode, "--td-border-level-2-color"))
                            .bg(color(mode, "--td-bg-color-specialcomponent"))
                            .text_size(px(14.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(if path.is_empty() {
                                color(mode, "--td-text-color-placeholder")
                            } else {
                                color(mode, "--td-text-color-primary")
                            })
                            .child(if path.is_empty() { placeholder } else { path }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .flex_shrink_0()
                            .child(
                                Button::new(SharedString::from(format!("{id}-dir-select")))
                                    .label(self.t("settings.storage.selectDirectory"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.pick_directory(cache, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("{id}-dir-open")))
                                    .label(self.t("settings.storage.openDirectory"))
                                    .outline()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_storage_directory(&open_path, window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn storage_directory_groups(
        &mut self,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if self.storage_info.dirty {
            self.storage_info.dirty = false;
            self.refresh_storage_info(cx);
        }
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let directory = self
            .setting_group("storage-directory", None, narrow)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(16.))
                    .child(
                        div()
                            .text_size(px(if narrow { 16. } else { 18. }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.t("settings.storage.directoryConfig")),
                    )
                    .child(
                        Button::new("dir-reset")
                            .label(self.t("settings.storage.resetToDefault"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_storage_confirm(Confirm::ResetDirectories, window, cx)
                            })),
                    ),
            )
            .child(self.directory_block(true, narrow, cx))
            .child(
                div()
                    .w_full()
                    .h(px(1.))
                    .my(px(24.))
                    .bg(color(mode, "--td-border-level-1-color")),
            )
            .child(self.directory_block(false, narrow, cx))
            .child(
                div().mt(px(20.)).child(
                    Button::new("dir-save")
                        .label(self.t("settings.storage.saveSettings"))
                        .primary()
                        .disabled(self.storage_info.saving)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.save_directories(window, cx)),
                        ),
                ),
            )
            .into_any_element();

        let info = &self.storage_info;
        let cache_count = info.cache_files;
        let stats = format!(
            "{}{}{}",
            self.t("settings.storage.existingCacheSize"),
            format_size(info.cache_bytes),
            if cache_count > 0 {
                format!(
                    "（{}）",
                    self.t("settings.storage.fileCount")
                        .replace("{count}", &cache_count.to_string())
                )
            } else {
                String::new()
            }
        );
        let clearing = info.clearing;
        let cache = self
            .setting_group("storage-cache", None, narrow)
            .child(
                div()
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
                            .text_color(primary)
                            .child(self.t("settings.storage.localCacheConfig")),
                    )
                    .child(div().text_size(px(13.)).text_color(secondary).child(stats)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        Button::new("cache-clear")
                            .label(if clearing {
                                self.t("settings.storage.clearingCache")
                            } else {
                                self.t("settings.storage.clearLocalCache")
                            })
                            .disabled(cache_count == 0 || clearing)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_storage_confirm(Confirm::ClearCache, window, cx)
                            })),
                    )
                    .when(cache_count == 0, |row| {
                        row.child(
                            div()
                                .text_size(px(13.))
                                .text_color(secondary)
                                .child(self.t("settings.storage.noCache")),
                        )
                    }),
            )
            .into_any_element();

        let auto_cache = self.app_settings.auto_cache_music;
        let limit = self.app_settings.cache_size_limit;
        let brand = color(mode, "--td-brand-color");
        let mut limits = div()
            .flex()
            .rounded(px(6.))
            .border_1()
            .border_color(color(mode, "--td-component-border"))
            .bg(color(mode, "--td-bg-color-secondarycontainer"))
            .overflow_hidden();
        for (label, bytes) in CACHE_LIMITS {
            let active = limit == bytes;
            limits = limits.child(
                gpui_kit::base::Button::new(SharedString::from(format!("cache-limit-{bytes}")))
                    .accessibility_label(label)
                    .h(px(32.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(if active { primary } else { secondary })
                    .when(active, |b| {
                        b.bg(color(mode, "--td-bg-color-component-active"))
                            .border_1()
                            .border_color(brand)
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.app_settings.cache_size_limit = bytes;
                        this.save_preferences();
                        this.apply_cache_config();
                        cx.notify();
                    })),
            );
        }
        let strategy = self
            .setting_group(
                "storage-cache-strategy",
                Some(self.t("settings.storage.cacheStrategy")),
                narrow,
            )
            .child(
                self.setting_item(
                    "auto-cache-music",
                    self.t("settings.storage.autoCacheMusic"),
                    Some(self.t("settings.storage.autoCacheMusicDesc")),
                    Switch::new("auto-cache-switch")
                        .accessibility_label(self.t("settings.storage.autoCacheMusic"))
                        .checked(auto_cache)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.app_settings.auto_cache_music = *checked;
                            this.save_preferences();
                            this.apply_cache_config();
                            cx.notify();
                        }))
                        .into_any_element(),
                    narrow,
                ),
            )
            .when(auto_cache, |group| {
                group.child(self.setting_item(
                    "cache-size-limit",
                    self.t("settings.storage.cacheSizeLimit"),
                    Some(self.t("settings.storage.cacheSizeLimitDesc")),
                    limits.into_any_element(),
                    narrow,
                ))
            })
            .into_any_element();

        let scan = self
            .setting_group(
                "storage-local-scan",
                Some(self.t("settings.storage.localScanTitle")),
                narrow,
            )
            .child(
                self.setting_item(
                    "skip-hidden-files",
                    self.t("settings.storage.skipHiddenFiles"),
                    Some(self.t("settings.storage.skipHiddenFilesDesc")),
                    Switch::new("skip-hidden-switch")
                        .accessibility_label(self.t("settings.storage.skipHiddenFiles"))
                        .checked(self.app_settings.skip_hidden_files)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.app_settings.skip_hidden_files = *checked;
                            this.save_preferences();
                            cx.notify();
                        }))
                        .into_any_element(),
                    narrow,
                ),
            )
            .into_any_element();
        let secondary_text = secondary;
        let import = self
            .setting_group(
                "storage-legacy-import",
                Some(
                    if self.english {
                        "Original application data"
                    } else {
                        "旧版应用数据"
                    }
                    .to_owned(),
                ),
                narrow,
            )
            .child(
                self.setting_item(
                    "legacy-import",
                    if self.english {
                        "Import from the original application"
                    } else {
                        "从旧版导入数据"
                    }
                    .to_owned(),
                    Some(
                        if self.english {
                            "Adds the original's playlists, music-source plugin, settings, hotkeys and folders. Nothing is changed or removed in the original."
                        } else {
                            "导入旧版的歌单、音源插件、设置、快捷键与目录；不会修改或删除旧版的任何数据。"
                        }
                        .to_owned(),
                    ),
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(
                            Button::new("legacy-import-run")
                                .label(if self.storage_info.importing {
                                    if self.english { "Importing…" } else { "导入中…" }
                                } else if self.english {
                                    "Import"
                                } else {
                                    "导入"
                                })
                                .loading(self.storage_info.importing)
                                .disabled(self.storage_info.importing)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.import_legacy(window, cx)
                                })),
                        )
                        .when(self.storage_info.import_restart, |row| {
                            row.child(
                                Button::new("legacy-import-restart")
                                    .label(if self.english { "Restart now" } else { "立即重启" })
                                    .outline()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_session();
                                        match crate::updater::relaunch() {
                                            Ok(()) => cx.quit(),
                                            Err(_) => window.push_notification(
                                                Notification::error(
                                                    this.t("settings.about.restartFailed"),
                                                ),
                                                cx,
                                            ),
                                        }
                                    })),
                            )
                        })
                        .into_any_element(),
                    narrow,
                ),
            )
            .children(self.storage_info.import_result.clone().map(|message| {
                div()
                    .mt(px(8.))
                    .text_size(px(13.))
                    .text_color(secondary_text)
                    .child(message)
            }))
            .into_any_element();
        vec![directory, cache, strategy, scan, import]
    }
}

impl StorageDialog {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let action = self.action;
        window.close_dialog(cx);
        let _ = self.parent.update(cx, |parent, cx| match action {
            Confirm::ResetDirectories => parent.reset_directories(window, cx),
            Confirm::ClearCache => parent.clear_cache(window, cx),
        });
    }
}

impl Render for StorageDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let (title, body, cancel, confirm) = match self.action {
            Confirm::ResetDirectories => (
                self.t("settings.storage.resetDirTitle"),
                self.t("settings.storage.resetDirBody"),
                self.t("common.cancel"),
                self.t("settings.storage.confirmReset"),
            ),
            Confirm::ClearCache => (
                self.t("settings.storage.confirmClearCache"),
                self.t("settings.storage.clearCacheBody"),
                self.t("settings.storage.letMeThink"),
                self.t("settings.storage.confirmClear"),
            ),
        };
        div()
            .p(px(32.))
            .text_size(px(14.))
            .text_color(primary)
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt(px(16.))
                    .line_height(px(22.))
                    .text_color(secondary)
                    .child(body),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(24.))
                    .child(
                        Button::new("storage-dialog-cancel")
                            .label(cancel)
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("storage-dialog-confirm")
                            .label(confirm)
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            )
    }
}
