// Modified by Longbridge for gpui-fast.
//! How much drawing a window of views costs with and without retained views,
//! measured headlessly. Ignored by default; run it with
//!
//! ```text
//! cargo test -p gpui --lib --release retained_bench -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use crate::{
    Context, Entity, IntoElement, Render, SharedString, TestAppContext, Window, WindowHandle, div,
    hsla, prelude::*, px,
};

struct Panel {
    index: usize,
    count: usize,
    tick: usize,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tick = self.tick;
        div()
            .flex()
            .flex_col()
            .w(px(220.))
            .p_1()
            .bg(hsla(0.6, 0.2, 0.18, 1.))
            .child(SharedString::from(format!("panel {} {}", self.index, tick)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .children((0..self.count).map(|cell| {
                        div()
                            .w(px(26.))
                            .h(px(14.))
                            .hover(|style| style.bg(hsla(0.1, 0.6, 0.4, 1.)))
                            .child(SharedString::from(((cell + tick) % 97).to_string()))
                    })),
            )
    }
}

struct Dashboard {
    panels: Vec<Entity<Panel>>,
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .children(self.panels.iter().cloned())
    }
}

/// Draws `frames` frames of `panels` panels, `changing` of them notified
/// before each, and returns the time a frame took on average.
fn measure(retention: bool, panels: usize, changing: usize, frames: usize) -> Duration {
    let mut cx = TestAppContext::single();
    let window: WindowHandle<Dashboard> = cx.add_window(|_, cx| Dashboard {
        panels: (0..panels)
            .map(|index| {
                cx.new(|_| Panel {
                    index,
                    count: 64,
                    tick: index,
                })
            })
            .collect(),
    });
    let panels = window
        .update(&mut cx, |view, _, _| view.panels.clone())
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.set_view_retention(retention);
        window.draw(cx).clear(cx);
    })
    .unwrap();

    // Timed as a whole: the test app may draw a notified window by itself.
    let started = Instant::now();
    for frame in 0..frames {
        // Notified in one update, as one event handler would, so that the
        // window draws once for all of them.
        cx.update(|cx| {
            for n in 0..changing {
                panels[(frame * 7 + n * 13) % panels.len()].update(cx, |panel, cx| {
                    panel.tick += 1;
                    cx.notify();
                });
            }
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    started.elapsed() / frames as u32
}

#[test]
#[ignore]
fn retained_bench() {
    for (panels, changing) in [(60, 0), (60, 1), (60, 6), (60, 60)] {
        let off = measure(false, panels, changing, 200);
        let on = measure(true, panels, changing, 200);
        println!(
            "{panels} panels, {changing:>2} notified per frame: from scratch {:>7.3} ms, retained {:>7.3} ms ({:+.0}%)",
            off.as_secs_f64() * 1e3,
            on.as_secs_f64() * 1e3,
            (on.as_secs_f64() / off.as_secs_f64() - 1.) * 100.,
        );
    }
}
