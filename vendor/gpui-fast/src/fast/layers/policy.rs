// Modified by Longbridge for gpui-fast.
//! Which scroll containers get a layer: eligibility, promotion, demotion
//! and drop (M4).
//!
//! A scroll container is looked at every frame it prepaints its children.
//! It gets a layer once it has scrolled on two frames in a row (a container
//! that never scrolls never pays for one); from then on the layer is
//! composited on frames that only scrolled it and painted again on the
//! others. A layer is demoted — dropped, the container kept on today's path
//! until its content has been stable for a while — when its content keeps
//! changing, when it paints paths, or when its visible tiles alone exceed
//! the budget. Content holding what a layer cannot composite otherwise
//! (spec §6.5) keeps the container on today's path only while it does: the
//! layer is dropped and painted again a few frames later, to see whether it
//! still does. Layers are dropped when the window is resized or rescaled,
//! and when they have not been composited for long.

use crate::fast::layers::invalidate::OwnerWatch;
use crate::fast::layers::record::LayerRecord;
use crate::fast::layers::work::WorkBudget;
use crate::fast::layers::{Layer, WindowLayers, input, invalidate, lists, scene::LayerKey};
use crate::{
    App, Bounds, ContentMask, EntityId, GlobalElementId, Pixels, Point, Size, TextStyle, Window,
};

/// What a scroll container does with its content this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Today's path: the content is prepainted and painted into the frame.
    Bypass,
    /// The content is painted into the container's layer, whose tiles are
    /// composited.
    Repaint,
    /// The content is left as the layer holds it; its tiles are composited
    /// at the new offset.
    Composite,
}

/// How many frames in a row a container must scroll on to get a layer.
const PROMOTE_AFTER_SCROLLED_FRAMES: u8 = 2;
/// A layer whose content changed on at least this many of the last 16
/// frames is demoted. Include the halfway boundary at 120 Hz.
const DEMOTE_AFTER_CHANGED_FRAMES: u32 = 8;
/// Uniform lists rebuild about five viewports on content changes, so their
/// frequency limit is lower even before the work budget is exhausted. A
/// `list` rebuilds only the rows it shows, as it would without a layer.
const LIST_DEMOTE_AFTER_CHANGED_FRAMES: u32 = 4;
/// How many frames a demoted container's content must be stable for before
/// it may get a layer again.
const REPROMOTE_AFTER_STABLE_FRAMES: u64 = 60;
/// Repeatedly unprofitable caches wait longer, up to this many doublings.
const MAX_COOLDOWN_DOUBLINGS: u8 = 5;
/// How many frames a container whose content held what a layer cannot
/// composite waits before its content is painted into a layer again.
const RETRY_AFTER_INELIGIBLE_FRAMES: u64 = 8;
/// A layer not composited for this many frames is dropped.
const DROP_AFTER_FRAMES: u64 = 120;
/// The tile textures a window's layers may use, in bytes.
const TILE_BUDGET_BYTES: u64 = 64 << 20;
/// The side of a tile, in device pixels (spec §3).
const TILE_SIZE: u64 = 512;
/// Layer keys stay below this, as tile texture ids require.
const LAYER_KEY_LIMIT: u32 = 0x0100_0000;

/// A scroll container's standing with its layer.
#[derive(Default)]
pub(crate) struct LayerPolicy {
    /// How many frames in a row, up to the last one looked at, the container
    /// scrolled on.
    pub(crate) scrolled_streak: u8,
    /// Whether the content changed, one bit per frame, the last frame looked
    /// at in the lowest bit.
    pub(crate) change_history: u16,
    /// The last frame the content was seen to change on, while demoted.
    pub(crate) stable_since: u64,
    /// While demoted, the first frame the container may get a layer again.
    pub(crate) demoted_until: Option<u64>,
    /// After its content held what a layer cannot composite, the first frame
    /// the container may get a layer again.
    retry_at: Option<u64>,
    /// The last frame the container scrolled on.
    last_scrolled_frame: Option<u64>,
    /// The last frame the container was looked at.
    last_seen_frame: u64,
    /// The offset the container had then.
    last_offset: Option<Point<Pixels>>,
    /// Where the content in the layer was painted and what it inherited.
    painted_in: Option<LayerContext>,
    /// What was decided the last time the container was looked at.
    last_decision: Option<Decision>,
    /// Counts the notifications of the view holding the container.
    owner: Option<OwnerWatch>,
    /// How often that view had been notified when the container was last
    /// looked at.
    owner_notifies_at: u64,
    /// Whether the content, as last painted into the layer, prepainted an
    /// anchored element.
    content_anchored: bool,
    /// Recent rebuilding work, relative to drawing the visible content.
    work: WorkBudget,
    /// Backoff survives dropping the record and its cached rows.
    cooldown_doublings: u8,
    cooldown_frames: u64,
    last_content_change: u64,
}

