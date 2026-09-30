//! Keyboard commands: the dispatch, delete and duplicate (REFERENCE-001
//! sections 7-8), element locks.
use std::collections::{HashMap, HashSet};

use super::{Command, Editor, Tool};
use crate::geometry::Point;
use crate::hit::container_id;
use crate::scene::Element;

/// `1 + FONT_SIZE_RELATIVE_INCREASE_STEP`.
const FONT_STEP: f64 = 1.1;

/// Scene units a duplicate is offset by (half the default grid).
const DUPLICATE_OFFSET: f64 = 10.0;

// Public API
impl Editor {
    /// Runs a keyboard command. Any command first finishes a multi-point
    /// line or arrow in progress.
    pub fn command(&mut self, command: Command) {
        self.finish_text();
        let drawing = self.multi.is_some();
        self.finish_multi();
        // The line editor stays open only for what it handles itself.
        if !matches!(
            command,
            Command::Delete | Command::Undo | Command::Redo | Command::Escape | Command::Nudge(_)
        ) {
            self.line_edit = None;
        }
        match command {
            Command::Escape if self.line_edit.is_some() => self.line_edit = None,
            Command::Delete if self.line_edit.is_some() => self.delete_line_points(),
            Command::EditLine => self.enter_line_editor(),
            Command::CopyStyles => self.copy_styles(),
            Command::Flip(axis) => self.flip(axis),
            Command::ToggleElementLock => self.toggle_element_lock(),
            Command::PasteStyles => self.paste_styles(),
            Command::Tool(tool) => {
                self.tool = tool;
                if tool != Tool::Selection {
                    self.selected.clear();
                }
            }
            Command::ToggleLock => {
                self.locked = !self.locked;
                if !self.locked {
                    self.tool = Tool::Selection;
                }
            }
            Command::Escape => self.tool = Tool::Selection,
            Command::Finish if drawing => {}
            Command::Finish => {
                let selected: Vec<(String, Point)> = self
                    .selection()
                    .map(|e| (e.base.id.clone(), [e.base.x, e.base.y]))
                    .collect();
                if let [(id, at)] = &selected[..] {
                    self.edit_element(id, *at);
                }
            }
            Command::Delete => self.delete(),
            Command::Duplicate => self.duplicate(),
            Command::SelectAll => {
                self.selected = self
                    .top_level()
                    .filter(|e| !e.is_locked())
                    .map(|e| e.base.id.clone())
                    .collect();
            }
            Command::Undo => {
                if self.history.undo(&mut self.scene.elements) {
                    self.prune_selection();
                    self.prune_line_edit();
                }
            }
            Command::Redo => {
                if self.history.redo(&mut self.scene.elements) {
                    self.prune_selection();
                    self.prune_line_edit();
                }
            }
            Command::Reorder(order) => self.reorder(order),
            Command::Group => self.group(),
            Command::Ungroup => self.ungroup(),
            Command::LargerFont => self.step_font_size(FONT_STEP),
            Command::SmallerFont => self.step_font_size(1.0 / FONT_STEP),
            Command::Nudge(offset) => {
                if !self.selected.is_empty() {
                    self.history.record(self.scene.elements.clone());
                    let moving = self.moving_with_children();
                    let moved = moving
                        .iter()
                        .map(|(i, _)| self.scene.elements[*i].base.id.clone())
                        .collect();
                    for (i, [x, y]) in moving {
                        self.place(i, [x + offset[0], y + offset[1]]);
                    }
                    self.update_bound_arrows(&moved);
                }
            }
        }
    }
}

// Private API
impl Editor {
    /// Deletes the selection and its labels; a deleted frame's children
    /// stay, out of the frame and selected (`actionDeleteSelected`).
    pub(super) fn delete(&mut self) {
        // Text being typed is committed first, so its element is not left
        // half-edited under a deleted container.
        self.finish_text();
        let children = self.frame_children();
        self.delete_keeping(&children);
    }

