// Modified by Longbridge for gpui-fast.
//! Tests of retained layout: layout nodes carried from one frame to the next,
//! the keys that match them to elements, and the statistics that show it.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use crate::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Div, ElementId, Entity, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Render, SharedString, Styled,
    TestAppContext, UniformListScrollHandle, Window, WindowHandle, canvas, div,
    fast::stats::LayoutStats, hsla, point, px, size, uniform_list,
};

/// Drives the retained-layout tests.
///
/// The shape, the styling and the text of the tree are each controllable on
/// their own, and every row records the bounds its trailing probe resolved
/// to, so a frame assembled out of retained nodes can be compared against
/// the frame a fresh tree produces for the same inputs.
struct RetainedLayoutView {
    rows: usize,
    row_width: Pixels,
    label: SharedString,
    probes: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// Identities of the rows, in order. Rows are keyed by these when
    /// `keyed` is set, and by their position otherwise.
    row_ids: Vec<u64>,
    keyed: bool,
    text_color: Hsla,
}

impl Render for RetainedLayoutView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let probes = self.probes.clone();
        probes.borrow_mut().clear();
        let label = self.label.clone();
        let row_width = self.row_width;
        let keyed = self.keyed;
        let row_ids = self.row_ids.clone();
        let text_color = self.text_color;
        div()
            .flex()
            .flex_col()
            .children((0..self.rows).map(move |ix| {
                let probes = probes.clone();
                // Rows have to be distinguishable for the test to mean
                // anything: interchangeable rows can be matched to the
                // wrong node and nobody is any the wiser.
                let row_id = row_ids.get(ix).copied().unwrap_or(ix as u64);
                let row = div()
                    .flex()
                    .flex_row()
                    .w(row_width + px((row_id % 5) as f32 * 10.))
                    .h(px(20.))
                    .text_color(text_color)
                    .child(label.clone())
                    .child(
                        canvas(
                            move |bounds, _, _| probes.borrow_mut().push(bounds),
                            |_, _, _, _| {},
                        )
                        .flex_1()
                        .h_full(),
                    );
                if keyed {
                    row.id(("row", row_ids[ix])).into_any_element()
                } else {
                    row.into_any_element()
                }
            }))
    }
}

/// Draws one frame and returns the layout work it took.
fn draw_frame(cx: &mut TestAppContext, window: AnyWindowHandle) -> LayoutStats {
    cx.update_window(window, |_, window, cx| {
        window.reset_layout_stats();
        window.draw(cx).clear(cx);
        window.layout_stats()
    })
    .unwrap()
}

/// Applies a change to the view and returns the layout work that followed.
///
/// Notifying a view can draw a frame of its own before the explicit one
/// here, so the counters start before the change rather than before the
/// draw; otherwise the work the change caused would be measured a frame too
/// late, once the tree had already settled.
fn change_and_draw<V: Render>(
    cx: &mut TestAppContext,
    window: WindowHandle<V>,
    change: impl FnOnce(&mut V),
) -> LayoutStats {
    cx.update_window(window.into(), |_, window, _| window.reset_layout_stats())
        .unwrap();
    window
        .update(cx, |view, _, cx| {
            change(view);
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        window.layout_stats()
    })
    .unwrap()
}

fn retained_layout_window(
    cx: &mut TestAppContext,
    probes: Rc<RefCell<Vec<Bounds<Pixels>>>>,
) -> WindowHandle<RetainedLayoutView> {
    cx.add_window(move |_, _| RetainedLayoutView {
        rows: 4,
        row_width: px(200.),
        label: "ab".into(),
        probes,
        row_ids: (0..4).collect(),
        keyed: false,
        text_color: hsla(0.0, 0.0, 0.1, 1.0),
    })
}

