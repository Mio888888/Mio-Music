// Modified by Longbridge for gpui-fast.
//! Tests of scroll layers over virtual lists (M6): a `uniform_list` or
//! `list` whose layer is extended by the rows a scroll uncovers, rendering
//! only those.

use crate::fast::layers::policy::{Decision, last_decision};
use crate::{
    AnyWindowHandle, App, AppContext as _, Bounds, GlobalElementId, Hsla, ScaledPixels, Scene,
    ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, Window, point, px,
};

/// How tall the list is, in pixels.
const VIEWPORT_HEIGHT: f32 = 100.;
/// How wide the list is, in pixels.
const VIEWPORT_WIDTH: f32 = 200.;
/// How tall a row of a uniform list is, in pixels.
const ROW_HEIGHT: f32 = 20.;

fn with_window<R>(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    f: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(window, |_, window, cx| f(window, cx))
        .unwrap()
}

fn draw(cx: &mut TestAppContext, window: AnyWindowHandle) {
    with_window(cx, window, |window, cx| window.draw(cx).clear(cx));
}

/// Scrolls by `dy` with the wheel over the list, and draws the frame that
/// follows, unless the scroll drew it.
fn wheel(cx: &mut TestAppContext, window: AnyWindowHandle, dy: f32) {
    let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
    with_window(cx, window, |window, cx| {
        window.dispatch_event(
            crate::PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: point(px(20.), px(20.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
            }),
            cx,
        );
    });
    if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
        draw(cx, window);
    }
}

/// The id of the window's only layer.
fn layer_id(cx: &mut TestAppContext, window: AnyWindowHandle) -> GlobalElementId {
    with_window(cx, window, |window, _| {
        assert_eq!(window.fast_layers.layers.len(), 1, "one layer");
        window.fast_layers.layers.keys().next().unwrap().clone()
    })
}

/// What the window's only scroll container decided in the last frame.
fn decision(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<Decision> {
    let id = layer_id(cx, window);
    with_window(cx, window, |window, _| last_decision(window, &id))
}

/// The rows the window's only layer holds.
fn held_rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<usize> {
    with_window(cx, window, |window, _| {
        let layer = window.fast_layers.layers.values().next().expect("a layer");
        layer.rows.held().collect()
    })
}

/// A row's colour.
fn row_color(row: usize) -> Hsla {
    crate::hsla((row % 97) as f32 / 97., 0.5, 0.5, 1.)
}

/// The quads `scene` draws, with the layers it composites expanded: each
/// layer's content moved to where its tiles are composited and clipped to
/// their viewport, as drawing the content straight into the frame would
/// have put it. Each quad is its bounds, the part of them its content mask
/// lets it draw, and its colour, sorted.
fn expanded_quads(scene: &Scene) -> Vec<String> {
    let mut quads = Vec::new();
    let describe = |bounds: Bounds<ScaledPixels>,
                    mask: Bounds<ScaledPixels>,
                    background: &crate::Background| {
        let drawn = bounds.intersect(&mask);
        format!("{bounds:?} {drawn:?} {background:?}")
    };
    for quad in &scene.quads {
        quads.push(describe(
            quad.bounds,
            quad.content_mask.bounds,
            &quad.background,
        ));
    }
    for frame in &scene.layers.frames {
        let sprite = scene
            .polychrome_sprites
            .iter()
            .find_map(|sprite| {
                crate::decode_layer_tile(sprite.tile.texture_id, sprite.tile.tile_id)
                    .filter(|(key, _)| *key == frame.key)
                    .map(|(_, coord)| (coord, sprite))
            })
            .expect("a composited layer has tiles");
        let (coord, sprite) = sprite;
        let tile = frame.tile_bounds(coord);
        let translation = sprite.bounds.origin - tile.origin;
        let viewport = sprite.content_mask.bounds;
        let mut content = Scene::default();
        for operation in frame.content.operations() {
            if let crate::scene::PaintOperation::Primitive(primitive) = operation {
                content.insert_primitive(primitive.clone());
            }
        }
        for quad in &content.quads {
            let bounds = Bounds {
                origin: quad.bounds.origin + translation,
                size: quad.bounds.size,
            };
            let mask = Bounds {
                origin: quad.content_mask.bounds.origin + translation,
                size: quad.content_mask.bounds.size,
            }
            .intersect(&viewport);
            if bounds.intersect(&mask).is_empty() {
                continue;
            }
            quads.push(describe(bounds, mask, &quad.background));
        }
    }
    quads.sort();
    quads
}

/// Whether the window's last frame drew a layer's tiles, its rows painted
/// afresh or kept.
fn composites(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    with_window(cx, window, |window, _| {
        !window.rendered_frame.scene.layers.frames.is_empty()
    })
}

/// How many frames on this thread composited a list's layer keeping the
/// rows it held, rendering at most those a scroll uncovered.
fn extended_frames() -> usize {
    crate::fast::layers::lists::extended_frames()
}

/// A pseudo-random sequence of wheel deltas, whole pixels in -60..=60.
fn wheel_deltas(count: usize) -> Vec<f32> {
    let mut state: u32 = 0x2545_f491;
    (0..count)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((state >> 16) % 121) as f32 - 60.
        })
        .collect()
}

/// A pseudo-random sequence of wheel deltas from `seed`, fractional and
/// whole, some of them half a device pixel at a scale of 1.25.
fn fractional_deltas(seed: u32, count: usize) -> Vec<f32> {
    const DELTAS: [f32; 6] = [-0.37, 0.37, -12.5, 12.5, -1.0, -7.3];
    let mut state: u32 = 0x2545_f491 ^ seed.wrapping_mul(0x9e37_79b9);
    (0..count)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            DELTAS[((state >> 16) as usize) % DELTAS.len()]
        })
        .collect()
}

/// Scrolls `with_layers` and `without_layers`, which draws without layers,
/// by each of `deltas`, checking after each that both draw the same quads;
/// returns in how many frames `with_layers` composited its layer.
fn compare_with_layers_off(
    cx: &mut TestAppContext,
    with_layers: AnyWindowHandle,
    without_layers: AnyWindowHandle,
    deltas: &[f32],
    label: &str,
) -> usize {
    with_window(cx, without_layers, |window, _| {
        window.set_scroll_layers(false)
    });
    draw(cx, without_layers);
    let mut composited = 0;
    for (frame, dy) in deltas.iter().copied().enumerate() {
        wheel(cx, with_layers, dy);
        wheel(cx, without_layers, dy);
        if composites(cx, with_layers) {
            composited += 1;
        }
        let expected = with_window(cx, without_layers, |window, _| {
            assert!(window.rendered_frame.scene.layers.frames.is_empty());
            expanded_quads(&window.rendered_frame.scene)
        });
        let actual = with_window(cx, with_layers, |window, _| {
            expanded_quads(&window.rendered_frame.scene)
        });
        assert_eq!(actual, expected, "{label}: frame {frame}, scrolled by {dy}");
    }
    composited
}

/// Sets the scale factor of `window`, not yet drawn, to `scale_factor`, and
/// draws it twice.
fn open_at(cx: &mut TestAppContext, window: AnyWindowHandle, scale_factor: f32) {
    if scale_factor != 1. {
        cx.test_window(window)
            .simulate_scale_factor_change(scale_factor);
    }
    draw(cx, window);
    draw(cx, window);
}

mod uniform {
    use super::{
        Decision, ROW_HEIGHT, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, compare_with_layers_off, composites,
        decision, draw, expanded_quads, extended_frames, fractional_deltas, held_rows, open_at,
        row_color, wheel, wheel_deltas, with_window,
    };
    use crate::{
        AnyWindowHandle, Context, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, Window, WindowHandle, div, px, rgb,
    };
    use std::{cell::RefCell, ops::Range, rc::Rc};

    /// A white panel holding a uniform list of `count` rows of `row_height`
    /// px, 100 px tall, at the top left of the window. Every range of rows
    /// the list renders is logged.
    pub(super) struct UniformPage {
        pub(super) count: usize,
        pub(super) row_height: f32,
        tint: bool,
        background: crate::Hsla,
        pub(super) rendered: Rc<RefCell<Vec<Range<usize>>>>,
    }

