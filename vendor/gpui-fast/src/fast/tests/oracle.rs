// Modified by Longbridge for gpui-fast.
//! Frames drawn with everything a window retains between frames must match the
//! frames it would draw with none of it.
//!
//! Each run drives two windows holding the same view through the same random
//! history of changes. One draws every frame as an application would, reusing
//! retained layout nodes. The other forgets them before every frame, so it draws each one as though it
//! were its first. Every frame, the two must paint the same primitives in the
//! same places and leave the same hitboxes; any difference is something a
//! retained shortcut got wrong.
//!
//! A shortcut taken within a single frame is taken by both windows alike, so
//! it is invisible here; its own tests have to cover it.

use std::{borrow::Cow, sync::Arc};

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

use crate::{
    AnyElement, App, Bounds, Context, DevicePixels, Entity, Font, FontId, FontMetrics, FontRun,
    Global, GlyphId, Hsla, InputEvent as _, IntoElement, LineLayout, ListAlignment, ListOffset,
    ListState, MouseMoveEvent, NoopTextSystem, Pixels, PlatformTextSystem, Render,
    RenderGlyphParams, Result, SharedString, Size, StyleRefinement, TestAppContext,
    TextRenderingMode, UniformListScrollHandle, Window, WindowHandle, anchored, deferred, div,
    hsla, list, point, prelude::*, px, size, uniform_list,
};

const WORDS: [&str; 10] = [
    "a",
    "grid",
    "cell",
    "ticking",
    "value",
    "with a longer label",
    "42",
    "lorem ipsum dolor",
    "x",
    "a label long enough to be cut short",
];

const PALETTE: [Hsla; 5] = [
    hsla(0.0, 0.0, 0.1, 1.0),
    hsla(0.6, 0.7, 0.5, 1.0),
    hsla(0.3, 0.6, 0.4, 1.0),
    hsla(0.0, 0.8, 0.6, 1.0),
    hsla(0.1, 0.9, 0.5, 0.5),
];

const GRID_CELLS: usize = 24;
const ROW_HEIGHT: f32 = 20.;
const INITIAL_ROWS: u64 = 30;

