// Modified by Longbridge for gpui-fast.
//! Cached views whose layout position changes while their paint is reused.

use crate::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, StyleRefinement,
    Styled as _, TestAppContext, Window, div,
};
use std::{cell::Cell, rc::Rc};

struct Frame {
    label: usize,
    builds: Rc<Cell<usize>>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.builds.set(self.builds.get() + 1);
        div().size_full().child(format!("frame {}", self.label))
    }
}

struct Workspace {
    frame: Entity<Frame>,
    wrapped: bool,
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let frame = self
            .frame
            .clone()
            .cached(StyleRefinement::default().size_full())
            .into_any_element();
        let leaf = if self.wrapped {
            div().size_full().child(frame).into_any_element()
        } else {
            frame
        };
        div().size_full().child(leaf)
    }
}

struct Root {
    workspace: Entity<Workspace>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.workspace.clone())
    }
}

fn frames(retention: bool, initially_wrapped: bool, wrapped: bool) -> Vec<Vec<String>> {
    let mut cx = TestAppContext::single();
    let builds = Rc::new(Cell::new(0));
    let frame = cx.new(|_| Frame {
        label: 0,
        builds: builds.clone(),
    });
    let workspace = cx.new(|_| Workspace {
        frame: frame.clone(),
        wrapped: initially_wrapped,
    });
    let window = cx.add_window(|window, _| {
        window.set_view_retention(retention);
        Root {
            workspace: workspace.clone(),
        }
    });
    let draw = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
            for record in &window.rendered_frame.retained.records {
                if let (Some(layout), Some(rebuild)) = (&record.layout, &record.rebuild) {
                    assert_eq!(
                        layout.parent_layout_key,
                        Some(rebuild.parent_layout_key()),
                        "a reused view rebuilds under its current layout path"
                    );
                }
            }
            window.describe_rendered_frame()
        })
        .unwrap()
    };
    let mut output = vec![draw(&mut cx)];
    let before = builds.get();
    workspace.update(&mut cx, |workspace, cx| {
        workspace.wrapped = wrapped;
        cx.notify();
    });
    output.push(draw(&mut cx));
    assert_eq!(builds.get(), before, "the clean cached view reuses paint");
    for label in 1..=3 {
        frame.update(&mut cx, |frame, cx| {
            frame.label = label;
            cx.notify();
        });
        output.push(draw(&mut cx));
    }
    output
}

#[test]
fn splice_after_cached_view_moved() {
    assert_eq!(frames(true, true, false), frames(false, true, false));
}

#[test]
fn splice_after_cached_view_gained_wrapper() {
    assert_eq!(frames(true, false, true), frames(false, false, true));
}

#[test]
fn splice_after_cached_view_stayed() {
    assert_eq!(frames(true, true, true), frames(false, true, true));
}