#[test]
fn an_unchanged_frame_reuses_every_layout_node_and_writes_to_none() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes.clone());

    draw_frame(&mut cx, window.into());
    let first = probes.borrow().clone();

    let stats = draw_frame(&mut cx, window.into());
    assert_eq!(
        stats.nodes_created, 0,
        "an unchanged frame should not allocate a single node"
    );
    assert!(stats.nodes_reused > 0);
    assert_eq!(
        stats.style_writes, 0,
        "writing a style dirties the node and its ancestors, undoing the point of retaining it"
    );
    assert_eq!(stats.children_writes, 0);
    assert_eq!(stats.measure_rebinds, 0);
    assert_eq!(
        &first,
        &*probes.borrow(),
        "a frame laid out from retained nodes must land in the same place as the frame before it"
    );
}

#[test]
fn a_retained_frame_follows_a_style_change() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes.clone());

    draw_frame(&mut cx, window.into());
    let before = probes.borrow()[0];

    change_and_draw(&mut cx, window, |view| view.row_width = px(400.));
    let after = probes.borrow()[0];

    assert_eq!(
        after.size.width - before.size.width,
        px(200.),
        "the probe fills what is left of the row, so widening the row must widen it too"
    );
    assert_eq!(
        probes.borrow().len(),
        4,
        "widening rows should not have changed how many there are"
    );
}

#[test]
fn a_retained_frame_follows_a_text_change() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes.clone());

    draw_frame(&mut cx, window.into());
    let before = probes.borrow()[0];

    let stats = change_and_draw(&mut cx, window, |view| {
        view.label = "abcdefghijklmnop".into()
    });
    let after = probes.borrow()[0];

    assert!(
        stats.measure_rebinds > 0,
        "changed text must invalidate the measurement it is cached under: {stats:?}"
    );
    assert!(
        after.origin.x > before.origin.x,
        "longer text should push the probe further along the row, \
         got {before:?} then {after:?}"
    );
}

#[test]
fn a_retained_frame_follows_a_structural_change() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes.clone());

    draw_frame(&mut cx, window.into());
    assert_eq!(probes.borrow().len(), 4);
    let row_height = probes.borrow()[1].origin.y - probes.borrow()[0].origin.y;

    change_and_draw(&mut cx, window, |view| view.rows = 7);
    assert_eq!(probes.borrow().len(), 7);
    assert_eq!(
        probes.borrow()[6].origin.y - probes.borrow()[0].origin.y,
        row_height * 6.,
        "rows added to a retained tree must stack like the ones already there"
    );

    let stats = change_and_draw(&mut cx, window, |view| view.rows = 2);
    assert_eq!(probes.borrow().len(), 2);
    assert!(
        stats.nodes_freed > 0,
        "nodes that left the tree must be released rather than accumulated: {stats:?}"
    );
}

/// Rows are matched to their nodes by position unless they say otherwise,
/// so inserting at the front of a list makes every row that follows look
/// like a different row. An `ElementId` is how a row says otherwise.
#[test]
fn rows_identified_by_an_element_id_keep_their_nodes_when_one_is_inserted_ahead() {
    fn insert_at_head(cx: &mut TestAppContext, keyed: bool) -> LayoutStats {
        let probes = Rc::new(RefCell::new(Vec::new()));
        let window = retained_layout_window(cx, probes.clone());
        change_and_draw(cx, window, |view| view.keyed = keyed);

        let stats = change_and_draw(cx, window, |view| {
            view.rows += 1;
            view.row_ids.insert(0, 100);
        });
        assert_eq!(probes.borrow().len(), 5);
        stats
    }

    let mut cx = TestAppContext::single();
    let positional = insert_at_head(&mut cx, false);
    let keyed = insert_at_head(&mut cx, true);

    // Shifting keys do not throw nodes away — the row now at index 1
    // claims the node index 1 had — they hand each node to a different row,
    // which then has to write its own style over it. That write is what
    // dirties the node and every ancestor above it.
    assert!(
        positional.style_writes > 0,
        "rows matched by position should be restyled once they shift: {positional:?}"
    );
    assert_eq!(
        keyed.style_writes, 0,
        "rows matched by an ElementId should keep the node they styled, \
         wrote {} against {} for positional rows",
        keyed.style_writes, positional.style_writes
    );
}

