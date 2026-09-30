//! Scene to renderer-agnostic drawings, the way Excalidraw 0.18 draws them.
//!
//! Ports rough options per element type (`scene/Shape.ts`), arrowhead geometry
//! (`element/bounds.ts`), and draw order, transforms and text layout
//! (`renderer/staticSvgScene.ts`) from Excalidraw 0.18.1, MIT licensed,
//! Copyright (c) 2020 Excalidraw.
use std::collections::{HashMap, HashSet};

use rough_rs::{Drawable, Generator, Op, OpSetType, OpType, Options, ShapeType};

use self::linear::{curve_bounds, curve_ops, linear, points_bounds};
use self::shapes::{diamond, rectangle};
use self::text::text_block;

use crate::color::Rgba;
use crate::geometry::{Affine, Point};
use crate::scene::{Element, FillStyle, Kind, Roundness, Scene, StrokeStyle};

pub(crate) use self::segment::{segment_length, segment_midpoint};

mod freedraw;
mod linear;
mod segment;
mod shapes;
mod text;

/// One element, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    /// Maps element-local points to scene points.
    pub transform: Affine,
    /// What to draw, bottom first, in element-local units.
    pub items: Vec<Item>,
}

/// One stroke, fill or block of text.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Outline along a path, with round caps and joins.
    Stroke {
        /// The path to outline.
        path: Vec<Segment>,
        /// Color, element opacity included.
        color: Rgba,
        /// Line width.
        width: f64,
        /// Alternating dash and gap lengths; `None` is a solid line.
        dash: Option<Vec<f64>>,
    },
    /// Filled path.
    Fill {
        /// The path to fill.
        path: Vec<Segment>,
        /// Color, element opacity included.
        color: Rgba,
        /// Which regions of a self-intersecting path count as inside.
        rule: FillRule,
    },
    /// Lines of text.
    Text(TextBlock),
    /// A picture filling the element's box.
    Image {
        /// Key into the scene's `files`.
        file_id: String,
        /// Box size.
        size: [f64; 2],
        /// Element opacity, from 0 to 1.
        opacity: f32,
        /// The part of the file shown, or the whole file.
        crop: Option<Crop>,
        /// Mirrored horizontally and vertically about the box centre.
        flip: [bool; 2],
    },
}

/// The part of an image file an image element shows (`crop`).
#[derive(Clone, Debug, PartialEq)]
pub struct Crop {
    /// Left, top, width and height of the shown part, in file pixels.
    pub rect: [f64; 4],
    /// The file's size the rectangle refers to.
    pub natural: [f64; 2],
}

/// One path command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Starts a new subpath.
    MoveTo(Point),
    /// Straight line to a point.
    LineTo(Point),
    /// Cubic Bézier through two control points to a point.
    CubicTo(Point, Point, Point),
}

/// Fill rule for self-intersecting paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    /// Inside when the winding number is not zero.
    NonZero,
    /// Inside when a ray crosses the path an odd number of times.
    EvenOdd,
}

/// Lines of text, laid out the way Excalidraw lays them out.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    /// Lines, top to bottom, drawn as stored (already wrapped).
    pub lines: Vec<String>,
    /// Horizontal anchor of every line; see `align`.
    pub x: f64,
    /// Distance between the tops of consecutive lines.
    pub line_height: f64,
    /// Distance from a line's top to its alphabetic baseline.
    pub baseline: f64,
    /// Which part of each line sits on `x`.
    pub align: Align,
    /// Excalidraw font id.
    pub font_family: u32,
    /// Font size.
    pub font_size: f64,
    /// Color, element opacity included.
    pub color: Rgba,
}

/// Which part of a text line sits on the anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Line starts at the anchor.
    Start,
    /// Line is centered on the anchor.
    Middle,
    /// Line ends at the anchor.
    End,
}

/// Returns drawings for every visible element, in Excalidraw's draw order.
///
/// Types this version does not draw yet (freedraw, frame, embeds) are
/// skipped.
pub fn render(scene: &Scene) -> Vec<Drawing> {
    let background = scene.background_color();
    draw_order(scene)
        .into_iter()
        .filter_map(|element| render_element(element, background))
        .collect()
}

/// Returns the visible elements in Excalidraw's draw order.
///
/// Deleted elements are skipped, and a text label comes right after its
/// container instead of at its own position in the list.
pub fn draw_order(scene: &Scene) -> Vec<&Element> {
    let live: Vec<&Element> = scene
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect();
    let ids: HashSet<&str> = live.iter().map(|e| e.base.id.as_str()).collect();
    let container_of = |element| live_container(element, &ids);
    let labels: HashMap<&str, &Element> = live
        .iter()
        .filter_map(|e| Some((container_of(e)?, *e)))
        .collect();

    let mut order = Vec::with_capacity(live.len());
    for element in live.iter().filter(|e| container_of(e).is_none()) {
        order.push(*element);
        order.extend(labels.get(element.base.id.as_str()));
    }
    order
}

