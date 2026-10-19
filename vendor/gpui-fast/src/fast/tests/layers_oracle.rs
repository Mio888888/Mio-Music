// Modified by Longbridge for gpui-fast.
//! Frames drawn through scroll layers must match frames drawn without them.
//!
//! Each run drives two windows holding the same view through the same random
//! history: wheel scrolls with fractional deltas, programmatic scrolls,
//! pointer moves, clicks, key presses into a focused element inside scrolled
//! content, content changes and resizes, at scale factors 1 and 1.25. One
//! window has scroll layers on, the other off. After every step:
//!
//! - the two frames must draw the same thing. The layer window's scene is
//!   expanded first: the tile quads of each composited layer are replaced by
//!   the layer's content, moved to where the tiles put it and clipped to
//!   their viewport, and every piece of content must lie under a composited
//!   tile. Both scenes are then compared primitive by primitive in a
//!   canonical drawing order (see `fast::layers::verify`);
//! - hit tests at random points must find the same hitboxes;
//! - every listener must have seen the same events at the same positions,
//!   with the same hitbox bounds.
//!
//! Where layers are compiled in, the runs must composite layers; elsewhere
//! the two windows draw the same way and the run checks only that the
//! oracle itself holds.

use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

use super::oracle::GlyphBoxTextSystem;
use crate::fast::layers::verify::drawn;
use crate::{
    AnyElement, AppContext as _, Bounds, ContentMask, Context, DispatchPhase, Div, Entity,
    FocusHandle, HitboxBehavior, Hsla, InputEvent as _, InteractiveElement as _, IntoElement,
    KeyDownEvent, LayerFrame, LayerKey, ListAlignment, ListOffset, ListState, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, NoopTextSystem, ParentElement as _,
    Pixels, PlatformInput, Point, PolychromeSprite, Quad, Render, ScaledPixels, Scene, ScrollDelta,
    ScrollHandle, ScrollStrategy, ScrollWheelEvent, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestAppContext, TileCoord, TouchPhase, UniformListScrollHandle, Window,
    WindowHandle, canvas, div, hsla, layer_tile_id, layer_tile_texture_id, list, point, px, rgba,
    size, uniform_list, util::FluentBuilder as _, white,
};

fn sp(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: point(ScaledPixels(x), ScaledPixels(y)),
        size: size(ScaledPixels(width), ScaledPixels(height)),
    }
}

fn quad(bounds: Bounds<ScaledPixels>, mask: Bounds<ScaledPixels>, hue: f32) -> Quad {
    Quad {
        bounds,
        content_mask: ContentMask { bounds: mask },
        background: hsla(hue, 0.5, 0.5, 1.).into(),
        ..Default::default()
    }
}

const WINDOW: Bounds<ScaledPixels> = Bounds {
    origin: crate::Point {
        x: ScaledPixels(0.),
        y: ScaledPixels(0.),
    },
    size: crate::Size {
        width: ScaledPixels(1000.),
        height: ScaledPixels(1000.),
    },
};

