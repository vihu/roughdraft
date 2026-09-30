//! Arrow binding (`element/binding.ts`): an arrow end attaches to a shape
//! with a `focus` (where its line passes the shape, -1 to 1 relative to the
//! centre) and a `gap` (distance from the outline), and follows the shape
//! when it moves, resizes or rotates. Geometry ported from Excalidraw 0.18.1
//! (MIT, Copyright (c) 2020 Excalidraw): `determineFocusDistance`,
//! `determineFocusPoint`, `updateBoundPoint`, `maxBindingGap`,
//! `distanceToBindableElement`, `intersectElementWithLineSegment`.
use std::collections::HashSet;
use std::f64::consts::{SQRT_2, TAU};

use super::{Editor, Gesture};
use crate::geometry::{self, Point};
use crate::scene::{ArrowEnd, Binding, BoundRef, Element, Kind};

/// `BINDING_HIGHLIGHT_THICKNESS`.
const HIGHLIGHT_THICKNESS: f64 = 10.0;

/// `BINDING_HIGHLIGHT_OFFSET`.
const HIGHLIGHT_OFFSET: f64 = 4.0;

/// `PRECISION` for points on segments.
const PRECISION: f64 = 10e-5;

/// Outline samples when measuring distance to an ellipse.
const ELLIPSE_SAMPLES: usize = 360;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outline {
    Box,
    Diamond,
    Ellipse,
}

// Public API
impl Editor {
    /// Returns the shape the moving end of the arrow being drawn or dragged
    /// would bind to, for Excalidraw's binding highlight.
    // ponytail: only the end under the pointer; Excalidraw also highlights
    // under the arrow tool before a press and while moving bound arrows.
    pub fn binding_suggestion(&self) -> Option<&Element> {
        let (index, end) = match (&self.gesture, &self.multi) {
            (_, Some(multi)) => (multi.index, ArrowEnd::End),
            (
                Some(Gesture::Line {
                    index,
                    dragged: true,
                    ..
                }),
                _,
            ) => (*index, ArrowEnd::End),
            (
                Some(Gesture::Endpoint {
                    index, which: 0, ..
                }),
                _,
            ) => (*index, ArrowEnd::Start),
            (Some(Gesture::Endpoint { index, .. }), _) => (*index, ArrowEnd::End),
            _ => return None,
        };
        self.binding_target(index, end)
    }
}

// Private API
impl Editor {
    /// Binds each end of the arrow at `index` to the shape under it, or
    /// unbinds it (`bindOrUnbindLinearElement`). Only arrows bind.
    pub(super) fn bind_arrow_ends(&mut self, index: usize) {
        for end in [ArrowEnd::Start, ArrowEnd::End] {
            if !matches!(self.scene.elements[index].kind, Kind::Arrow(_)) {
                return;
            }
            let target = self.binding_target(index, end).map(|e| e.base.id.clone());
            self.set_arrow_binding(index, end, target);
        }
    }

    /// The topmost shape within binding distance of one end of the arrow at
    /// `index` (`getHoveredElementForBinding`).
    fn binding_target(&self, index: usize, end: ArrowEnd) -> Option<&Element> {
        let arrow = &self.scene.elements[index];
        if !matches!(arrow.kind, Kind::Arrow(_)) {
            return None;
        }
        let points = arrow_points(arrow);
        let edge = points[end_index(end, points.len())];
        let other = arrow.binding(opposite(end)).map(|b| b.element_id);
        let simple = points.len() < 3;
        self.scene
            .elements
            .iter()
            .rev()
            .filter(|e| is_bindable(e) && e.base.id != arrow.base.id)
            // Don't bind both ends of a simple segment to one shape.
            .filter(|e| !(simple && other.as_deref() == Some(e.base.id.as_str())))
            .find(|e| distance_to_outline(e, edge) <= max_binding_gap(e, self.zoom))
    }

