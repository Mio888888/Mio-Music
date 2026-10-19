//! The settings page (`views/settings/index.vue` of the original): a full-window page with its
//! own header (back, title, window buttons), a sidebar of eight categories and the selected
//! category's groups on the right. Opening it from the discovery guide goes to the "music" or
//! "plugins" category (`/settings?category=…`).
//!
//! The original passes its settings search through a `TitleBarControls` slot that does not
//! exist, so the search box never appears in its window; none is drawn here either.
use super::*;
use gpui_kit::component::{
    IndexPath, Sizable,
    input::{InlineToken, InputContent},
    searchable_list::DropdownStyle,
    select::{Select, SelectEvent, SelectItem, SelectState},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Appearance,
    Playlist,
    Effects,
    Hotkeys,
    Plugins,
    Music,
    Storage,
    About,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Self::Appearance,
        Self::Playlist,
        Self::Effects,
        Self::Hotkeys,
        Self::Plugins,
        Self::Music,
        Self::Storage,
        Self::About,
    ];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
    fn label_key(self) -> &'static str {
        match self {
            Self::Appearance => "settings.musicSource.navAppearance",
            Self::Playlist => "settings.musicSource.navPlaylist",
            Self::Effects => "settings.musicSource.navEffects",
            Self::Hotkeys => "settings.musicSource.navHotkeys",
            Self::Plugins => "settings.musicSource.navPlugins",
            Self::Music => "settings.musicSource.navMusic",
            Self::Storage => "settings.musicSource.navStorage",
            Self::About => "settings.musicSource.navAbout",
        }
    }
    fn description_key(self) -> &'static str {
        match self {
            Self::Appearance => "settings.musicSource.navAppearanceDesc",
            Self::Playlist => "settings.musicSource.navPlaylistDesc",
            Self::Effects => "settings.musicSource.navEffectsDesc",
            Self::Hotkeys => "settings.musicSource.navHotkeysDesc",
            Self::Plugins => "settings.musicSource.navPluginsDesc",
            Self::Music => "settings.musicSource.navMusicDesc",
            Self::Storage => "settings.musicSource.navStorageDesc",
            Self::About => "settings.musicSource.navAboutDesc",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Appearance => "TD-Palette",
            Self::Playlist => "TD-PlayCircle",
            Self::Effects => "TD-Audio",
            Self::Hotkeys => "TD-Keyboard",
            Self::Plugins => "TD-TreeRoundDot",
            Self::Music => "TD-Music",
            Self::Storage => "TD-Save",
            Self::About => "TD-InfoCircle",
        }
    }
}

/// One entry of a select; the value is its position.
#[derive(Clone)]
pub(super) struct Choice(pub usize, pub SharedString);
impl SelectItem for Choice {
    type Value = usize;
    fn title(&self) -> SharedString {
        self.1.clone()
    }
    fn value(&self) -> &usize {
        &self.0
    }
}

pub(super) const LANGUAGES: [&str; 3] = ["system", "zh-CN", "en-US"];
pub(super) const POSITIONS: [&str; 4] = ["left", "center", "right", "both"];
pub(super) const DESKTOP_WEIGHTS: [u32; 5] = [400, 500, 600, 700, 800];

pub(super) fn normalize_lyric_families(value: &str) -> String {
    let mut fonts = Vec::new();
    for font in value.split([',', '，']).map(str::trim) {
        if !font.is_empty() && !fonts.contains(&font) {
            fonts.push(font);
        }
    }
    fonts.join(", ")
}

pub(super) fn font_input_content(value: &str) -> InputContent {
    let value = normalize_lyric_families(value);
    // Each selected font owns its hidden separator, including the last one.
    // New keyboard input is a separate draft instead of extending that font.
    let text = value
        .split(", ")
        .filter(|font| !font.is_empty())
        .map(|font| format!("{font}, "))
        .collect::<String>();
    let mut content = InputContent::new(text.clone());
    let mut offset = 0;
    for part in text.split_inclusive(", ") {
        let font = part.trim_end_matches(", ");
        if !font.is_empty() {
            content = match content.with_token(
                offset..offset + part.len(),
                InlineToken::new(font.to_owned(), part.to_owned()).with_label(font.to_owned()),
            ) {
                Ok(content) => content,
                Err(_) => return InputContent::new(value),
            };
        }
        offset += part.len();
    }
    content
}