#[derive(Clone, Copy, Debug)]
enum CellFlag {
    Background,
    Underline,
    Truncate,
    Hover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CellState {
    word: usize,
    color: usize,
    width: f32,
    background: bool,
    underline: bool,
    truncate: bool,
    hover: bool,
}

/// How list rows identify themselves.
#[derive(Clone, Copy, Debug)]
enum RowIdentity {
    Position,
    Id,
    /// Wrapped in an element with the id, so switching to or from `Id`
    /// hands a row's node to its wrapper or its first child.
    Wrapped,
}

#[derive(Clone, Debug)]
enum Change {
    Word {
        cell: usize,
        word: usize,
    },
    Color {
        cell: usize,
        color: usize,
    },
    Width {
        cell: usize,
        width: f32,
    },
    Toggle {
        cell: usize,
        flag: CellFlag,
    },
    Paragraph {
        words: usize,
        width: f32,
    },
    InsertRow {
        at: usize,
    },
    RemoveRow {
        at: usize,
    },
    Scroll {
        top: f32,
    },
    InsertChip {
        at: usize,
    },
    RemoveChip {
        at: usize,
    },
    RotateChips {
        by: usize,
    },
    RowIdentity(RowIdentity),
    Direction,
    Badge,
    /// Changes one panel, notifying only it.
    Panel {
        panel: usize,
        value: usize,
    },
    /// Changes the leaf view nested in a panel, notifying only it.
    Leaf {
        panel: usize,
    },
    /// Changes only the colors of a panel, notifying only it: its layout
    /// stays as it was, so the view around it is drawn from last frame
    /// around it.
    TintPanel {
        panel: usize,
    },
    /// Changes only the colors of the leaf in a panel, notifying only it: the
    /// views around it are drawn from last frame around it.
    TintLeaf {
        panel: usize,
    },
    /// Changes the model some panels read without observing it, notifying
    /// only the model.
    Shared {
        value: usize,
    },
    /// Changes a global some panels read.
    Global {
        value: usize,
    },
    MoveMouse {
        x: f32,
        y: f32,
    },
    Resize {
        width: f32,
        height: f32,
    },
    Redraw,
}

impl Change {
    fn random(rng: &mut StdRng) -> Self {
        let cell = rng.random_range(0..GRID_CELLS);
        match rng.random_range(0..124) {
            0..20 => Change::Word {
                cell,
                word: rng.random_range(0..WORDS.len()),
            },
            20..27 => Change::Color {
                cell,
                color: rng.random_range(0..PALETTE.len()),
            },
            27..33 => Change::Width {
                cell,
                width: rng.random_range(20.0..220.0),
            },
            33..42 => Change::Toggle {
                cell,
                flag: [
                    CellFlag::Background,
                    CellFlag::Underline,
                    CellFlag::Truncate,
                    CellFlag::Hover,
                ][rng.random_range(0..4)],
            },
            42..48 => Change::Paragraph {
                words: rng.random_range(0..40),
                width: rng.random_range(60.0..400.0),
            },
            48..55 => Change::InsertRow {
                at: rng.random_range(0..64),
            },
            55..61 => Change::RemoveRow {
                at: rng.random_range(0..64),
            },
            61..67 => Change::Scroll {
                top: rng.random_range(0.0..400.0),
            },
            67..69 => Change::InsertChip {
                at: rng.random_range(0..16),
            },
            69..71 => Change::RemoveChip {
                at: rng.random_range(0..16),
            },
            71..72 => Change::RotateChips {
                by: rng.random_range(1..4),
            },
            72..75 => Change::RowIdentity(
                [RowIdentity::Position, RowIdentity::Id, RowIdentity::Wrapped]
                    [rng.random_range(0..3)],
            ),
            75..77 => Change::Direction,
            77..83 => Change::Badge,
            83..92 => Change::MoveMouse {
                x: rng.random_range(0.0..900.0),
                y: rng.random_range(0.0..700.0),
            },
            100..105 => Change::Panel {
                panel: rng.random_range(0..PANELS),
                value: rng.random_range(0..WORDS.len()),
            },
            105..109 => Change::Leaf {
                panel: rng.random_range(0..PANELS),
            },
            109..113 => Change::Shared {
                value: rng.random_range(0..WORDS.len()),
            },
            113..116 => Change::Global {
                value: rng.random_range(0..PALETTE.len()),
            },
            116..120 => Change::TintPanel {
                panel: rng.random_range(0..PANELS),
            },
            120..124 => Change::TintLeaf {
                panel: rng.random_range(0..PANELS),
            },
            92..95 => Change::Resize {
                width: rng.random_range(300.0..1000.0),
                height: rng.random_range(240.0..800.0),
            },
            _ => Change::Redraw,
        }
    }
}

const PANELS: usize = 6;

/// A small application: a grid of cached cell views, wrapping paragraphs, a
/// row of chips, a cached child view, child views that are not cached, one of
/// them deferred, and the same rows in a uniform list and a list.
struct OracleView {
    cells: Vec<CellState>,
    /// A view per cell, showing the cell's state.
    cell_views: Vec<Entity<GridCell>>,
    paragraph_words: usize,
    paragraph_width: Pixels,
    rows: Vec<u64>,
    /// Chips share the row numbering, so no chip and row are the same thing.
    chips: Vec<u64>,
    next_row: u64,
    row_identity: RowIdentity,
    scroll_top: Pixels,
    column: bool,
    uniform_scroll: UniformListScrollHandle,
    list_state: ListState,
    badge: Entity<Badge>,
    panels: Vec<Entity<Panel>>,
    shared: Entity<Shared>,
}

impl OracleView {
    fn new(cx: &mut Context<Self>) -> Self {
        let shared = cx.new(|_| Shared { value: 0 });
        let cells: Vec<CellState> = (0..GRID_CELLS)
            .map(|ix| CellState {
                word: ix % WORDS.len(),
                color: ix % PALETTE.len(),
                width: 40. + (ix % 5) as f32 * 30.,
                background: ix.is_multiple_of(3),
                truncate: ix.is_multiple_of(4),
                ..CellState::default()
            })
            .collect();
        Self {
            cell_views: cells
                .iter()
                .map(|&cell| cx.new(|_| GridCell(cell)))
                .collect(),
            cells,
            paragraph_words: 12,
            paragraph_width: px(180.),
            rows: (0..INITIAL_ROWS).collect(),
            chips: (INITIAL_ROWS..INITIAL_ROWS + 6).collect(),
            next_row: INITIAL_ROWS + 6,
            row_identity: RowIdentity::Position,
            scroll_top: px(0.),
            column: false,
            uniform_scroll: UniformListScrollHandle::new(),
            list_state: ListState::new(INITIAL_ROWS as usize, ListAlignment::Top, px(40.)),
            badge: cx.new(|_| Badge { count: 0 }),
            panels: {
                (0..PANELS)
                    .map(|ix| {
                        let shared = shared.clone();
                        cx.new(|cx| Panel {
                            ix,
                            value: ix,
                            tint: 0,
                            shared,
                            leaf: cx.new(|_| Leaf { count: ix, tint: 0 }),
                        })
                    })
                    .collect()
            },
            shared,
        }
    }