/// Timing a measurement or a shaped line reads the clock twice, which is
/// not free on a frame full of text, so the times are kept only once the
/// stats have been reset — which is how a benchmark asks for them. The
/// counts are kept all along.
#[test]
fn layout_times_are_kept_only_once_the_stats_are_reset() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes);
    let stats = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.layout_stats()
        })
        .unwrap()
    };

    let untimed = stats(&mut cx);
    assert!(untimed.compute_layout_calls > 0 && untimed.lines_shaped > 0);
    assert_eq!(untimed.compute_layout_time, Duration::ZERO);
    assert_eq!(untimed.measure_time, Duration::ZERO);
    assert_eq!(untimed.shape_time, Duration::ZERO);

    cx.update_window(window.into(), |_, window, _| window.reset_layout_stats())
        .unwrap();
    change_and_draw(&mut cx, window, |view| {
        view.label = "a label to shape".into()
    });
    let timed = stats(&mut cx);
    assert!(timed.compute_layout_time > Duration::ZERO);
}

/// A chip padded inside a row, its child a probe of where it lands.
fn chip(probes: Rc<RefCell<Vec<Bounds<Pixels>>>>) -> Div {
    div().pl(px(7.)).child(
        canvas(
            move |bounds, _, _| probes.borrow_mut().push(bounds),
            |_, _, _, _| {},
        )
        .w(px(10.))
        .h(px(10.)),
    )
}

/// The chip drawn as a view of its own, which takes a step in the path
/// its contents' nodes are found by without a node of its own.
struct ChipView {
    probes: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl Render for ChipView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        chip(self.probes.clone())
    }
}

/// A chip identified by the view it is drawn in, or by an id of the same
/// value.
struct ReparentedChip {
    chip_view: Entity<ChipView>,
    in_view: bool,
    probes: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl Render for ReparentedChip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.probes.borrow_mut().clear();
        div().flex().pl(px(50.)).child(if self.in_view {
            self.chip_view.clone().into_any_element()
        } else {
            chip(self.probes.clone())
                .id(ElementId::View(self.chip_view.entity_id()))
                .into_any_element()
        })
    }
}

/// A chip that leaves its view for an id of the same value takes the key
/// its child used to find its node by, so it gets a new node, and the
/// child is handed the node the chip had. That node is still listed under
/// the row when the chip's new node adopts it, and the row rewriting its
/// children must not cut the link the child's position is added up along.
#[test]
fn a_node_adopted_from_another_parent_keeps_its_position() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window({
        let probes = probes.clone();
        move |_, cx| ReparentedChip {
            chip_view: cx.new(|_| ChipView {
                probes: probes.clone(),
            }),
            in_view: true,
            probes,
        }
    });

    draw_frame(&mut cx, window.into());
    assert_eq!(probes.borrow()[0].origin.x, px(57.));

    change_and_draw(&mut cx, window, |view| view.in_view = false);
    assert_eq!(
        probes.borrow()[0].origin.x,
        px(57.),
        "the child should still be offset by the row's and the chip's padding"
    );
}

/// Rows of a uniform list five rows tall, scrolled to `scroll_top`.
struct ScrolledRows {
    row_ids: Vec<u64>,
    keyed: bool,
    scroll_top: Pixels,
    scroll: UniformListScrollHandle,
}

impl Render for ScrolledRows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), -self.scroll_top));
        let row_ids = self.row_ids.clone();
        let keyed = self.keyed;
        div().w(px(300.)).h(px(100.)).child(
            uniform_list("rows", row_ids.len(), move |range, _, _| {
                range
                    .map(|ix| {
                        // Rows have to be distinguishable for a row landing
                        // on a neighbour's node to show.
                        let id = row_ids[ix];
                        let row = div().w(px(200.) + px((id % 5) as f32 * 10.)).h(px(20.));
                        if keyed {
                            row.id(("row", id)).into_any_element()
                        } else {
                            row.into_any_element()
                        }
                    })
                    .collect()
            })
            .track_scroll(&self.scroll)
            .size_full(),
        )
    }
}