pub(super) struct SettingsUi {
    pub category: Category,
    pub compact_detail: bool,
    compact_list_scroll: ScrollHandle,
    /// One scroll position per category, kept while the page is open (`scrollPositions`).
    pub scrolls: [ScrollHandle; 8],
    pub pills: ScrollHandle,
    pub language: Entity<SelectState<Vec<Choice>>>,
    pub lyric_family: Entity<InputState>,
    pub font_menu_open: bool,
    pub font_menu_cursor: Option<usize>,
    pub font_menu_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub font_menu_scroll: ScrollHandle,
    pub font_menu_context: Option<gpui_kit::base::DeferredPopover>,
    pub lyric_weight: Entity<SliderState>,
    pub lyric_weight_pressed: Rc<Cell<bool>>,
    pub desktop_weight: Entity<SelectState<Vec<Choice>>>,
    pub desktop_position: Entity<SelectState<Vec<Choice>>>,
    pub desktop_color: Entity<InputState>,
    pub filename: Entity<InputState>,
    /// What the desktop lyric controls show and write (see `settings_desktop_style`).
    pub desktop_style: crate::shell::desktop_lyric::LyricOption,
}

fn language_choices(english: bool) -> Vec<Choice> {
    ["languageSystem", "languageZhCN", "languageEnUS"]
        .iter()
        .enumerate()
        .map(|(i, key)| Choice(i, i18n::text(english, &format!("settings.{key}")).into()))
        .collect()
}
fn weight_choices(english: bool) -> Vec<Choice> {
    [
        "fontWeightThin",
        "fontWeightMedium",
        "fontWeightSemibold",
        "fontWeightBold",
        "fontWeightExtraBold",
    ]
    .iter()
    .enumerate()
    .map(|(i, key)| {
        Choice(
            i,
            i18n::text(english, &format!("settings.desktopLyric.{key}")).into(),
        )
    })
    .collect()
}
fn position_choices(english: bool) -> Vec<Choice> {
    [
        "positionLeft",
        "positionCenter",
        "positionRight",
        "positionBoth",
    ]
    .iter()
    .enumerate()
    .map(|(i, key)| {
        Choice(
            i,
            i18n::text(english, &format!("settings.desktopLyric.{key}")).into(),
        )
    })
    .collect()
}

