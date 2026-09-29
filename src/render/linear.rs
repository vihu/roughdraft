//! Lines and arrows: shaft, arrowheads, and the curve bounds Excalidraw
//! rotates them around. Ports `getArrowheadShapes` (`scene/Shape.ts`) and
//! `getArrowheadPoints` / `getMinMaxXYFromCurvePathOps` (`element/bounds.ts`)
//! from Excalidraw 0.18.1, MIT licensed, Copyright (c) 2020 Excalidraw.
use std::f64::consts::PI;

use rough_rs::{Drawable, Generator, Op, OpSetType, OpType, Options};

use super::{dotted, rough_options};
use crate::geometry::Point;
use crate::scene::{Arrowhead, Element, Kind, Linear, StrokeStyle};

/// Which end of a linear element an arrowhead sits on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Start,
    Last,
}

/// Shaft first, then start and end arrowheads.
pub(super) fn linear(
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
pub(super) fn curve_ops(shape: &Drawable) -> &[Op] {
    let stroke = shape
        .sets
        .iter()
        .find(|set| set.set_type == OpSetType::Path);
    stroke.or(shape.sets.first()).map_or(&[], |set| &set.ops)
}

/// `getMinMaxXYFromCurvePathOps`: bounds of the Bézier segments only.
pub(super) fn curve_bounds(ops: &[Op]) -> Option<[f64; 4]> {
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

pub(super) fn points_bounds(points: &[Point]) -> [f64; 4] {
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
