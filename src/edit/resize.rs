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

/// Smallest font a Shift resize scales a label to (`MIN_FONT_SIZE`).
const MIN_FONT_SIZE: f64 = 1.0;

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
        let keep_aspect = (modifiers.shift != is_image) || is_text;
        let label = start.iter().find(|(_, e)| container_id(e).is_some());
        let mut bounds = resized_box(frame.bounds, handle, pointer, modifiers.alt, keep_aspect);
        // A labelled shape keeps room for one character on one line, unless
        // Shift scales its label instead (`resizeSingleElement`).
        if let Some((_, label)) = label
            && !keep_aspect
        {
            bounds = at_least(bounds, handle, modifiers.alt, self.min_label_box(label));
        }
        if bounds[2] == bounds[0] && bounds[3] == bounds[1] {
            return;
        }
        // Free text scales by the larger of the two axis ratios and stops
        // at font size 1 (`resizeSingleTextElement`): past its opposite
        // corner on both axes it stops, on one it keeps growing unflipped.
        if let Kind::Text(text) = &original.kind {
            let flipped = [bounds[2] < bounds[0], bounds[3] < bounds[1]];
            if flipped == [true, true] {
                return;
            }
            if flipped.contains(&true) {
                bounds = unflip(bounds, frame.bounds, handle);
            }
            let [x1, _, x2, _] = frame.bounds;
            let size = text.font_size * (bounds[2] - bounds[0]) / (x2 - x1);
            if size.is_nan() || size < MIN_FONT_SIZE {
                return;
            }
        }
        let resized = scale_element(original, frame, bounds);
        // With Shift the label's font follows the room it has
        // (`measureFontSizeFromWidth`; an arrow's width for arrows);
        // without it the font is the one at the press.
        let label_font = label.and_then(|(_, e)| match &e.kind {
            Kind::Text(text) => Some(text.font_size),
            _ => None,
        });
        let font_size = match label_font {
            Some(size) if keep_aspect => {
                let ratio = if matches!(original.kind, Kind::Arrow(_)) {
                    resized.base.width / original.base.width
                } else {
                    super::text::max_label_width(&resized, size)
                        / super::text::max_label_width(original, size)
                };
                let scaled = size * ratio;
                if !scaled.is_finite() || scaled < MIN_FONT_SIZE {
                    return;
                }
                Some(scaled)
            }
            other => other,
        };
        self.scene.elements[*index] = resized;
        self.scene.elements[*index].touch();
        // The label re-wraps to the new width; the shape grows to fit it,
        // upward when pulled by a top handle (`handleBindTextResize`).
        let from_top = matches!(handle, Handle::N | Handle::Nw | Handle::Ne);
        if let Some((label, _)) = label {
            if let (Some(size), Kind::Text(text)) =
                (font_size, &mut self.scene.elements[*label].kind)
            {
                text.font_size = size;
            }
            self.rewrap_label(*label, from_top);
        }
    }

    /// Smallest box a labelled shape keeps (`getApproxMinLineWidth`,
    /// `getApproxMinLineHeight`): one character by one line, plus padding.
    fn min_label_box(&self, label: &Element) -> [f64; 2] {
        let Kind::Text(text) = &label.kind else {
            return [0.0, 0.0];
        };
        let widest = label
            .original_text()
            .chars()
            .map(|c| {
                self.measure
                    .line_width(&c.to_string(), text.font_family, text.font_size)
            })
            .fold(0.0, f64::max);
        [
            widest + 2.0 * LABEL_PADDING,
            text.font_size * text.line_height + 2.0 * LABEL_PADDING,
        ]
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
        // Frames never rotate (`rotateSingleElement`, `rotateMultipleElements`).
        let turns = |e: &Element| container_id(e).is_none() && e.frame_title().is_none();
        for (index, original) in start.iter().filter(|(_, e)| turns(e)) {
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

/// A resized box with both extents positive, kept against the sides the
/// handle does not drag (of `original`).
fn unflip(bounds: Bounds, original: Bounds, handle: Handle) -> Bounds {
    let (moves_left, moves_right, moves_top, moves_bottom) = edges(handle);
    let (w, h) = ((bounds[2] - bounds[0]).abs(), (bounds[3] - bounds[1]).abs());
    let [ox1, oy1, ox2, oy2] = original;
    let (x1, x2) = match (moves_left, moves_right) {
        (true, false) => (ox2 - w, ox2),
        (false, true) => (ox1, ox1 + w),
        _ => (bounds[0].min(bounds[2]), bounds[0].max(bounds[2])),
    };
    let (y1, y2) = match (moves_top, moves_bottom) {
        (true, false) => (oy2 - h, oy2),
        (false, true) => (oy1, oy1 + h),
        _ => (bounds[1].min(bounds[3]), bounds[1].max(bounds[3])),
    };
    [x1, y1, x2, y2]
}

/// Grows a resized box to at least `min` wide and high from the sides the
/// handle drags (about the centre with Alt), which also undoes a flip, like
/// `Math.max(nextWidth, minWidth)`.
fn at_least(bounds: Bounds, handle: Handle, alt: bool, [min_w, min_h]: [f64; 2]) -> Bounds {
    let (moves_left, moves_right, moves_top, moves_bottom) = edges(handle);
    let fit = |low: f64, high: f64, moves_low: bool, moves_high: bool, min: f64| {
        if high - low >= min {
            (low, high)
        } else if alt {
            let mid = (low + high) / 2.0;
            (mid - min / 2.0, mid + min / 2.0)
        } else if moves_low {
            (high - min, high)
        } else if moves_high {
            (low, low + min)
        } else {
            // A side not being dragged grows about its middle.
            let mid = (low + high) / 2.0;
            (mid - min / 2.0, mid + min / 2.0)
        }
    };
    let [x1, y1, x2, y2] = bounds;
    let (x1, x2) = fit(x1, x2, moves_left, moves_right, min_w);
    let (y1, y2) = fit(y1, y2, moves_top, moves_bottom, min_h);
    [x1, y1, x2, y2]
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
            let first = scaled.first().copied().unwrap_or_default();
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
    // Pulled past its opposite side, an image mirrors (`scale` times the
    // sign of the new size in `resizeSingleElement`).
    if element.file_id().is_some() && (x2 < x1 || y2 < y1) {
        let [fx, fy] = original.image_flip();
        let sign = |mirrored: bool| if mirrored { -1 } else { 1 };
        element.json_mut().insert(
            "scale".into(),
            serde_json::json!([sign(fx != (x2 < x1)), sign(fy != (y2 < y1))]),
        );
    }
    element
}

/// Maps an angle into `[0, 2π)`.
pub(super) fn normalize_angle(angle: f64) -> f64 {
    angle.rem_euclid(TAU)
}

/// Where an arrow's label is centred (`getBoundTextElementPosition`): its
/// middle point, or the middle of its middle segment (on the curve of a
/// round arrow).
fn arrow_label_centre(arrow: &Element, points: &[Point]) -> Point {
    let transform = geometry::element_transform(arrow);
    let n = points.len();
    if n % 2 == 1 {
        return transform.apply(points[n / 2]);
    }
    transform.apply(crate::render::segment_midpoint(arrow, n / 2))
}