impl SettingsUi {
    pub fn new(
        preferences: &Preferences,
        window: &mut Window,
        cx: &mut Context<Mio>,
    ) -> (Self, Vec<Subscription>) {
        let english = preferences.english();
        let app = &preferences.app;
        let language_index = preferences
            .language
            .as_deref()
            .and_then(|l| LANGUAGES.iter().position(|v| *v == l))
            .unwrap_or(if preferences.english { 2 } else { 1 });
        let language = cx.new(|cx| {
            SelectState::new(
                language_choices(english),
                Some(IndexPath::default().row(language_index)),
                window,
                cx,
            )
        });
        let desktop_weight = cx.new(|cx| {
            SelectState::new(
                weight_choices(english),
                Some(IndexPath::default().row(3)),
                window,
                cx,
            )
        });
        let desktop_position = cx.new(|cx| {
            SelectState::new(
                position_choices(english),
                Some(IndexPath::default().row(1)),
                window,
                cx,
            )
        });
        let family = app.lyric_font_family.clone();
        let lyric_family = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(i18n::text(english, "settings.lyricFont.selectFont"));
            state.set_value(font_input_content(&family), window, cx);
            state
        });
        let weight = app.lyric_font_weight as f32;
        let lyric_weight = cx.new(|_| {
            SliderState::new()
                .min(100.)
                .max(900.)
                .step(100.)
                .default_value(weight)
        });
        let desktop_color = cx.new(|cx| InputState::new(window, cx));
        let template = app.filename_template.clone();
        let filename = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(i18n::text(english, "settings.storage.filenamePlaceholder"));
            state.set_value(template, window, cx);
            state
        });
        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.subscribe_in(&language, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(index)) = event {
                    this.set_language(LANGUAGES[(*index).min(2)], window, cx);
                }
            }),
        );
        subscriptions.push(cx.subscribe_in(
            &lyric_family,
            window,
            |this, state, event, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => {
                        let content = state.read(cx).content();
                        let mut values = content
                            .tokens()
                            .iter()
                            .map(|span| span.token().label().to_string())
                            .collect::<Vec<_>>();
                        // A draft may also be typed before or between atomic tags.
                        // Read the untagged ranges so it never merges into a font name.
                        let mut offset = 0;
                        for span in content.tokens() {
                            let range = span.range();
                            values.push(content.text()[offset..range.start].to_owned());
                            offset = range.end;
                        }
                        values.push(content.text()[offset..].to_owned());
                        this.set_lyric_families(&values.join(", "), window, cx);
                    }
                    InputEvent::Change => {
                        // Typing only changes the draft. Keyboard deletion of selected
                        // tokens does update the persisted selection, like TDesign.
                        let selected = state
                            .read(cx)
                            .tokens()
                            .iter()
                            .map(|span| span.token().label().as_ref())
                            .collect::<Vec<_>>()
                            .join(", ");
                        let selected = if selected.is_empty() {
                            "lyricfont".to_owned()
                        } else {
                            selected
                        };
                        if this.app_settings.lyric_font_family != selected {
                            this.app_settings.lyric_font_family = selected;
                            this.save_preferences();
                        }
                        this.set_font_menu_open(true, cx);
                        this.settings_ui.font_menu_cursor = None;
                        cx.notify();
                    }
                    InputEvent::Blur => {
                        this.set_font_menu_open(false, cx);
                        // Closing the original filter does not create its pending option.
                        let value = this.app_settings.lyric_font_family.clone();
                        state.update(cx, |state, cx| {
                            state.set_value(font_input_content(&value), window, cx);
                        });
                    }
                    _ => {}
                }
            },
        ));
        subscriptions.push(
            cx.subscribe_in(&lyric_weight, window, |this, _, event, _, cx| {
                let (value, release) = match event {
                    SliderEvent::Change(v) => (v.start(), false),
                    SliderEvent::Release(v) => (v.start(), true),
                };
                this.app_settings.lyric_font_weight =
                    ((value / 100.).round() as u32 * 100).clamp(100, 900);
                if release {
                    this.save_preferences();
                }
                cx.notify();
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&desktop_weight, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(index)) = event {
                    let weight = DESKTOP_WEIGHTS[(*index).min(4)];
                    this.change_desktop_lyric_option(|o| o.font_weight = weight, cx);
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&desktop_position, window, |this, _, event, _, cx| {
                if let SelectEvent::Confirm(Some(index)) = event {
                    let position = POSITIONS[(*index).min(3)].to_string();
                    this.change_desktop_lyric_option(|o| o.position = position, cx);
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&desktop_color, window, |this, state, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().trim().to_string();
                    if crate::shell::desktop_lyric::color_is_valid(&value) {
                        this.change_desktop_lyric_option(|o| o.main_color = value, cx);
                    }
                }
            }),
        );
        subscriptions.push(
            cx.subscribe_in(&filename, window, |this, state, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.app_settings.filename_template = if value.is_empty() {
                        "%t - %s".into()
                    } else {
                        value
                    };
                    if let Some(manager) = &this.song_view.downloads.manager {
                        manager.set_filename_template(&this.app_settings.filename_template);
                    }
                    this.save_preferences();
                    cx.notify();
                }
            }),
        );
        (
            Self {
                category: Category::Appearance,
                compact_detail: true,
                compact_list_scroll: ScrollHandle::new(),
                scrolls: std::array::from_fn(|_| ScrollHandle::new()),
                pills: ScrollHandle::new(),
                language,
                lyric_family,
                font_menu_open: false,
                font_menu_cursor: None,
                font_menu_bounds: Rc::new(Cell::new(Bounds::default())),
                font_menu_scroll: ScrollHandle::new(),
                font_menu_context: None,
                lyric_weight,
                lyric_weight_pressed: Rc::new(Cell::new(false)),
                desktop_weight,
                desktop_position,
                desktop_color,
                filename,
                desktop_style: Default::default(),
            },
            subscriptions,
        )
    }
}

impl Mio {
    pub(super) fn settings_select(
        &self,
        state: &Entity<SelectState<Vec<Choice>>>,
    ) -> Select<Vec<Choice>> {
        let mode = self.ui_colors();
        Select::new(state)
            .menu_style(self.settings_dropdown_style())
            .menu_max_h(px(300.))
            .bg(color(mode, "--td-bg-color-specialcomponent"))
            .border_color(color(mode, "--td-border-level-2-color"))
            .rounded(px(3.))
            .text_size(px(14.))
            .line_height(px(22.))
            .icon(
                icon("TD-Down")
                    .size(px(16.))
                    .text_color(color(mode, "--td-text-color-secondary")),
            )
    }