impl LayerPolicy {
    /// How often `owner` was notified since the container was last looked
    /// at, if it held the container then.
    pub(crate) fn owner_notifies_since(&self, owner: EntityId) -> Option<u64> {
        let watch = self.owner.as_ref().filter(|watch| watch.view() == owner)?;
        Some(watch.notifies().saturating_sub(self.owner_notifies_at))
    }
}

/// What a container's content is painted with besides what it reads: where
/// the container is and what it inherits. A layer painted in one context is
/// not composited in another.
#[derive(Clone, PartialEq)]
struct LayerContext {
    bounds: Bounds<Pixels>,
    content_size: Size<Pixels>,
    content_mask: ContentMask<Pixels>,
    text_style: TextStyle,
    opacity: f32,
    rem_size: Pixels,
    scale_factor: f32,
}

impl LayerContext {
    fn current(window: &Window, bounds: Bounds<Pixels>, content_size: Size<Pixels>) -> Self {
        LayerContext {
            bounds,
            content_size,
            content_mask: window.content_mask(),
            text_style: window.text_style(),
            opacity: window.element_opacity,
            rem_size: window.rem_size(),
            scale_factor: window.scale_factor(),
        }
    }
}

/// Decides what the scroll container `id`, prepainted at `bounds` with
/// children spanning `content_size` and scrolled by `scroll_offset`, does
/// with its children this frame, and keeps its standing up to date.
///
/// Called where the container prepaints its children, before it does.
///
/// A container is given an entry in `layers` the first frame it scrolls, to
/// count its scrolls towards promotion; it has a layer only once the entry
/// holds a record.
pub(crate) fn decide(
    window: &mut Window,
    cx: &mut App,
    id: &GlobalElementId,
    bounds: Bounds<Pixels>,
    content_size: Size<Pixels>,
    scroll_offset: Point<Pixels>,
) -> Decision {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(decision) = window.fast_layers.forced_decision {
        return decision;
    }
    window.fast_layers.scrolls.take_offsets_set();
    drop_layers_on_resize(window);
    // A scroll container inside a layer is painted into it (spec §6.6).
    if !super::active(window, cx) || window.fast_layers.painting.is_some() {
        return Decision::Bypass;
    }
    let frame = window.fast_layers.frame;
    let noted = invalidate::scrolled(window, id);
    if !window.fast_layers.layers.contains_key(id) {
        if !noted {
            return Decision::Bypass;
        }
        let key = LayerKey(window.fast_layers.next_key);
        window.fast_layers.next_key = (window.fast_layers.next_key + 1) % LAYER_KEY_LIMIT;
        window
            .fast_layers
            .layers
            .insert(id.clone(), new_layer(key, frame));
    }
    let context = LayerContext::current(window, bounds, content_size);
    let layer = &window.fast_layers.layers[id];
    let policy = &layer.policy;
    // A wheel moves the offset of every `div` under the pointer, and one that
    // cannot scroll that far puts it back when it is prepainted: it only
    // scrolled if its offset moved. A list clamps a wheel's scroll itself.
    let scrolled =
        noted && (invalidate::is_list(window, id) || policy.last_offset != Some(scroll_offset));

    let streak = if !scrolled {
        0
    } else if policy
        .last_scrolled_frame
        .is_some_and(|last| last + 1 == frame)
    {
        policy.scrolled_streak.saturating_add(1)
    } else {
        1
    };
    let mut demoted_until = policy.demoted_until;
    let changed_while_demoted =
        demoted_until.is_some() && invalidate::changed_without_layer(window, cx, id);
    if changed_while_demoted {
        // The cooldown runs from the demotion; a change while it runs only
        // asks the content to stay as it is a while before the layer is
        // tried again, so that one changed now and then, as a view notified
        // every few seconds, is not kept off its layer for good.
        demoted_until = demoted_until.map(|until| until.max(frame + REPROMOTE_AFTER_STABLE_FRAMES));
    }
    if demoted_until.is_some_and(|until| frame >= until) {
        demoted_until = None;
    }

    // Whether the content changed this frame, whether the layer is demoted
    // for it, and whether it is dropped for holding what it cannot
    // composite.
    let mut changed = false;
    let mut rerendered = false;
    let mut demote = layer.record.is_some() && policy.work.over_budget();
    let mut ineligible = false;
    let decision = match &layer.record {
        _ if demoted_until.is_some() => Decision::Bypass,
        Some(record) if !fits(window, record) => {
            demote = true;
            Decision::Bypass
        }
        Some(record) if !eligible(window, id, policy, record) => {
            ineligible = true;
            Decision::Bypass
        }
        Some(record)
            if policy.painted_in.as_ref() == Some(&context)
                && invalidate::scroll_only(window, cx, id, record) =>
        {
            // Only scrolled; painted again if the scroll exposes what was
            // not painted, which is not a change of the content. A list's
            // layer is extended by the rows it exposes instead.
            if layer.rows.list || covers(record, bounds, content_size, scroll_offset) {
                Decision::Composite
            } else {
                Decision::Repaint
            }
        }
        Some(record)
            if policy.painted_in.as_ref() == Some(&context)
                && layer.rows.repaints_shown_rows
                && !layer.rows.items_changed
                && invalidate::owner_rerendered_only(window, cx, id, record) =>
        {
            // Only the view holding a `list` renders again: the rows it shows
            // are rendered again, and the rows the layer holds besides as
            // they come to show (see `lists::LayerRows::rerendered`).
            rerendered = true;
            Decision::Composite
        }
        Some(_) => {
            changed = true;
            Decision::Repaint
        }
        None if streak >= PROMOTE_AFTER_SCROLLED_FRAMES
            && policy.retry_at.is_none_or(|at| frame >= at) =>
        {
            Decision::Repaint
        }
        None => Decision::Bypass,
    };
    let elapsed = frame.saturating_sub(policy.last_seen_frame).min(16) as u32;
    let mut history = policy
        .change_history
        .checked_shl(elapsed)
        .unwrap_or_default();
    if changed {
        history |= 1;
        let changed_limit = if layer.rows.list && !layer.rows.repaints_shown_rows {
            LIST_DEMOTE_AFTER_CHANGED_FRAMES
        } else {
            DEMOTE_AFTER_CHANGED_FRAMES
        };
        demote |= history.count_ones() >= changed_limit;
    }
    let decision = if demote { Decision::Bypass } else { decision };

    let starting_cache = decision == Decision::Repaint && layer.record.is_none();
    let owner = invalidate::owner_view(window);
    let layer = window.fast_layers.layers.get_mut(id).unwrap();
    layer.rows.rerendered = rerendered && decision == Decision::Composite;
    let policy = &mut layer.policy;
    if policy.owner.as_ref().map(OwnerWatch::view) != owner {
        policy.owner = owner.map(OwnerWatch::new);
    }
    policy.owner_notifies_at = policy.owner.as_ref().map_or(0, OwnerWatch::notifies);
    policy.scrolled_streak = streak;
    if scrolled {
        policy.last_scrolled_frame = Some(frame);
    }
    policy.last_seen_frame = frame;
    policy.last_offset = Some(scroll_offset);
    policy.last_decision = Some(decision);
    policy.change_history = history;
    policy.demoted_until = demoted_until;
    if starting_cache {
        policy.work = WorkBudget::default();
    }
    if frame.saturating_sub(policy.last_content_change)
        >= REPROMOTE_AFTER_STABLE_FRAMES << MAX_COOLDOWN_DOUBLINGS
    {
        policy.cooldown_doublings = 0;
    }
    if changed || changed_while_demoted {
        policy.last_content_change = frame;
    }
    if demote {
        layer.record = None;
        layer.rows = lists::LayerRows::default();
        policy.work = WorkBudget::default();
        policy.painted_in = None;
        policy.change_history = 0;
        policy.scrolled_streak = 0;
        policy.stable_since = frame;
        policy.cooldown_frames = REPROMOTE_AFTER_STABLE_FRAMES << policy.cooldown_doublings;
        policy.cooldown_doublings = (policy.cooldown_doublings + 1).min(MAX_COOLDOWN_DOUBLINGS);
        policy.demoted_until = Some(frame + policy.cooldown_frames);
        if let Some(engine) = window.layout_engine.as_mut() {
            engine.retention.stats.layers_demoted += 1;
        }
        return decision;
    }
    if changed_while_demoted {
        policy.stable_since = frame;
    }
    if ineligible {
        layer.record = None;
        policy.painted_in = None;
        policy.retry_at = Some(frame + RETRY_AFTER_INELIGIBLE_FRAMES);
        return decision;
    }
    match decision {
        Decision::Repaint => {
            policy.painted_in = Some(context);
            layer.last_composited_frame = frame;
        }
        Decision::Composite => layer.last_composited_frame = frame,
        Decision::Bypass => {}
    }
    decision
}