/// Draws one element; `background` is the canvas color, used by outlined
/// arrowheads. Returns `None` for types this version does not draw
/// (frames, embeds, images without a file).
pub fn render_element(element: &Element, background: &str) -> Option<Drawing> {
    draw(element, background)
}

/// Returns the id of a text label's container, if that container is drawn.
fn live_container<'a>(element: &'a Element, ids: &HashSet<&str>) -> Option<&'a str> {
    match &element.kind {
        Kind::Text(text) => text.container_id.as_deref().filter(|id| ids.contains(id)),
        _ => None,
    }
}

/// `ROUGHNESS.cartoonist`.
const CARTOONIST: f64 = 2.0;

/// First and last point closer than this close a line into a loop (px).
const LINE_CONFIRM_THRESHOLD: f64 = 8.0;

fn draw(element: &Element, background: &str) -> Option<Drawing> {
    let base = &element.base;
    let opacity = (base.opacity / 100.0) as f32;
    let generator = Generator::default();
    let (width, height) = (base.width, base.height);
    let box_center = [width / 2.0, height / 2.0];

    let mut items = Vec::new();
    let center = match &element.kind {
        Kind::Rectangle => {
            push_items(&mut items, &rectangle(&generator, element), opacity);
            box_center
        }
        Kind::Diamond => {
            push_items(&mut items, &diamond(&generator, element), opacity);
            box_center
        }
        Kind::Ellipse => {
            let options = rough_options(element, false);
            let ellipse =
                generator.ellipse(box_center[0], box_center[1], width, height, Some(options));
            push_items(&mut items, &ellipse, opacity);
            box_center
        }
        Kind::Line(line) | Kind::Arrow(line) => {
            let shapes = linear(&generator, element, line, background);
            for shape in &shapes {
                push_items(&mut items, shape, opacity);
            }
            let bounds = (line.points.len() >= 2).then(|| curve_bounds(curve_ops(&shapes[0])));
            let [x1, y1, x2, y2] = bounds
                .flatten()
                .unwrap_or_else(|| points_bounds(&line.points));
            [(x1 + x2) / 2.0, (y1 + y2) / 2.0]
        }
        Kind::Text(text) => {
            items.push(Item::Text(text_block(element, text, opacity)));
            box_center
        }
        Kind::Other(kind) if kind == "freedraw" => {
            let pen = element.freedraw()?;
            // A closed stroke with a background gets a rough fill under it
            // (`generateElementShape`, "freedraw").
            if is_loop(&pen.points) && base.background_color != "transparent" {
                let mut options = rough_options(element, false);
                options.fill_style = Some(rough_fill(&base.fill_style));
                options.fill = Some(base.background_color.clone());
                options.stroke = Some("none".into());
                let simplified = rough_rs::renderer::simplify(&pen.points, 0.75);
                push_items(
                    &mut items,
                    &generator.curve(&simplified, Some(options)),
                    opacity,
                );
            }
            let path = freedraw::outline(
                &pen.points,
                &pen.pressures,
                pen.simulate_pressure,
                pen.complete,
                base.stroke_width,
            );
            items.push(Item::Fill {
                path,
                color: color(&base.stroke_color, opacity),
                rule: FillRule::NonZero,
            });
            let [x1, y1, x2, y2] = points_bounds(&pen.points);
            [(x1 + x2) / 2.0, (y1 + y2) / 2.0]
        }
        Kind::Other(_) => {
            items.push(Item::Image {
                file_id: element.file_id()?.to_owned(),
                size: [width, height],
                opacity,
                crop: element
                    .image_crop()
                    .map(|(rect, natural)| Crop { rect, natural }),
                flip: element.image_flip(),
            });
            box_center
        }
    };

    let transform =
        Affine::rotate_about(base.angle, center).then(Affine::translate([base.x, base.y]));
    Some(Drawing { transform, items })
}

