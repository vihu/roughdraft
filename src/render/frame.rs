//! Frames: a plain rounded outline, the title above it, and the box that
//! clips the frame's children. Ports `FRAME_STYLE` (`constants.ts`), the
//! SVG export's frame `<rect>` and clip paths (`staticSvgScene.ts`,
//! `scene/export.ts`) and `addFrameLabelsAsTextElements` from Excalidraw
//! 0.18.1, MIT licensed, Copyright (c) 2020 Excalidraw.
use std::collections::HashMap;

use super::{Drawing, Item, Segment, text::left_block};
use crate::color::Rgba;
use crate::geometry::{self, Affine};
use crate::scene::{Element, Scene};

/// Corner radius of the outline and of the clip.
pub(crate) const FRAME_RADIUS: f64 = 8.0;

/// Outline color (`#bbb`).
const OUTLINE_COLOR: Rgba = Rgba::rgb(
    0xbb as f32 / 255.0,
    0xbb as f32 / 255.0,
    0xbb as f32 / 255.0,
);

/// Outline width.
const OUTLINE_WIDTH: f64 = 2.0;

/// Gap between the title's bottom and the frame's top.
const TITLE_OFFSET_Y: f64 = 3.0;

/// Title font: Helvetica, 14, line height 1.25.
const TITLE_FONT: (u32, f64, f64) = (2, 14.0, 1.25);

/// Title color in light mode (`nameColorLightTheme`).
const TITLE_COLOR: Rgba = Rgba::rgb(
    0x99 as f32 / 255.0,
    0x99 as f32 / 255.0,
    0x99 as f32 / 255.0,
);

/// Returns a frame's title drawing, a text element above its top-left
/// corner the way Excalidraw's export adds one; `None` for other elements.
///
/// The color is the light theme's; Excalidraw's dark export uses
/// `#7a7a7a` before its dark filter.
// ponytail: titles are not cut to the frame's width with "..."
// (`truncateText` measures Helvetica), and the editor draws them at a fixed
// 14 px on screen where this scales with the zoom.
pub fn frame_label(element: &Element) -> Option<Drawing> {
    let title = element.frame_title()?;
    let (family, size, line_height) = TITLE_FONT;
    let height = size * line_height;
    let top = element.base.y - TITLE_OFFSET_Y - height;
    Some(Drawing {
        transform: Affine::translate([element.base.x, top]),
        items: vec![Item::Text(left_block(
            title,
            family,
            size,
            line_height,
            TITLE_COLOR,
        ))],
    })
}

/// Returns the box of a frame's title in the frame's local coordinates
/// (`[x1, y1, x2, y2]`), where a click selects the frame; `None` for other
/// elements.
// ponytail: as wide as the frame, Excalidraw measures the title's DOM box
pub(crate) fn title_box(element: &Element) -> Option<[f64; 4]> {
    element.frame_title()?;
    let (_, size, line_height) = TITLE_FONT;
    let bottom = -TITLE_OFFSET_Y;
    Some([0.0, bottom - size * line_height, element.base.width, bottom])
}

/// Returns the live frames by id, for clipping their children.
pub fn frames(scene: &Scene) -> HashMap<&str, &Element> {
    scene
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted && e.frame_title().is_some())
        .map(|e| (e.base.id.as_str(), e))
        .collect()
}

/// A rounded rectangle from the origin as a path, corners as cubics.
pub(crate) fn rounded_rect([w, h]: [f64; 2], radius: f64) -> Vec<Segment> {
    /// Control point distance for a quarter circle.
    const KAPPA: f64 = 0.552_284_749_830_793_4;

    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = r * KAPPA;
    vec![
        Segment::MoveTo([r, 0.0]),
        Segment::LineTo([w - r, 0.0]),
        Segment::CubicTo([w - r + k, 0.0], [w, r - k], [w, r]),
        Segment::LineTo([w, h - r]),
        Segment::CubicTo([w, h - r + k], [w - r + k, h], [w - r, h]),
        Segment::LineTo([r, h]),
        Segment::CubicTo([r - k, h], [0.0, h - r + k], [0.0, h - r]),
        Segment::LineTo([0.0, r]),
        Segment::CubicTo([0.0, r - k], [r - k, 0.0], [r, 0.0]),
    ]
}

/// A frame's outline: a plain rounded rectangle, not rough.
pub(super) fn outline(element: &Element) -> Drawing {
    Drawing {
        transform: geometry::element_transform(element),
        items: vec![Item::Frame {
            size: [element.base.width, element.base.height],
            radius: FRAME_RADIUS,
            color: OUTLINE_COLOR,
            width: OUTLINE_WIDTH,
        }],
    }
}
