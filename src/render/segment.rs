//! Segment middles and lengths of lines, on the drawn curve of round ones.
//! Ports `getSegmentMidPoint` / `isSegmentTooShort`
//! (`element/linearElementEditor.ts`) and `getControlPointsForBezierCurve`,
//! `getBezierXY`, `getBezierCurveArcLengths`, `mapIntervalToBezierT`
//! (`shapes.tsx`) from Excalidraw 0.18.1, MIT licensed, Copyright (c) 2020
//! Excalidraw.
use rough_rs::{Generator, OpType};

use super::linear::curve_ops;
use super::{Segment, rough_options, segments};
use crate::geometry::Point;
use crate::scene::{Element, Kind, Linear};

/// The middle of the segment ending at point `end`, in local coordinates
/// (`getSegmentMidPoint`): on a round line with 3+ points, the point half
/// way along the drawn curve; otherwise the chord's middle.
pub(crate) fn segment_midpoint(element: &Element, end: usize) -> Point {
    let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
        return [0.0, 0.0];
    };
    let [a, b] = [line.points[end - 1], line.points[end]];
    let chord = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let Some(cubic) = segment_cubic(element, line, b) else {
        return chord;
    };
    let arcs = arc_lengths(cubic, b);
    bezier_xy(cubic, interval_to_t(&arcs, 0.5))
}

/// The length of the segment ending at point `end` (`isSegmentTooShort`):
/// along the drawn curve on a round line with 3+ points, else the chord.
pub(crate) fn segment_length(element: &Element, end: usize) -> f64 {
    let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
        return 0.0;
    };
    let [a, b] = [line.points[end - 1], line.points[end]];
    if line.points.len() > 2 && element.base.roundness.is_some() {
        // `getBezierCurveLength`: no cubic found means no samples, length 0.
        return segment_cubic(element, line, b)
            .and_then(|cubic| arc_lengths(cubic, b).last().copied())
            .unwrap_or(0.0);
    }
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// A round line's drawn shaft as a path (`getCurvePathOps`): its stroke
/// set, without fill or arrowheads. Empty for other elements.
pub(crate) fn curve_path(element: &Element) -> Vec<Segment> {
    let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
        return Vec::new();
    };
    // Same options and seed as the rendered shaft, which is generated first.
    let shaft = Generator::default().curve(&line.points, Some(rough_options(element, false)));
    segments(curve_ops(&shaft))
}

/// The cubic of a round line's drawn shaft that ends nearest `end`
/// (`getControlPointsForBezierCurve`: every pass counts, the first nearest
/// wins). `None` for straight lines and 2-point ones.
fn segment_cubic(element: &Element, line: &Linear, end: Point) -> Option<[Point; 4]> {
    if line.points.len() <= 2 || element.base.roundness.is_none() {
        return None;
    }
    // Same options and seed as the rendered shaft, which is generated first.
    let shaft = Generator::default().curve(&line.points, Some(rough_options(element, false)));
    let mut current = [0.0, 0.0];
    let mut nearest = None;
    let mut min = f64::INFINITY;
    for op in curve_ops(&shaft) {
        let d = &op.data;
        match op.op {
            OpType::Move => current = [d[0], d[1]],
            OpType::BCurveTo => {
                let p3 = [d[4], d[5]];
                let distance = (p3[0] - end[0]).hypot(p3[1] - end[1]);
                if distance < min {
                    min = distance;
                    nearest = Some([current, [d[0], d[1]], [d[2], d[3]], p3]);
                }
                current = p3;
            }
            OpType::LineTo => {}
        }
    }
    nearest
}

/// `getBezierXY`: note that `t = 1` is the start and `t = 0` the end.
fn bezier_xy([p0, p1, p2, p3]: [Point; 4], t: f64) -> Point {
    let equation = |i: usize| {
        (1.0 - t).powf(3.0) * p3[i]
            + 3.0 * t * (1.0 - t).powf(2.0) * p2[i]
            + 3.0 * t.powf(2.0) * (1.0 - t) * p1[i]
            + p0[i] * t.powf(3.0)
    };
    [equation(0), equation(1)]
}

/// `getBezierCurveArcLengths`: running lengths through samples at
/// `t = 1, 0.95, ...` while `t > 0` (`getPointsInBezierCurve`).
fn arc_lengths(cubic: [Point; 4], end: Point) -> Vec<f64> {
    /// `PRECISION` in `math/utils.ts`.
    const PRECISION: f64 = 10e-5;
    /// Step between samples.
    const STEP: f64 = 0.05;

    let mut samples = Vec::new();
    let mut t = 1.0;
    while t > 0.0 {
        samples.push(bezier_xy(cubic, t));
        t -= STEP;
    }
    // Excalidraw appends the end only when the last sample already equals it.
    if samples
        .last()
        .is_some_and(|p| (p[0] - end[0]).abs() < PRECISION && (p[1] - end[1]).abs() < PRECISION)
    {
        samples.push(end);
    }
    let mut lengths = vec![0.0];
    let mut distance = 0.0;
    for pair in samples.windows(2) {
        distance += (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
        lengths.push(distance);
    }
    lengths
}

/// `mapIntervalToBezierT`: the `t` at `interval` of the curve's length,
/// with Excalidraw's binary search and its quirks kept.
fn interval_to_t(lengths: &[f64], interval: f64) -> f64 {
    let count = lengths.len() - 1;
    let target = interval * lengths[count];
    let (mut low, mut high, mut index) = (0, count, 0);
    while low < high {
        index = low + (high - low) / 2;
        if lengths[index] < target {
            low = index + 1;
        } else {
            high = index;
        }
    }
    if lengths[index] > target {
        index -= 1;
    }
    if lengths[index] == target {
        return index as f64 / count as f64;
    }
    let fraction = (target - lengths[index]) / (lengths[index + 1] - lengths[index]);
    1.0 - (index as f64 + fraction) / count as f64
}

#[cfg(test)]
mod tests {
    use super::{arc_lengths, bezier_xy, interval_to_t};

    #[test]
    fn curve_middle_matches_excalidraws_sampling() {
        // Excalidraw's `mapIntervalToBezierT` and `getBezierXY` run on the
        // same cubic under Bun: its samples stop short of the end, so the
        // middle of a symmetric curve is not at t = 0.5.
        let cubic = [[0.0, 0.0], [30.0, 40.0], [70.0, 40.0], [100.0, 0.0]];
        let lengths = arc_lengths(cubic, cubic[3]);
        assert_eq!(lengths.len(), 20);
        assert_eq!(lengths[19], 113.6166811317384);
        let t = interval_to_t(&lengths, 0.5);
        assert_eq!(t, 0.510270375616428);
        assert_eq!(bezier_xy(cubic, t), [48.92163222678585, 29.9873423261637]);
    }
}
