//! Selection tool: click, shift-click, box select, move
//! (REFERENCE-001 sections 3 and 6).
use std::collections::{HashMap, HashSet};

use super::{Editor, Gesture, Modifiers, common_bounds, normalize};
use crate::geometry::{self, Point};
use crate::hit::{self, container_id};
use crate::scene::{ArrowEnd, Element, Kind};

impl Editor {
    /// Whether `at` is inside the selection's box, padded by the threshold:
    /// the element's rotated box for one, the common box for several.
    pub(super) fn in_selection(&self, at: Point) -> bool {
        let pad = self.threshold();
        let selected: Vec<&Element> = self.selection().collect();
        match selected[..] {
            [] => false,
            [one] => hit::in_box(one, at, pad),
            _ => {
                let [x1, y1, x2, y2] = common_bounds(selected.into_iter());
                at[0] >= x1 - pad && at[0] <= x2 + pad && at[1] >= y1 - pad && at[1] <= y2 + pad
            }
        }
    }

    pub(super) fn select_press(&mut self, at: Point, modifiers: Modifiers) {
        if self.press_handle(at, modifiers) {
            return;
        }
        // A press off the edited line's points leaves the line editor.
        self.line_edit = None;
        let hit = self.hit(at).map(|e| e.base.id.clone());
        // Clicking outside the entered group leaves it.
        let inside_group = |id: &String| {
            let entered = self.editing_group.as_deref();
            self.scene
                .elements
                .iter()
                .any(|e| &e.base.id == id && entered.is_some_and(|g| e.group_ids().contains(&g)))
        };
        if !hit.as_ref().is_some_and(inside_group) {
            self.editing_group = None;
        }
        // A wholly locked selection cannot be dragged; the press goes
        // through to what is under it.
        let grab_selection =
            !modifiers.shift && self.in_selection(at) && self.selection().any(|e| !e.is_locked());
        let clicked = match hit {
            Some(id) if modifiers.shift && self.selected.contains(&id) => {
                // Shift-click takes the element's whole group out.
                let element = self.scene.elements.iter().find(|e| e.base.id == id);
                let group = element.and_then(|e| self.selection_group(e));
                let members: Vec<String> = match group {
                    Some(g) => self
                        .scene
                        .elements
                        .iter()
                        .filter(|e| e.group_ids().contains(&g.as_str()))
                        .map(|e| e.base.id.clone())
                        .collect(),
                    None => vec![id],
                };
                for member in members {
                    self.selected.remove(&member);
                }
                return;
            }
            _ if grab_selection => hit.filter(|id| self.selected.contains(id)),
            Some(id) => {
                if !modifiers.shift {
                    self.selected.clear();
                }
                self.selected.insert(id);
                self.expand_to_groups();
                // A frame and its children are never selected together
                // (`App.tsx` shift-click on frames and framed elements).
                self.exclude_framed_from_selection();
                None
            }
            None => {
                if !modifiers.shift {
                    self.selected.clear();
                }
                let keep = self.selected.clone();
                self.gesture = Some(Gesture::Marquee {
                    from: at,
                    to: at,
                    keep,
                });
                return;
            }
        };
        self.gesture = Some(Gesture::Move {
            from: at,
            before: self.scene.elements.clone(),
            starts: self.moving_with_children(),
            moved: false,
            clicked,
            duplicated: false,
        });
    }

    /// Alt+drag: copies of the moving elements go on top, become the
    /// selection and take over the drag; the originals go back to where the
    /// drag started, keeping their ids and bindings. Excalidraw swaps ids the
    /// other way round (the copies stay), which looks the same.
    fn alt_duplicate(&mut self) {
        let Some(Gesture::Move {
            starts, duplicated, ..
        }) = &mut self.gesture
        else {
            return;
        };
        *duplicated = true;
        let starts = std::mem::take(starts);
        let ids: HashMap<String, String> = starts
            .iter()
            .map(|(i, _)| (self.scene.elements[*i].base.id.clone(), crate::random::id()))
            .collect();
        let mut groups = self.kept_groups();
        let copies: Vec<Element> = starts
            .iter()
            .map(|(i, _)| {
                let original = &self.scene.elements[*i];
                let mut copy = original.duplicate();
                copy.base.id = ids[&original.base.id].clone();
                copy.remap(&ids, &mut groups);
                super::frame::keep_frame(&mut copy, original, &ids);
                copy
            })
            .collect();
        for &(i, start) in &starts {
            self.place(i, start);
        }
        self.update_bound_arrows(&ids.keys().cloned().collect());
        self.selected = copies
            .iter()
            .filter(|c| container_id(c).is_none())
            .map(|c| c.base.id.clone())
            .collect();
        let first = self.scene.elements.len();
        self.scene.elements.extend(copies);
        self.exclude_framed_from_selection();
        // The dragged copies keep the originals' bindings to shapes that
        // were not copied (`fixBindingsAfterDuplication`); `remap` cleared
        // them.
        for (k, &(original, _)) in starts.iter().enumerate() {
            for end in [ArrowEnd::Start, ArrowEnd::End] {
                let Some(binding) = self.scene.elements[original].binding(end) else {
                    continue;
                };
                if !ids.contains_key(&binding.element_id) {
                    self.set_arrow_binding(first + k, end, Some(binding.element_id));
                }
            }
        }
        if let Some(Gesture::Move { starts: moving, .. }) = &mut self.gesture {
            *moving = (first..)
                .zip(starts.iter().map(|(_, start)| *start))
                .collect();
        }
    }

