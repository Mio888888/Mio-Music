//! What the window's close button does, and the tray icon that goes with it (`closeToTray`,
//! `hasConfiguredCloseBehavior`, `TitleBarControls.vue` and the tray setup of `lib.rs`).
//!
//! The first time the button is used it asks: exit, or hide the window to the tray; the answer
//! is remembered when the box is ticked. With "hide to tray" the window disappears and the tray
//! icon's left click brings it back.
use super::*;
use crate::tray;
use gpui_kit::component::{WindowExt, button::ButtonCustomVariant, checkbox::Checkbox};

struct CloseChoice {
    parent: WeakEntity<Mio>,
    colors: crate::skin::Colors,
    english: bool,
    /// "Remember my choice": ticked by default.
    remember: bool,
}

impl Mio {
    /// Creates the tray icon and starts listening to it. Called once for the real application;
    /// captures and tests run without one.
    pub(super) fn start_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match tray::create() {
            Ok(icon) => self.tray = Some(icon),
            Err(error) => {
                eprintln!("Unable to create the tray icon: {error}");
                return;
            }
        }
        // Weak entity: the loop ends with the window, and dropping the view removes the icon.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let events = tray::poll();
                if update_main(&this, cx, |this, window, cx| {
                    for event in events {
                        this.tray_event(event, window, cx);
                    }
                })
                .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn tray_event(&mut self, event: tray::Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            tray::Event::Panel(x, y, width, height) => {
                self.toggle_tray_panel((x, y, width, height), window, cx);
            }
        }
    }

    /// The close button of the main window and of the full player.
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_recognition(cx);
        if !self.close_behavior_configured {
            self.confirm_close(window, cx);
        } else {
            let to_tray = self.close_to_tray;
            self.leave(to_tray, window);
        }
    }

    /// Hides the window to the tray, or closes it (which ends the application).
    fn leave(&mut self, to_tray: bool, window: &mut Window) {
        if !to_tray {
            self.save_session();
            window.remove_window();
        } else if self.tray.is_some() && crate::platform::hide_window(window).is_ok() {
            // Hidden: the tray icon is the way back.
        } else {
            // No tray icon to come back from; keep the window reachable.
            window.minimize_window();
        }
    }

    fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let parent = cx.weak_entity();
        let colors = self.ui_colors();
        let english = self.english;
        let remember = self.close_remember;
        let choice = cx.new(|_| CloseChoice {
            parent,
            colors,
            english,
            remember,
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
                // `placement="top"`
                .margin_top(window.viewport_size().height * 0.2)
                .when(narrow, |dialog| dialog.bottom_offset(px(12.)))
                .rounded(px(if narrow { 22. } else { 6. }))
                .p(px(0.))
                .gap(px(0.))
                .bg(color(
                    colors,
                    if narrow {
                        "--mobile-glass-bg-strong"
                    } else {
                        "--td-bg-color-container"
                    },
                ))
                .border_color(color(
                    colors,
                    if narrow {
                        "--mobile-glass-border"
                    } else {
                        "--td-border-level-1-color"
                    },
                ))
                .overlay_tint(color(
                    colors,
                    if narrow {
                        "--mobile-scrim"
                    } else {
                        "--td-mask-active"
                    },
                ))
                .when(cfg!(target_os = "macos"), |dialog| {
                    dialog
                        .overlay_filter(
                            if narrow { 12. } else { 5. },
                            if narrow { 1.8 } else { 1. },
                        )
                        .when(narrow, |dialog| dialog.surface_filter(12., 1.8, px(22.)))
                })
                .close_button(false)
                .on_ok(|_, _, _| false)
                .child(choice.clone())
        });
    }

    /// The switch and its text under "关闭按钮行为" in the settings page.
    pub(super) fn close_behavior_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.ui_colors();
        div()
            .flex()
            .items_center()
            .h(px(22.))
            .gap(px(10.))
            .child(
                gpui_kit::component::switch::Switch::new("close-to-tray")
                    .checked(self.close_to_tray)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        // Only the preference changes; the first-close question stays pending
                        // until it has been answered, as in the original.
                        this.close_to_tray = *checked;
                        this.save_preferences();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(color(mode, "--td-text-color-primary"))
                    .child(self.t(if self.close_to_tray {
                        "settings.appearance.closeToTray"
                    } else {
                        "settings.appearance.directExit"
                    })),
            )
            .into_any_element()
    }
}