    fn apply(&mut self, change: &Change, cx: &mut Context<Self>) {
        match *change {
            Change::Word { cell, word } => self.update_cell(cell, |cell| cell.word = word, cx),
            Change::Color { cell, color } => self.update_cell(cell, |cell| cell.color = color, cx),
            Change::Width { cell, width } => self.update_cell(cell, |cell| cell.width = width, cx),
            Change::Toggle { cell, flag } => self.update_cell(
                cell,
                |cell| {
                    let value = match flag {
                        CellFlag::Background => &mut cell.background,
                        CellFlag::Underline => &mut cell.underline,
                        CellFlag::Truncate => &mut cell.truncate,
                        CellFlag::Hover => &mut cell.hover,
                    };
                    *value = !*value;
                },
                cx,
            ),
            Change::Paragraph { words, width } => {
                self.paragraph_words = words;
                self.paragraph_width = px(width);
            }
            Change::InsertRow { at } => {
                let at = at % (self.rows.len() + 1);
                self.rows.insert(at, self.next_row);
                self.next_row += 1;
                self.list_state.splice(at..at, 1);
            }
            Change::RemoveRow { at } => {
                if self.rows.is_empty() {
                    return;
                }
                let at = at % self.rows.len();
                self.rows.remove(at);
                self.list_state.splice(at..at + 1, 0);
            }
            Change::Scroll { top } => self.scroll_top = px(top),
            Change::InsertChip { at } => {
                let at = at % (self.chips.len() + 1);
                self.chips.insert(at, self.next_row);
                self.next_row += 1;
            }
            Change::RemoveChip { at } => {
                if !self.chips.is_empty() {
                    let at = at % self.chips.len();
                    self.chips.remove(at);
                }
            }
            Change::RotateChips { by } => {
                if !self.chips.is_empty() {
                    let by = by % self.chips.len();
                    self.chips.rotate_left(by);
                }
            }
            Change::RowIdentity(identity) => self.row_identity = identity,
            Change::Direction => self.column = !self.column,
            Change::Badge
            | Change::Panel { .. }
            | Change::Leaf { .. }
            | Change::TintPanel { .. }
            | Change::TintLeaf { .. }
            | Change::Shared { .. } => {
                self.children().apply_to_children(change, cx);
                return;
            }
            Change::Global { .. }
            | Change::MoveMouse { .. }
            | Change::Resize { .. }
            | Change::Redraw => return,
        }
        cx.notify();
    }