    impl Render for UniformPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.rendered.clone();
            let row_height = self.row_height;
            let tint = self.tint;
            div().size_full().bg(self.background).child(
                crate::uniform_list(
                    "list",
                    self.count,
                    cx.processor(move |_, range: Range<usize>, _, _| {
                        rendered.borrow_mut().push(range.clone());
                        range
                            .map(|row| {
                                div().w(px(VIEWPORT_WIDTH)).h(px(row_height)).bg(if tint {
                                    rgb(0x224466).into()
                                } else {
                                    row_color(row)
                                })
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    pub(super) fn page(
        cx: &mut TestAppContext,
        count: usize,
    ) -> (WindowHandle<UniformPage>, Rc<RefCell<Vec<Range<usize>>>>) {
        page_with_rows_of(cx, count, ROW_HEIGHT, 1.)
    }

    fn page_with_rows_of(
        cx: &mut TestAppContext,
        count: usize,
        row_height: f32,
        scale_factor: f32,
    ) -> (WindowHandle<UniformPage>, Rc<RefCell<Vec<Range<usize>>>>) {
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let log = rendered.clone();
        let window = cx.add_window(move |_, _| UniformPage {
            count,
            row_height,
            tint: false,
            background: rgb(0xffffff).into(),
            rendered: log,
        });
        open_at(cx, window.into(), scale_factor);
        (window, rendered)
    }

    /// The rows rendered since the log was last taken, leaving out the
    /// measured item (row 0 alone).
    fn rendered_rows(log: &Rc<RefCell<Vec<Range<usize>>>>) -> Vec<Range<usize>> {
        std::mem::take(&mut *log.borrow_mut())
            .into_iter()
            .filter(|range| *range != (0..1))
            .collect()
    }

    /// Scrolls until the list has a layer: promoted on the second scrolled
    /// frame.
    fn promote(cx: &mut TestAppContext, window: AnyWindowHandle) {
        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
    }

    #[crate::test]
    fn uniform_list_with_expensive_quarter_rate_updates_falls_back(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(cx, 1000);
        let window = handle.into();
        promote(cx, window);
        for frame in 0..64 {
            if frame % 4 == 0 {
                handle
                    .update(cx, |page, _, cx| {
                        page.tint = !page.tint;
                        cx.notify();
                    })
                    .unwrap();
            }
            rendered_rows(&log);
            wheel(cx, window, -ROW_HEIGHT);
        }
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        assert!(
            held_rows(cx, window).is_empty(),
            "demotion releases cached rows"
        );
        assert!(
            rendered_rows(&log)
                .iter()
                .map(|range| range.len())
                .sum::<usize>()
                <= 6,
            "the fallback renders the viewport, without layer overscan"
        );
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            1
        );
        // Ongoing refreshes never re-promoted the layer. Once they stop,
        // the same container may cache again, with a fresh work budget.
        for _ in 0..64 {
            wheel(cx, window, -ROW_HEIGHT);
        }
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert!(!held_rows(cx, window).is_empty());
    }

    #[crate::test]
    fn uniform_list_with_sparse_broad_updates_stays_demoted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, _) = page(cx, 1000);
        let window = handle.into();
        promote(cx, window);
        for frame in 0..128 {
            if frame % 16 == 0 {
                handle
                    .update(cx, |page, _, cx| {
                        page.tint = !page.tint;
                        cx.notify();
                    })
                    .unwrap();
            }
            wheel(cx, window, -ROW_HEIGHT);
        }
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            1
        );
    }

    #[crate::test]
    fn an_unbakeable_list_background_stops_rebuilding_overscan(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(cx, 1000);
        let window = handle.into();
        handle
            .update(cx, |page, _, cx| {
                page.background = crate::rgba(0xffffff80).into();
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        promote(cx, window);
        rendered_rows(&log);
        for _ in 0..20 {
            wheel(cx, window, -ROW_HEIGHT);
            assert_eq!(decision(cx, window), Some(Decision::Bypass));
            let rendered: usize = rendered_rows(&log).iter().map(|range| range.len()).sum();
            assert!(
                rendered <= (VIEWPORT_HEIGHT / ROW_HEIGHT) as usize + 2,
                "only visible rows should render while the background cannot be baked: {rendered}"
            );
        }
        handle
            .update(cx, |page, _, cx| {
                page.background = rgb(0xffffff).into();
                cx.notify();
            })
            .unwrap();
        for _ in 0..70 {
            wheel(cx, window, -ROW_HEIGHT);
        }
        assert!(
            composites(cx, window),
            "a bakeable background can recover after the retry delay"
        );
    }

    #[crate::test]
    fn uniform_list_renders_only_new_rows_when_scrolling(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(cx, 1000);
        let window = handle.into();
        promote(cx, window);
        // Rows 2..7 show, and ten rows of overscan below them and above them,
        // as far as row 0. Row 1 showed in the frame before, and was left out:
        // painted into the layer, it could have kept element states that a
        // list without a layer drops.
        let mut expected: Vec<usize> = [0].into_iter().chain(2..17).collect();
        assert_eq!(held_rows(cx, window), expected);
        rendered_rows(&log);

        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert_eq!(
            rendered_rows(&log),
            vec![1..2, 17..18],
            "the row left out is rendered, and the row entering the overscan"
        );

        for step in 0..14 {
            wheel(cx, window, -ROW_HEIGHT);
            assert_eq!(decision(cx, window), Some(Decision::Composite));
            let new_row = 18 + step;
            assert_eq!(
                rendered_rows(&log),
                vec![new_row..new_row + 1],
                "step {step}: scrolling down one row renders the row entering the overscan"
            );
            // Rows that left the overscan are kept until they are more than
            // a quarter of the rows the layer is to hold, then dropped.
            let first = if step < 13 { 0 } else { new_row - 24 };
            expected = (first..new_row + 1).collect();
            assert_eq!(held_rows(cx, window), expected, "step {step}");
        }
        // Rows 17..22 show.
        assert_eq!(held_rows(cx, window), (7..32).collect::<Vec<_>>());

        wheel(cx, window, ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert_eq!(
            rendered_rows(&log),
            vec![6..7],
            "scrolling up one row renders the dropped row entering the overscan"
        );
        assert_eq!(held_rows(cx, window), (6..32).collect::<Vec<_>>());

        // Rows 16..21 show; a quarter of a row down, 16..22 do, all held.
        for _ in 0..2 {
            wheel(cx, window, -ROW_HEIGHT / 4.);
            assert_eq!(decision(cx, window), Some(Decision::Composite));
            assert_eq!(
                rendered_rows(&log),
                Vec::<Range<usize>>::new(),
                "a scroll that uncovers no row renders none"
            );
        }
    }

    /// A list whose content changes on every other frame while it scrolls, as
    /// a feed of 60 updates a second does at 120 Hz, would paint five
    /// viewports of rows on each of them: its layer is demoted by the fourth
    /// change within sixteen frames (the work guard may act earlier), and stays demoted while the feed goes on.
    #[crate::test]
    fn a_list_whose_content_keeps_changing_is_demoted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, _log) = page(cx, 1000);
        let window: AnyWindowHandle = handle.into();
        // Changes every row and draws the frame that follows, unless the
        // change drew it.
        let change = |cx: &mut TestAppContext| {
            let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
            handle
                .update(cx, |page, _, cx| {
                    // A new row height: every row the layer holds changes.
                    page.row_height = if page.row_height == ROW_HEIGHT {
                        ROW_HEIGHT + 1.
                    } else {
                        ROW_HEIGHT
                    };
                    cx.notify();
                })
                .unwrap();
            if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
                draw(cx, window);
            }
        };
        let demoted = |cx: &mut TestAppContext| {
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted)
        };
        promote(cx, window);
        for _ in 0..4 {
            change(cx);
            if decision(cx, window) == Some(Decision::Bypass) {
                break;
            }
            wheel(cx, window, -ROW_HEIGHT);
        }
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        assert_eq!(demoted(cx), 1);
        for step in 0..20 {
            if step % 2 == 0 {
                change(cx);
            } else {
                wheel(cx, window, -ROW_HEIGHT);
            }
            assert_eq!(decision(cx, window), Some(Decision::Bypass), "step {step}");
        }
    }

    #[crate::test]
    fn uniform_list_matches_layers_off(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_layers, _) = page(cx, 300);
        let (without_layers, _) = page(cx, 300);
        let with_layers: AnyWindowHandle = with_layers.into();
        let without_layers: AnyWindowHandle = without_layers.into();
        with_window(cx, without_layers, |window, _| {
            window.set_scroll_layers(false)
        });
        draw(cx, without_layers);

        let mut composited = 0;
        let extended = extended_frames();
        for (frame, dy) in wheel_deltas(50).into_iter().enumerate() {
            wheel(cx, with_layers, dy);
            wheel(cx, without_layers, dy);
            if composites(cx, with_layers) {
                composited += 1;
            }
            let expected = with_window(cx, without_layers, |window, _| {
                assert!(window.rendered_frame.scene.layers.frames.is_empty());
                expanded_quads(&window.rendered_frame.scene)
            });
            let actual = with_window(cx, with_layers, |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            assert_eq!(actual, expected, "frame {frame}, scrolled by {dy}");
        }
        assert!(composited > 40, "the layer was composited ({composited})");
        let extended = extended_frames() - extended;
        assert!(extended > 35, "frames kept the rows held ({extended})");
    }

    /// Scrolls a uniform list of rows `row_height` px tall at a scale of
    /// 1.25 by fractional deltas, checking it draws as it does without
    /// layers; returns in how many frames it composited its layer.
    fn uniform_list_matches_layers_off_at_a_fractional_scale(
        cx: &mut TestAppContext,
        row_height: f32,
    ) -> usize {
        let mut composited = 0;
        for seed in 0..6 {
            let (with_layers, _) = page_with_rows_of(cx, 300, row_height, 1.25);
            let (without_layers, _) = page_with_rows_of(cx, 300, row_height, 1.25);
            let with_layers: AnyWindowHandle = with_layers.into();
            let without_layers: AnyWindowHandle = without_layers.into();
            composited += compare_with_layers_off(
                cx,
                with_layers,
                without_layers,
                &fractional_deltas(seed, 120),
                &format!("rows of {row_height} px, seed {seed}"),
            );
        }
        composited
    }

    #[crate::test]
    fn uniform_list_matches_layers_off_at_a_fractional_scale_with_whole_rows(
        cx: &mut TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // 20 px rows are 25 device pixels.
        let extended = extended_frames();
        let composited = uniform_list_matches_layers_off_at_a_fractional_scale(cx, ROW_HEIGHT);
        assert!(composited > 300, "the layer was composited ({composited})");
        let extended = extended_frames() - extended;
        assert!(extended > 550, "frames kept the rows held ({extended})");
    }

    #[crate::test]
    fn uniform_list_matches_layers_off_at_a_fractional_scale_with_half_pixel_rows(
        cx: &mut TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // 30 px rows are 37.5 device pixels.
        let extended = extended_frames();
        let composited = uniform_list_matches_layers_off_at_a_fractional_scale(cx, 30.);
        let extended = extended_frames() - extended;
        assert!(composited > 550, "the layer was composited ({composited})");
        assert!(extended > 550, "frames kept the rows held ({extended})");
    }

    #[crate::test]
    fn measure_item_is_skipped_on_composite_frames(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(cx, 1000);
        let window = handle.into();
        let measured = |log: &Rc<RefCell<Vec<Range<usize>>>>| {
            std::mem::take(&mut *log.borrow_mut())
                .into_iter()
                .filter(|range| *range == (0..1))
                .count()
        };
        measured(&log);
        // Layout and prepaint each measure row 0.
        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(measured(&log), 2);
        promote_from_second_frame(cx, window);
        measured(&log);

        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert_eq!(measured(&log), 1, "only layout measures row 0");

        let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
        handle
            .update(cx, |page, _, cx| {
                page.count = 999;
                cx.notify();
            })
            .unwrap();
        if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
            draw(cx, window);
        }
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        assert_eq!(measured(&log), 2, "a frame that changed measures again");
    }

