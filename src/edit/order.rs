//! Z-order: send backward, bring forward, to back, to front
//! (`actions/actionZindex.tsx`). Array order is the source of truth;
//! Excalidraw re-derives fractional `index` values from it on load.
use super::Editor;
use crate::hit::container_id;
use crate::scene::Element;

/// Where to move the selection in the stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// One step down, past the next visible element.
    Backward,
    /// One step up, past the next visible element.
    Forward,
    /// To the bottom.
    ToBack,
    /// To the top.
    ToFront,
}

/// A top-level element with its labels, which move together.
struct Unit {
    elements: Vec<Element>,
    selected: bool,
    visible: bool,
    /// The top-level element's `groupIds`, innermost first.
    groups: Vec<String>,
}

impl Editor {
    /// Moves the selection in the stack, as one undo step.
    pub(super) fn reorder(&mut self, order: Order) {
        if self.selected.is_empty() {
            return;
        }
        let before = self.scene.elements.clone();
        let mut units = self.units();
        match order {
            Order::ToFront => units.sort_by_key(|u| u.selected),
            Order::ToBack => units.sort_by_key(|u| !u.selected),
            Order::Forward => step(&mut units, Direction::Up, self.editing_group.as_deref()),
            Order::Backward => step(&mut units, Direction::Down, self.editing_group.as_deref()),
        }
        self.scene.elements = units.into_iter().flat_map(|u| u.elements).collect();
        if before != self.scene.elements {
            self.history.record(before);
        }
    }

    /// Splits the array into units: each element that is not a label, with
    /// its labels right after it. Orphan labels stay units of their own.
    fn units(&self) -> Vec<Unit> {
        let elements = &self.scene.elements;
        let is_label_of =
            |label: &Element, owner: &Element| container_id(label) == Some(owner.base.id.as_str());
        let owned = |label: &Element| elements.iter().any(|owner| is_label_of(label, owner));
        elements
            .iter()
            .filter(|e| !owned(e))
            .map(|owner| {
                let labels = elements.iter().filter(|l| is_label_of(l, owner)).cloned();
                Unit {
                    elements: std::iter::once(owner.clone()).chain(labels).collect(),
                    selected: self.selected.contains(&owner.base.id),
                    visible: !owner.base.is_deleted,
                    groups: owner.group_ids().into_iter().map(String::from).collect(),
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

/// Moves every selected unit one visible, unselected unit up or down,
/// keeping selected runs together. Processing from the moving end first
/// lets a whole run pass the same neighbour. A neighbour in a group is
/// passed as a whole, and nothing leaves the entered group
/// (`getTargetIndex`).
fn step(units: &mut Vec<Unit>, direction: Direction, entered: Option<&str>) {
    let order: Vec<usize> = match direction {
        Direction::Up => (0..units.len()).rev().collect(),
        Direction::Down => (0..units.len()).collect(),
    };
    for start in order {
        if !units[start].selected || !units[start].visible {
            continue;
        }
        // In an entered group only its members count as neighbours
        // (`indexFilter`), whatever lies between them.
        let candidate = |i: &usize| {
            units[*i].visible && entered.is_none_or(|g| units[*i].groups.iter().any(|x| x == g))
        };
        let next = match direction {
            Direction::Up => (start + 1..units.len()).find(candidate),
            Direction::Down => (0..start).rev().find(candidate),
        };
        let Some(mut target) = next.filter(|&i| !units[i].selected) else {
            continue;
        };
        // A neighbour in a group of its own is passed as a whole; a sibling
        // in the same group is just stepped over (`getTargetIndex`).
        let groups = &units[target].groups;
        let sibling = if *groups == units[start].groups {
            None
        } else {
            match entered {
                Some(entered) => groups
                    .iter()
                    .position(|g| g == entered)
                    .and_then(|at| at.checked_sub(1))
                    .map(|i| groups[i].clone()),
                None => groups.last().cloned(),
            }
        };
        if let Some(group) = sibling {
            let mut members = (0..units.len()).filter(|&i| units[i].groups.contains(&group));
            let edge = match direction {
                Direction::Up => members.next_back(),
                Direction::Down => members.next(),
            };
            target = edge.unwrap_or(target);
        }
        let unit = units.remove(start);
        units.insert(target, unit);
    }
}