    /// The views nested in this one and the model, for changes applied to
    /// them alone.
    fn children(&self) -> Children {
        Children {
            badge: self.badge.clone(),
            panels: self.panels.clone(),
            shared: self.shared.clone(),
        }
    }

    /// Changes a cell and notifies its view; the parent is notified too,
    /// because it sizes the cell.
    fn update_cell(
        &mut self,
        ix: usize,
        change: impl FnOnce(&mut CellState),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.cells[ix]);
        let cell = self.cells[ix];
        self.cell_views[ix].update(cx, |view, cx| {
            view.0 = cell;
            cx.notify();
        });
    }
}

/// Handles on the views nested in an [`OracleView`], and on its model.
struct Children {
    badge: Entity<Badge>,
    panels: Vec<Entity<Panel>>,
    shared: Entity<Shared>,
}

impl Children {
    /// Changes a nested view, or the model, notifying only it.
    /// The harness applies these without updating this view, so that it is
    /// dirty only because of what is nested in it, as it is when an
    /// application notifies a nested view on its own.
    fn apply_to_children(&self, change: &Change, cx: &mut App) {
        match *change {
            Change::Badge => {
                // Only the child is notified, so the parent's frame reuses
                // whatever it can of the last one around it.
                self.badge.update(cx, |badge, cx| {
                    badge.count += 1;
                    cx.notify();
                });
            }
            Change::Panel { panel, value } => {
                self.panels[panel].update(cx, |panel, cx| {
                    panel.value = value;
                    cx.notify();
                });
            }
            Change::Leaf { panel } => {
                let leaf = self.panels[panel].read(cx).leaf.clone();
                leaf.update(cx, |leaf, cx| {
                    leaf.count += 1;
                    cx.notify();
                });
            }
            Change::TintPanel { panel } => {
                self.panels[panel].update(cx, |panel, cx| {
                    panel.tint += 1;
                    cx.notify();
                });
            }
            Change::TintLeaf { panel } => {
                let leaf = self.panels[panel].read(cx).leaf.clone();
                leaf.update(cx, |leaf, cx| {
                    leaf.tint += 1;
                    cx.notify();
                });
            }
            Change::Shared { value } => {
                self.shared.update(cx, |shared, cx| {
                    shared.value = value;
                    cx.notify();
                });
            }
            _ => unreachable!("not a change to a nested view"),
        }
    }
}

/// A cell of the grid, drawn as a cached view.
struct GridCell(CellState);

impl Render for GridCell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        render_cell(self.0)
    }
}

fn render_cell(cell: CellState) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .w(px(cell.width))
        .h(px(18.))
        .text_color(PALETTE[(cell.color + 1) % PALETTE.len()])
        .when(cell.background, |this| this.bg(PALETTE[cell.color]))
        .when(cell.underline, |this| this.underline())
        .when(cell.truncate, |this| {
            this.overflow_hidden().whitespace_nowrap().text_ellipsis()
        })
        .when(cell.hover, |this| {
            this.hover(|style| style.bg(PALETTE[4]).text_color(PALETTE[0]))
        })
        .child(WORDS[cell.word])
        .into_any_element()
}

fn render_row(row: u64, identity: RowIdentity) -> AnyElement {
    let word = WORDS[(row as usize * 7) % WORDS.len()];
    let row_element = div()
        .flex()
        .flex_row()
        .gap_2()
        .h(px(ROW_HEIGHT))
        .child(SharedString::from(format!("row {row}")))
        .child(
            div()
                .w(px(8. + (row % 4) as f32 * 6.))
                .h(px(8.))
                .bg(PALETTE[row as usize % PALETTE.len()]),
        )
        .child(
            div()
                .border_1()
                .border_color(PALETTE[1])
                .px_1()
                .when(row.is_multiple_of(3), |this| {
                    this.hover(|style| style.bg(PALETTE[2]))
                })
                .child(word),
        );
    match identity {
        RowIdentity::Position => row_element.into_any_element(),
        RowIdentity::Id => row_element.id(("row", row)).into_any_element(),
        RowIdentity::Wrapped => div().id(("row", row)).child(row_element).into_any_element(),
    }
}