    /// Moves arrow ends bound to any element in `changed`, so they keep
    /// their focus and gap (`updateBoundElements`). Arrows in `changed`
    /// moved themselves and are left alone.
    pub(super) fn update_bound_arrows(&mut self, changed: &HashSet<String>) {
        for index in 0..self.scene.elements.len() {
            let arrow = &self.scene.elements[index];
            if arrow.base.is_deleted
                || changed.contains(&arrow.base.id)
                || !matches!(arrow.kind, Kind::Arrow(_))
            {
                continue;
            }
            let points = arrow_points(arrow);
            let mut updates = Vec::new();
            for end in [ArrowEnd::Start, ArrowEnd::End] {
                let Some(binding) = arrow
                    .binding(end)
                    .filter(|b| changed.contains(&b.element_id))
                else {
                    continue;
                };
                let Some(shape) = self
                    .scene
                    .elements
                    .iter()
                    .find(|e| e.base.id == binding.element_id && !e.base.is_deleted)
                else {
                    continue;
                };
                let (edge, adjacent) = edge_and_adjacent(&points, end);
                updates.push((
                    end_index(end, points.len()),
                    bound_point(shape, &binding, points[edge], points[adjacent]),
                ));
            }
            if !updates.is_empty() {
                self.move_arrow_points(index, &updates);
            }
        }
    }

    /// Ids of arrows bound to any of `ids`, for keeping them in the moving
    /// layer while dragging.
    pub(super) fn arrows_bound_to(&self, ids: &HashSet<&str>) -> Vec<&str> {
        self.scene
            .elements
            .iter()
            .filter(|e| {
                [ArrowEnd::Start, ArrowEnd::End].into_iter().any(|end| {
                    e.binding(end)
                        .is_some_and(|b| ids.contains(b.element_id.as_str()))
                })
            })
            .map(|e| e.base.id.as_str())
            .collect()
    }

    /// Sets one end's binding to `target` (or none), keeping both sides'
    /// `boundElements` in step (`bindLinearElement`, `unbindLinearElement`).
    fn set_arrow_binding(&mut self, index: usize, end: ArrowEnd, target: Option<String>) {
        let arrow = &self.scene.elements[index];
        let arrow_id = arrow.base.id.clone();
        let previous = arrow.binding(end).map(|b| b.element_id);
        let binding = target.as_ref().and_then(|id| {
            let shape = self.scene.elements.iter().find(|e| &e.base.id == id)?;
            let points = arrow_points(arrow);
            let (edge, adjacent) = edge_and_adjacent(&points, end);
            let focus = focus_distance(shape, points[adjacent], points[edge]);
            let mut gap = distance_to_outline(shape, points[edge]).max(1.0);
            if gap > max_binding_gap(shape, 1.0) {
                gap = HIGHLIGHT_THICKNESS + HIGHLIGHT_OFFSET;
            }
            Some(Binding {
                element_id: id.clone(),
                focus,
                gap,
            })
        });
        if self.scene.elements[index].binding(end) != binding {
            self.scene.elements[index].set_binding(end, binding);
            self.scene.elements[index].touch();
        }
        let other_end = self.scene.elements[index]
            .binding(opposite(end))
            .map(|b| b.element_id);
        // The old shape forgets the arrow unless the other end still uses it.
        if let Some(old) =
            previous.filter(|old| Some(old) != target.as_ref() && Some(old) != other_end.as_ref())
            && let Some(shape) = self.scene.elements.iter_mut().find(|e| e.base.id == old)
        {
            let doomed = std::iter::once(arrow_id.clone()).collect();
            if shape.forget_bindings(&doomed) {
                shape.touch();
            }
        }
        if let Some(target) = target
            && let Some(shape) = self.scene.elements.iter_mut().find(|e| e.base.id == target)
        {
            let bound = shape.base.bound_elements.get_or_insert_with(Vec::new);
            if !bound.iter().any(|b| b.id == arrow_id) {
                bound.push(BoundRef {
                    id: arrow_id,
                    kind: "arrow".into(),
                });
                shape.touch();
            }
        }
    }

