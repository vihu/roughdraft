//! Resize, rotate and endpoint math behind the transform handles
//! (REFERENCE-001 section 5; `element/resizeElements.ts`).
use std::f64::consts::{PI, TAU};

use super::transform::{Frame, Handle};
use super::{Editor, Modifiers};
use crate::geometry::{self, Affine, Bounds, Point};
use crate::hit::container_id;
use crate::scene::{Element, Kind, TextAlign, VerticalAlign};

/// Shift snaps rotation to multiples of this (`SHIFT_LOCKING_ANGLE`).
pub(super) const LOCK_ANGLE: f64 = PI / 12.0;

/// Gap between a container's edge and its label (`BOUND_TEXT_PADDING`).
const LABEL_PADDING: f64 = 5.0;

impl Editor {
    /// Resizes one element (and re-centres its label). The pointer is read in
    /// the element's unrotated frame, so the opposite edge stays put.
    pub(super) fn resize_one(
        &mut self,
        start: &[(usize, Element)],
        frame: &Frame,
        handle: Handle,
        at: Point,
        offset: Point,
        modifiers: Modifiers,
    ) {
        let Some((index, original)) = start.iter().find(|(_, e)| container_id(e).is_none()) else {
            return;
        };
        let is_text = matches!(original.kind, Kind::Text(_));
        // Images keep their aspect ratio unless Shift is held.
        let is_image = original.file_id().is_some();
        let local = frame.transform.inverse().apply(at);
        let pointer = [local[0] - offset[0], local[1] - offset[1]];
        if is_text && matches!(handle, Handle::E | Handle::W) {
            self.resize_text_width(*index, original, frame, handle, pointer);
            return;
        }
        let [x1, y1, x2, y2] = frame.bounds;
        let bounds = resized_box(
            [x1, y1, x2, y2],
            handle,
            pointer,
            modifiers.alt,
            (modifiers.shift != is_image) || is_text,
        );
        if bounds[2] == bounds[0] && bounds[3] == bounds[1] {
            return;
        }
        let resized = scale_element(original, frame, bounds);
        self.scene.elements[*index] = resized;
        self.scene.elements[*index].touch();
        // The label re-wraps to the new width; the shape grows to fit it,
        // upward when pulled by a top handle (`handleBindTextResize`).
        // ponytail: no minimum size and no font scaling with Shift, which
        // Excalidraw applies to labelled shapes.
        let from_top = matches!(handle, Handle::N | Handle::Nw | Handle::Ne);
        for (label, _) in start.iter().filter(|(_, e)| container_id(e).is_some()) {
            self.rewrap_label(*label, from_top);
        }
    }

    /// Scales several elements about the opposite side or corner of their
    /// common box (its centre with Alt). Uniform with Shift, or when any
    /// element is rotated, text or grouped; uniform scaling scales label
    /// fonts too. Labels re-wrap and their shapes grow to fit.
    pub(super) fn resize_many(
        &mut self,
        start: &[(usize, Element)],
        frame: &Frame,
        handle: Handle,
        at: Point,
        offset: Point,
        modifiers: Modifiers,
    ) {
        // Where the dragged edge would be, not the pointer on the handle
        // (`pointerDownState.resize.offset`).
        let at = [at[0] - offset[0], at[1] - offset[1]];
        let [x1, y1, x2, y2] = frame.bounds;
        let (moves_left, moves_right, moves_top, moves_bottom) = edges(handle);
        let center = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
        let anchor = if modifiers.alt {
            center
        } else {
            [
                if moves_left { x2 } else { x1 },
                if moves_top { y2 } else { y1 },
            ]
        };
        let axis = |moves: bool, edge: f64, origin: f64, pointer: f64| {
            if !moves || edge == origin {
                1.0
            } else {
                (pointer - origin) / (edge - origin)
            }
        };
        let mut sx = axis(
            moves_left || moves_right,
            if moves_left { x1 } else { x2 },
            anchor[0],
            at[0],
        );
        let mut sy = axis(
            moves_top || moves_bottom,
            if moves_top { y1 } else { y2 },
            anchor[1],
            at[1],
        );
        let uniform = modifiers.shift
            || start.iter().any(|(_, e)| {
                e.base.angle != 0.0
                    || (matches!(e.kind, Kind::Text(_)) && container_id(e).is_none())
                    || !e.group_ids().is_empty()
            });
        if uniform {
            let side = matches!(handle, Handle::N | Handle::S | Handle::W | Handle::E);
            let scale = if side {
                if sx != 1.0 { sx.abs() } else { sy.abs() }
            } else {
                sx.abs().max(sy.abs())
            };
            (sx, sy) = (scale, scale);
        }
        if sx == 0.0 || sy == 0.0 {
            return;
        }
        let map = |[x, y]: Point| {
            [
                anchor[0] + (x - anchor[0]) * sx,
                anchor[1] + (y - anchor[1]) * sy,
            ]
        };
        for (index, original) in start.iter().filter(|(_, e)| container_id(e).is_none()) {
            let element_frame = Frame {
                bounds: geometry::local_bounds(original),
                transform: geometry::element_transform(original),
                margin: 0.0,
            };
            let [lx1, ly1, lx2, ly2] = element_frame.bounds;
            let resized = if original.base.angle == 0.0 {
                // Unrotated: map the box corners, keeping any flip.
                let [ax, ay] = map(element_frame.transform.apply([lx1, ly1]));
                let [bx, by] = map(element_frame.transform.apply([lx2, ly2]));
                let to_local = element_frame.transform.inverse();
                let ([ax, ay], [bx, by]) = (to_local.apply([ax, ay]), to_local.apply([bx, by]));
                scale_element(original, &element_frame, [ax, ay, bx, by])
            } else {
                // Rotated (uniform): move the centre, scale the box about it.
                let c = element_frame
                    .transform
                    .apply([(lx1 + lx2) / 2.0, (ly1 + ly2) / 2.0]);
                let [nx, ny] = map(c);
                let (hw, hh) = ((lx2 - lx1) * sx.abs() / 2.0, (ly2 - ly1) * sy.abs() / 2.0);
                let to_local = element_frame.transform.inverse();
                let [cx, cy] = to_local.apply([nx, ny]);
                scale_element(
                    original,
                    &element_frame,
                    [cx - hw, cy - hh, cx + hw, cy + hh],
                )
            };
            self.scene.elements[*index] = resized;
            self.scene.elements[*index].touch();
        }
        let from_top = matches!(handle, Handle::N | Handle::Nw | Handle::Ne);
        for (label, original) in start.iter().filter(|(_, e)| container_id(e).is_some()) {
            if let (true, Kind::Text(from), Kind::Text(to)) = (
                uniform,
                &original.kind,
                &mut self.scene.elements[*label].kind,
            ) {
                to.font_size = from.font_size * sx.abs();
            }
            self.rewrap_label(*label, from_top);
        }
    }