/// A scroll container whose viewport is `(50, 50) – (250, 150)`, scrolled
/// 400 px down since its content was painted, so that it shows the bottom of
/// tile (0, 0) and the top of tile (0, 1), over a background, with a bar
/// painted over its bottom edge afterwards: once drawn directly, and once
/// through a layer holding the content with `tiles` of it composited. `nudge`
/// moves a visible row of the layer's content.
fn scenes(tiles: &[TileCoord], nudge: f32) -> (Scene, Scene) {
    let viewport = sp(50., 50., 200., 100.);
    let translation = point(ScaledPixels(0.), ScaledPixels(-400.));
    let rows = || (0..20).map(|i| sp(60., 60. + i as f32 * 30., 100., 20.));

    let mut direct = Scene::default();
    direct.insert_primitive(quad(WINDOW, WINDOW, 0.));
    for (i, row) in rows().enumerate() {
        let row = Bounds {
            origin: row.origin + translation,
            size: row.size,
        };
        direct.insert_primitive(quad(row, viewport, 0.1 + i as f32 * 0.01));
    }
    direct.insert_primitive(quad(sp(40., 140., 300., 20.), WINDOW, 0.9));
    direct.finish();

    let painted_region = sp(50., 50., 200., 800.);
    let mut content = Scene::default();
    for (i, row) in rows().enumerate() {
        let row = if i == 14 {
            Bounds {
                origin: point(row.origin.x + ScaledPixels(nudge), row.origin.y),
                size: row.size,
            }
        } else {
            row
        };
        content.insert_primitive(quad(row, painted_region, 0.1 + i as f32 * 0.01));
    }
    content.finish();
    let frame = LayerFrame {
        key: LayerKey(3),
        generation: 1,
        background: rgba(0xffffffff),
        tile_size: 512,
        content: content.into(),
        dirty_tiles: Vec::new(),
    };

    let mut layered = Scene::default();
    layered.insert_primitive(quad(WINDOW, WINDOW, 0.));
    layered.push_layer(viewport);
    for &tile in tiles {
        let bounds = frame.tile_bounds(tile);
        layered.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale: false.into(),
            opacity: 1.,
            bounds: Bounds {
                origin: bounds.origin + translation,
                size: bounds.size,
            },
            content_mask: ContentMask { bounds: viewport },
            corner_radii: Default::default(),
            tile: crate::AtlasTile {
                texture_id: layer_tile_texture_id(frame.key),
                tile_id: layer_tile_id(tile),
                padding: 0,
                bounds: Bounds {
                    origin: point(crate::DevicePixels(0), crate::DevicePixels(0)),
                    size: size(crate::DevicePixels(512), crate::DevicePixels(512)),
                },
            },
        });
    }
    layered.pop_layer();
    layered.insert_primitive(quad(sp(40., 140., 300., 20.), WINDOW, 0.9));
    layered.layers.frames.push(frame);
    layered.finish();
    (direct, layered)
}

#[test]
fn a_composited_layer_expands_to_the_scene_drawn_directly() {
    let (direct, layered) = scenes(&[TileCoord { x: 0, y: 0 }, TileCoord { x: 0, y: 1 }], 0.);
    assert_eq!(drawn(&layered), drawn(&direct));
    assert!(
        drawn(&direct).len() > 3,
        "rows in the viewport are compared"
    );
}

#[test]
fn a_layer_that_moved_a_row_does_not_match() {
    let (direct, layered) = scenes(&[TileCoord { x: 0, y: 0 }, TileCoord { x: 0, y: 1 }], 1.);
    assert_ne!(drawn(&layered), drawn(&direct));
}

#[test]
#[should_panic(expected = "no tile composited")]
fn content_under_a_tile_that_was_not_composited_is_caught() {
    let (_, layered) = scenes(&[TileCoord { x: 0, y: 1 }], 0.);
    drawn(&layered);
}

const WORDS: [&str; 6] = ["a", "row", "scrolled", "lorem ipsum", "x", "a longer label"];

const PALETTE: [Hsla; 5] = [
    hsla(0.0, 0.0, 0.1, 1.0),
    hsla(0.6, 0.7, 0.5, 1.0),
    hsla(0.3, 0.6, 0.4, 1.0),
    hsla(0.0, 0.8, 0.6, 1.0),
    hsla(0.1, 0.9, 0.5, 0.5),
];

const CONTAINERS: usize = 4;
const CONTAINER_WIDTH: f32 = 220.;
const CONTAINER_HEIGHT: f32 = 200.;
const CONTAINER_TOP: f32 = 30.;
const INITIAL_ROWS: u64 = 60;
const UNIFORM_ROW_HEIGHT: f32 = 22.;
/// Wheel deltas, as trackpads (fractions of a pixel) and wheels send them.
const WHEEL_DELTAS: [f32; 5] = [0.37, 1.0, 12.5, 40., 90.];

fn container_left(container: usize) -> f32 {
    10. + container as f32 * (CONTAINER_WIDTH + 10.)
}

/// What every listener in the view saw, in order.
type Log = Rc<RefCell<Vec<String>>>;

#[derive(Clone, Copy, Debug)]
struct Row {
    id: u64,
    color: usize,
    word: usize,
}

struct Rows {
    rows: Vec<Row>,
    next_id: u64,
}