/// A chip in a row whose children come and go and change places, which a
/// retained parent has to follow.
fn render_chip(chip: u64, identity: RowIdentity) -> AnyElement {
    let chip_element = div()
        .flex()
        .flex_row()
        .px_1()
        .h(px(16.))
        .min_w(px(10. + (chip % 5) as f32 * 8.))
        .bg(PALETTE[chip as usize % PALETTE.len()])
        .when(chip.is_multiple_of(2), |this| {
            this.border_1().border_color(PALETTE[0])
        })
        .child(SharedString::from(chip.to_string()));
    match identity {
        RowIdentity::Position => chip_element.into_any_element(),
        RowIdentity::Id => chip_element.id(("chip", chip)).into_any_element(),
        RowIdentity::Wrapped => div()
            .id(("chip", chip))
            .child(chip_element)
            .into_any_element(),
    }
}

impl Render for OracleView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let paragraph: String = (0..self.paragraph_words)
            .map(|ix| WORDS[ix % WORDS.len()])
            .collect::<Vec<_>>()
            .join(" ");

        let rows = self.rows.clone();
        let identity = self.row_identity;
        self.uniform_scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.), -self.scroll_top));
        let uniform_rows = uniform_list("uniform rows", rows.len(), {
            let rows = rows.clone();
            move |range, _, _| range.map(|ix| render_row(rows[ix], identity)).collect()
        })
        .track_scroll(&self.uniform_scroll)
        .w(px(260.))
        .h(px(120.));

        if !rows.is_empty() {
            let item_ix = ((self.scroll_top / px(ROW_HEIGHT)).floor() as usize).min(rows.len() - 1);
            self.list_state.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(self.scroll_top.as_f32() % ROW_HEIGHT),
            });
        }
        let list_rows = list(self.list_state.clone(), move |ix, _, _| {
            render_row(rows[ix], identity)
        })
        .w(px(260.))
        .h(px(120.));

        div()
            .size_full()
            .flex()
            .flex_wrap()
            .gap_2()
            .when(self.column, |this| this.flex_col())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .w(px(420.))
                    .gap_1()
                    // Every cell is a cached view notified when it changes, so
                    // the window drawing incrementally reuses the ones that did
                    // not change while the one drawing from scratch builds them
                    // all.
                    .children(self.cells.iter().zip(&self.cell_views).map(|(cell, view)| {
                        view.clone()
                            .cached(StyleRefinement::default().w(px(cell.width)).h(px(18.)))
                    })),
            )
            .child(
                div()
                    .w(self.paragraph_width)
                    .text_color(PALETTE[0])
                    .child(paragraph.clone()),
            )
            .child(
                // A flex item is measured for its content size before it is
                // shrunk to fit, so this text is shaped unconstrained and then
                // at whatever width it ends up with.
                div()
                    .flex()
                    .flex_row()
                    .w(self.paragraph_width * 0.8)
                    .child(div().text_color(PALETTE[1]).child(paragraph))
                    .child(div().w(px(24.)).h(px(12.)).bg(PALETTE[2])),
            )
            .child(
                self.badge
                    .clone()
                    .cached(StyleRefinement::default().w(px(120.)).h(px(24.))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .children(self.chips.iter().map(|&chip| render_chip(chip, identity))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(self.panels[..PANELS - 1].iter().cloned()),
            )
            .child(
                div()
                    .size(px(10.))
                    .child(deferred(anchored().child(self.panels[PANELS - 1].clone()))),
            )
            .child(uniform_rows)
            .child(list_rows)
    }
}

