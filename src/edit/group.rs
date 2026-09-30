//! Groups: group, ungroup, and group-aware selection (`actions/actionGroup.tsx`,
//! `groups.ts`). An element's `groupIds` run innermost first; selecting one
//! member selects its outermost group, or the next group inside the one
//! entered with a double-click.
use std::collections::{HashMap, HashSet};

use super::{Editor, common_bounds};
use crate::geometry::Bounds;
use crate::hit::container_id;
use crate::scene::Element;

impl Editor {
    /// Groups the selection (two or more elements) under a new outermost
    /// group, as one undo step. The members keep their order but move up to
    /// the topmost one, so the group is contiguous in the stack.
    pub(super) fn group(&mut self) {
        let targets = self.moving();
        let top_level = targets
            .iter()
            .filter(|(i, _)| container_id(&self.scene.elements[*i]).is_none())
            .count();
        if top_level < 2 {
            return;
        }
        // Exactly one whole group selected: it is already a group.
        if let [(only, _)] = &self.selected_groups()[..]
            && self
                .selection()
                .all(|e| e.group_ids().contains(&only.as_str()))
        {
            return;
        }
        let before = self.scene.elements.clone();
        let group = crate::random::id();
        for (index, _) in targets {
            let element = &mut self.scene.elements[index];
            let mut groups: Vec<String> =
                element.group_ids().into_iter().map(str::to_owned).collect();
            // Inside an entered group, the new group nests within it.
            let at = self
                .editing_group
                .as_ref()
                .and_then(|g| groups.iter().position(|x| x == g))
                .unwrap_or(groups.len());
            groups.insert(at, group.clone());
            element.set_group_ids(groups);
            element.touch();
        }
        let is_member = |e: &Element| e.group_ids().contains(&group.as_str());
        if let Some(last) = self.scene.elements.iter().rposition(is_member) {
            let after = self.scene.elements.split_off(last + 1);
            let (members, others): (Vec<Element>, Vec<Element>) =
                std::mem::take(&mut self.scene.elements)
                    .into_iter()
                    .partition(is_member);
            self.scene.elements = others.into_iter().chain(members).chain(after).collect();
        }
        self.history.record(before);
    }

    /// Dissolves the selected groups, as one undo step.
    pub(super) fn ungroup(&mut self) {
        let doomed: HashSet<String> = self
            .selected_groups()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        if doomed.is_empty() {
            return;
        }
        let before = self.scene.elements.clone();
        for element in &mut self.scene.elements {
            let groups: Vec<String> = element
                .group_ids()
                .into_iter()
                .filter(|g| !doomed.contains(*g))
                .map(str::to_owned)
                .collect();
            if groups.len() != element.group_ids().len() {
                element.set_group_ids(groups);
                element.touch();
            }
        }
        self.history.record(before);
    }

    /// Returns the selected groups (every member selected) with their box,
    /// for the dashed group border.
    pub fn selected_groups(&self) -> Vec<(String, Bounds)> {
        let mut seen = HashSet::new();
        let mut groups = Vec::new();
        for element in self.selection() {
            let Some(group) = self.selection_group(element) else {
                continue;
            };
            if !seen.insert(group.clone()) {
                continue;
            }
            let members: Vec<&Element> = self.group_members(&group).collect();
            if members.iter().all(|m| self.selected.contains(&m.base.id)) {
                let bounds = common_bounds(members.into_iter());
                groups.push((group, bounds));
            }
        }
        groups
    }

    /// Returns whether `id` is drawn as part of a selected group (no border
    /// of its own).
    pub fn in_selected_group(&self, id: &str) -> bool {
        let Some(element) = self.scene.elements.iter().find(|e| e.base.id == id) else {
            return false;
        };
        let Some(group) = self.selection_group(element) else {
            return false;
        };
        self.group_members(&group)
            .all(|m| self.selected.contains(&m.base.id))
    }
}

// Private API
impl Editor {
    /// The group that selecting `element` selects: its outermost group, or
    /// the next one inside the entered group. `None` selects it alone.
    pub(super) fn selection_group(&self, element: &Element) -> Option<String> {
        let groups = element.group_ids();
        let limit = match &self.editing_group {
            Some(entered) => groups.iter().position(|g| g == entered)?,
            None => groups.len(),
        };
        limit.checked_sub(1).map(|i| groups[i].to_owned())
    }

    /// Group ids copies keep (`getNewGroupIdsForDuplication`): the entered
    /// group and the groups around it, mapped to themselves; groups inside
    /// it get new ids.
    pub(super) fn kept_groups(&self) -> HashMap<String, String> {
        let Some(entered) = &self.editing_group else {
            return HashMap::new();
        };
        self.scene
            .elements
            .iter()
            .flat_map(|e| {
                let groups = e.group_ids();
                let from = groups.iter().position(|g| g == entered);
                from.map(|p| {
                    groups[p..]
                        .iter()
                        .map(|g| (g.to_string(), g.to_string()))
                        .collect()
                })
                .unwrap_or_else(Vec::new)
            })
            .collect()
    }

    /// Live, non-label elements in `group`.
    fn group_members<'a>(&'a self, group: &'a str) -> impl Iterator<Item = &'a Element> {
        self.scene.elements.iter().filter(move |e| {
            !e.base.is_deleted && container_id(e).is_none() && e.group_ids().contains(&group)
        })
    }

    /// Adds the whole group of every selected element to the selection.
    pub(super) fn expand_to_groups(&mut self) {
        let groups: HashSet<String> = self
            .selection()
            .filter_map(|e| self.selection_group(e))
            .collect();
        let members: Vec<String> = groups
            .iter()
            .flat_map(|g| {
                self.group_members(g)
                    .map(|e| e.base.id.clone())
                    .collect::<Vec<_>>()
            })
            .collect();
        self.selected.extend(members);
    }

    /// Double-click on a grouped element enters its group; returns whether
    /// it did.
    pub(super) fn enter_group(&mut self, id: &str) -> bool {
        let Some(element) = self.scene.elements.iter().find(|e| e.base.id == id) else {
            return false;
        };
        let Some(group) = self.selection_group(element) else {
            return false;
        };
        self.editing_group = Some(group);
        self.selected = std::iter::once(id.to_owned()).collect();
        self.expand_to_groups();
        true
    }
}