fn scrolled_rows_window(cx: &mut TestAppContext, keyed: bool) -> WindowHandle<ScrolledRows> {
    let window = cx.add_window(move |_, _| ScrolledRows {
        row_ids: (0..50).collect(),
        keyed,
        scroll_top: px(0.),
        scroll: UniformListScrollHandle::new(),
    });
    draw_frame(cx, window.into());
    draw_frame(cx, window.into());
    window
}

/// A list lays out only the items in view, so an item without an id was
/// matched by where it came among them, and a list scrolled by one row
/// handed every item its neighbour's nodes. Matched by its index, an item
/// keeps its nodes while it stays in view.
#[test]
fn unidentified_list_items_keep_their_nodes_when_the_list_scrolls() {
    let mut cx = TestAppContext::single();
    let window = scrolled_rows_window(&mut cx, false);

    let scrolled = change_and_draw(&mut cx, window, |view| view.scroll_top = px(20.));
    assert_eq!(
        scrolled.style_writes, 0,
        "rows still in view should keep the node they styled: {scrolled:?}"
    );
    assert_eq!(
        scrolled.nodes_created, 1,
        "only the row scrolling in should need a node: {scrolled:?}"
    );
}

/// Keying list items by index must not come between an item and an id of
/// its own: an item identified by its data keeps its nodes when an item
/// is inserted ahead of it, which its index could not do.
#[test]
fn identified_list_items_keep_their_nodes_when_one_is_inserted_ahead() {
    let mut cx = TestAppContext::single();
    let window = scrolled_rows_window(&mut cx, true);

    let inserted = change_and_draw(&mut cx, window, |view| view.row_ids.insert(0, 100));
    assert_eq!(
        inserted.style_writes, 0,
        "identified rows should keep the node they styled: {inserted:?}"
    );
    // The inserted row needs a node, and so does the first row, which the
    // list lays out on its own to find the height of every row. The five
    // rows in view keeping theirs is what an index would have broken.
    assert!(
        inserted.nodes_created < 5,
        "identified rows should not be rebuilt when one is inserted ahead: {inserted:?}"
    );
}

/// Shaping is counted only when the text cache cannot answer, so a frame
/// that shows the same text as the last one shapes nothing, and one that
/// shows new text shapes exactly that.
#[test]
fn only_text_the_cache_does_not_hold_is_counted_as_shaped() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes);
    draw_frame(&mut cx, window.into());

    let unchanged = draw_frame(&mut cx, window.into());
    assert_eq!(
        unchanged.lines_shaped, 0,
        "text shown last frame should come from the cache: {unchanged:?}"
    );

    let relabeled = change_and_draw(&mut cx, window, |view| view.label = "cd".into());
    assert!(
        relabeled.lines_shaped > 0,
        "text not shown before has to be shaped: {relabeled:?}"
    );
}

/// A window root, sized explicitly or left to fill the window, that records
/// where its child ends up. The same as the one `window.rs`'s tests use.
struct RootView {
    explicit_size: bool,
    child_bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl Render for RootView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let child_bounds = self.child_bounds.clone();
        let root = div().flex().flex_col().child(
            canvas(
                move |bounds, _, _| child_bounds.set(bounds),
                |_, _, _, _| {},
            )
            .size_full(),
        );
        if self.explicit_size {
            root.w(px(300.)).h(px(200.))
        } else {
            root
        }
    }
}