impl CloseChoice {
    fn t(&self, key: &str) -> String {
        i18n::text(self.english, key)
    }

    fn choose(&mut self, to_tray: bool, window: &mut Window, cx: &mut Context<Self>) {
        let remember = self.remember;
        window.close_dialog(cx);
        let _ = self.parent.update(cx, |parent, cx| {
            if remember {
                parent.close_to_tray = to_tray;
                parent.close_behavior_configured = true;
                parent.save_preferences();
            }
            parent.leave(to_tray, window);
            cx.notify();
        });
    }
}

impl Render for CloseChoice {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width <= px(768.);
        let primary = color(self.colors, "--td-text-color-primary");
        let secondary = color(self.colors, "--td-text-color-secondary");
        let surface = color(self.colors, "--td-bg-color-container");
        let exit_style = ButtonCustomVariant::new(cx)
            .color(if narrow {
                surface.opacity(0.7)
            } else {
                surface
            })
            .foreground(primary)
            .hover(color(self.colors, "--td-bg-color-container-hover"))
            .active(color(self.colors, "--td-bg-color-container-active"));
        div()
            .relative()
            .p(px(if narrow { 16. } else { 32. }))
            .when(narrow, |div| div.pt(px(12.)))
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
            .text_color(primary)
            .when(narrow, |root| {
                root.child(
                    div()
                        .w(px(38.))
                        .h(px(4.))
                        .mx_auto()
                        .mb(px(12.))
                        .rounded(px(999.))
                        .bg(rgb(0x787880).opacity(0.36)),
                )
            })
            .child(
                div()
                    .pr(px(28.))
                    .text_size(px(16.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t("common.titleBar.closeTip")),
            )
            .child(
                gpui_kit::base::Button::new("close-close-dialog")
                    .accessibility_label(self.t("common.titleBar.close"))
                    .absolute()
                    .top(px(if narrow { 10. } else { 32. }))
                    .right(px(if narrow { 10. } else { 32. }))
                    .size(px(if narrow { 44. } else { 20. }))
                    .rounded(px(if narrow { 999. } else { 3. }))
                    .when(narrow, |button| button.bg(surface.opacity(0.6)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(secondary)
                    .hover(|button| button.bg(color(self.colors, "--td-bg-color-container-hover")))
                    .child(icon("WindowClose").size(px(20.)))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                div()
                    .pt(px(if narrow { 14. } else { 16. }))
                    .line_height(px(22.))
                    .text_color(secondary)
                    .child(self.t("common.titleBar.closeQuestion")),
            )
            .child(
                div().mt(px(10.)).child(
                    Checkbox::new("close-remember-choice")
                        .label(self.t("common.titleBar.rememberChoice"))
                        .checked(self.remember)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.remember = *checked;
                            // Kept for the next time the question opens, like the original's ref.
                            let checked = *checked;
                            let _ = this
                                .parent
                                .update(cx, |parent, _| parent.close_remember = checked);
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(if narrow { 16. } else { 8. }))
                    .pt(px(if narrow { 12. } else { 24. }))
                    .when(narrow, |div| div.flex_row_reverse())
                    .child(
                        Button::new("close-exit")
                            .label(self.t("common.titleBar.exitDirectly"))
                            .custom(exit_style)
                            .outline()
                            .h(px(if narrow { 44. } else { 32. }))
                            .rounded(px(if narrow { 12. } else { 3. }))
                            .when(narrow, |button| button.flex_1())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.choose(false, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("close-to-tray-choice")
                            .label(self.t("common.titleBar.minimizeToTray"))
                            // TDesign's primary button: the theme's brand colour, solid.
                            .primary()
                            .h(px(if narrow { 44. } else { 32. }))
                            .rounded(px(if narrow { 12. } else { 3. }))
                            .when(narrow, |button| button.flex_1())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.choose(true, window, cx)),
                            ),
                    ),
            )
    }
}
