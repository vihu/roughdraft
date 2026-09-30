//! iced component that edits a scene: canvas, pan and zoom, selection overlay.
//!
//! Embed it the usual iced way: keep a [`Sketch`] in your state, show
//! [`Sketch::view`] mapped to your message type, and pass its messages back
//! to [`Sketch::update`].
mod paint;
mod program;

use std::collections::HashMap;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::canvas::{self, Canvas, Frame, LineDash, Path, Stroke, Style};
use iced::{Color, Element, Length, Task};

use crate::color::Rgba;
use crate::edit::{self, Command, Editor, Pointer, Tool};
use crate::geometry::{self, Affine};
use crate::render::{self, Drawing, Item, Segment};
use crate::scene::Scene;

/// Excalifont, Excalidraw's default hand-drawn font (OFL-1.1).
///
/// Register it with `iced::application(..).fonts([EXCALIFONT])` so text renders
/// in it.
pub const EXCALIFONT: &[u8] =
    include_bytes!("../../assets/fonts/Excalifont/Excalifont-Regular.ttf");

/// Canvas color scheme, like Excalidraw's theme toggle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// Colors as stored.
    #[default]
    Light,
    /// Excalidraw's dark filter applied to every color.
    Dark,
}

/// A scene with its editor and the render caches for it.
#[derive(Debug)]
pub struct Sketch {
    editor: Editor,
    /// Per element id: the revision it was rendered at, and its drawing
    /// (`None` for types not drawn yet).
    drawings: HashMap<String, ((i64, i64), Option<Drawing>)>,
    /// Drawn element ids in draw order.
    order: Vec<String>,
    active: Vec<String>,
    below: canvas::Cache,
    above: canvas::Cache,
    background: Rgba,
    appearance: Appearance,
    content_origin: [f64; 2],
}

/// Input for a [`Sketch`], produced by its view.
#[derive(Clone, Debug)]
pub struct Message(Input);

#[derive(Clone, Debug)]
enum Input {
    Pointer(Pointer, geometry::Point, edit::Modifiers),
    Command(Command),
    Zoom(f64),
    Copy,
    Cut,
    /// Paste centred on a scene point.
    Paste(geometry::Point),
    Pasted(geometry::Point, Option<String>),
}

/// Gap between the content and the canvas edge on open, like Excalidraw's
/// SVG export padding.
const PADDING: f64 = 10.0;

/// Screen pixels between an element and its selection border.
const SELECTION_PADDING: f64 = 4.0;

// Public API
impl Sketch {
    /// Starts editing `scene`.
    pub fn new(scene: Scene) -> Self {
        let background = Rgba::parse(scene.background_color()).unwrap_or(Rgba::WHITE);
        let mut sketch = Self {
            editor: Editor::new(scene),
            drawings: HashMap::new(),
            order: Vec::new(),
            active: Vec::new(),
            below: canvas::Cache::new(),
            above: canvas::Cache::new(),
            background,
            appearance: Appearance::Light,
            content_origin: [0.0, 0.0],
        };
        sketch.refresh();
        sketch.content_origin = content_origin(sketch.drawings());
        sketch
    }

    /// Returns the scene being edited.
    pub fn scene(&self) -> &Scene {
        self.editor.scene()
    }

    /// Returns the editor: tool, selection and the rest of its state.
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Applies a message from [`Sketch::view`].
    ///
    /// Clipboard shortcuts return a task that talks to the system clipboard;
    /// run it (`Task::map` it into your message type).
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message.0 {
            Input::Pointer(pointer, at, modifiers) => {
                self.editor.pointer(pointer, at, modifiers);
                Task::none()
            }
            Input::Command(command) => {
                self.editor.command(command);
                Task::none()
            }
            Input::Zoom(zoom) => {
                self.editor.set_zoom(zoom);
                return Task::none();
            }
            Input::Copy => write_clipboard(self.editor.copy()),
            Input::Cut => write_clipboard(self.editor.cut()),
            Input::Paste(at) => {
                return iced::clipboard::read_text()
                    .map(move |text| Message(Input::Pasted(at, text.ok().map(|t| (*t).clone()))));
            }
            Input::Pasted(at, text) => {
                if let Some(text) = text {
                    self.editor.paste(&text, at);
                }
                Task::none()
            }
        };
        self.refresh();
        task
    }

    /// Returns the current color scheme.
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    /// Switches the color scheme.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
        self.clear_caches();
    }

    /// Sets the scene point shown at the canvas' top-left corner on open.
    ///
    /// Defaults to the content's top-left minus a small padding.
    pub fn set_origin(&mut self, origin: [f64; 2]) {
        self.content_origin = origin;
        self.clear_caches();
    }

    /// Returns the canvas widget, filling the available space.
    pub fn view(&self) -> Element<'_, Message> {
        Canvas::new(self)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

