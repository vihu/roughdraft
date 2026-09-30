//! Editing state machine: tools, selection, gestures and commands, iced-free.
//!
//! The widget converts input to scene coordinates and calls [`Editor`]; every
//! interaction is testable without a window. Behaviour follows Excalidraw
//! 0.18 as recorded in `.ai-docs/REFERENCE-001-excalidraw-editing-spec.md`.
mod clipboard;
mod commands;
mod create;
mod group;
mod order;
mod resize;
mod restyle;
mod select;
mod style;
mod text;
mod transform;

use std::collections::HashSet;

pub use self::order::Order;
pub use self::style::{Style, StyleChange};
pub use self::text::{ApproxMeasure, Measure};
pub use self::transform::{HANDLE_SIZE, Handle, Handles, POINT_RADIUS};
use crate::geometry::{self, Bounds, Point};
use crate::history::History;
use crate::hit::{self, container_id};
use crate::scene::{Element, Scene};

/// A scene being edited: selection, active tool, gesture and undo history.
#[derive(Debug)]
pub struct Editor {
    scene: Scene,
    selected: HashSet<String>,
    tool: Tool,
    locked: bool,
    style: Style,
    gesture: Option<Gesture>,
    multi: Option<Multi>,
    text: Option<text::TextEdit>,
    measure: Box<dyn Measure>,
    /// The group entered with a double-click; clicks select inside it.
    editing_group: Option<String>,
    history: History,
    zoom: f64,
}

/// What a pointer press on the canvas does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Select and move elements.
    #[default]
    Selection,
    /// Pan the canvas; the widget handles it.
    Hand,
    /// Drag out a rectangle.
    Rectangle,
    /// Drag out a diamond.
    Diamond,
    /// Drag out an ellipse.
    Ellipse,
    /// Drag, or click point by point, to draw an arrow.
    Arrow,
    /// Drag, or click point by point, to draw a line.
    Line,
    /// Click to type text; on a shape, its label.
    Text,
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

/// Pointer event phase for the primary button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// Button pressed.
    Down,
    /// Pointer moved with the button held.
    Move,
    /// Pointer moved with no button held; sent only while
    /// [`Editor::wants_hover`] is true.
    Hover,
    /// Button released.
    Up,
}

/// A keyboard action, already decoded from its shortcut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Switches tool. Any tool but selection clears the selection.
    Tool(Tool),
    /// Keeps the current tool after drawing (Excalidraw's tool lock, Q).
    ToggleLock,
    /// Finishes what is being drawn and returns to the selection tool.
    Escape,
    /// Enter: finishes a multi-point line or arrow, or starts editing the
    /// selected text or shape label.
    Finish,
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
    /// Moves the selection in the stack.
    Reorder(Order),
    /// Groups the selection (Ctrl+G).
    Group,
    /// Dissolves the selected groups (Ctrl+Shift+G).
    Ungroup,
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
    /// Dragging out a new rectangle, diamond or ellipse.
    Shape {
        index: usize,
        origin: Point,
        before: Vec<Element>,
    },
    /// Pressed with the line or arrow tool; `dragged` once past the threshold.
    Line {
        index: usize,
        origin: Point,
        before: Vec<Element>,
        dragged: bool,
    },
    /// Dragging a resize handle; `start` holds the elements as pressed.
    Resize {
        handle: Handle,
        before: Vec<Element>,
        start: Vec<(usize, Element)>,
        frame: transform::Frame,
        offset: Point,
    },
    /// Dragging the rotation knob around `center`.
    Rotate {
        before: Vec<Element>,
        start: Vec<(usize, Element)>,
        center: Point,
    },
    /// Dragging one end of a 2-point line or arrow.
    Endpoint {
        index: usize,
        which: usize,
        before: Vec<Element>,
    },
}

/// A line or arrow being drawn click by click; its last point follows the
/// pointer until the next click.
#[derive(Debug)]
struct Multi {
    index: usize,
    before: Vec<Element>,
}

// Public API
impl Editor {
    /// Starts editing `scene` with nothing selected.
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            selected: HashSet::new(),
            tool: Tool::default(),
            locked: false,
            style: Style::default(),
            gesture: None,
            multi: None,
            text: None,
            measure: Box::new(ApproxMeasure),
            editing_group: None,
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

    /// Returns whether the tool stays active after drawing.
    pub fn is_tool_locked(&self) -> bool {
        self.locked
    }

    /// Returns the style for new elements.
    pub fn style(&self) -> &Style {
        &self.style
    }

    /// Sets the style for new elements.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
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
        let id = |i: usize| self.scene.elements[i].base.id.as_str();
        if let Some(edit) = &self.text {
            let label = &self.scene.elements[edit.index];
            return std::iter::once(label.base.id.as_str())
                .chain(container_id(label))
                .collect();
        }
        match (&self.gesture, &self.multi) {
            (_, Some(multi)) => vec![id(multi.index)],
            (
                Some(Gesture::Move {
                    starts,
                    moved: true,
                    ..
                }),
                _,
            ) => starts.iter().map(|(i, _)| id(*i)).collect(),
            (
                Some(
                    Gesture::Shape { index, .. }
                    | Gesture::Line { index, .. }
                    | Gesture::Endpoint { index, .. },
                ),
                _,
            ) => vec![id(*index)],
            (Some(Gesture::Resize { start, .. } | Gesture::Rotate { start, .. }), _) => {
                start.iter().map(|(i, _)| id(*i)).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Whether the widget should send [`Pointer::Hover`] events.
    pub fn wants_hover(&self) -> bool {
        self.multi.is_some()
    }

    /// Tells the editor the canvas zoom, which scales hit tolerances.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = zoom;
    }

    /// Whether a press at `at` would grab something to move: an element, or
    /// the current selection when `at` is inside its box.
    pub fn grabs(&self, at: Point) -> bool {
        self.tool == Tool::Selection && (self.in_selection(at) || self.hit(at).is_some())
    }

    /// Handles a primary-button pointer event at a scene point.
    pub fn pointer(&mut self, pointer: Pointer, at: Point, modifiers: Modifiers) {
        match pointer {
            Pointer::Down => {
                // A click anywhere ends text editing first.
                self.finish_text();
                match self.tool {
                    Tool::Selection => self.select_press(at, modifiers),
                    Tool::Hand => {}
                    Tool::Text => self.text_press(at),
                    _ => self.create_press(at, modifiers),
                }
            }
            Pointer::Move | Pointer::Hover if self.multi.is_some() => {
                self.multi_hover(at, modifiers)
            }
            Pointer::Move => {
                let _ = self.select_drag(at, modifiers)
                    || self.drag_handle(at, modifiers)
                    || self.create_drag(at, modifiers);
            }
            Pointer::Hover => {}
            Pointer::Up => {
                if !self.select_release() {
                    self.create_release(at);
                }
            }
        }
    }

    /// Runs a keyboard command. Any command first finishes a multi-point
    /// line or arrow in progress.
    pub fn command(&mut self, command: Command) {
        self.finish_text();
        let drawing = self.multi.is_some();
        self.finish_multi();
        match command {
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
            Command::Reorder(order) => self.reorder(order),
            Command::Group => self.group(),
            Command::Ungroup => self.ungroup(),
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

    fn place(&mut self, index: usize, [x, y]: Point) {
        let element = &mut self.scene.elements[index];
        if (element.base.x, element.base.y) != (x, y) {
            element.base.x = x;
            element.base.y = y;
            element.touch();
        }
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