/// A row of scrolled content: text, a swatch, some with a hover style, a
/// click listener and a probe recording where the mouse went down in it.
fn render_row(
    container: &'static str,
    row: Row,
    tint: usize,
    log: &Log,
    fixed: bool,
) -> AnyElement {
    let label = SharedString::from(format!("{container}/{}", row.id));
    div()
        .id(label.clone())
        .relative()
        .flex()
        .flex_row()
        .gap_1()
        // Glyphs stay inside their line and the line inside the row. A glyph
        // reaching past its row is drawn from a layer's overscan row where a
        // list leaves the row out, and one reaching past its line is drawn
        // at the line's ordering: both differ from drawing from scratch in
        // ways the rows are not here to test.
        .text_size(px(8.))
        .line_height(px(12.))
        .h(px(if fixed {
            UNIFORM_ROW_HEIGHT
        } else {
            18. + (row.id % 3) as f32 * 7.
        }))
        .when(row.id.is_multiple_of(2), |this| {
            this.bg(PALETTE[(row.color + tint) % PALETTE.len()])
        })
        .when(row.id.is_multiple_of(3), |this| {
            this.hover(|style| style.bg(PALETTE[4]))
        })
        .child(SharedString::from(format!(
            "{} {}",
            WORDS[row.word], row.id
        )))
        .child(
            div()
                .w(px(10. + (row.id % 4) as f32 * 5.))
                .h(px(8.))
                .bg(PALETTE[row.id as usize % PALETTE.len()]),
        )
        .on_click({
            let log = log.clone();
            let label = label.clone();
            move |event, _, _| {
                log.borrow_mut()
                    .push(format!("click {label} at {:?}", event.position()))
            }
        })
        .child(probe(label, log.clone()))
        .into_any_element()
}

/// Records, for every mouse down over it, where it happened and the bounds
/// of the hitbox its listener was given.
fn probe(label: SharedString, log: Log) -> impl IntoElement {
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    log.borrow_mut().push(format!(
                        "down {label} at {:?} in {:?}",
                        event.position, hitbox.bounds
                    ));
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// Content of the first container: a child view (pattern A) holding a
/// focusable field and the rows.
struct ContentView {
    rows: Entity<Rows>,
    log: Log,
    focus: FocusHandle,
    /// Where the field was last prepainted, which its key listener reports.
    field_bounds: Rc<Cell<Bounds<Pixels>>>,
    tint: usize,
}

impl Render for ContentView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        let field_bounds = self.field_bounds.clone();
        let field = div()
            .id("field")
            .relative()
            .h(px(20.))
            .bg(PALETTE[1])
            .track_focus(&self.focus)
            .on_key_down({
                let field_bounds = field_bounds.clone();
                move |event: &KeyDownEvent, _, _| {
                    log.borrow_mut().push(format!(
                        "key {} in {:?}",
                        event.keystroke.key,
                        field_bounds.get()
                    ))
                }
            })
            .child(
                canvas(
                    move |bounds, _, _| field_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        div().flex().flex_col().child(field).children(
            self.rows
                .read(cx)
                .rows
                .iter()
                .map(|&row| render_row("a", row, self.tint, &self.log, false)),
        )
    }
}

/// Four scroll containers side by side on opaque panels, one per pattern:
/// a scrolling div holding a child view, a scrolling div holding the rows
/// directly, a uniform list and a list, with a bar painted over the bottom of
/// the first one afterwards.
struct LayerOracleView {
    rows: Entity<Rows>,
    content: Entity<ContentView>,
    scroll_a: ScrollHandle,
    scroll_b: ScrollHandle,
    uniform: UniformListScrollHandle,
    list: ListState,
    log: Log,
}

impl LayerOracleView {
    fn new(cx: &mut Context<Self>) -> Self {
        let rows = cx.new(|_| Rows {
            rows: (0..INITIAL_ROWS)
                .map(|id| Row {
                    id,
                    color: id as usize % PALETTE.len(),
                    word: id as usize % WORDS.len(),
                })
                .collect(),
            next_id: INITIAL_ROWS,
        });
        let log = Log::default();
        let content = cx.new(|cx| ContentView {
            rows: rows.clone(),
            log: log.clone(),
            focus: cx.focus_handle(),
            field_bounds: Rc::default(),
            tint: 0,
        });
        Self {
            rows,
            content,
            scroll_a: ScrollHandle::new(),
            scroll_b: ScrollHandle::new(),
            uniform: UniformListScrollHandle::new(),
            list: ListState::new(INITIAL_ROWS as usize, ListAlignment::Top, px(40.)),
            log,
        }
    }
}

fn panel(container: usize) -> Div {
    div()
        .absolute()
        .left(px(container_left(container)))
        .top(px(CONTAINER_TOP))
        .w(px(CONTAINER_WIDTH))
        .h(px(CONTAINER_HEIGHT))
        .bg(white())
}

impl Render for LayerOracleView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.read(cx).rows.clone();
        let log = self.log.clone();
        div()
            .size_full()
            .relative()
            .bg(hsla(0.6, 0.1, 0.9, 1.))
            .text_color(PALETTE[0])
            .child(
                panel(0).child(
                    div()
                        .id("a")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_a)
                        .child(self.content.clone()),
                ),
            )
            .child(
                panel(1).child(
                    div()
                        .id("b")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_b)
                        .child(div().flex().flex_col().children(
                            rows.iter().map(|&row| render_row("b", row, 0, &log, false)),
                        )),
                ),
            )
            .child(
                panel(2).child(
                    uniform_list("u", rows.len(), {
                        let rows = rows.clone();
                        let log = log.clone();
                        move |range, _, _| {
                            range
                                .map(|ix| render_row("u", rows[ix], 0, &log, true))
                                .collect()
                        }
                    })
                    .track_scroll(&self.uniform)
                    .size_full(),
                ),
            )
            .child(
                panel(3).child(
                    list(self.list.clone(), move |ix, _, _| {
                        render_row("l", rows[ix], 0, &log, false)
                    })
                    .size_full(),
                ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(container_left(0) - 4.))
                    .top(px(CONTAINER_TOP + CONTAINER_HEIGHT - 8.))
                    .w(px(CONTAINER_WIDTH + 40.))
                    .h(px(14.))
                    .bg(PALETTE[2]),
            )
    }
}

