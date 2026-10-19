//! The music source category of the settings page (`MusicSourceSection.vue`): the Subsonic
//! form (its own entity), and — with a plugin installed — the source cards, the quality tags of
//! the selected source, the global quality and the resulting configuration.
//!
//! The choice is kept the way the original keeps it: `selectSources` and `selectQuality`
//! (`AppSettings`) and one quality per source (`sourceQualityMap`, shared with the download
//! dialog's remembered quality). Playback and downloads read it through `quality::preference`.
use super::settings_page::Choice;
use super::*;
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectState},
};

/// `QUALITY_ORDER` of the page: low to high; unknown names last.
fn quality_rank(quality: &str) -> u32 {
    match quality {
        "128k" => 1,
        "320k" => 2,
        "flac" => 3,
        "flac24bit" => 4,
        "hires" => 5,
        "atmos" => 6,
        "master" => 7,
        _ => 999,
    }
}

fn sorted(qualities: &[String]) -> Vec<String> {
    let mut list = qualities.to_vec();
    list.sort_by_key(|q| quality_rank(q));
    list
}

#[derive(Clone, Copy, PartialEq)]
enum Tier {
    Standard,
    Lossless,
    HiRes,
}
fn tier(quality: &str) -> Tier {
    match quality {
        "flac" | "flac24bit" => Tier::Lossless,
        "hires" | "atmos" | "master" => Tier::HiRes,
        _ => Tier::Standard,
    }
}

/// The global quality select and what it lists.
pub(super) struct MusicSourceUi {
    global: Entity<SelectState<Vec<Choice>>>,
    options: Vec<String>,
    /// The option shown; defaults to the highest common quality.
    selected: Option<String>,
    signature: String,
}

impl MusicSourceUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Mio>) -> (Self, Vec<Subscription>) {
        let global = cx.new(|cx| SelectState::new(Vec::<Choice>::new(), None, window, cx));
        let subscription = cx.subscribe_in(&global, window, |this, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(index)) = event
                && let Some(quality) = this.music_source_ui.options.get(*index).cloned()
            {
                this.apply_global_quality(&quality, cx);
            }
        });
        (
            Self {
                global,
                options: Vec::new(),
                selected: None,
                signature: String::new(),
            },
            vec![subscription],
        )
    }
}

/// One selectable source as the plugin declares it.
struct SourceOption {
    id: String,
    name: String,
    qualities: Vec<String>,
}

