//! Editing state machine: tools, selection, gestures and commands, iced-free.
//!
//! The widget converts input to scene coordinates and calls [`Editor`]; every
//! interaction is testable without a window. Behaviour follows Excalidraw
//! 0.18 as recorded in `.ai-docs/REFERENCE-001-excalidraw-editing-spec.md`.
use std::collections::{HashMap, HashSet};

use crate::geometry::{self, Bounds, Point};
use crate::history::History;
use crate::hit::{self, container_id};
use crate::scene::{Element, Kind, Scene};

/// A scene being edited: selection, active tool, gesture and undo history.
#[derive(Debug)]
pub struct Editor {
    scene: Scene,
    selected: HashSet<String>,
    tool: Tool,
    gesture: Option<Gesture>,
    history: History,
    zoom: f64,
}

/// What a pointer drag on the canvas does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Select and move elements.
    #[default]
    Selection,
    /// Pan the canvas; the widget handles it.
    Hand,
}

/// Modifier keys held during a pointer event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Alt / Option.
    pub alt: bool,
    /// Ctrl, or Cmd on macOS.
    pub command: bool,
}

/// Primary-button pointer event phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// Button pressed.
    Down,
    /// Pointer moved with the button held.
    Move,
    /// Button released.
    Up,
}

/// A keyboard action, already decoded from its shortcut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Switches tool. Any tool but selection clears the selection.
    Tool(Tool),
    /// Deletes the selection and its labels.
    Delete,
    /// Duplicates the selection, offset down and right.
    Duplicate,
    /// Selects every element.
    SelectAll,
    /// Undoes the last action.
    Undo,
    /// Redoes the last undone action.
    Redo,
    /// Moves the selection by a scene-unit offset.
    Nudge(Point),
}

#[derive(Debug)]
enum Gesture {
    /// Dragging the selection. `clicked` narrows the selection to itself if
    /// the press ends without moving.
    Move {
        from: Point,
        before: Vec<Element>,
        starts: Vec<(usize, Point)>,
        moved: bool,
        clicked: Option<String>,
    },
    /// Rubber-band selection; `keep` stays selected (shift).
    Marquee {
        from: Point,
        to: Point,
        keep: HashSet<String>,
    },
}

/// Scene units a duplicate is offset by (half the default grid).
const DUPLICATE_OFFSET: f64 = 10.0;

// Public API
impl Editor {
    /// Starts editing `scene` with nothing selected.
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            selected: HashSet::new(),
            tool: Tool::default(),
            gesture: None,
            history: History::default(),
            zoom: 1.0,
        }
    }

    /// Returns the scene.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Returns the active tool.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Returns whether the element is selected.
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.contains(id)
    }

    /// Returns the selected elements in z-order.
    pub fn selection(&self) -> impl Iterator<Item = &Element> {
        self.scene
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.base.id))
    }

    /// Returns the rubber-band rectangle while box-selecting.
    pub fn marquee(&self) -> Option<Bounds> {
        match &self.gesture {
            Some(Gesture::Marquee { from, to, .. }) => Some(normalize(*from, *to)),
            _ => None,
        }
    }

    /// Returns the ids of elements that change on every pointer move of the
    /// current gesture, so the widget can keep the rest cached.
    pub fn active(&self) -> Vec<&str> {
        match &self.gesture {
            Some(Gesture::Move {
                starts,
                moved: true,
                ..
            }) => starts
                .iter()
                .map(|(i, _)| self.scene.elements[*i].base.id.as_str())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Tells the editor the canvas zoom, which scales hit tolerances.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom;
    }

    /// Whether a press at `at` would grab something to move: an element, or
    /// the current selection when `at` is inside its box.
    pub fn grabs(&self, at: Point) -> bool {
        self.in_selection(at) || self.hit(at).is_some()
    }

    /// Handles a primary-button pointer event at a scene point.
    pub fn pointer(&mut self, pointer: Pointer, at: Point, modifiers: Modifiers) {
        match (pointer, self.tool) {
            (Pointer::Down, Tool::Selection) => self.press(at, modifiers),
            (Pointer::Down, Tool::Hand) => {}
            (Pointer::Move, _) => self.drag(at, modifiers),
            (Pointer::Up, _) => self.release(),
        }
    }

    /// Runs a keyboard command.
    pub fn command(&mut self, command: Command) {
        match command {
            Command::Tool(tool) => {
                self.tool = tool;
                if tool != Tool::Selection {
                    self.selected.clear();
                }
            }
            Command::Delete => self.delete(),
            Command::Duplicate => self.duplicate(),
            Command::SelectAll => {
                self.selected = self.top_level().map(|e| e.base.id.clone()).collect();
            }
            Command::Undo => {
                if self.history.undo(&mut self.scene.elements) {
                    self.prune_selection();
                }
            }
            Command::Redo => {
                if self.history.redo(&mut self.scene.elements) {
                    self.prune_selection();
                }
            }
            Command::Nudge(offset) => {
                if !self.selected.is_empty() {
                    self.history.record(self.scene.elements.clone());
                    for (i, [x, y]) in self.moving() {
                        self.place(i, [x + offset[0], y + offset[1]]);
                    }
                }
            }
        }
    }
}

