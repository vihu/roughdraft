//! Hit testing, the way Excalidraw 0.18 picks the element under the pointer.
//!
//! Mirrors `element/collision.ts`: shapes that are "draggable from inside"
//! (filled, labelled, or text) hit anywhere inside; the rest only within the
//! threshold of their outline.
use crate::geometry::{self, Point};
use crate::render::is_transparent;
use crate::scene::{Element, Kind, Scene};

/// Screen pixels around an outline that still hit it
/// (`DEFAULT_COLLISION_THRESHOLD`); divide by the zoom for scene units.
pub const THRESHOLD: f64 = 8.0;

/// Returns the topmost element at `at`, within `threshold` scene units.
///
/// Labels are never returned: hitting a label hits its container. Locked
/// elements are skipped (`getElementsAtPosition`).
pub fn element_at(scene: &Scene, at: Point, threshold: f64) -> Option<&Element> {
    let order = crate::render::draw_order(scene);
    let label_of = |container: &Element| {
        order
            .iter()
            .copied()
            .find(|e| container_id(e) == Some(container.base.id.as_str()))
    };
    order
        .iter()
        .rev()
        .copied()
        // Locked elements are skipped, so what is below them can be hit.
        .filter(|e| container_id(e).is_none() && !e.is_locked())
        .find(|e| {
            let label = label_of(e);
            hits(e, label.is_some(), at, threshold) || label.is_some_and(|l| in_box(l, at, 0.0))
        })
}

/// Whether `at` lies in the element's rotated box grown by `pad` on each side.
pub fn in_box(element: &Element, at: Point, pad: f64) -> bool {
    let [x, y] = geometry::element_transform(element).inverse().apply(at);
    let [x1, y1, x2, y2] = geometry::local_bounds(element);
    x >= x1 - pad && x <= x2 + pad && y >= y1 - pad && y <= y2 + pad
}

/// Returns the id of the element a text label belongs to.
pub fn container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        Kind::Text(text) => text.container_id.as_deref(),
        _ => None,
    }
}

/// `hitElementItself`: inside (when draggable from inside) or near the outline.
fn hits(element: &Element, labelled: bool, at: Point, threshold: f64) -> bool {
    let p = geometry::element_transform(element).inverse().apply(at);
    let (w, h) = (element.base.width, element.base.height);
    let from_inside = !is_transparent(&element.base.background_color) || labelled;
    match &element.kind {
        Kind::Rectangle | Kind::Text(_) => {
            let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
            polygon_hit(
                &corners,
                true,
                p,
                threshold,
                from_inside || matches!(element.kind, Kind::Text(_)),
            )
        }
        Kind::Diamond => {
            let corners = [[w / 2.0, 0.0], [w, h / 2.0], [w / 2.0, h], [0.0, h / 2.0]];
            polygon_hit(&corners, true, p, threshold, from_inside)
        }
        Kind::Ellipse => ellipse_hit(
            [w / 2.0, h / 2.0],
            w / 2.0,
            h / 2.0,
            p,
            threshold,
            from_inside,
        ),
        // ponytail: straight segments between points, Excalidraw samples the rough curve; differs on curved lines
        Kind::Line(line) => {
            let loop_ = crate::render::is_loop(&line.points);
            polygon_hit(&line.points, false, p, threshold, from_inside && loop_)
        }
        Kind::Arrow(line) => polygon_hit(&line.points, false, p, threshold, false),
        Kind::Other(kind) if kind == "image" => {
            let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
            polygon_hit(&corners, true, p, threshold, true)
        }
        Kind::Other(_) => false,
    }
}

fn polygon_hit(
    points: &[Point],
    closed: bool,
    p: Point,
    threshold: f64,
    inside_counts: bool,
) -> bool {
    if inside_counts && contains(points, p) {
        return true;
    }
    let edges = points.windows(2).map(|w| (w[0], w[1]));
    let closing = closed.then(|| (points[points.len() - 1], points[0]));
    edges
        .chain(closing)
        .any(|(a, b)| segment_distance(p, a, b) < threshold)
}

