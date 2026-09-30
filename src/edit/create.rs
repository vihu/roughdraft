//! Drawing new shapes, lines and arrows (REFERENCE-001 sections 11-12); the
//! pen's strokes are in `freedraw.rs`.
use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, PI};

use super::{Editor, Gesture, Modifiers, Multi, Tool};
use crate::geometry::Point;
use crate::scene::{Element, FillStyle, Kind, Linear, Roundness, StrokeStyle};

/// Scene units a line press must travel before it counts as a drag
/// (`DRAGGING_THRESHOLD`).
const DRAG_THRESHOLD: f64 = 10.0;

/// Scene units around the last point where a click finishes a multi-point
/// line, and around the first point where it closes a loop
/// (`LINE_CONFIRM_THRESHOLD`).
pub(super) const CONFIRM_THRESHOLD: f64 = 8.0;

/// Shift snaps line angles to multiples of this (`SHIFT_LOCKING_ANGLE`).
const LOCK_ANGLE: f64 = PI / 12.0;

/// `ROUNDNESS.PROPORTIONAL_RADIUS`.
const PROPORTIONAL: u8 = 2;

/// `ROUNDNESS.ADAPTIVE_RADIUS`.
const ADAPTIVE: u8 = 3;

impl Editor {
    pub(super) fn create_press(&mut self, at: Point, _modifiers: Modifiers) {
        if self.multi.is_some() {
            return self.multi_click(at);
        }
        let before = self.scene.elements.clone();
        let index = self.scene.elements.len();
        let Some(element) = self.new_element(at) else {
            return;
        };
        let gesture = match element.kind {
            Kind::Line(_) | Kind::Arrow(_) => Gesture::Line {
                index,
                origin: at,
                before,
                dragged: false,
            },
            _ if self.tool == Tool::Freedraw => Gesture::Freedraw {
                index,
                before,
                bounds: [0.0; 4],
            },
            _ => Gesture::Shape {
                index,
                origin: at,
                before,
            },
        };
        self.scene.elements.push(element);
        self.gesture = Some(gesture);
    }

    /// Continues drawing; returns `false` when not drawing.
    pub(super) fn create_drag(&mut self, at: Point, modifiers: Modifiers) -> bool {
        match &mut self.gesture {
            Some(Gesture::Shape { index, origin, .. }) => {
                let (index, origin) = (*index, *origin);
                self.drag_shape(index, origin, at, modifiers);
                true
            }
            Some(Gesture::Line {
                index,
                origin,
                dragged,
                ..
            }) => {
                let offset = [at[0] - origin[0], at[1] - origin[1]];
                if !*dragged && offset[0].hypot(offset[1]) < DRAG_THRESHOLD {
                    return true;
                }
                *dragged = true;
                let index = *index;
                self.set_points(index, vec![[0.0, 0.0], lock_angle(offset, modifiers.shift)]);
                true
            }
            Some(Gesture::Freedraw { .. }) => {
                self.extend_stroke(at);
                true
            }
            _ => false,
        }
    }

    /// Ends a press while drawing; returns `false` when not drawing.
    pub(super) fn create_release(&mut self, at: Point) -> bool {
        match self.gesture.take() {
            Some(Gesture::Shape { index, before, .. }) => {
                let base = &self.scene.elements[index].base;
                if base.width == 0.0 && base.height == 0.0 {
                    // A click without a drag draws nothing; the tool stays.
                    self.scene.elements.remove(index);
                } else {
                    if self.tool == Tool::Frame {
                        self.adopt_into_frame(index);
                    }
                    self.finish(index, before);
                }
            }
            Some(Gesture::Line {
                index,
                before,
                dragged: true,
                ..
            }) => self.finish(index, before),
            Some(Gesture::Line {
                index,
                before,
                origin,
                dragged: false,
            }) => {
                // No drag: switch to click-by-click drawing with a floating point.
                self.set_points(
                    index,
                    vec![[0.0, 0.0], [at[0] - origin[0], at[1] - origin[1]]],
                );
                self.multi = Some(Multi { index, before });
            }
            Some(Gesture::Freedraw { index, before, .. }) => self.finish_stroke(index, at, before),
            other => {
                self.gesture = other;
                return false;
            }
        }
        true
    }