    /// Rotates the selection: one element about its centre, several about
    /// their common centre, each keeping its own angle offset.
    pub(super) fn rotate(&mut self, start: &[(usize, Element)], center: Point, angle: f64) {
        let single = start
            .iter()
            .filter(|(_, e)| container_id(e).is_none())
            .count()
            == 1;
        for (index, original) in start.iter().filter(|(_, e)| container_id(e).is_none()) {
            let element = &mut self.scene.elements[*index];
            if single {
                element.base.angle = angle;
            } else {
                let [lx1, ly1, lx2, ly2] = geometry::local_bounds(original);
                let c = geometry::element_transform(original)
                    .apply([(lx1 + lx2) / 2.0, (ly1 + ly2) / 2.0]);
                let [nx, ny] = Affine::rotate_about(angle, center).apply(c);
                element.base.x = original.base.x + nx - c[0];
                element.base.y = original.base.y + ny - c[1];
                element.base.angle = normalize_angle(original.base.angle + angle);
            }
            element.touch();
        }
        self.sync_labels(start);
    }

    /// Moves one end of a 2-point line; Shift snaps its angle. Points are
    /// re-based so the first stays at the element's `x`/`y`.
    pub(super) fn drag_endpoint(
        &mut self,
        index: usize,
        which: usize,
        at: Point,
        modifiers: Modifiers,
    ) {
        let element = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
            return;
        };
        let mut points = line.points.clone();
        let local = geometry::element_transform(element).inverse().apply(at);
        // Shift snaps the angle to the neighbour before it (after it for the
        // first point).
        let other = points[if which == 0 { 1 } else { which - 1 }];
        let offset =
            super::create::lock_angle([local[0] - other[0], local[1] - other[1]], modifiers.shift);
        points[which] = [other[0] + offset[0], other[1] + offset[1]];
        self.rebase_points(index, points);
    }

    /// Inserts a point, given in scene units, before position `at` of the
    /// line or arrow at `index`.
    pub(super) fn insert_point(&mut self, index: usize, at: usize, point: Point) {
        let element = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
            return;
        };
        let mut points = line.points.clone();
        points.insert(
            at,
            geometry::element_transform(element).inverse().apply(point),
        );
        self.rebase_points(index, points);
    }

    /// Sets local points, moving `x`/`y` so the first point stays at
    /// `[0, 0]`, and keeps any label on the line's middle.
    pub(super) fn rebase_points(&mut self, index: usize, points: Vec<Point>) {
        let shift = points[0];
        let points: Vec<Point> = points
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1]])
            .collect();
        let element = &mut self.scene.elements[index];
        element.base.x += shift[0];
        element.base.y += shift[1];
        self.set_points(index, points);
        let id = self.scene.elements[index].base.id.clone();
        let labels: Vec<(usize, Element)> = self
            .scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| container_id(e) == Some(id.as_str()))
            .map(|(i, e)| (i, e.clone()))
            .collect();
        self.sync_labels(&labels);
    }

    /// Puts labels back inside their containers after a resize or rotation:
    /// aligned per `textAlign`/`verticalAlign`, same angle; arrow labels on
    /// the arrow's middle.
    // ponytail: aligns within the container's full box; Excalidraw insets
    // top/bottom/left/right-aligned labels further in ellipses and diamonds
    pub(super) fn sync_labels(&mut self, start: &[(usize, Element)]) {
        for (label_index, _) in start.iter().filter(|(_, e)| container_id(e).is_some()) {
            let label = &self.scene.elements[*label_index];
            let Some(container) = container_id(label).and_then(|id| {
                self.scene
                    .elements
                    .iter()
                    .find(|e| e.base.id == id && !e.base.is_deleted)
            }) else {
                continue;
            };
            let Kind::Text(text) = &label.kind else {
                continue;
            };
            let (w, h) = (label.base.width, label.base.height);
            let is_arrow = matches!(container.kind, Kind::Arrow(_));
            let (x, y) = if let Kind::Arrow(line) = &container.kind {
                let [mx, my] = arrow_label_centre(container, &line.points);
                (mx - w / 2.0, my - h / 2.0)
            } else {
                let b = &container.base;
                let [x1, y1, x2, y2] = [b.x, b.y, b.x + b.width, b.y + b.height];
                let x = match &text.text_align {
                    TextAlign::Left => x1 + LABEL_PADDING,
                    TextAlign::Right => x2 - LABEL_PADDING - w,
                    _ => (x1 + x2 - w) / 2.0,
                };
                let y = match &text.vertical_align {
                    VerticalAlign::Top => y1 + LABEL_PADDING,
                    VerticalAlign::Bottom => y2 - LABEL_PADDING - h,
                    _ => (y1 + y2 - h) / 2.0,
                };
                (x, y)
            };
            let angle = if is_arrow {
                label.base.angle
            } else {
                container.base.angle
            };
            let label = &mut self.scene.elements[*label_index];
            if (label.base.x, label.base.y, label.base.angle) != (x, y, angle) {
                (label.base.x, label.base.y, label.base.angle) = (x, y, angle);
                label.touch();
            }
        }
    }
}