/// The window root is the one node whose style Taffy does not hold as the
/// element wrote it, because an `auto` size is rewritten to fill the
/// viewport. Retaining that node means the rewrite has to stay recoverable
/// across frames, or the root silently stops following the window.
#[test]
fn a_retained_auto_sized_root_keeps_filling_a_resized_window() {
    let mut cx = TestAppContext::single();
    let child_bounds = Rc::new(Cell::new(Bounds::default()));
    let window = cx.add_window({
        let child_bounds = child_bounds.clone();
        move |_, _| RootView {
            explicit_size: false,
            child_bounds,
        }
    });
    let handle: AnyWindowHandle = window.into();

    for resized_size in [
        size(px(800.), px(600.)),
        size(px(640.), px(480.)),
        size(px(1024.), px(768.)),
        // Back to a size already seen, to catch a stale record of the
        // previous fill rather than of the request behind it.
        size(px(800.), px(600.)),
    ] {
        cx.simulate_window_resize(handle, resized_size);
        draw_frame(&mut cx, handle);
        assert_eq!(
            child_bounds.get().size,
            resized_size,
            "an auto-sized root must still fill the window after it is resized"
        );
    }

    // And a frame that changes nothing must leave the stretched root alone
    // rather than rewriting it and dirtying the whole tree.
    let stats = draw_frame(&mut cx, handle);
    assert_eq!(
        stats.style_writes, 0,
        "a settled auto-sized root should not be restyled every frame: {stats:?}"
    );
}

#[test]
fn retaining_layout_nodes_does_not_grow_the_tree_over_time() {
    let mut cx = TestAppContext::single();
    let probes = Rc::new(RefCell::new(Vec::new()));
    let window = retained_layout_window(&mut cx, probes);

    for frame in 0..12 {
        // Oscillate the shape so nodes are created and released repeatedly
        // rather than settling.
        change_and_draw(&mut cx, window, |view| view.rows = 2 + frame % 5);
    }

    let live = cx
        .update_window(window.into(), |_, window, _| window.layout_node_count())
        .unwrap();
    change_and_draw(&mut cx, window, |view| view.rows = 2);
    let settled = cx
        .update_window(window.into(), |_, window, _| window.layout_node_count())
        .unwrap();

    assert!(
        settled <= live,
        "the tree should shrink back down, held {live} nodes and settled at {settled}"
    );
    assert!(
        settled < 40,
        "two rows should not need {settled} layout nodes"
    );
}

/// Text wrapped in a narrow box, above a probe that lands below it.
struct WrappedText {
    text: SharedString,
    probe: Rc<Cell<Bounds<Pixels>>>,
}

impl Render for WrappedText {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let probe = self.probe.clone();
        div()
            .flex()
            .flex_col()
            .child(div().w(px(40.)).child(self.text.clone()))
            .child(canvas(move |bounds, _, _| probe.set(bounds), |_, _, _, _| {}).h(px(5.)))
    }
}

/// Text that changed but measures what it measured before, under every
/// constraint it was measured under, leaves its node and the nodes above it
/// clean; text that wraps differently does not. Either way the frame is the
/// one a window drawing from scratch draws.
#[test]
fn changed_text_measuring_the_same_leaves_its_layout_alone() {
    let mut cx = TestAppContext::single();
    let probe = Rc::new(Cell::new(Bounds::default()));
    let window = cx.add_window({
        let probe = probe.clone();
        move |_, _| WrappedText {
            text: "aaaa bbbb cccc dddd".into(),
            probe,
        }
    });
    draw_frame(&mut cx, window.into());
    let from_scratch = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| {
            let retained = window.describe_rendered_frame();
            window.forget_retained_state();
            window.draw(cx).clear(cx);
            (retained, window.describe_rendered_frame())
        })
        .unwrap()
    };

    let wrapped = probe.get();
    let same_shape = change_and_draw(&mut cx, window, |view| {
        view.text = "eeee ffff gggg hhhh".into()
    });
    assert_eq!(
        (same_shape.measure_rebinds, same_shape.measurements_replayed),
        (0, 1),
        "{same_shape:?}"
    );
    assert_eq!(probe.get(), wrapped);
    let (retained, fresh) = from_scratch(&mut cx);
    assert_eq!(retained, fresh);

    let unwrapped = change_and_draw(&mut cx, window, |view| view.text = "e".into());
    assert_eq!(
        (unwrapped.measure_rebinds, unwrapped.measurements_replayed),
        (1, 0),
        "{unwrapped:?}"
    );
    assert!(probe.get().origin.y < wrapped.origin.y);
    let (retained, fresh) = from_scratch(&mut cx);
    assert_eq!(retained, fresh);
}

