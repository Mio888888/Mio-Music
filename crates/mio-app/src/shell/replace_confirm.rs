//! Confirmation before a list replaces the play queue ("play all" / "shuffle").
//! Same dialog as the Subsonic album's; the caller supplies what happens on confirm.
use super::*;
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant};
use std::rc::Rc;

impl Mio {
    pub(super) fn confirm_replace_queue(
        &self,
        shuffle: bool,
        title: String,
        body: String,
        on_confirm: Rc<dyn Fn(&mut Mio, &mut Context<Mio>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let confirm = self.t("music.list.confirmReplace");
        let cancel = self.t("common.cancel");
        let close = self.t("common.titleBar.close");
        let colors = self.ui_colors();
        let primary = color(colors, "--td-text-color-primary");
        let secondary = color(colors, "--td-text-color-secondary");
        let brand = color(colors, "--td-brand-color");
        let cancel_style = ButtonCustomVariant::new(cx)
            .color(primary.opacity(0.06))
            .foreground(primary)
            .hover(primary.opacity(0.12))
            .active(primary.opacity(0.12));
        let confirm_style = ButtonCustomVariant::new(cx)
            .color(brand.opacity(0.16))
            .foreground(brand)
            .hover(brand.opacity(0.22))
            .active(brand.opacity(0.22));
        window.open_dialog(cx, move |dialog, window, _| {
            let narrow = window.viewport_size().width <= px(768.);
            let gutter = (f32::from(window.viewport_size().width) * 0.04).clamp(16., 24.);
            let view = view.clone();
            let on_confirm = on_confirm.clone();
            dialog
                .w(px(if narrow {
                    (f32::from(window.viewport_size().width) - 2. * gutter).min(440.)
                } else {
                    (f32::from(window.viewport_size().width) - 32.).min(420.)
                }))
                .margin_top((window.viewport_size().height - px(250.)) / 2.)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(22.))
                .p(px(if narrow { 16. } else { 28. }))
                .when(narrow, |dialog| dialog.pt(px(12.)))
                .gap(px(0.))
                .bg(primary.opacity(0.02))
                .border_color(white().opacity(0.4))
                .overlay_tint(if narrow {
                    color(colors, "--mobile-scrim")
                } else {
                    black().opacity(0.45)
                })
                .when(cfg!(target_os = "macos"), |dialog| {
                    dialog
                        .overlay_filter(
                            if narrow { 12. } else { 8. },
                            if narrow { 1.8 } else { 1.4 },
                        )
                        .surface_filter(6.56, 1.8, px(22.))
                })
                .close_button(false)
                // The source confirms only an activated button; Enter on the dialog is inert.
                .on_ok(|_, _, _| false)
                .title(
                    div()
                        .flex()
                        .flex_col()
                        .when(narrow, |header| {
                            header.child(
                                div()
                                    .w(px(38.))
                                    .h(px(4.))
                                    .flex_shrink_0()
                                    .mb(px(12.))
                                    .mx_auto()
                                    .rounded_full()
                                    .bg(rgb(0x787880).opacity(0.36)),
                            )
                        })
                        .mb(px(if narrow { 6. } else { 10. }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .h(px(if narrow { 44. } else { 42. }))
                                .child(
                                    div()
                                        .size(px(if narrow { 38. } else { 42. }))
                                        .flex_shrink_0()
                                        .rounded(px(if narrow { 12. } else { 13. }))
                                        .border_1()
                                        .border_color(primary.opacity(0.08))
                                        .bg(linear_gradient(
                                            135.,
                                            linear_color_stop(brand.opacity(0.18), 0.),
                                            linear_color_stop(rgb(0x8c50ff).opacity(0.12), 1.),
                                        ))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_color(brand)
                                        .child(
                                            if shuffle {
                                                icon("Shuffle")
                                            } else {
                                                glyph("bofang")
                                            }
                                            .size(px(20.)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_size(px(17.))
                                        .line_height(relative(1.2))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(primary)
                                        .child(title.clone()),
                                )
                                .child(
                                    gpui_kit::base::Button::new("close-album-confirm")
                                        .accessibility_label(close.clone())
                                        .size(px(if narrow { 44. } else { 30. }))
                                        .flex_shrink_0()
                                        .rounded(px(if narrow { 999. } else { 9. }))
                                        .border_1()
                                        .border_color(primary.opacity(0.08))
                                        .bg(primary.opacity(0.03))
                                        .text_color(secondary)
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|button| {
                                            button
                                                .bg(rgb(0xff5050).opacity(0.15))
                                                .border_color(rgb(0xff5050).opacity(0.25))
                                                .text_color(color(colors, "--td-error-color"))
                                        })
                                        .child(icon("WindowClose").size(px(13.)))
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                ),
                        ),
                )
                .content({
                    let body = body.clone();
                    move |content, _, _| {
                        content
                            .font_weight(FontWeight::NORMAL)
                            .text_size(px(14.))
                            .line_height(relative(1.7))
                            .text_color(secondary)
                            .child(body.clone())
                    }
                })
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(if narrow { 8. } else { 10. }))
                        .mt(px(if narrow { 20. } else { 24. }))
                        .when(narrow, |footer| footer.flex_col_reverse())
                        .child(
                            Button::new("cancel-album-play")
                                .label(cancel.clone())
                                .border_1()
                                .custom(cancel_style)
                                .border_color(primary.opacity(0.08))
                                .min_w(px(92.))
                                .h(px(if narrow { 44. } else { 38. }))
                                .px(px(18.))
                                .rounded(px(if narrow { 999. } else { 12. }))
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(narrow, |button| button.w_full())
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("confirm-album-play")
                                .label(confirm.clone())
                                .border_1()
                                .custom(confirm_style)
                                .border_color(brand.opacity(0.32))
                                .min_w(px(92.))
                                .h(px(if narrow { 44. } else { 38. }))
                                .px(px(18.))
                                .rounded(px(if narrow { 999. } else { 12. }))
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(narrow, |button| button.w_full())
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| on_confirm(this, cx));
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }
}