    pub(super) fn settings_dropdown_style(&self) -> DropdownStyle {
        let mode = self.ui_colors();
        let shadows = match mode.mode {
            Mode::Light => vec![
                BoxShadow::new(px(0.), px(3.), black().opacity(0.05))
                    .blur_radius(px(7.))
                    .spread_radius(px(2.)),
                BoxShadow::new(px(0.), px(8.), black().opacity(0.06))
                    .blur_radius(px(5.))
                    .spread_radius(px(1.)),
                BoxShadow::new(px(0.), px(5.), black().opacity(0.10))
                    .blur_radius(px(2.5))
                    .spread_radius(px(-3.)),
            ],
            Mode::Dark => vec![
                BoxShadow::new(px(0.), px(8.), black().opacity(0.12)).blur_radius(px(5.)),
                BoxShadow::new(px(0.), px(3.), black().opacity(0.10)).blur_radius(px(7.)),
                BoxShadow::new(px(0.), px(5.), black().opacity(0.16)).blur_radius(px(2.5)),
            ],
        };
        DropdownStyle {
            surface: div()
                .bg(color(mode, "--td-bg-color-container"))
                .rounded(px(6.))
                .shadow(shadows)
                .style()
                .clone(),
            item: div()
                .h(px(28.))
                .min_h(px(28.))
                .py(px(0.))
                .px(px(8.))
                .rounded(px(3.))
                .text_size(px(14.))
                .line_height(px(22.))
                .text_color(color(mode, "--td-text-color-primary"))
                .style()
                .clone(),
            selected: div()
                .bg(color(mode, "--td-brand-color-light"))
                .text_color(color(mode, "--td-brand-color"))
                .style()
                .clone(),
            hover: div()
                .bg(color(mode, "--td-bg-color-container-hover"))
                .style()
                .clone(),
            padding: px(6.),
            spacing: px(2.),
            offset: px(8.),
            show_check: false,
        }
    }

    /// Opens the settings page on a category (`router.push({ path: '/settings', query })`).
    pub(super) fn open_settings(&mut self, category: Category, cx: &mut Context<Self>) {
        if self.navigation.current() != Page::Appearance {
            // A fresh visit starts at the top of every category.
            for scroll in &self.settings_ui.scrolls {
                scroll.set_offset(point(px(0.), px(0.)));
            }
        }
        self.settings_ui.category = category;
        self.settings_ui.compact_detail = true;
        self.go(Page::Appearance, cx);
    }

    pub(super) fn open_compact_settings(&mut self, cx: &mut Context<Self>) {
        self.set_font_menu_open(false, cx);
        self.settings_ui.compact_detail = false;
        self.go(Page::Appearance, cx);
    }

