//! Frames take in what is drawn inside them and carry their children
//! (`frame.ts`, `includeElementsInFrames`).
use std::collections::{HashMap, HashSet};

use super::Editor;
use crate::geometry::{self, Bounds, Point};
use crate::hit::container_id;
use crate::scene::Element;

// Private API
impl Editor {
    /// Makes what lies wholly inside a frame its children, labels
    /// included (`getElementsInNewFrame`, `addElementsToFrame`): unlocked
    /// elements in no frame yet, not frames, and groups only when all of
    /// the group is inside (`omitPartialGroups`).
    pub(super) fn adopt_into_frame(&mut self, frame: usize) {
        let owner = &self.scene.elements[frame];
        let id = owner.base.id.clone();
        let [fx1, fy1, fx2, fy2] = frame_box(owner);
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
        self.set_frame(&joining, Some(&id));
    }

    /// After a drag, the dragged elements join the frame they were dropped
    /// on if they overlap it, and leave their frame otherwise
    /// (`handleCanvasPointerUp`: `addElementsToFrame`, then
    /// `updateFrameMembershipOfSelectedElements`). A child whose frame is
    /// selected too keeps it; with a frame among the dragged elements
    /// nothing joins a frame (no `frameToHighlight`).
    pub(super) fn update_frame_membership(&mut self, at: Point) {
        let drags_frame = self.selection().any(|e| e.frame_title().is_some());
        let target = if drags_frame {
            None
        } else {
            self.frame_at(at, &self.selected)
        };
        let changes: Vec<(String, Option<String>)> = self
            .selection()
            .filter(|e| e.frame_title().is_none() && container_id(e).is_none())
            .filter(|e| !e.frame_id().is_some_and(|f| self.selected.contains(f)))
            .filter_map(|e| {
                let joins = target.as_ref().filter(|(_, frame)| overlaps(e, *frame));
                let next = joins.map(|(id, _)| id.clone());
                (e.frame_id() != next.as_deref()).then(|| (e.base.id.clone(), next))
            })
            .collect();
        for (id, frame) in changes {
            self.set_frame(&HashSet::from([id]), frame.as_deref());
        }
    }

    /// After resizing frames (`getElementsInResizingFrame`): children that
    /// no longer overlap their frame leave it, and what now lies wholly
    /// inside joins.
    pub(super) fn refit_selected_frames(&mut self) {
        let frames: Vec<usize> = self
            .scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !e.base.is_deleted
                    && e.frame_title().is_some()
                    && self.selected.contains(&e.base.id)
            })
            .map(|(i, _)| i)
            .collect();
        for index in frames {
            let frame = &self.scene.elements[index];
            let (id, bounds) = (frame.base.id.clone(), frame_box(frame));
            let leaving: HashSet<String> = self
                .top_level()
                .filter(|e| e.frame_id() == Some(id.as_str()) && !overlaps(e, bounds))
                .map(|e| e.base.id.clone())
                .collect();
            self.set_frame(&leaving, None);
            self.adopt_into_frame(index);
        }
    }

    /// Pasted elements join the topmost frame under the paste point
    /// (`addElementsFromPasteOrLibrary`): those in no frame, not frames.
    pub(super) fn paste_into_frame(&mut self, at: Point, pasted: &HashSet<String>) {
        let Some((frame, _)) = self.frame_at(at, pasted) else {
            return;
        };
        let joining: HashSet<String> = self
            .top_level()
            .filter(|e| {
                pasted.contains(&e.base.id) && e.frame_title().is_none() && e.frame_id().is_none()
            })
            .map(|e| e.base.id.clone())
            .collect();
        self.set_frame(&joining, Some(&frame));
    }

    /// Drops selected elements whose frame is selected too
    /// (`excludeElementsInFramesFromSelection`): the frame stands for them.
    pub(super) fn exclude_framed_from_selection(&mut self) {
        let framed: Vec<String> = self
            .selection()
            .filter(|e| e.frame_id().is_some_and(|f| self.selected.contains(f)))
            .map(|e| e.base.id.clone())
            .collect();
        for id in framed {
            self.selected.remove(&id);
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

    /// The topmost live frame whose box holds `at`, leaving out `skip`
    /// (`getTopLayerFrameAtSceneCoords`).
    fn frame_at(&self, at: Point, skip: &HashSet<String>) -> Option<(String, Bounds)> {
        self.scene
            .elements
            .iter()
            .rev()
            .filter(|e| !e.base.is_deleted && e.frame_title().is_some())
            .filter(|e| !skip.contains(&e.base.id))
            .find(|e| {
                let [x1, y1, x2, y2] = frame_box(e);
                (x1..=x2).contains(&at[0]) && (y1..=y2).contains(&at[1])
            })
            .map(|e| (e.base.id.clone(), frame_box(e)))
    }

    /// Puts the elements `ids` and their labels in `frame`, or in none.
    fn set_frame(&mut self, ids: &HashSet<String>, frame: Option<&str>) {
        if ids.is_empty() {
            return;
        }
        let value = frame.map_or(serde_json::Value::Null, Into::into);
        for element in self.scene.elements.iter_mut().filter(|e| {
            !e.base.is_deleted
                && (ids.contains(&e.base.id) || container_id(e).is_some_and(|c| ids.contains(c)))
        }) {
            element.json_mut().insert("frameId".into(), value.clone());
            element.touch();
        }
    }
}

/// A frame's box in scene coordinates (frames do not rotate).
fn frame_box(frame: &Element) -> Bounds {
    let b = &frame.base;
    [b.x, b.y, b.x + b.width, b.y + b.height]
}

/// Whether an element's rotated box meets `frame`'s box
/// (`elementOverlapsWithFrame`: inside, intersecting or containing).
fn overlaps(element: &Element, [fx1, fy1, fx2, fy2]: Bounds) -> bool {
    let [x1, y1, x2, y2] = geometry::element_bounds(element);
    x1 <= fx2 && x2 >= fx1 && y1 <= fy2 && y2 >= fy1
}

/// A copy of a frame child stays in the frame when the frame is not copied
/// along (`bindElementsToFramesAfterDuplication`); `remap` has cleared it.
pub(super) fn keep_frame(copy: &mut Element, original: &Element, ids: &HashMap<String, String>) {
    if let Some(frame) = original.frame_id().filter(|f| !ids.contains_key(*f)) {
        copy.json_mut().insert("frameId".into(), frame.into());
    }
}
