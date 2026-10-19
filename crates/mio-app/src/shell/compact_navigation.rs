use super::*;

fn gutter(width: f32) -> f32 {
    (width * 0.04).clamp(16., 24.)
}

impl Mio {
    pub(super) fn compact_page_background(&self) -> Background {
        if let Some(song) = self.ui_colors().song {
            return song.mobile_background();
        }
        let (top, bottom) = if self.mode == Mode::Light {
            (white().opacity(0.94), Hsla::from(rgb(0xf5f7f8)))
        } else {
            (
                Hsla::from(rgb(0x181818)).opacity(0.96),
                Hsla::from(rgb(0x111111)),
            )
        };
        linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(bottom, 1.),
        )
        .color_space(ColorSpace::Srgb)
    }

    pub(super) fn compact_header(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.ui_colors();
        let top_gutter = (width * 0.035).clamp(14., 22.);
        div()
            .id("header")
            .test_support()
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(gutter(width)))
            .py(px(top_gutter))
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
                    .id("search")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .mr(px(11.5))
                    .px(px(8.))
                    .rounded(px(20.))
                    .bg(color(mode, "--td-bg-color-container"))
                    .child(
                        Styled::h(
                            Input::new(&self.search).appearance(false).when(
                                self.online
                                    .as_ref()
                                    .is_some_and(|o| !o.sources().is_empty()),
                                |input| input.prefix(self.discovery_source_menu(cx)),
                            ),
                            px(44.),
                        )
                        .rounded(px(20.))
                        .text_size(px(14.))
                        .suffix(
                            control(
                                "compact-search-submit",
                                "Search",
                                self.t("common.search"),
                                mode,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.search(cx))),
                        ),
                    ),
            )
            .child(
                gpui_kit::base::Button::new("compact-backup")
                    .accessibility_label(self.t("backup.backupAndRestore"))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .h(px(30.))
                    .pl(px(8.))
                    .pr(px(10.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .line_height(px(12.))
                    .child(icon("BackupCloud").size(px(15.)))
                    .child(if self.english { "Backup" } else { "备份" })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_backup_dialog(window, cx)),
                    ),
            )
            .child(
                gpui_kit::base::Button::new("compact-downloads")
                    .accessibility_label(self.t("common.homeNav.download"))
                    .size(px(36.))
                    .ml(px(6.))
                    .rounded(px(4.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .hover(move |s| s.bg(color(mode, "--td-bg-color-component-hover")))
                    .child(glyph("xiazai").size(px(18.)))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Page::Downloads, cx))),
            )
    }

    pub(super) fn compact_bottom_navigation(
        &self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let nav_width = (width - 2. * gutter(width)).min(520.);
        let mode = self.ui_colors();
        let dark = mode.mode == Mode::Dark;
        let brand = color(mode, "--td-brand-color");
        div()
            .id("mobileNav")
            .test_support()
            .tab_group()
            .tab_index(2)
            .tab_stop(false)
            .absolute()
            .left(px((width - nav_width) * 0.5))
            .bottom(px(10.))
            .w(px(nav_width))
            .h(px(64.))
            .rounded(px(32.))
            .shadow(vec![
                BoxShadow::new(px(0.), px(14.), black().opacity(0.22)).blur_radius(px(28.)),
            ])
            // The glass of the original reads as a solid light pill; without a fill the shadow
            // underneath shows through it.
            .bg(color(mode, "--td-bg-color-container").opacity(if dark { 0.9 } else { 1. }))
            .when(cfg!(target_os = "macos"), |nav| {
                #[cfg(target_os = "macos")]
                {
                    nav.child(
                        canvas(
                            |_, _, _| (),
                            |bounds, _, window, _| {
                                window.paint_backdrop(bounds, Corners::all(px(32.)), 6.56, 1.8);
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    nav
                }
            })
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(32.))
                    .border(px(0.5))
                    .border_color(white().opacity(0.5)),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .px(px(8.))
                    .py(px(7.))
                    .children(
                        [
                            (Page::Discover, "faxian", "common.homeNav.discover"),
                            (Page::Playlists, "yanchu", "common.homeNav.playlists"),
                            (Page::Local, "shouye", "common.homeNav.local"),
                            (Page::Appearance, "shezhi", "common.titleBar.settings"),
                        ]
                        .into_iter()
                        .map(|(page, name, key)| {
                            let active = self.navigation.current() == page;
                            gpui_kit::base::Button::new(SharedString::from(format!(
                                "compact-{key}"
                            )))
                            .accessibility_label(self.t(key))
                            .relative()
                            .flex_1()
                            .min_w(px(44.))
                            .h_full()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(3.))
                            .pt(px(5.))
                            .pb(px(4.))
                            .rounded(px(999.))
                            .text_color(if active {
                                brand
                            } else {
                                color(mode, "--td-text-color-secondary")
                            })
                            .bg(if active {
                                brand.opacity(if dark { 0.14 } else { 0.1 })
                            } else {
                                transparent_black()
                            })
                            .when(active, |button| {
                                button
                                    .shadow(vec![
                                        BoxShadow::new(
                                            px(0.),
                                            px(1.),
                                            white().opacity(if dark { 0.11 } else { 0.42 }),
                                        )
                                        .inset(),
                                        BoxShadow::new(
                                            px(0.),
                                            px(if dark { 7. } else { 6. }),
                                            brand.opacity(if dark { 0.16 } else { 0.14 }),
                                        )
                                        .blur_radius(px(if dark { 18. } else { 16. })),
                                    ])
                                    .child(
                                        div().absolute().size_full().rounded(px(999.)).bg(
                                            linear_gradient(
                                                180.,
                                                linear_color_stop(
                                                    white().opacity(if dark { 0.1 } else { 0.32 }),
                                                    0.,
                                                ),
                                                linear_color_stop(
                                                    white().opacity(if dark { 0.03 } else { 0.1 }),
                                                    1.,
                                                ),
                                            )
                                            .color_space(ColorSpace::Srgb),
                                        ),
                                    )
                            })
                            .focus_visible(move |style| style.border(px(2.)).border_color(brand))
                            .child(
                                glyph(name)
                                    .relative()
                                    .top(px(if active { -1. } else { 0. }))
                                    .size(px(21.)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(10.))
                                    .line_height(px(10.))
                                    .font_weight(if active {
                                        FontWeight::BOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .child(self.t(key)),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if page == Page::Appearance {
                                        this.open_compact_settings(cx);
                                    } else {
                                        this.go(page, cx);
                                    }
                                },
                            ))
                        }),
                    ),
            )
    }

    pub(super) fn compact_detail_back(
        &self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        gpui_kit::base::Button::new("compact-detail-back")
            .accessibility_label(self.t("common.back"))
            .absolute()
            .left(px(gutter(width)))
            .top(px(8.))
            .size(px(44.))
            .rounded(px(999.))
            .bg(black().opacity(0.2))
            .border(px(0.5))
            .border_color(white().opacity(0.18))
            .text_color(white())
            .child(glyph("xiangzuo").size(px(22.)))
            .disabled(!self.navigation.can_back())
            .on_click(cx.listener(|this, _, window, cx| {
                this.navigate_history(false, window, cx);
            }))
    }
}