    /// Replaces arrow points given in scene coordinates, re-basing so the
    /// first point stays at the element's `x`/`y` (`movePoints`).
    fn move_arrow_points(&mut self, index: usize, updates: &[(usize, Point)]) {
        let arrow = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &arrow.kind else {
            return;
        };
        let to_local = geometry::element_transform(arrow).inverse();
        let mut points = line.points.clone();
        for (i, global) in updates {
            points[*i] = to_local.apply(*global);
        }
        let shift = points[0];
        let points: Vec<Point> = points
            .iter()
            .map(|p| [p[0] - shift[0], p[1] - shift[1]])
            .collect();
        let arrow = &mut self.scene.elements[index];
        arrow.base.x += shift[0];
        arrow.base.y += shift[1];
        self.set_points(index, points);
        let labels: Vec<(usize, Element)> = self
            .scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                crate::hit::container_id(e) == Some(self.scene.elements[index].base.id.as_str())
            })
            .map(|(i, e)| (i, e.clone()))
            .collect();
        self.sync_labels(&labels);
    }
}

/// Shapes arrows can bind to (`isBindableElement`).
fn is_bindable(element: &Element) -> bool {
    !element.base.is_deleted
        && match &element.kind {
            Kind::Rectangle | Kind::Diamond | Kind::Ellipse => true,
            Kind::Text(text) => text.container_id.is_none(),
            Kind::Other(kind) => matches!(
                kind.as_str(),
                "image" | "frame" | "magicframe" | "iframe" | "embeddable"
            ),
            Kind::Line(_) | Kind::Arrow(_) => false,
        }
}

fn outline(element: &Element) -> Outline {
    match element.kind {
        Kind::Diamond => Outline::Diamond,
        Kind::Ellipse => Outline::Ellipse,
        _ => Outline::Box,
    }
}

fn opposite(end: ArrowEnd) -> ArrowEnd {
    match end {
        ArrowEnd::Start => ArrowEnd::End,
        ArrowEnd::End => ArrowEnd::Start,
    }
}

fn end_index(end: ArrowEnd, len: usize) -> usize {
    match end {
        ArrowEnd::Start => 0,
        ArrowEnd::End => len - 1,
    }
}

/// Indices of an end point and its neighbour on the line.
fn edge_and_adjacent(points: &[Point], end: ArrowEnd) -> (usize, usize) {
    match end {
        ArrowEnd::Start => (0, 1.min(points.len() - 1)),
        ArrowEnd::End => (points.len() - 1, points.len().saturating_sub(2)),
    }
}

/// The arrow's points in scene coordinates.
fn arrow_points(arrow: &Element) -> Vec<Point> {
    let (Kind::Line(line) | Kind::Arrow(line)) = &arrow.kind else {
        return Vec::new();
    };
    let transform = geometry::element_transform(arrow);
    line.points.iter().map(|p| transform.apply(*p)).collect()
}

/// `maxBindingGap`: how near the outline an arrow end must be to bind.
fn max_binding_gap(element: &Element, zoom: f64) -> f64 {
    const MIN: f64 = 16.0;
    const MAX_SHARE: f64 = 0.25;
    const MAX: f64 = 32.0;
    let zoom = if zoom < 1.0 { zoom } else { 1.0 };
    let ratio = if outline(element) == Outline::Diamond {
        1.0 / SQRT_2
    } else {
        1.0
    };
    let smaller = ratio * element.base.width.min(element.base.height);
    MIN.max((MAX_SHARE * smaller).min(MAX))
        .max(HIGHLIGHT_THICKNESS / zoom + HIGHLIGHT_OFFSET)
}

fn center(element: &Element) -> Point {
    let b = &element.base;
    [b.x + b.width / 2.0, b.y + b.height / 2.0]
}

