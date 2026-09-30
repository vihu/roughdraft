//! The canvas `Program`: pointer and keyboard input to [`Sketch`] messages,
//! and the layered draw.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::{self, Event, Frame, Geometry};
use iced::{Point, Rectangle, Renderer, Theme};

use super::camera::zoom_key;
use super::keys::{code_shortcut, shortcut};
use super::{Input, Message, Sketch};
use crate::edit::{self, Handle, Pointer, Tool};

/// Transient input state of the canvas; pan and zoom live in [`Sketch`].
#[derive(Debug, Default)]
pub struct State {
    pan_from: Option<Point>,
    pressed: bool,
    space: bool,
    modifiers: keyboard::Modifiers,
    last_click: Option<(Instant, Point)>,
    /// A press landed outside the canvas (another widget, the tool bar or
    /// the style panel): keys belong to that widget until the next press
    /// on the canvas.
    unfocused: bool,
}

/// Pixels per scrolled line.
const LINE_HEIGHT: f64 = 50.0;

/// Two presses closer than this in time and space are a double-click.
const DOUBLE_CLICK: (Duration, f32) = (Duration::from_millis(500), 6.0);

impl State {
    fn modifiers(&self) -> edit::Modifiers {
        edit::Modifiers {
            shift: self.modifiers.shift(),
            alt: self.modifiers.alt(),
            command: self.modifiers.command(),
        }
    }
}

