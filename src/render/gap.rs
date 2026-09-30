//! The gap in a labelled arrow: its line stops short of the label, like
//! Excalidraw clearing the label's box from the arrow's own canvas
//! (`renderElement.ts`, REFERENCE-001 section 15).
use super::{Drawing, Item, Segment};
use crate::geometry::{Affine, Bounds, Point};
use crate::scene::{Element, Kind, Scene};

/// `BOUND_TEXT_PADDING`: how far the gap reaches past the label's box.
const PADDING: f64 = 5.0;

/// Straight pieces each cubic of a stroke is cut into before clipping.
const PIECES: usize = 8;

/// The label of a line or arrow, if it has one.
pub fn arrow_label<'a>(element: &Element, scene: &'a Scene) -> Option<&'a Element> {
    if !matches!(element.kind, Kind::Arrow(_) | Kind::Line(_)) {
        return None;
    }
    let id = &element
        .base
        .bound_elements
        .as_ref()?
        .iter()
        .find(|b| b.kind == "text")?
        .id;
    scene
        .elements
        .iter()
        .find(|e| &e.base.id == id && !e.base.is_deleted)
}

/// The box a label clears from its arrow: its own box, 5 units larger on
/// every side, in scene units.
pub fn label_gap(label: &Element) -> Bounds {
    let b = &label.base;
    [
        b.x - PADDING,
        b.y - PADDING,
        b.x + b.width + PADDING,
        b.y + b.height + PADDING,
    ]
}

/// Returns `drawing` in scene units with its strokes cut where they cross
/// `gap`. Fills are kept whole (an arrowhead never sits under its label).
pub fn cut_gap(drawing: &Drawing, gap: Bounds) -> Drawing {
    let transform = drawing.transform;
    let items = drawing
        .items
        .iter()
        .map(|item| match item {
            Item::Stroke {
                path,
                color,
                width,
                dash,
            } => Item::Stroke {
                path: clip_outside(&flatten(path, transform), gap),
                color: *color,
                width: *width,
                dash: dash.clone(),
            },
            Item::Fill { path, color, rule } => Item::Fill {
                path: path.iter().map(|s| moved(s, transform)).collect(),
                color: *color,
                rule: *rule,
            },
            other => other.clone(),
        })
        .collect();
    Drawing {
        transform: Affine::IDENTITY,
        items,
    }
}

/// A path's subpaths as polylines in scene units.
fn flatten(path: &[Segment], transform: Affine) -> Vec<Vec<Point>> {
    let mut lines: Vec<Vec<Point>> = Vec::new();
    let mut at = [0.0, 0.0];
    for segment in path {
        match *segment {
            Segment::MoveTo(p) => {
                lines.push(vec![transform.apply(p)]);
                at = p;
            }
            Segment::LineTo(p) => {
                push(&mut lines, at, transform.apply(p));
                at = p;
            }
            Segment::CubicTo(c1, c2, p) => {
                for i in 1..=PIECES {
                    let t = i as f64 / PIECES as f64;
                    let u = 1.0 - t;
                    let point = [0, 1].map(|k| {
                        u * u * u * at[k]
                            + 3.0 * u * u * t * c1[k]
                            + 3.0 * u * t * t * c2[k]
                            + t * t * t * p[k]
                    });
                    push(&mut lines, at, transform.apply(point));
                }
                at = p;
            }
        }
    }
    lines
}

/// Adds `point` to the last polyline, starting one at `from` if there is
/// none (a path that does not begin with a move).
fn push(lines: &mut Vec<Vec<Point>>, from: Point, point: Point) {
    match lines.last_mut() {
        Some(line) => line.push(point),
        None => lines.push(vec![from, point]),
    }
}

/// The parts of `lines` outside `gap`, as a path of straight segments.
fn clip_outside(lines: &[Vec<Point>], gap: Bounds) -> Vec<Segment> {
    let mut path = Vec::new();
    for line in lines {
        // Whether the pen is down at the end of `path`.
        let mut drawing = false;
        for pair in line.windows(2) {
            let (p, q) = (pair[0], pair[1]);
            let lerp = |t: f64| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
            let (enter, leave) = inside(p, q, gap).unwrap_or((1.0, 1.0));
            if enter > 0.0 {
                if !drawing {
                    path.push(Segment::MoveTo(p));
                }
                path.push(Segment::LineTo(lerp(enter)));
                drawing = enter >= 1.0;
            }
            if leave < 1.0 {
                path.push(Segment::MoveTo(lerp(leave)));
                path.push(Segment::LineTo(q));
                drawing = true;
            }
        }
    }
    path
}

/// The part of the segment from `p` to `q` inside `gap`, as a range of its
/// parameter (Liang-Barsky); `None` when it misses.
fn inside(p: Point, q: Point, [x1, y1, x2, y2]: Bounds) -> Option<(f64, f64)> {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
    for (towards, room) in [
        (-dx, p[0] - x1),
        (dx, x2 - p[0]),
        (-dy, p[1] - y1),
        (dy, y2 - p[1]),
    ] {
        if towards == 0.0 {
            if room < 0.0 {
                return None;
            }
        } else if towards < 0.0 {
            enter = enter.max(room / towards);
        } else {
            leave = leave.min(room / towards);
        }
    }
    (enter < leave).then_some((enter, leave))
}

fn moved(segment: &Segment, transform: Affine) -> Segment {
    match *segment {
        Segment::MoveTo(p) => Segment::MoveTo(transform.apply(p)),
        Segment::LineTo(p) => Segment::LineTo(transform.apply(p)),
        Segment::CubicTo(a, b, p) => {
            Segment::CubicTo(transform.apply(a), transform.apply(b), transform.apply(p))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Segment, clip_outside};

    #[test]
    fn a_line_through_the_gap_is_cut_at_its_edges() {
        let line = vec![vec![[0.0, 5.0], [100.0, 5.0]]];
        assert_eq!(
            clip_outside(&line, [40.0, 0.0, 60.0, 10.0]),
            [
                Segment::MoveTo([0.0, 5.0]),
                Segment::LineTo([40.0, 5.0]),
                Segment::MoveTo([60.0, 5.0]),
                Segment::LineTo([100.0, 5.0]),
            ]
        );
        // A line that misses the gap stays one piece.
        let clear = vec![vec![[0.0, 50.0], [50.0, 50.0], [100.0, 50.0]]];
        assert_eq!(
            clip_outside(&clear, [40.0, 0.0, 60.0, 10.0]),
            [
                Segment::MoveTo([0.0, 50.0]),
                Segment::LineTo([50.0, 50.0]),
                Segment::LineTo([100.0, 50.0]),
            ]
        );
        // One ending inside the gap stops at its edge.
        let into = vec![vec![[0.0, 5.0], [50.0, 5.0]]];
        assert_eq!(
            clip_outside(&into, [40.0, 0.0, 60.0, 10.0]),
            [Segment::MoveTo([0.0, 5.0]), Segment::LineTo([40.0, 5.0])]
        );
    }
}
