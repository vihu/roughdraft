//! Pen strokes: perfect-freehand 1.2.0 `getStroke` (MIT, Copyright (c) 2021
//! Stephen Ruiz Ltd), ported line by line with Excalidraw's options
//! (`getFreeDrawSvgPath`), and the outline turned into a path the way
//! Excalidraw's `getSvgPathFromStroke` does.
use std::f64::consts::PI;

use super::Segment;
use crate::geometry::Point;

/// `RATE_OF_PRESSURE_CHANGE`.
const RATE_OF_PRESSURE_CHANGE: f64 = 0.275;

/// `FIXED_PI`: "browser strokes seem to be off if PI is regular".
const FIXED_PI: f64 = PI + 0.0001;

/// Excalidraw's `thinning`, `smoothing` and `streamline`.
const THINNING: f64 = 0.6;
const SMOOTHING: f64 = 0.5;
const STREAMLINE: f64 = 0.5;

/// Excalidraw's `size` is this many stroke widths.
const SIZE_PER_WIDTH: f64 = 4.25;

/// The closed outline of a pen stroke, in the element's coordinates.
/// `pressures` is used unless `simulate_pressure`; `complete` is whether
/// the stroke was finished (`lastCommittedPoint`).
pub(super) fn outline(
    points: &[Point],
    pressures: &[f64],
    simulate_pressure: bool,
    complete: bool,
    stroke_width: f64,
) -> Vec<Segment> {
    svg_path(&stroke(
        points,
        pressures,
        simulate_pressure,
        complete,
        stroke_width,
    ))
}

/// `getStroke` with Excalidraw's options: the outline's points.
fn stroke(
    points: &[Point],
    pressures: &[f64],
    simulate_pressure: bool,
    complete: bool,
    stroke_width: f64,
) -> Vec<Point> {
    // `[x, y, pressure]`, NaN standing in for JavaScript's `undefined`.
    let input: Vec<[f64; 3]> = match points {
        [] => vec![[0.0, 0.0, 0.5]],
        _ if simulate_pressure => points.iter().map(|&[x, y]| [x, y, f64::NAN]).collect(),
        _ => points
            .iter()
            .enumerate()
            .map(|(i, &[x, y])| [x, y, pressures.get(i).copied().unwrap_or(f64::NAN)])
            .collect(),
    };
    let size = stroke_width * SIZE_PER_WIDTH;
    outline_points(
        &stroke_points(&input, size, complete),
        size,
        simulate_pressure,
    )
}

struct StrokePoint {
    point: Point,
    pressure: f64,
    vector: Point,
    distance: f64,
    running_length: f64,
}

/// `getStrokePoints`.
fn stroke_points(input: &[[f64; 3]], size: f64, complete: bool) -> Vec<StrokePoint> {
    let t = 0.15 + (1.0 - STREAMLINE) * 0.85;
    let mut pts = input.to_vec();
    // Extra points between two, to avoid "dash" lines.
    if let [first, last] = pts[..] {
        pts = std::iter::once(first)
            .chain((1..5).map(|i| {
                let [x, y] = lrp([first[0], first[1]], [last[0], last[1]], f64::from(i) / 4.0);
                [x, y, f64::NAN]
            }))
            .collect();
    }
    // One point gets a second one a pixel away.
    if let [only] = pts[..] {
        pts.push([only[0] + 1.0, only[1] + 1.0, only[2]]);
    }
    let pressure_or = |p: f64, default: f64| if p >= 0.0 { p } else { default };
    let mut stroke = vec![StrokePoint {
        point: [pts[0][0], pts[0][1]],
        pressure: pressure_or(pts[0][2], 0.25),
        vector: [1.0, 1.0],
        distance: 0.0,
        running_length: 0.0,
    }];
    let mut reached_minimum_length = false;
    let mut running_length = 0.0;
    let max = pts.len() - 1;
    for (i, &[x, y, pressure]) in pts.iter().enumerate().skip(1) {
        let prev = stroke.last().expect("starts with the first point");
        let point = if complete && i == max {
            [x, y]
        } else {
            lrp(prev.point, [x, y], t)
        };
        if prev.point == point {
            continue;
        }
        let distance = dist(point, prev.point);
        running_length += distance;
        // Wait until the line is a little way from the start, to avoid noise.
        if i < max && !reached_minimum_length {
            if running_length < size {
                continue;
            }
            reached_minimum_length = true;
        }
        let vector = uni(sub(prev.point, point));
        stroke.push(StrokePoint {
            point,
            pressure: pressure_or(pressure, 0.5),
            vector,
            distance,
            running_length,
        });
    }
    stroke[0].vector = stroke.get(1).map_or([0.0, 0.0], |second| second.vector);
    stroke
}