/// `generateRoughOptions` in `scene/Shape.ts`.
pub(super) fn rough_options(element: &Element, continuous_path: bool) -> Options {
    let base = &element.base;
    let solid = base.stroke_style == StrokeStyle::Solid;
    let mut options = Options {
        seed: Some(base.seed as u64),
        stroke_line_dash: line_dash(&base.stroke_style, base.stroke_width),
        // Dashes overlap when stroked twice.
        disable_multi_stroke: Some(!solid),
        stroke_width: Some(if solid {
            base.stroke_width
        } else {
            base.stroke_width + 0.5
        }),
        fill_weight: Some(base.stroke_width / 2.0),
        hachure_gap: Some(base.stroke_width * 4.0),
        roughness: Some(adjusted_roughness(element)),
        stroke: Some(base.stroke_color.clone()),
        preserve_vertices: Some(continuous_path || base.roughness < CARTOONIST),
        ..Options::default()
    };
    match &element.kind {
        Kind::Rectangle | Kind::Diamond | Kind::Ellipse => {
            options.fill_style = Some(rough_fill(&base.fill_style));
            options.fill =
                (!is_transparent(&base.background_color)).then(|| base.background_color.clone());
            if matches!(element.kind, Kind::Ellipse) {
                options.curve_fitting = Some(1.0);
            }
        }
        Kind::Line(line) if is_loop(&line.points) => {
            options.fill_style = Some(rough_fill(&base.fill_style));
            // Excalidraw checks only the literal here, not `isTransparent`.
            options.fill =
                (base.background_color != "transparent").then(|| base.background_color.clone());
        }
        _ => {}
    }
    options
}

/// `adjustRoughness`: small shapes get less wobble.
fn adjusted_roughness(element: &Element) -> f64 {
    let base = &element.base;
    let (max, min) = (base.width.max(base.height), base.width.min(base.height));
    let can_round = matches!(
        element.kind,
        Kind::Rectangle | Kind::Diamond | Kind::Line(_)
    );
    let linear = matches!(element.kind, Kind::Line(_) | Kind::Arrow(_));
    if (min >= 20.0 && max >= 50.0)
        || (min >= 15.0 && base.roundness.is_some() && can_round)
        || (linear && max >= 50.0)
    {
        return base.roughness;
    }
    (base.roughness / if max < 10.0 { 3.0 } else { 2.0 }).min(2.5)
}

fn line_dash(style: &StrokeStyle, width: f64) -> Option<Vec<f64>> {
    match style {
        StrokeStyle::Dashed => Some(vec![8.0, 8.0 + width]),
        StrokeStyle::Dotted => Some(dotted(width).to_vec()),
        StrokeStyle::Solid | StrokeStyle::Other(_) => None,
    }
}

pub(super) fn dotted(width: f64) -> [f64; 2] {
    [1.5, 6.0 + width]
}

fn rough_fill(style: &FillStyle) -> rough_rs::FillStyle {
    match style {
        FillStyle::CrossHatch => rough_rs::FillStyle::CrossHatch,
        FillStyle::Solid => rough_rs::FillStyle::Solid,
        FillStyle::Zigzag => rough_rs::FillStyle::Zigzag,
        FillStyle::Hachure | FillStyle::Other(_) => rough_rs::FillStyle::Hachure,
    }
}

/// `isTransparent`: `transparent`, or a hex color with zero alpha.
pub(crate) fn is_transparent(color: &str) -> bool {
    (color.len() == 5 && color.ends_with('0'))
        || (color.len() == 9 && color.ends_with("00"))
        || color == "transparent"
}

/// `isPathALoop` at zoom 1.
pub(crate) fn is_loop(points: &[Point]) -> bool {
    match points {
        [first, .., last] if points.len() >= 3 => {
            (first[0] - last[0]).hypot(first[1] - last[1]) <= LINE_CONFIRM_THRESHOLD
        }
        _ => false,
    }
}

/// `getCornerRadius`.
pub(crate) fn corner_radius(x: f64, roundness: &Roundness) -> f64 {
    const LEGACY: u8 = 1;
    const PROPORTIONAL_RADIUS: u8 = 2;
    const ADAPTIVE_RADIUS: u8 = 3;
    const DEFAULT_PROPORTIONAL_RADIUS: f64 = 0.25;
    const DEFAULT_ADAPTIVE_RADIUS: f64 = 32.0;

    match roundness.kind {
        LEGACY | PROPORTIONAL_RADIUS => x * DEFAULT_PROPORTIONAL_RADIUS,
        ADAPTIVE_RADIUS => {
            let fixed = roundness.value.unwrap_or(DEFAULT_ADAPTIVE_RADIUS);
            if x <= fixed / DEFAULT_PROPORTIONAL_RADIUS {
                x * DEFAULT_PROPORTIONAL_RADIUS
            } else {
                fixed
            }
        }
        _ => 0.0,
    }
}