    /// Moves the floating last point of a multi-point line to the pointer.
    pub(super) fn multi_hover(&mut self, at: Point, modifiers: Modifiers) {
        let Some(index) = self.multi.as_ref().map(|m| m.index) else {
            return;
        };
        let (origin, mut points) = self.origin_and_points(index);
        let committed = points[points.len() - 2];
        let offset = [
            at[0] - origin[0] - committed[0],
            at[1] - origin[1] - committed[1],
        ];
        let offset = lock_angle(offset, modifiers.shift);
        let last = points.len() - 1;
        points[last] = [committed[0] + offset[0], committed[1] + offset[1]];
        self.set_points(index, points);
    }

    /// Ends a multi-point line: drops the floating point, and removes the
    /// line when fewer than two points remain.
    pub(super) fn finish_multi(&mut self) {
        let Some(Multi { index, before }) = self.multi.take() else {
            return;
        };
        let (_, mut points) = self.origin_and_points(index);
        points.pop();
        if points.len() < 2 {
            self.scene.elements.remove(index);
        } else {
            self.set_points(index, points);
            self.finish(index, before);
        }
    }
}

// Private API
impl Editor {
    /// A new 0x0 element for the active tool at `at`, styled from the
    /// current style.
    fn new_element(&self, at: Point) -> Option<Element> {
        let style = &self.style;
        let mut base = style.base(at);
        let round = |kind| Some(Roundness { kind, value: None });
        let linear = |start, end| Linear {
            points: vec![[0.0, 0.0]],
            start_arrowhead: start,
            end_arrowhead: end,
        };
        let kind = match self.tool {
            Tool::Rectangle => {
                base.roundness = style.round_edges.then(|| round(ADAPTIVE)).flatten();
                Kind::Rectangle
            }
            Tool::Diamond | Tool::Ellipse => {
                base.roundness = style.round_edges.then(|| round(PROPORTIONAL)).flatten();
                if self.tool == Tool::Diamond {
                    Kind::Diamond
                } else {
                    Kind::Ellipse
                }
            }
            Tool::Line => {
                base.roundness = style.round_edges.then(|| round(PROPORTIONAL)).flatten();
                Kind::Line(linear(None, None))
            }
            Tool::Arrow => {
                base.roundness = style.round_arrows.then(|| round(PROPORTIONAL)).flatten();
                Kind::Arrow(linear(
                    style.start_arrowhead.clone(),
                    style.end_arrowhead.clone(),
                ))
            }
            // `FRAME_STYLE`; the name stays null, shown as "Frame".
            Tool::Frame => {
                base.stroke_color = "#bbb".into();
                base.background_color = "transparent".into();
                base.fill_style = FillStyle::Solid;
                base.stroke_width = 2.0;
                base.stroke_style = StrokeStyle::Solid;
                base.roughness = 0.0;
                let mut frame = Element::new(Kind::Other("frame".into()), base);
                frame
                    .json_mut()
                    .insert("name".into(), serde_json::Value::Null);
                return Some(frame);
            }
            Tool::Freedraw => return Some(self.new_stroke(at)),
            Tool::Selection | Tool::Hand | Tool::Text | Tool::Eraser => return None,
        };
        Some(Element::new(kind, base))
    }