/// `pointRotateRads`.
fn rotate([x, y]: Point, [cx, cy]: Point, angle: f64) -> Point {
    let (sin, cos) = angle.sin_cos();
    [
        (x - cx) * cos - (y - cy) * sin + cx,
        (x - cx) * sin + (y - cy) * cos + cy,
    ]
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn normalize(v: Point) -> Point {
    let length = v[0].hypot(v[1]);
    if length == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / length, v[1] / length]
    }
}

/// Samples per corner curve when intersecting or measuring against it.
const CURVE_SAMPLES: usize = 64;

/// A shape outline in its unrotated frame: straight sides and cubic corner
/// curves.
struct Parts {
    sides: Vec<[Point; 2]>,
    curves: Vec<[Point; 4]>,
}

impl Parts {
    /// Every piece as straight segments, curves sampled.
    fn segments(&self) -> Vec<[Point; 2]> {
        let mut segments = self.sides.clone();
        for curve in &self.curves {
            let points: Vec<Point> = (0..=CURVE_SAMPLES)
                .map(|i| cubic(curve, i as f64 / CURVE_SAMPLES as f64))
                .collect();
            segments.extend(points.windows(2).map(|w| [w[0], w[1]]));
        }
        segments
    }
}

fn cubic(c: &[Point; 4], t: f64) -> Point {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    [
        a * c[0][0] + b * c[1][0] + cc * c[2][0] + d * c[3][0],
        a * c[0][1] + b * c[1][1] + cc * c[2][1] + d * c[3][1],
    ]
}

/// `getCornerRadius` for this element (0 when sharp).
fn corner_radius(x: f64, element: &Element) -> f64 {
    element
        .base
        .roundness
        .as_ref()
        .map_or(0.0, |r| crate::render::corner_radius(x, r))
}

