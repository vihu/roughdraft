//! Eraser (`App.tsx` `handleEraser`, `eraseElements`): dragging marks every
//! element the pointer passes over, whole groups at once and never locked
//! ones; Alt unmarks. Marked elements are drawn faded and deleted, with
//! their labels, on release as one undo step. A click erases what is under
//! it.
use std::collections::HashSet;

use super::{Editor, Gesture, Modifiers};
use crate::geometry::Point;
use crate::hit::{self, container_id};

// Public API
impl Editor {
    /// Returns the ids of the elements (and their labels) the eraser will
    /// delete on release, to draw faded (`ELEMENT_READY_TO_ERASE_OPACITY`).
    pub fn pending_erasure(&self) -> Option<&HashSet<String>> {
        match &self.gesture {
            Some(Gesture::Erase { marked, .. }) => Some(marked),
            _ => None,
        }
    }
}

// Private API
impl Editor {
    pub(super) fn erase_press(&mut self, at: Point, modifiers: Modifiers) {
        let mut marked = HashSet::new();
        self.mark_along(&mut marked, at, at, modifiers.alt);
        self.gesture = Some(Gesture::Erase { last: at, marked });
    }

    /// Continues an erase; returns `false` for other gestures.
    pub(super) fn erase_drag(&mut self, at: Point, modifiers: Modifiers) -> bool {
        if !matches!(self.gesture, Some(Gesture::Erase { .. })) {
            return false;
        }
        let Some(Gesture::Erase { last, mut marked }) = self.gesture.take() else {
            unreachable!("checked above");
        };
        self.mark_along(&mut marked, last, at, modifiers.alt);
        self.gesture = Some(Gesture::Erase { last: at, marked });
        true
    }

    /// Deletes what was marked; returns `false` for other gestures.
    pub(super) fn erase_release(&mut self) -> bool {
        if !matches!(self.gesture, Some(Gesture::Erase { .. })) {
            return false;
        }
        let Some(Gesture::Erase { marked, .. }) = self.gesture.take() else {
            unreachable!("checked above");
        };
        let top_level: HashSet<String> = marked
            .into_iter()
            .filter(|id| {
                self.scene
                    .elements
                    .iter()
                    .any(|e| &e.base.id == id && container_id(e).is_none())
            })
            .collect();
        if !top_level.is_empty() {
            let tool = self.tool;
            self.selected = top_level;
            self.delete();
            self.tool = tool;
        }
        true
    }

    /// Marks (or with `unmark`, unmarks) what lies along the segment, sampled
    /// every hit threshold like Excalidraw.
    fn mark_along(&self, marked: &mut HashSet<String>, from: Point, to: Point, unmark: bool) {
        let step = self.threshold();
        let length = (to[0] - from[0]).hypot(to[1] - from[1]);
        let mut travelled: f64 = 0.0;
        loop {
            let t = if length == 0.0 {
                0.0
            } else {
                travelled / length
            };
            let at = [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
            ];
            for element in hit::elements_at(&self.scene, at, step) {
                // Groups go together (by their outermost group), labels with
                // their shapes.
                let group = element.group_ids().last().map(|g| (*g).to_owned());
                let members: Vec<&str> = match &group {
                    Some(group) => self
                        .scene
                        .elements
                        .iter()
                        .filter(|e| !e.base.is_deleted && e.group_ids().contains(&group.as_str()))
                        .map(|e| e.base.id.as_str())
                        .collect(),
                    None => vec![element.base.id.as_str()],
                };
                let labels = self.scene.elements.iter().filter(|e| {
                    !e.base.is_deleted && container_id(e).is_some_and(|c| members.contains(&c))
                });
                let ids: Vec<String> = members
                    .iter()
                    .map(|id| (*id).to_owned())
                    .chain(labels.map(|l| l.base.id.clone()))
                    .collect();
                for id in ids {
                    if unmark {
                        marked.remove(&id);
                    } else {
                        marked.insert(id);
                    }
                }
            }
            if travelled >= length {
                break;
            }
            travelled = (travelled + step).min(length);
        }
    }
}
