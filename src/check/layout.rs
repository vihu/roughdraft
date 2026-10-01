//! Where elements sit against each other: shapes on top of each other,
//! arrows over shapes, arrow labels over the shapes they join.
use std::collections::HashMap;

use super::{Problem, Severity, contains, problem};
use crate::edit::Measure;
use crate::edit::arrow_points;
use crate::geometry::{self, Bounds};
use crate::hit::container_id;
use crate::render::{TITLE_FONT, segment_inside, title_box};
use crate::scene::{ArrowEnd, Element, Kind};

/// Room to leave on each side of an arrow label: Excalidraw's arrowhead
/// (25 long) and a margin.
const LABEL_CLEARANCE: f64 = 35.0;

/// Overlaps and crossings thinner than this are edges touching.
const TOUCH: f64 = 2.0;

/// Shapes and free text that partly cover each other. One wholly inside
/// another (a panel, a note on a shape) and members of one group are fine.
pub(super) fn overlaps(live: &[&Element], problems: &mut Vec<Problem>) {
    let solid = solid(live);
    for (i, (a, box_a)) in solid.iter().enumerate() {
        for (b, box_b) in &solid[i + 1..] {
            let shared_group = a.group_ids().iter().any(|g| b.group_ids().contains(g));
            if shared_group || within(*box_a, *box_b) || within(*box_b, *box_a) {
                continue;
            }
            if let Some((width, height)) = overlap(*box_a, *box_b) {
                problems.push(problem(
                    "overlap",
                    Severity::Warning,
                    &[a, b],
                    format!(
                        "\"{}\" and \"{}\" overlap by {width:.0} × {height:.0}; move one, or put one wholly inside the other",
                        a.base.id, b.base.id
                    ),
                ));
            }
        }
    }
}

/// What an arrow may be in the way of.
enum Obstacle {
    /// A shape, picture or free text.
    Shape,
    /// A frame's area.
    Frame,
    /// A frame's name, above its top-left corner.
    Title,
}

/// Lines and arrows that pass over a shape, through a frame or over a
/// frame's name that they do not start or end at. Shapes and frames
/// holding an end (a panel around both) are passed over on purpose.
pub(super) fn crossings(
    live: &[&Element],
    by_id: &HashMap<&str, &Element>,
    measure: &dyn Measure,
    problems: &mut Vec<Problem>,
) {
    let frames: Vec<&Element> = live
        .iter()
        .filter(|e| {
            e.base.angle == 0.0
                && matches!(&e.kind, Kind::Other(k) if k == "frame" || k == "magicframe")
        })
        .copied()
        .collect();
    // A name is as wide as its text, up to the frame's width.
    let titles = frames.iter().filter_map(|f| {
        let [x1, y1, x2, y2] = title_box(f)?;
        let (family, size, _) = TITLE_FONT;
        let width = measure.line_width(f.frame_title()?, family, size);
        let [x, y] = [f.base.x, f.base.y];
        let bounds = [x + x1, y + y1, x + x2.min(x1 + width), y + y2];
        Some((*f, bounds, Obstacle::Title))
    });
    let obstacles: Vec<(&Element, Bounds, Obstacle)> = solid(live)
        .into_iter()
        .map(|(e, b)| (e, b, Obstacle::Shape))
        .chain(
            frames
                .iter()
                .map(|f| (*f, geometry::element_bounds(f), Obstacle::Frame)),
        )
        .chain(titles)
        .collect();
    for arrow in live
        .iter()
        .filter(|e| matches!(e.kind, Kind::Arrow(_) | Kind::Line(_)))
    {
        let points = arrow_points(arrow);
        let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
            continue;
        };
        let ends: Vec<&str> = [ArrowEnd::Start, ArrowEnd::End]
            .iter()
            .filter_map(|&end| arrow.binding(end))
            .filter_map(|b| by_id.get(b.element_id.as_str()).map(|e| e.base.id.as_str()))
            .collect();
        for (shape, bounds, obstacle) in &obstacles {
            let inner = [
                bounds[0] + TOUCH,
                bounds[1] + TOUCH,
                bounds[2] - TOUCH,
                bounds[3] - TOUCH,
            ];
            if ends.contains(&shape.base.id.as_str())
                || contains(*bounds, first)
                || contains(*bounds, last)
                || inner[0] >= inner[2]
                || inner[1] >= inner[3]
            {
                continue;
            }
            if points
                .windows(2)
                .any(|s| segment_inside(s[0], s[1], inner).is_some())
            {
                let (arrow_id, shape_id) = (arrow_name(arrow), &shape.base.id);
                let over = match obstacle {
                    Obstacle::Shape => format!("passes over \"{shape_id}\""),
                    Obstacle::Frame => format!("passes through frame \"{shape_id}\""),
                    Obstacle::Title => format!("passes over the name of frame \"{shape_id}\""),
                };
                problems.push(problem(
                    "arrow-crosses",
                    Severity::Warning,
                    &[arrow, shape],
                    format!(
                        "{arrow_id} {over}; route it around with points, or move \"{shape_id}\""
                    ),
                ));
            }
        }
    }
}

