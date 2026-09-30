//! iced component that edits a scene: canvas, pan and zoom, selection overlay.
//!
//! Embed it the usual iced way: keep a [`Sketch`] in your state, show
//! [`Sketch::view`] mapped to your message type, and pass its messages back
//! to [`Sketch::update`].
mod overlay;
mod paint;
mod program;
mod text;
mod ui;

use std::collections::HashMap;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::canvas::{self, Canvas, Frame};
use iced::widget::{stack, text_editor};
use iced::{Color, Element, Length, Task};

use crate::color::Rgba;
use crate::edit::{self, Command, Editor, Order, Pointer, Tool};
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
    camera: Camera,
    /// What the text overlay edits, and the id of the element it belongs to.
    content: text_editor::Content,
    editing: Option<String>,
}

/// Pan and zoom: the scene point at the canvas' top-left, and the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Camera {
    origin: [f64; 2],
    zoom: f64,
}

/// Input for a [`Sketch`], produced by its view.
#[derive(Clone, Debug)]
pub struct Message(Input);

#[derive(Clone, Debug)]
enum Input {
    Pointer(Pointer, geometry::Point, edit::Modifiers),
    DoubleClick(geometry::Point),
    Command(Command),
    /// Pan by screen pixels.
    Pan([f64; 2]),
    /// Zoom by a factor, keeping the scene point under `cursor` (canvas
    /// pixels) in place.
    Zoom {
        factor: f64,
        cursor: [f64; 2],
    },
    /// A change from the style panel.
    Style(edit::StyleChange),
    /// An edit in the text overlay.
    Text(text_editor::Action),
    FinishText,
    Copy,
    Cut,
    /// Paste centred on a scene point.
    Paste(geometry::Point),
    Pasted(geometry::Point, Option<String>),
}

/// Excalidraw's zoom limits.
const ZOOM: std::ops::RangeInclusive<f64> = 0.1..=30.0;

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
        text::load_fonts();
        let mut editor = Editor::new(scene);
        editor.set_measure(Box::new(text::CosmicMeasure));
        let mut sketch = Self {
            editor,
            drawings: HashMap::new(),
            order: Vec::new(),
            active: Vec::new(),
            below: canvas::Cache::new(),
            above: canvas::Cache::new(),
            background,
            appearance: Appearance::Light,
            camera: Camera {
                origin: [0.0, 0.0],
                zoom: 1.0,
            },
            content: text_editor::Content::new(),
            editing: None,
        };
        sketch.refresh();
        sketch.camera.origin = content_origin(sketch.drawings());
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
            Input::DoubleClick(at) => {
                self.editor.double_click(at);
                Task::none()
            }
            Input::Command(command) => {
                self.editor.command(command);
                Task::none()
            }
            Input::Pan([dx, dy]) => {
                let [x, y] = self.camera.origin;
                self.camera.origin = [x - dx / self.camera.zoom, y - dy / self.camera.zoom];
                self.clear_caches();
                return Task::none();
            }
            Input::Zoom { factor, cursor } => {
                let zoom = (self.camera.zoom * factor).clamp(*ZOOM.start(), *ZOOM.end());
                self.camera.origin = zoom_at(self.camera.origin, self.camera.zoom, zoom, cursor);
                self.camera.zoom = zoom;
                self.editor.set_zoom(zoom);
                self.clear_caches();
                return Task::none();
            }
            Input::Text(action) => {
                let edit = action.is_edit();
                self.content.perform(action);
                if edit {
                    self.editor.set_text(&self.content.text());
                }
                Task::none()
            }
            Input::Style(change) => {
                self.editor.apply_style(change);
                Task::none()
            }
            Input::FinishText => {
                self.editor.finish_text();
                Task::none()
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
        task.chain(self.sync_text_overlay())
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
        self.camera.origin = origin;
        self.clear_caches();
    }

    /// Returns the editor: canvas with the tool bar and style panel over it.
    pub fn view(&self) -> Element<'_, Message> {
        let mut layers = vec![self.canvas(), self.toolbar()];
        layers.extend(self.style_panel());
        iced::widget::Stack::with_children(layers).into()
    }

    /// Returns only the canvas (and the text overlay while typing), for
    /// hosts that bring their own chrome, and for snapshots.
    pub fn canvas(&self) -> Element<'_, Message> {
        let canvas = Canvas::new(self).width(Length::Fill).height(Length::Fill);
        match self.text_overlay() {
            Some(overlay) => stack![canvas, overlay].into(),
            None => canvas.into(),
        }
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
        // The element being typed is shown by the text overlay instead.
        let shown = ids.iter().filter(|id| self.editing.as_ref() != Some(*id));
        for drawing in shown.filter_map(|id| self.drawings[id].1.as_ref()) {
            let transform = drawing.transform.then(view);
            for item in &drawing.items {
                paint::draw_item(frame, item, transform, &|c| self.paint(c));
            }
        }
    }
}

impl Camera {
    fn view(&self) -> Affine {
        let [x, y] = self.origin;
        Affine::translate([-x, -y]).then(Affine::scale(self.zoom))
    }

    /// Scene point under a window position.
    fn scene_point(&self, bounds: iced::Rectangle, position: iced::Point) -> geometry::Point {
        let [x, y] = self.origin;
        [
            x + f64::from(position.x - bounds.x) / self.zoom,
            y + f64::from(position.y - bounds.y) / self.zoom,
        ]
    }
}

/// Returns the origin that keeps the scene point under `cursor` (canvas
/// pixels) fixed while zooming from `from` to `to`.
fn zoom_at(origin: [f64; 2], from: f64, to: f64, cursor: [f64; 2]) -> [f64; 2] {
    let anchor = [origin[0] + cursor[0] / from, origin[1] + cursor[1] / from];
    [anchor[0] - cursor[0] / to, anchor[1] - cursor[1] / to]
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
            ("g", true) if shift => Some(Command::Ungroup),
            ("g", true) => Some(Command::Group),
            // Shift turns the brackets into braces on most layouts.
            ("[" | "{", true) if shift => Some(Command::Reorder(Order::ToBack)),
            ("[", true) => Some(Command::Reorder(Order::Backward)),
            ("]" | "}", true) if shift => Some(Command::Reorder(Order::ToFront)),
            ("]", true) => Some(Command::Reorder(Order::Forward)),
            (_, true) => None,
            _ if alt => None,
            ("v" | "1", _) => Some(Command::Tool(Tool::Selection)),
            ("h", _) => Some(Command::Tool(Tool::Hand)),
            ("r" | "2", _) => Some(Command::Tool(Tool::Rectangle)),
            ("d" | "3", _) => Some(Command::Tool(Tool::Diamond)),
            ("o" | "4", _) => Some(Command::Tool(Tool::Ellipse)),
            ("a" | "5", _) => Some(Command::Tool(Tool::Arrow)),
            ("l" | "6", _) => Some(Command::Tool(Tool::Line)),
            ("t" | "8", _) => Some(Command::Tool(Tool::Text)),
            ("q", _) => Some(Command::ToggleLock),
            _ => None,
        },
        _ => None,
    }
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
    use super::{Command, Tool, shortcut, zoom_at};

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