    /// The second scrolled frame, which promotes the list.
    fn promote_from_second_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
        wheel(cx, window, -ROW_HEIGHT);
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
    }

    #[crate::test]
    fn adding_a_row_keeps_the_rows_held_as_they_were(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(cx, 1000);
        let window = handle.into();
        promote(cx, window);
        wheel(cx, window, -ROW_HEIGHT);
        let parts = |cx: &mut TestAppContext| {
            with_window(cx, window, |window, _| {
                let layer = window.fast_layers.layers.values().next().unwrap();
                let record = layer.record.as_ref().unwrap();
                let parts: Vec<_> = record.content.part_scenes().cloned().collect();
                (parts, record.dirty_tiles.clone(), record.tile_hashes.len())
            })
        };
        for step in 0..10 {
            let (before, _, _) = parts(cx);
            rendered_rows(&log);
            wheel(cx, window, -ROW_HEIGHT);
            assert_eq!(decision(cx, window), Some(Decision::Composite));
            assert_eq!(rendered_rows(&log).len(), 1, "step {step}: one row added");
            let (after, dirty, tiles) = parts(cx);
            // One part per row: those of the rows kept are the ones the layer
            // held, not copies, and only the tiles the new row reaches are
            // hashed differently.
            assert_eq!(after.len(), before.len() + 1, "step {step}");
            let kept = after
                .iter()
                .filter(|part| before.iter().any(|old| std::rc::Rc::ptr_eq(old, part)))
                .count();
            assert_eq!(kept, before.len(), "step {step}");
            assert!(
                (1..=2).contains(&dirty.len()) && dirty.len() < tiles,
                "step {step}: {} of {tiles} tiles dirty",
                dirty.len()
            );
        }
    }
}

mod list {
    use super::{
        Decision, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, compare_with_layers_off, composites, decision,
        draw, expanded_quads, extended_frames, fractional_deltas, held_rows, open_at, row_color,
        wheel, wheel_deltas, with_window,
    };
    use crate::{
        AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ListAlignment, ListState,
        ParentElement as _, Render, Styled as _, TestAppContext, Window, WindowHandle, div,
        prelude::FluentBuilder as _, px, rgb,
    };
    use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

    /// How tall row `row` is: 20, 30 or 40 px.
    fn row_height(row: usize) -> f32 {
        20. + (row % 3) as f32 * 10.
    }

    /// A white panel holding a list of rows of varying heights, 100 px tall,
    /// at the top left of the window. Every row the list renders is logged.
    pub(super) struct ListPage {
        pub(super) state: ListState,
        pub(super) rendered: Rc<RefCell<Vec<usize>>>,
    }

    impl Render for ListPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.rendered.clone();
            let tint = cx.try_global::<RowTint>().map_or(0, |tint| tint.0);
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.state.clone(), move |row, _, _| {
                    rendered.borrow_mut().push(row);
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(row_height(row)))
                        .bg(row_color(row + tint))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    fn page(
        cx: &mut TestAppContext,
        state: ListState,
    ) -> (WindowHandle<ListPage>, Rc<RefCell<Vec<usize>>>) {
        page_at(cx, state, 1.)
    }

    fn page_at(
        cx: &mut TestAppContext,
        state: ListState,
        scale_factor: f32,
    ) -> (WindowHandle<ListPage>, Rc<RefCell<Vec<usize>>>) {
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let log = rendered.clone();
        let window = cx.add_window(move |_, _| ListPage {
            state,
            rendered: log,
        });
        open_at(cx, window.into(), scale_factor);
        (window, rendered)
    }

    fn promote(cx: &mut TestAppContext, window: AnyWindowHandle) {
        wheel(cx, window, -20.);
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        wheel(cx, window, -20.);
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
    }

    /// The rows rendered since the log was last taken.
    fn rendered(log: &Rc<RefCell<Vec<usize>>>) -> BTreeSet<usize> {
        log.borrow_mut().drain(..).collect()
    }

    #[crate::test]
    fn list_renders_only_new_rows_when_scrolling(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(1000, ListAlignment::Top, px(0.)).measure_all();
        let (handle, log) = page(cx, state);
        let window = handle.into();
        promote(cx, window);
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        assert!(held.contains(&0) && held.len() > 5, "held {held:?}");
        rendered(&log);

        let mut rendered_any = false;
        for step in 0..30 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
            let rendered = rendered(&log);
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let added: BTreeSet<usize> = now.difference(&held).copied().collect();
            assert_eq!(
                rendered, added,
                "step {step}: only the rows new to the layer are rendered"
            );
            assert!(rendered.len() <= 2, "step {step}: {rendered:?}");
            rendered_any |= !rendered.is_empty();
            held = now;
        }
        assert!(rendered_any);
        assert!(
            held.first().copied().unwrap_or(0) > 0,
            "rows left behind are dropped"
        );

