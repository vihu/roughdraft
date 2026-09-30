//! Line editor (`element/linearElementEditor.ts`): Ctrl+Enter or
//! Ctrl+double-click on a line or arrow shows every point and every segment
//! middle; points can be selected (Shift adds), dragged together, added from
//! a middle and deleted. Escape or a click elsewhere leaves it.
use super::{Editor, Gesture, Modifiers};
use crate::geometry::{self, Point};
use crate::scene::Kind;

/// The line or arrow being edited point by point, and its selected points.
#[derive(Debug)]
pub(super) struct LineEdit {
    pub(super) id: String,
    pub(super) selected: Vec<usize>,
}

// Public API
impl Editor {
    /// Returns the id of the line or arrow in the line editor, if any.
    pub fn editing_line(&self) -> Option<&str> {
        self.line_edit.as_ref().map(|edit| edit.id.as_str())
    }
}

// Private API
impl Editor {
    /// Opens the line editor on the one selected line or arrow.
    pub(super) fn enter_line_editor(&mut self) {
        let selected: Vec<_> = self.selection().collect();
        if let [one] = selected[..]
            && matches!(one.kind, Kind::Line(_) | Kind::Arrow(_))
        {
            self.line_edit = Some(LineEdit {
                id: one.base.id.clone(),
                selected: Vec::new(),
            });
        }
    }

    /// Index of the edited element in the scene.
    pub(super) fn line_edit_index(&self) -> Option<usize> {
        let id = &self.line_edit.as_ref()?.id;
        self.scene
            .elements
            .iter()
            .position(|e| &e.base.id == id && !e.base.is_deleted)
    }

    /// Press on point `which` in the editor: Shift toggles it in the
    /// selection, otherwise it becomes the selection unless already in it.
    /// Returns the gesture that drags the selected points.
    pub(super) fn press_line_point(
        &mut self,
        index: usize,
        which: usize,
        at: Point,
        modifiers: Modifiers,
    ) -> Gesture {
        let before = self.scene.elements.clone();
        let edit = self.line_edit.as_mut().expect("called in the line editor");
        if modifiers.shift {
            match edit.selected.iter().position(|&p| p == which) {
                Some(found) => {
                    edit.selected.remove(found);
                }
                None => edit.selected.push(which),
            }
        } else if !edit.selected.contains(&which) {
            edit.selected = vec![which];
        }
        let element = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
            unreachable!("the line editor only opens on lines and arrows")
        };
        let transform = geometry::element_transform(element);
        let starts = edit
            .selected
            .iter()
            .map(|&p| (p, transform.apply(line.points[p])))
            .collect();
        Gesture::Points {
            index,
            before,
            from: at,
            starts,
        }
    }

    /// Moves the selected points by the pointer's travel since the press.
    pub(super) fn drag_line_points(
        &mut self,
        index: usize,
        from: Point,
        starts: &[(usize, Point)],
        at: Point,
    ) {
        let element = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
            return;
        };
        let to_local = geometry::element_transform(element).inverse();
        let mut points = line.points.clone();
        let delta = [at[0] - from[0], at[1] - from[1]];
        for &(p, [x, y]) in starts {
            points[p] = to_local.apply([x + delta[0], y + delta[1]]);
        }
        self.rebase_points(index, points);
    }

    /// Delete in the editor: removes the selected points, or the whole
    /// element when fewer than two would be left, as one undo step.
    pub(super) fn delete_line_points(&mut self) {
        let (Some(index), Some(edit)) = (self.line_edit_index(), self.line_edit.as_mut()) else {
            return;
        };
        if edit.selected.is_empty() {
            return;
        }
        let element = &self.scene.elements[index];
        let (Kind::Line(line) | Kind::Arrow(line)) = &element.kind else {
            return;
        };
        let mut doomed = std::mem::take(&mut edit.selected);
        doomed.sort_unstable();
        doomed.dedup();
        if line.points.len() - doomed.len() < 2 {
            self.line_edit = None;
            self.delete();
            return;
        }
        let before = self.scene.elements.clone();
        let points = line
            .points
            .iter()
            .enumerate()
            .filter(|(i, _)| doomed.binary_search(i).is_err())
            .map(|(_, p)| *p)
            .collect();
        self.rebase_points(index, points);
        self.bind_arrow_ends(index);
        if let Some(edit) = &mut self.line_edit {
            edit.selected = vec![doomed[0].saturating_sub(1)];
        }
        self.history.record(before);
    }

    /// Drops selected points that no longer exist (after undo).
    pub(super) fn prune_line_edit(&mut self) {
        let Some(index) = self.line_edit_index() else {
            self.line_edit = None;
            return;
        };
        let (Kind::Line(line) | Kind::Arrow(line)) = &self.scene.elements[index].kind else {
            self.line_edit = None;
            return;
        };
        let len = line.points.len();
        if let Some(edit) = &mut self.line_edit {
            edit.selected.retain(|&p| p < len);
        }
    }
}