fn offset_along(from: Point, center: Point, offset: f64) -> Point {
    let v = normalize(sub(from, center));
    [v[0] * offset, v[1] * offset]
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

/// `deconstructRectanguloidElement` / `deconstructDiamondElement`: the
/// outline grown by `offset`, with Excalidraw's exact corner construction.
fn parts(element: &Element, offset: f64) -> Parts {
    let b = &element.base;
    let (x, y, w, h) = (b.x, b.y, b.width, b.height);
    let c = center(element);
    if outline(element) == Outline::Diamond {
        // `getDiamondPoints`: the +1 keeps sides away from zero length.
        let (top_x, right_y) = ((w / 2.0).floor() + 1.0, (h / 2.0).floor() + 1.0);
        let v = corner_radius((top_x - 0.0).abs(), element);
        let hr = corner_radius((right_y - 0.0).abs(), element);
        if b.roundness.is_none() {
            let top = [x + top_x, y - offset];
            let right = [x + w + offset, y + right_y];
            let bottom = [x + top_x, y + h + offset];
            let left = [x - offset, y + right_y];
            return Parts {
                sides: vec![[top, right], [right, bottom], [bottom, left], [left, top]],
                curves: Vec::new(),
            };
        }
        let (top, right, bottom, left) = (
            [x + top_x, y],
            [x + w, y + right_y],
            [x + top_x, y + h],
            [x, y + right_y],
        );
        let o = [right, bottom, left, top].map(|p| offset_along(p, c, offset));
        let curves = vec![
            [
                add(o[0], [right[0] - v, right[1] - hr]),
                add(o[0], right),
                add(o[0], right),
                add(o[0], [right[0] - v, right[1] + hr]),
            ],
            [
                add(o[1], [bottom[0] + v, bottom[1] - hr]),
                add(o[1], bottom),
                add(o[1], bottom),
                add(o[1], [bottom[0] - v, bottom[1] - hr]),
            ],
            [
                add(o[2], [left[0] + v, left[1] + hr]),
                add(o[2], left),
                add(o[2], left),
                add(o[2], [left[0] + v, left[1] - hr]),
            ],
            [
                add(o[3], [top[0] - v, top[1] + hr]),
                add(o[3], top),
                add(o[3], top),
                add(o[3], [top[0] + v, top[1] + hr]),
            ],
        ];
        let sides = (0..4)
            .map(|i| [curves[i][3], curves[(i + 1) % 4][0]])
            .collect();
        return Parts { sides, curves };
    }
    let r = corner_radius(w.min(h), element);
    if r <= 0.0 {
        let (x1, y1, x2, y2) = (x - offset, y - offset, x + w + offset, y + h + offset);
        let sides = vec![
            [[x1, y1], [x2, y1]],
            [[x2, y1], [x2, y2]],
            [[x1, y2], [x2, y2]],
            [[x1, y2], [x1, y1]],
        ];
        return Parts {
            sides,
            curves: Vec::new(),
        };
    }
    let (x1, y1, x2, y2) = (x, y, x + w, y + h);
    let top = [[x1 + r, y1], [x2 - r, y1]];
    let right = [[x2, y1 + r], [x2, y2 - r]];
    let bottom = [[x1 + r, y2], [x2 - r, y2]];
    let left = [[x1, y2 - r], [x1, y1 + r]];
    let o = [
        [x1 - offset, y1 - offset],
        [x2 + offset, y1 - offset],
        [x2 + offset, y2 + offset],
        [x1 - offset, y2 + offset],
    ]
    .map(|p| offset_along(p, c, offset));
    let toward = |from: Point, corner: Point| {
        [
            from[0] + 2.0 / 3.0 * (corner[0] - from[0]),
            from[1] + 2.0 / 3.0 * (corner[1] - from[1]),
        ]
    };
    let curves = vec![
        [
            add(o[0], left[1]),
            add(o[0], toward(left[1], [x1, y1])),
            add(o[0], toward(top[0], [x1, y1])),
            add(o[0], top[0]),
        ],
        [
            add(o[1], top[1]),
            add(o[1], toward(top[1], [x2, y1])),
            add(o[1], toward(right[0], [x2, y1])),
            add(o[1], right[0]),
        ],
        [
            add(o[2], right[1]),
            add(o[2], toward(right[1], [x2, y2])),
            add(o[2], toward(bottom[1], [x2, y2])),
            add(o[2], bottom[1]),
        ],
        [
            add(o[3], bottom[0]),
            add(o[3], toward(bottom[0], [x1, y2])),
            add(o[3], toward(left[0], [x1, y2])),
            add(o[3], left[0]),
        ],
    ];
    let sides = (0..4)
        .map(|i| [curves[i][3], curves[(i + 1) % 4][0]])
        .collect();
    Parts { sides, curves }
}

/// `distanceToBindableElement`: distance from `p` to the outline.
fn distance_to_outline(element: &Element, p: Point) -> f64 {
    let c = center(element);
    let q = rotate(p, c, -element.base.angle);
    match outline(element) {
        Outline::Ellipse => {
            let (rx, ry) = (element.base.width / 2.0, element.base.height / 2.0);
            (0..ELLIPSE_SAMPLES)
                .map(|i| {
                    let t = i as f64 / ELLIPSE_SAMPLES as f64 * TAU;
                    distance(q, [c[0] + rx * t.cos(), c[1] + ry * t.sin()])
                })
                .fold(f64::INFINITY, f64::min)
        }
        _ => parts(element, 0.0)
            .segments()
            .iter()
            .map(|s| segment_distance(q, s[0], s[1]))
            .fold(f64::INFINITY, f64::min),
    }
}

fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let d = sub(b, a);
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length).clamp(0.0, 1.0)
    };
    distance(p, [a[0] + t * d[0], a[1] + t * d[1]])
}

