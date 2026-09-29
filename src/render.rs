//! Scene to renderer-agnostic drawings, the way Excalidraw 0.18 draws them.
//!
//! Ports rough options per element type (`scene/Shape.ts`), arrowhead geometry
//! (`element/bounds.ts`), and draw order, transforms and text layout
//! (`renderer/staticSvgScene.ts`) from Excalidraw 0.18.1, MIT licensed,
//! Copyright (c) 2020 Excalidraw.
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use rough_rs::{Drawable, Generator, Op, OpSetType, OpType, Options, ShapeType};

use crate::color::Rgba;
use crate::geometry::{Affine, Point};
use crate::scene::{
    Arrowhead, Element, FillStyle, Kind, Linear, Roundness, Scene, StrokeStyle, Text, TextAlign,
};

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
/// Deleted elements are skipped, and a text label is drawn right after its
/// container instead of at its own position in the list. Types this version
/// does not draw yet (image, freedraw, frame, embeds) are skipped.
pub fn render(scene: &Scene) -> Vec<Drawing> {
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

    let background = scene.background_color();
    let mut drawings = Vec::new();
    for element in live.iter().filter(|e| container_of(e).is_none()) {
        drawings.extend(draw(element, background));
        if let Some(label) = labels.get(element.base.id.as_str()) {
            drawings.extend(draw(label, background));
        }
    }
    drawings
}

/// Returns the id of a text label's container, if that container is drawn.
fn live_container<'a>(element: &'a Element, ids: &HashSet<&str>) -> Option<&'a str> {
    match &element.kind {
        Kind::Text(text) => text.container_id.as_deref().filter(|id| ids.contains(id)),
        _ => None,
    }
}

/// Which end of a linear element an arrowhead sits on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Start,
    Last,
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
        Kind::Other(_) => return None,
    };

    let transform =
        Affine::rotate_about(base.angle, center).then(Affine::translate([base.x, base.y]));
    Some(Drawing { transform, items })
}