/// Which box edges a handle moves: (left, right, top, bottom).
pub(super) fn edges(handle: Handle) -> (bool, bool, bool, bool) {
    match handle {
        Handle::N => (false, false, true, false),
        Handle::S => (false, false, false, true),
        Handle::W => (true, false, false, false),
        Handle::E => (false, true, false, false),
        Handle::Nw => (true, false, true, false),
        Handle::Ne => (false, true, true, false),
        Handle::Sw => (true, false, false, true),
        Handle::Se => (false, true, false, true),
        Handle::Rotation => (false, false, false, false),
    }
}

/// The box after dragging `handle` to `pointer` (both in the box's frame).
/// Alt mirrors the moving edges about the centre; `keep_aspect` scales both
/// axes by the larger ratio (corners) or the dragged axis (edges, centred).
/// The result may be flipped (x2 < x1).
fn resized_box(
    bounds: Bounds,
    handle: Handle,
    pointer: Point,
    alt: bool,
    keep_aspect: bool,
) -> Bounds {
    let [ox1, oy1, ox2, oy2] = bounds;
    let (w0, h0) = (ox2 - ox1, oy2 - oy1);
    let center = [(ox1 + ox2) / 2.0, (oy1 + oy2) / 2.0];
    let (moves_left, moves_right, moves_top, moves_bottom) = edges(handle);
    let [mut x1, mut y1, mut x2, mut y2] = bounds;
    if moves_left {
        x1 = pointer[0];
    }
    if moves_right {
        x2 = pointer[0];
    }
    if moves_top {
        y1 = pointer[1];
    }
    if moves_bottom {
        y2 = pointer[1];
    }
    if alt {
        if moves_left {
            x2 = 2.0 * center[0] - x1;
        }
        if moves_right {
            x1 = 2.0 * center[0] - x2;
        }
        if moves_top {
            y2 = 2.0 * center[1] - y1;
        }
        if moves_bottom {
            y1 = 2.0 * center[1] - y2;
        }
    }
    if keep_aspect && w0 != 0.0 && h0 != 0.0 {
        let (w, h) = (x2 - x1, y2 - y1);
        let corner = (moves_left || moves_right) && (moves_top || moves_bottom);
        let ratio = if corner {
            (w.abs() / w0.abs()).max(h.abs() / h0.abs())
        } else if moves_left || moves_right {
            w.abs() / w0.abs()
        } else {
            h.abs() / h0.abs()
        };
        let (w, h) = (w0 * ratio * sign(w), h0 * ratio * sign(h));
        let place =
            |moves_low: bool, moves_high: bool, low: f64, high: f64, size: f64, mid: f64| match (
                alt, moves_low, moves_high,
            ) {
                (true, _, _) | (false, false, false) => (mid - size / 2.0, mid + size / 2.0),
                (false, true, _) => (high - size, high),
                (false, false, true) => (low, low + size),
            };
        (x1, x2) = place(moves_left, moves_right, x1, x2, w, center[0]);
        (y1, y2) = place(moves_top, moves_bottom, y1, y2, h, center[1]);
    }
    [x1, y1, x2, y2]
}