/// A model panels read without observing it.
struct Shared {
    value: usize,
}

/// A global panels read.
struct Accent(usize);

impl Global for Accent {}

/// A child view that is not cached, so the window keeps it
/// from one frame to the next by itself. Some panels read the shared model
/// and the global.
struct Panel {
    ix: usize,
    value: usize,
    /// Shifts its colors, leaving its layout as it was.
    tint: usize,
    shared: Entity<Shared>,
    leaf: Entity<Leaf>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shared = if self.ix.is_multiple_of(2) {
            self.shared.read(cx).value
        } else {
            0
        };
        let accent = if self.ix.is_multiple_of(3) {
            cx.try_global::<Accent>().map_or(0, |accent| accent.0)
        } else {
            0
        };
        div()
            .flex()
            .flex_col()
            .p_1()
            .bg(PALETTE[(self.value + accent + self.tint) % PALETTE.len()])
            .when(self.ix == 1, |this| {
                this.hover(|style| style.bg(PALETTE[4]))
            })
            .child(WORDS[(self.value + shared) % WORDS.len()])
            // Some leaves are cached, so a panel is drawn around a cached
            // view built again in it, as around one that is not.
            .when(self.ix >= PANELS / 2, |this| {
                this.child(
                    self.leaf
                        .clone()
                        .cached(StyleRefinement::default().w(px(30.)).h(px(8.))),
                )
            })
            .when(self.ix < PANELS / 2, |this| this.child(self.leaf.clone()))
    }
}

/// A view nested in a panel, notified on its own.
struct Leaf {
    count: usize,
    tint: usize,
}

impl Render for Leaf {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .children((0..self.count % 3 + 1).map(|ix| {
                div()
                    .w(px(4. + ix as f32 * 5.))
                    .h(px(6.))
                    .bg(PALETTE[(self.count + ix + self.tint) % PALETTE.len()])
            }))
    }
}

/// A child view that is sometimes notified on its own.
struct Badge {
    count: usize,
}

impl Render for Badge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .gap_1()
            .size_full()
            .children((0..self.count % 4 + 1).map(|ix| {
                div()
                    .w(px(6. + ix as f32 * 3.))
                    .h(px(10.))
                    .bg(PALETTE[ix % PALETTE.len()])
            }))
            .child(SharedString::from(self.count.to_string()))
    }
}

/// The no-op text system, except that every glyph rasterizes to a small box,
/// so text paints a sprite per glyph and where each glyph went is compared.
pub(super) struct GlyphBoxTextSystem(pub(super) NoopTextSystem);

impl PlatformTextSystem for GlyphBoxTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        self.0.all_font_names()
    }

    fn font_id(&self, descriptor: &Font) -> Result<FontId> {
        self.0.font_id(descriptor)
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.0.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        self.0.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(&self, _params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        Ok(Bounds {
            origin: point(DevicePixels(0), DevicePixels(-8)),
            size: size(DevicePixels(5), DevicePixels(9)),
        })
    }

    fn rasterize_glyph(
        &self,
        _params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        let area = raster_bounds.size.width.0 * raster_bounds.size.height.0;
        Ok((raster_bounds.size, vec![u8::MAX; area as usize]))
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.0.layout_line(text, font_size, runs)
    }

    fn recommended_rendering_mode(&self, font_id: FontId, font_size: Pixels) -> TextRenderingMode {
        self.0.recommended_rendering_mode(font_id, font_size)
    }
}

