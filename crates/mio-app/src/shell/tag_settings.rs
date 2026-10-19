//! "Download tag write settings" (`StorageSection.vue` of the original): what gets written into
//! a downloaded file's tags. The options live in `preferences.json` (`tag_write_options`) and the
//! download queue reads them when a file has finished.
use super::*;
use crate::tags::TagOptions;
use gpui_kit::component::{
    checkbox::Checkbox,
    radio::{Radio, RadioGroup},
};

const FORMATS: [&str; 2] = ["lrc", "word-by-word"];

impl Mio {
    /// Applies a change to the tag options, saves them and hands them to the download queue.
    fn change_tag_options(&mut self, change: impl FnOnce(&mut TagOptions), cx: &mut Context<Self>) {
        change(&mut self.song_view.downloads.tags);
        // `download_manager` passes the current options on to the queue.
        let _ = self.download_manager();
        self.save_preferences();
        cx.notify();
    }

    pub(super) fn tag_settings(&self, narrow: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        let options = self.song_view.downloads.tags.clone();
        let description = |text: String| {
            div()
                .mt(px(2.))
                .ml(px(28.))
                .text_size(px(12.))
                .text_color(color(mode, "--td-text-color-placeholder"))
                .child(text)
        };
        let enabled: Vec<String> = [
            (options.basic_info, "settings.storage.tagBasicInfo"),
            (options.cover, "settings.storage.tagCover"),
            (options.lyrics, "settings.storage.tagLyricsInfo"),
            (
                options.download_lyrics,
                "settings.storage.tagDownloadLyricsFile",
            ),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, key)| self.t(key))
        .collect();
        let summary = if enabled.is_empty() {
            self.t("settings.storage.tagNoSelection")
        } else {
            enabled.join("、")
        };
        let format_disabled = !options.lyrics && !options.download_lyrics;
        let selected = FORMATS
            .iter()
            .position(|format| *format == options.lyric_format);
        self.setting_group(
            "storage-tags",
            Some(self.t("settings.storage.tagWriteSettings")),
            narrow,
        )
        .gap(px(12.))
        .child(
            div()
                .mb(px(4.))
                .text_size(px(13.))
                .text_color(color(mode, "--td-text-color-secondary"))
                .child(self.t("settings.storage.tagWriteDesc")),
        )
        .child(
            div()
                .child(
                    Checkbox::new("tag-basic-info")
                        .label(self.t("settings.storage.tagBasicInfo"))
                        .checked(options.basic_info)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.change_tag_options(|o| o.basic_info = checked, cx);
                        })),
                )
                .child(description(self.t("settings.storage.tagBasicInfoDesc"))),
        )
        .child(
            div()
                .child(
                    Checkbox::new("tag-cover")
                        .label(self.t("settings.storage.tagCover"))
                        .checked(options.cover)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.change_tag_options(|o| o.cover = checked, cx);
                        })),
                )
                .child(description(self.t("settings.storage.tagCoverDesc"))),
        )
        .child(
            div()
                .child(
                    Checkbox::new("tag-lyrics")
                        .label(self.t("settings.storage.tagLyricsInfo"))
                        .checked(options.lyrics)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.change_tag_options(|o| o.lyrics = checked, cx);
                        })),
                )
                .child(description(self.t("settings.storage.tagLyricsInfoDesc"))),
        )
        .child(
            div()
                .child(
                    Checkbox::new("tag-download-lyrics")
                        .label(self.t("settings.storage.tagDownloadLyricsFile"))
                        .checked(options.download_lyrics)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let checked = *checked;
                            this.change_tag_options(|o| o.download_lyrics = checked, cx);
                        })),
                )
                .child(description(
                    self.t("settings.storage.tagDownloadLyricsFileDesc"),
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    RadioGroup::horizontal("tag-lyric-format")
                        .disabled(format_disabled)
                        .selected_index(selected)
                        .child(
                            Radio::new("tag-format-lrc")
                                .label(self.t("settings.storage.tagStandardLrc")),
                        )
                        .child(
                            Radio::new("tag-format-word")
                                .label(self.t("settings.storage.tagWordByWord")),
                        )
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            if let Some(format) = FORMATS.get(*index) {
                                this.change_tag_options(
                                    |o| o.lyric_format = (*format).to_string(),
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(color(mode, "--td-text-color-placeholder"))
                        .child(self.t("settings.storage.tagLyricFormatDesc")),
                ),
        )
        .child(
            div()
                .pt(px(10.))
                .border_t_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .flex()
                .gap(px(8.))
                .text_size(px(13.))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(self.t("settings.storage.tagCurrentConfig")),
                )
                .child(
                    div()
                        .text_color(color(mode, "--td-brand-color"))
                        .child(summary),
                ),
        )
    }
}
