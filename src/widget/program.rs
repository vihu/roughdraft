//! The canvas `Program`: camera (pan and zoom), input to editor messages,
//! and the layered draw.
use std::collections::HashSet;

use iced::keyboard::{self, Key, key::Named};
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::{self, Event, Frame, Geometry};
use iced::{Point, Rectangle, Renderer, Theme};

use super::{Input, Message, Sketch, shortcut};
use crate::edit::{self, Handle, Pointer, Tool};
use crate::geometry::{self, Affine};

/// Pan, zoom and input state of the canvas.
#[derive(Debug)]
pub struct Camera {
    /// Scene point at the canvas' top-left corner; `None` until first moved.
    origin: Option<[f64; 2]>,
    zoom: f64,
    pan_from: Option<Point>,
    pressed: bool,
    space: bool,
    modifiers: keyboard::Modifiers,
}

/// Excalidraw's zoom limits.
const ZOOM: std::ops::RangeInclusive<f64> = 0.1..=30.0;

/// Pixels per scrolled line.
const LINE_HEIGHT: f64 = 50.0;

impl Camera {
    fn origin(&self, sketch: &Sketch) -> [f64; 2] {
        self.origin.unwrap_or(sketch.content_origin)
    }

    fn view(&self, sketch: &Sketch) -> Affine {
        let [x, y] = self.origin(sketch);
        Affine::translate([-x, -y]).then(Affine::scale(self.zoom))
    }

    /// Scene point under a window position.
    fn scene_point(&self, sketch: &Sketch, bounds: Rectangle, position: Point) -> geometry::Point {
        let [x, y] = self.origin(sketch);
        [
            x + f64::from(position.x - bounds.x) / self.zoom,
            y + f64::from(position.y - bounds.y) / self.zoom,
        ]
    }

    fn modifiers(&self) -> edit::Modifiers {
        edit::Modifiers {
            shift: self.modifiers.shift(),
            alt: self.modifiers.alt(),
            command: self.modifiers.command(),
        }
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            origin: None,
            zoom: 1.0,
            pan_from: None,
            pressed: false,
            space: false,
            modifiers: keyboard::Modifiers::default(),
        }
    }
}

impl canvas::Program<Message> for Sketch {
    type State = Camera;