/// Records work completed for the layer in this frame. One unit is the
/// work of drawing its visible content directly; overscan and rows rebuilt
/// for hover count too. The next prepaint can fall back before doing more.
/// Notes that the layer of `id`, painted afresh this frame for a change of
/// its content, came out as it was: the frame does not count as one its
/// content changed on (see [`DEMOTE_AFTER_CHANGED_FRAMES`]).
pub(crate) fn note_unchanged_repaint(window: &mut Window, id: &GlobalElementId) {
    let frame = window.fast_layers.frame;
    if let Some(layer) = window.fast_layers.layers.get_mut(id)
        && layer.policy.last_seen_frame == frame
    {
        layer.policy.change_history &= !1;
    }
}

pub(crate) fn note_work(window: &mut Window, id: &GlobalElementId, work: f32) {
    let frame = window.fast_layers.frame;
    if let Some(layer) = window.fast_layers.layers.get_mut(id) {
        layer.policy.work.note(frame, work);
        if layer.policy.last_seen_frame == frame
            && layer.policy.last_decision == Some(Decision::Repaint)
            && layer.policy.last_content_change == frame
        {
            layer.policy.work.note_refresh(frame, work);
        }
    }
}

/// Whether the content `record` holds can ever be composited from a layer:
/// it painted no path, and the tiles covering its viewport fit the budget.
/// A layer whose content does not is demoted.
fn fits(window: &Window, record: &LayerRecord) -> bool {
    let scale_factor = window.scale_factor();
    // A viewport straddles one more tile than it spans on each axis.
    let tiles = |extent: Pixels| (extent.0 * scale_factor / TILE_SIZE as f32).ceil() as u64 + 1;
    let viewport = record.viewport.size;
    let tile_bytes = TILE_SIZE * TILE_SIZE * 4;
    !record.has_paths
        && tiles(viewport.width) * tiles(viewport.height) * tile_bytes <= TILE_BUDGET_BYTES
}

