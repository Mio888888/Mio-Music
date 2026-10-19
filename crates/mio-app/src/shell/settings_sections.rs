//! What each category of the settings page holds (`views/settings/sections/*.vue`). Groups of
//! features the app does not have yet are drawn with their original title and a note instead of
//! controls that would do nothing.
use super::settings_page::Category;
use super::*;
use gpui_kit::base::{ElementExt as _, StyledExt as _};
use gpui_kit::component::switch::Switch;

/// The settings preview of `formatMusicInfo`: source `navidrome`, quality `master` and a fixed
/// date stand in for the fields a song would have.
pub(super) fn format_music_info(template: &str, title: &str, artist: &str, album: &str) -> String {
    crate::download::format_template(
        template,
        title,
        artist,
        album,
        "navidrome",
        "master",
        "2026-01-01",
    )
}

impl Mio {
    pub(super) fn settings_category(
        &mut self,
        category: Category,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if category != Category::Playlist {
            // The devices are listed again the next time the playback category opens.
            self.audio_output.dirty = true;
        }
        if category != Category::Storage {
            // The folders are measured again the next time the storage category opens.
            self.storage_info.dirty = true;
        }
        let groups: Vec<AnyElement> = match category {
            Category::Appearance => self.appearance_groups(narrow, cx),
            Category::Playlist => self.playback_groups(narrow, window, cx),
            Category::Effects => self.effects_groups(narrow, window, cx),
            Category::Hotkeys => self.hotkey_groups(narrow, cx),
            Category::Plugins => self.plugin_groups(narrow, cx),
            Category::Music => self.music_groups(narrow, window, cx),
            Category::Storage => self.storage_groups(narrow, window, cx),
            Category::About => self.about_groups(narrow, cx),
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .children(groups)
            .into_any_element()
    }

    /// A circular "-" / "+" stepper with the current value between them.
    fn stepper(
        &self,
        id: &'static str,
        value: String,
        narrow: bool,
        can_decrease: bool,
        can_increase: bool,
        on_change: impl Fn(&mut Mio, i32, &mut Context<Mio>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = self.ui_colors();
        let on_change = Rc::new(on_change);
        let button = |label: &'static str, delta: i32, enabled: bool, cx: &mut Context<Self>| {
            let on_change = on_change.clone();
            gpui_kit::base::Button::new(SharedString::from(format!("{id}-{label}")))
                .accessibility_label(label)
                .w(px(32.))
                .h(px(if narrow { 44. } else { 32. }))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(color(mode, "--td-border-level-2-color"))
                .bg(color(mode, "--td-bg-color-specialcomponent"))
                .text_size(px(16.))
                .text_color(color(mode, "--td-text-color-primary"))
                .hover(|style| style.bg(color(mode, "--td-bg-color-component-hover")))
                .when(!enabled, |button| button.opacity(0.4))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if enabled {
                        on_change(this, delta, cx);
                    }
                }))
        };
        div()
            .flex()
            .items_center()
            .gap(px(if narrow { 0. } else { 8. }))
            .when(narrow, |stepper| stepper.w_full())
            .child(button("-", -1, can_decrease, cx))
            .child(
                div()
                    .min_w(px(48.))
                    .h(px(if narrow { 44. } else { 32. }))
                    .when(narrow, |value| value.flex_1())
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(color(mode, "--td-border-level-2-color"))
                    .bg(color(mode, "--td-bg-color-specialcomponent"))
                    .text_size(px(14.))
                    .child(value),
            )
            .child(button("+", 1, can_increase, cx))
            .into_any_element()
    }

    /// `.setting-item` of `LyricFontSettings`: rem padding, 22px lines, page-coloured box. The
    /// control keeps its own width, or takes the row (`wide`) and squeezes the text like the
    /// original's select does.
    fn lyric_font_item(
        &self,
        id: &'static str,
        title: String,
        description: Option<String>,
        control: AnyElement,
        wide: bool,
        narrow: bool,
    ) -> Stateful<Div> {
        let mode = self.ui_colors();
        div()
            .id(id)
            .w_full()
            .mt(px(if narrow && id.starts_with("lyric-font-") {
                10.
            } else {
                12.
            }))
            .flex()
            .when(narrow, |item| item.flex_col().items_start().gap(px(8.)))
            .when(!narrow, |item| item.items_center().justify_between())
            .px(if narrow {
                AbsoluteLength::from(px(12.))
            } else {
                rems(1.).into()
            })
            // Layout snaps the 14.7px padding to half pixels (14.5), a row 0.4px short of the
            // browser's; the bottom side takes the next half pixel to land on its 79.6 / 53.4.
            .pt(if narrow { px(10.) } else { px(14.5) })
            .pb(if narrow { px(10.) } else { px(15.) })
            .rounded(if narrow {
                AbsoluteLength::from(px(16.))
            } else {
                rems(0.5).into()
            })
            .border_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .bg(color(mode, "--td-bg-color-page"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.25))
                    .min_w_0()
                    // The original multi-select leaves this label column before
                    // filling the rest of the desktop row with its popup anchor.
                    .when(wide && !narrow, |info| info.min_w(px(129.5)))
                    .when(narrow, |info| info.w_full())
                    .child(
                        div()
                            .text_size(if narrow {
                                AbsoluteLength::from(px(14.))
                            } else {
                                rems(0.95).into()
                            })
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(if narrow { 18. } else { 22. }))
                            .text_color(color(mode, "--td-text-color-primary"))
                            .child(title),
                    )
                    .when_some(description, |info, description| {
                        info.child(
                            div()
                                .text_size(if narrow {
                                    AbsoluteLength::from(px(12.))
                                } else {
                                    rems(0.8).into()
                                })
                                .line_height(px(if narrow { 17. } else { 22. }))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(description),
                        )
                    }),
            )
            .child(
                div()
                    .when(narrow || wide, |slot| slot.w_full().min_w_0())
                    .when(!wide || narrow, |slot| slot.flex_shrink_0())
                    .child(control),
            )
    }

    fn divider(&self) -> Div {
        div()
            .w_full()
            .h(px(1.))
            .my(px(24.))
            .bg(color(self.ui_colors(), "--td-border-level-1-color"))
    }

    /// `.mock-titlebar` with a `TitleBarControls`: the title wraps in a 6.08rem column next to the
    /// controls, which are 2.25rem buttons (settings, a separator, then the window buttons).
    fn mock_title_bar(&self, traffic_light: bool, title: String) -> Div {
        let mode = self.ui_colors();
        let secondary = color(mode, "--td-text-color-secondary");
        let button = |child: AnyElement| {
            div()
                .size(rems(2.25))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .child(child)
        };
        let glyph_button = |name: &'static str, size: f32| {
            button(
                glyph(name)
                    .size(rems(size))
                    .text_color(secondary)
                    .into_any_element(),
            )
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .px(rems(1.))
            .py(rems(0.75))
            .rounded(px(6.))
            .border_1()
            .border_color(color(mode, "--td-border-level-1-color"))
            .bg(color(mode, "--td-bg-color-container"))
            .child(
                div()
                    .w(rems(6.083))
                    .flex_shrink_0()
                    .text_size(rems(0.884))
                    .line_height(px(22.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color(mode, "--td-text-color-primary"))
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.125))
                    .child(glyph_button("shezhi", 1.125))
                    .child(
                        div()
                            .w(px(1.))
                            .h(rems(1.))
                            .mx(px(3.))
                            .rounded(px(2.))
                            .bg(color(mode, "--td-border-level-1-color")),
                    )
                    .when(!traffic_light, |bar| {
                        bar.child(glyph_button("zuixiaohua", 1.125))
                            .child(glyph_button("a-tingzhiwukuang", 1.125))
                            .child(glyph_button("a-quxiaoguanbi", 1.125))
                    })
                    .when(traffic_light, |bar| {
                        bar.children([0xffbd2e_u32, 0x28ca42, 0xff5f57].map(|c| {
                            button(
                                div()
                                    .size(rems(1.))
                                    .rounded(px(999.))
                                    .bg(rgb(c))
                                    .into_any_element(),
                            )
                        }))
                    }),
            )
    }

    fn lyric_font_selector(&self, narrow: bool, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        let owner = cx.entity().downgrade();
        let english = self.english;
        let input_state = self.settings_ui.lyric_family.downgrade();
        let input = Input::new(&self.settings_ui.lyric_family)
            .px(px(4.))
            .py(px(3.))
            .on_token_click(move |_, window, cx| {
                // Selected fonts behave as select tags, not replaceable rich-text objects.
                let _ = input_state.update(cx, |state, cx| {
                    let end = state.value().len();
                    state.set_selected_range(end..end, cx);
                    state.focus(window, cx);
                });
            })
            .token(move |token, _, _| {
                let font = token.token().label().to_string();
                let owner = owner.clone();
                let tag = div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .h(px(24.))
                    .rounded(px(3.))
                    .bg(color(mode, "--td-bg-color-component"))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .child(font.clone())
                    .child(
                        gpui_kit::base::Button::new("remove-font")
                            .accessibility_label(format!(
                                "{}: {font}",
                                i18n::text(english, "common.delete")
                            ))
                            .size(px(14.))
                            .child(
                                icon("TD-Close")
                                    .size(px(14.))
                                    .text_color(color(mode, "--td-text-color-secondary")),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    let value = &this.app_settings.lyric_font_family;
                                    let next = value
                                        .split(',')
                                        .map(str::trim)
                                        .filter(|name| *name != font)
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    this.set_lyric_families(&next, window, cx);
                                });
                                cx.stop_propagation();
                            }),
                    );
                // TDesign keeps tags at the first 24px row even in a 44px touch target.
                div()
                    .h(px(if narrow { 36. } else { 24. }))
                    .child(tag.flex_shrink_0())
            })
            .suffix(
                gpui_kit::base::Button::new("open-font-options")
                    .focusable(false)
                    .accessibility_label(self.t("settings.lyricFont.selectFont"))
                    .size(px(16.))
                    .child(
                        icon("TD-Down")
                            .size(px(16.))
                            .text_color(color(mode, "--td-text-color-secondary")),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.set_font_menu_open(!this.settings_ui.font_menu_open, cx);
                        this.settings_ui
                            .lyric_family
                            .update(cx, |state, cx| state.focus(window, cx));
                    })),
            )
            .bg(color(mode, "--td-bg-color-specialcomponent"))
            .border_color(color(mode, "--td-border-level-2-color"))
            .focus_bordered(false)
            .when(self.settings_ui.font_menu_open, |input| {
                input
                    .border_color(color(mode, "--td-brand-color"))
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(0.), color(mode, "--td-brand-color-focus"))
                            .spread_radius(px(2.)),
                    ])
            })
            .text_size(px(14.))
            .line_height(px(if narrow { 36. } else { 24. }))
            .id("lyric-font-family-input")
            .h(px(if narrow { 44. } else { 32. }))
            .when(narrow, |input| input.min_h(px(44.)))
            .rounded(px(3.))
            .aria_label(self.t("settings.lyricFont.lyricFont"));
        let bounds = self.settings_ui.font_menu_bounds.clone();
        div()
            .id("lyric-font-selector")
            .relative()
            .w_full()
            .on_prepaint(move |rect, _, _| bounds.set(rect))
            .on_click(cx.listener(|this, _, _, cx| this.set_font_menu_open(true, cx)))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if matches!(key, "up" | "down") {
                    this.set_font_menu_open(true, cx);
                    let count = this.font_menu_options(cx).len();
                    if count > 0 {
                        let cursor = match (this.settings_ui.font_menu_cursor, key) {
                            (None, _) => 0,
                            (Some(0), "up") => count - 1,
                            (Some(i), "up") => i.min(count) - 1,
                            (Some(i), _) => (i + 1) % count,
                        };
                        this.settings_ui.font_menu_cursor = Some(cursor);
                        let top = cursor as f32 * 30.;
                        let offset = -f32::from(this.settings_ui.font_menu_scroll.offset().y);
                        if top < offset || top + 28. > offset + 288. {
                            let next = if top < offset { top } else { top + 28. - 288. };
                            this.settings_ui
                                .font_menu_scroll
                                .set_offset(point(px(0.), px(-next)));
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                } else if this.settings_ui.font_menu_open && key == "enter" {
                    if let Some(font) = this
                        .settings_ui
                        .font_menu_cursor
                        .and_then(|i| this.font_menu_options(cx).get(i).cloned())
                    {
                        this.toggle_menu_font(&font, window, cx);
                        cx.stop_propagation();
                    }
                } else if this.settings_ui.font_menu_open && matches!(key, "escape" | "tab") {
                    this.set_font_menu_open(false, cx);
                    let value = this.app_settings.lyric_font_family.clone();
                    this.settings_ui.lyric_family.update(cx, |state, cx| {
                        state.set_value(settings_page::font_input_content(&value), window, cx);
                    });
                    if key == "escape" {
                        cx.stop_propagation();
                    }
                }
            }))
            .child(input)
            .when(self.settings_ui.font_menu_open, |root| {
                let style = self.settings_dropdown_style();
                let bounds = self.settings_ui.font_menu_bounds.get();
                let fonts = self.font_menu_options(cx);
                root.child(
                    deferred(
                        gpui_kit::base::Positioner::side(bounds)
                            .placement(gpui_kit::base::Placement::Bottom)
                            .align(gpui_kit::base::Align::Start)
                            .offset(style.offset)
                            .margin(px(8.))
                            .child(
                                div()
                                    .id("lyric-font-menu")
                                    .occlude()
                                    .w(bounds.size.width)
                                    .max_h(px(300.))
                                    .refine_style(&style.surface)
                                    .p(style.padding)
                                    .child(
                                        div()
                                            .id("lyric-font-options-list")
                                            .flex()
                                            .flex_col()
                                            .gap(style.spacing)
                                            .max_h(px(288.))
                                            .overflow_y_scroll()
                                            .track_scroll(&self.settings_ui.font_menu_scroll)
                                            .children(fonts.into_iter().enumerate().map(
                                                |(index, font)| {
                                                    let selected = self
                                                        .app_settings
                                                        .lyric_font_family
                                                        .split(',')
                                                        .any(|f| f.trim() == font);
                                                    let hovered = self.settings_ui.font_menu_cursor
                                                        == Some(index);
                                                    let hover_style = style.hover.clone();
                                                    gpui_kit::base::Button::new((
                                                        "font-option",
                                                        index,
                                                    ))
                                                    .focusable(false)
                                                    .w_full()
                                                    .flex()
                                                    .items_center()
                                                    .justify_start()
                                                    .gap(px(8.))
                                                    .refine_style(&style.item)
                                                    .when(selected, |row| {
                                                        row.refine_style(&style.selected)
                                                    })
                                                    .when(!selected && hovered, |row| {
                                                        row.refine_style(&style.hover)
                                                    })
                                                    .hover(move |row| {
                                                        if selected {
                                                            row
                                                        } else {
                                                            row.refine_style(&hover_style)
                                                        }
                                                    })
                                                    .child(
                                                        div()
                                                            .size(px(16.))
                                                            .flex_shrink_0()
                                                            .rounded(px(3.))
                                                            .border_1()
                                                            .border_color(color(
                                                                mode,
                                                                if selected {
                                                                    "--td-brand-color"
                                                                } else {
                                                                    "--td-border-level-2-color"
                                                                },
                                                            ))
                                                            .when(selected, |box_| {
                                                                box_.bg(color(
                                                                    mode,
                                                                    "--td-brand-color",
                                                                ))
                                                                .child(
                                                                    icon("TD-Check")
                                                                        .size(px(14.))
                                                                        .text_color(white()),
                                                                )
                                                            }),
                                                    )
                                                    .child(font.clone())
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        |_, window, _| window.prevent_default(),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.toggle_menu_font(
                                                                &font, window, cx,
                                                            );
                                                        },
                                                    ))
                                                },
                                            )),
                                    )
                                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                                        this.set_font_menu_open(false, cx);
                                        let value = this.app_settings.lyric_font_family.clone();
                                        this.settings_ui.lyric_family.update(cx, |state, cx| {
                                            state.set_value(
                                                settings_page::font_input_content(&value),
                                                window,
                                                cx,
                                            )
                                        });
                                    })),
                            ),
                    )
                    .with_priority(gpui_kit::base::POPUP_PRIORITY),
                )
            })
            .into_any_element()
    }

    pub(super) fn set_font_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.settings_ui.font_menu_open != open {
            self.settings_ui.font_menu_open = open;
            self.settings_ui.font_menu_cursor = None;
            self.settings_ui
                .font_menu_scroll
                .set_offset(point(px(0.), px(0.)));
            self.settings_ui.font_menu_context =
                open.then(|| gpui_kit::base::GlobalState::register_deferred_popover(cx));
            cx.notify();
        }
    }

    fn font_menu_options(&self, cx: &App) -> Vec<String> {
        let content = self.settings_ui.lyric_family.read(cx).content();
        let mut query = String::new();
        let mut offset = 0;
        for span in content.tokens() {
            let range = span.range();
            query.push_str(&content.text()[offset..range.start]);
            offset = range.end;
        }
        query.push_str(&content.text()[offset..]);
        let query = query.trim();
        let mut fonts = [
            "lyricfont",
            "PingFang SC",
            "Helvetica Neue",
            "Arial",
            "Microsoft YaHei",
            "SimHei",
            "SimSun",
            "KaiTi",
            "FangSong",
            "STHeiti",
            "STKaiti",
            "STSong",
            "STFangsong",
        ]
        .into_iter()
        .filter(|f| f.to_lowercase().contains(&query.to_lowercase()))
        .map(str::to_owned)
        .collect::<Vec<_>>();
        if !query.is_empty() && !fonts.iter().any(|f| f == query) {
            fonts.insert(0, query.to_owned());
        }
        fonts
    }

    fn toggle_menu_font(&mut self, font: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut fonts = self
            .app_settings
            .lyric_font_family
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if fonts.iter().any(|f| f == font) {
            fonts.retain(|f| f != font);
        } else {
            fonts.push(font.to_owned());
        }
        self.set_lyric_families(&fonts.join(", "), window, cx);
        self.settings_ui
            .lyric_family
            .update(cx, |state, cx| state.focus(window, cx));
    }

    pub(super) fn set_lyric_families(
        &mut self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = settings_page::normalize_lyric_families(value);
        let value = if value.is_empty() {
            "lyricfont".to_owned()
        } else {
            value
        };
        self.settings_ui.lyric_family.update(cx, |state, cx| {
            state.set_value(settings_page::font_input_content(&value), window, cx);
        });
        self.app_settings.lyric_font_family = value;
        self.save_preferences();
        cx.notify();
    }

    fn appearance_groups(&mut self, narrow: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let traffic_light = self.app_settings.traffic_light;
        let style_button =
            |id: &'static str, key: &str, selected: bool, light: bool, cx: &mut Context<Self>| {
                let button = Button::new(id)
                    .h(px(32.))
                    .px(px(16.))
                    .accessibility_label(self.t(key))
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(px(22.))
                            .font_weight(FontWeight::NORMAL)
                            .child(self.t(key)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.app_settings.traffic_light = light;
                        this.save_preferences();
                        cx.notify();
                    }));
                if selected {
                    button.primary()
                } else {
                    button
                        .custom(
                            gpui_kit::component::button::ButtonCustomVariant::new(cx)
                                .color(color(mode, "--td-bg-color-component"))
                                .foreground(color(mode, "--td-text-color-primary")),
                        )
                        .bg(color(mode, "--td-bg-color-component"))
                }
            };
        let basic = self
            .setting_card("appearance-card", self.t("settings.appearance.title"), narrow)
            .test_support()
            .child(
                div()
                    .px(px(if narrow { 14. } else { 24. }))
                    .pb(px(if narrow { 14. } else { 16. }))
                    // `.t-card__body` starts 16px below the title row on desktop.
                    .pt(px(if narrow { 8. } else { 16. }))
                    .child(
                        div()
                            .id("appearance-language")
                            .test_support()
                            .when(narrow, |group| group.mb(px(16.)))
                            .child(self.setting_label(
                                self.t("settings.language"),
                                self.t("settings.languageDesc"),
                                narrow,
                            ))
                            .child(
                                div().max_w(px(260.)).child(
                                    self.settings_select(&self.settings_ui.language)
                                        .h(px(if narrow { 44. } else { 32. }))
                                        .accessibility_label(self.t("settings.language"))
                                        .menu_width(px(260.))
                                        .w_full(),
                                ),
                            ),
                    )
                    .child(self.lyric_font_item(
                        "appearance-cover-theme",
                        self.t("settings.appearance.coverDynamicTheme"),
                        Some(self.t("settings.appearance.coverDynamicThemeDesc")),
                        Switch::new("cover-dynamic-theme")
                            .accessibility_label(self.t("settings.appearance.coverDynamicTheme"))
                            .checked(self.app_settings.cover_dynamic_theme)
                            .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                                this.app_settings.cover_dynamic_theme = *enabled;
                                this.apply_theme(cx);
                                this.save_preferences();
                                cx.notify();
                            }))
                            .into_any_element(),
                        false,
                        narrow,
                    ))
                    .when(narrow, |body| {
                        body.child(self.divider().mt(px(0.)).mb(px(12.)))
                    })
                    .when(!narrow, |body| {
                        body.child(self.divider())
                            .child(
                                div()
                                    .id("appearance-titlebar")
                                    .test_support()
                                    .child(self.setting_label(
                                        self.t("settings.appearance.titleBarStyle"),
                                        self.t("settings.appearance.titleBarStyleDesc"),
                                narrow,
                                    ))
                                    .child(
                                        div()
                                            .id("appearance-buttons")
                                            .test_support()
                                            .flex()
                                            .gap(rems(0.75))
                                            .mb(rems(1.5))
                                            .child(style_button(
                                                "titlebar-windows",
                                                "settings.appearance.windowsStyle",
                                                !traffic_light,
                                                false,
                                                cx,
                                            ))
                                            .child(style_button(
                                                "titlebar-traffic-light",
                                                "settings.appearance.trafficLightStyle",
                                                traffic_light,
                                                true,
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("appearance-preview")
                                            .test_support()
                                            .flex()
                                            .gap(rems(1.))
                                            .children([false, true].map(|light| {
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .p(rems(1.))
                                                    .rounded(px(8.))
                                                    .border_1()
                                                    .border_color(color(
                                                        mode,
                                                        "--td-border-level-1-color",
                                                    ))
                                                    .bg(color(mode, "--td-bg-color-page"))
                                                    .child(
                                                        div()
                                                            .mb(rems(0.75))
                                                            .text_size(rems(0.875))
                                                            .line_height(px(22.))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .relative()
                                                            .top(px(-0.5))
                                                            .child(self.t(if light {
                                                                "settings.appearance.trafficLightPreviewTitle"
                                                            } else {
                                                                "settings.appearance.windowsPreviewTitle"
                                                            })),
                                                    )
                                                    .child(self.mock_title_bar(
                                                        light,
                                                        self.t(if light {
                                                            "settings.appearance.trafficLightPreviewBar"
                                                        } else {
                                                            "settings.appearance.windowsPreviewBar"
                                                        }),
                                                    ))
                                            })),
                                    ),
                            )
                            .child(self.divider())
                            .child(
                                div()
                                    .id("appearance-close-behavior")
                                    .test_support()
                                    .child(self.setting_label(
                                        self.t("settings.appearance.closeBehavior"),
                                        self.t("settings.appearance.closeBehaviorDesc"),
                                narrow,
                                    ))
                                    .child(self.close_behavior_control(cx)),
                            )
                    }),
            );
        let rate = self.app_settings.lyric_font_rate;
        let weight = self.app_settings.lyric_font_weight;
        let family = self.app_settings.lyric_font_family.clone();
        let preview_family: SharedString = match family
            .split(',')
            .map(str::trim)
            .find(|f| !f.is_empty())
            .unwrap_or("lyricfont")
        {
            "lyricfont" => "Alimama FangYuanTi VF".into(),
            other => other.to_string().into(),
        };
        let lyric_font = self
            .setting_card(
                "appearance-lyric-font",
                self.t("settings.lyricFont.title"),
                narrow,
            )
            .test_support()
            .child(
                div()
                    .px(px(if narrow { 14. } else { 24. }))
                    .pb(px(if narrow { 14. } else { 16. }))
                    // `.t-card__body` starts 16px below the title row on desktop.
                    .pt(px(if narrow { 8. } else { 16. }))
                    .flex()
                    .flex_col()
                    .child(
                        self.lyric_font_item(
                            "lyric-font-family",
                            self.t("settings.lyricFont.lyricFont"),
                            Some(self.t("settings.lyricFont.lyricFontDesc")),
                            div()
                                .w_full()
                                .child(self.lyric_font_selector(narrow, cx))
                                .into_any_element(),
                            true,
                            narrow,
                        )
                        .mt(px(0.))
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "lyric-font-rate",
                            self.t("settings.lyricFont.fontRate"),
                            Some(format!("{rate:.1}x")),
                            self.stepper(
                                "lyric-rate",
                                format!("{rate:.1}"),
                                narrow,
                                rate > 0.1 + 1e-3,
                                rate < 2. - 1e-3,
                                |this, delta, cx| {
                                    let next = (this.app_settings.lyric_font_rate * 10.).round()
                                        / 10.
                                        + delta as f32 * 0.1;
                                    this.app_settings.lyric_font_rate =
                                        ((next * 10.).round() / 10.).clamp(0.1, 2.);
                                    this.save_preferences();
                                    cx.notify();
                                },
                                cx,
                            ),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "lyric-font-weight",
                            self.t("settings.lyricFont.fontWeight"),
                            Some(weight.to_string()),
                            div()
                                .w(px(200.))
                                .when(narrow, |slider| slider.h(px(12.)).flex().items_center())
                                .child(playback_slider(
                                    &self.settings_ui.lyric_weight,
                                    &self.settings_ui.lyric_weight_pressed,
                                    false,
                                    mode,
                                    cx,
                                ))
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        div()
                            .id("lyric-font-preview")
                            .test_support()
                            .mt(px(if narrow { 10. } else { 12. }))
                            .p(if narrow {
                                AbsoluteLength::from(px(12.))
                            } else {
                                rems(1.).into()
                            })
                            .rounded(rems(0.5))
                            .border_1()
                            .border_color(color(mode, "--td-border-level-1-color"))
                            .bg(color(mode, "--td-bg-color-page"))
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .mb(rems(0.5))
                                    .text_size(rems(0.75))
                                    .line_height(px(22.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(self.t("settings.lyricFont.fontPreview")),
                            )
                            .child(
                                div()
                                    .text_color(color(mode, "--td-text-color-primary"))
                                    .font_family(preview_family)
                                    .font_weight(FontWeight(weight as f32))
                                    .text_size(px(if narrow { 32. } else { (36. * rate).round() }))
                                    .line_height(px(if narrow {
                                        44.
                                    } else {
                                        (36. * rate).round() * 1.4 - 0.9
                                    }))
                                    .child(self.t("settings.lyricFont.fontPreviewText")),
                            ),
                    ),
            );
        let style = self.settings_ui.desktop_style.clone();
        let desktop_lyric = self
            .setting_card(
                "appearance-desktop-lyric",
                self.t("settings.desktopLyric.title"),
                narrow,
            )
            .test_support()
            .child(
                div()
                    .px(px(if narrow { 14. } else { 24. }))
                    .pb(px(if narrow { 14. } else { 16. }))
                    // `.t-card__body` starts 16px below the title row on desktop.
                    .pt(px(if narrow { 8. } else { 16. }))
                    .flex()
                    .flex_col()
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-switch",
                            self.t("settings.desktopLyric.lyricSwitch"),
                            Some(self.t("settings.desktopLyric.lyricSwitchDesc")),
                            Switch::new("desktop-lyric-open")
                                .accessibility_label(self.t("settings.desktopLyric.lyricSwitch"))
                                .checked(self.desktop_lyric.is_some())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_desktop_lyric(window.scale_factor(), cx);
                                }))
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .mt(px(0.))
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-size",
                            self.t("settings.desktopLyric.fontSize"),
                            Some(format!("{}px", style.font_size.round() as i32)),
                            self.stepper(
                                "desktop-lyric-size-step",
                                (style.font_size.round() as i32).to_string(),
                                narrow,
                                style.font_size > 12.,
                                style.font_size < 96.,
                                |this, delta, cx| {
                                    this.change_desktop_lyric_option(
                                        |o| {
                                            o.font_size =
                                                (o.font_size.round() + delta as f32).clamp(12., 96.)
                                        },
                                        cx,
                                    );
                                },
                                cx,
                            ),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-weight",
                            self.t("settings.desktopLyric.fontWeight"),
                            None,
                            div()
                                .w(px(120.))
                                .child(
                                    self.settings_select(&self.settings_ui.desktop_weight)
                                        .h(px(if narrow { 44. } else { 32. }))
                                        .accessibility_label(
                                            self.t("settings.desktopLyric.fontWeight"),
                                        )
                                        .menu_width(px(120.))
                                        .w_full(),
                                )
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-color",
                            self.t("settings.desktopLyric.lyricColor"),
                            None,
                            div()
                                .flex()
                                .items_center()
                                .when(narrow, |control| control.w_full())
                                .gap(px(8.))
                                .child(
                                    div()
                                        .size(px(24.))
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(color(mode, "--td-border-level-2-color"))
                                        .bg(crate::shell::desktop_lyric::style_color(
                                            &style.main_color,
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(140.))
                                        .when(narrow, |input| input.flex_1())
                                        .child(
                                            Input::new(&self.settings_ui.desktop_color)
                                                .bg(color(mode, "--td-bg-color-specialcomponent"))
                                                .border_color(color(
                                                    mode,
                                                    "--td-border-level-2-color",
                                                ))
                                                .text_size(px(14.))
                                                .line_height(px(22.))
                                                .id("desktop-lyric-color-input")
                                                .h(px(if narrow { 44. } else { 32. }))
                                                .when(narrow, |input| input.min_h(px(44.)))
                                                .rounded(px(3.))
                                                .aria_label(
                                                    self.t("settings.desktopLyric.lyricColor"),
                                                ),
                                        ),
                                )
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-position",
                            self.t("settings.desktopLyric.lyricPosition"),
                            None,
                            div()
                                .w(px(120.))
                                .child(
                                    self.settings_select(&self.settings_ui.desktop_position)
                                        .h(px(if narrow { 44. } else { 32. }))
                                        .accessibility_label(
                                            self.t("settings.desktopLyric.lyricPosition"),
                                        )
                                        .menu_width(px(120.))
                                        .w_full(),
                                )
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-yrc",
                            self.t("settings.desktopLyric.showYrc"),
                            Some(self.t("settings.desktopLyric.showYrcDesc")),
                            Switch::new("desktop-lyric-show-yrc")
                                .accessibility_label(self.t("settings.desktopLyric.showYrc"))
                                .checked(style.show_yrc)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    let checked = *checked;
                                    this.change_desktop_lyric_option(|o| o.show_yrc = checked, cx);
                                }))
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    )
                    .child(
                        self.lyric_font_item(
                            "desktop-lyric-tran",
                            self.t("settings.desktopLyric.showTran"),
                            None,
                            Switch::new("desktop-lyric-show-tran")
                                .accessibility_label(self.t("settings.desktopLyric.showTran"))
                                .checked(style.show_tran)
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    let checked = *checked;
                                    this.change_desktop_lyric_option(|o| o.show_tran = checked, cx);
                                }))
                                .into_any_element(),
                            false,
                            narrow,
                        )
                        .test_support(),
                    ),
            );
        // The cards are observed under the `parity` feature, which changes their type.
        fn gap<E: Styled + IntoElement>(content: E, margin: f32) -> AnyElement {
            content.mb(px(margin)).into_any_element()
        }
        vec![
            gap(basic, if narrow { 20. } else { 24. }),
            gap(lyric_font, if narrow { 10. } else { 24. }),
            gap(desktop_lyric, if narrow { 0. } else { 24. }),
        ]
    }

    fn storage_groups(
        &mut self,
        narrow: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let template = self.settings_ui.filename.read(cx).value().to_string();
        let preview = format_music_info(
            &template,
            &self.t("settings.storage.templateSongName"),
            &self.t("settings.storage.templateSinger"),
            &self.t("settings.storage.templateAlbum"),
        );
        let tag = |text: &str| {
            div()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(color(mode, "--td-bg-color-secondarycontainer"))
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .text_size(px(12.))
                .child(text.to_string())
        };
        let template_tips = div().flex().flex_wrap().gap(px(12.)).mb(px(12.)).children(
            [
                ("%t", "settings.storage.templateSongName"),
                ("%s", "settings.storage.templateSinger"),
                ("%a", "settings.storage.templateAlbum"),
                ("%u", "settings.storage.templatePlatform"),
                ("%q", "settings.storage.templateQuality"),
                ("%d", "settings.storage.templateDate"),
            ]
            .map(|(token, key)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .child(tag(token))
                    .child(self.t(key))
            }),
        );
        let mut groups = self.storage_directory_groups(narrow, cx);
        groups.extend(vec![
            self.setting_group(
                "storage-filename",
                Some(self.t("settings.storage.filenameFormat")),
                narrow,
            )
            .child(
                div()
                    .mb(px(12.))
                    .text_size(px(13.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(self.t("settings.storage.filenameFormatDesc")),
            )
            .child(template_tips)
            .child(
                Input::new(&self.settings_ui.filename)
                    .h(px(32.))
                    .rounded(px(3.)),
            )
            .child(
                div()
                    .mt(px(12.))
                    .p(px(12.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(color(mode, "--settings-preview-border"))
                    .bg(color(mode, "--settings-preview-bg"))
                    .text_size(px(13.))
                    .child(div().child(self.t("settings.storage.preview")))
                    .child(div().child(preview)),
            )
            .into_any_element(),
            self.tag_settings(narrow, cx).into_any_element(),
        ]);
        groups
    }

    fn about_groups(&mut self, narrow: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mode = self.ui_colors();
        let description = |text: String| {
            div()
                .mt(px(4.))
                .text_size(px(13.6))
                .line_height(relative(1.6))
                .text_color(color(mode, "--td-text-color-secondary"))
                .child(text)
        };
        let notice = |title: String, text: String| {
            div()
                .mt(px(12.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(description(text))
        };
        let intro_template = self.t("settings.about.aboutIntro");
        let link = "CeruMusic";
        let author = "时迁酱";
        let (intro, links) = {
            let mut text = String::new();
            let mut ranges: Vec<(std::ops::Range<usize>, &'static str)> = Vec::new();
            let mut rest = intro_template.as_str();
            let urls = [
                Some("https://github.com/timeshiftsauce/CeruMusic"),
                None,
                Some("https://ceru.docs.shiqianjiang.cn/"),
            ];
            let values = [link, author, link];
            for (index, (value, url)) in values.into_iter().zip(urls).enumerate() {
                let marker = format!("{{{index}}}");
                if let Some(at) = rest.find(&marker) {
                    text.push_str(&rest[..at]);
                    let start = text.len();
                    text.push_str(value);
                    if let Some(url) = url {
                        ranges.push((start..text.len(), url));
                    }
                    rest = &rest[at + marker.len()..];
                }
            }
            text.push_str(rest);
            (text, ranges)
        };
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = links
            .iter()
            .map(|(range, _)| {
                (
                    range.clone(),
                    HighlightStyle {
                        color: Some(color(mode, "--td-brand-color")),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let click_ranges: Vec<std::ops::Range<usize>> =
            links.iter().map(|(range, _)| range.clone()).collect();
        let urls: Vec<&'static str> = links.iter().map(|(_, url)| *url).collect();
        let support_card = |id: &'static str,
                            title: String,
                            text: String,
                            points: Vec<String>,
                            qr: AnyElement,
                            caption: String| {
            div()
                .id(id)
                .mt(px(16.))
                .p(px(16.))
                .flex()
                .when(narrow, |card| card.flex_col())
                .gap(px(16.))
                .rounded(px(12.))
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
                .bg(color(mode, "--td-bg-color-page"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(description(text))
                        .children(points.into_iter().map(|point| {
                            div()
                                .mt(px(4.))
                                .text_size(px(13.))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(format!("· {point}"))
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.))
                        .child(qr)
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(caption),
                        ),
                )
        };
        let qr = |source: ImageSource| {
            img(source)
                .size(px(132.))
                .rounded(px(8.))
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        };
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        vec![
            self.setting_group("about-header", None, narrow)
                .items_center()
                .text_center()
                .py(px(if narrow { 14. } else { 40. }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .mb(px(16.))
                        .child(img("images/icon.png").size(px(56.)).rounded(px(12.)))
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .text_size(rems(1.75))
                                        .font_weight(FontWeight::EXTRA_BOLD)
                                        .child("Mio Music"),
                                )
                                .child(
                                    div()
                                        .px(px(10.))
                                        .py(px(3.))
                                        .rounded(px(16.))
                                        .text_size(rems(0.75))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .border_1()
                                        .border_color(color(mode, "--td-brand-color-3"))
                                        .bg(color(mode, "--td-brand-color-1"))
                                        .text_color(color(mode, "--td-brand-color-6"))
                                        .child(version),
                                ),
                        ),
                )
                .child(
                    div()
                        .max_w(px(420.))
                        .text_size(rems(0.9))
                        .line_height(relative(1.6))
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(self.t("settings.about.description")),
                )
                .into_any_element(),
            self.setting_group(
                "about-version",
                Some(self.t("settings.about.versionInfo")),
                narrow,
            )
            .child(self.update_section(narrow, cx))
            .into_any_element(),
            self.setting_group("about-legal", Some(self.t("settings.about.legalNotice")), narrow)
                .child(notice(
                    self.t("settings.about.dataResponsibility"),
                    self.t("settings.about.dataResponsibilityDesc"),
                ))
                .child(notice(
                    self.t("settings.about.copyrightCompliance"),
                    self.t("settings.about.copyrightComplianceDesc"),
                ))
                .child(notice(
                    self.t("settings.about.usageRestriction"),
                    self.t("settings.about.usageRestrictionDesc"),
                ))
                .child(
                    div()
                        .mt(px(24.))
                        .mb(px(8.))
                        .text_size(px(if narrow { 16. } else { 18. }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.t("settings.about.aboutUs")),
                )
                .child(
                    div()
                        .text_size(px(13.6))
                        .line_height(relative(1.7))
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(
                            InteractiveText::new(
                                "about-intro",
                                StyledText::new(intro).with_highlights(highlights),
                            )
                            .on_click(click_ranges, move |index, _, cx| {
                                if let Some(url) = urls.get(index) {
                                    cx.open_url(url);
                                }
                            }),
                        ),
                )
                .child(support_card(
                    "about-current-author",
                    self.t("settings.about.supportCurrentAuthorTitle"),
                    self.t("settings.about.supportCurrentAuthorText"),
                    vec![
                        self.t("settings.about.supportPointMaintenance"),
                        self.t("settings.about.supportPointCompatibility"),
                        self.t("settings.about.supportPointTransparency"),
                    ],
                    qr(img_source("images/current-author-sponsor-qr.jpg")),
                    self.t("settings.about.supportQrCaption"),
                ))
                .child(support_card(
                    "about-original-author",
                    self.t("settings.about.originalAuthorTitle"),
                    self.t("settings.about.sponsorText"),
                    vec![
                        self.t("settings.about.originalAuthorPoint"),
                        self.t("settings.about.sponsorHint"),
                    ],
                    qr(img_source("https://oss.shiqianjiang.cn/storage/default/20250907/image-2025082711173bb1bba3608ef15d0e1fb485f80f29c728186.png")),
                    self.t("settings.about.sponsorQrCaption"),
                ))
                .into_any_element(),
        ]
    }

    /// The "前往插件管理" button of the music source category (`switch-category`).
    pub(super) fn switch_to_plugins(&mut self, cx: &mut Context<Self>) {
        self.settings_ui.category = Category::Plugins;
        cx.notify();
    }
}

fn img_source(path: &'static str) -> ImageSource {
    ImageSource::from(path)
}
