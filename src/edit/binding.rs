//! Arrow binding (`element/binding.ts`): an arrow end attaches to a shape
//! with a `focus` (where its line passes the shape, -1 to 1 relative to the
//! centre) and a `gap` (distance from the outline), and follows the shape
//! when it moves, resizes or rotates. Geometry ported from Excalidraw 0.18.1
//! (MIT, Copyright (c) 2020 Excalidraw): `determineFocusDistance`,
//! `determineFocusPoint`, `updateBoundPoint`, `maxBindingGap`,
//! `distanceToBindableElement`, `intersectElementWithLineSegment`.
use std::collections::HashSet;

pub(crate) use self::outline::{arrow_points, distance_to_outline};
use self::outline::{
    bound_point, edge_and_adjacent, end_index, focus_distance, is_bindable, max_binding_gap,
    opposite,
};
use super::{Editor, Gesture};
use crate::geometry::{self, Point};
use crate::scene::{ArrowEnd, Binding, BoundRef, Element, FillStyle, Kind};

/// `BINDING_HIGHLIGHT_THICKNESS`.
const HIGHLIGHT_THICKNESS: f64 = 10.0;

/// `BINDING_HIGHLIGHT_OFFSET`.
const HIGHLIGHT_OFFSET: f64 = 4.0;

// Public API
impl Editor {
    /// Returns the shapes to highlight as binding targets
    /// (`suggestedBindings`): under the arrow tool before a press, at the
    /// moving end of an arrow being drawn or dragged by an end, and at the
    /// ends of arrows being moved.
    pub fn binding_suggestions(&self) -> Vec<&Element> {
        /// `getSuggestedBindingsForArrows` gives up above this many.
        const MOVING_LIMIT: usize = 50;

        // The bool: the arrow moves as a whole, so only ends whose shape is
        // still in reach count (`getOriginalBindingsIfStillCloseToArrowEnds`).
        let ends: Vec<(usize, ArrowEnd, bool)> = match (&self.gesture, &self.multi) {
            (_, Some(multi)) => vec![(multi.index, ArrowEnd::End, false)],
            (
                Some(Gesture::Line {
                    index,
                    dragged: true,
                    ..
                }),
                _,
            ) => vec![(*index, ArrowEnd::End, false)],
            (
                Some(Gesture::Endpoint {
                    index, which: 0, ..
                }),
                _,
            ) => vec![(*index, ArrowEnd::Start, false)],
            (Some(Gesture::Endpoint { index, .. }), _) => vec![(*index, ArrowEnd::End, false)],
            (
                Some(Gesture::Move {
                    starts,
                    moved: true,
                    ..
                }),
                _,
            ) if starts.len() <= MOVING_LIMIT => starts
                .iter()
                .flat_map(|(index, _)| {
                    [
                        (*index, ArrowEnd::Start, true),
                        (*index, ArrowEnd::End, true),
                    ]
                })
                .collect(),
            (None, None) if self.tool == super::Tool::Arrow => {
                return self
                    .hover
                    .and_then(|at| self.bindable_near(at, |_| false))
                    .into_iter()
                    .collect();
            }
            _ => Vec::new(),
        };
        let mut shapes: Vec<&Element> = Vec::new();
        for (index, end, whole) in ends {
            let target = if whole {
                self.moved_arrow_target(index, end)
            } else {
                self.binding_target(index, end)
            };
            if let Some(shape) = target
                && !self.selected.contains(&shape.base.id)
                && !shapes.iter().any(|s| s.base.id == shape.base.id)
            {
                shapes.push(shape);
            }
        }
        shapes
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

    /// After an arrow moves as a whole (dragged, nudged, resized, rotated):
    /// each end keeps a binding only while its shape is still in reach,
    /// re-aimed at the shape under it, and never picks up a new shape
    /// (`getBindingStrategyForDraggingArrowOrJoints`).
    pub(super) fn rebind_moved_arrow(&mut self, index: usize) {
        for end in [ArrowEnd::Start, ArrowEnd::End] {
            if !matches!(self.scene.elements[index].kind, Kind::Arrow(_)) {
                return;
            }
            let target = self
                .moved_arrow_target(index, end)
                .map(|e| e.base.id.clone());
            self.set_arrow_binding(index, end, target);
        }
    }

    /// The shape a moved arrow's end binds to: the one under it, if the
    /// end's current shape is still within its binding gap
    /// (`getOriginalBindingIfStillCloseOfLinearElementEdge`).
    fn moved_arrow_target(&self, index: usize, end: ArrowEnd) -> Option<&Element> {
        let arrow = &self.scene.elements[index];
        if !matches!(arrow.kind, Kind::Arrow(_)) {
            return None;
        }
        let points = arrow_points(arrow);
        let edge = points[end_index(end, points.len())];
        let id = arrow.binding(end)?.element_id;
        let shape = self
            .scene
            .elements
            .iter()
            .find(|e| e.base.id == id && is_bindable(e))?;
        if distance_to_outline(shape, edge) > max_binding_gap(shape, self.zoom) {
            return None;
        }
        self.binding_target(index, end)
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
        self.bindable_near(edge, |e| {
            e.base.id == arrow.base.id
                // Don't bind both ends of a simple segment to one shape.
                || (simple && other.as_deref() == Some(e.base.id.as_str()))
        })
    }

    /// The topmost bindable shape within binding distance of `point`, other
    /// than those `skip` rules out.
    ///
    /// Locked shapes are left out (`isBindableElement(el, false)`), and a
    /// shape with a solid, visible fill binds anywhere in its box
    /// (`isBindingFallthroughEnabled`), frames excepted.
    fn bindable_near(&self, point: Point, skip: impl Fn(&Element) -> bool) -> Option<&Element> {
        let solid = |e: &Element| {
            e.base.fill_style == FillStyle::Solid
                && !crate::render::is_transparent(&e.base.background_color)
                && e.frame_title().is_none()
        };
        let in_box = |e: &Element| {
            let [x1, y1, x2, y2] = geometry::element_bounds(e);
            (x1..=x2).contains(&point[0]) && (y1..=y2).contains(&point[1])
        };
        self.scene
            .elements
            .iter()
            .rev()
            .filter(|e| is_bindable(e) && !e.is_locked() && !skip(e))
            .find(|e| {
                distance_to_outline(e, point) <= max_binding_gap(e, self.zoom)
                    || (solid(e) && in_box(e))
            })
    }

    /// Moves arrow ends bound to any element in `changed`, so they keep
    /// their focus and gap (`updateBoundElements`). Arrows in `changed`
    /// moved themselves and are left alone.
    pub(crate) fn update_bound_arrows(&mut self, changed: &HashSet<String>) {
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
    pub(crate) fn set_arrow_binding(
        &mut self,
        index: usize,
        end: ArrowEnd,
        target: Option<String>,
    ) {
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

mod outline;