/// Whether the content `record` holds of the scroll container `id` can be
/// composited from a layer this frame (spec §5.6, §6.5): it deferred no
/// draws (anchored popovers), placed no anchored element, handles no text
/// input (a focused input is inside), and no view in it asked for an
/// animation frame.
fn eligible(
    window: &Window,
    id: &GlobalElementId,
    policy: &LayerPolicy,
    record: &LayerRecord,
) -> bool {
    let prepaint = &record.prepaint_range;
    let paint = &record.paint_range;
    prepaint.start.deferred_draws_index == prepaint.end.deferred_draws_index
        && paint.start.input_handlers_index == paint.end.input_handlers_index
        && !policy.content_anchored
        && !invalidate::animation_frame_requested(window, id)
}

/// Drops every layer when the window's size or scale factor changed since
/// they were painted: their tiles no longer fit it.
pub(crate) fn drop_layers_on_resize(window: &mut Window) {
    let size = (window.viewport_size(), window.scale_factor());
    let layers = &mut window.fast_layers;
    if layers.window_size != Some(size) {
        layers.layers.clear();
        layers.window_size = Some(size);
    }
}

/// Ends the frame being drawn for the layers painted in it: notes whether
/// their content placed an anchored element.
pub(crate) fn finish_frame(layers: &mut WindowLayers) {
    let frame = layers.frame;
    let anchored = &layers.scrolls.anchored;
    for (id, layer) in &mut layers.layers {
        let policy = &mut layer.policy;
        if layer.record.is_some() && policy.last_seen_frame == frame {
            // An element without an id of its own has the id of the
            // nearest one around it with one: the container's, when it is
            // directly inside it.
            let found = anchored.iter().any(|element| element.starts_with(id));
            match policy.last_decision {
                Some(Decision::Repaint) => policy.content_anchored = found,
                // A list's layer adds the rows a scroll uncovers.
                Some(Decision::Composite) => policy.content_anchored |= found,
                _ => {}
            }
        }
    }
}