        for step in 0..10 {
            wheel(cx, window, 25.);
            assert_eq!(decision(cx, window), Some(Decision::Composite), "up {step}");
            let rendered = rendered(&log);
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let added: BTreeSet<usize> = now.difference(&held).copied().collect();
            assert_eq!(rendered, added, "up {step}");
            held = now;
        }
    }

    #[crate::test]
    fn list_matches_layers_off_with_varying_heights(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_layers, _) = page(cx, ListState::new(300, ListAlignment::Top, px(50.)));
        let (without_layers, _) = page(cx, ListState::new(300, ListAlignment::Top, px(50.)));
        let with_layers: AnyWindowHandle = with_layers.into();
        let without_layers: AnyWindowHandle = without_layers.into();
        with_window(cx, without_layers, |window, _| {
            window.set_scroll_layers(false)
        });
        draw(cx, without_layers);

        let mut composited = 0;
        let extended = extended_frames();
        let mut deltas = wheel_deltas(50);
        // And back up past where it started, through rows measured on the way.
        deltas.extend([60., 60., 60., 45., 60., 60., 60., 60., 33., 60.]);
        for (frame, dy) in deltas.into_iter().enumerate() {
            wheel(cx, with_layers, dy);
            wheel(cx, without_layers, dy);
            if composites(cx, with_layers) {
                composited += 1;
            }
            let expected = with_window(cx, without_layers, |window, _| {
                assert!(window.rendered_frame.scene.layers.frames.is_empty());
                expanded_quads(&window.rendered_frame.scene)
            });
            let actual = with_window(cx, with_layers, |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            assert_eq!(actual, expected, "frame {frame}, scrolled by {dy}");
        }
        assert!(composited > 40, "the layer was composited ({composited})");
        let extended = extended_frames() - extended;
        assert!(extended > 40, "frames kept the rows held ({extended})");
    }

    /// Scrolls a list aligned by `alignment` with rows of 20, 30 and 40 px
    /// at a scale of 1.25, where a 30 px row is 37.5 device pixels, by
    /// fractional deltas, and checks it draws as it does without layers.
    fn list_matches_layers_off_at_a_fractional_scale(
        cx: &mut TestAppContext,
        alignment: ListAlignment,
    ) -> usize {
        let mut composited = 0;
        let mut cached_rows = 0;
        let mut direct_rows = 0;
        for seed in 0..8 {
            let (with_layers, cached_log) =
                page_at(cx, ListState::new(300, alignment, px(50.)), 1.25);
            let (without_layers, direct_log) =
                page_at(cx, ListState::new(300, alignment, px(50.)), 1.25);
            let with_layers: AnyWindowHandle = with_layers.into();
            let without_layers: AnyWindowHandle = without_layers.into();
            cached_log.borrow_mut().clear();
            direct_log.borrow_mut().clear();
            composited += compare_with_layers_off(
                cx,
                with_layers,
                without_layers,
                &fractional_deltas(seed, 120),
                &format!("{alignment:?}, seed {seed}"),
            );
            cached_rows += cached_log.borrow().len();
            direct_rows += direct_log.borrow().len();
        }
        assert!(
            cached_rows < direct_rows,
            "{alignment:?}: cached path rendered {cached_rows} rows, direct path {direct_rows}"
        );
        composited
    }

    #[crate::test]
    fn list_matches_layers_off_at_a_fractional_scale_aligned_top(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let extended = extended_frames();
        let composited = list_matches_layers_off_at_a_fractional_scale(cx, ListAlignment::Top);
        assert!(composited > 480, "the layer was composited ({composited})");
        let extended = extended_frames() - extended;
        // Expensive extensions may fall back; the differential helper also
        // checks that the cached path renders fewer rows overall.
        assert!(extended > 480, "frames kept the rows held ({extended})");
    }

    #[crate::test]
    fn list_matches_layers_off_at_a_fractional_scale_aligned_bottom(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let extended = extended_frames();
        let composited = list_matches_layers_off_at_a_fractional_scale(cx, ListAlignment::Bottom);
        assert!(composited > 480, "the layer was composited ({composited})");
        let extended = extended_frames() - extended;
        // Expensive extensions may fall back; the differential helper also
        // checks that the cached path renders fewer rows overall.
        assert!(extended > 480, "frames kept the rows held ({extended})");
    }

    #[crate::test]
    fn a_list_splice_repaints_the_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(1000, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state);
        let window = handle.into();
        promote(cx, window);
        wheel(cx, window, -20.);
        assert_eq!(decision(cx, window), Some(Decision::Composite));

        let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
        handle
            .update(cx, |page, _, cx| {
                page.state.splice(3..4, 2);
                cx.notify();
            })
            .unwrap();
        if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
            draw(cx, window);
        }
        assert_eq!(decision(cx, window), Some(Decision::Repaint));

        wheel(cx, window, -20.);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
    }

    /// Notifies the view of `handle`, as something it reads changing does,
    /// and draws the frame that follows.
    /// How far the rows of every [`ListPage`] are tinted, as the page
    /// reads it when it renders and hands its row renderer.
    struct RowTint(usize);

    impl crate::Global for RowTint {}

    /// Tints the rows of every [`ListPage`] anew.
    fn tint_rows(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let tint = cx.try_global::<RowTint>().map_or(0, |tint| tint.0);
            cx.set_global(RowTint(tint + 1));
        });
    }

    /// Tints the rows of every [`ListPage`] anew, and notifies `handle`'s
    /// page, drawing it.
    fn notify_rows(cx: &mut TestAppContext, handle: WindowHandle<ListPage>) {
        tint_rows(cx);
        notify(cx, handle);
    }

    fn notify(cx: &mut TestAppContext, handle: WindowHandle<ListPage>) {
        let window: AnyWindowHandle = handle.into();
        let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
        handle.update(cx, |_, _, cx| cx.notify()).unwrap();
        if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
            draw(cx, window);
        }
    }

    /// A view rendering its list again, its row renderer changed — tinting
    /// every row anew here — keeps the list on its layer: the list renders
    /// the rows it shows again, and the rows its layer holds besides are
    /// rendered again as they come to show. It draws as it does without
    /// layers.
    #[crate::test]
    fn a_view_rendering_again_renders_its_rows_as_they_show(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (handle, log) = page(
            cx,
            ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
        );
        let (without, _) = page(
            cx,
            ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
        );
        let window: AnyWindowHandle = handle.into();
        compare_with_layers_off(cx, window, without.into(), &[-20., -20., -15.], "promote");
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        let held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        rendered(&log);

        notify_rows(cx, handle);
        notify(cx, without);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        let rendered_now = rendered(&log);
        // 100 px of rows 20 to 40 px tall, and the one cut by each edge.
        assert!(
            !rendered_now.is_empty() && rendered_now.len() <= 6,
            "only the rows shown render: {rendered_now:?}"
        );
        assert_eq!(
            held_rows(cx, window).into_iter().collect::<BTreeSet<_>>(),
            held,
            "the layer keeps the rows it held"
        );
        let composited =
            compare_with_layers_off(cx, window, without.into(), &[-15.; 10], "scroll down");
        assert_eq!(composited, 10);
        let rendered_after = rendered(&log);
        assert!(
            rendered_after.iter().any(|row| held.contains(row)),
            "the rows held come to show and render again: {rendered_after:?}"
        );
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            0
        );
    }

    #[crate::test]
    fn a_list_whose_view_is_notified_now_and_then_keeps_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(1000, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state);
        let window = handle.into();
        promote(cx, window);
        for frame in 0..200 {
            if frame % 20 == 0 {
                notify(cx, handle);
            }
            wheel(cx, window, -10.);
        }
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            0
        );
    }

    /// A view notified on every frame for a while as it scrolls its list —
    /// fading in a button beside it, say — paints the rows it shows afresh
    /// each of those frames, and they come out as the layer held them: the
    /// list keeps its layer, as it does for a view notified now and then.
    #[crate::test]
    fn a_list_whose_view_animates_beside_it_keeps_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(1000, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state);
        let window = handle.into();
        promote(cx, window);
        for frame in 0..60 {
            if frame % 30 < 12 {
                notify(cx, handle);
                eprintln!(
                    "DBGT {frame} notify {:?} hist={:b} dem={}",
                    decision(cx, window),
                    with_window(cx, window, |w, _| w
                        .fast_layers
                        .layers
                        .values()
                        .next()
                        .map_or(0, |l| l.policy.change_history)),
                    with_window(cx, window, |w, _| w.layout_stats().layers_demoted)
                );
            }
            wheel(cx, window, -10.);
            eprintln!("DBGT {frame} wheel {:?}", decision(cx, window));
        }
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            0
        );
    }

    /// A list kept off its layer for changing too often, whose view is then
    /// notified every couple of seconds, gets its layer back between the
    /// notifications: a change while it is kept off asks only for the rows
    /// to stay as they are a second before it is tried again.
    #[crate::test]
    fn a_demoted_list_notified_every_few_seconds_gets_its_layer_back(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(1000, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state);
        let window = handle.into();
        promote(cx, window);
        // Its rows changed on every frame: demoted, and kept off its layer
        // while they keep changing, twice, its cooldown growing.
        let scroll_changing_rows = |cx: &mut TestAppContext, frame: usize| {
            tint_rows(cx);
            wheel(cx, window, if frame % 40 < 20 { -10. } else { 10. });
        };
        let demoted = |cx: &mut TestAppContext| {
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted)
        };
        let mut frames = 0;
        for times in 1..=2 {
            while demoted(cx) < times {
                scroll_changing_rows(cx, frames);
                frames += 1;
                assert!(frames < 2_000, "demoted {times} times");
            }
            for frame in 0..200 {
                scroll_changing_rows(cx, frame);
                assert_eq!(
                    decision(cx, window),
                    Some(Decision::Bypass),
                    "frame {frame}"
                );
            }
            if times < 2 {
                // Left alone, it gets its layer back.
                while decision(cx, window) == Some(Decision::Bypass) {
                    wheel(cx, window, if frames % 40 < 20 { -10. } else { 10. });
                    frames += 1;
                    assert!(frames < 2_000, "promoted again");
                }
            }
        }
        let mut composited = 0;
        for frame in 0..1_200 {
            if frame % 100 == 0 {
                notify(cx, handle);
            }
            wheel(cx, window, if frame % 200 < 100 { -5. } else { 5. });
            composited += usize::from(decision(cx, window) == Some(Decision::Composite));
        }
        assert!(composited > 600, "composited {composited}");
    }

    #[crate::test]
    fn whether_a_list_is_at_its_end_is_told_as_the_list_tells_it(cx: &mut TestAppContext) {
        let state = ListState::new(30, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state.clone());
        let window: AnyWindowHandle = handle.into();
        let at_end = || crate::fast::layers::lists::scrolled_to_end(&state.0.borrow());
        assert_eq!(at_end(), state.is_scrolled_to_end());
        let mut seen = BTreeSet::new();
        for _ in 0..40 {
            wheel(cx, window, -30.);
            assert_eq!(at_end(), state.is_scrolled_to_end());
            seen.insert(at_end());
        }
        assert_eq!(seen, [Some(false), Some(true)].into());
    }

    #[crate::test]
    fn scrolling_a_list_to_its_end_where_it_is_changes_nothing(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = ListState::new(30, ListAlignment::Top, px(0.)).measure_all();
        let (handle, _) = page(cx, state.clone());
        let window: AnyWindowHandle = handle.into();
        state.scroll_to_end();
        draw(cx, window);
        let version = state.0.borrow().version.get();
        state.scroll_to_end();
        assert_eq!(state.0.borrow().version.get(), version);
        state.scroll_to(crate::ListOffset {
            item_ix: 3,
            offset_in_item: px(0.),
        });
        let version = state.0.borrow().version.get();
        state.scroll_to_end();
        assert_ne!(state.0.borrow().version.get(), version, "it moved");
    }

    /// A [`ListPage`] whose render writes what it was given into a model it
    /// holds every time, as an input component writes its options into its
    /// state.
    struct WritingPage {
        list: ListPage,
        options: Entity<bool>,
    }

    impl Render for WritingPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.options.update(cx, |readonly, _| *readonly = true);
            let rendered = self.list.rendered.clone();
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.list.state.clone(), move |row, _, _| {
                    rendered.borrow_mut().push(row);
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(row_height(row)))
                        .bg(row_color(row))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    #[crate::test]
    fn a_view_writing_as_it_renders_keeps_its_list_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = cx.add_window(|_, cx| WritingPage {
            list: ListPage {
                state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                rendered: Rc::new(RefCell::new(Vec::new())),
            },
            options: cx.new(|_| false),
        });
        let window: AnyWindowHandle = window.into();
        open_at(cx, window, 1.);
        promote(cx, window);
        for step in 0..20 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
        }
    }

    /// A white page holding a [`WritingPage`] as a cached view, as a panel
    /// holds the view showing a list.
    struct CachedWritingPage {
        child: Entity<WritingPage>,
    }

    impl Render for CachedWritingPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                crate::AnyView::from(self.child.clone())
                    .cached(crate::StyleRefinement::default().size_full()),
            )
        }
    }

    #[crate::test]
    fn a_cached_view_writing_as_it_renders_keeps_its_list_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = cx.add_window(|_, cx| CachedWritingPage {
            child: cx.new(|cx| WritingPage {
                list: ListPage {
                    state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                    rendered: Rc::new(RefCell::new(Vec::new())),
                },
                options: cx.new(|_| false),
            }),
        });
        let window: AnyWindowHandle = window.into();
        open_at(cx, window, 1.);
        promote(cx, window);
        for step in 0..20 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
        }
    }

    /// A [`ListPage`] drawn after a backdrop of its parent's, as a page is
    /// drawn after a sidebar.
    struct BackdropPage {
        list: Entity<ListPage>,
    }

    impl Render for BackdropPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().bg(rgb(0xeeeeee)).child(self.list.clone())
        }
    }

    /// A list view drawn from last frame while its parent renders again, after
    /// the parent has drawn something of its own, keeps its layer's ranges,
    /// which count from the layer's own scene and not the window's, and draws
    /// as it does without layers.
    #[crate::test]
    fn a_list_view_drawn_again_after_its_parents_backdrop_keeps_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let open = |cx: &mut TestAppContext| {
            let window = cx.add_window(|_, cx| BackdropPage {
                list: cx.new(|_| ListPage {
                    state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                    rendered: Rc::new(RefCell::new(Vec::new())),
                }),
            });
            open_at(cx, window.into(), 1.);
            window
        };
        let with_layers = open(cx);
        let without_layers = open(cx);
        with_window(cx, without_layers.into(), |window, _| {
            window.set_scroll_layers(false)
        });
        draw(cx, without_layers.into());
        promote(cx, with_layers.into());
        wheel(cx, without_layers.into(), -20.);
        wheel(cx, without_layers.into(), -20.);
        for frame in 0..40 {
            if frame % 4 == 0 {
                for window in [with_layers, without_layers] {
                    window.update(cx, |_, _, cx| cx.notify()).unwrap();
                }
            }
            wheel(cx, with_layers.into(), -10.);
            wheel(cx, without_layers.into(), -10.);
            let expected = with_window(cx, without_layers.into(), |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            let actual = with_window(cx, with_layers.into(), |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            assert_eq!(actual, expected, "frame {frame}");
        }
        assert_eq!(decision(cx, with_layers.into()), Some(Decision::Composite));
    }

    /// A caret drawn by a component, which reads whether it shows as the
    /// component renders, when the element it is laid out, as an input reads
    /// its blinking cursor.
    #[derive(IntoElement)]
    struct Caret {
        visible: Entity<bool>,
    }

    impl crate::RenderOnce for Caret {
        fn render(self, _: &mut Window, cx: &mut crate::App) -> impl IntoElement {
            let visible = *self.visible.read(cx);
            div()
                .w(px(2.))
                .h(px(10.))
                .when(visible, |caret| caret.bg(crate::red()))
        }
    }

    /// A [`ListPage`] with a [`Caret`] beside its list.
    struct CaretPage {
        list: ListPage,
        caret: Entity<bool>,
    }

    impl Render for CaretPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.list.rendered.clone();
            div()
                .flex()
                .size_full()
                .bg(rgb(0xffffff))
                .child(
                    crate::list(self.list.state.clone(), move |row, _, _| {
                        rendered.borrow_mut().push(row);
                        div()
                            .w(px(VIEWPORT_WIDTH))
                            .h(px(row_height(row)))
                            .bg(row_color(row))
                            .into_any_element()
                    })
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(VIEWPORT_HEIGHT)),
                )
                .child(Caret {
                    visible: self.caret.clone(),
                })
        }
    }

    #[crate::test]
    fn a_blinking_caret_beside_a_list_leaves_it_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let caret = cx.new(|_| true);
        let log = Rc::new(RefCell::new(Vec::new()));
        let window = cx.add_window({
            let caret = caret.clone();
            let log = log.clone();
            move |_, _| CaretPage {
                list: ListPage {
                    state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                    rendered: log,
                },
                caret,
            }
        });
        let window: AnyWindowHandle = window.into();
        open_at(cx, window, 1.);
        promote(cx, window);
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        rendered(&log);
        for step in 0..40 {
            if step % 3 == 0 {
                caret.update(cx, |visible, cx| {
                    *visible = !*visible;
                    cx.notify();
                });
            }
            wheel(cx, window, -15.);
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let added: BTreeSet<usize> = now.difference(&held).copied().collect();
            assert_eq!(
                rendered(&log),
                added,
                "step {step}: only rows new to the layer render"
            );
            held = now;
        }
    }

    /// A [`ListPage`] that, after building its list, reads where the list is
    /// scrolled to, to mark beside it the first row it shows, as an outline
    /// beside a transcript marks the turns in view.
    struct OutlinePage {
        list: ListPage,
    }

    impl Render for OutlinePage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.list.rendered.clone();
            let list = crate::list(self.list.state.clone(), move |row, _, _| {
                rendered.borrow_mut().push(row);
                div()
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(row_height(row)))
                    .bg(row_color(row))
                    .into_any_element()
            })
            .w(px(VIEWPORT_WIDTH))
            .h(px(VIEWPORT_HEIGHT));
            let first_shown = self.list.state.logical_scroll_top().item_ix;
            div()
                .flex()
                .size_full()
                .bg(rgb(0xffffff))
                .child(list)
                .child(
                    div()
                        .ml(px(10.))
                        .w(px(10.))
                        .h(px(10.))
                        .bg(row_color(first_shown)),
                )
        }
    }

    fn outline_page(cx: &mut TestAppContext) -> (AnyWindowHandle, Rc<RefCell<Vec<usize>>>) {
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let log = rendered.clone();
        let window = cx.add_window(move |_, _| OutlinePage {
            list: ListPage {
                state: ListState::new(300, ListAlignment::Top, px(0.)).measure_all(),
                rendered: log,
            },
        });
        open_at(cx, window.into(), 1.);
        (window.into(), rendered)
    }

    #[crate::test]
    fn a_view_reading_its_list_offset_after_building_it_keeps_it_on_its_layer(
        cx: &mut TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (window, log) = outline_page(cx);
        promote(cx, window);
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        rendered(&log);
        for step in 0..30 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let added: BTreeSet<usize> = now.difference(&held).copied().collect();
            assert_eq!(rendered(&log), added, "step {step}: only new rows render");
            held = now;
        }
    }

    #[crate::test]
    fn a_view_reading_its_list_offset_after_building_it_matches_layers_off(
        cx: &mut TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_layers, _) = outline_page(cx);
        let (without_layers, _) = outline_page(cx);
        let composited = compare_with_layers_off(
            cx,
            with_layers,
            without_layers,
            &wheel_deltas(60),
            "outline",
        );
        assert!(composited > 40, "the layer was composited ({composited})");
    }

    /// A [`ListPage`] whose render reads a blinking caret itself, before or
    /// after building its list, as a chat view renders its composer.
    struct ReadingPage {
        list: ListPage,
        caret: Entity<bool>,
        read_before_list: bool,
    }

    impl Render for ReadingPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let before = self.read_before_list.then(|| *self.caret.read(cx));
            let rendered = self.list.rendered.clone();
            let list = crate::list(self.list.state.clone(), move |row, _, _| {
                rendered.borrow_mut().push(row);
                div()
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(row_height(row)))
                    .bg(row_color(row))
                    .into_any_element()
            })
            .w(px(VIEWPORT_WIDTH))
            .h(px(VIEWPORT_HEIGHT));
            let visible = before.unwrap_or_else(|| *self.caret.read(cx));
            div()
                .flex()
                .size_full()
                .bg(rgb(0xffffff))
                .child(list)
                .child(
                    div()
                        .w(px(2.))
                        .h(px(10.))
                        .when(visible, |caret| caret.bg(crate::red())),
                )
        }
    }

    /// Scrolls a [`ReadingPage`], blinking its caret every third step, and
    /// returns whether any step rendered a row its layer held.
    fn blink_while_scrolling(cx: &mut TestAppContext, read_before_list: bool) -> bool {
        let caret = cx.new(|_| true);
        let log = Rc::new(RefCell::new(Vec::new()));
        let window = cx.add_window({
            let caret = caret.clone();
            let log = log.clone();
            move |_, _| ReadingPage {
                list: ListPage {
                    state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                    rendered: log,
                },
                caret,
                read_before_list,
            }
        });
        let window: AnyWindowHandle = window.into();
        open_at(cx, window, 1.);
        promote(cx, window);
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        rendered(&log);
        let mut rendered_held = false;
        for step in 0..40 {
            if step % 3 == 0 {
                caret.update(cx, |visible, cx| {
                    *visible = !*visible;
                    cx.notify();
                });
            }
            wheel(cx, window, -15.);
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            rendered_held |= rendered(&log).iter().any(|row| held.contains(row));
            held = now;
        }
        rendered_held
    }

    #[crate::test]
    fn what_a_view_reads_after_building_its_list_leaves_it_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        assert!(!blink_while_scrolling(cx, false), "the rows held stay");
    }

    #[crate::test]
    fn what_a_view_reads_before_building_its_list_paints_the_rows_again(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        assert!(
            blink_while_scrolling(cx, true),
            "the row renderer may hold what the view read"
        );
    }

    /// A list whose rows read a model as they render, as rows whose text
    /// finishes loading after they first show.
    struct LoadingPage {
        state: ListState,
        loaded: Entity<u32>,
    }

    impl Render for LoadingPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let loaded = self.loaded.clone();
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.state.clone(), move |row, _, cx| {
                    let loaded = *loaded.read(cx) as usize;
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(row_height(row)))
                        .bg(row_color(row + loaded))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    #[crate::test]
    fn a_list_whose_rows_change_in_a_burst_now_and_then_keeps_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let loaded = cx.new(|_| 0u32);
        let window = cx.add_window({
            let loaded = loaded.clone();
            move |_, _| LoadingPage {
                state: ListState::new(1000, ListAlignment::Top, px(0.)).measure_all(),
                loaded,
            }
        });
        let window: AnyWindowHandle = window.into();
        open_at(cx, window, 1.);
        promote(cx, window);
        for burst in 0..3 {
            // Several rows finish loading on frames close together.
            for _ in 0..5 {
                let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
                loaded.update(cx, |loaded, cx| {
                    *loaded += 1;
                    cx.notify();
                });
                if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
                    draw(cx, window);
                }
            }
            for step in 0..30 {
                wheel(cx, window, -10.);
                if step > 10 {
                    assert_eq!(
                        decision(cx, window),
                        Some(Decision::Composite),
                        "burst {burst}, step {step}"
                    );
                }
            }
        }
        assert_eq!(
            with_window(cx, window, |window, _| window.layout_stats().layers_demoted),
            0
        );
    }

    /// What a view holding a list reads of where the list is scrolled to as
    /// it renders.
    #[derive(Clone, Copy)]
    enum ScrollRead {
        /// Only whether it is scrolled to its end, as a chat transcript does
        /// to show a "back to bottom" button.
        AtEnd,
        /// Its offset.
        Offset,
    }

    /// A [`ListPage`] that reads where its list is scrolled to as it
    /// renders, and draws a button beside the list while it is not at its
    /// end, or, reading its offset, while it is not at its top.
    struct ScrollAwarePage {
        list: ListPage,
        read: ScrollRead,
    }

    impl Render for ScrollAwarePage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.list.rendered.clone();
            let show_button = match self.read {
                ScrollRead::AtEnd => self.list.state.is_scrolled_to_end() == Some(false),
                ScrollRead::Offset => self.list.state.logical_scroll_top().item_ix > 0,
            };
            div()
                .flex()
                .size_full()
                .bg(rgb(0xffffff))
                .child(
                    crate::list(self.list.state.clone(), move |row, _, _| {
                        rendered.borrow_mut().push(row);
                        div()
                            .w(px(VIEWPORT_WIDTH))
                            .h(px(row_height(row)))
                            .bg(row_color(row))
                            .into_any_element()
                    })
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(VIEWPORT_HEIGHT)),
                )
                .when(show_button, |page| {
                    page.child(div().ml(px(10.)).size(px(20.)).bg(crate::red()))
                })
        }
    }

    fn scroll_aware_page(
        cx: &mut TestAppContext,
        rows: usize,
        read: ScrollRead,
    ) -> (AnyWindowHandle, Rc<RefCell<Vec<usize>>>) {
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let log = rendered.clone();
        let window = cx.add_window(move |_, _| ScrollAwarePage {
            list: ListPage {
                state: ListState::new(rows, ListAlignment::Top, px(0.)).measure_all(),
                rendered: log,
            },
            read,
        });
        open_at(cx, window.into(), 1.);
        (window.into(), rendered)
    }

    #[crate::test]
    fn a_view_asking_whether_its_list_is_at_its_end_keeps_it_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (window, log) = scroll_aware_page(cx, 1000, ScrollRead::AtEnd);
        promote(cx, window);
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        rendered(&log);
        for step in 0..30 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let added: BTreeSet<usize> = now.difference(&held).copied().collect();
            assert_eq!(rendered(&log), added, "step {step}: only new rows render");
            held = now;
        }
    }

    #[crate::test]
    fn a_view_reading_its_list_offset_keeps_it_off_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (window, _) = scroll_aware_page(cx, 1000, ScrollRead::Offset);
        for step in 0..30 {
            wheel(cx, window, -15.);
            assert_ne!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}: what the view shows moves with the offset"
            );
        }
    }

    #[crate::test]
    fn a_view_asking_whether_its_list_is_at_its_end_matches_layers_off(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_layers, _) = scroll_aware_page(cx, 30, ScrollRead::AtEnd);
        let (without_layers, _) = scroll_aware_page(cx, 30, ScrollRead::AtEnd);
        with_window(cx, without_layers, |window, _| {
            window.set_scroll_layers(false)
        });
        // Down to the end, where the button goes: the frame that gets there
        // paints the layer again, the others composite it.
        let composited =
            compare_with_layers_off(cx, with_layers, without_layers, &[-30.; 40], "down");
        assert!(composited > 35, "the layer was composited ({composited})");
        // And back up, where it comes back.
        compare_with_layers_off(cx, with_layers, without_layers, &[30.; 40], "up");
    }

    /// A row in a view of its own, of `color`, that asks for an animation
    /// frame each time it renders when it `animate`s, and holds an anchored
    /// element when it is `anchored`.
    struct Ticker {
        animate: bool,
        anchored: bool,
        color: usize,
    }

    impl Render for Ticker {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            if self.animate {
                window.request_animation_frame();
            }
            div()
                .w(px(VIEWPORT_WIDTH))
                .h(px(20.))
                .bg(row_color(self.color))
                .when(self.anchored, |row| {
                    row.child(crate::anchored().child(div().w(px(50.)).h(px(50.)).bg(crate::red())))
                })
        }
    }

    /// A list of 20 px rows whose row 2 is a [`Ticker`].
    struct TickerListPage {
        state: ListState,
        ticker: Entity<Ticker>,
    }

    impl Render for TickerListPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let ticker = self.ticker.clone();
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.state.clone(), move |row, _, _| {
                    if row == 2 {
                        return ticker.clone().into_any_element();
                    }
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(20.))
                        .bg(row_color(row))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    /// A [`TickerListPage`] scrolled until its list has a layer, the ticker
    /// showing at the top of the list.
    fn ticker_page(
        cx: &mut TestAppContext,
        animate: bool,
        anchored: bool,
    ) -> (AnyWindowHandle, Entity<Ticker>) {
        let handle = cx.add_window(move |_, cx| TickerListPage {
            state: ListState::new(300, ListAlignment::Top, px(0.)),
            ticker: cx.new(|_| Ticker {
                animate,
                anchored,
                color: 2,
            }),
        });
        let window: AnyWindowHandle = handle.into();
        draw(cx, window);
        draw(cx, window);
        let ticker = handle.update(cx, |page, _, _| page.ticker.clone()).unwrap();
        promote(cx, window);
        (window, ticker)
    }

    #[crate::test]
    fn an_animating_row_view_keeps_the_list_off_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (window, ticker) = ticker_page(cx, true, false);
        // The ticker asks for an animation frame each time it renders, and
        // is notified each frame, as the animation frame it asked for would
        // have it: what it draws changes every frame, and the list is drawn
        // on today's path, as a div whose content view animates is.
        for step in 0..4 {
            ticker.update(cx, |_, cx| cx.notify());
            wheel(cx, window, -2.);
            assert_eq!(decision(cx, window), Some(Decision::Bypass), "step {step}");
        }
    }

    #[crate::test]
    fn a_row_view_held_by_the_layer_stays_tracked(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (window, ticker) = ticker_page(cx, false, false);
        let tracked = |cx: &mut TestAppContext| {
            with_window(cx, window, |window, cx| {
                let id = window.window_handle().window_id();
                cx.tracked_entities
                    .get(&id)
                    .is_some_and(|tracked| tracked.contains(&ticker.entity_id()))
            })
        };
        for step in 0..3 {
            wheel(cx, window, -2.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
            // On today's path the list renders the ticker, which it shows,
            // every frame: the window is told when it is notified.
            assert!(tracked(cx), "step {step}: the window tracks the ticker");
        }

        ticker.update(cx, |ticker, cx| {
            ticker.color = 50;
            cx.notify();
        });
        wheel(cx, window, -2.);
        let color = format!("{:?}", crate::Background::from(row_color(50)));
        with_window(cx, window, |window, _| {
            let quads = expanded_quads(&window.rendered_frame.scene);
            assert!(
                quads.iter().any(|quad| quad.ends_with(&color)),
                "the ticker shows its new colour"
            );
        });
    }

    #[crate::test]
    fn an_anchored_element_in_a_row_keeps_the_list_off_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // Positioned against the window's edges when prepainted, which a
        // composited layer would move with the rows.
        let (window, _) = ticker_page(cx, false, true);
        for step in 0..3 {
            wheel(cx, window, -2.);
            assert_eq!(decision(cx, window), Some(Decision::Bypass), "step {step}");
        }
    }

    /// A row of [`CellsPage`]: a view of its own, tinted by its model, that
    /// logs that it rendered.
    struct CellRow {
        row: usize,
        tint: Entity<u32>,
        /// Shading the row further, as the view itself holds it.
        shade: usize,
        rendered: Rc<RefCell<Vec<usize>>>,
    }

    impl Render for CellRow {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.rendered.borrow_mut().push(self.row);
            let tint = *self.tint.read(cx);
            div()
                .w(px(VIEWPORT_WIDTH))
                .h(px(row_height(self.row)))
                .bg(row_color(self.row + tint as usize * 7 + self.shade * 11))
        }
    }

    /// What changes in one row of a [`CellsPage`].
    #[derive(Clone, Copy, Debug)]
    enum RowChange {
        /// The model the row's view reads.
        ModelReadByView,
        /// The model the page's row renderer reads, the page's rows being
        /// tinted by it rather than views.
        ModelReadByRenderer,
        /// The row's view itself.
        View,
    }

    /// A white panel holding a list of 300 [`CellRow`]s, 100 px tall, at
    /// the top left of the window; or, `inline`, of rows its row renderer
    /// tints by their models itself.
    struct CellsPage {
        state: ListState,
        rows: Vec<Entity<CellRow>>,
        inline: bool,
    }

    impl Render for CellsPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rows = self.rows.clone();
            let inline = self.inline;
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.state.clone(), move |row, _, cx| {
                    if !inline {
                        return rows[row].clone().into_any_element();
                    }
                    let cell = rows[row].read(cx);
                    cell.rendered.borrow_mut().push(row);
                    let tint = *cell.tint.read(cx);
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(row_height(row)))
                        .bg(row_color(row + tint as usize * 7))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    /// A [`CellsPage`], the models tinting its rows, and the log of the rows
    /// that rendered.
    fn cells_page(
        cx: &mut TestAppContext,
        inline: bool,
    ) -> (
        AnyWindowHandle,
        Vec<Entity<u32>>,
        Vec<Entity<CellRow>>,
        Rc<RefCell<Vec<usize>>>,
    ) {
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let tints: Vec<Entity<u32>> = (0..300).map(|_| cx.new(|_| 0)).collect();
        let rows: Vec<Entity<CellRow>> = (0..300)
            .map(|row| {
                let tint = tints[row].clone();
                let rendered = rendered.clone();
                cx.new(move |_| CellRow {
                    row,
                    tint,
                    shade: 0,
                    rendered,
                })
            })
            .collect();
        let window = cx.add_window({
            let rows = rows.clone();
            move |_, _| CellsPage {
                state: ListState::new(300, ListAlignment::Top, px(0.)).measure_all(),
                rows,
                inline,
            }
        });
        open_at(cx, window.into(), 1.);
        (window.into(), tints, rows, rendered)
    }

    /// A list whose rows read models keeps its layer when one of them
    /// changes: the row that read it is rendered again, alone, and the rest
    /// of the rows the layer holds stay as they are, whether the row shows
    /// or lies in the overscan. It draws as it does without layers.
    #[crate::test]
    fn a_change_read_by_one_row_renders_that_row_alone(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        for change in [
            RowChange::ModelReadByView,
            RowChange::ModelReadByRenderer,
            RowChange::View,
        ] {
            change_read_by_one_row_renders_that_row_alone(cx, change);
        }
    }

    /// See [`a_change_read_by_one_row_renders_that_row_alone`].
    fn change_read_by_one_row_renders_that_row_alone(cx: &mut TestAppContext, change: RowChange) {
        let inline = matches!(change, RowChange::ModelReadByRenderer);
        let (with_layers, with_tints, with_rows, log) = cells_page(cx, inline);
        let (without_layers, without_tints, without_rows, _) = cells_page(cx, inline);
        compare_with_layers_off(
            cx,
            with_layers,
            without_layers,
            &[-20., -20., -20.],
            "promote",
        );
        assert_eq!(decision(cx, with_layers), Some(Decision::Composite));
        let held: BTreeSet<usize> = held_rows(cx, with_layers).into_iter().collect();
        let shown = 5;
        let overscan = *held.last().unwrap();
        assert!(held.contains(&shown) && overscan > 10, "held {held:?}");
        log.borrow_mut().clear();

        let tint = |cx: &mut TestAppContext, row: usize| {
            for (tints, rows) in [(&with_tints, &with_rows), (&without_tints, &without_rows)] {
                match change {
                    RowChange::View => rows[row].update(cx, |cell, cx| {
                        cell.shade += 1;
                        cx.notify();
                    }),
                    _ => tints[row].update(cx, |tint, cx| {
                        *tint += 1;
                        cx.notify();
                    }),
                }
            }
        };
        let extended = extended_frames();
        let mut held = held;
        for (step, row) in [shown, overscan, shown].into_iter().enumerate() {
            tint(cx, row);
            let composited =
                compare_with_layers_off(cx, with_layers, without_layers, &[-1.], "tint");
            assert_eq!(composited, 1, "{change:?}, step {step}");
            // Besides the rows the scroll adds to the layer.
            let now: BTreeSet<usize> = held_rows(cx, with_layers).into_iter().collect();
            let rendered: BTreeSet<usize> = log
                .borrow_mut()
                .drain(..)
                .filter(|rendered| held.contains(rendered))
                .collect();
            assert_eq!(
                rendered,
                BTreeSet::from([row]),
                "{change:?}, step {step}: only the row that read the change renders"
            );
            held = now;
        }
        assert!(extended_frames() - extended >= 3);
        let demoted = with_window(cx, with_layers, |window, _| {
            window.layout_stats().layers_demoted
        });
        assert_eq!(demoted, 0);
        // The row in the overscan shows as it was tinted.
        compare_with_layers_off(cx, with_layers, without_layers, &[-25.; 12], "scroll down");
    }
}