fn ellipse_hit(
    center: Point,
    rx: f64,
    ry: f64,
    p: Point,
    threshold: f64,
    inside_counts: bool,
) -> bool {
    /// Outline samples; the outline is compared at this resolution.
    const SAMPLES: usize = 64;

    let [dx, dy] = [p[0] - center[0], p[1] - center[1]];
    if inside_counts && rx > 0.0 && ry > 0.0 && (dx / rx).powi(2) + (dy / ry).powi(2) <= 1.0 {
        return true;
    }
    let outline: Vec<Point> = (0..=SAMPLES)
        .map(|i| {
            let t = i as f64 / SAMPLES as f64 * std::f64::consts::TAU;
            [center[0] + rx * t.cos(), center[1] + ry * t.sin()]
        })
        .collect();
    polygon_hit(&outline, false, p, threshold, false)
}

/// Even-odd point-in-polygon.
fn contains(points: &[Point], [x, y]: Point) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for (i, &[xi, yi]) in points.iter().enumerate() {
        let [xj, yj] = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segment_distance([px, py]: Point, [ax, ay]: Point, [bx, by]: Point) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / length).clamp(0.0, 1.0)
    };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

#[cfg(test)]
mod tests {
    use super::element_at;
    use crate::scene::Scene;

    const BASE: &str = r##""y":0,"width":100,"height":60,"angle":0,"strokeColor":"#1e1e1e","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"isDeleted":false"##;

    fn scene(elements: &[String]) -> Scene {
        serde_json::from_str(&format!(r#"{{"elements":[{}]}}"#, elements.join(","))).unwrap()
    }

    fn at(scene: &Scene, p: [f64; 2]) -> Option<&str> {
        element_at(scene, p, 8.0).map(|e| e.base.id.as_str())
    }

    #[test]
    fn transparent_shapes_hit_on_the_outline_filled_inside() {
        let s = scene(&[
            format!(
                r##"{{"id":"box","type":"rectangle","x":0,"backgroundColor":"transparent",{BASE}}}"##
            ),
            format!(
                r##"{{"id":"dia","type":"diamond","x":200,"backgroundColor":"#ffc9c9",{BASE}}}"##
            ),
            format!(
                r##"{{"id":"ell","type":"ellipse","x":400,"backgroundColor":"transparent",{BASE}}}"##
            ),
        ]);
        assert_eq!(at(&s, [50.0, 30.0]), None, "empty middle of an outline box");
        assert_eq!(at(&s, [50.0, 5.0]), Some("box"));
        assert_eq!(at(&s, [250.0, 30.0]), Some("dia"), "filled diamond centre");
        assert_eq!(at(&s, [203.0, 3.0]), None, "diamond's empty corner");
        assert_eq!(
            at(&s, [450.0, 2.0]),
            Some("ell"),
            "top of the ellipse outline"
        );
        assert_eq!(at(&s, [450.0, 30.0]), None);
    }

    #[test]
    fn arrows_hit_near_segments_and_labels_hit_their_container() {
        let s = scene(&[
            format!(
                r##"{{"id":"arr","type":"arrow","x":0,"backgroundColor":"#ffc9c9",{BASE},"points":[[0,0],[100,60]]}}"##
            ),
            format!(
                r##"{{"id":"box","type":"rectangle","x":200,"backgroundColor":"transparent",{BASE}}}"##
            ),
            format!(
                r##"{{"id":"lbl","type":"text","x":230,"backgroundColor":"transparent",{BASE},"width":40,"height":25,"text":"x","fontSize":20,"fontFamily":5,"textAlign":"center","verticalAlign":"middle","lineHeight":1.25,"containerId":"box"}}"##
            ),
        ]);
        assert_eq!(at(&s, [50.0, 33.0]), Some("arr"));
        assert_eq!(at(&s, [80.0, 10.0]), None, "arrows never hit inside");
        assert_eq!(
            at(&s, [250.0, 10.0]),
            Some("box"),
            "label hits the container"
        );
        assert_eq!(
            at(&s, [210.0, 50.0]),
            Some("box"),
            "labelled box hits inside"
        );
    }
}