/// Whether the layer is kept at the end of `frame`: it was composited
/// lately, or it is demoted and remembers until when. Keep the lightweight
/// policy briefly after cooldown too, so re-promotion does not lose backoff.
pub(crate) fn keep(layer: &Layer, frame: u64) -> bool {
    frame < layer.last_composited_frame + DROP_AFTER_FRAMES
        || (layer.policy.cooldown_doublings > 0
            && frame
                < layer
                    .policy
                    .last_seen_frame
                    .saturating_add(DROP_AFTER_FRAMES))
        || layer
            .policy
            .demoted_until
            .is_some_and(|until| frame < until.saturating_add(DROP_AFTER_FRAMES))
}

/// Whether the part of the content `record` painted still covers the
/// viewport at `bounds`, scrolled by `scroll_offset`, with a margin of a
/// quarter of the overscan — a quarter of the viewport's extent — around
/// it, as far as the content reaches (spec §5.4). The margin is kept only on
/// the axes overscan was painted on, those the container scrolls on.
fn covers(
    record: &LayerRecord,
    bounds: Bounds<Pixels>,
    content_size: Size<Pixels>,
    scroll_offset: Point<Pixels>,
) -> bool {
    let translation = scroll_offset - record.scroll_offset;
    let painted = Bounds {
        origin: record.painted_region.origin + translation,
        size: record.painted_region.size,
    };
    let margin = |painted: Pixels, viewport: Pixels| {
        if painted > viewport {
            viewport / 4.
        } else {
            Pixels::ZERO
        }
    };
    let margin = Point {
        x: margin(record.painted_region.size.width, record.viewport.size.width),
        y: margin(
            record.painted_region.size.height,
            record.viewport.size.height,
        ),
    };
    let viewport = record.viewport;
    let wanted = Bounds::from_corners(viewport.origin - margin, viewport.bottom_right() + margin);
    let content = Bounds {
        origin: bounds.origin + scroll_offset,
        size: content_size,
    };
    let wanted = wanted.intersect(&content);
    if wanted.size.width <= Pixels::ZERO || wanted.size.height <= Pixels::ZERO {
        return true;
    }
    wanted.origin.x >= painted.origin.x
        && wanted.origin.y >= painted.origin.y
        && wanted.right() <= painted.right()
        && wanted.bottom() <= painted.bottom()
}

/// Keeps the container `id`, whose background could not be baked into its
/// tiles, on today's path for a while: a background that cannot be baked
/// (a translucent window, a gradient) seldom becomes one that can, and
/// painting a layer every scrolled frame that cannot be composited costs a
/// repaint for nothing.
pub(crate) fn defer_unbaked(window: &mut Window, id: &GlobalElementId) {
    let frame = window.fast_layers.frame;
    if let Some(layer) = window.fast_layers.layers.get_mut(id) {
        layer.record = None;
        layer.policy.painted_in = None;
        layer.policy.retry_at = Some(frame + REPROMOTE_AFTER_STABLE_FRAMES);
    }
}

fn new_layer(key: LayerKey, frame: u64) -> Layer {
    Layer {
        key,
        generation: 0,
        record: None,
        policy: LayerPolicy {
            last_seen_frame: frame,
            ..LayerPolicy::default()
        },
        input: input::LayerInput::default(),
        rows: lists::LayerRows::default(),
        last_composited_frame: frame,
        prepainted: None,
    }
}

/// What the scroll container `id` decided in the last frame drawn, if it
/// was looked at in it.
#[cfg(test)]
pub(crate) fn last_decision(window: &Window, id: &GlobalElementId) -> Option<Decision> {
    let policy = &window.fast_layers.layers.get(id)?.policy;
    (policy.last_seen_frame + 1 == window.fast_layers.frame)
        .then_some(policy.last_decision)
        .flatten()
}
