//! Resize and rotate handles, and endpoint handles for 2-point lines
//! (REFERENCE-001 sections 4 and 5; `element/transformHandles.ts`,
//! `element/resizeElements.ts`).
use super::resize::{LOCK_ANGLE, edges, normalize_angle};
use std::f64::consts::PI;

use super::{Editor, Gesture, Modifiers, common_bounds};
use crate::geometry::{self, Affine, Bounds, Point};
use crate::hit::container_id;
use crate::render;
use crate::scene::{Element, Kind};

/// A transform handle on the selection box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Top edge.
    N,
    /// Bottom edge.
    S,
    /// Left edge.
    W,
    /// Right edge.
    E,
    /// Top-left corner.
    Nw,
    /// Top-right corner.
    Ne,
    /// Bottom-left corner.
    Sw,
    /// Bottom-right corner.
    Se,
    /// Rotation knob above the box.
    Rotation,
}

/// What the overlay draws for the current selection.
#[derive(Clone, Debug, PartialEq)]
pub struct Handles {
    /// Rotation of the box, in radians.
    pub angle: f64,
    /// Centres of the corner and rotation handles, in scene units. Edges
    /// resize along the border and have no drawn handle, like desktop
    /// Excalidraw.
    pub handles: Vec<(Handle, Point)>,
    /// Points of a selected line or arrow, in scene units; each can be
    /// dragged.
    pub points: Vec<Point>,
    /// Segment middles long enough to have a handle, with the index a point
    /// dragged from there is inserted at: only a 2-point line's, or every
    /// segment's in the line editor.
    pub midpoints: Vec<(usize, Point)>,
    /// Points selected in the line editor.
    pub selected_points: Vec<usize>,
    /// Whether the line editor is open (points are drawn larger).
    pub editing_line: bool,
}

/// Transform handle size in screen pixels (mouse).
pub const HANDLE_SIZE: f64 = 8.0;

/// Point handle radius in screen pixels (`POINT_HANDLE_SIZE / 2`).
pub const POINT_RADIUS: f64 = 5.0;

/// How near, in screen pixels, a point or segment middle of a selected
/// line is under the pointer (`POINT_HANDLE_SIZE + 1`,
/// `getPointIndexUnderCursor`, `getSegmentMidpointHitCoords`).
const POINT_REACH: f64 = 11.0;

/// A point or segment middle of the selected line or arrow.
#[derive(Clone, Copy, Debug, PartialEq)]
enum LinePart {
    /// A point, by index.
    Point(usize),
    /// A segment middle: where a point dragged from it is inserted, and
    /// where it is.
    Middle(usize, Point),
}

/// Segments shorter than this on screen get no midpoint handle
/// (`isSegmentTooShort`: `POINT_HANDLE_SIZE * 4`).
const MIDPOINT_MIN_LENGTH: f64 = 40.0;

/// Gap between the box and its handles, in screen pixels.
const SPACING: f64 = 2.0;

/// Rotation knob distance above the top handles (`ROTATION_RESIZE_HANDLE_GAP`).
const ROTATION_GAP: f64 = 16.0;

/// Reach of edge resizing either side of the border (`SIDE_RESIZING_THRESHOLD`).
const SIDE_THRESHOLD: f64 = 4.0;

/// The box handles attach to: one element's local box and transform, or
/// the selection's common scene box.
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub(super) bounds: Bounds,
    pub(super) transform: Affine,
    pub(super) margin: f64,
}

// Public API
impl Editor {
    /// The point of the selected line or arrow under `at`, which the
    /// overlay rings (`highlightPoint`); none for points already selected
    /// in the line editor.
    pub fn hovered_point(&self, at: Point) -> Option<Point> {
        let LinePart::Point(index) = self.line_part_at(at)? else {
            return None;
        };
        if self
            .line_edit
            .as_ref()
            .is_some_and(|edit| edit.selected.contains(&index))
        {
            return None;
        }
        self.handles()?.points.get(index).copied()
    }

    /// Whether a point or segment middle of the selected line or arrow is
    /// under `at`, where a press drags it (the pointer cursor).
    pub fn over_line_handle(&self, at: Point) -> bool {
        self.line_part_at(at).is_some()
    }

