//! Flip horizontally or vertically (Shift+H, Shift+V; `actions/actionFlip.ts`):
//! the selection mirrors about the centre of its box, like Excalidraw's
//! resize with a negative scale. Positions and angles mirror, as do the
//! points of lines and pen strokes and the pictures of images; text stays
//! readable. A selection of bound arrows only swaps their arrowheads.
use std::collections::HashSet;

use super::resize::normalize_angle;
use super::{Editor, common_bounds};
use crate::geometry::{self, Point};
use crate::hit::container_id;
use crate::scene::{ArrowEnd, Element, Kind};

/// Which way to mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Left and right swap (Shift+H).
    Horizontal,
    /// Top and bottom swap (Shift+V).
    Vertical,
}

// Private API
impl Editor {
    /// Mirrors the selection as one undo step.
    pub(super) fn flip(&mut self, axis: Axis) {
        let indices: Vec<usize> = self
            .moving_with_children()
            .into_iter()
            .map(|(i, _)| i)
            .filter(|&i| container_id(&self.scene.elements[i]).is_none())
            .collect();
        if indices.is_empty() {
            return;
        }
        let before = self.scene.elements.clone();
        let bound_arrow = |e: &Element| {
            matches!(e.kind, Kind::Arrow(_))
                && [ArrowEnd::Start, ArrowEnd::End]
                    .into_iter()
                    .any(|end| e.binding(end).is_some())
        };
        // Excalidraw's selection counts labels, so a labelled arrow is not
        // "only bound arrows" and flips for real.
        let labelled = self
            .moving_with_children()
            .iter()
            .any(|&(i, _)| container_id(&self.scene.elements[i]).is_some());
        if !labelled
            && indices
                .iter()
                .all(|&i| bound_arrow(&self.scene.elements[i]))
        {
            for &i in &indices {
                if let Kind::Arrow(line) = &mut self.scene.elements[i].kind {
                    std::mem::swap(&mut line.start_arrowhead, &mut line.end_arrowhead);
                }
                self.scene.elements[i].touch();
            }
            self.history.record(before);
            return;
        }
        let [x1, y1, x2, y2] = common_bounds(indices.iter().map(|&i| &self.scene.elements[i]));
        let centre = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
        for &i in &indices {
            mirror(&mut self.scene.elements[i], axis, centre);
            self.scene.elements[i].touch();
        }
        let flipped: HashSet<String> = indices
            .iter()
            .map(|&i| self.scene.elements[i].base.id.clone())
            .collect();
        let labels: Vec<(usize, Element)> = self
            .scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| container_id(e).is_some_and(|c| flipped.contains(c)))
            .map(|(i, e)| (i, e.clone()))
            .collect();
        self.sync_labels(&labels);
        for &i in &indices {
            self.rebind_moved_arrow(i);
        }
        self.update_bound_arrows(&flipped);
        self.history.record(before);
    }
}

/// Mirrors one element about the line through `centre` across `axis`.
fn mirror(element: &mut Element, axis: Axis, centre: Point) {
    let [lx1, ly1, lx2, ly2] = geometry::local_bounds(element);
    let local_centre = [(lx1 + lx2) / 2.0, (ly1 + ly2) / 2.0];
    let [cx, cy] = geometry::element_transform(element).apply(local_centre);
    let (target, flip_x) = match axis {
        Axis::Horizontal => ([2.0 * centre[0] - cx, cy], true),
        Axis::Vertical => ([cx, 2.0 * centre[1] - cy], false),
    };
    element.base.angle = normalize_angle(-element.base.angle);
    let reflect = |[x, y]: Point| {
        if flip_x {
            [2.0 * local_centre[0] - x, y]
        } else {
            [x, 2.0 * local_centre[1] - y]
        }
    };
    // Points mirror about the local centre, then shift so the first is at
    // the origin again; the local centre moves by the same shift.
    let rebase = |points: Vec<Point>| -> (Vec<Point>, Point) {
        let shift = points.first().copied().unwrap_or_default();
        let points = points
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1]])
            .collect();
        (
            points,
            [local_centre[0] - shift[0], local_centre[1] - shift[1]],
        )
    };
    let mut moved_centre = local_centre;
    match &mut element.kind {
        Kind::Line(line) | Kind::Arrow(line) => {
            let (points, centre) = rebase(line.points.iter().copied().map(reflect).collect());
            line.points = points;
            moved_centre = centre;
        }
        Kind::Other(kind) if kind == "freedraw" => {
            let points = element.freedraw().map(|pen| pen.points).unwrap_or_default();
            let (points, centre) = rebase(points.into_iter().map(reflect).collect());
            let json: Vec<serde_json::Value> = points
                .iter()
                .map(|&[x, y]| {
                    serde_json::Value::Array(vec![
                        crate::scene::js_number(x),
                        crate::scene::js_number(y),
                    ])
                })
                .collect();
            element.json_mut().insert("points".into(), json.into());
            moved_centre = centre;
        }
        Kind::Other(kind) if kind == "image" => {
            let [sx, sy] = element.image_flip().map(|f| if f { -1 } else { 1 });
            let scale = if flip_x { [-sx, sy] } else { [sx, -sy] };
            element
                .json_mut()
                .insert("scale".into(), serde_json::json!(scale));
        }
        _ => {}
    }
    element.base.x = target[0] - moved_centre[0];
    element.base.y = target[1] - moved_centre[1];
}