/// A leaf that measures itself through [`Window::request_measured_layout`],
/// as a rich text element laying out its own lines does: as tall as `height`
/// and as wide as it is allowed, and painted as a quad.
struct MeasuredLeaf {
    height: Rc<Cell<f32>>,
}

impl IntoElement for MeasuredLeaf {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl crate::Element for MeasuredLeaf {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&crate::GlobalElementId>,
        _: Option<&crate::InspectorElementId>,
        window: &mut Window,
        _: &mut crate::App,
    ) -> (crate::LayoutId, ()) {
        let height = self.height.get();
        let id = window.request_measured_layout(
            crate::Style::default(),
            move |known, available, _, _| {
                let width = known.width.unwrap_or(match available.width {
                    crate::AvailableSpace::Definite(width) => width,
                    _ => px(80.),
                });
                size(width, px(height))
            },
        );
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&crate::GlobalElementId>,
        _: Option<&crate::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut crate::App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&crate::GlobalElementId>,
        _: Option<&crate::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut crate::App,
    ) {
        window.paint_quad(crate::fill(bounds, hsla(0.6, 0.5, 0.5, 1.0)));
    }
}

/// A [`MeasuredLeaf`] in a narrow box, above a probe that lands below it.
struct MeasuredLeafView {
    height: Rc<Cell<f32>>,
    probe: Rc<Cell<Bounds<Pixels>>>,
}

impl Render for MeasuredLeafView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let probe = self.probe.clone();
        div()
            .flex()
            .flex_col()
            .child(div().w(px(40.)).child(MeasuredLeaf {
                height: self.height.clone(),
            }))
            .child(canvas(move |bounds, _, _| probe.set(bounds), |_, _, _, _| {}).h(px(5.)))
    }
}

/// A leaf measuring itself through the public API is measured again every
/// frame it is built, its closure being new. Measuring what it measured
/// before, under every constraint it was measured under, leaves its node and
/// the nodes above it clean; measuring something else lays them out afresh.
/// Either way the frame is the one a window drawing from scratch draws.
#[test]
fn a_rebuilt_measured_leaf_measuring_the_same_leaves_its_layout_alone() {
    let mut cx = TestAppContext::single();
    let height = Rc::new(Cell::new(20.));
    let probe = Rc::new(Cell::new(Bounds::default()));
    let window = cx.add_window({
        let height = height.clone();
        let probe = probe.clone();
        move |_, _| MeasuredLeafView { height, probe }
    });
    draw_frame(&mut cx, window.into());
    draw_frame(&mut cx, window.into());
    let from_scratch = |cx: &mut TestAppContext| {
        cx.update_window(window.into(), |_, window, cx| {
            let retained = window.describe_rendered_frame();
            window.forget_retained_state();
            window.draw(cx).clear(cx);
            (retained, window.describe_rendered_frame())
        })
        .unwrap()
    };

    let below = probe.get();
    let same = change_and_draw(&mut cx, window, |_| {});
    assert!(same.measurements_replayed >= 1, "{same:?}");
    assert_eq!(probe.get(), below);
    let (retained, fresh) = from_scratch(&mut cx);
    assert_eq!(retained, fresh);

    let taller = change_and_draw(&mut cx, window, |_| height.set(40.));
    assert_eq!(taller.measurements_replayed, 0, "{taller:?}");
    assert_eq!(probe.get().origin.y, below.origin.y + px(20.));
    let (retained, fresh) = from_scratch(&mut cx);
    assert_eq!(retained, fresh);
}