/// `getStrokeOutlinePoints` with Excalidraw's options: no taper, round caps,
/// sine easing.
fn outline_points(points: &[StrokePoint], size: f64, simulate_pressure: bool) -> Vec<Point> {
    let easing = |t: f64| (t * PI / 2.0).sin();
    let radius_for = |pressure: f64| size * easing(0.5 - THINNING * (0.5 - pressure));
    let Some(last_point) = points.last() else {
        return Vec::new();
    };
    if size <= 0.0 {
        return Vec::new();
    }
    let total_length = last_point.running_length;
    let min_distance = (size * SMOOTHING).powi(2);
    let (mut left, mut right): (Vec<Point>, Vec<Point>) = (Vec::new(), Vec::new());
    // Starts from the average of the first pressures: lines start slow.
    let simulated = |previous: f64, distance: f64| {
        let sp = (distance / size).min(1.0);
        let rp = (1.0 - sp).min(1.0);
        (previous + (rp - previous) * (sp * RATE_OF_PRESSURE_CHANGE)).min(1.0)
    };
    let mut prev_pressure = points
        .iter()
        .take(10)
        .fold(points[0].pressure, |acc, current| {
            let pressure = if simulate_pressure {
                simulated(acc, current.distance)
            } else {
                current.pressure
            };
            (acc + pressure) / 2.0
        });
    let mut radius = radius_for(last_point.pressure);
    let mut first_radius = None;
    let mut prev_vector = points[0].vector;
    let (mut pl, mut pr) = (points[0].point, points[0].point);
    let (mut tl, mut tr) = (pl, pr);
    let mut prev_sharp = false;
    let n = points.len();
    for (i, current) in points.iter().enumerate() {
        let StrokePoint {
            point,
            vector,
            distance,
            running_length,
            ..
        } = *current;
        // Noise at the end of the line.
        if i < n - 1 && total_length - running_length < 3.0 {
            continue;
        }
        let pressure = if simulate_pressure {
            simulated(prev_pressure, distance)
        } else {
            current.pressure
        };
        let unclamped = radius_for(pressure);
        first_radius.get_or_insert(unclamped);
        radius = unclamped.max(0.01);
        let next_vector = if i < n - 1 {
            points[i + 1].vector
        } else {
            vector
        };
        let next_dpr = if i < n - 1 {
            dpr(vector, next_vector)
        } else {
            1.0
        };
        let prev_dpr = dpr(vector, prev_vector);
        let sharp = prev_dpr < 0.0 && !prev_sharp;
        let next_sharp = next_dpr < 0.0;
        if sharp || next_sharp {
            // A rounded cap at a sharp corner.
            let offset = mul(per(prev_vector), radius);
            let step = 1.0 / 13.0;
            let mut t = 0.0;
            while t <= 1.0 {
                tl = rot_around(sub(point, offset), point, FIXED_PI * t);
                left.push(tl);
                tr = rot_around(add(point, offset), point, FIXED_PI * -t);
                right.push(tr);
                t += step;
            }
            pl = tl;
            pr = tr;
            if next_sharp {
                prev_sharp = true;
            }
            continue;
        }
        prev_sharp = false;
        if i == n - 1 {
            let offset = mul(per(vector), radius);
            left.push(sub(point, offset));
            right.push(add(point, offset));
            continue;
        }
        let offset = mul(per(lrp(next_vector, vector, next_dpr)), radius);
        tl = sub(point, offset);
        if i <= 1 || dist2(pl, tl) > min_distance {
            left.push(tl);
            pl = tl;
        }
        tr = add(point, offset);
        if i <= 1 || dist2(pr, tr) > min_distance {
            right.push(tr);
            pr = tr;
        }
        prev_pressure = pressure;
        prev_vector = vector;
    }

    let first_point = points[0].point;
    let last = if n > 1 {
        last_point.point
    } else {
        add(points[0].point, [1.0, 1.0])
    };
    if n == 1 {
        // A dot: without taper, drawn whether or not the stroke is complete.
        let start = prj(
            first_point,
            uni(per(sub(first_point, last))),
            -first_radius.unwrap_or(radius),
        );
        let step = 1.0 / 13.0;
        let mut dot = Vec::new();
        let mut t = step;
        while t <= 1.0 {
            dot.push(rot_around(start, first_point, FIXED_PI * 2.0 * t));
            t += step;
        }
        return dot;
    }
    let mut start_cap = Vec::new();
    if let Some(&first_right) = right.first() {
        let step = 1.0 / 13.0;
        let mut t = step;
        while t <= 1.0 {
            start_cap.push(rot_around(first_right, first_point, FIXED_PI * t));
            t += step;
        }
    }
    let direction = per(neg(last_point.vector));
    let start = prj(last, direction, radius);
    let mut end_cap = Vec::new();
    let step = 1.0 / 29.0;
    let mut t = step;
    while t < 1.0 {
        end_cap.push(rot_around(start, last, FIXED_PI * 3.0 * t));
        t += step;
    }
    right.reverse();
    left.into_iter()
        .chain(end_cap)
        .chain(right)
        .chain(start_cap)
        .collect()
}