    /// Deletes the selection and its labels as one undo step, except the
    /// elements in `keep` and their labels, which leave their frame and
    /// become the selection.
    pub(super) fn delete_keeping(&mut self, keep: &HashSet<String>) {
        if self.selected.is_empty() {
            return;
        }
        self.history.record(self.scene.elements.clone());
        let freed = |e: &Element| {
            keep.contains(&e.base.id) || container_id(e).is_some_and(|c| keep.contains(c))
        };
        let doomed_indices: Vec<usize> = self
            .moving()
            .into_iter()
            .map(|(i, _)| i)
            .filter(|&i| !freed(&self.scene.elements[i]))
            .collect();
        let freed_indices: Vec<usize> = (0..self.scene.elements.len())
            .filter(|&i| !self.scene.elements[i].base.is_deleted && freed(&self.scene.elements[i]))
            .collect();
        let doomed: HashSet<String> = doomed_indices
            .into_iter()
            .map(|i| {
                let element = &mut self.scene.elements[i];
                element.base.is_deleted = true;
                element.touch();
                element.base.id.clone()
            })
            .collect();
        self.selected.clear();
        for i in freed_indices {
            let element = &mut self.scene.elements[i];
            element
                .json_mut()
                .insert("frameId".into(), serde_json::Value::Null);
            element.touch();
            if container_id(element).is_none() {
                self.selected.insert(element.base.id.clone());
            }
        }
        for element in self
            .scene
            .elements
            .iter_mut()
            .filter(|e| !e.base.is_deleted)
        {
            if element.forget_bindings(&doomed) {
                element.touch();
            }
        }
        self.tool = Tool::Selection;
    }

    /// Locks the selection with its labels unless all of it is already
    /// locked, then unlocks it (`actionToggleElementLock`). With nothing
    /// selected, unlocks every element and selects them
    /// (`actionUnlockAllElements`, which Excalidraw offers in its context
    /// menu; there is no context menu here).
    pub(super) fn toggle_element_lock(&mut self) {
        let unlock_all = self.selected.is_empty();
        let targets: Vec<usize> = if unlock_all {
            self.scene
                .elements
                .iter()
                .enumerate()
                .filter(|(_, e)| e.is_locked() && !e.base.is_deleted)
                .map(|(i, _)| i)
                .collect()
        } else {
            self.moving_with_children()
                .into_iter()
                .map(|(i, _)| i)
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let lock = !unlock_all && targets.iter().all(|&i| !self.scene.elements[i].is_locked());
        self.history.record(self.scene.elements.clone());
        for &i in &targets {
            let element = &mut self.scene.elements[i];
            element.json_mut().insert("locked".into(), lock.into());
            element.touch();
        }
        if lock {
            self.line_edit = None;
        }
        if unlock_all {
            self.selected = targets
                .iter()
                .map(|&i| &self.scene.elements[i])
                .filter(|e| container_id(e).is_none())
                .map(|e| e.base.id.clone())
                .collect();
        }
    }

    /// Duplicates the selection with its labels. Each copy goes right after
    /// its original, or after the container and label pair, and the copies
    /// become the selection.
    pub(super) fn duplicate(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.history.record(self.scene.elements.clone());
        let originals: Vec<usize> = self
            .moving_with_children()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        let new_ids: HashMap<String, String> = originals
            .iter()
            .map(|&i| (self.scene.elements[i].base.id.clone(), crate::random::id()))
            .collect();

        // A unit is a top-level element plus its labels; its copies follow
        // the unit's last member.
        let mut units: HashMap<&str, (usize, Vec<usize>)> = HashMap::new();
        for &i in &originals {
            let element = &self.scene.elements[i];
            let unit = container_id(element).unwrap_or(&element.base.id);
            let entry = units.entry(unit).or_insert((i, Vec::new()));
            entry.0 = entry.0.max(i);
            entry.1.push(i);
        }
        let mut groups = HashMap::new();
        let mut inserts: Vec<(usize, Vec<Element>)> = units
            .into_values()
            .map(|(last, members)| {
                let copies = members
                    .iter()
                    .map(|&i| self.copy_of(i, &new_ids, &mut groups))
                    .collect();
                (last, copies)
            })
            .collect();
        inserts.sort_by_key(|(last, _)| std::cmp::Reverse(*last));

        self.selected.clear();
        for (last, copies) in inserts {
            for copy in copies.iter().filter(|c| container_id(c).is_none()) {
                self.selected.insert(copy.base.id.clone());
            }
            self.scene.elements.splice(last + 1..last + 1, copies);
        }
    }

    fn copy_of(
        &self,
        index: usize,
        new_ids: &HashMap<String, String>,
        groups: &mut HashMap<String, String>,
    ) -> Element {
        let original = &self.scene.elements[index];
        let mut copy = original.duplicate();
        copy.base.id = new_ids[&original.base.id].clone();
        copy.base.x += DUPLICATE_OFFSET;
        copy.base.y += DUPLICATE_OFFSET;
        copy.remap(new_ids, groups);
        copy
    }
}
