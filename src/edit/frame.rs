//! Frames carry their children (`includeElementsInFrames`).
use std::collections::HashSet;

use super::Editor;
use crate::geometry::Point;
use crate::hit::container_id;

// Private API
impl Editor {
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
