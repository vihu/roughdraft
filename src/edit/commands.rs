//! Delete and duplicate (REFERENCE-001 sections 7-8).
use std::collections::{HashMap, HashSet};

use super::{Editor, Tool};
use crate::hit::container_id;
use crate::scene::Element;

/// Scene units a duplicate is offset by (half the default grid).
const DUPLICATE_OFFSET: f64 = 10.0;

impl Editor {
    pub(super) fn delete(&mut self) {
        // Text being typed is committed first, so its element is not left
        // half-edited under a deleted container.
        self.finish_text();
        if self.selected.is_empty() {
            return;
        }
        self.history.record(self.scene.elements.clone());
        let doomed: HashSet<String> = self
            .moving()
            .into_iter()
            .map(|(i, _)| {
                let element = &mut self.scene.elements[i];
                element.base.is_deleted = true;
                element.touch();
                element.base.id.clone()
            })
            .collect();
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
        self.selected.clear();
        self.tool = Tool::Selection;
    }

    /// Duplicates the selection with its labels. Each copy goes right after
    /// its original, or after the container and label pair, and the copies
    /// become the selection.
    pub(super) fn duplicate(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        self.history.record(self.scene.elements.clone());
        let originals: Vec<usize> = self.moving().into_iter().map(|(i, _)| i).collect();
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