    fn update(
        &self,
        camera: &mut Camera,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let publish = |input| Some(canvas::Action::publish(Message(input)).and_capture());
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                camera.modifiers = *modifiers;
                None
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Space),
                ..
            }) => {
                camera.space = true;
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: Key::Named(Named::Space),
                ..
            }) => {
                camera.space = false;
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                let clipboard = match key.as_ref() {
                    Key::Character(c) if modifiers.command() => match c.to_lowercase().as_str() {
                        "c" => Some(Input::Copy),
                        "x" => Some(Input::Cut),
                        "v" => {
                            // Paste at the pointer, or the canvas centre.
                            let at = cursor.position_over(bounds).unwrap_or(bounds.center());
                            Some(Input::Paste(camera.scene_point(self, bounds, at)))
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
                let position = cursor.position_over(bounds)?;
                let pans = *button == mouse::Button::Middle
                    || (*button == mouse::Button::Left
                        && (camera.space || self.editor.tool() == Tool::Hand));
                if pans {
                    camera.pan_from = Some(position);
                    return Some(canvas::Action::capture());
                }
                if *button != mouse::Button::Left {
                    return None;
                }
                camera.pressed = true;
                let at = camera.scene_point(self, bounds, position);
                publish(Input::Pointer(Pointer::Down, at, camera.modifiers()))
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(from) = camera.pan_from {
                    camera.pan_from = Some(*position);
                    let origin = camera.origin(self);
                    let delta = *position - from;
                    camera.origin = Some([
                        origin[0] - f64::from(delta.x) / camera.zoom,
                        origin[1] - f64::from(delta.y) / camera.zoom,
                    ]);
                    self.clear_caches();
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                let pointer = if camera.pressed {
                    Pointer::Move
                } else if self.editor.wants_hover() {
                    Pointer::Hover
                } else {
                    return None;
                };
                let at = camera.scene_point(self, bounds, *position);
                publish(Input::Pointer(pointer, at, camera.modifiers()))
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                if camera.pan_from.take().is_some() {
                    return Some(canvas::Action::capture());
                }
                if !std::mem::take(&mut camera.pressed) {
                    return None;
                }
                let position = cursor.position().unwrap_or_default();
                let at = camera.scene_point(self, bounds, position);
                publish(Input::Pointer(Pointer::Up, at, camera.modifiers()))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let at = cursor.position_in(bounds)?;
                let (dx, dy) = match *delta {
                    ScrollDelta::Lines { x, y } => {
                        (f64::from(x) * LINE_HEIGHT, f64::from(y) * LINE_HEIGHT)
                    }
                    ScrollDelta::Pixels { x, y } => (f64::from(x), f64::from(y)),
                };
                let origin = camera.origin(self);
                self.clear_caches();
                if camera.modifiers.command() {
                    let zoom = (camera.zoom * (dy / 500.0).exp()).clamp(*ZOOM.start(), *ZOOM.end());
                    camera.origin = Some(zoom_at(
                        origin,
                        camera.zoom,
                        zoom,
                        [f64::from(at.x), f64::from(at.y)],
                    ));
                    camera.zoom = zoom;
                    return publish(Input::Zoom(zoom));
                }
                let (dx, dy) = if camera.modifiers.shift() {
                    (dy, dx)
                } else {
                    (dx, dy)
                };
                camera.origin = Some([origin[0] - dx / camera.zoom, origin[1] - dy / camera.zoom]);
                Some(canvas::Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        camera: &Camera,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let view = camera.view(self);
        let size = bounds.size();
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
            frame.fill_rectangle(Point::ORIGIN, frame.size(), self.paint(self.background));
            self.draw_ids(frame, below, view);
        });
        let mut dynamic = Frame::new(renderer, size);
        self.draw_ids(&mut dynamic, middle, view);
        let above = self
            .above
            .draw(renderer, size, |frame| self.draw_ids(frame, above, view));
        let mut overlay = Frame::new(renderer, size);
        self.draw_overlay(&mut overlay, camera.zoom, view);
        vec![
            below,
            dynamic.into_geometry(),
            above,
            overlay.into_geometry(),
        ]
    }

    fn mouse_interaction(
        &self,
        camera: &Camera,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(position) = cursor.position_over(bounds) else {
            return mouse::Interaction::default();
        };
        if camera.pan_from.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if camera.space || self.editor.tool() == Tool::Hand {
            return mouse::Interaction::Grab;
        }
        if !matches!(self.editor.tool(), Tool::Selection) {
            return mouse::Interaction::Crosshair;
        }
        let at = camera.scene_point(self, bounds, position);
        if let Some(handle) = self.editor.handle_at(at) {
            return match handle {
                Handle::N | Handle::S => mouse::Interaction::ResizingVertically,
                Handle::W | Handle::E => mouse::Interaction::ResizingHorizontally,
                Handle::Nw | Handle::Se => mouse::Interaction::ResizingDiagonallyDown,
                Handle::Ne | Handle::Sw => mouse::Interaction::ResizingDiagonallyUp,
                Handle::Rotation => mouse::Interaction::Grab,
            };
        }
        if self
            .editor
            .grabs(camera.scene_point(self, bounds, position))
        {
            mouse::Interaction::Move
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Returns the origin that keeps the scene point under `cursor` (canvas
/// pixels) fixed while zooming from `from` to `to`.
fn zoom_at(origin: [f64; 2], from: f64, to: f64, cursor: [f64; 2]) -> [f64; 2] {
    let anchor = [origin[0] + cursor[0] / from, origin[1] + cursor[1] / from];
    [anchor[0] - cursor[0] / to, anchor[1] - cursor[1] / to]
}

#[cfg(test)]
mod tests {
    use super::zoom_at;

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let (origin, cursor) = ([-10.0, 40.0], [300.0, 200.0]);
        let under = |origin: [f64; 2], zoom: f64| {
            [origin[0] + cursor[0] / zoom, origin[1] + cursor[1] / zoom]
        };
        let zoomed = zoom_at(origin, 1.0, 2.5, cursor);
        assert_eq!(under(zoomed, 2.5), under(origin, 1.0));
        assert_eq!(zoom_at(origin, 2.0, 2.0, cursor), origin);
    }
}