    /// `dragNewElement`: the box between the press point and the pointer;
    /// Shift makes it square, Alt grows it from the press point.
    fn drag_shape(&mut self, index: usize, origin: Point, at: Point, modifiers: Modifiers) {
        let (dx, dy) = (at[0] - origin[0], at[1] - origin[1]);
        let (mut width, mut height) = (dx.abs(), dy.abs());
        if modifiers.shift {
            let size = width.max(height);
            (width, height) = (size, size);
        }
        let mut x = if dx < 0.0 {
            origin[0] - width
        } else {
            origin[0]
        };
        let mut y = if dy < 0.0 {
            origin[1] - height
        } else {
            origin[1]
        };
        if modifiers.alt {
            (x, y) = (origin[0] - width, origin[1] - height);
            (width, height) = (width * 2.0, height * 2.0);
        }
        if width == 0.0 || height == 0.0 {
            return;
        }
        let base = &mut self.scene.elements[index].base;
        (base.x, base.y, base.width, base.height) = (x, y, width, height);
        self.scene.elements[index].touch();
    }

    fn multi_click(&mut self, at: Point) {
        let Some(multi) = &self.multi else {
            return;
        };
        let index = multi.index;
        let (origin, mut points) = self.origin_and_points(index);
        let local = [at[0] - origin[0], at[1] - origin[1]];
        let last = points.len() - 1;
        let committed = points[last - 1];
        if distance(local, committed) < CONFIRM_THRESHOLD {
            return self.finish_multi();
        }
        let is_line = matches!(self.scene.elements[index].kind, Kind::Line(_));
        if is_line
            && points.len() >= 3
            && distance(local, points[0]) <= CONFIRM_THRESHOLD / self.zoom
        {
            // Close the loop on the first point.
            points[last] = points[0];
            self.set_points(index, points);
            let before = self.multi.take().expect("checked above").before;
            return self.finish(index, before);
        }
        // Commit the floating point where it is and float a new one.
        points.push(points[last]);
        self.set_points(index, points);
    }

    fn origin_and_points(&self, index: usize) -> (Point, Vec<Point>) {
        let element = &self.scene.elements[index];
        let points = match &element.kind {
            Kind::Line(line) | Kind::Arrow(line) => line.points.clone(),
            _ => unreachable!("multi-point drawing is for lines and arrows"),
        };
        ([element.base.x, element.base.y], points)
    }

    /// Replaces a line's or a pen stroke's points; width and height
    /// follow, like `mutateElement` does.
    pub(super) fn set_points(&mut self, index: usize, points: Vec<Point>) {
        let element = &mut self.scene.elements[index];
        let (xs, ys): (Vec<f64>, Vec<f64>) = points.iter().map(|[x, y]| (*x, *y)).unzip();
        let span = |v: &[f64]| {
            v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - v.iter().copied().fold(f64::INFINITY, f64::min)
        };
        element.base.width = span(&xs);
        element.base.height = span(&ys);
        if let Kind::Line(line) | Kind::Arrow(line) = &mut element.kind {
            line.points = points;
        } else {
            // A stroke keeps its points in its JSON (`Element::freedraw`).
            element
                .json_mut()
                .insert("points".into(), serde_json::json!(points));
        }
        element.touch();
    }

    /// Records a finished drawing, selects it, and returns to the selection
    /// tool unless the tool is locked.
    fn finish(&mut self, index: usize, before: Vec<Element>) {
        self.bind_arrow_ends(index);
        self.history.record(before);
        self.selected = HashSet::from([self.scene.elements[index].base.id.clone()]);
        if !self.locked {
            self.tool = Tool::Selection;
        }
    }
}

pub(super) fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// `getLockedLinearCursorAlignSize`: with Shift, snaps the direction to
/// 15-degree steps, keeping the horizontal distance.
pub(super) fn lock_angle([dx, dy]: Point, shift: bool) -> Point {
    if !shift || (dx == 0.0 && dy == 0.0) {
        return [dx, dy];
    }
    let locked = ((dy / dx).atan() / LOCK_ANGLE).round() * LOCK_ANGLE;
    if locked == 0.0 {
        [dx, 0.0]
    } else if locked.abs() == FRAC_PI_2 {
        [0.0, dy]
    } else {
        [dx, dx.abs() * locked.tan().abs() * dy.signum()]
    }
}