fn apply(cx: &mut TestAppContext, window: WindowHandle<OracleView>, change: &Change) {
    match *change {
        Change::MoveMouse { x, y } => {
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(
                    MouseMoveEvent {
                        position: point(px(x), px(y)),
                        pressed_button: None,
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .unwrap();
        }
        Change::Resize { width, height } => {
            cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        }
        Change::Global { value } => cx.update(|cx| cx.set_global(Accent(value))),
        Change::Badge
        | Change::Panel { .. }
        | Change::Leaf { .. }
        | Change::TintPanel { .. }
        | Change::TintLeaf { .. }
        | Change::Shared { .. } => {
            let view = window.read_with(cx, |view, _| view.children()).unwrap();
            cx.update(|cx| view.apply_to_children(change, cx));
        }
        _ => window
            .update(cx, |view, _, cx| view.apply(change, cx))
            .unwrap(),
    }
}

/// Draws a frame, after forgetting what the window retains if asked, and
/// returns what it drew with the layout nodes it reused.
fn draw(
    cx: &mut TestAppContext,
    window: WindowHandle<OracleView>,
    from_scratch: bool,
) -> (Vec<String>, u64, bool) {
    cx.update_window(window.into(), |_, window, cx| {
        if from_scratch {
            window.forget_retained_state();
        }
        window.reset_layout_stats();
        window.draw(cx).clear(cx);
        (
            window.describe_rendered_frame(),
            window.layout_stats().nodes_reused,
            window.rendered_frame.retained.reused_any(),
        )
    })
    .unwrap()
}

/// Drives both windows through one random history and returns how many
/// layout nodes the incremental window reused along the way.
fn run(seed: u64, steps: usize) -> (u64, usize) {
    let mut cx = TestAppContext::with_text_system(Arc::new(GlyphBoxTextSystem(NoopTextSystem)));
    let incremental = cx.add_window(|_, cx| OracleView::new(cx));
    let from_scratch = cx.add_window(|_, cx| OracleView::new(cx));
    // What scroll layers composite is checked against drawing from scratch
    // by the layer oracle; here the incremental window draws its lists.
    cx.update_window(incremental.into(), |_, window, _| {
        window.set_scroll_layers(false)
    })
    .unwrap();
    let mut rng = StdRng::seed_from_u64(seed);
    let mut history: Vec<Vec<Change>> = Vec::new();
    let mut reused = 0;
    let mut frames_reusing_subtrees = 0;

    for step in 0..steps {
        let changes: Vec<Change> = if step == 0 {
            Vec::new()
        } else {
            (0..rng.random_range(1..=3))
                .map(|_| Change::random(&mut rng))
                .collect()
        };
        for change in &changes {
            apply(&mut cx, incremental, change);
            apply(&mut cx, from_scratch, change);
        }
        history.push(changes);

        let (expected, reused_from_scratch, subtrees_from_scratch) =
            draw(&mut cx, from_scratch, true);
        let (actual, reused_incrementally, reused_subtrees) = draw(&mut cx, incremental, false);
        assert_eq!(
            reused_from_scratch, 0,
            "a window that forgot its layout nodes cannot have reused any"
        );
        assert!(
            !subtrees_from_scratch,
            "a refreshed window cannot draw anything from its last frame"
        );
        reused += reused_incrementally;
        frames_reusing_subtrees += reused_subtrees as usize;

        if actual != expected {
            let first = actual
                .iter()
                .zip(&expected)
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
            let history = history
                .iter()
                .enumerate()
                .map(|(step, changes)| format!("  {step}: {changes:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            panic!(
                "seed {seed}, step {step}: the incremental frame differs from the frame drawn \
                 from scratch at line {first} ({} lines against {})\n\
                 incremental:\n{}\nfrom scratch:\n{}\nchanges so far:\n{history}",
                actual.len(),
                expected.len(),
                excerpt(&actual),
                excerpt(&expected),
            );
        }
    }
    (reused, frames_reusing_subtrees)
}

#[test]
fn incremental_frames_match_frames_drawn_from_scratch() {
    let (reused, frames_reusing_subtrees) = (0..24)
        .map(|seed| run(seed, 60))
        .fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
    assert!(
        reused > 0,
        "the incremental window never reused a layout node, so nothing was compared"
    );
    assert!(
        frames_reusing_subtrees > 0,
        "the incremental window never drew a view again from its last frame"
    );
}