/// Lists whose rows hand the frame more than what they draw: hitboxes,
/// element states, paths; and rows in views of their own.
mod rows {
    use super::{
        Decision, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, compare_with_layers_off, decision, draw,
        expanded_quads, held_rows, open_at, row_color, wheel, with_window,
    };
    use crate::{
        AnyWindowHandle, AppContext as _, Bounds, Context, Entity, IntoElement, ListAlignment,
        ListState, ParentElement as _, Pixels, Render, StatefulInteractiveElement as _,
        Styled as _, TestAppContext, Window, div, px, rgb,
    };
    use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

    /// What each row of a [`RowsPage`] holds besides its colour.
    #[derive(Clone, Copy)]
    enum RowKind {
        /// A scroll container of its own, taller inside than it shows.
        Scrolling,
        /// A path, drawn over the row's colour.
        Path,
        /// A hover style, as a list's rows mostly have: a hitbox.
        Hover,
    }

    /// A row of `kind`, 40 px tall.
    fn row(kind: RowKind, ix: usize) -> crate::AnyElement {
        use crate::InteractiveElement as _;
        let base = div().w(px(VIEWPORT_WIDTH)).h(px(40.)).bg(row_color(ix));
        match kind {
            RowKind::Scrolling => base
                .child(
                    div()
                        .id(("inner", ix))
                        .overflow_y_scroll()
                        .h(px(40.))
                        .child(div().h(px(40.)).bg(row_color(ix + 7)))
                        .child(div().h(px(40.)).bg(row_color(ix + 13))),
                )
                .into_any_element(),
            RowKind::Path => base
                .child(
                    crate::canvas(
                        |_, _, _| {},
                        |bounds, _, window, _| {
                            let origin = bounds.origin;
                            let mut path = crate::Path::new(origin);
                            path.line_to(origin + crate::point(px(30.), px(0.)));
                            path.line_to(origin + crate::point(px(0.), px(30.)));
                            window.paint_path(path, crate::black());
                        },
                    )
                    .h(px(40.))
                    .w_full(),
                )
                .into_any_element(),
            RowKind::Hover => base
                .id(("row", ix))
                .hover(move |style| style.bg(row_color(ix + 1)))
                .into_any_element(),
        }
    }