    fn compact_settings_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_font_menu_open(false, cx);
        if self.settings_ui.compact_detail {
            self.settings_ui.compact_detail = false;
            cx.notify();
        } else {
            self.navigate_history(false, window, cx);
        }
    }

    fn compact_settings_root(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.settings_ui.category == Category::Hotkeys {
            self.settings_ui.compact_detail = false;
        }
        let mode = self.ui_colors();
        let gutter = (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.);
        let detail = self.settings_ui.compact_detail;
        let category = self.settings_ui.category;
        let header = div()
            .id("settings-header")
            .test_support()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(gutter))
            .py(px(10.))
            .bg(color(mode, "--mobile-glass-bg-strong"))
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if !window.default_prevented() {
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                }
            })
            .child(
                control("back", "NavigateBefore", self.t("common.back"), mode, cx)
                    .disabled(!detail && !self.navigation.can_back())
                    .size(px(36.))
                    .icon(glyph("xiangzuo"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.compact_settings_back(window, cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t(if detail {
                        category.label_key()
                    } else {
                        "settings.title"
                    })),
            );
        let body = if detail {
            let content = self.settings_category(category, true, window, cx);
            div()
                .id(("settings-panel", category.index()))
                .test_support()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.settings_ui.scrolls[category.index()])
                .p(px(gutter))
                .child(content)
                .into_any_element()
        } else {
            let rows: Vec<_> = Category::ALL
                .into_iter()
                .filter(|category| *category != Category::Hotkeys)
                .map(|category| {
                    gpui_kit::base::Button::new(SharedString::from(format!(
                        "compact-settings-category-{}",
                        category.index()
                    )))
                    .accessibility_label(self.t(category.label_key()))
                    .w_full()
                    .min_h(px(76.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .p(px(14.))
                    .text_left()
                    .rounded(px(16.))
                    .border_1()
                    .border_color(color(mode, "--mobile-glass-border"))
                    .bg(color(mode, "--mobile-glass-bg"))
                    .hover(move |style| style.bg(color(mode, "--settings-nav-hover-bg")))
                    .child(icon(category.icon()).size(px(22.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(self.t(category.label_key())),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(relative(1.4))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(self.t(category.description_key())),
                            ),
                    )
                    .child(glyph("xiangyou").size(px(16.)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.switch_category(category, cx);
                        this.settings_ui.compact_detail = true;
                        cx.notify();
                    }))
                })
                .collect();
            div()
                .id("settings-category-list")
                .test_support()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.settings_ui.compact_list_scroll)
                .p(px(gutter))
                .flex()
                .flex_col()
                .gap(px(10.))
                .children(rows)
                .into_any_element()
        };
        if window.focused(cx).is_none() {
            self.player_focus.focus(window, cx);
        }
        div()
            .id("settings-root")
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .line_height(relative(1.6))
            .text_color(color(mode, "--td-text-color-primary"))
            .bg(self.compact_page_background())
            .child(header)
            .child(body)
            .into_any_element()
    }

    pub(super) fn compact_settings_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key == "escape" && !self.settings_ui.font_menu_open {
            self.compact_settings_back(window, cx);
        } else if event.keystroke.modifiers.alt
            && event.keystroke.key == "o"
            && self.settings_ui.compact_detail
            && self.settings_ui.category == Category::Playlist
        {
            self.toggle_audio_ab(window, cx);
        } else {
            return false;
        }
        true
    }

    fn switch_category(&mut self, category: Category, cx: &mut Context<Self>) {
        if self.settings_ui.category != category {
            self.set_font_menu_open(false, cx);
            self.settings_ui.category = category;
            cx.notify();
        }
    }

    /// The language select: `system` follows the operating system.
    pub(super) fn set_language(
        &mut self,
        language: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.language = language.to_string();
        self.english = match language {
            "en-US" => true,
            "zh-CN" => false,
            _ => crate::platform::system_is_english(),
        };
        self.apply_language(window, cx);
    }

    /// Refreshes everything that holds translated text of its own.
    pub(super) fn apply_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_background_controls(window, cx);
        self.save_preferences();
        let placeholder = self.t("common.searchMusicArtist");
        self.search.update(cx, |state, cx| {
            state.set_placeholder(placeholder, window, cx);
        });
        let task_placeholder = self.t("download.searchTask");
        self.song_view.downloads.search.update(cx, |state, cx| {
            state.set_placeholder(task_placeholder, window, cx);
        });
        let english = self.english;
        let ui = &self.settings_ui;
        ui.language.update(cx, |state, cx| {
            state.set_items(language_choices(english), window, cx)
        });
        ui.desktop_weight.update(cx, |state, cx| {
            state.set_items(weight_choices(english), window, cx)
        });
        ui.desktop_position.update(cx, |state, cx| {
            state.set_items(position_choices(english), window, cx)
        });
        let font_placeholder = i18n::text(english, "settings.lyricFont.selectFont");
        ui.lyric_family.update(cx, |state, cx| {
            state.set_placeholder(font_placeholder, window, cx)
        });
        let name_placeholder = i18n::text(english, "settings.storage.filenamePlaceholder");
        ui.filename.update(cx, |state, cx| {
            state.set_placeholder(name_placeholder, window, cx)
        });
        cx.notify();
    }

    /// Window buttons in the chosen title bar style (`TitleBarControls`).
    pub(super) fn window_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        if !self.app_settings.traffic_light {
            // `TitleBarControls.vue`: 2.25rem buttons with a 0.25rem corner, 1.125rem glyphs,
            // 0.125rem apart; the close button turns red under the pointer.
            let window_button = |id: &'static str,
                                 name: &'static str,
                                 glyph_name: &'static str,
                                 label: String,
                                 cx: &mut Context<Self>| {
                control(id, name, label, mode, cx)
                    .icon(glyph(glyph_name).size(rems(1.125)))
                    .w(rems(2.25))
                    .h(rems(2.25))
                    .rounded(px(4.))
            };
            return div()
                .flex()
                .items_center()
                .gap(rems(0.125))
                .child(
                    window_button(
                        "minimize",
                        "WindowMinimize",
                        "zuixiaohua",
                        self.t("common.titleBar.minimize"),
                        cx,
                    )
                    .on_click(|_, window, _| window.minimize_window()),
                )
                .child(
                    window_button(
                        "maximize",
                        "WindowMaximize",
                        "a-tingzhiwukuang",
                        self.t("common.titleBar.maximize"),
                        cx,
                    )
                    .on_click(|_, window, _| window.zoom_window()),
                )
                .child(
                    window_button(
                        "close",
                        "WindowClose",
                        "a-quxiaoguanbi",
                        self.t("common.titleBar.close"),
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.request_close(window, cx))),
                )
                .into_any_element();
        }
        let light =
            |id: &'static str, label: String, base: u32, hover: u32| -> gpui_kit::base::Button {
                gpui_kit::base::Button::new(id)
                    .accessibility_label(label)
                    .size(rems(2.25))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(999.))
                    .group(id)
                    .child(
                        div()
                            .size(px(16.))
                            .rounded(px(999.))
                            .bg(rgb(base))
                            .group_hover(id, move |style| style.bg(rgb(hover))),
                    )
            };
        div()
            .flex()
            .items_center()
            .gap(rems(0.125))
            .child(
                light(
                    "minimize",
                    self.t("common.titleBar.minimize"),
                    0xffbd2e,
                    0xff9500,
                )
                .on_click(|_, window, _| window.minimize_window()),
            )
            .child(
                light(
                    "maximize",
                    self.t("common.titleBar.maximize"),
                    0x28ca42,
                    0x30d158,
                )
                .on_click(|_, window, _| window.zoom_window()),
            )
            .child(
                light("close", self.t("common.titleBar.close"), 0xff5f57, 0xff3b30)
                    .on_click(cx.listener(|this, _, window, cx| this.request_close(window, cx))),
            )
            .into_any_element()
    }

    /// The settings page, replacing the whole window.
    pub(super) fn settings_root(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if f32::from(window.viewport_size().width) <= 768. {
            return self.compact_settings_root(window, cx);
        }
        let mode = self.ui_colors();
        let narrow = f32::from(window.viewport_size().width) <= 768.;
        let gutter = (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.);
        // The hotkeys category is not offered on narrow windows.
        if narrow && self.settings_ui.category == Category::Hotkeys {
            self.settings_ui.category = Category::Appearance;
        }
        let active = self.settings_ui.category;
        let categories: Vec<Category> = Category::ALL
            .into_iter()
            .filter(|c| !(narrow && *c == Category::Hotkeys))
            .collect();
        let header = div()
            .id("settings-header")
            .test_support()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(rems(0.25))
            .when(narrow, |h| h.px(px(gutter)).py(px(10.)))
            .when(!narrow, |h| h.p(rems(1.5)))
            .bg(color(mode, "--settings-header-bg"))
            .when(narrow, |header| {
                header.bg(color(mode, "--mobile-glass-bg-strong"))
            })
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if !window.default_prevented() {
                    if event.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                }
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.25))
                    .flex_1()
                    .child(
                        control("back", "NavigateBefore", self.t("common.back"), mode, cx)
                            .disabled(!self.navigation.can_back())
                            .size(rems(2.25))
                            .icon(glyph("xiangzuo"))
                            .with_size(gpui_kit::component::Size::Size(window.rem_size() * 1.5))
                            .when(narrow, |button| button.size(px(36.)))
                            .mr(rems(0.5))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.navigate_history(false, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_size(rems(0.875))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(relative(1.2))
                            .text_color(color(mode, "--td-text-color-primary"))
                            .child(self.t("settings.title")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.125))
                    .child(
                        control(
                            "settings",
                            "Settings",
                            self.t("common.titleBar.settings"),
                            mode,
                            cx,
                        )
                        .size(rems(2.25))
                        .icon(glyph("shezhi"))
                        .with_size(gpui_kit::component::Size::Size(window.rem_size() * 1.5))
                        .when(narrow, |button| button.size(px(36.)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_settings(Category::Appearance, cx)
                        })),
                    )
                    .when(!narrow, |controls| {
                        controls
                            .child(
                                div()
                                    .w(px(1.))
                                    .h(px(16.))
                                    .mx(px(3.))
                                    .bg(color(mode, "--td-border-level-1-color")),
                            )
                            .child(self.window_buttons(cx))
                    }),
            );
        let nav_item = |category: Category, cx: &mut Context<Self>| {
            let selected = category == active;
            let label = self.t(category.label_key());
            let description = self.t(category.description_key());
            div()
                .id(("settings-nav", category.index()))
                .flex()
                .items_center()
                .gap(rems(0.75))
                .when(!narrow, |item| {
                    item.px(rems(1.5))
                        // WebKit's line boxes and padding land on whole/half CSS pixels.
                        .pt(px((f32::from(window.rem_size()) * 0.875 * 2.).floor() / 2.))
                        .pb(px((f32::from(window.rem_size()) * 0.875 * 2.).ceil() / 2.))
                        .mt(px(5.))
                        .border_l(px(3.))
                        .rounded(px(5.))
                        .border_color(if selected {
                            color(mode, "--settings-nav-active-border")
                        } else {
                            transparent_black()
                        })
                })
                .when(narrow, |item| {
                    item.flex_shrink_0()
                        .h(px(44.))
                        .px(px(12.))
                        .rounded(px(20.))
                        .border_1()
                        .border_color(if selected {
                            color(mode, "--settings-nav-active-border")
                        } else {
                            color(mode, "--mobile-glass-border")
                        })
                })
                // WebKit's system CJK fallback advances 13.895px at 14px; native shaping
                // otherwise widens every pill and accumulates horizontal drift.
                .when(
                    narrow && !self.english && cfg!(target_os = "macos"),
                    |item| item.w(px(58. + label.chars().count() as f32 * 13.895)),
                )
                .cursor_pointer()
                .bg(if selected {
                    color(mode, "--settings-nav-active-bg")
                } else if narrow {
                    color(mode, "--mobile-glass-bg")
                } else {
                    transparent_black()
                })
                .hover(move |style| {
                    if selected {
                        style
                    } else {
                        style.bg(color(mode, "--settings-nav-hover-bg"))
                    }
                })
                .child(
                    icon(category.icon())
                        .size(px(20.))
                        .flex_shrink_0()
                        .text_color(color(
                            mode,
                            if selected {
                                "--settings-nav-icon-active"
                            } else {
                                "--settings-nav-icon-color"
                            },
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(".SystemUIFont")
                        .when(narrow, |content| content.relative().top(px(-1.5)))
                        .child(
                            div()
                                .text_size(px(f32::from(window.rem_size()) * 0.875
                                    - if !narrow && cfg!(target_os = "macos") {
                                        0.28
                                    } else {
                                        0.
                                    }))
                                .font_weight(if narrow && selected {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .when(!narrow, |label| label.mb(rems(0.125)))
                                .when(narrow, |label| label.whitespace_nowrap())
                                .line_height(px(if narrow {
                                    14.
                                } else {
                                    (f32::from(window.rem_size()) * 0.875 * 1.6).floor()
                                }))
                                .text_color(color(
                                    mode,
                                    if narrow && selected {
                                        "--settings-nav-label-active"
                                    } else {
                                        "--settings-nav-label-color"
                                    },
                                ))
                                .child(label),
                        )
                        .when(!narrow, |content| {
                            content.child(
                                div()
                                    // Match the measured system-fallback em metrics while keeping
                                    // the original line boxes and wrapping width.
                                    .text_size(px(f32::from(window.rem_size()) * 0.75
                                        - if cfg!(target_os = "macos") { 0.28 } else { 0. }))
                                    .line_height(px(
                                        (f32::from(window.rem_size()) * 0.75 * 1.3).floor()
                                    ))
                                    .text_color(color(mode, "--settings-nav-desc-color"))
                                    .child(description),
                            )
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.switch_category(category, cx)))
        };
        let items: Vec<_> = categories.iter().map(|c| nav_item(*c, cx)).collect();
        let sidebar = if narrow {
            div()
                .id("settings-sidebar")
                .test_support()
                .flex_shrink_0()
                .w_full()
                .px(px(gutter))
                .pt(px(6.))
                .pb(px(8.))
                .child(
                    div()
                        .id("settings-pills")
                        .flex()
                        .gap(px(6.))
                        .pt(px(2.))
                        .pb(px(6.))
                        .w_full()
                        .overflow_x_scroll()
                        .track_scroll(&self.settings_ui.pills)
                        .children(items),
                )
        } else {
            div()
                .id("settings-sidebar")
                .test_support()
                .flex_shrink_0()
                .w(px(280.))
                .h_full()
                .px(px(5.))
                .pb(px(5.))
                .bg(color(mode, "--settings-sidebar-bg"))
                .overflow_y_scroll()
                .children(items)
        };
        let content = self.settings_category(active, narrow, window, cx);
        let panel = div()
            .id(("settings-panel", active.index()))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.settings_ui.scrolls[active.index()])
            .bg(color(mode, "--settings-main-bg"))
            .when(narrow, |panel| {
                panel
                    .border_1()
                    .border_color(color(mode, "--mobile-glass-border"))
                    .rounded(px(22.))
                    .p(px(10.))
                    // Original panel padding includes the mobile player and navigation inset.
                    .pb(px(mio_tokens::player_height(f32::from(
                        window.viewport_size().height,
                    )) + 74.
                        + 12.))
                    .bg(color(mode, "--mobile-glass-bg"))
            })
            .child(content);
        // Alt+O switches the A/B output devices while the playback category shows.
        if window.focused(cx).is_none() {
            self.player_focus.focus(window, cx);
        }
        div()
            .id("settings-root")
            .track_focus(&self.player_focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers.alt
                    && event.keystroke.key == "o"
                    && this.settings_ui.category == Category::Playlist
                {
                    this.toggle_audio_ab(window, cx);
                    cx.stop_propagation();
                }
            }))
            .size_full()
            .rounded(px(20.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
            .line_height(relative(1.6))
            .text_color(color(mode, "--td-text-color-primary"))
            .bg(color(mode, "--settings-main-bg"))
            .when(narrow, |root| root.bg(self.compact_page_background()))
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(narrow, |layout| layout.flex_col())
                    .overflow_hidden()
                    .child(sidebar)
                    .child(
                        div()
                            .id("settings-content")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .p(px(if narrow { 0. } else { 10. }))
                            .when(narrow, |c| c.px(px(gutter)))
                            .child(panel),
                    ),
            )
            .into_any_element()
    }
}

// Building blocks shared by the categories.
impl Mio {
    /// `.setting-group`: a bordered card; `title` is its `h3`.
    pub(super) fn setting_group(
        &self,
        id: &'static str,
        title: Option<String>,
        narrow: bool,
    ) -> Stateful<Div> {
        let mode = self.ui_colors();
        div()
            .id(id)
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .p(px(if narrow { 14. } else { 24. }))
            .mb(px(if narrow { 10. } else { 24. }))
            .rounded(px(if narrow { 16. } else { 12. }))
            .border_1()
            .border_color(color(mode, "--settings-group-border"))
            .bg(color(mode, "--settings-group-bg"))
            .shadow(vec![BoxShadow {
                color: color(mode, "--settings-group-shadow"),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }])
            .when_some(title, |group, title| {
                group.child(
                    div()
                        .mb(px(8.))
                        .text_size(px(if narrow { 16. } else { 18. }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .line_height(relative(1.35))
                        .text_color(color(mode, "--td-text-color-primary"))
                        .child(title),
                )
            })
    }

    /// `t-card` with a title (the appearance section's cards).
    pub(super) fn setting_card(
        &self,
        id: &'static str,
        title: String,
        narrow: bool,
    ) -> Stateful<Div> {
        let mode = self.ui_colors();
        div()
            .id(id)
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .rounded(px(if narrow { 16. } else { 6. }))
            .border_1()
            .border_color(color(mode, "--td-border-level-2-color"))
            .bg(color(mode, "--td-bg-color-container"))
            .child(
                div()
                    .px(px(if narrow { 14. } else { 24. }))
                    .pt(px(if narrow { 14. } else { 16. }))
                    .pb(px(if narrow { 8. } else { 16. }))
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(24.))
                    .text_color(color(mode, "--td-text-color-primary"))
                    .child(title),
            )
    }

    /// `.setting-item`: title and description on the left, the control on the right.
    pub(super) fn setting_item(
        &self,
        id: impl Into<ElementId>,
        title: String,
        description: Option<String>,
        control: AnyElement,
        narrow: bool,
    ) -> Stateful<Div> {
        let mode = self.ui_colors();
        div()
            .id(id)
            .w_full()
            .mt(px(12.))
            .flex()
            .when(narrow, |item| item.flex_col().items_start().gap(px(10.)))
            .when(!narrow, |item| {
                item.items_center().justify_between().gap(px(16.))
            })
            .px(px(if narrow { 12. } else { 16. }))
            .py(px(if narrow { 10. } else { 14. }))
            .rounded(px(8.))
            .border_1()
            .border_color(color(mode, "--settings-feature-border"))
            .bg(color(mode, "--settings-feature-bg"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(15.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(1.2))
                            .text_color(color(mode, "--settings-text-primary"))
                            .child(title),
                    )
                    .when_some(description, |info, description| {
                        info.child(
                            div()
                                .text_size(px(12.8))
                                .line_height(relative(1.2))
                                .text_color(color(mode, "--settings-text-secondary"))
                                .child(description),
                        )
                    }),
            )
            .child(div().flex_shrink_0().child(control))
    }

    /// The label above a control inside a card (`.setting-label` of the appearance section).
    pub(super) fn setting_label(&self, title: String, description: String, narrow: bool) -> Div {
        let mode = self.ui_colors();
        div()
            .mb(px(if narrow { 12. } else { 16. }))
            .child(
                div()
                    .mb(px(4.))
                    .text_size(px(14.))
                    .line_height(px(if narrow { 18. } else { 22. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(mode, "--td-text-color-primary"))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(px(if narrow { 17. } else { 22. }))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(description),
            )
    }
}

impl Mio {
    /// Fills the desktop lyric controls from the saved style; once at startup.
    pub(super) fn sync_desktop_lyric_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let style = self.settings_desktop_style();
        let weight = DESKTOP_WEIGHTS
            .iter()
            .position(|w| *w == style.font_weight)
            .unwrap_or(3);
        let position = POSITIONS
            .iter()
            .position(|p| *p == style.position)
            .unwrap_or(1);
        let color_text = style.main_color.clone();
        self.settings_ui.desktop_style = style;
        let ui = &self.settings_ui;
        ui.desktop_weight.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::default().row(weight)), window, cx)
        });
        ui.desktop_position.update(cx, |state, cx| {
            state.set_selected_index(Some(IndexPath::default().row(position)), window, cx)
        });
        ui.desktop_color
            .update(cx, |state, cx| state.set_value(color_text, window, cx));
    }
}
