// Modified by Longbridge for gpui-fast.
//! Paths drawn from kept triangles. See [`crate::fast::path_cache`].

use crate::{PathBuilder, Pixels, point, px};

fn sparkline(at: (f32, f32)) -> PathBuilder {
    let mut builder = PathBuilder::stroke(px(1.5));
    builder.move_to(point(px(at.0), px(at.1)));
    for step in 1..20 {
        let x = at.0 + step as f32 * 3.;
        let y = at.1 + ((step * 7) % 11) as f32;
        builder.line_to(point(px(x), px(y)));
    }
    builder
}

fn positions(path: &crate::Path<Pixels>) -> Vec<(f32, f32)> {
    path.vertices
        .iter()
        .map(|vertex| (vertex.xy_position.x.0, vertex.xy_position.y.0))
        .collect()
}

/// A shape built again gives the same triangles, whether they were kept or
/// not.
#[test]
fn a_path_built_again_is_the_same() {
    let first = sparkline((10., 20.)).build().unwrap();
    let again = sparkline((10., 20.)).build().unwrap();
    assert_eq!(positions(&first), positions(&again));
    assert_eq!(first.bounds, again.bounds);
    assert!(!first.vertices.is_empty());
}

/// The same shape built somewhere else is the same triangles moved there.
#[test]
fn a_path_of_the_same_shape_elsewhere_is_moved() {
    let here = sparkline((0., 0.)).build().unwrap();
    let there = sparkline((100., 250.)).build().unwrap();
    let moved: Vec<(f32, f32)> = positions(&here)
        .into_iter()
        .map(|(x, y)| (x + 100., y + 250.))
        .collect();
    assert_eq!(positions(&there), moved);
    assert_eq!(
        there.bounds.origin,
        here.bounds.origin + point(px(100.), px(250.))
    );
    assert_eq!(there.bounds.size, here.bounds.size);
}

/// A different shape, or the same shape stroked differently, is not taken
/// for a kept one.
#[test]
fn a_different_shape_or_stroke_is_tessellated_again() {
    let thin = sparkline((0., 0.)).build().unwrap();
    let mut wide_builder = PathBuilder::stroke(px(4.));
    wide_builder.move_to(point(px(0.), px(0.)));
    for step in 1..20 {
        wide_builder.line_to(point(px(step as f32 * 3.), px(((step * 7) % 11) as f32)));
    }
    let wide = wide_builder.build().unwrap();
    assert_ne!(positions(&thin), positions(&wide));
    assert!(wide.bounds.size.height > thin.bounds.size.height);
}