    /// Returns the handles for the current selection, if it has any.
    pub fn handles(&self) -> Option<Handles> {
        if self.text.is_some() || !matches!(self.gesture, None | Some(Gesture::Resize { .. })) {
            return None;
        }
        // A locked selection shows its border only (`getTransformHandles`).
        if self.selection().all(Element::is_locked) {
            return None;
        }
        let points = self.line_points().unwrap_or_default();
        let editing_line = self.line_edit.is_some();
        // A 2-point line or the line editor shows only points
        // (`shouldShowBoundingBox`).
        if points.len() == 2 || (editing_line && !points.is_empty()) {
            // Middles sit on the drawn curve of round lines, and their
            // length is measured along it (`getSegmentMidPoint`).
            let line = self
                .selection()
                .next()
                .expect("points come from the one selected line");
            let transform = geometry::element_transform(line);
            let midpoints = (1..points.len())
                .filter(|&end| render::segment_length(line, end) * self.zoom >= MIDPOINT_MIN_LENGTH)
                .map(|end| (end, transform.apply(render::segment_midpoint(line, end))))
                .collect();
            let selected_points = self
                .line_edit
                .as_ref()
                .map(|edit| edit.selected.clone())
                .unwrap_or_default();
            return Some(Handles {
                angle: 0.0,
                handles: Vec::new(),
                points,
                midpoints,
                selected_points,
                editing_line,
            });
        }
        let frame = self.frame()?;
        let z = self.zoom;
        let [x1, y1, x2, y2] = frame.bounds;
        let (size, margin, centering) = (
            HANDLE_SIZE / z,
            frame.margin / z,
            (HANDLE_SIZE - 2.0 * SPACING) / (2.0 * z),
        );
        let left = x1 - margin - size / 2.0 + centering;
        let right = x2 + margin - centering + size / 2.0;
        let top = y1 - margin - size / 2.0 + centering;
        let bottom = y2 + margin - centering + size / 2.0;
        // The knob's top sits the gap above the top handles' top edge.
        let rotation = [(x1 + x2) / 2.0, top - ROTATION_GAP / z];
        let local = [
            (Handle::Nw, [left, top]),
            (Handle::Ne, [right, top]),
            (Handle::Sw, [left, bottom]),
            (Handle::Se, [right, bottom]),
            (Handle::Rotation, rotation),
        ];
        // No rotation knob when a frame is selected (`getTransformHandles`,
        // and `renderTransformHandles` for several).
        let has_frame = self.selection().any(|e| e.frame_title().is_some());
        let handles = local
            .into_iter()
            .filter(|(h, _)| !(has_frame && *h == Handle::Rotation))
            .map(|(h, p)| (h, frame.transform.apply(p)))
            .collect();
        Some(Handles {
            angle: frame.transform.rotation(),
            handles,
            points,
            midpoints: Vec::new(),
            selected_points: Vec::new(),
            editing_line: false,
        })
    }

    /// Returns the handle under `at`: the rotation knob, then corners, then
    /// edges.
    pub fn handle_at(&self, at: Point) -> Option<Handle> {
        let handles = self.handles()?;
        // A 2-point line has no box to resize by.
        if handles.handles.is_empty() {
            return None;
        }
        let frame = self.frame()?;
        let to_local = frame.transform.inverse();
        let p = to_local.apply(at);
        let half = HANDLE_SIZE / self.zoom / 2.0;
        let over = |handle: &(Handle, Point)| {
            let [cx, cy] = to_local.apply(handle.1);
            (p[0] - cx).abs() <= half && (p[1] - cy).abs() <= half
        };
        let order = [
            Handle::Rotation,
            Handle::Nw,
            Handle::Ne,
            Handle::Sw,
            Handle::Se,
        ];
        if let Some(hit) = order
            .into_iter()
            .find(|h| handles.handles.iter().any(|x| x.0 == *h && over(x)))
        {
            return Some(hit);
        }
        // Edges: within the threshold of the border, which sits outside the box.
        let [x1, y1, x2, y2] = frame.bounds;
        let reach = SIDE_THRESHOLD / self.zoom;
        let (inside_x, inside_y) = (p[0] >= x1 && p[0] <= x2, p[1] >= y1 && p[1] <= y2);
        let near = |value: f64, line: f64| (value - line).abs() <= reach;
        match () {
            _ if inside_x && near(p[1], y1 - reach) => Some(Handle::N),
            _ if inside_x && near(p[1], y2 + reach) => Some(Handle::S),
            _ if inside_y && near(p[0], x1 - reach) => Some(Handle::W),
            _ if inside_y && near(p[0], x2 + reach) => Some(Handle::E),
            _ => None,
        }
    }
}