/// `generateRoughOptions` in `scene/Shape.ts`.
fn rough_options(element: &Element, continuous_path: bool) -> Options {
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

fn dotted(width: f64) -> [f64; 2] {
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
fn is_transparent(color: &str) -> bool {
    (color.len() == 5 && color.ends_with('0'))
        || (color.len() == 9 && color.ends_with("00"))
        || color == "transparent"
}

/// `isPathALoop` at zoom 1.
fn is_loop(points: &[Point]) -> bool {
    match points {
        [first, .., last] if points.len() >= 3 => {
            (first[0] - last[0]).hypot(first[1] - last[1]) <= LINE_CONFIRM_THRESHOLD
        }
        _ => false,
    }
}

/// `getCornerRadius`.
fn corner_radius(x: f64, roundness: &Roundness) -> f64 {
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

fn rectangle(generator: &Generator, element: &Element) -> Drawable {
    let (w, h) = (element.base.width, element.base.height);
    match &element.base.roundness {
        Some(roundness) => {
            let r = corner_radius(w.min(h), roundness);
            let (wr, hr) = (w - r, h - r);
            let d = format!(
                "M {r} 0 L {wr} 0 Q {w} 0, {w} {r} L {w} {hr} Q {w} {h}, {wr} {h} L {r} {h} Q 0 {h}, 0 {hr} L 0 {r} Q 0 0, {r} 0"
            );
            generator.path(&d, Some(rough_options(element, true)))
        }
        None => generator.rectangle(0.0, 0.0, w, h, Some(rough_options(element, false))),
    }
}

fn diamond(generator: &Generator, element: &Element) -> Drawable {
    let (w, h) = (element.base.width, element.base.height);
    // `getDiamondPoints`; the +1 keeps rough.js away from zero-length sides.
    let (top_x, top_y) = ((w / 2.0).floor() + 1.0, 0.0);
    let (right_x, right_y) = (w, (h / 2.0).floor() + 1.0);
    let (bottom_x, bottom_y) = (top_x, h);
    let (left_x, left_y) = (0.0, right_y);

    match &element.base.roundness {
        Some(roundness) => {
            let v = corner_radius((top_x - left_x).abs(), roundness);
            let h = corner_radius((right_y - top_y).abs(), roundness);
            let d = format!(
                "M {} {} L {} {} C {right_x} {right_y}, {right_x} {right_y}, {} {} L {} {} C {bottom_x} {bottom_y}, {bottom_x} {bottom_y}, {} {} L {} {} C {left_x} {left_y}, {left_x} {left_y}, {} {} L {} {} C {top_x} {top_y}, {top_x} {top_y}, {} {}",
                top_x + v,
                top_y + h,
                right_x - v,
                right_y - h,
                right_x - v,
                right_y + h,
                bottom_x + v,
                bottom_y - h,
                bottom_x - v,
                bottom_y - h,
                left_x + v,
                left_y + h,
                left_x + v,
                left_y - h,
                top_x - v,
                top_y + h,
                top_x + v,
                top_y + h,
            );
            generator.path(&d, Some(rough_options(element, true)))
        }
        None => {
            let points = [
                [top_x, top_y],
                [right_x, right_y],
                [bottom_x, bottom_y],
                [left_x, left_y],
            ];
            generator.polygon(&points, Some(rough_options(element, false)))
        }
    }
}

/// Shaft first, then start and end arrowheads.
fn linear(
    generator: &Generator,
    element: &Element,
    line: &Linear,
    background: &str,
) -> Vec<Drawable> {
    let mut options = rough_options(element, false);
    let points = if line.points.is_empty() {
        vec![[0.0, 0.0]]
    } else {
        line.points.clone()
    };
    // ponytail: elbow arrows drawn as plain polylines, port `generateElbowArrowShape` if needed
    let shaft = match (&element.base.roundness, &options.fill) {
        (None, Some(_)) => generator.polygon(&points, Some(options.clone())),
        (None, None) => generator.linear_path(&points, Some(options.clone())),
        (Some(_), _) => generator.curve(&points, Some(options.clone())),
    };
    let mut shapes = vec![shaft];
    if matches!(element.kind, Kind::Arrow(_)) {
        // Excalidraw mutates one options object across both ends, so the
        // start arrowhead's changes carry over to the end arrowhead.
        for (end, head) in [
            (End::Start, &line.start_arrowhead),
            (End::Last, &line.end_arrowhead),
        ] {
            if let Some(head) = head {
                let heads = arrowhead(
                    generator,
                    element,
                    line,
                    &shapes[0],
                    end,
                    head,
                    &mut options,
                    background,
                );
                shapes.extend(heads);
            }
        }
    }
    shapes
}

/// `getArrowheadShapes` in `scene/Shape.ts`.
#[allow(clippy::too_many_arguments)]
fn arrowhead(
    generator: &Generator,
    element: &Element,
    line: &Linear,
    shaft: &Drawable,
    end: End,
    head: &Arrowhead,
    options: &mut Options,
    background: &str,
) -> Vec<Drawable> {
    let Some(p) = arrowhead_points(element, line, shaft, end, head) else {
        return Vec::new();
    };
    let stroke = element.base.stroke_color.as_str();
    let roughness = options.roughness.unwrap_or(0.0);
    let solid = |fill: &str, max_roughness: f64, options: &Options| Options {
        fill: Some(fill.to_owned()),
        fill_style: Some(rough_rs::FillStyle::Solid),
        stroke: Some(stroke.to_owned()),
        roughness: Some(max_roughness.min(roughness)),
        ..options.clone()
    };

    match head {
        Arrowhead::Dot | Arrowhead::Circle | Arrowhead::CircleOutline => {
            options.stroke_line_dash = None;
            let fill = if *head == Arrowhead::CircleOutline {
                background
            } else {
                stroke
            };
            vec![generator.circle(p[0], p[1], p[2], Some(solid(fill, 0.5, options)))]
        }
        Arrowhead::Triangle | Arrowhead::TriangleOutline => {
            options.stroke_line_dash = None;
            let fill = if *head == Arrowhead::TriangleOutline {
                background
            } else {
                stroke
            };
            let points = [[p[0], p[1]], [p[2], p[3]], [p[4], p[5]], [p[0], p[1]]];
            vec![generator.polygon(&points, Some(solid(fill, 1.0, options)))]
        }
        Arrowhead::Diamond | Arrowhead::DiamondOutline => {
            options.stroke_line_dash = None;
            let fill = if *head == Arrowhead::DiamondOutline {
                background
            } else {
                stroke
            };
            let points = [
                [p[0], p[1]],
                [p[2], p[3]],
                [p[4], p[5]],
                [p[6], p[7]],
                [p[0], p[1]],
            ];
            vec![generator.polygon(&points, Some(solid(fill, 1.0, options)))]
        }
        Arrowhead::CrowfootOne => {
            vec![generator.line(p[2], p[3], p[4], p[5], Some(options.clone()))]
        }
        Arrowhead::Arrow
        | Arrowhead::Bar
        | Arrowhead::CrowfootMany
        | Arrowhead::CrowfootOneOrMany
        | Arrowhead::Other(_) => {
            options.stroke_line_dash = if element.base.stroke_style == StrokeStyle::Dotted {
                // Tighter dots so the cap stays legible.
                let [dot, gap] = dotted(element.base.stroke_width - 1.0);
                Some(vec![dot, gap - 1.0])
            } else {
                None
            };
            options.roughness = Some(roughness.min(1.0));
            let mut lines = vec![
                generator.line(p[2], p[3], p[0], p[1], Some(options.clone())),
                generator.line(p[4], p[5], p[0], p[1], Some(options.clone())),
            ];
            if *head == Arrowhead::CrowfootOneOrMany
                && let Some(one) =
                    arrowhead_points(element, line, shaft, end, &Arrowhead::CrowfootOne)
            {
                lines.push(generator.line(one[2], one[3], one[4], one[5], Some(options.clone())));
            }
            lines
        }
    }
}

/// `getArrowheadPoints` in `element/bounds.ts`: flat coordinates whose layout
/// depends on the arrowhead type, in element-local units.
fn arrowhead_points(
    element: &Element,
    line: &Linear,
    shaft: &Drawable,
    end: End,
    head: &Arrowhead,
) -> Option<Vec<f64>> {
    let ops = curve_ops(shaft);
    let index = match end {
        End::Start => 1,
        End::Last => ops.len().checked_sub(1)?,
    };
    let (op, prev) = (ops.get(index)?, ops.get(index.checked_sub(1)?)?);
    let [p1x, p1y, p2x, p2y, p3x, p3y] = op.data[..] else {
        return None;
    };
    let (p1, p2, p3) = ([p1x, p1y], [p2x, p2y], [p3x, p3y]);
    let p0 = match prev.op {
        OpType::Move => [prev.data[0], prev.data[1]],
        OpType::BCurveTo => [prev.data[4], prev.data[5]],
        OpType::LineTo => [0.0, 0.0],
    };

    // Excalidraw weights the control points in reverse, so t = 0.3 lands
    // near p3. Kept as is: it decides the arrowhead direction.
    let equation = |t: f64, i: usize| {
        (1.0 - t).powi(3) * p3[i]
            + 3.0 * t * (1.0 - t).powi(2) * p2[i]
            + 3.0 * t.powi(2) * (1.0 - t) * p1[i]
            + p0[i] * t.powi(3)
    };
    let [x2, y2] = if end == End::Start { p0 } else { p3 };
    let (x1, y1) = (equation(0.3, 0), equation(0.3, 1));
    let distance = (x2 - x1).hypot(y2 - y1);
    let (nx, ny) = ((x2 - x1) / distance, (y2 - y1) / distance);

    let points = &line.points;
    let last = points.len().checked_sub(1)?;
    let (tip, next) = match end {
        End::Start => (0, 1),
        End::Last => (last, last.wrapping_sub(1)),
    };
    let [cx, cy] = points[tip];
    let [px, py] = if points.len() > 1 {
        points[next]
    } else {
        [0.0, 0.0]
    };
    let length = (cx - px).hypot(cy - py);

    let diamond = matches!(head, Arrowhead::Diamond | Arrowhead::DiamondOutline);
    let min_size = arrowhead_size(head).min(length * if diamond { 0.25 } else { 0.5 });
    let (xs, ys) = (x2 - nx * min_size, y2 - ny * min_size);

    if matches!(
        head,
        Arrowhead::Dot | Arrowhead::Circle | Arrowhead::CircleOutline
    ) {
        let diameter = (ys - y2).hypot(xs - x2) + element.base.stroke_width - 2.0;
        return Some(vec![x2, y2, diameter]);
    }

    let angle = arrowhead_angle(head);
    if matches!(head, Arrowhead::CrowfootMany | Arrowhead::CrowfootOneOrMany) {
        let [x3, y3] = rotate([x2, y2], [xs, ys], radians(-angle));
        let [x4, y4] = rotate([x2, y2], [xs, ys], radians(angle));
        return Some(vec![xs, ys, x3, y3, x4, y4]);
    }

    let [x3, y3] = rotate([xs, ys], [x2, y2], radians(-angle));
    let [x4, y4] = rotate([xs, ys], [x2, y2], radians(angle));
    if diamond {
        let [ox, oy] = match end {
            End::Start => rotate(
                [x2 + min_size * 2.0, y2],
                [x2, y2],
                (py - y2).atan2(px - x2),
            ),
            End::Last => rotate(
                [x2 - min_size * 2.0, y2],
                [x2, y2],
                (y2 - py).atan2(x2 - px),
            ),
        };
        return Some(vec![x2, y2, x3, y3, ox, oy, x4, y4]);
    }
    Some(vec![x2, y2, x3, y3, x4, y4])
}

/// `getArrowheadSize`, in px.
fn arrowhead_size(head: &Arrowhead) -> f64 {
    match head {
        Arrowhead::Arrow => 25.0,
        Arrowhead::Diamond | Arrowhead::DiamondOutline => 12.0,
        Arrowhead::CrowfootMany | Arrowhead::CrowfootOne | Arrowhead::CrowfootOneOrMany => 20.0,
        _ => 15.0,
    }
}

/// `getArrowheadAngle`, in degrees.
fn arrowhead_angle(head: &Arrowhead) -> f64 {
    match head {
        Arrowhead::Bar => 90.0,
        Arrowhead::Arrow => 20.0,
        _ => 25.0,
    }
}

/// `degreesToRadians`, same operation order as Excalidraw.
fn radians(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

/// `pointRotateRads`.
fn rotate([x, y]: Point, [cx, cy]: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    [
        (x - cx) * cos - (y - cy) * sin + cx,
        (x - cx) * sin + (y - cy) * cos + cy,
    ]
}

/// `getCurvePathOps`: the first stroke set, else the first set.
fn curve_ops(shape: &Drawable) -> &[Op] {
    let stroke = shape
        .sets
        .iter()
        .find(|set| set.set_type == OpSetType::Path);
    stroke.or(shape.sets.first()).map_or(&[], |set| &set.ops)
}

/// `getMinMaxXYFromCurvePathOps`: bounds of the Bézier segments only.
fn curve_bounds(ops: &[Op]) -> Option<[f64; 4]> {
    let mut current = [0.0, 0.0];
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for op in ops {
        match op.op {
            OpType::Move => current = [op.data[0], op.data[1]],
            OpType::BCurveTo => {
                let d = &op.data;
                let [p1, p2, p3] = [[d[0], d[1]], [d[2], d[3]], [d[4], d[5]]];
                for axis in 0..2 {
                    let (lo, hi) = cubic_range(current[axis], p1[axis], p2[axis], p3[axis]);
                    bounds[axis] = bounds[axis].min(lo);
                    bounds[axis + 2] = bounds[axis + 2].max(hi);
                }
                current = p3;
            }
            OpType::LineTo => {}
        }
    }
    bounds.iter().all(|v| v.is_finite()).then_some(bounds)
}

/// Range of a cubic Bézier along one axis (`getCubicBezierCurveBound`).
fn cubic_range(p0: f64, p1: f64, p2: f64, p3: f64) -> (f64, f64) {
    let at = |t: f64| {
        (1.0 - t).powi(3) * p0
            + 3.0 * (1.0 - t).powi(2) * t * p1
            + 3.0 * (1.0 - t) * t.powi(2) * p2
            + t.powi(3) * p3
    };
    let (i, j, k) = (p1 - p0, p2 - p1, p3 - p2);
    let (a, b, c) = (3.0 * i - 6.0 * j + 3.0 * k, 6.0 * j - 6.0 * i, 3.0 * i);
    let discriminant = b * b - 4.0 * a * c;
    let mut range = (p0.min(p3), p0.max(p3));
    if discriminant >= 0.0 {
        let roots = if a == 0.0 {
            [-c / b; 2]
        } else {
            let root = discriminant.sqrt();
            [(-b + root) / (2.0 * a), (-b - root) / (2.0 * a)]
        };
        for t in roots.into_iter().filter(|t| (0.0..=1.0).contains(t)) {
            range = (range.0.min(at(t)), range.1.max(at(t)));
        }
    }
    range
}

fn points_bounds(points: &[Point]) -> [f64; 4] {
    points.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[x1, y1, x2, y2], [x, y]| [x1.min(*x), y1.min(*y), x2.max(*x), y2.max(*y)],
    )
}

fn text_block(element: &Element, text: &Text, opacity: f32) -> TextBlock {
    let line_height = text.font_size * text.line_height;
    // `getVerticalOffset`: center the font's ascent + descent in the line.
    let metrics = font_metrics(text.font_family);
    let em = text.font_size / metrics.units_per_em;
    let gap = (line_height - em * metrics.ascender + em * metrics.descender) / 2.0;
    let (x, align) = match text.text_align {
        TextAlign::Center => (element.base.width / 2.0, Align::Middle),
        TextAlign::Right => (element.base.width, Align::End),
        // ponytail: RTL lines anchor at the start, Excalidraw anchors them at the end
        TextAlign::Left | TextAlign::Other(_) => (0.0, Align::Start),
    };
    TextBlock {
        lines: text
            .text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(String::from)
            .collect(),
        x,
        line_height,
        baseline: em * metrics.ascender + gap,
        align,
        font_family: text.font_family,
        font_size: text.font_size,
        color: color(&element.base.stroke_color, opacity),
    }
}

/// Vertical font metrics from `fonts/FontMetadata.ts` (hhea table values).
struct FontMetrics {
    units_per_em: f64,
    ascender: f64,
    descender: f64,
}

fn font_metrics(family: u32) -> FontMetrics {
    let (units_per_em, ascender, descender) = match family {
        6 => (1000.0, 1011.0, -353.0), // Nunito
        7 => (1000.0, 923.0, -220.0),  // Lilita One
        8 => (1000.0, 750.0, -250.0),  // Comic Shanns
        2 => (2048.0, 1577.0, -471.0), // Helvetica
        3 => (2048.0, 1900.0, -480.0), // Cascadia
        9 => (2048.0, 1854.0, -434.0), // Liberation Sans
        _ => (1000.0, 886.0, -374.0),  // Excalifont, Virgil, and the fallback
    };
    FontMetrics {
        units_per_em,
        ascender,
        descender,
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