/// `getSvgPathFromStroke`: quadratic curves through the midpoints, closed,
/// numbers cut (not rounded) to two decimals by its regular expression.
fn svg_path(points: &[Point]) -> Vec<Segment> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let cut = |[x, y]: Point| [truncate(x), truncate(y)];
    let mut path = vec![Segment::MoveTo(cut(first))];
    let mut current = cut(first);
    for (i, &point) in points.iter().enumerate() {
        let next = points[(i + 1) % points.len()];
        let (control, end) = (cut(point), cut(med(point, next)));
        // Quadratic to cubic.
        let c1 = add(current, mul(sub(control, current), 2.0 / 3.0));
        let c2 = add(end, mul(sub(control, end), 2.0 / 3.0));
        path.push(Segment::CubicTo(c1, c2, end));
        current = end;
    }
    path.push(Segment::LineTo(cut(first)));
    path
}

/// Cuts a number's decimal spelling after two digits, like
/// `TO_FIXED_PRECISION` does to the path string.
///
/// When `value * 100` is clearly between two integers, the cut is its
/// integer part; only near an integer, where the product's rounding could
/// cross it, does the spelling decide ([`truncate_spelled`]). Same result,
/// several times faster on a long stroke.
fn truncate(value: f64) -> f64 {
    /// How close to an integer `value * 100` must be to need the spelling:
    /// far above the product's rounding error below [`FAST_LIMIT`].
    const NEAR: f64 = 1e-6;
    /// Largest `|value * 100|` for the fast path.
    const FAST_LIMIT: f64 = 1e9;
    let scaled = value * 100.0;
    let fraction = (scaled - scaled.trunc()).abs();
    if scaled.abs() < FAST_LIMIT && (NEAR..1.0 - NEAR).contains(&fraction) {
        return scaled.trunc() / 100.0;
    }
    truncate_spelled(value)
}

/// [`truncate`] through the decimal spelling itself.
fn truncate_spelled(value: f64) -> f64 {
    let text = value.to_string();
    match text.find('.') {
        Some(dot) => text[..(dot + 3).min(text.len())].parse().unwrap_or(value),
        None => value,
    }
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: Point, n: f64) -> Point {
    [a[0] * n, a[1] * n]
}

fn neg(a: Point) -> Point {
    [-a[0], -a[1]]
}

fn per(a: Point) -> Point {
    [a[1], -a[0]]
}

fn dpr(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn dist(a: Point, b: Point) -> f64 {
    (a[1] - b[1]).hypot(a[0] - b[0])
}

fn dist2(a: Point, b: Point) -> f64 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1]
}

fn uni(a: Point) -> Point {
    let length = a[0].hypot(a[1]);
    [a[0] / length, a[1] / length]
}

fn med(a: Point, b: Point) -> Point {
    mul(add(a, b), 0.5)
}

fn lrp(a: Point, b: Point, t: f64) -> Point {
    add(a, mul(sub(b, a), t))
}

fn prj(a: Point, b: Point, c: f64) -> Point {
    add(a, mul(b, c))
}

