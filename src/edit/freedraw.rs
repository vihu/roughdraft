//! Drawing with the pen: a freehand stroke that follows the pointer
//! (REFERENCE-001 section 16).
use serde_json::{Value, json};

use super::create::{CONFIRM_THRESHOLD, distance};
use super::{Editor, Gesture};
use crate::geometry::Point;
use crate::scene::{Element, Kind};

/// Added to both coordinates of a click's second point, so a dot is not
/// zero-sized ("Allows dots to avoid being flagged as infinitely small").
const DOT: f64 = 0.0001;

impl Editor {
    /// A stroke of one point at `at` (`newFreeDrawElement`). A mouse has no
    /// pressure, so the pen simulates it from the drawing speed.
    pub(super) fn new_stroke(&self, at: Point) -> Element {
        let mut stroke = Element::new(Kind::Other("freedraw".into()), self.style.base(at));
        let json = stroke.json_mut();
        json.insert("points".into(), json!([[0.0, 0.0]]));
        json.insert("pressures".into(), json!([]));
        json.insert("simulatePressure".into(), true.into());
        json.insert("lastCommittedPoint".into(), Value::Null);
        stroke
    }

    /// Adds the pointer to the stroke being drawn, unless it has not moved
    /// since the last point. Appends in place, so a point costs the same
    /// however long the stroke is.
    pub(super) fn extend_stroke(&mut self, at: Point) {
        let Some(Gesture::Freedraw { index, bounds, .. }) = &mut self.gesture else {
            return;
        };
        let stroke = &mut self.scene.elements[*index];
        let point = [at[0] - stroke.base.x, at[1] - stroke.base.y];
        let points = stroke
            .json_mut()
            .get_mut("points")
            .and_then(Value::as_array_mut)
            .expect("a stroke has points");
        let last = points
            .last()
            .and_then(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?]));
        if last == Some(point) {
            return;
        }
        points.push(json!(point));
        *bounds = [
            bounds[0].min(point[0]),
            bounds[1].min(point[1]),
            bounds[2].max(point[0]),
            bounds[3].max(point[1]),
        ];
        stroke.base.width = bounds[2] - bounds[0];
        stroke.base.height = bounds[3] - bounds[1];
        stroke.touch();
    }

    /// Ends the stroke at `at` (`handleCanvasPointerUp`, `actionFinalize`):
    /// a release on the first point leaves a dot, and a stroke that ends
    /// within 8 screen px of its start closes into a loop. The stroke is
    /// not selected and the pen stays the tool.
    pub(super) fn finish_stroke(&mut self, index: usize, at: Point, before: Vec<Element>) {
        let (mut point, mut points) = self.stroke_and(index, at);
        if point == points[0] {
            point = [point[0] + DOT, point[1] + DOT];
        }
        points.push(point);
        let last = points.len() - 1;
        if points.len() >= 3 && distance(points[0], points[last]) <= CONFIRM_THRESHOLD / self.zoom {
            points[last] = points[0];
        }
        self.set_points(index, points);
        self.scene.elements[index]
            .json_mut()
            .insert("lastCommittedPoint".into(), json!(point));
        self.history.record(before);
    }
}

// Private API
impl Editor {
    /// `at` relative to the stroke's origin, and the stroke's points.
    fn stroke_and(&self, index: usize, at: Point) -> (Point, Vec<Point>) {
        let stroke = &self.scene.elements[index];
        let points = stroke.freedraw().expect("the pen draws freedraw").points;
        ([at[0] - stroke.base.x, at[1] - stroke.base.y], points)
    }
}