// Private API
impl Editor {
    /// The selection's handle frame; `None` without a selection or for a
    /// 2-point line (which gets endpoint handles instead).
    fn frame(&self) -> Option<Frame> {
        let selected: Vec<&Element> = self.selection().collect();
        match selected[..] {
            [] => None,
            [one] => {
                let linear = matches!(one.kind, Kind::Line(_) | Kind::Arrow(_));
                Some(Frame {
                    bounds: geometry::local_bounds(one),
                    transform: geometry::element_transform(one),
                    margin: if linear { 10.0 } else { 2.0 },
                })
            }
            _ => Some(Frame {
                bounds: common_bounds(selected.into_iter()),
                transform: Affine::IDENTITY,
                margin: 4.0,
            }),
        }
    }

    /// Points of the selected element when it is a lone line or arrow, in
    /// scene units.
    fn line_points(&self) -> Option<Vec<Point>> {
        let selected: Vec<&Element> = self.selection().collect();
        let [one] = selected[..] else { return None };
        let (Kind::Line(line) | Kind::Arrow(line)) = &one.kind else {
            return None;
        };
        let transform = geometry::element_transform(one);
        Some(line.points.iter().map(|p| transform.apply(*p)).collect())
    }

    /// Starts a resize, rotation or point drag when `at` is on a handle. A
    /// press on a 2-point line's midpoint adds a point there and drags it.
    pub(super) fn press_handle(&mut self, at: Point, modifiers: Modifiers) -> bool {
        if let Some(part) = self.line_part_at(at) {
            let index = self.selection_indices()[0];
            let before = self.scene.elements.clone();
            let which = match part {
                LinePart::Point(which) => which,
                LinePart::Middle(insert, middle) => {
                    // On the grid, the new point starts at the pointer's grid
                    // point (`addMidpoint`).
                    let point = if self.grid_size(modifiers).is_some() {
                        self.snap(at, modifiers)
                    } else {
                        middle
                    };
                    self.insert_point(index, insert, point);
                    if let Some(edit) = &mut self.line_edit {
                        edit.selected.clear();
                    }
                    insert
                }
            };
            self.gesture = Some(if self.line_edit.is_some() {
                let mut gesture = self.press_line_point(index, which, at, modifiers);
                // The undo step starts before any inserted point.
                if let Gesture::Points { before: kept, .. } = &mut gesture {
                    *kept = before;
                }
                gesture
            } else {
                // Grabbed a little off the point, it moves without a jump.
                let point = self.handles().and_then(|h| h.points.get(which).copied());
                let offset = point.map_or([0.0, 0.0], |[x, y]| [at[0] - x, at[1] - y]);
                Gesture::Endpoint {
                    index,
                    which,
                    before,
                    offset,
                }
            });
            return true;
        }
        let (Some(handle), Some(frame)) = (self.handle_at(at), self.frame()) else {
            return false;
        };
        let start: Vec<(usize, Element)> = self
            .moving()
            .into_iter()
            .map(|(i, _)| (i, self.scene.elements[i].clone()))
            .collect();
        let before = self.scene.elements.clone();
        // Keep the grab point's distance to the edge, so the edge does not jump.
        let local = frame.transform.inverse().apply(at);
        let [x1, y1, x2, y2] = frame.bounds;
        let (moves_left, moves_right, moves_top, moves_bottom) = edges(handle);
        let edge = [
            if moves_left {
                x1
            } else if moves_right {
                x2
            } else {
                local[0]
            },
            if moves_top {
                y1
            } else if moves_bottom {
                y2
            } else {
                local[1]
            },
        ];
        let offset = [local[0] - edge[0], local[1] - edge[1]];
        self.gesture = Some(if handle == Handle::Rotation {
            let [cx, cy] = frame.transform.apply([(x1 + x2) / 2.0, (y1 + y2) / 2.0]);
            Gesture::Rotate {
                before,
                start,
                center: [cx, cy],
            }
        } else {
            Gesture::Resize {
                handle,
                before,
                start,
                frame,
                offset,
            }
        });
        true
    }

