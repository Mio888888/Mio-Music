use super::*;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
struct CommentPage {
    rows: Vec<Value>,
    page: u64,
    total: u64,
    max_page: u64,
    cursor: String,
    loading: bool,
    error: Option<String>,
    scroll: ScrollHandle,
}
impl CommentPage {
    fn more(&self) -> bool {
        self.page > 0 && self.page < self.max_page && (self.rows.len() as u64) < self.total
    }
}
#[derive(Default)]
pub(super) struct Comments {
    pub open: bool,
    song: Option<String>,
    revision: u64,
    active: usize,
    pages: [CommentPage; 2],
    expanded: HashSet<String>,
    outstanding: usize,
    texts: std::cell::RefCell<
        std::collections::HashMap<String, Entity<gpui_kit::component::text::TextViewState>>,
    >,
    pub image: Option<String>,
}
fn text(value: &Value, field: &str) -> String {
    match &value[field] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn count(n: u64) -> String {
    if n > 10000 {
        format!("{:.1}w", n as f64 / 10000.)
    } else {
        n.to_string()
    }
}
fn image_url(value: &str) -> Option<SharedString> {
    reqwest::Url::parse(value)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
        .map(|_| SharedString::from(value.to_string()))
}
impl Mio {
    pub(super) fn invalidate_comments(&mut self) {
        let open = self.comments.open;
        let outstanding = self.comments.outstanding;
        let revision = self.comments.revision.wrapping_add(1);
        self.comments = Comments {
            open,
            outstanding,
            revision,
            ..Default::default()
        };
    }
    pub(super) fn comment_item(&self) -> Option<mio_music_sdk::MusicItem> {
        let song = self.player.song()?;
        self.online
            .as_ref()?
            .item_json(&song.id)
            .and_then(|v| serde_json::from_value::<mio_music_sdk::MusicItem>(v).ok())
            .filter(|item| mio_music_sdk::capabilities(&item.source).comments)
    }
    pub(super) fn toggle_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.comments.open = !self.comments.open;
        if self.comments.open {
            if !self.full_player_open {
                self.toggle_full_player(window, cx);
            }
            self.player_settings_open = false;
            self.queue_open = false;
            self.player_idle.manual = false;
            self.wake_player(cx);
            self.sync_comments(cx);
        }
        cx.notify();
    }
    pub(super) fn sync_comments(&mut self, cx: &mut Context<Self>) {
        let song = self.player.song().map(|s| s.id.clone());
        if song != self.comments.song {
            let outstanding = self.comments.outstanding;
            let open = self.comments.open && self.full_player_open;
            let revision = self.comments.revision.wrapping_add(1);
            self.comments = Comments {
                song,
                open,
                revision,
                outstanding,
                ..Default::default()
            };
        }
        if !self.full_player_open {
            self.comments.open = false;
        }
        if !self.comments.open {
            return;
        }
        let active = self.comments.active;
        let page = &self.comments.pages[active];
        let near_end =
            f32::from(page.scroll.max_offset().y) + f32::from(page.scroll.offset().y) < 200.;
        if page.error.is_none() && !page.loading && (page.page == 0 || (page.more() && near_end)) {
            self.load_comments(active, cx);
        }
    }
    fn load_comments(&mut self, tab: usize, cx: &mut Context<Self>) {
        if self.comments.outstanding >= 2 || self.comments.pages[tab].loading {
            return;
        }
        let Some(item) = self
            .comment_item()
            .filter(|item| mio_music_sdk::capabilities(&item.source).comments)
        else {
            return;
        };
        let state = &mut self.comments.pages[tab];
        let page = state.page + 1;
        let cursor = state.cursor.clone();
        state.loading = true;
        state.error = None;
        self.comments.outstanding += 1;
        let revision = self.comments.revision;
        let task = cx
            .background_executor()
            .spawn(async move { mio_music_sdk::get_comments(&item, page, tab == 0, &cursor) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.comments.outstanding = this.comments.outstanding.saturating_sub(1);
                if revision != this.comments.revision {
                    this.sync_comments(cx);
                    return;
                }
                let state = &mut this.comments.pages[tab];
                state.loading = false;
                match result {
                    Ok(data) => {
                        let mut added = 0;
                        if let Some(rows) = data["comments"].as_array() {
                            let mut ids: HashSet<String> =
                                state.rows.iter().map(|v| text(v, "id")).collect();
                            for row in rows {
                                if ids.insert(text(row, "id")) {
                                    state.rows.push(row.clone());
                                    added += 1;
                                }
                            }
                        }
                        state.page = page;
                        state.total = data["total"].as_u64().unwrap_or(0);
                        state.max_page = data["maxPage"].as_u64().unwrap_or(1);
                        state.cursor = text(&data, "cursor");
                        // Stop a broken upstream from endlessly returning the same page.
                        if added == 0
                            || (item_source_cursor(&data) && tab == 1 && state.cursor.is_empty())
                        {
                            state.max_page = page;
                        }
                    }
                    Err(error) => state.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn comment_body(
        &self,
        row: &Value,
        key: String,
        reply: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut body = div().flex().gap(px(if reply { 8. } else { 16. }));
        let avatar = text(row, "avatar");
        body = body.child(
            div()
                .size(px(if reply { 24. } else { 40. }))
                .flex_shrink_0()
                .rounded_full()
                .overflow_hidden()
                .bg(white().opacity(0.08))
                .when_some(image_url(&avatar), |el, url| {
                    el.child(
                        img(url)
                            .size_full()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                }),
        );
        let images = row["images"].as_array().cloned().unwrap_or_default();
        let content_text = text(row, "text");
        let escaped = content_text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br>");
        let selectable = self
            .comments
            .texts
            .borrow_mut()
            .entry(key.clone())
            .or_insert_with(|| {
                cx.new(|cx| gpui_kit::component::text::TextViewState::html(&escaped, cx))
            })
            .clone();
        let mut content = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(10.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text(row, "userName")),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(white().opacity(0.5))
                            .child(text(row, "timeStr")),
                    ),
            )
            .child(
                div()
                    .text_color(white().opacity(0.9))
                    .child(gpui_kit::component::text::TextView::new(&selectable).selectable(true)),
            );
        if !images.is_empty() {
            content = content.child(div().flex().flex_wrap().gap(px(8.)).children(
                images.into_iter().enumerate().filter_map(|(i, v)| {
                    let url = image_url(v.as_str()?)?;
                    let click_url = url.to_string();
                    Some(
                        div()
                            .id(SharedString::from(format!("{key}-image-{i}")))
                            .size(px(100.))
                            .rounded(px(8.))
                            .overflow_hidden()
                            .cursor_pointer()
                            .child(img(url).size_full().object_fit(ObjectFit::Cover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.comments.image = Some(click_url.clone());
                                cx.notify();
                            })),
                    )
                }),
            ));
        }
        let replies = row["reply"].as_array().cloned().unwrap_or_default();
        if !reply && !replies.is_empty() {
            let expanded = self.comments.expanded.contains(&key);
            let reply_key = key.clone();
            let mut section = div()
                .p(px(12.))
                .rounded(px(12.))
                .bg(black().opacity(0.15))
                .flex()
                .flex_col()
                .gap(px(12.));
            if expanded {
                section =
                    section.children(replies.iter().enumerate().map(|(i, row)| {
                        self.comment_body(row, format!("{key}-reply-{i}"), true, cx)
                    }));
            }
            section = section.child(
                Button::new(SharedString::from(format!("{key}-expand")))
                    .self_start()
                    .text_size(px(12.))
                    .ghost()
                    .custom(
                        gpui_kit::component::button::ButtonCustomVariant::new(cx)
                            .foreground(white().opacity(0.9))
                            .hover(white().opacity(0.1)),
                    )
                    .text_color(white())
                    .label(if expanded {
                        self.t("play.comments.collapseReplies")
                    } else {
                        self.t("play.comments.expandReplies")
                            .replace("{count}", &replies.len().to_string())
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.comments.expanded.insert(reply_key.clone()) {
                            this.comments.expanded.remove(&reply_key);
                        }
                        cx.notify();
                    })),
            );
            content = content.child(section);
        }
        body.child(content)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(white().opacity(0.5))
                    .child(format!(
                        "♡ {}",
                        count(
                            row["likedCount"]
                                .as_u64()
                                .or_else(|| row["likedCount"].as_str().and_then(|s| s.parse().ok()))
                                .unwrap_or(0)
                        )
                    )),
            )
            .into_any_element()
    }
    pub(super) fn comments_overlay(
        &self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.comments.active;
        let state = &self.comments.pages[active];
        let supported = self
            .comment_item()
            .is_some_and(|i| mio_music_sdk::capabilities(&i.source).comments);
        let mut list = div().flex().flex_col().gap(px(24.));
        if !supported {
            list = list.child(if self.english {
                "Comments are unavailable for this source"
            } else {
                "该来源不支持评论"
            });
        } else {
            list = list.children(state.rows.iter().enumerate().map(|(i, row)| {
                self.comment_body(row, format!("comment-{active}-{i}"), false, cx)
            }));
            if state.loading {
                list = list.child(self.t("common.loading"));
            } else if let Some(error) = &state.error {
                list = list.child(
                    div().child(error.clone()).child(
                        Button::new("retry-comments")
                            .ghost()
                            .custom(
                                gpui_kit::component::button::ButtonCustomVariant::new(cx)
                                    .foreground(white().opacity(0.9))
                                    .hover(white().opacity(0.1)),
                            )
                            .label(if self.english { "Retry" } else { "重试" })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.load_comments(active, cx)),
                            ),
                    ),
                );
            } else if state.page > 0 {
                list = list.child(self.t(if state.rows.is_empty() {
                    "play.comments.empty"
                } else if !state.more() {
                    "play.comments.noMore"
                } else {
                    "common.loading"
                }));
            }
        }
        let accent = self.theme_palette().map(|p| p.main).unwrap_or(white());
        let tabs = div()
            .flex()
            .gap(px(if width <= 768. { 8. } else { 20. }))
            .children((0..2).map(|tab| {
                Button::new(if tab == 0 {
                    "comments-hot"
                } else {
                    "comments-latest"
                })
                .ghost()
                .custom(
                    gpui_kit::component::button::ButtonCustomVariant::new(cx)
                        .foreground(white().opacity(0.9))
                        .hover(white().opacity(0.1)),
                )
                .px(px(if width <= 768. { 4. } else { 10. }))
                .label(format!(
                    "{} {}",
                    self.t(if tab == 0 {
                        "play.comments.hot"
                    } else {
                        "play.comments.latest"
                    }),
                    count(self.comments.pages[tab].total)
                ))
                .text_color(white().opacity(if active == tab { 1. } else { 0.6 }))
                .when(active == tab, |el| el.border_b(px(3.)).border_color(accent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.comments.active = tab;
                    this.sync_comments(cx);
                    cx.notify();
                }))
            }));
        div()
            .id("comments-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(black().opacity(0.4))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.comments.open = false;
                    this.comments.image = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("comments-card")
                    .w(px(width * 0.8))
                    .h(px(height * 0.8))
                    .rounded(px(24.))
                    .overflow_hidden()
                    .bg(rgb(0x25262d).opacity(0.96))
                    .text_color(white())
                    .text_size(px(14.))
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(60.))
                            .flex_shrink_0()
                            .px(px(24.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b(px(1.))
                            .border_color(white().opacity(0.08))
                            .child(tabs)
                            .child(
                                Button::new("close-comments")
                                    .ghost()
                                    .custom(
                                        gpui_kit::component::button::ButtonCustomVariant::new(cx)
                                            .foreground(white().opacity(0.9))
                                            .hover(white().opacity(0.1)),
                                    )
                                    .label("×")
                                    .accessibility_label(self.t("common.close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.comments.open = false;
                                        this.comments.image = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("comments-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&state.scroll)
                            .p(px(20.))
                            .child(list),
                    ),
            )
            .when_some(self.comments.image.clone(), |overlay, url| {
                overlay.child(
                    div()
                        .id("comment-image-viewer")
                        .absolute()
                        .inset_0()
                        .bg(black().opacity(0.9))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.comments.image = None;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .child(
                            img(SharedString::from(url))
                                .w(px(width * 0.85))
                                .h(px(height * 0.85))
                                .object_fit(ObjectFit::Contain),
                        ),
                )
            })
            .into_any_element()
    }
}
fn item_source_cursor(value: &Value) -> bool {
    value["source"].as_str() == Some("mg")
}