/// `lineSegmentIntersectionPoints`: where two segments cross, if they do.
fn segment_intersection(l: [Point; 2], s: [Point; 2]) -> Option<Point> {
    let (a1, b1) = (l[1][1] - l[0][1], l[0][0] - l[1][0]);
    let (a2, b2) = (s[1][1] - s[0][1], s[0][0] - s[1][0]);
    let d = a1 * b2 - a2 * b1;
    if d == 0.0 {
        return None;
    }
    let (c1, c2) = (a1 * l[0][0] + b1 * l[0][1], a2 * s[0][0] + b2 * s[0][1]);
    let p = [(c1 * b2 - c2 * b1) / d, (a1 * c2 - a2 * c1) / d];
    let on = |seg: [Point; 2]| {
        let distance = segment_distance(p, seg[0], seg[1]);
        distance == 0.0 || distance < PRECISION
    };
    (on(s) && on(l)).then_some(p)
}

/// `determineFocusDistance`: the signed ratio (-1 to 1) of where the line
/// through `a` and `b` passes the element, relative to its centre.
fn focus_distance(element: &Element, a: Point, b: Point) -> f64 {
    let b_ = &element.base;
    let c = center(element);
    if a == b {
        return 0.0;
    }
    let ra = rotate(a, c, -b_.angle);
    let rb = rotate(b, c, -b_.angle);
    // Excalidraw mixes the unrotated `a` in here; kept for identical focus.
    let sign = -cross(sub(rb, a), sub(rb, c)).signum();
    let reach = (b_.width * 2.0).max(b_.height * 2.0);
    let direction = normalize(sub(rb, ra));
    let interceptor = [
        rb,
        [rb[0] + direction[0] * reach, rb[1] + direction[1] * reach],
    ];
    let (x, y, w, h) = (b_.x, b_.y, b_.width, b_.height);
    let diamond = outline(element) == Outline::Diamond;
    let axes = if diamond {
        [
            [[x + w / 2.0, y], [x + w / 2.0, y + h]],
            [[x, y + h / 2.0], [x + w, y + h / 2.0]],
        ]
    } else {
        [[[x, y], [x + w, y + h]], [[x + w, y], [x, y + h]]]
    };
    let interceptees = if diamond {
        [
            [[x + w / 2.0, y - h], [x + w / 2.0, y + h * 2.0]],
            [[x - w, y + h / 2.0], [x + w * 2.0, y + h / 2.0]],
        ]
    } else {
        [
            [[x - w, y - h], [x + w * 2.0, y + h * 2.0]],
            [[x + w * 2.0, y - h], [x - w, y + h * 2.0]],
        ]
    };
    let mut hits: Vec<Point> = interceptees
        .iter()
        .filter_map(|s| segment_intersection(interceptor, *s))
        .collect();
    let squared = |p: Point| (p[0] - b[0]).powi(2) + (p[1] - b[1]).powi(2);
    hits.sort_by(|g, h| squared(*g).total_cmp(&squared(*h)));
    let mut ratios: Vec<f64> = hits
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let scale = if diamond {
                distance(axes[i][0], axes[i][1]) / 2.0
            } else {
                w.hypot(h) / 2.0
            };
            sign * distance(c, *p) / scale
        })
        .collect();
    ratios.sort_by(|g, h| g.abs().total_cmp(&h.abs()));
    ratios.first().copied().unwrap_or(0.0)
}