/// Arrow labels in the way: on an arrow too short to show them and its
/// head, over a shape, or over another arrow's label. A label wholly
/// inside a shape (a panel around the arrow) is fine.
pub(super) fn arrow_labels(
    live: &[&Element],
    by_id: &HashMap<&str, &Element>,
    problems: &mut Vec<Problem>,
) {
    let solid = solid(live);
    let mut placed: Vec<(&Element, Bounds)> = Vec::new();
    for label in live.iter().filter(|e| matches!(e.kind, Kind::Text(_))) {
        let Some(arrow) = container_id(label)
            .and_then(|c| by_id.get(c))
            .filter(|c| matches!(c.kind, Kind::Arrow(_) | Kind::Line(_)))
        else {
            continue;
        };
        let id = arrow_name(arrow);
        let text = label.original_text().lines().next().unwrap_or_default();
        let at = geometry::element_bounds(label);
        let points = arrow_points(arrow);
        // With an even number of points the label sits on the middle
        // segment; with an odd number, on the middle point.
        if points.len() >= 2 && points.len().is_multiple_of(2) {
            let (p, q) = (points[points.len() / 2 - 1], points[points.len() / 2]);
            let (dx, dy) = ((q[0] - p[0]).abs(), (q[1] - p[1]).abs());
            let (length, along) = if dx >= dy {
                (dx, at[2] - at[0])
            } else {
                (dy, at[3] - at[1])
            };
            let needed = along + 2.0 * LABEL_CLEARANCE;
            if length < needed {
                problems.push(problem(
                    "arrow-too-short",
                    Severity::Warning,
                    &[arrow],
                    format!(
                        "{id}, labelled \"{text}\", runs {length:.0} where its label sits, and the label needs about {needed:.0} to leave the arrowhead clear: move its shapes {:.0} further apart (a larger gap in the row or column that holds them), or shorten the label or give it \"fontSize\": 16",
                        needed - length
                    ),
                ));
                placed.push((arrow, at));
                continue;
            }
        }
        for (shape, bounds) in &solid {
            if within(at, *bounds) || overlap(at, *bounds).is_none() {
                continue;
            }
            problems.push(problem(
                "arrow-label-overlap",
                Severity::Warning,
                &[arrow, shape],
                format!(
                    "the label \"{text}\" of {id} covers \"{}\"; move the shape, or route the arrow away from it with points",
                    shape.base.id
                ),
            ));
        }
        for (other, bounds) in &placed {
            if overlap(at, *bounds).is_some() {
                problems.push(problem(
                    "arrow-label-overlap",
                    Severity::Warning,
                    &[arrow, other],
                    format!(
                        "the label \"{text}\" of {id} and the label of {} cover each other; move one of the arrows",
                        arrow_name(other)
                    ),
                ));
            }
        }
        placed.push((arrow, at));
    }
}

/// Straight arrows between the same two shapes: drawn on top of each
/// other, they read as one.
pub(super) fn stacked_arrows(live: &[&Element], problems: &mut Vec<Problem>) {
    let straight: Vec<(&Element, [String; 2])> = live
        .iter()
        .filter(|e| matches!(e.kind, Kind::Arrow(_)) && arrow_points(e).len() == 2)
        .filter_map(|e| {
            let start = e.binding(ArrowEnd::Start)?;
            let end = e.binding(ArrowEnd::End)?;
            let mut ends = [start.element_id, end.element_id];
            ends.sort_unstable();
            Some((*e, ends))
        })
        .collect();
    for (i, (a, ends)) in straight.iter().enumerate() {
        for (b, _) in straight[i + 1..].iter().filter(|(_, other)| other == ends) {
            problems.push(problem(
                "arrows-stacked",
                Severity::Warning,
                &[a, b],
                format!(
                    "\"{}\" and \"{}\" both run straight between \"{}\" and \"{}\", one on top of the other: make them one arrow (with a startArrowhead for a two-way link), or route one with points",
                    a.base.id, b.base.id, ends[0], ends[1]
                ),
            ));
        }
    }
}

/// An arrow's id in quotes, with the shapes it joins: generated ids say
/// nothing.
fn arrow_name(arrow: &Element) -> String {
    let id = &arrow.base.id;
    match (arrow.binding(ArrowEnd::Start), arrow.binding(ArrowEnd::End)) {
        (Some(from), Some(to)) => format!(
            "\"{id}\" (from \"{}\" to \"{}\")",
            from.element_id, to.element_id
        ),
        _ => format!("\"{id}\""),
    }
}

/// The elements that can sit in the way, with their boxes.
fn solid<'a>(live: &[&'a Element]) -> Vec<(&'a Element, Bounds)> {
    live.iter()
        .filter(|e| is_solid(e))
        .map(|e| (*e, geometry::element_bounds(e)))
        .collect()
}

/// Width and height of where two boxes cover each other, if more than
/// their edges touch.
fn overlap(a: Bounds, b: Bounds) -> Option<(f64, f64)> {
    let width = a[2].min(b[2]) - a[0].max(b[0]);
    let height = a[3].min(b[3]) - a[1].max(b[1]);
    (width > TOUCH && height > TOUCH).then_some((width, height))
}

/// Shapes, pictures and free text: what can sit in the way. Rotated ones
/// are left out: their boxes are not the space they cover.
fn is_solid(element: &Element) -> bool {
    if element.base.angle != 0.0 {
        return false;
    }
    match &element.kind {
        Kind::Rectangle | Kind::Diamond | Kind::Ellipse => true,
        Kind::Text(_) => container_id(element).is_none(),
        Kind::Other(kind) => kind == "image",
        _ => false,
    }
}

/// Whether `inner` lies within `outer`.
fn within(inner: Bounds, outer: Bounds) -> bool {
    inner[0] >= outer[0] - TOUCH
        && inner[1] >= outer[1] - TOUCH
        && inner[2] <= outer[2] + TOUCH
        && inner[3] <= outer[3] + TOUCH
}