// Private API
impl Sketch {
    /// Re-renders elements whose revision changed and invalidates the
    /// static layers when anything outside the current gesture changed.
    fn refresh(&mut self) {
        let scene = self.editor.scene();
        let background = scene.background_color();
        let active: Vec<String> = self.editor.active().into_iter().map(String::from).collect();
        let mut previous = std::mem::take(&mut self.drawings);
        let mut order = Vec::new();
        let mut static_changed = active != self.active;

        for element in render::draw_order(scene) {
            let id = &element.base.id;
            let revision = element.revision();
            let drawing = match previous.remove(id) {
                Some((cached, drawing)) if cached == revision => drawing,
                _ => {
                    static_changed |= !active.contains(id);
                    render::render_element(element, background)
                }
            };
            if drawing.is_some() {
                order.push(id.clone());
            }
            self.drawings.insert(id.clone(), (revision, drawing));
        }
        static_changed |= !previous.is_empty() || order != self.order;
        self.order = order;
        self.active = active;
        if static_changed {
            self.clear_caches();
        }
    }

    fn drawings(&self) -> impl Iterator<Item = &Drawing> {
        self.order
            .iter()
            .filter_map(|id| self.drawings[id].1.as_ref())
    }

    fn clear_caches(&self) {
        self.below.clear();
        self.above.clear();
    }

    fn paint(&self, color: Rgba) -> Color {
        let Rgba { r, g, b, a } = match self.appearance {
            Appearance::Light => color,
            Appearance::Dark => color.to_dark(),
        };
        Color { r, g, b, a }
    }

    fn draw_ids(&self, frame: &mut Frame, ids: &[String], view: Affine) {
        for drawing in ids.iter().filter_map(|id| self.drawings[id].1.as_ref()) {
            let transform = drawing.transform.then(view);
            for item in &drawing.items {
                paint::draw_item(frame, item, transform, &|c| self.paint(c));
            }
        }
    }

    fn draw_overlay(&self, frame: &mut Frame, zoom: f64, view: Affine) {
        let (selection, dark_selection) = (
            Rgba::rgb(0.412, 0.396, 0.859),
            Rgba::rgb(0.208, 0.188, 0.769),
        );
        let color = self.paint(match self.appearance {
            Appearance::Light => selection,
            Appearance::Dark => dark_selection,
        });
        let line = |dash: &'static [f32]| Stroke {
            style: Style::Solid(color),
            width: 1.0,
            line_dash: LineDash {
                segments: dash,
                offset: 0,
            },
            ..Stroke::default()
        };
        let pad = SELECTION_PADDING / zoom;

        let selected: Vec<_> = self.editor.selection().collect();
        for element in &selected {
            let [x1, y1, x2, y2] = geometry::local_bounds(element);
            let corners = [
                [x1 - pad, y1 - pad],
                [x2 + pad, y1 - pad],
                [x2 + pad, y2 + pad],
                [x1 - pad, y2 + pad],
            ];
            let transform = geometry::element_transform(element).then(view);
            frame.stroke(&polygon(&corners, transform), line(&[]));
        }
        if selected.len() > 1 {
            let [x1, y1, x2, y2] = edit::common_bounds(selected.into_iter());
            let corners = [
                [x1 - pad, y1 - pad],
                [x2 + pad, y1 - pad],
                [x2 + pad, y2 + pad],
                [x1 - pad, y2 + pad],
            ];
            frame.stroke(&polygon(&corners, view), line(&[2.0, 2.0]));
        }
        if let Some([x1, y1, x2, y2]) = self.editor.marquee() {
            let marquee = polygon(&[[x1, y1], [x2, y1], [x2, y2], [x1, y2]], view);
            frame.fill(
                &marquee,
                self.paint(Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 200.0 / 255.0,
                    a: 0.04,
                }),
            );
            frame.stroke(&marquee, line(&[]));
        }
    }
}

fn write_clipboard(json: Option<String>) -> Task<Message> {
    json.map_or_else(Task::none, |json| iced::clipboard::write(json).discard())
}

