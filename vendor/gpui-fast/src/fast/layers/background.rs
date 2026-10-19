// Modified by Longbridge for gpui-fast.
//! Baking the solid background under a layer's viewport into its tiles (M3).
//!
//! Subpixel text blends with what it is drawn over, so a tile cannot be
//! transparent: it is cleared with the colour the frame has under the
//! viewport, which it can only know when that is one opaque, solid quad
//! covering the whole viewport (spec §5.2).

use crate::{
    Bounds, Hsla, Quad, Rgba, ScaledPixels, Scene,
    color::BackgroundTag,
    fast::layers::scene::visible_bounds,
    scene::{DrawOrder, PaintOperation, Primitive},
};

/// The colour of what `scene`, as painted so far, shows under `viewport`,
/// if a tile can be cleared with it: the topmost primitive visible in the
/// viewport is a quad with a solid, opaque background, no visible border or
/// rounded corner inside the viewport, covering all of it, and the window is
/// opaque.
pub(crate) fn bake(
    scene: &Scene,
    viewport: Bounds<ScaledPixels>,
    window_opaque: bool,
) -> Option<Rgba> {
    if !window_opaque {
        return None;
    }
    bake_operations(&scene.paint_operations, viewport)
}

/// [`bake`] over `operations`, the ones painted under the viewport.
pub(crate) fn bake_operations(
    operations: &[PaintOperation],
    viewport: Bounds<ScaledPixels>,
) -> Option<Rgba> {
    // The primitive drawn last over the viewport, and how many share its
    // draw order, whose drawing order among themselves is by kind.
    let mut top: Option<(DrawOrder, &Primitive)> = None;
    let mut sharing_top = 0;
    for operation in operations {
        let PaintOperation::Primitive(primitive) = operation else {
            continue;
        };
        let visible = visible_bounds(primitive);
        if !visible.intersects(&viewport) {
            continue;
        }
        let order = draw_order(primitive);
        match top {
            Some((top_order, _)) if order < top_order => {}
            Some((top_order, _)) if order == top_order => sharing_top += 1,
            _ => {
                top = Some((order, primitive));
                sharing_top = 1;
            }
        }
    }
    let (_, Primitive::Quad(quad)) = top? else {
        return None;
    };
    if sharing_top != 1 || !covers(quad, viewport) {
        return None;
    }
    Some(quad.background.solid.into())
}

fn covers(quad: &Quad, viewport: Bounds<ScaledPixels>) -> bool {
    let background = &quad.background;
    if background.tag != BackgroundTag::Solid || background.solid.a < 1. {
        return false;
    }
    let visible = quad.bounds.intersect(&quad.content_mask.bounds);
    if !contains(&visible, &viewport) {
        return false;
    }
    if visible_border(quad) {
        let widths = &quad.border_widths;
        let inner = Bounds::from_corners(
            quad.bounds.origin + crate::point(widths.left, widths.top),
            quad.bounds.bottom_right() - crate::point(widths.right, widths.bottom),
        );
        if !contains(&inner, &viewport) {
            return false;
        }
    }
    let radii = &quad.corner_radii;
    let (min, max) = (quad.bounds.origin, quad.bounds.bottom_right());
    let corners = [
        (radii.top_left, min.x, min.y),
        (radii.top_right, max.x - radii.top_right, min.y),
        (
            radii.bottom_right,
            max.x - radii.bottom_right,
            max.y - radii.bottom_right,
        ),
        (radii.bottom_left, min.x, max.y - radii.bottom_left),
    ];
    corners.into_iter().all(|(radius, x, y)| {
        radius.0 <= 0.
            || !Bounds {
                origin: crate::point(x, y),
                size: crate::size(radius, radius),
            }
            .intersects(&viewport)
    })
}

fn visible_border(quad: &Quad) -> bool {
    let widths = &quad.border_widths;
    let color: Hsla = quad.border_color;
    color.a > 0.
        && (widths.top.0 > 0. || widths.right.0 > 0. || widths.bottom.0 > 0. || widths.left.0 > 0.)
}

fn contains(outer: &Bounds<ScaledPixels>, inner: &Bounds<ScaledPixels>) -> bool {
    let (outer_max, inner_max) = (outer.bottom_right(), inner.bottom_right());
    outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer_max.x >= inner_max.x
        && outer_max.y >= inner_max.y
}

fn draw_order(primitive: &Primitive) -> DrawOrder {
    match primitive {
        Primitive::Shadow(shadow) => shadow.order,
        Primitive::Quad(quad) => quad.order,
        Primitive::Path(path) => path.order,
        Primitive::Underline(underline) => underline.order,
        Primitive::MonochromeSprite(sprite) => sprite.order,
        Primitive::SubpixelSprite(sprite) => sprite.order,
        Primitive::PolychromeSprite(sprite) => sprite.order,
        Primitive::Surface(surface) => surface.order,
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Primitive::Backdrop(backdrop) => backdrop.order,
    }
}
