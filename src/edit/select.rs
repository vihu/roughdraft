//! Selection tool: click, shift-click, box select, move
//! (REFERENCE-001 sections 3 and 6).
use std::collections::HashSet;

use super::{Editor, Gesture, Modifiers, common_bounds, normalize};
use crate::geometry::{self, Point};
use crate::hit::{self, container_id};
use crate::scene::Element;

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
        let hit = self.hit(at).map(|e| e.base.id.clone());
        let grab_selection = !modifiers.shift && self.in_selection(at);
        let clicked = match hit {
            Some(id) if modifiers.shift && self.selected.contains(&id) => {
                self.selected.remove(&id);
                return;
            }
            _ if grab_selection => hit.filter(|id| self.selected.contains(id)),
            Some(id) => {
                if !modifiers.shift {
                    self.selected.clear();
                }
                self.selected.insert(id);
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
            starts: self.moving(),
            moved: false,
            clicked,
        });
    }

    /// Continues a move or box select; returns `false` for other gestures.
    pub(super) fn select_drag(&mut self, at: Point, modifiers: Modifiers) -> bool {
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
                for (i, position) in targets {
                    self.place(i, position);
                }
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
                        && a >= x1
                        && b >= y1
                        && c <= x2
                        && d <= y2
                });
                selected.extend(inside.map(|e| e.base.id.clone()));
                self.selected = selected;
                true
            }
            _ => false,
        }
    }

    /// Ends a move or box select; returns `false` for other gestures.
    pub(super) fn select_release(&mut self) -> bool {
        match self.gesture.take() {
            Some(Gesture::Move {
                before,
                moved: true,
                ..
            }) => self.history.record(before),
            Some(Gesture::Move {
                clicked: Some(id), ..
            }) => self.selected = HashSet::from([id]),
            Some(Gesture::Move { .. } | Gesture::Marquee { .. }) => {}
            other => {
                self.gesture = other;
                return false;
            }
        }
        true
    }
}