impl canvas::Program<Message> for Sketch {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let publish = |input| Some(canvas::Action::publish(Message(input)).and_capture());
        let camera = self.camera.get();
        let typing = self.editor.editing().is_some();
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            // The text overlay owns the keyboard while typing, and another
            // widget does after a press outside the canvas.
            // A colour being typed in the style panel keeps the arrow keys
            // (its text field does not take them).
            Event::Keyboard(_) if typing || state.unfocused || self.color_draft.is_some() => None,
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Space),
                ..
            }) => {
                state.space = true;
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: Key::Named(Named::Space),
                ..
            }) => {
                state.space = false;
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                physical_key,
                ..
            }) => {
                let zoom = match physical_key {
                    keyboard::key::Physical::Code(code) => zoom_key(*code, *modifiers),
                    keyboard::key::Physical::Unidentified(_) => None,
                };
                if let Some(zoom) = zoom {
                    return publish(Input::ZoomKey(zoom));
                }
                if let keyboard::key::Physical::Code(code) = physical_key
                    && let Some(command) = code_shortcut(*code, *modifiers)
                {
                    return publish(Input::Command(command));
                }
                let clipboard = match key.as_ref() {
                    Key::Character(c) if modifiers.command() => match c.to_lowercase().as_str() {
                        "c" => Some(Input::Copy),
                        "x" => Some(Input::Cut),
                        "v" => {
                            // Paste at the pointer, or the canvas centre.
                            let at = cursor.position_over(bounds).unwrap_or(bounds.center());
                            Some(Input::Paste(camera.scene_point(bounds, at)))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                clipboard
                    .or_else(|| shortcut(key, *modifiers).map(Input::Command))
                    .and_then(publish)
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    state.unfocused = true;
                    return None;
                };
                state.unfocused = false;
                let pans = *button == mouse::Button::Middle
                    || (*button == mouse::Button::Left
                        && (state.space || self.editor.tool() == Tool::Hand));
                if pans {
                    state.pan_from = Some(position);
                    return Some(canvas::Action::capture());
                }
                if *button != mouse::Button::Left {
                    return None;
                }
                let at = camera.scene_point(bounds, position);
                let double = state.last_click.is_some_and(|(time, place)| {
                    time.elapsed() <= DOUBLE_CLICK.0 && place.distance(position) <= DOUBLE_CLICK.1
                });
                if double && self.editor.tool() == Tool::Selection {
                    state.last_click = None;
                    // Ctrl+double-click opens the line editor.
                    if state.modifiers.command() {
                        return publish(Input::Command(edit::Command::EditLine));
                    }
                    return publish(Input::DoubleClick(at));
                }
                state.last_click = Some((Instant::now(), position));
                state.pressed = true;
                publish(Input::Pointer(Pointer::Down, at, state.modifiers()))
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(from) = state.pan_from {
                    state.pan_from = Some(*position);
                    let delta = *position - from;
                    return publish(Input::Pan([f64::from(delta.x), f64::from(delta.y)]));
                }
                let pointer = if state.pressed {
                    Pointer::Move
                } else if self.editor.wants_hover() {
                    Pointer::Hover
                } else {
                    return None;
                };
                let at = camera.scene_point(bounds, *position);
                publish(Input::Pointer(pointer, at, state.modifiers()))
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                if state.pan_from.take().is_some() {
                    return Some(canvas::Action::capture());
                }
                if !std::mem::take(&mut state.pressed) {
                    return None;
                }
                let position = cursor.position().unwrap_or_default();
                let at = camera.scene_point(bounds, position);
                publish(Input::Pointer(Pointer::Up, at, state.modifiers()))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let at = cursor.position_in(bounds)?;
                let (dx, dy) = match *delta {
                    ScrollDelta::Lines { x, y } => {
                        (f64::from(x) * LINE_HEIGHT, f64::from(y) * LINE_HEIGHT)
                    }
                    ScrollDelta::Pixels { x, y } => (f64::from(x), f64::from(y)),
                };
                if state.modifiers.command() {
                    let cursor = [f64::from(at.x), f64::from(at.y)];
                    return publish(Input::Zoom {
                        factor: (dy / 500.0).exp(),
                        cursor,
                    });
                }
                // Shift scrolls sideways; macOS swaps the axes itself.
                let (dx, dy) = if state.modifiers.shift() && !cfg!(target_os = "macos") {
                    (dy, dx)
                } else {
                    (dx, dy)
                };
                // A positive delta (wheel up) moves the content down, like
                // iced's scrollable and Excalidraw.
                publish(Input::Pan([dx, dy]))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        if self.unplaced.replace(false) {
            self.centre_content(size);
        }
        let view = self.camera.get().view();
        self.viewport.set(size);
        let active: HashSet<&str> = self.active.iter().map(String::as_str).collect();
        let first = self
            .order
            .iter()
            .position(|id| active.contains(id.as_str()));
        let last = self
            .order
            .iter()
            .rposition(|id| active.contains(id.as_str()));
        let (below, middle, above) = match (first, last) {
            (Some(first), Some(last)) => (
                &self.order[..first],
                &self.order[first..=last],
                &self.order[last + 1..],
            ),
            _ => (&self.order[..], &[][..], &[][..]),
        };

        let below = self.below.draw(renderer, size, |frame| {
            // Through a clip draft like the elements (`draw_ids`), or wgpu
            // would put it over them.
            let full = iced::Rectangle::with_size(frame.size());
            frame.with_clip(full, |frame| {
                frame.fill_rectangle(Point::ORIGIN, frame.size(), self.paint(self.background));
            });
            self.draw_ids(frame, below, view, &HashMap::new());
        });
        let mut dynamic = Frame::new(renderer, size);
        let fresh = self.render_stale(middle);
        self.draw_ids(&mut dynamic, middle, view, &fresh);
        let above = self.above.draw(renderer, size, |frame| {
            self.draw_ids(frame, above, view, &HashMap::new())
        });
        let mut overlay = Frame::new(renderer, size);
        self.draw_overlay(&mut overlay, self.camera.get().zoom, view);
        vec![
            below,
            dynamic.into_geometry(),
            above,
            overlay.into_geometry(),
        ]
    }

    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_over(bounds) else {
            return mouse::Interaction::default();
        };
        if state.pan_from.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if state.space || self.editor.tool() == Tool::Hand {
            return mouse::Interaction::Grab;
        }
        match self.editor.tool() {
            Tool::Selection => {}
            Tool::Text => return mouse::Interaction::Text,
            _ => return mouse::Interaction::Crosshair,
        }
        let at = self.camera.get().scene_point(bounds, position);
        if let Some(handle) = self.editor.handle_at(at) {
            return match handle {
                Handle::N | Handle::S => mouse::Interaction::ResizingVertically,
                Handle::W | Handle::E => mouse::Interaction::ResizingHorizontally,
                Handle::Nw | Handle::Se => mouse::Interaction::ResizingDiagonallyDown,
                Handle::Ne | Handle::Sw => mouse::Interaction::ResizingDiagonallyUp,
                Handle::Rotation => mouse::Interaction::Grab,
            };
        }
        if self.editor.grabs(at) {
            mouse::Interaction::Move
        } else {
            mouse::Interaction::default()
        }
    }
}