#[derive(Clone, Debug)]
enum Change {
    /// A wheel turned over a container, `y` below its top.
    Wheel {
        container: usize,
        y: f32,
        delta: f32,
    },
    /// A scroll by the application, to `top`.
    Scroll {
        container: usize,
        top: f32,
    },
    MoveMouse {
        x: f32,
        y: f32,
    },
    Click {
        x: f32,
        y: f32,
    },
    /// A click where the pointer is, without moving it first.
    ClickInPlace,
    /// Focuses the field inside the first container's content.
    Focus,
    Key {
        key: &'static str,
    },
    Recolor {
        row: usize,
        color: usize,
    },
    Reword {
        row: usize,
        word: usize,
    },
    /// Changes the first container's content view alone.
    Tint,
    InsertRow {
        at: usize,
    },
    RemoveRow {
        at: usize,
    },
    Resize {
        width: f32,
        height: f32,
    },
    Redraw,
}

impl Change {
    /// A random change, wheels and application scrolls going to one of
    /// `containers`.
    fn random(rng: &mut StdRng, containers: &Range<usize>) -> Self {
        let container = rng.random_range(containers.clone());
        match rng.random_range(0..100) {
            0..30 => Change::Wheel {
                container,
                y: rng.random_range(2.0..CONTAINER_HEIGHT - 2.),
                delta: WHEEL_DELTAS[rng.random_range(0..WHEEL_DELTAS.len())]
                    * if rng.random_bool(0.6) { -1. } else { 1. },
            },
            30..36 => Change::Scroll {
                container,
                top: rng.random_range(0.0..900.0),
            },
            36..46 => Change::MoveMouse {
                x: rng.random_range(0.0..960.0),
                y: rng.random_range(0.0..260.0),
            },
            46..54 => Change::Click {
                x: rng.random_range(0.0..960.0),
                y: rng.random_range(0.0..260.0),
            },
            54..60 => Change::ClickInPlace,
            60..63 => Change::Focus,
            63..68 => Change::Key {
                key: ["a", "b", "space", "enter"][rng.random_range(0..4)],
            },
            68..75 => Change::Recolor {
                row: rng.random_range(0..80),
                color: rng.random_range(0..PALETTE.len()),
            },
            75..80 => Change::Reword {
                row: rng.random_range(0..80),
                word: rng.random_range(0..WORDS.len()),
            },
            80..85 => Change::Tint,
            85..89 => Change::InsertRow {
                at: rng.random_range(0..80),
            },
            89..93 => Change::RemoveRow {
                at: rng.random_range(0..80),
            },
            93..95 => Change::Resize {
                width: rng.random_range(500.0..1000.0),
                height: rng.random_range(200.0..700.0),
            },
            _ => Change::Redraw,
        }
    }
}