/// Maps Excalidraw's shortcuts to editor commands.
fn shortcut(key: &Key, modifiers: keyboard::Modifiers) -> Option<Command> {
    /// Arrow-key nudge in scene units: plain, and with Shift.
    const NUDGE: (f64, f64) = (1.0, 5.0);

    let (command, shift, alt) = (modifiers.command(), modifiers.shift(), modifiers.alt());
    let step = if shift { NUDGE.1 } else { NUDGE.0 };
    match key.as_ref() {
        Key::Named(Named::Delete | Named::Backspace) if !command => Some(Command::Delete),
        Key::Named(Named::Escape) => Some(Command::Escape),
        Key::Named(Named::Enter) => Some(Command::Finish),
        Key::Named(Named::ArrowLeft) => Some(Command::Nudge([-step, 0.0])),
        Key::Named(Named::ArrowRight) => Some(Command::Nudge([step, 0.0])),
        Key::Named(Named::ArrowUp) => Some(Command::Nudge([0.0, -step])),
        Key::Named(Named::ArrowDown) => Some(Command::Nudge([0.0, step])),
        Key::Character(c) => match (c.to_lowercase().as_str(), command) {
            ("z", true) if shift => Some(Command::Redo),
            ("z", true) => Some(Command::Undo),
            ("y", true) => Some(Command::Redo),
            ("d", true) => Some(Command::Duplicate),
            ("a", true) => Some(Command::SelectAll),
            (_, true) => None,
            _ if alt => None,
            ("v" | "1", _) => Some(Command::Tool(Tool::Selection)),
            ("h", _) => Some(Command::Tool(Tool::Hand)),
            ("r" | "2", _) => Some(Command::Tool(Tool::Rectangle)),
            ("d" | "3", _) => Some(Command::Tool(Tool::Diamond)),
            ("o" | "4", _) => Some(Command::Tool(Tool::Ellipse)),
            ("a" | "5", _) => Some(Command::Tool(Tool::Arrow)),
            ("l" | "6", _) => Some(Command::Tool(Tool::Line)),
            ("q", _) => Some(Command::ToggleLock),
            _ => None,
        },
        _ => None,
    }
}

fn polygon(corners: &[geometry::Point], transform: Affine) -> Path {
    let segments: Vec<Segment> = corners
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                Segment::MoveTo(*p)
            } else {
                Segment::LineTo(*p)
            }
        })
        .chain(std::iter::once(Segment::LineTo(corners[0])))
        .collect();
    paint::to_path(&segments, transform)
}

/// Top-left of everything drawn, minus [`PADDING`], so a scene opens at 100%
/// zoom aligned like Excalidraw's SVG export.
// ponytail: uses the rough outline, off by the wobble (~2px) from Excalidraw's element bounds
fn content_origin<'a>(drawings: impl Iterator<Item = &'a Drawing>) -> [f64; 2] {
    let mut min = [f64::INFINITY, f64::INFINITY];
    for drawing in drawings {
        for item in &drawing.items {
            let points: Vec<[f64; 2]> = match item {
                Item::Stroke { path, .. } | Item::Fill { path, .. } => path
                    .iter()
                    .map(|segment| match *segment {
                        Segment::MoveTo(p) | Segment::LineTo(p) | Segment::CubicTo(_, _, p) => p,
                    })
                    .collect(),
                Item::Text(_) => vec![[0.0, 0.0]],
            };
            for p in points {
                let [x, y] = drawing.transform.apply(p);
                min = [min[0].min(x), min[1].min(y)];
            }
        }
    }
    if min[0].is_finite() {
        [min[0] - PADDING, min[1] - PADDING]
    } else {
        [0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Tool, shortcut};
    use iced::keyboard::{Key, Modifiers, key::Named};

    #[test]
    fn shortcuts_follow_excalidraw() {
        let ctrl = Modifiers::CTRL;
        let key = |c: &str| Key::Character(c.into());
        assert_eq!(shortcut(&key("z"), ctrl), Some(Command::Undo));
        assert_eq!(
            shortcut(&key("Z"), ctrl | Modifiers::SHIFT),
            Some(Command::Redo)
        );
        assert_eq!(shortcut(&key("d"), ctrl), Some(Command::Duplicate));
        assert_eq!(
            shortcut(&key("d"), Modifiers::empty()),
            Some(Command::Tool(Tool::Diamond))
        );
        assert_eq!(
            shortcut(&key("5"), Modifiers::empty()),
            Some(Command::Tool(Tool::Arrow))
        );
        assert_eq!(
            shortcut(&key("h"), Modifiers::empty()),
            Some(Command::Tool(Tool::Hand))
        );
        assert_eq!(
            shortcut(&key("d"), Modifiers::ALT | Modifiers::SHIFT),
            None,
            "theme toggle stays with the app"
        );
        assert_eq!(
            shortcut(&Key::Named(Named::ArrowLeft), Modifiers::SHIFT),
            Some(Command::Nudge([-5.0, 0.0]))
        );
    }
}
