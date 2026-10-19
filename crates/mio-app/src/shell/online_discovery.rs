//! Online discovery shares the existing playlist cards/details and playback host.
use super::*;
use crate::online::PlaylistCard;
use gpui_kit::component::{
    Sizable,
    menu::{DropdownMenu, PopupMenuItem},
    skeleton::Skeleton,
};

#[derive(Default)]
pub(super) struct OnlineDiscovery {
    pub subsonic: bool,
    key: Option<(String, usize)>,
    categories: Vec<mio_music_sdk::DiscoveryCategoryGroup>,
    hot: Vec<mio_music_sdk::DiscoveryCategory>,
    category_group: usize,
    pub(super) more_categories: bool,
    /// When an outside press last closed the panel, so the same press on the
    /// toggle button does not reopen it straight away.
    more_closed_at: Option<Instant>,
    hot_scroll: ScrollHandle,
    group_scroll: ScrollHandle,
    sheet_scroll: ScrollHandle,
    category: String,
    charts: bool,
    results: Option<Vec<PlaylistCard>>,
    play_counts: std::collections::HashMap<String, u64>,
    error: Option<String>,
    category_error: Option<String>,
    page: usize,
    has_more: bool,
    busy: bool,
    revision: u64,
    scroll: ScrollHandle,
}
impl OnlineDiscovery {
    pub(super) fn invalidate(&mut self) {
        let revision = self.revision.wrapping_add(1);
        let subsonic = self.subsonic;
        let busy = self.busy;
        *self = Self::default();
        self.busy = busy;
        self.revision = revision;
        self.subsonic = subsonic;
    }
}
impl Mio {
    pub(super) fn discovery_online_source(&self) -> Option<String> {
        let online = self.online.as_ref()?;
        let sources = online.sources();
        sources
            .iter()
            .find(|s| {
                s.id == self.app_settings.selected_source
                    && mio_music_sdk::capabilities(&s.id).discovery
            })
            .or_else(|| {
                sources
                    .iter()
                    .find(|s| mio_music_sdk::capabilities(&s.id).discovery)
            })
            .map(|s| s.id.clone())
    }
    pub(super) fn ensure_online_discovery(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.discovery_online_source() else {
            return;
        };
        let signature = self
            .online
            .as_ref()
            .map(|o| Arc::as_ptr(o) as usize)
            .unwrap_or_default();
        let key = (source, signature);
        if self.online_discovery.key.as_ref() != Some(&key) {
            let state = &mut self.online_discovery;
            state.key = Some(key);
            state.revision = state.revision.wrapping_add(1);
            state.categories.clear();
            state.hot.clear();
            state.more_categories = false;
            state.category.clear();
            state.results = None;
            state.play_counts.clear();
            state.error = None;
            state.category_error = None;
            state.page = 0;
            state.has_more = false;
            state.scroll.set_offset(point(px(0.), px(0.)));
        }
        if self.online_discovery.results.is_none()
            && self.online_discovery.error.is_none()
            && !self.online_discovery.busy
        {
            self.load_online_discovery(false, cx);
        }
    }
    fn reset_online_discovery(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.online_discovery;
        state.revision = state.revision.wrapping_add(1);
        state.results = None;
        state.error = None;
        state.page = 0;
        state.has_more = false;
        state.scroll.set_offset(point(px(0.), px(0.)));
        self.ensure_online_discovery(cx);
        cx.notify();
    }
    fn load_online_discovery(&mut self, more: bool, cx: &mut Context<Self>) {
        let Some(source) = self.discovery_online_source() else {
            return;
        };
        let state = &mut self.online_discovery;
        if state.busy || (more && !state.has_more) {
            return;
        }
        state.busy = true;
        state.error = None;
        let revision = state.revision;
        let category = state.category.clone();
        let charts = state.charts;
        let categories = state.categories.is_empty();
        let page = if more { state.page + 1 } else { 1 };
        let key = state.key.clone();
        let task = cx.background_executor().spawn(async move {
            let groups = categories.then(|| mio_music_sdk::discovery_categories(&source));
            let cards = if charts {
                mio_music_sdk::discovery_charts(&source).map(|charts| {
                    (
                        charts
                            .into_iter()
                            .map(|c| PlaylistCard {
                                id: format!("__chart__:{}", c.id),
                                source: c.source,
                                title: c.name,
                                description: c.update_frequency,
                                cover: (!c.img.is_empty()).then_some(c.img),
                                author: String::new(),
                                song_count: None,
                                album: false,
                            })
                            .collect::<Vec<_>>(),
                        false,
                        std::collections::HashMap::new(),
                    )
                })
            } else {
                mio_music_sdk::discovery_playlists(&source, &category, page).map(|p| {
                    let mut play_counts = std::collections::HashMap::new();
                    let cards = p
                        .list
                        .into_iter()
                        .filter_map(|c| {
                            let id = match c.id {
                                serde_json::Value::String(v) => v,
                                serde_json::Value::Number(v) => v.to_string(),
                                _ => return None,
                            };
                            if id.is_empty() {
                                return None;
                            }
                            let count = c
                                .play_count
                                .as_u64()
                                .or_else(|| c.play_count.as_str().and_then(|v| v.parse().ok()));
                            if let Some(count) = count.filter(|n| *n > 0) {
                                play_counts.insert(id.clone(), count);
                            }
                            Some(PlaylistCard {
                                id,
                                source: c.source,
                                title: c.name,
                                description: c.desc,
                                cover: (!c.img.is_empty()).then_some(c.img),
                                author: c.author,
                                song_count: c.total.as_u64(),
                                album: false,
                            })
                        })
                        .collect::<Vec<_>>();
                    (cards, p.has_more, play_counts)
                })
            };
            (groups, cards)
        });
        cx.spawn(async move |this, cx| {
            let (groups, cards) = task.await;
            let _ = this.update(cx, |this, cx| {
                let current_source = this.discovery_online_source();
                let state = &mut this.online_discovery;
                state.busy = false;
                if state.revision == revision
                    && state.key == key
                    && current_source.as_deref() == key.as_ref().map(|k| k.0.as_str())
                {
                    if let Some(groups) = groups {
                        match groups {
                            Ok(c) => {
                                state.hot = c.hot;
                                state.categories = c.groups;
                                state.category_error = None;
                            }
                            Err(e) => state.category_error = Some(e),
                        }
                    }
                    match cards {
                        Ok((mut cards, has_more, play_counts)) => {
                            if !more {
                                state.play_counts.clear();
                            }
                            state.play_counts.extend(play_counts);
                            if more {
                                let current = state.results.get_or_insert_with(Vec::new);
                                cards.retain(|card| {
                                    !current
                                        .iter()
                                        .any(|old| old.id == card.id && old.source == card.source)
                                });
                                let added = !cards.is_empty();
                                current.extend(cards);
                                state.has_more = has_more && added;
                            } else {
                                state.results = Some(cards);
                                state.has_more = has_more;
                            }
                            state.page = page;
                        }
                        Err(error) => state.error = Some(error),
                    }
                }
                // Only one network job runs at a time. A changed source/category starts here.
                this.ensure_online_discovery(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn plugin_source_icon(id: &str, size: f32) -> AnyElement {
        use base64::Engine;
        let provider = mio_music_sdk::provider(id);
        if let Some(icon) = provider.as_ref().and_then(|p| p.icon()) {
            if let Some(encoded) = icon.strip_prefix("data:image/png;base64,") {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) {
                    // Bound plugin image dimensions before handing it to the renderer.
                    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
                        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
                        if (1..=512).contains(&width) && (1..=512).contains(&height) {
                            return img(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)))
                                .size(px(size))
                                .into_any_element();
                        }
                    }
                }
            } else if icon.len() <= 2048
                && reqwest::Url::parse(icon).is_ok_and(|url| {
                    matches!(url.scheme(), "http" | "https")
                        && url.host_str().is_some()
                        && url.username().is_empty()
                        && url.password().is_none()
                })
            {
                return img(icon.to_owned()).size(px(size)).into_any_element();
            } else if icon.len() <= 32 && !icon.chars().any(char::is_control) {
                return div()
                    .text_size(px(size))
                    .child(icon.to_owned())
                    .into_any_element();
            }
        }
        glyph("liebiao").size(px(size)).into_any_element()
    }

    pub(super) fn discovery_source_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let online = self
            .online
            .as_ref()
            .map(|o| o.sources())
            .unwrap_or_default();
        let selected = online
            .iter()
            .find(|s| s.id == self.app_settings.selected_source)
            .or_else(|| online.first())
            .map(|s| s.id.clone());
        let label = selected
            .as_ref()
            .and_then(|id| online.iter().find(|s| &s.id == id))
            .map(|s| s.name.clone())
            .unwrap_or_else(|| {
                if self.english {
                    "Source".into()
                } else {
                    "音源".into()
                }
            });
        let entity = cx.entity().downgrade();
        Button::new("header-source-menu")
            .ghost()
            .accessibility_label(label)
            .child(Self::plugin_source_icon(
                selected.as_deref().unwrap_or(""),
                20.,
            ))
            .small()
            .dropdown_menu(move |mut menu, _, _| {
                for source in &online {
                    let id = source.id.clone();
                    let entity = entity.clone();
                    let name = source.name.clone();
                    let icon_id = id.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            div()
                                .id(SharedString::from(format!("source-menu-{icon_id}")))
                                .role(gpui_kit::Role::MenuItem)
                                .aria_label(name.clone())
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(Self::plugin_source_icon(&icon_id, 20.))
                                .child(name.clone())
                        })
                        .checked(selected.as_deref() == Some(&id))
                        .on_click(move |_, _, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                this.online_discovery.subsonic = false;
                                this.select_music_source(&id, cx);
                                this.ensure_online_discovery(cx);
                                cx.notify();
                            });
                        }),
                    );
                }
                menu
            })
    }
    pub(super) fn online_discovery_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_online_discovery(cx);
        let width = f32::from(window.viewport_size().width);
        let narrow = width <= 768.;
        let mode = self.ui_colors();
        let state = &self.online_discovery;
        let rem = if narrow {
            16.
        } else {
            f32::from(window.rem_size())
        };
        let gutter = if narrow {
            (width * 0.04).clamp(16., 24.)
        } else {
            2. * rem
        };
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let mut body = div()
            .line_height(px(22.))
            .size_full()
            .flex()
            .flex_col()
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                rem
            }))
            .child(
                div()
                    .flex_shrink_0()
                    .mx(px(gutter))
                    .mb(px(rem))
                    // Wide windows drop the page title so the cards get the room.
                    .when(!narrow, |header| header.hidden())
                    .child(
                        div()
                            .mb(px(0.5 * rem))
                            .text_size(px(if narrow {
                                (width * 0.09).clamp(32., 41.6)
                            } else {
                                1.875 * rem
                            }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(if narrow {
                                (width * 0.09).clamp(32., 41.6) * 1.1
                            } else {
                                (1.875 * rem * 1.5).floor()
                            }))
                            .when(!narrow, |title| {
                                title
                                    .border_l(px(8.))
                                    .border_color(color(mode, "--td-brand-color-3"))
                                    .pl(px(12.))
                                    .rounded(px(8.))
                            })
                            .child(self.t("music.find.title")),
                    )
                    .child(
                        div()
                            .text_size(px(if narrow { rem } else { 0.85 * rem }))
                            .text_color(secondary)
                            .child(self.t("music.find.subtitle")),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .mx(px(gutter))
                    .mb(px(rem))
                    .flex()
                    .gap(px(2.))
                    .p(px(if narrow { 4. } else { 3. }))
                    .rounded(px(if narrow { 999. } else { 8. }))
                    .bg(color(
                        mode,
                        if narrow {
                            "--mobile-glass-bg-strong"
                        } else {
                            "--td-bg-color-secondarycontainer"
                        },
                    ))
                    .when(narrow, |tabs| {
                        tabs.border(px(0.5))
                            .border_color(color(mode, "--mobile-glass-border"))
                    })
                    .children(
                        [
                            (false, "music.find.tabPlaylist"),
                            (true, "music.find.tabLeaderboard"),
                        ]
                        .into_iter()
                        .map(|(leaderboard, key)| {
                            let active = state.charts == leaderboard;
                            gpui_kit::base::Button::new(key)
                                .accessibility_label(self.t(key))
                                .flex_1()
                                .min_w_0()
                                .px(px(16.))
                                .py(px(6.))
                                .when(narrow, |tab| tab.min_h(px(36.)))
                                .rounded(px(if narrow { 999. } else { 6. }))
                                .text_size(px(if narrow { 15. } else { 14. }))
                                .line_height(px(17.))
                                .font_weight(if active {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .text_color(if active { brand } else { secondary })
                                .bg(if active {
                                    color(mode, "--td-bg-color-container")
                                } else {
                                    transparent_black()
                                })
                                .when(active, |tab| {
                                    tab.shadow(vec![BoxShadow {
                                        color: black().opacity(0.08),
                                        offset: point(px(0.), px(1.)),
                                        blur_radius: px(3.),
                                        spread_radius: px(0.),
                                        inset: false,
                                    }])
                                })
                                .hover(move |s| s.text_color(if active { brand } else { primary }))
                                .child(self.t(key))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.online_discovery.charts = leaderboard;
                                    this.reset_online_discovery(cx);
                                }))
                        }),
                    ),
            );
        if !state.charts {
            let chip = |id: SharedString, name: String, active: bool| {
                gpui_kit::base::Button::new(id)
                    .accessibility_label(name.clone())
                    .flex_shrink_0()
                    .px(px(if narrow { 14. } else { 12. }))
                    .py(px(if narrow { 0. } else { 4. }))
                    .when(narrow, |c| {
                        c.min_h(px(44.))
                            .border_1()
                            .border_color(color(mode, "--td-border-level-1-color"))
                    })
                    .rounded(px(if narrow { 999. } else { 6. }))
                    .text_size(px(if narrow { 14. } else { 13. }))
                    .text_color(if active { brand } else { secondary })
                    .bg(if active {
                        color(mode, "--td-brand-color-light")
                    } else {
                        if narrow {
                            color(mode, "--td-bg-color-container")
                        } else {
                            transparent_black()
                        }
                    })
                    .hover(move |s| s.bg(color(mode, "--td-bg-color-secondarycontainer")))
                    .child(name)
            };
            let mut categories = div()
                .id("discover-hot-tags")
                .flex()
                .w_full()
                .min_w_0()
                .overflow_x_scroll()
                .overflow_y_hidden()
                .restrict_scroll_to_axis()
                .track_scroll(&state.hot_scroll)
                .when(narrow, |c| c.pt(px(4.)).pb(px(8.)).pr(px(32.)))
                .gap(px(8.))
                .child(
                    chip(
                        "discover-category-all".into(),
                        self.t("music.playlistCategory.hot"),
                        state.category.is_empty(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.online_discovery.category.clear();
                        this.reset_online_discovery(cx);
                    })),
                );
            for (i, category) in state.hot.iter().enumerate() {
                let id = category.id.clone();
                categories = categories.child(
                    chip(
                        format!("discover-hot-{i}").into(),
                        category.name.clone(),
                        state.category == id,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.online_discovery.category = id.clone();
                        this.reset_online_discovery(cx);
                    })),
                );
            }
            let more = chip(
                "discover-more-categories".into(),
                format!("{} ▾", self.t("music.playlistCategory.moreCategory")),
                false,
            )
            .bg(color(mode, "--td-bg-color-secondarycontainer"))
            .on_click(cx.listener(|this, _, _, cx| {
                let state = &mut this.online_discovery;
                let just_closed = state
                    .more_closed_at
                    .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(300));
                state.more_closed_at = None;
                if !just_closed {
                    state.more_categories = !state.more_categories;
                }
                cx.notify();
            }));
            // Narrow keeps "more" inside the scrolling strip; wide pins it at the
            // end of a single row.
            let more = if narrow {
                categories = categories.child(more);
                None
            } else {
                Some(more)
            };
            let mut category_area = div()
                .relative()
                .mx(px(gutter))
                .mb(px(if narrow { 12. } else { rem }))
                .when(narrow, |c| c.pt(px(8.)).pb(px(6.)).overflow_hidden())
                .flex_shrink_0()
                .child(match more {
                    None => categories.into_any_element(),
                    Some(more) => div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(categories.flex_1())
                        .child(more)
                        .into_any_element(),
                });
            if narrow {
                let tags = state.hot_scroll.clone();
                let page = color(mode, "--td-bg-color-page");
                category_area = category_area.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            if tags.max_offset().x + tags.offset().x > px(8.) {
                                window.paint_quad(fill(
                                    bounds,
                                    linear_gradient(
                                        90.,
                                        linear_color_stop(page.opacity(0.), 0.),
                                        linear_color_stop(page, 0.85),
                                    )
                                    .color_space(ColorSpace::Srgb),
                                ));
                            }
                        },
                    )
                    .absolute()
                    .right_0()
                    .top(px(8.))
                    .bottom(px(6.))
                    .w(px(32.)),
                );
            }
            if state.more_categories && !narrow {
                let mut panel = div()
                    .id("discover-more-panel")
                    .absolute()
                    .top(px(36.))
                    .left_0()
                    .right_0()
                    .p(px(16.))
                    .rounded(px(12.))
                    .bg(color(mode, "--td-bg-color-container"))
                    .border_1()
                    .border_color(color(mode, "--td-border-level-1-color"))
                    .shadow_md()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.online_discovery.more_categories = false;
                        this.online_discovery.more_closed_at = Some(Instant::now());
                        cx.notify();
                    }));
                panel = panel.child(div().flex().flex_wrap().gap(px(8.)).children(
                    state.categories.iter().enumerate().map(|(i, group)| {
                        chip(
                            format!("discover-group-{i}").into(),
                            group.name.clone(),
                            state.category_group == i,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.online_discovery.category_group = i;
                            cx.notify();
                        }))
                    }),
                ));
                if let Some(group) = state.categories.get(state.category_group) {
                    panel = panel.child(
                        div()
                            .id("discover-group-tags")
                            .mt(px(12.))
                            .max_h(px(220.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .children(group.list.iter().enumerate().map(|(i, category)| {
                                let id = category.id.clone();
                                chip(
                                    format!("discover-category-{i}").into(),
                                    category.name.clone(),
                                    state.category == id,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.online_discovery.category = id.clone();
                                        this.online_discovery.more_categories = false;
                                        this.reset_online_discovery(cx);
                                    },
                                ))
                            })),
                    );
                }
                category_area = category_area.child(deferred(panel).with_priority(80));
            }
            body = body.child(category_area);
            if let Some(error) = &state.category_error {
                body = body.child(
                    div()
                        .text_size(px(12.))
                        .text_color(color(mode, "--td-text-color-secondary"))
                        .child(format!(
                            "{}: {}",
                            if self.english {
                                "Categories unavailable"
                            } else {
                                "分类暂不可用"
                            },
                            error
                        )),
                );
            }
        }
        let mut content = div()
            .id("discover-online-cards")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .on_scroll_wheel(cx.listener(|_, _, window, cx| {
                cx.defer_in(window, |this, _, cx| {
                    let state = &this.online_discovery;
                    if state.page > 0
                        && state.has_more
                        && !state.busy
                        && state.error.is_none()
                        && state.scroll.max_offset().y + state.scroll.offset().y <= px(100.)
                    {
                        this.load_online_discovery(true, cx);
                    }
                });
            }))
            .px(px(if narrow && state.charts { 12. } else { gutter }))
            .pb(px(rem));
        let category_name = state
            .hot
            .iter()
            .chain(state.categories.iter().flat_map(|g| g.list.iter()))
            .find(|c| c.id == state.category)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| self.t("music.playlistCategory.hot"));
        content = content.child(
            div()
                .mb(px(if narrow && !state.charts { 12. } else { rem }))
                .text_size(px(if narrow { 17. } else { 20. }))
                .font_weight(FontWeight::BOLD)
                .child(if state.charts {
                    self.t("music.leaderboard.title")
                } else {
                    self.t("music.playlistCategory.hotPlaylists")
                        .replace("{name}", &category_name)
                }),
        );
        let available = (width
            - if narrow {
                0.
            } else {
                f32::from(window.rem_size()) * 15.
            }
            - 2. * if narrow && state.charts { 12. } else { gutter })
        .max(1.);
        let card_gap = if narrow {
            if state.charts { 10. } else { 12. }
        } else if state.charts {
            24.
        } else {
            12.
        };
        let cols = if narrow {
            if state.charts { 2 } else { 1 }
        } else {
            ((available + card_gap) / (if state.charts { 160. } else { 136. } + card_gap))
                .floor()
                .max(1.) as u16
        };
        let card_width = (available - card_gap * (cols - 1) as f32) / cols as f32;
        if let Some(cards) = &state.results {
            content = content.child(div().grid().grid_cols(cols).gap(px(card_gap)).children(
                cards.iter().map(|card| {
                    if state.charts {
                        self.discovery_chart_card(card, card_width, mode, cx)
                            .into_any_element()
                    } else {
                        self.discovery_playlist_card(card, card_width, rem, narrow, mode, cx)
                            .into_any_element()
                    }
                }),
            ));
            if cards.is_empty() {
                content = content.child(div().p(px(32.)).child(self.t("common.noData")));
            }
        }
        if state.busy {
            let loading_more = state
                .results
                .as_ref()
                .is_some_and(|cards| !cards.is_empty());
            content = content.child(
                div()
                    .when(loading_more, |grid| grid.mt(px(card_gap)))
                    .grid()
                    .grid_cols(cols)
                    .gap(px(card_gap))
                    .children(
                        (0..cols as usize * if loading_more { 1 } else { 3 }).map(|_| {
                            let cover_size = if narrow && !state.charts {
                                84.
                            } else {
                                card_width
                            };
                            div()
                                .w(px(card_width))
                                .flex()
                                .when(!narrow || state.charts, |card| card.flex_col())
                                .when(narrow && !state.charts, |card| {
                                    card.p(px(10.)).gap(px(12.)).items_center()
                                })
                                .rounded(px(if state.charts { 12. } else { rem }))
                                .overflow_hidden()
                                .bg(color(mode, "--td-bg-color-container"))
                                .child(
                                    Skeleton::new()
                                        .w(px(cover_size))
                                        .h(px(cover_size))
                                        .flex_shrink_0()
                                        .rounded(px(12.)),
                                )
                                .when(!state.charts, |card| {
                                    card.child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(10.))
                                            .when(!narrow, |info| info.w_full().p(px(rem)))
                                            .child(Skeleton::new().h(px(18.)))
                                            .child(
                                                Skeleton::new()
                                                    .secondary()
                                                    .w(relative(0.75))
                                                    .h(px(14.)),
                                            )
                                            .child(
                                                Skeleton::new()
                                                    .secondary()
                                                    .w(relative(0.45))
                                                    .h(px(12.)),
                                            ),
                                    )
                                })
                        }),
                    ),
            );
        }
        if let Some(error) = &state.error {
            content = content.child(
                div().p(px(16.)).child(error.clone()).child(
                    Button::new("discover-retry")
                        .label(self.t("music.leaderboard.retry"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let more = this.online_discovery.page > 0;
                            this.load_online_discovery(more, cx);
                        })),
                ),
            );
        }
        body.child(content)
    }
    pub(super) fn discovery_category_sheet(
        &self,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = f32::from(viewport.width).min(430.);
        let max_height = (f32::from(viewport.height) * 0.78).min(640.);
        let gutter = (f32::from(viewport.width) * 0.04).clamp(16., 24.);
        let mode = self.ui_colors();
        let state = &self.online_discovery;
        let primary = color(mode, "--td-text-color-primary");
        let secondary = color(mode, "--td-text-color-secondary");
        let brand = color(mode, "--td-brand-color");
        let chip = |id: SharedString, name: String, active: bool, group: bool| {
            gpui_kit::base::Button::new(id)
                .accessibility_label(name.clone())
                .flex_shrink_0()
                .min_h(px(if group { 38. } else { 44. }))
                .px(px(if group { 14. } else { 6. }))
                .rounded(px(if group { 999. } else { 12. }))
                .border_1()
                .border_color(if active {
                    brand
                } else {
                    color(mode, "--td-border-level-1-color")
                })
                .text_size(px(14.))
                .text_color(if active {
                    brand
                } else if group {
                    secondary
                } else {
                    primary
                })
                .bg(color(
                    mode,
                    if active {
                        "--td-brand-color-light"
                    } else if group {
                        "--td-bg-color-container"
                    } else {
                        "--td-bg-color-secondarycontainer"
                    },
                ))
                .child(name)
        };
        let mut body = div()
            .id("discover-sheet-body")
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.sheet_scroll)
            .px(px(gutter))
            .pt(px(14.))
            .pb(px(16.))
            .child(
                div()
                    .id("discover-sheet-groups")
                    .w_full()
                    .flex()
                    .gap(px(8.))
                    .pb(px(2.))
                    .overflow_x_scroll()
                    .overflow_y_hidden()
                    .restrict_scroll_to_axis()
                    .track_scroll(&state.group_scroll)
                    .children(state.categories.iter().enumerate().map(|(i, group)| {
                        chip(
                            format!("discover-sheet-group-{i}").into(),
                            group.name.clone(),
                            state.category_group == i,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.online_discovery.category_group = i;
                            cx.notify();
                        }))
                    })),
            );
        if let Some(group) = state.categories.get(state.category_group) {
            body = body.child(div().grid().grid_cols(3).gap(px(8.)).mt(px(14.)).children(
                group.list.iter().enumerate().map(|(i, category)| {
                    let id = category.id.clone();
                    chip(
                        format!("discover-sheet-tag-{i}").into(),
                        category.name.clone(),
                        state.category == id,
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.online_discovery.category = id.clone();
                        this.online_discovery.more_categories = false;
                        this.reset_online_discovery(cx);
                    }))
                }),
            ));
        }
        let panel = div()
            .id("discover-category-sheet")
            .w(px(width))
            .max_h(px(max_height))
            .flex()
            .flex_col()
            .rounded_t(px(20.))
            .overflow_hidden()
            .bg(color(mode, "--td-bg-color-container"))
            .shadow_lg()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .pt(px(10.))
                    .child(
                        div()
                            .w(px(36.))
                            .h(px(4.))
                            .rounded_full()
                            .bg(color(mode, "--td-border-level-2-color")),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .px(px(gutter))
                    .pt(px(14.))
                    .pb(px(12.))
                    .border_b_1()
                    .border_color(color(mode, "--td-border-level-1-color"))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child(self.t("music.playlistCategory.discoverMore")),
                    )
                    .child(
                        gpui_kit::base::Button::new("discover-sheet-close")
                            .accessibility_label(
                                self.t("music.playlistCategory.closeCategoryPanel"),
                            )
                            .size(px(44.))
                            .rounded(px(12.))
                            .child(icon("TD-Close").size(px(18.)).text_color(secondary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.online_discovery.more_categories = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(body);
        deferred(
            div()
                .id("discover-sheet-mask")
                .absolute()
                .inset_0()
                .flex()
                .items_end()
                .justify_center()
                .bg(black().opacity(0.42))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.online_discovery.more_categories = false;
                        cx.notify();
                    }),
                )
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(panel),
        )
        .with_priority(110)
        .into_any_element()
    }
    fn discovery_playlist_card(
        &self,
        card: &PlaylistCard,
        width: f32,
        rem: f32,
        narrow: bool,
        mode: crate::skin::Colors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = card.clone();
        let cover = card
            .cover
            .clone()
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        let primary = color(mode, "--td-text-color-primary");
        let placeholder = color(mode, "--td-text-color-placeholder");
        let mut info = div()
            .when(!narrow, |i| i.w_full())
            .when(narrow, |i| i.flex_1().flex().flex_col().justify_center())
            .min_w_0()
            .text_left()
            .px(px(if narrow { 0. } else { 10. }))
            .py(px(if narrow { 2. } else { 8. }))
            .child(
                div()
                    .text_size(px(if narrow { 15. } else { 13. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(if narrow { 21. } else { 16.9 }))
                    .min_h(px(if narrow { 0. } else { 33.8 }))
                    .w_full()
                    .overflow_hidden()
                    .text_ellipsis()
                    .line_clamp(2)
                    .mb(px(6.))
                    .text_color(primary)
                    .child(card.title.clone()),
            );
        // Wide cards stay compact: the description lives on the detail page.
        if narrow {
            info = info.child(
                div()
                    .text_size(px(13.))
                    .line_height(px(18.2))
                    .w_full()
                    .overflow_hidden()
                    .text_ellipsis()
                    .line_clamp(1)
                    .mb(px(8.))
                    .text_color(color(mode, "--td-text-color-secondary"))
                    .child(card.description.clone()),
            );
        }
        let songs = card.song_count.filter(|n| *n > 0);
        let plays = self.online_discovery.play_counts.get(&card.id).copied();
        if songs.is_some() || plays.is_some() {
            let mut meta = div()
                .flex()
                .items_center()
                .when(narrow, |m| m.justify_start())
                .when(!narrow, |m| m.justify_between())
                .gap(px(if narrow { 8. } else { 4. }))
                .text_size(px(if narrow { 12. } else { 11. }))
                .text_color(placeholder);
            if let Some(count) = plays {
                let label = if count >= 10000 {
                    format!(
                        "{:.1}{}",
                        count as f64 / 10000.,
                        if self.english { "w" } else { "万" }
                    )
                } else {
                    count.to_string()
                };
                meta = meta.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .child(icon("Play").size(px(if narrow { 14. } else { 11. })))
                        .child(label),
                );
            } else {
                meta = meta.child(div());
            }
            if let Some(count) = songs {
                meta = meta.child(div().child(if self.english {
                    format!("{count} songs")
                } else {
                    format!("{count} 首")
                }));
            }
            info = info.child(meta);
        }
        gpui_kit::base::Button::new(SharedString::from(format!(
            "discovery-playlist-{}-{}",
            card.source, card.id
        )))
        .accessibility_label(card.title.clone())
        .relative()
        .flex()
        .when(!narrow, |c| {
            // A faint tint and hairline border lift the card off the page.
            c.flex_col()
                .items_start()
                .border_1()
                .border_color(primary.opacity(0.1))
        })
        .when(narrow, |c| {
            c.items_stretch()
                .min_h(px(104.))
                .p(px(10.))
                .gap(px(12.))
                .border_1()
                .border_color(color(mode, "--td-border-level-1-color"))
        })
        .w(px(width))
        .rounded(px(if narrow { rem } else { 10. }))
        .overflow_hidden()
        .bg(if narrow {
            color(mode, "--td-bg-color-container")
        } else {
            primary.opacity(0.07)
        })
        .when(!narrow, |c| c.hover(|s| s.top(px(-2.))))
        .child(
            img(cover)
                .w(px(if narrow { 84. } else { width }))
                .h(px(if narrow { 84. } else { width }))
                .when(narrow, |i| i.rounded(px(12.)))
                .flex_shrink_0()
                .object_fit(ObjectFit::Cover)
                .with_fallback(|| {
                    img("images/default-cover.png")
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .into_any_element()
                }),
        )
        .child(info)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_online_playlist(selected.clone(), window, cx)
        }))
    }
    fn discovery_chart_card(
        &self,
        card: &PlaylistCard,
        width: f32,
        mode: crate::skin::Colors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = card.clone();
        let cover = card
            .cover
            .clone()
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        gpui_kit::base::Button::new(SharedString::from(format!(
            "discovery-chart-{}-{}",
            card.source, card.id
        )))
        .accessibility_label(card.title.clone())
        .relative()
        .size(px(width))
        .rounded(px(12.))
        .overflow_hidden()
        .shadow_md()
        .bg(color(mode, "--td-bg-color-secondarycontainer"))
        .hover(|s| s.top(px(-4.)).shadow_lg())
        .child(
            img(cover)
                .size_full()
                .rounded(px(12.))
                .object_fit(ObjectFit::Cover)
                .with_fallback(|| {
                    img("images/default-cover.png")
                        .size_full()
                        .rounded(px(12.))
                        .object_fit(ObjectFit::Cover)
                        .into_any_element()
                }),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .rounded_b(px(12.))
                .pt(px(40.))
                .px(px(12.))
                .pb(px(12.))
                .text_color(white())
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(transparent_black(), 0.),
                    linear_color_stop(black().opacity(0.7), 1.),
                ))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .mb(px(4.))
                        .child(card.title.clone()),
                )
                .child(div().text_size(px(12.)).child(card.description.clone())),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_online_playlist(selected.clone(), window, cx)
        }))
    }
}
