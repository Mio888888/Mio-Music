// Modified by Longbridge for gpui-fast.
//! Tessellated paths kept from one build to the next.
//!
//! A path is tessellated into triangles every time it is built, which for a
//! sparkline in every row of a table, a chart's moving averages or a donut is
//! a large part of what painting them costs, though most of them are the same
//! shape as the frame before, or the same shape moved, as a table scrolls.
//!
//! [`build`] tessellates a path relative to its first point and keeps the
//! triangles, keyed by that relative path and by how it is stroked or filled;
//! a path of the same shape built again anywhere reuses them, moved to its
//! first point. Every path is tessellated that way, kept or not, so a path
//! drawn from kept triangles is exactly the path tessellating it again gives.

use std::{cell::RefCell, rc::Rc};

use anyhow::Error;
use collections::FxHashMap;
use lyon::path::PathEvent;

use crate::{Bounds, Path, PathBuilder, PathStyle, Pixels, Point, point, px};

/// How many shapes are kept before the least recently used half is let go.
const KEPT: usize = 2048;

thread_local! {
    static KEPT_PATHS: RefCell<KeptPaths> = RefCell::new(KeptPaths::default());
}

/// Tessellated shapes, in two generations: those built since the last turn
/// over, and those built before it, which are dropped at the next.
#[derive(Default)]
struct KeptPaths {
    recent: FxHashMap<Vec<u32>, Rc<Path<Pixels>>>,
    older: FxHashMap<Vec<u32>, Rc<Path<Pixels>>>,
}

impl KeptPaths {
    fn get(&mut self, key: &[u32]) -> Option<Rc<Path<Pixels>>> {
        if let Some(path) = self.recent.get(key) {
            return Some(path.clone());
        }
        let (key, path) = self.older.remove_entry(key)?;
        self.insert(key, path.clone());
        Some(path)
    }

    fn insert(&mut self, key: Vec<u32>, path: Rc<Path<Pixels>>) {
        if self.recent.len() >= KEPT / 2 {
            self.older = std::mem::take(&mut self.recent);
        }
        self.recent.insert(key, path);
    }
}

/// Builds `builder` as [`PathBuilder::build`] does, from triangles kept from
/// an earlier path of the same shape where there is one.
#[inline]
pub(crate) fn build(builder: PathBuilder) -> Result<Path<Pixels>, Error> {
    let path = match builder.transform {
        Some(transform) => builder.raw.build().transformed(&transform),
        None => builder.raw.build(),
    };
    let Some(origin) = path.iter().find_map(|event| match event {
        PathEvent::Begin { at } => Some(at),
        _ => None,
    }) else {
        return tessellate(&builder.style, builder.dash_array, &path);
    };
    let relative = path.transformed(&lyon::math::Transform::translation(-origin.x, -origin.y));
    let offset = point(px(origin.x), px(origin.y));

    let key = shape_key(&relative, &builder.style, builder.dash_array.as_deref());
    if let Some(kept) = KEPT_PATHS.with_borrow_mut(|kept| kept.get(&key)) {
        return Ok(moved(&kept, offset));
    }
    let tessellated = tessellate(&builder.style, builder.dash_array, &relative)?;
    let tessellated = Rc::new(tessellated);
    KEPT_PATHS.with_borrow_mut(|kept| kept.insert(key, tessellated.clone()));
    Ok(moved(&tessellated, offset))
}

fn tessellate(
    style: &PathStyle,
    dash_array: Option<Vec<Pixels>>,
    path: &lyon::path::Path,
) -> Result<Path<Pixels>, Error> {
    match style {
        PathStyle::Stroke(options) => PathBuilder::tessellate_stroke(dash_array, path, options),
        PathStyle::Fill(options) => PathBuilder::tessellate_fill(path, options),
    }
}

/// `path`, tessellated at the origin, moved by `offset`, as tessellating it
/// with `build_path` there gives it: starting at its first vertex.
fn moved(path: &Path<Pixels>, offset: Point<Pixels>) -> Path<Pixels> {
    let start = path
        .vertices
        .first()
        .map_or(offset, |vertex| vertex.xy_position + offset);
    let mut moved = Path::new(start);
    moved.vertices = path
        .vertices
        .iter()
        .map(|vertex| {
            let mut vertex = vertex.clone();
            vertex.xy_position = vertex.xy_position + offset;
            vertex
        })
        .collect();
    moved.bounds = Bounds {
        origin: path.bounds.origin + offset,
        size: path.bounds.size,
    };
    moved
}

/// The key a shape is kept by: its events at the origin, and how it is
/// stroked or filled, as bits, so that only the same shape matches.
fn shape_key(
    path: &lyon::path::Path,
    style: &PathStyle,
    dash_array: Option<&[Pixels]>,
) -> Vec<u32> {
    let mut key = Vec::with_capacity(64);
    let mut point = |key: &mut Vec<u32>, point: lyon::math::Point| {
        key.push(point.x.to_bits());
        key.push(point.y.to_bits());
    };
    for event in path.iter() {
        match event {
            PathEvent::Begin { at } => {
                key.push(0);
                point(&mut key, at);
            }
            PathEvent::Line { to, .. } => {
                key.push(1);
                point(&mut key, to);
            }
            PathEvent::Quadratic { ctrl, to, .. } => {
                key.push(2);
                point(&mut key, ctrl);
                point(&mut key, to);
            }
            PathEvent::Cubic {
                ctrl1, ctrl2, to, ..
            } => {
                key.push(3);
                point(&mut key, ctrl1);
                point(&mut key, ctrl2);
                point(&mut key, to);
            }
            PathEvent::End { close, .. } => key.push(4 + close as u32),
        }
    }
    // The options are small and seldom vary; their debug form covers every
    // field without listing them.
    let options = match style {
        PathStyle::Stroke(options) => format!("s{options:?}"),
        PathStyle::Fill(options) => format!("f{options:?}"),
    };
    key.push(u32::MAX);
    key.extend(options.bytes().map(u32::from));
    if let Some(dash_array) = dash_array {
        key.push(u32::MAX - 1);
        key.extend(dash_array.iter().map(|dash| dash.0.to_bits()));
    }
    key
}