impl Mio {
    fn source_options(&self) -> Vec<SourceOption> {
        self.online
            .as_ref()
            .map(|online| {
                online
                    .sources()
                    .iter()
                    .map(|s| SourceOption {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        qualities: s.qualities.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The selected source (the first one while nothing valid is chosen) and its quality
    /// (`userSource`): the source's entry in the map, else `selectQuality`, else the shared playback default.
    fn selection(&self, options: &[SourceOption]) -> Option<(usize, Option<String>)> {
        let index = options
            .iter()
            .position(|s| s.id == self.app_settings.selected_source)
            .or(if options.is_empty() { None } else { Some(0) })?;
        let source = &options[index];
        let quality = self
            .song_view
            .downloads
            .preferred
            .get(&source.id)
            .filter(|q| source.qualities.contains(q))
            .or(Some(&self.app_settings.selected_quality).filter(|q| source.qualities.contains(q)))
            .or_else(|| crate::quality::default_quality(&source.qualities))
            .cloned();
        Some((index, quality))
    }

    /// `userSource.quality`: the selected source's quality, as the settings page shows it.
    pub(super) fn selected_music_quality(&self) -> Option<String> {
        let options = self.source_options();
        self.selection(&options).and_then(|(_, quality)| quality)
    }

    /// Pushes the choice to where playback reads it; called whenever preferences are saved.
    pub(super) fn sync_quality_preference(&self) {
        crate::quality::set_preference(crate::quality::Preference {
            per_source: self
                .song_view
                .downloads
                .preferred
                .clone()
                .into_iter()
                .collect(),
            selected_source: Some(self.app_settings.selected_source.clone())
                .filter(|s| !s.is_empty()),
            selected_quality: Some(self.app_settings.selected_quality.clone())
                .filter(|s| !s.is_empty()),
        });
    }

    /// `selectSource`: the source's saved quality if it is still declared, else the shared playback default.
    pub(super) fn select_music_source(&mut self, id: &str, cx: &mut Context<Self>) {
        let options = self.source_options();
        let Some(source) = options.iter().find(|s| s.id == id) else {
            return;
        };
        self.app_settings.selected_source = id.to_owned();
        if !source.qualities.is_empty() {
            let saved = self
                .song_view
                .downloads
                .preferred
                .get(id)
                .filter(|q| source.qualities.contains(q))
                .cloned();
            let quality = saved.unwrap_or_else(|| {
                crate::quality::default_quality(&source.qualities)
                    .cloned()
                    .unwrap_or_default()
            });
            self.song_view
                .downloads
                .preferred
                .insert(id.to_owned(), quality.clone());
            self.app_settings.selected_quality = quality;
        }
        self.save_preferences();
        cx.notify();
    }

    /// `selectQualityTag`.
    fn select_music_quality(&mut self, quality: &str, cx: &mut Context<Self>) {
        let options = self.source_options();
        let Some((index, _)) = self.selection(&options) else {
            return;
        };
        let id = options[index].id.clone();
        self.app_settings.selected_source = id.clone();
        self.app_settings.selected_quality = quality.to_owned();
        self.song_view
            .downloads
            .preferred
            .insert(id, quality.to_owned());
        self.save_preferences();
        cx.notify();
    }

    /// `applyGlobalQuality`: every source that offers it gets it.
    fn apply_global_quality(&mut self, quality: &str, cx: &mut Context<Self>) {
        let options = self.source_options();
        let current = self.selection(&options).map(|(index, _)| index);
        for source in &options {
            if source.qualities.iter().any(|q| q == quality) {
                self.song_view
                    .downloads
                    .preferred
                    .insert(source.id.clone(), quality.to_owned());
            }
        }
        if let Some(index) = current
            && options[index].qualities.iter().any(|q| q == quality)
        {
            self.app_settings.selected_source = options[index].id.clone();
            self.app_settings.selected_quality = quality.to_owned();
        }
        self.music_source_ui.selected = Some(quality.to_owned());
        self.save_preferences();
        cx.notify();
    }

    fn quality_name(&self, quality: &str) -> String {
        let key = match quality {
            "128k" => "settings.musicSource.quality128k",
            "320k" => "settings.musicSource.quality320k",
            "flac" => "settings.musicSource.qualityFlac",
            "flac24bit" => "settings.musicSource.qualityFlac24bit",
            "hires" => "settings.musicSource.qualityHires",
            "atmos" => "settings.musicSource.qualityAtmos",
            "master" => "settings.musicSource.qualityMaster",
            _ => return quality.to_owned(),
        };
        self.t(key)
    }

    fn quality_description(&self, quality: &str) -> String {
        let key = match quality {
            "128k" => "settings.musicSource.quality128kDesc",
            "320k" => "settings.musicSource.quality320kDesc",
            "flac" => "settings.musicSource.qualityFlacDesc",
            "flac24bit" => "settings.musicSource.qualityFlac24bitDesc",
            "hires" => "settings.musicSource.qualityHiresDesc",
            "atmos" => "settings.musicSource.qualityAtmosDesc",
            "master" => "settings.musicSource.qualityMasterDesc",
            _ => "settings.musicSource.qualityCustomDesc",
        };
        self.t(key)
    }

    /// Refills the global quality select when what it lists changed.
    fn sync_music_source_controls(
        &mut self,
        options: &[SourceOption],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Qualities every source offers, low to high.
        let common: Vec<String> = match options.split_first() {
            Some((first, rest)) => sorted(
                &first
                    .qualities
                    .iter()
                    .filter(|q| rest.iter().all(|s| s.qualities.contains(q)))
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            None => Vec::new(),
        };
        if self
            .music_source_ui
            .selected
            .as_ref()
            .is_none_or(|selected| !common.contains(selected))
        {
            self.music_source_ui.selected = common.last().cloned();
        }
        let signature = format!("{}|{}", common.join(","), self.english);
        if self.music_source_ui.signature != signature || self.music_source_ui.options != common {
            self.music_source_ui.signature = signature;
            self.music_source_ui.options = common.clone();
            let items: Vec<Choice> = common
                .iter()
                .enumerate()
                .map(|(i, q)| Choice(i, self.quality_name(q).into()))
                .collect();
            let position = self
                .music_source_ui
                .selected
                .as_ref()
                .and_then(|s| common.iter().position(|q| q == s));
            self.music_source_ui.global.update(cx, |state, cx| {
                state.set_items(items, window, cx);
                state.set_selected_index(position.map(|i| IndexPath::default().row(i)), window, cx);
            });
        }
    }

    pub(super) fn music_groups(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let mut groups: Vec<AnyElement> = Vec::new();
        let options = self.source_options();
        if self.online.is_none() || options.is_empty() {
            groups.push(
                self.setting_group("music-source", None, narrow)
                    .child(
                        div()
                            .py(px(8.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(self.t("settings.musicSource.noPluginTitle")),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(secondary)
                                    .child(self.t("settings.musicSource.noPluginDesc")),
                            )
                            .child(
                                Button::new("music-go-plugins")
                                    .label(self.t("settings.musicSource.goToPlugin"))
                                    .primary()
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.switch_to_plugins(cx)),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
            return groups;
        }
        self.sync_music_source_controls(&options, window, cx);
        let Some((selected_index, selected_quality)) = self.selection(&options) else {
            return groups;
        };
        let selected_id = options[selected_index].id.clone();
        let selected_name = options[selected_index].name.clone();
        let plugin_name = self
            .online
            .as_ref()
            .map(|o| o.plugin.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.t("settings.musicSource.unknownPlugin"));

        // The plugin in use.
        groups.push(
            self.setting_group("music-plugin", None, narrow)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .px(px(14.))
                        .py(px(10.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(brand.opacity(0.3))
                        .bg(color(mode, "--td-bg-color-container"))
                        .child(
                            div()
                                .size(px(8.))
                                .rounded(px(4.))
                                .bg(color(mode, "--td-success-color")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(14.4))
                                .text_color(primary)
                                .child(
                                    self.t("settings.musicSource.currentConfig")
                                        .replace("{name}", &plugin_name),
                                ),
                        )
                        .child(
                            div()
                                .px(px(10.))
                                .py(px(2.))
                                .rounded(px(999.))
                                .text_size(px(11.5))
                                .text_color(color(mode, "--td-success-color"))
                                .bg(color(mode, "--td-success-color").opacity(0.12))
                                .border_1()
                                .border_color(color(mode, "--td-success-color").opacity(0.3))
                                .child(self.t("settings.musicSource.enabled")),
                        ),
                )
                .into_any_element(),
        );

        // Source cards.
        let mut cards = div().flex().flex_wrap().gap(px(10.));
        for source in &options {
            let active = source.id == selected_id;
            let id = source.id.clone();
            let own_quality = self
                .song_view
                .downloads
                .preferred
                .get(&source.id)
                .cloned()
                .or_else(|| active.then(|| selected_quality.clone()).flatten());
            let tag = own_quality.map(|quality| {
                let (fg, bg, border) = match tier(&quality) {
                    Tier::Standard => (secondary, primary.opacity(0.06), primary.opacity(0.12)),
                    Tier::Lossless => (brand, brand.opacity(0.10), brand.opacity(0.24)),
                    Tier::HiRes => (
                        rgb(0xb8860b).into(),
                        rgba(0xffc10724).into(),
                        rgba(0xffc10759).into(),
                    ),
                };
                div()
                    .px(px(7.))
                    .py(px(1.))
                    .rounded(px(999.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(fg)
                    .bg(bg)
                    .border_1()
                    .border_color(border)
                    .child(self.quality_name(&quality))
            });
            cards = cards.child(
                div()
                    .id(SharedString::from(format!("source-card-{id}")))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .min_w(px(215.))
                    .when(!narrow, |card| card.w(px(230.)))
                    .when(narrow, |card| card.w_full())
                    .px(px(14.))
                    .py(px(11.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(if active {
                        brand
                    } else {
                        color(mode, "--td-border-level-1-color")
                    })
                    .bg(if active {
                        brand.opacity(0.09)
                    } else {
                        color(mode, "--td-bg-color-container")
                    })
                    .cursor_pointer()
                    .hover(move |card| card.border_color(brand.opacity(0.4)))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_music_source(&id, cx)))
                    .child(
                        div()
                            .size(px(36.))
                            .flex_shrink_0()
                            .rounded(px(18.))
                            .overflow_hidden()
                            .bg(color(mode, "--td-brand-color-1"))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(secondary)
                            .child(Self::plugin_source_icon(&source.id, 36.)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(source.name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(12.))
                                    .text_color(secondary)
                                    .child(self.t("settings.musicSource.sourceType"))
                                    .when_some(tag, |row, tag| row.child(tag)),
                            ),
                    )
                    .when(active, |card| {
                        card.child(
                            div()
                                .absolute()
                                .top(px(-7.))
                                .right(px(-7.))
                                .size(px(18.))
                                .rounded(px(9.))
                                .bg(brand)
                                .text_color(color(mode, "--td-text-color-anti"))
                                .text_size(px(11.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("✓"),
                        )
                    }),
            );
        }
        groups.push(
            self.setting_group(
                "music-source",
                Some(self.t("settings.musicSource.musicSourceSelect")),
                narrow,
            )
            .child(cards)
            .into_any_element(),
        );

        // Quality tags of the selected source.
        let declared = sorted(&options[selected_index].qualities);
        if !declared.is_empty() {
            let mut tags = div().flex().flex_wrap().gap(px(8.)).py(px(8.));
            for quality in &declared {
                let active = selected_quality.as_deref() == Some(quality.as_str());
                let value = quality.clone();
                tags =
                    tags.child(
                        div()
                            .id(SharedString::from(format!("quality-tag-{quality}")))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px(px(14.))
                            .py(px(6.))
                            .rounded(px(999.))
                            .border_1()
                            .border_color(if active { brand } else { primary.opacity(0.14) })
                            .bg(if active {
                                brand.opacity(0.10)
                            } else {
                                color(mode, "--td-bg-color-container")
                            })
                            .cursor_pointer()
                            .hover(move |tag| tag.border_color(brand.opacity(0.4)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_music_quality(&value, cx)
                            }))
                            .when(active, |tag| {
                                tag.child(div().text_size(px(12.)).text_color(brand).child("✓"))
                            })
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(if active {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(if active { brand } else { secondary })
                                    .child(self.quality_name(quality)),
                            ),
                    );
            }
            groups.push(
                self.setting_group("music-quality", None, narrow)
                    .child(
                        div()
                            .mb(px(6.))
                            .flex()
                            .items_baseline()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(primary)
                                    .child(self.t("settings.musicSource.qualitySelect")),
                            )
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(secondary)
                                    .child(format!("— {selected_name}")),
                            ),
                    )
                    .child(tags)
                    .when_some(selected_quality.clone(), |group, quality| {
                        group.child(
                            div()
                                .mt(px(4.))
                                .text_size(px(13.))
                                .line_height(relative(1.45))
                                .text_color(secondary)
                                .child(self.quality_description(&quality)),
                        )
                    })
                    .into_any_element(),
            );
        }

        // Global quality: what every source offers.
        if !self.music_source_ui.options.is_empty() {
            groups.push(
                self.setting_group(
                    "music-global-quality",
                    Some(self.t("settings.musicSource.globalQuality")),
                    narrow,
                )
                .child(Select::new(&self.music_source_ui.global).w_full())
                .into_any_element(),
            );
        }

        // The configuration as it stands.
        let status = |label: String, value: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(8.))
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .bg(color(mode, "--td-bg-color-page"))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(secondary)
                        .child(format!("{label}:")),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(primary)
                        .child(value),
                )
        };
        groups.push(
            self.setting_group(
                "music-status",
                Some(self.t("settings.musicSource.configStatus")),
                narrow,
            )
            .child(
                div()
                    .flex()
                    .when(narrow, |row| row.flex_col())
                    .gap(px(10.))
                    .child(div().flex_1().child(status(
                        self.t("settings.musicSource.musicSourceLabel"),
                        selected_name,
                    )))
                    .child(
                        div().flex_1().child(status(
                            self.t("settings.musicSource.qualityLabel"),
                            selected_quality
                                .as_deref()
                                .map(|q| self.quality_name(q))
                                .unwrap_or_default(),
                        )),
                    ),
            )
            .into_any_element(),
        );
        groups
    }
}