    /// Continues a handle drag; returns `false` for other gestures.
    pub(super) fn drag_handle(&mut self, at: Point, modifiers: Modifiers) -> bool {
        match self.gesture.take() {
            Some(Gesture::Resize {
                handle,
                before,
                start,
                frame,
                offset,
            }) => {
                // The handle, not the pointer, lands on the grid: snap where
                // it would go and keep the grab offset (`getResizeOffsetXY`).
                let local = frame.transform.inverse().apply(at);
                let handle_at = frame
                    .transform
                    .apply([local[0] - offset[0], local[1] - offset[1]]);
                let snapped = frame
                    .transform
                    .inverse()
                    .apply(self.snap(handle_at, modifiers));
                let at = frame
                    .transform
                    .apply([snapped[0] + offset[0], snapped[1] + offset[1]]);
                if start
                    .iter()
                    .filter(|(_, e)| container_id(e).is_none())
                    .count()
                    == 1
                {
                    self.resize_one(&start, &frame, handle, at, offset, modifiers);
                } else {
                    self.resize_many(&start, &frame, handle, at, offset, modifiers);
                }
                self.update_bound_arrows(&ids(&start));
                self.gesture = Some(Gesture::Resize {
                    handle,
                    before,
                    start,
                    frame,
                    offset,
                });
            }
            Some(Gesture::Rotate {
                before,
                start,
                center,
            }) => {
                // The pointer on the grid (`maybeHandleResize`); the angle
                // snapped with Shift, then normalised, like
                // `rotateSingleElement`.
                let at = self.snap(at, modifiers);
                let mut angle = 5.0 * PI / 2.0 + (at[1] - center[1]).atan2(at[0] - center[0]);
                if modifiers.shift {
                    angle += LOCK_ANGLE / 2.0;
                    angle -= angle % LOCK_ANGLE;
                }
                let angle = normalize_angle(angle);
                self.rotate(&start, center, angle);
                self.update_bound_arrows(&ids(&start));
                self.gesture = Some(Gesture::Rotate {
                    before,
                    start,
                    center,
                });
            }
            Some(Gesture::Endpoint {
                index,
                which,
                before,
                offset,
            }) => {
                let point = [at[0] - offset[0], at[1] - offset[1]];
                self.drag_endpoint(index, which, point, modifiers);
                self.gesture = Some(Gesture::Endpoint {
                    index,
                    which,
                    before,
                    offset,
                });
            }
            Some(Gesture::Points {
                index,
                before,
                from,
                starts,
            }) => {
                self.drag_line_points(index, from, &starts, at, modifiers);
                self.gesture = Some(Gesture::Points {
                    index,
                    before,
                    from,
                    starts,
                });
            }
            other => {
                self.gesture = other;
                return false;
            }
        }
        true
    }

    fn selection_indices(&self) -> Vec<usize> {
        let ids = &self.selected;
        self.scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| ids.contains(&e.base.id))
            .map(|(i, _)| i)
            .collect()
    }
}

fn ids(start: &[(usize, Element)]) -> std::collections::HashSet<String> {
    start.iter().map(|(_, e)| e.base.id.clone()).collect()
}

// Private API
impl Editor {
    /// The point or segment middle of the selected line or arrow under
    /// `at`, within 11 screen pixels; points first, the last one first
    /// (drawn on top).
    fn line_part_at(&self, at: Point) -> Option<LinePart> {
        let handles = self.handles()?;
        let reach = POINT_REACH / self.zoom;
        let near = |p: &Point| (p[0] - at[0]).hypot(p[1] - at[1]) < reach;
        if let Some(index) = handles.points.iter().rposition(near) {
            return Some(LinePart::Point(index));
        }
        handles
            .midpoints
            .iter()
            .find(|(_, p)| near(p))
            .map(|&(insert, middle)| LinePart::Middle(insert, middle))
    }
}