fn dispatch(cx: &mut TestAppContext, window: WindowHandle<LayerOracleView>, event: PlatformInput) {
    cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(event, cx);
    })
    .unwrap();
}

fn apply(cx: &mut TestAppContext, window: WindowHandle<LayerOracleView>, change: &Change) {
    match *change {
        Change::Wheel {
            container,
            y,
            delta,
        } => dispatch(
            cx,
            window,
            ScrollWheelEvent {
                position: point(
                    px(container_left(container) + CONTAINER_WIDTH / 2.),
                    px(CONTAINER_TOP + y),
                ),
                delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
                modifiers: Modifiers::default(),
                touch_phase: TouchPhase::Moved,
            }
            .to_platform_input(),
        ),
        Change::Scroll { container, top } => {
            window
                .update(cx, |view, _, cx| {
                    let top = px(top);
                    match container {
                        0 => view.scroll_a.set_offset(point(px(0.), -top)),
                        1 => view.scroll_b.set_offset(point(px(0.), -top)),
                        2 => view.uniform.scroll_to_item(
                            (top / px(UNIFORM_ROW_HEIGHT)) as usize,
                            ScrollStrategy::Top,
                        ),
                        _ => view.list.scroll_to(ListOffset {
                            item_ix: (top / px(25.)) as usize,
                            offset_in_item: px(top.as_f32() % 11.),
                        }),
                    }
                    cx.notify();
                })
                .unwrap();
        }
        Change::MoveMouse { x, y } => dispatch(
            cx,
            window,
            MouseMoveEvent {
                position: point(px(x), px(y)),
                pressed_button: None,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
        ),
        Change::Click { x, y } => click(cx, window, point(px(x), px(y))),
        Change::ClickInPlace => {
            let position = cx
                .update_window(window.into(), |_, window, _| window.mouse_position())
                .unwrap();
            click(cx, window, position);
        }
        Change::Focus => {
            window
                .update(cx, |view, window, cx| {
                    let focus = view.content.read(cx).focus.clone();
                    window.focus(&focus, cx);
                })
                .unwrap();
        }
        Change::Key { key } => cx.simulate_keystrokes(window.into(), key),
        Change::Recolor { .. }
        | Change::Reword { .. }
        | Change::InsertRow { .. }
        | Change::RemoveRow { .. } => {
            window
                .update(cx, |view, _, cx| {
                    let list = view.list.clone();
                    view.rows
                        .update(cx, |rows, cx| change_rows(rows, &list, change, cx));
                    view.content.update(cx, |_, cx| cx.notify());
                    cx.notify();
                })
                .unwrap();
        }
        Change::Tint => {
            let content = window
                .read_with(cx, |view, _| view.content.clone())
                .unwrap();
            content.update(cx, |content, cx| {
                content.tint += 1;
                cx.notify();
            });
        }
        Change::Resize { width, height } => {
            cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        }
        Change::Redraw => {}
    }
}

fn change_rows(rows: &mut Rows, list: &ListState, change: &Change, cx: &mut Context<Rows>) {
    let len = rows.rows.len();
    match *change {
        Change::Recolor { row, color } if len > 0 => rows.rows[row % len].color = color,
        Change::Reword { row, word } if len > 0 => rows.rows[row % len].word = word,
        Change::InsertRow { at } => {
            let at = at % (len + 1);
            let id = rows.next_id;
            rows.next_id += 1;
            rows.rows.insert(
                at,
                Row {
                    id,
                    color: id as usize % PALETTE.len(),
                    word: id as usize % WORDS.len(),
                },
            );
            list.splice(at..at, 1);
        }
        Change::RemoveRow { at } if len > 0 => {
            let at = at % len;
            rows.rows.remove(at);
            list.splice(at..at + 1, 0);
        }
        _ => {}
    }
    cx.notify();
}

fn click(cx: &mut TestAppContext, window: WindowHandle<LayerOracleView>, position: Point<Pixels>) {
    dispatch(
        cx,
        window,
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
    );
    dispatch(
        cx,
        window,
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
    );
}

/// What a window drew, what `points` hit in it, and what its listeners saw
/// since the last call.
struct Observed {
    drawn: Vec<String>,
    hits: Vec<String>,
    log: Vec<String>,
}

fn draw(
    cx: &mut TestAppContext,
    window: WindowHandle<LayerOracleView>,
    points: &[Point<Pixels>],
) -> Observed {
    let log = window.read_with(cx, |view, _| view.log.take()).unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        let frame = &window.rendered_frame;
        let hits = points
            .iter()
            .map(|&position| {
                let hit = frame.hit_test(position);
                let hitboxes: Vec<String> = hit
                    .ids
                    .iter()
                    .map(|id| {
                        let hitbox = frame
                            .hitboxes
                            .iter()
                            .find(|hitbox| hitbox.id == *id)
                            .unwrap();
                        // To a thousandth of a pixel: a row of a list's layer
                        // keeps the hitboxes it inserted, moved, and a bound
                        // laid out and moved can differ in its last bits
                        // from one laid out where it lies now.
                        let bounds = hitbox.bounds.intersect(&hitbox.content_mask.bounds);
                        format!(
                            "({:.3}, {:.3}) {:.3} × {:.3} {:?}",
                            bounds.origin.x.as_f32(),
                            bounds.origin.y.as_f32(),
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                            hitbox.behavior
                        )
                    })
                    .collect();
                format!(
                    "{position:?}: {hitboxes:?}, hover {}",
                    hit.hover_hitbox_count
                )
            })
            .collect();
        Observed {
            drawn: drawn(&frame.scene),
            hits,
            log,
        }
    })
    .unwrap()
}