    /// Continues a move or box select; returns `false` for other gestures.
    pub(super) fn select_drag(&mut self, at: Point, modifiers: Modifiers) -> bool {
        if modifiers.alt
            && let Some(Gesture::Move {
                duplicated: false, ..
            }) = self.gesture
        {
            self.alt_duplicate();
        }
        match &mut self.gesture {
            Some(Gesture::Move {
                from,
                starts,
                moved,
                ..
            }) => {
                let mut offset = [at[0] - from[0], at[1] - from[1]];
                if modifiers.shift {
                    // Lock to the dominant axis.
                    let axis = usize::from(offset[1].abs() < offset[0].abs());
                    offset[axis] = 0.0;
                }
                if offset == [0.0, 0.0] && !*moved {
                    return true;
                }
                *moved = true;
                let targets: Vec<(usize, Point)> = starts
                    .iter()
                    .map(|&(i, [x, y])| (i, [x + offset[0], y + offset[1]]))
                    .collect();
                let moved: HashSet<String> = targets
                    .iter()
                    .map(|(i, _)| self.scene.elements[*i].base.id.clone())
                    .collect();
                for (i, position) in targets {
                    self.place(i, position);
                }
                self.update_bound_arrows(&moved);
                true
            }
            Some(Gesture::Marquee { from, to, keep }) => {
                *to = at;
                let [x1, y1, x2, y2] = normalize(*from, *to);
                let mut selected = keep.clone();
                let inside = self.scene.elements.iter().filter(|e| {
                    let [a, b, c, d] = geometry::element_bounds(e);
                    !e.base.is_deleted
                        && container_id(e).is_none()
                        && !e.is_locked()
                        && a >= x1
                        && b >= y1
                        && c <= x2
                        && d <= y2
                });
                selected.extend(inside.map(|e| e.base.id.clone()));
                self.selected = selected;
                self.expand_to_groups();
                self.exclude_framed_from_selection();
                true
            }
            _ => false,
        }
    }

    /// Ends a move or box select; returns `false` for other gestures.
    pub(super) fn select_release(&mut self, at: Point) -> bool {
        match self.gesture.take() {
            Some(Gesture::Move {
                before,
                moved: true,
                starts,
                ..
            }) => {
                // Dragged arrows keep bindings still in reach and pick up no
                // new ones (`getBindingStrategyForDraggingArrowOrJoints`).
                for (index, _) in starts {
                    self.rebind_moved_arrow(index);
                }
                self.update_frame_membership(at);
                self.history.record(before);
            }
            Some(Gesture::Move {
                clicked: Some(id), ..
            }) => {
                self.selected = HashSet::from([id]);
                self.expand_to_groups();
            }
            Some(Gesture::Move { .. } | Gesture::Marquee { .. }) => {}
            Some(
                Gesture::Endpoint { index, before, .. } | Gesture::Points { index, before, .. },
            ) => {
                self.bind_arrow_ends(index);
                if before != self.scene.elements {
                    self.history.record(before);
                }
            }
            Some(Gesture::Resize { before, .. } | Gesture::Rotate { before, .. }) => {
                if before != self.scene.elements {
                    // Resized or rotated arrows rebind like moved ones.
                    let arrows: Vec<usize> = self
                        .scene
                        .elements
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            matches!(e.kind, Kind::Arrow(_)) && self.selected.contains(&e.base.id)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    for index in arrows {
                        self.rebind_moved_arrow(index);
                    }
                    self.refit_selected_frames();
                    self.history.record(before);
                }
            }
            other => {
                self.gesture = other;
                return false;
            }
        }
        true
    }
}