    /// A white panel holding a list, or a uniform list when `uniform`, of
    /// 300 rows of `kind`, 100 px tall, at the top left of the window.
    struct RowsPage {
        kind: RowKind,
        uniform: bool,
        state: ListState,
    }

    impl Render for RowsPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let kind = self.kind;
            let page = div().size_full().bg(rgb(0xffffff));
            if self.uniform {
                page.child(
                    crate::uniform_list(
                        "list",
                        300,
                        cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                            range.map(|ix| row(kind, ix)).collect::<Vec<_>>()
                        }),
                    )
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(VIEWPORT_HEIGHT)),
                )
            } else {
                page.child(
                    crate::list(self.state.clone(), move |ix, _, _| row(kind, ix))
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(VIEWPORT_HEIGHT)),
                )
            }
        }
    }

    fn page(cx: &mut TestAppContext, kind: RowKind, uniform: bool) -> AnyWindowHandle {
        let window = cx.add_window(move |_, _| RowsPage {
            kind,
            uniform,
            state: ListState::new(300, ListAlignment::Top, px(0.)).measure_all(),
        });
        open_at(cx, window.into(), 1.);
        window.into()
    }

    /// The part of each hitbox of the window's last frame that can be hit,
    /// sorted.
    fn hittable(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<Bounds<Pixels>> {
        with_window(cx, window, |window, _| {
            let mut hittable: Vec<_> = window
                .rendered_frame
                .hitboxes
                .iter()
                .map(|hitbox| hitbox.bounds.intersect(&hitbox.content_mask.bounds))
                .filter(|bounds| !bounds.is_empty())
                .collect();
            hittable.sort_by_key(|bounds| format!("{bounds:?}"));
            hittable
        })
    }

    /// Scrolls a list whose rows are scroll containers themselves, and checks
    /// that it can be hit, and draws, as it does without layers: a wheel over
    /// a row the layer holds scrolls that row.
    fn rows_that_can_be_hit_stay_hittable(cx: &mut TestAppContext, uniform: bool) {
        let with_layers = page(cx, RowKind::Scrolling, uniform);
        let without_layers = page(cx, RowKind::Scrolling, uniform);
        with_window(cx, without_layers, |window, _| {
            window.set_scroll_layers(false)
        });
        draw(cx, without_layers);
        // Each wheel scrolls the row under the pointer as far as it goes, then
        // the list.
        for (frame, dy) in [-40., -40., -40., -40., -10., -40., 25., -40., -40., -40.]
            .into_iter()
            .enumerate()
        {
            wheel(cx, with_layers, dy);
            wheel(cx, without_layers, dy);
            assert_eq!(
                hittable(cx, with_layers),
                hittable(cx, without_layers),
                "frame {frame}: the rows can be hit as without layers"
            );
            let expected = with_window(cx, without_layers, |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            let actual = with_window(cx, with_layers, |window, _| {
                expanded_quads(&window.rendered_frame.scene)
            });
            assert_eq!(actual, expected, "frame {frame}, scrolled by {dy}");
        }
    }

    #[crate::test]
    fn a_list_whose_rows_can_be_hit_stays_hittable(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        rows_that_can_be_hit_stay_hittable(cx, false);
    }

    #[crate::test]
    fn a_uniform_list_whose_rows_can_be_hit_stays_hittable(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        rows_that_can_be_hit_stay_hittable(cx, true);
    }

    #[crate::test]
    fn a_list_whose_rows_paint_paths_is_kept_off_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        for uniform in [false, true] {
            let with_layers = page(cx, RowKind::Path, uniform);
            let without_layers = page(cx, RowKind::Path, uniform);
            // Promoted and painted once into the layer, then demoted: paths
            // composited from tiles would not land as drawn afresh.
            compare_with_layers_off(cx, with_layers, without_layers, &[-20., -20.], "promote");
            assert_eq!(decision(cx, with_layers), Some(Decision::Repaint));
            for step in 0..5 {
                compare_with_layers_off(cx, with_layers, without_layers, &[-15.], "demoted");
                assert_eq!(
                    decision(cx, with_layers),
                    Some(Decision::Bypass),
                    "uniform {uniform}, step {step}"
                );
            }
        }
    }

    /// A list whose rows take input (a hover style: a hitbox, a listener,
    /// element state) is composited from its layer while it scrolls under a
    /// still pointer, its rows' records carried from frame to frame; the
    /// rows whose hover changes, and only those, are rendered again, and it
    /// draws and can be hit as without layers.
    #[crate::test]
    fn a_list_whose_rows_take_input_composites_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        for uniform in [false, true] {
            let with_layers = page(cx, RowKind::Hover, uniform);
            let without_layers = page(cx, RowKind::Hover, uniform);
            with_window(cx, with_layers, |window, _| window.reset_layout_stats());
            let extended = crate::fast::layers::lists::extended_frames();
            let for_hover = crate::fast::layers::lists::rows_rendered_for_hover();
            let mut composited = 0;
            for sweep in 0..3 {
                let dy = if sweep % 2 == 0 { -5. } else { 5. };
                composited +=
                    compare_with_layers_off(cx, with_layers, without_layers, &[dy; 80], "scroll");
                assert_eq!(
                    hittable(cx, with_layers),
                    hittable(cx, without_layers),
                    "uniform {uniform}, sweep {sweep}"
                );
            }
            let extended = crate::fast::layers::lists::extended_frames() - extended;
            let for_hover = crate::fast::layers::lists::rows_rendered_for_hover() - for_hover;
            let stats = with_window(cx, with_layers, |window, _| window.layout_stats());
            assert!(
                composited >= 235,
                "uniform {uniform}: composited {composited}"
            );
            assert!(
                extended >= 235,
                "uniform {uniform}: kept rows in {extended}"
            );
            assert_eq!(stats.layers_demoted, 0, "uniform {uniform}");
            // The pointer, near the top of the list, crosses into another row
            // every eight frames: the row it leaves and the row it enters are
            // rendered again, and so is a row showing again after it left.
            assert!(
                (50..=90).contains(&for_hover),
                "uniform {uniform}: {for_hover} rows rendered again for their hover"
            );
        }
    }

    /// The rows a list shows are laid out before the first of them is
    /// prepainted, and the layer lays out a row before it prepaints it: the
    /// layout keys each row it holds keeps are those laying that row out
    /// claimed, and no other row's. Holding a row, it keeps no other row's
    /// nodes, which that row would then find taken when laid out again.
    #[crate::test]
    fn a_list_layer_keeps_each_row_its_own_layout_keys(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, RowKind::Hover, false);
        wheel(cx, window, -20.);
        wheel(cx, window, -20.);
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        for dy in [-10.; 6] {
            wheel(cx, window, dy);
            let rows = with_window(cx, window, |window, _| {
                let layer = window.fast_layers.layers.values().next().expect("a layer");
                layer
                    .rows
                    .held_layout_keys()
                    .map(|(row, keys)| (row, keys.to_vec()))
                    .collect::<Vec<_>>()
            });
            assert!(rows.len() > 3, "held {rows:?}");
            let mut owners = collections::FxHashMap::default();
            for (row, keys) in &rows {
                assert!(!keys.is_empty(), "row {row} keeps no keys");
                for key in keys {
                    if let Some(other) = owners.insert(*key, *row) {
                        assert_eq!(other, *row, "rows {other} and {row} share a key");
                    }
                }
            }
        }
    }

    /// A list scrolled down and back up again over rows it laid out before
    /// lays none of them out afresh: the rows its layer holds keep the
    /// layout nodes laying them out claimed, those it lays out to extend
    /// the layer theirs, and the rows scrolled out of the layer's reach
    /// keep theirs until they are back. It draws as it does without layers.
    #[crate::test]
    fn a_list_scrolled_back_and_forth_lays_no_row_out_afresh(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let with_layers = page(cx, RowKind::Hover, false);
        let without_layers = page(cx, RowKind::Hover, false);
        let sweep = |cx: &mut TestAppContext| {
            compare_with_layers_off(cx, with_layers, without_layers, &[-10.; 60], "down")
                + compare_with_layers_off(cx, with_layers, without_layers, &[10.; 60], "up")
        };
        sweep(cx);
        with_window(cx, with_layers, |window, _| window.reset_layout_stats());
        let composited = sweep(cx);
        let stats = with_window(cx, with_layers, |window, _| window.layout_stats());
        assert!(composited >= 110, "composited {composited}");
        assert_eq!(stats.nodes_created, 0, "{stats:?}");
    }

    /// A white panel holding a uniform list of 300 rows with a hover style,
    /// 40 px tall, 100 px tall itself, at the top left of the window. Every
    /// range of rows the list renders is logged.
    struct HoverRowsPage {
        rendered: Rc<RefCell<Vec<std::ops::Range<usize>>>>,
    }

    impl Render for HoverRowsPage {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.rendered.clone();
            div().size_full().bg(rgb(0xffffff)).child(
                crate::uniform_list(
                    "list",
                    300,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                        rendered.borrow_mut().push(range.clone());
                        range.map(|ix| row(RowKind::Hover, ix)).collect::<Vec<_>>()
                    }),
                )
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    /// Rows move under a still pointer: a frame renders the rows new to the
    /// layer and, when the pointer crosses into another row, the row it left
    /// and the row it entered, whose hover styles change, and no other.
    #[crate::test]
    fn a_hover_change_renders_only_the_rows_it_touches(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let log = rendered.clone();
        let window: AnyWindowHandle = cx
            .add_window(move |_, _| HoverRowsPage { rendered: log })
            .into();
        open_at(cx, window, 1.);
        // Far enough down that row 0, which the list renders alone to measure
        // it, is not held.
        for _ in 0..40 {
            wheel(cx, window, -40.);
        }
        let step = 5.;
        wheel(cx, window, -step);
        assert_eq!(decision(cx, window), Some(Decision::Composite));
        // The wheel is turned with the pointer 20 px down the list.
        let hovered = |offset: f32| ((20. + offset) / 40.).floor() as usize;
        let mut offset = 1600. + step;
        let mut held: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
        let mut crossings = 0;
        rendered.borrow_mut().clear();
        for frame in 0..60 {
            let before = hovered(offset);
            offset += step;
            wheel(cx, window, -step);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "frame {frame}"
            );
            let now: BTreeSet<usize> = held_rows(cx, window).into_iter().collect();
            let rows: BTreeSet<usize> = rendered
                .borrow_mut()
                .drain(..)
                .filter(|range| *range != (0..1))
                .flatten()
                .collect();
            let again: BTreeSet<usize> = rows.intersection(&held).copied().collect();
            let expected: BTreeSet<usize> = if hovered(offset) == before {
                BTreeSet::new()
            } else {
                crossings += 1;
                [before, hovered(offset)].into()
            };
            assert_eq!(again, expected, "frame {frame}: rows rendered again");
            assert!(rows.len() <= 3, "frame {frame}: {rows:?}");
            held = now;
        }
        assert!(crossings >= 7, "the pointer crossed {crossings} rows");
    }

    /// A row in a view of its own, of `color`.
    struct ColorRow {
        color: usize,
    }

    impl Render for ColorRow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(VIEWPORT_WIDTH))
                .h(px(20.))
                .bg(row_color(self.color))
        }
    }

    /// A list of 20 px rows whose row 2 is a [`ColorRow`].
    struct ViewRowPage {
        state: ListState,
        view: Entity<ColorRow>,
    }

    impl Render for ViewRowPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let view = self.view.clone();
            div().size_full().bg(rgb(0xffffff)).child(
                crate::list(self.state.clone(), move |ix, _, _| {
                    if ix == 2 {
                        return view.clone().into_any_element();
                    }
                    div()
                        .w(px(VIEWPORT_WIDTH))
                        .h(px(20.))
                        .bg(row_color(ix))
                        .into_any_element()
                })
                .w(px(VIEWPORT_WIDTH))
                .h(px(VIEWPORT_HEIGHT)),
            )
        }
    }

    /// A list of 300 rows, each a [`ColorRow`] of its own, beside an anchored
    /// element when `anchored`.
    struct ViewRowsPage {
        state: ListState,
        views: Vec<Entity<ColorRow>>,
        anchored: bool,
    }

    impl Render for ViewRowsPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use crate::prelude::FluentBuilder as _;
            let views = self.views.clone();
            div()
                .size_full()
                .bg(rgb(0xffffff))
                .child(
                    crate::list(self.state.clone(), move |ix, _, _| {
                        views[ix].clone().into_any_element()
                    })
                    .w(px(VIEWPORT_WIDTH))
                    .h(px(VIEWPORT_HEIGHT)),
                )
                .when(self.anchored, |page| {
                    page.child(
                        crate::anchored().child(div().w(px(50.)).h(px(50.)).bg(crate::red())),
                    )
                })
        }
    }

    fn view_rows_page(cx: &mut TestAppContext, anchored: bool) -> AnyWindowHandle {
        let handle = cx.add_window(move |_, cx| ViewRowsPage {
            state: ListState::new(300, ListAlignment::Top, px(0.)).measure_all(),
            views: (0..300)
                .map(|color| cx.new(|_| ColorRow { color }))
                .collect(),
            anchored,
        });
        let window: AnyWindowHandle = handle.into();
        open_at(cx, window, 1.);
        window
    }

    #[crate::test]
    fn an_anchored_element_beside_a_list_leaves_it_on_its_layer(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = view_rows_page(cx, true);
        let extended = crate::fast::layers::lists::extended_frames();
        for _ in 0..2 {
            wheel(cx, window, -10.);
        }
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        for step in 0..4 {
            wheel(cx, window, -10.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
        }
        assert_eq!(crate::fast::layers::lists::extended_frames(), extended + 4);
    }

    #[crate::test]
    fn what_a_list_layer_remembers_of_its_rows_stays_bounded(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = view_rows_page(cx, false);
        for _ in 0..2 {
            wheel(cx, window, -10.);
        }
        let extended = crate::fast::layers::lists::extended_frames();
        // Down through most of the 300 rows, 15 px a frame.
        for step in 0..350 {
            wheel(cx, window, -15.);
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
            with_window(cx, window, |window, _| {
                let layer = window.fast_layers.layers.values().next().unwrap();
                let views = layer.record.as_ref().unwrap().views.len();
                let held = layer.rows.held().count();
                assert!(
                    views <= held * 6,
                    "step {step}: the layer remembers {views} views for {held} rows"
                );
            });
        }
        let extended = crate::fast::layers::lists::extended_frames() - extended;
        assert!(extended > 300, "frames kept the rows held ({extended})");
    }

    #[crate::test]
    fn a_row_view_held_by_the_layer_shows_a_change_without_a_scroll(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = cx.add_window(move |_, cx| ViewRowPage {
            state: ListState::new(300, ListAlignment::Top, px(0.)),
            view: cx.new(|_| ColorRow { color: 2 }),
        });
        let window: AnyWindowHandle = handle.into();
        open_at(cx, window, 1.);
        let view = handle.update(cx, |page, _, _| page.view.clone()).unwrap();
        for _ in 0..4 {
            wheel(cx, window, -2.);
        }
        assert_eq!(decision(cx, window), Some(Decision::Composite));

        view.update(cx, |row, cx| {
            row.color = 50;
            cx.notify();
        });
        draw(cx, window);
        let color = format!("{:?}", crate::Background::from(row_color(50)));
        with_window(cx, window, |window, _| {
            let quads = expanded_quads(&window.rendered_frame.scene);
            assert!(
                quads.iter().any(|quad| quad.ends_with(&color)),
                "the row view shows its new colour"
            );
        });
    }
}