fn first_difference(actual: &[String], expected: &[String]) -> String {
    let first = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| actual != expected)
        .unwrap_or(actual.len().min(expected.len()));
    let excerpt = |lines: &[String]| {
        lines
            .iter()
            .enumerate()
            .skip(first.saturating_sub(2))
            .take(5)
            .map(|(ix, line)| format!("  {ix}: {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "at line {first} ({} lines against {})\nwith layers:\n{}\nwithout:\n{}",
        actual.len(),
        expected.len(),
        excerpt(actual),
        excerpt(expected)
    )
}

/// What a run went through, to tell that it compared something.
#[derive(Default)]
struct Coverage {
    /// Frames of the layer window that composited a scroll layer.
    composited: u64,
    /// Events listeners saw, by kind: clicks, mouse downs, key presses.
    clicks: usize,
    downs: usize,
    keys: usize,
    /// Frames drawn with some container scrolled.
    scrolled: usize,
    /// Frames that composited a virtual list's layer keeping the rows it
    /// held, and rows they rendered again because their hovers changed.
    list_frames: usize,
    rows_rendered_for_hover: usize,
}

/// Drives a window with layers and one without through one random history
/// at `scale_factor`, scrolling `containers`, a wheel turning on for
/// another frame with probability `keep_turning`.
fn run(
    seed: u64,
    scale_factor: f32,
    steps: usize,
    containers: Range<usize>,
    keep_turning: f64,
) -> Coverage {
    let mut cx = TestAppContext::with_text_system(Arc::new(GlyphBoxTextSystem(NoopTextSystem)));
    let layered = cx.add_window(|_, cx| LayerOracleView::new(cx));
    let plain = cx.add_window(|_, cx| LayerOracleView::new(cx));
    for (window, layers) in [(layered, true), (plain, false)] {
        cx.simulate_window_scale_factor_change(window.into(), scale_factor);
        cx.update_window(window.into(), |_, window, cx| {
            window.set_scroll_layers(layers);
            window.draw(cx).clear(cx);
            window.reset_layout_stats();
        })
        .unwrap();
    }
    let list_frames = crate::fast::layers::lists::extended_frames();
    let rows_rendered_for_hover = crate::fast::layers::lists::rows_rendered_for_hover();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut history: Vec<Vec<Change>> = Vec::new();
    let mut last_wheel: Option<Change> = None;
    let mut coverage = Coverage::default();

    for step in 0..steps {
        // A wheel turns for several frames in a row, which is what promotes
        // a container to a layer.
        let changes: Vec<Change> = match &last_wheel {
            Some(wheel) if rng.random_bool(keep_turning) => vec![wheel.clone()],
            _ => (0..rng.random_range(1..=2))
                .map(|_| Change::random(&mut rng, &containers))
                .collect(),
        };
        last_wheel = changes
            .iter()
            .find(|change| matches!(change, Change::Wheel { .. }))
            .cloned();
        let points: Vec<Point<Pixels>> = (0..32)
            .map(|_| {
                point(
                    px(rng.random_range(0.0..960.0)),
                    px(rng.random_range(0.0..260.0)),
                )
            })
            .collect();
        for change in &changes {
            apply(&mut cx, layered, change);
            apply(&mut cx, plain, change);
        }
        history.push(changes);

        let expected = draw(&mut cx, plain, &points);
        let actual = draw(&mut cx, layered, &points);
        let failure = if actual.log != expected.log {
            Some(format!(
                "listeners saw different events {}",
                first_difference(&actual.log, &expected.log)
            ))
        } else if actual.drawn != expected.drawn {
            Some(format!(
                "the frames differ {}",
                first_difference(&actual.drawn, &expected.drawn)
            ))
        } else if actual.hits != expected.hits {
            Some(format!(
                "hit tests differ {}",
                first_difference(&actual.hits, &expected.hits)
            ))
        } else {
            None
        };
        if let Some(failure) = failure {
            let history = history
                .iter()
                .enumerate()
                .map(|(step, changes)| format!("  {step}: {changes:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            panic!(
                "seed {seed}, scale {scale_factor}, step {step}: {failure}\n\
                 changes so far:\n{history}"
            );
        }
        let count = |kind: &str| {
            expected
                .log
                .iter()
                .filter(|line| line.starts_with(kind))
                .count()
        };
        coverage.clicks += count("click ");
        coverage.downs += count("down ");
        coverage.keys += count("key ");
        coverage.scrolled += plain
            .read_with(&cx, |view, _| {
                view.scroll_a.offset().y != px(0.)
                    || view.scroll_b.offset().y != px(0.)
                    || view.uniform.0.borrow().base_handle.offset().y != px(0.)
                    || view.list.logical_scroll_top().item_ix > 0
            })
            .unwrap() as usize;
    }
    coverage.composited = cx
        .update_window(layered.into(), |_, window, _| {
            window.layout_stats().layer_frames_composited
        })
        .unwrap();
    coverage.list_frames = crate::fast::layers::lists::extended_frames() - list_frames;
    coverage.rows_rendered_for_hover =
        crate::fast::layers::lists::rows_rendered_for_hover() - rows_rendered_for_hover;
    coverage
}

/// Runs three random histories at scale factors 1 and 1.25, as [`run`]
/// does, and adds up what they went through.
fn run_all(containers: Range<usize>, keep_turning: f64) -> Coverage {
    let mut total = Coverage::default();
    for seed in 0..3 {
        for scale_factor in [1., 1.25] {
            let coverage = run(seed, scale_factor, 300, containers.clone(), keep_turning);
            total.composited += coverage.composited;
            total.clicks += coverage.clicks;
            total.downs += coverage.downs;
            total.keys += coverage.keys;
            total.scrolled += coverage.scrolled;
            total.list_frames += coverage.list_frames;
            total.rows_rendered_for_hover += coverage.rows_rendered_for_hover;
        }
    }
    total
}

#[test]
fn frames_drawn_through_scroll_layers_match_frames_drawn_without() {
    let total = run_all(0..CONTAINERS, 0.7);
    assert!(
        total.clicks > 0 && total.downs > 0 && total.keys > 0,
        "no listener saw a click, a mouse down and a key press, so their positions \
         were not compared"
    );
    assert!(total.scrolled > 0, "nothing ever scrolled");
    // Where scroll layers are compiled in, the runs must have composited
    // layers, or they compared nothing but drawing from scratch with itself.
    if crate::fast::layers::COMPILED {
        assert!(
            total.composited > 0,
            "no frame composited a scroll layer over {} scrolled frames",
            total.scrolled
        );
    }
}

/// The same, scrolling only the uniform list and the list, whose layers keep
/// their rows from frame to frame, carry their rows' hitboxes and listeners,
/// and render again the rows whose hover changes.
#[test]
fn list_frames_drawn_through_scroll_layers_match_frames_drawn_without() {
    let total = run_all(2..CONTAINERS, 0.9);
    assert!(
        total.clicks > 0 && total.downs > 0,
        "no listener saw a click and a mouse down, so their positions were not compared"
    );
    if crate::fast::layers::COMPILED {
        assert!(
            total.list_frames > 50 && total.rows_rendered_for_hover > 0,
            "no frame kept the rows of a list's layer ({}), or rendered one again \
             for its hover ({})",
            total.list_frames,
            total.rows_rendered_for_hover
        );
    }
}
