//! Frames take in what is drawn inside them and carry their children
//! (`frame.ts`, `includeElementsInFrames`).
use std::collections::HashSet;

use super::Editor;
use crate::geometry::{self, Point};
use crate::hit::container_id;
use crate::scene::Element;

// Private API
impl Editor {
    /// Makes what lies wholly inside a new frame its children, labels
    /// included (`getElementsInNewFrame`, `addElementsToFrame`): unlocked
    /// elements in no frame yet, not frames, and groups only when all of
    /// the group is inside (`omitPartialGroups`).
    pub(super) fn adopt_into_frame(&mut self, frame: usize) {
        let owner = &self.scene.elements[frame];
        let id = owner.base.id.clone();
        let b = &owner.base;
        let [fx1, fy1, fx2, fy2] = [b.x, b.y, b.x + b.width, b.y + b.height];
        let inside = |e: &Element| {
            let [x1, y1, x2, y2] = geometry::element_bounds(e);
            fx1 <= x1 && fy1 <= y1 && fx2 >= x2 && fy2 >= y2
        };
        let candidates: HashSet<&str> = self
            .top_level()
            .filter(|e| {
                e.base.id != id
                    && !e.is_locked()
                    && e.frame_title().is_none()
                    && e.frame_id().is_none()
                    && inside(e)
            })
            .map(|e| e.base.id.as_str())
            .collect();
        let whole_groups = |e: &Element| {
            e.group_ids().iter().all(|group| {
                self.top_level()
                    .filter(|m| m.group_ids().contains(group))
                    .all(|m| candidates.contains(m.base.id.as_str()))
            })
        };
        let joining: HashSet<String> = self
            .top_level()
            .filter(|e| candidates.contains(e.base.id.as_str()) && whole_groups(e))
            .map(|e| e.base.id.clone())
            .collect();
        for element in self.scene.elements.iter_mut().filter(|e| {
            !e.base.is_deleted
                && (joining.contains(&e.base.id)
                    || container_id(e).is_some_and(|c| joining.contains(c)))
        }) {
            element
                .json_mut()
                .insert("frameId".into(), id.clone().into());
            element.touch();
        }
    }

    /// After a drag, the dragged elements join the frame they were dropped
    /// on if they overlap it, and leave their frame otherwise
    /// (`handleCanvasPointerUp`: `addElementsToFrame`, then
    /// `updateFrameMembershipOfSelectedElements`). A child whose frame is
    /// selected too keeps it.
    // ponytail: box overlap, element by element; Excalidraw intersects the
    // outlines and moves groups in or out whole
    pub(super) fn update_frame_membership(&mut self, at: Point) {
        let box_of = |e: &Element| {
            let b = &e.base;
            [b.x, b.y, b.x + b.width, b.y + b.height]
        };
        // The topmost unselected frame under the pointer
        // (`getTopLayerFrameAtSceneCoords`).
        let target: Option<(String, [f64; 4])> = self
            .scene
            .elements
            .iter()
            .rev()
            .filter(|e| !e.base.is_deleted && e.frame_title().is_some())
            .filter(|e| !self.selected.contains(&e.base.id))
            .find(|e| {
                let [x1, y1, x2, y2] = box_of(e);
                (x1..=x2).contains(&at[0]) && (y1..=y2).contains(&at[1])
            })
            .map(|e| (e.base.id.clone(), box_of(e)));
        let changes: Vec<(String, Option<String>)> = self
            .selection()
            .filter(|e| e.frame_title().is_none() && container_id(e).is_none())
            .filter(|e| !e.frame_id().is_some_and(|f| self.selected.contains(f)))
            .filter_map(|e| {
                let [x1, y1, x2, y2] = geometry::element_bounds(e);
                let joins = target.as_ref().filter(|(_, [fx1, fy1, fx2, fy2])| {
                    x1 <= *fx2 && x2 >= *fx1 && y1 <= *fy2 && y2 >= *fy1
                });
                let next = joins.map(|(id, _)| id.clone());
                (e.frame_id() != next.as_deref()).then(|| (e.base.id.clone(), next))
            })
            .collect();
        for (id, frame) in changes {
            let value = frame.map_or(serde_json::Value::Null, Into::into);
            for element in self.scene.elements.iter_mut().filter(|e| {
                !e.base.is_deleted && (e.base.id == id || container_id(e) == Some(id.as_str()))
            }) {
                element.json_mut().insert("frameId".into(), value.clone());
                element.touch();
            }
        }
    }

    /// [`Editor::moving`] plus the children of selected frames and their
    /// labels (`includeElementsInFrames`): what drags, nudges, duplicates,
    /// copies, flips and locks along with a frame.
    pub(super) fn moving_with_children(&self) -> Vec<(usize, Point)> {
        let children = self.frame_children();
        if children.is_empty() {
            return self.moving();
        }
        self.scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let id = e.base.id.as_str();
                let owner = container_id(e).unwrap_or(id);
                !e.base.is_deleted
                    && (self.selected.contains(id)
                        || self.selected.contains(owner)
                        || children.contains(id)
                        || children.contains(owner))
            })
            .map(|(i, e)| (i, [e.base.x, e.base.y]))
            .collect()
    }

    /// Ids of the live elements inside a selected frame (`frameId`).
    pub(super) fn frame_children(&self) -> HashSet<String> {
        let frames: HashSet<&str> = self
            .selection()
            .filter(|e| e.frame_title().is_some())
            .map(|e| e.base.id.as_str())
            .collect();
        self.scene
            .elements
            .iter()
            .filter(|e| !e.base.is_deleted && e.frame_id().is_some_and(|f| frames.contains(f)))
            .map(|e| e.base.id.clone())
            .collect()
    }
}
