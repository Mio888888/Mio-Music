//! The reference recent.vue reverses LocalUserDetail.list (the playback queue), capped at 200.
//! Keep that behavior instead of inventing a second playback-history database.
use super::*;

impl Mio {
    pub(super) fn recent_page(
        &self,
        narrow: bool,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let rows: Vec<_> = self
            .player
            .queue
            .iter()
            .enumerate()
            .rev()
            .take(200)
            .collect();
        div()
            .id("recent-page")
            .size_full()
            .flex()
            .flex_col()
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                20.
            }))
            .pt(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                20.
            }))
            .pb(px(if narrow { 0. } else { 20. }))
            .child(
                div()
                    .flex_shrink_0()
                    .mb(px(16.))
                    .flex()
                    .when(narrow, |header| header.flex_col().items_start())
                    .when(!narrow, |header| header.items_baseline())
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(if narrow {
                                (width * 0.09).clamp(32., 41.6)
                            } else {
                                20.
                            }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.t("music.recent.title")),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(color(mode, "--td-text-color-secondary"))
                            .child(format!(
                                "({} {})",
                                self.player.queue.len(),
                                self.t("common.unitSongs")
                            )),
                    ),
            )
            .child(
                div()
                    .id("recent-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |body| {
                        body.child(
                            div()
                                .h(px(300.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(color(mode, "--td-text-color-secondary"))
                                .child(self.t("music.recent.empty")),
                        )
                    })
                    .children(rows.into_iter().enumerate().map(|(number, (index, song))| {
                        let id = song.id.clone();
                        gpui_kit::base::Button::new(SharedString::from(format!("recent-{}", index)))
                            .accessibility_label(format!("{} - {}", song.title, song.artist))
                            .w_full()
                            .flex()
                            .items_center()
                            .min_h(px(if narrow { 56. } else { 44. }))
                            .px(px(12.))
                            .py(px(10.))
                            .rounded(px(6.))
                            .hover(move |s| s.bg(color(mode, "--td-bg-color-component-hover")))
                            .child(
                                div()
                                    .w(px(32.))
                                    .flex_shrink_0()
                                    .text_size(px(12.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child((number + 1).to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(14.))
                                            .child(song.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.))
                                            .text_color(color(mode, "--td-text-color-secondary"))
                                            .child(song.artist.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .ml(px(12.))
                                    .text_size(px(12.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(if song.duration > 0 {
                                        format!("{}:{:02}", song.duration / 60, song.duration % 60)
                                    } else {
                                        "--:--".into()
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                // A queue edit after rendering must not select a different track.
                                if let Some(index) =
                                    this.player.queue.iter().position(|s| s.id == id)
                                {
                                    this.play_index(index, cx);
                                }
                            }))
                    })),
            )
    }
}