fn sign(value: f64) -> f64 {
    if value < 0.0 { -1.0 } else { 1.0 }
}

/// Returns `original` resized so its local box becomes `bounds` (in the
/// original's local frame, possibly flipped). Lines scale their points,
/// text scales its font size.
fn scale_element(original: &Element, frame: &Frame, bounds: Bounds) -> Element {
    let [x1, y1, x2, y2] = bounds;
    let [ox1, oy1, ox2, oy2] = geometry::local_bounds(original);
    let (w0, h0) = (ox2 - ox1, oy2 - oy1);
    let center = frame.transform.apply([(x1 + x2) / 2.0, (y1 + y2) / 2.0]);
    let mut element = original.clone();
    match &mut element.kind {
        Kind::Line(line) | Kind::Arrow(line) => {
            let sx = if w0 == 0.0 { 1.0 } else { (x2 - x1) / w0 };
            let sy = if h0 == 0.0 { 1.0 } else { (y2 - y1) / h0 };
            let mid = [(ox1 + ox2) / 2.0, (oy1 + oy2) / 2.0];
            let scaled: Vec<Point> = line
                .points
                .iter()
                .map(|p| [(p[0] - mid[0]) * sx, (p[1] - mid[1]) * sy])
                .collect();
            let first = scaled[0];
            line.points = scaled
                .iter()
                .map(|p| [p[0] - first[0], p[1] - first[1]])
                .collect();
            element.base.x = center[0] + first[0];
            element.base.y = center[1] + first[1];
            element.base.width = (w0 * sx).abs();
            element.base.height = (h0 * sy).abs();
        }
        Kind::Other(kind) if kind == "freedraw" => {
            // Scaled about the box like a line's points, first point at x/y.
            let sx = if w0 == 0.0 { 1.0 } else { (x2 - x1) / w0 };
            let sy = if h0 == 0.0 { 1.0 } else { (y2 - y1) / h0 };
            let mid = [(ox1 + ox2) / 2.0, (oy1 + oy2) / 2.0];
            let points = original
                .freedraw()
                .map(|pen| pen.points)
                .unwrap_or_default();
            let scaled: Vec<Point> = points
                .iter()
                .map(|p| [(p[0] - mid[0]) * sx, (p[1] - mid[1]) * sy])
                .collect();
            let first = scaled.first().copied().unwrap_or_default();
            let points: Vec<serde_json::Value> = scaled
                .iter()
                .map(|p| {
                    let x = crate::scene::js_number(p[0] - first[0]);
                    serde_json::Value::Array(vec![x, crate::scene::js_number(p[1] - first[1])])
                })
                .collect();
            element.json_mut().insert("points".into(), points.into());
            element.base.x = center[0] + first[0];
            element.base.y = center[1] + first[1];
            element.base.width = (w0 * sx).abs();
            element.base.height = (h0 * sy).abs();
        }
        kind => {
            let (w, h) = ((x2 - x1).abs(), (y2 - y1).abs());
            if let Kind::Text(text) = kind
                && w0 != 0.0
            {
                text.font_size *= w / w0;
            }
            element.base.x = center[0] - w / 2.0;
            element.base.y = center[1] - h / 2.0;
            element.base.width = w;
            element.base.height = h;
        }
    }
    element
}

/// Maps an angle into `[0, 2π)`.
pub(super) fn normalize_angle(angle: f64) -> f64 {
    angle.rem_euclid(TAU)
}

/// Where an arrow's label is centred (`getBoundTextElementPosition`): its
/// middle point, or the middle of its middle segment.
// ponytail: straight segment midpoint; Excalidraw takes the curve's
// midpoint on round arrows with an even number of 4+ points.
fn arrow_label_centre(arrow: &Element, points: &[Point]) -> Point {
    let transform = geometry::element_transform(arrow);
    let n = points.len();
    if n % 2 == 1 {
        return transform.apply(points[n / 2]);
    }
    let (a, b) = (
        transform.apply(points[n / 2 - 1]),
        transform.apply(points[n / 2]),
    );
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}