// Private API
impl Editor {
    fn threshold(&self) -> f64 {
        hit::THRESHOLD / self.zoom
    }

    fn hit(&self, at: Point) -> Option<&Element> {
        hit::element_at(&self.scene, at, self.threshold())
    }

    /// Whether `at` is inside the selection's box, padded by the threshold:
    /// the element's rotated box for one, the common box for several.
    fn in_selection(&self, at: Point) -> bool {
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

    fn press(&mut self, at: Point, modifiers: Modifiers) {
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

    fn drag(&mut self, at: Point, modifiers: Modifiers) {
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
                    return;
                }
                *moved = true;
                let targets: Vec<(usize, Point)> = starts
                    .iter()
                    .map(|&(i, [x, y])| (i, [x + offset[0], y + offset[1]]))
                    .collect();
                for (i, position) in targets {
                    self.place(i, position);
                }
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
            }
            None => {}
        }
    }

    fn release(&mut self) {
        match self.gesture.take() {
            Some(Gesture::Move {
                before,
                moved: true,
                ..
            }) => self.history.record(before),
            Some(Gesture::Move {
                clicked: Some(id), ..
            }) => self.selected = HashSet::from([id]),
            _ => {}
        }
    }

    fn place(&mut self, index: usize, [x, y]: Point) {
        let element = &mut self.scene.elements[index];
        if (element.base.x, element.base.y) != (x, y) {
            element.base.x = x;
            element.base.y = y;
            element.touch();
        }
    }

    fn delete(&mut self) {
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
    fn duplicate(&mut self) {
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
        let mut inserts: Vec<(usize, Vec<Element>)> = units
            .into_values()
            .map(|(last, members)| {
                let copies = members.iter().map(|&i| self.copy_of(i, &new_ids)).collect();
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

    fn copy_of(&self, index: usize, new_ids: &HashMap<String, String>) -> Element {
        let original = &self.scene.elements[index];
        let mut copy = original.duplicate();
        copy.base.id = new_ids[&original.base.id].clone();
        copy.base.x += DUPLICATE_OFFSET;
        copy.base.y += DUPLICATE_OFFSET;
        if let Kind::Text(text) = &mut copy.kind
            && let Some(container) = &mut text.container_id
            && let Some(new) = new_ids.get(container)
        {
            *container = new.clone();
        }
        if let Some(bound) = &mut copy.base.bound_elements {
            bound.retain(|b| new_ids.contains_key(&b.id));
            bound.iter_mut().for_each(|b| b.id = new_ids[&b.id].clone());
        }
        copy
    }

    /// Selected elements plus their labels, with their current positions.
    fn moving(&self) -> Vec<(usize, Point)> {
        self.scene
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !e.base.is_deleted
                    && (self.selected.contains(&e.base.id)
                        || container_id(e).is_some_and(|c| self.selected.contains(c)))
            })
            .map(|(i, e)| (i, [e.base.x, e.base.y]))
            .collect()
    }

    /// Live elements that are not labels (labels follow their container).
    fn top_level(&self) -> impl Iterator<Item = &Element> {
        self.scene
            .elements
            .iter()
            .filter(|e| !e.base.is_deleted && container_id(e).is_none())
    }

    fn prune_selection(&mut self) {
        let live: HashSet<String> = self.top_level().map(|e| e.base.id.clone()).collect();
        self.selected.retain(|id| live.contains(id));
    }
}

/// Box around several elements' rotated boxes.
pub fn common_bounds<'a>(elements: impl Iterator<Item = &'a Element>) -> Bounds {
    elements.map(geometry::element_bounds).fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[a, b, c, d], [x1, y1, x2, y2]| [a.min(x1), b.min(y1), c.max(x2), d.max(y2)],
    )
}

fn normalize([x1, y1]: Point, [x2, y2]: Point) -> Bounds {
    [x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)]
}

#[cfg(test)]
mod tests;