/// `determineFocusPoint`: the point the arrow aims at for a given focus,
/// seen from `adjacent`.
fn focus_point(element: &Element, focus: f64, adjacent: Point) -> Point {
    let c = center(element);
    if focus == 0.0 {
        return c;
    }
    let b = &element.base;
    let (x, y, w, h) = (b.x, b.y, b.width, b.height);
    let corners = if outline(element) == Outline::Diamond {
        [
            [x, y + h / 2.0],
            [x + w / 2.0, y],
            [x + w, y + h / 2.0],
            [x + w / 2.0, y + h],
        ]
    } else {
        [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    };
    let k = corners.map(|p| {
        rotate(
            [
                c[0] + (p[0] - c[0]) * focus.abs(),
                c[1] + (p[1] - c[1]) * focus.abs(),
            ],
            c,
            b.angle,
        )
    });
    let side = |i: usize, j: usize| cross(sub(adjacent, k[i]), sub(k[j], k[i]));
    let forward = focus > 0.0;
    let selected = [
        side(0, 1) > 0.0
            && if forward {
                side(1, 2) < 0.0
            } else {
                side(3, 0) < 0.0
            },
        side(1, 2) > 0.0
            && if forward {
                side(2, 3) < 0.0
            } else {
                side(0, 1) < 0.0
            },
        side(2, 3) > 0.0
            && if forward {
                side(3, 0) < 0.0
            } else {
                side(1, 2) < 0.0
            },
    ];
    let pick = |a: usize, b: usize| if forward { k[a] } else { k[b] };
    match selected {
        [true, _, _] => pick(1, 0),
        [_, true, _] => pick(2, 1),
        [_, _, true] => pick(3, 2),
        _ => pick(0, 3),
    }
}

/// Crossings of a segment with the outline grown by `offset`
/// (`intersectElementWithLineSegment`).
fn intersect_outline(element: &Element, segment: [Point; 2], offset: f64) -> Vec<Point> {
    let c = center(element);
    let angle = element.base.angle;
    let local = segment.map(|p| rotate(p, c, -angle));
    let hits: Vec<Point> = match outline(element) {
        Outline::Ellipse => {
            let (rx, ry) = (
                element.base.width / 2.0 + offset,
                element.base.height / 2.0 + offset,
            );
            let (p, d) = (sub(local[0], c), sub(local[1], local[0]));
            let qa = (d[0] / rx).powi(2) + (d[1] / ry).powi(2);
            let qb = 2.0 * (p[0] * d[0] / rx.powi(2) + p[1] * d[1] / ry.powi(2));
            let qc = (p[0] / rx).powi(2) + (p[1] / ry).powi(2) - 1.0;
            let discriminant = qb * qb - 4.0 * qa * qc;
            if qa == 0.0 || discriminant < 0.0 {
                Vec::new()
            } else {
                let root = discriminant.sqrt();
                [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)]
                    .into_iter()
                    .filter(|t| (0.0..=1.0).contains(t))
                    .map(|t| [local[0][0] + d[0] * t, local[0][1] + d[1] * t])
                    .collect()
            }
        }
        _ => {
            let mut hits: Vec<Point> = Vec::new();
            for segment in parts(element, offset).segments() {
                // Sampled curves meet at joints; count a joint crossing once.
                if let Some(p) = segment_intersection(local, segment)
                    && !hits.iter().any(|h| distance(*h, p) < PRECISION)
                {
                    hits.push(p);
                }
            }
            hits
        }
    };
    hits.into_iter().map(|p| rotate(p, c, angle)).collect()
}

/// `updateBoundPoint`: where a bound end goes after its shape changed.
fn bound_point(shape: &Element, binding: &Binding, edge: Point, adjacent: Point) -> Point {
    let focus = focus_point(shape, binding.focus, adjacent);
    if binding.gap == 0.0 {
        return focus;
    }
    let b = &shape.base;
    let reach =
        distance(adjacent, edge) + distance(adjacent, center(shape)) + b.width.max(b.height) * 2.0;
    let direction = normalize(sub(focus, adjacent));
    let far = [
        adjacent[0] + direction[0] * reach,
        adjacent[1] + direction[1] * reach,
    ];
    let mut hits = intersect_outline(shape, [adjacent, far], binding.gap);
    let squared = |p: Point| (p[0] - adjacent[0]).powi(2) + (p[1] - adjacent[1]).powi(2);
    hits.sort_by(|g, h| squared(*g).total_cmp(&squared(*h)));
    match hits.len() {
        // The adjacent point is outside the shape (plus gap).
        2.. => hits[0],
        // Inside: aim straight at the focus point.
        1 => focus,
        0 => edge,
    }
}