/// Appends a rough drawable the way rough.js' canvas and SVG renderers draw it.
fn push_items(items: &mut Vec<Item>, drawable: &Drawable, opacity: f32) {
    let o = &drawable.options;
    let fill = || color(o.fill.as_deref().unwrap_or_default(), opacity);
    for set in &drawable.sets {
        let path = segments(&set.ops);
        items.push(match set.set_type {
            OpSetType::Path => Item::Stroke {
                path,
                color: color(&o.stroke, opacity),
                width: o.stroke_width,
                dash: o.stroke_line_dash.clone(),
            },
            OpSetType::FillPath => Item::Fill {
                path,
                color: fill(),
                rule: match drawable.shape {
                    ShapeType::Curve | ShapeType::Polygon | ShapeType::Path => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                },
            },
            OpSetType::FillSketch => Item::Stroke {
                path,
                color: fill(),
                width: if o.fill_weight < 0.0 {
                    o.stroke_width / 2.0
                } else {
                    o.fill_weight
                },
                dash: o.fill_line_dash.clone(),
            },
        });
    }
}

fn segments(ops: &[Op]) -> Vec<Segment> {
    ops.iter()
        .map(|op| {
            let d = &op.data;
            match op.op {
                OpType::Move => Segment::MoveTo([d[0], d[1]]),
                OpType::LineTo => Segment::LineTo([d[0], d[1]]),
                OpType::BCurveTo => Segment::CubicTo([d[0], d[1]], [d[2], d[3]], [d[4], d[5]]),
            }
        })
        .collect()
}

fn color(css: &str, opacity: f32) -> Rgba {
    Rgba::parse(css).unwrap_or(Rgba::BLACK).fade(opacity)
}

#[cfg(test)]
mod tests {
    use super::{Align, Item, render};
    use crate::scene::Scene;

    const BASE: &str = r##""x":100,"y":50,"width":120,"height":60,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"roundness":null,"seed":1968410193,"isDeleted":false"##;

    fn scene(elements: &[String]) -> Scene {
        let json = format!(
            r#"{{"type":"excalidraw","elements":[{}]}}"#,
            elements.join(",")
        );
        serde_json::from_str(&json).unwrap()
    }

    fn text(id: &str, container: &str) -> String {
        format!(
            r#"{{"id":"{id}","type":"text",{BASE},"text":"a\nb","fontSize":20,"fontFamily":5,"textAlign":"center","verticalAlign":"middle","lineHeight":1.25,"containerId":{container}}}"#
        )
    }

    #[test]
    fn labels_follow_their_container_and_deleted_are_skipped() {
        let rect = format!(r#"{{"id":"box","type":"rectangle",{BASE}}}"#);
        let deleted = format!(r#"{{"id":"gone","type":"ellipse",{BASE}}}"#)
            .replace(r#""isDeleted":false"#, r#""isDeleted":true"#);
        let label = text("label", r#""box""#);
        let free = text("free", "null");
        // Label listed first, container second: label must still draw after it.
        let drawings = render(&scene(&[label, deleted, free, rect]));
        let kinds: Vec<&str> = drawings
            .iter()
            .map(|d| match &d.items[0] {
                Item::Text(t) if t.x == 60.0 => "text",
                _ => "shape",
            })
            .collect();
        assert_eq!(kinds, ["text", "shape", "text"]);
    }

    #[test]
    fn text_uses_excalidraw_baseline() {
        let drawings = render(&scene(&[text("t", "null")]));
        let Item::Text(block) = &drawings[0].items[0] else {
            panic!("expected text");
        };
        assert_eq!(block.lines, ["a", "b"]);
        assert_eq!(block.align, Align::Middle);
        assert_eq!(block.line_height, 25.0);
        // Excalifont at 20px: ascent 17.72, descent 7.48, centered in 25px.
        assert!((block.baseline - 17.62).abs() < 1e-9, "{}", block.baseline);
        assert_eq!(drawings[0].transform.apply([0.0, 0.0]), [100.0, 50.0]);
    }

    #[test]
    fn arrow_gets_shaft_and_two_head_strokes() {
        let arrow = format!(
            r#"{{"id":"a","type":"arrow",{BASE},"points":[[0,0],[120,60]],"startArrowhead":null,"endArrowhead":"arrow"}}"#
        );
        let drawings = render(&scene(&[arrow]));
        let strokes = drawings[0]
            .items
            .iter()
            .filter(|i| matches!(i, Item::Stroke { .. }))
            .count();
        // Shaft: one stroke set; each head line: one stroke set.
        assert_eq!(strokes, 3);
    }
}