fn rot_around(a: Point, c: Point, r: f64) -> Point {
    let (s, co) = r.sin_cos();
    let (px, py) = (a[0] - c[0], a[1] - c[1]);
    [px * co - py * s + c[0], px * s + py * co + c[1]]
}

#[cfg(test)]
mod tests {
    use super::{outline, stroke, truncate, truncate_spelled};
    use crate::geometry::Point;
    use crate::render::Segment;
    use crate::scene::Scene;

    /// `getSvgPathFromStroke`'s text: points joined as JavaScript prints
    /// them, every number cut after two decimals.
    fn path_text(points: &[Point]) -> String {
        let number = |v: f64| {
            let text = if v == 0.0 {
                "0".to_owned()
            } else {
                v.to_string()
            };
            match text.find('.') {
                Some(dot) => text[..(dot + 3).min(text.len())].to_owned(),
                None => text,
            }
        };
        let pair = |[x, y]: Point| format!("{},{}", number(x), number(y));
        let mut parts = vec!["M".to_owned(), pair(points[0]), "Q".to_owned()];
        for (i, &point) in points.iter().enumerate() {
            let next = points[(i + 1) % points.len()];
            parts.push(pair(point));
            parts.push(pair([
                (point[0] + next[0]) / 2.0,
                (point[1] + next[1]) / 2.0,
            ]));
        }
        parts.extend(["L".to_owned(), pair(points[0]), "Z".to_owned()]);
        parts.join(" ")
    }

    #[test]
    fn outlines_match_excalidraw_and_perfect_freehand() {
        let root = env!("CARGO_MANIFEST_DIR");
        let scene: Scene = serde_json::from_str(
            &std::fs::read_to_string(format!("{root}/tests/fixtures/scenes/freedraw.excalidraw"))
                .unwrap(),
        )
        .unwrap();
        let reference: std::collections::HashMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(format!("{root}/tests/fixtures/freedraw/reference.json"))
                .unwrap(),
        )
        .unwrap();
        let mut checked = 0;
        for element in &scene.elements {
            let Some(pen) = element.freedraw() else {
                continue;
            };
            let points = stroke(
                &pen.points,
                &pen.pressures,
                pen.simulate_pressure,
                pen.complete,
                element.base.stroke_width,
            );
            assert_eq!(
                path_text(&points),
                reference[&element.base.id],
                "{}",
                element.base.id
            );
            checked += 1;
        }
        assert_eq!(checked, reference.len());
    }

    #[test]
    fn numbers_are_cut_not_rounded() {
        assert_eq!(truncate(1.239), 1.23);
        assert_eq!(truncate(-1.239), -1.23);
        assert_eq!(truncate(2.0), 2.0);
        // 0.29 * 100 is 28.999999999999996 in floating point.
        assert_eq!(truncate(0.29), 0.29);
        assert_eq!(truncate(-0.001).to_bits(), (-0.0f64).to_bits());
        // The fast path agrees with the spelling, near two-decimal numbers
        // (one step either side) and between them.
        for i in -200_000..200_000 {
            let exact = f64::from(i) / 100.0;
            for value in [
                exact,
                f64::from_bits(exact.to_bits() + 1),
                f64::from_bits(exact.to_bits().saturating_sub(1)),
                exact + 0.003_7,
                exact * 1.000_000_1,
            ] {
                assert_eq!(
                    truncate(value).to_bits(),
                    truncate_spelled(value).to_bits(),
                    "{value}"
                );
            }
        }
    }

    #[test]
    fn a_stroke_is_one_closed_outline_around_its_points() {
        let points: Vec<[f64; 2]> = (0..20)
            .map(|i| [f64::from(i) * 5.0, (f64::from(i) * 0.4).sin() * 10.0])
            .collect();
        let path = outline(&points, &[], true, true, 2.0);
        assert!(matches!(path.first(), Some(Segment::MoveTo(_))));
        assert!(matches!(path.last(), Some(Segment::LineTo(_))));
        // The outline stays within the stroke's half width (4.25) of the points' box.
        for segment in &path {
            let (Segment::MoveTo(p) | Segment::LineTo(p) | Segment::CubicTo(_, _, p)) = segment;
            assert!(p[0] > -5.0 && p[0] < 101.0 && p[1].abs() < 16.0, "{p:?}");
        }
        // A single point is a dot.
        assert!(outline(&[[3.0, 4.0]], &[], true, true, 2.0).len() > 10);
    }
}
