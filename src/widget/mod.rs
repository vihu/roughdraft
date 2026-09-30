//! iced component that edits a scene: canvas, pan and zoom, selection overlay.
//!
//! Embed it the usual iced way: keep a [`Sketch`] in your state, show
//! [`Sketch::view`] mapped to your message type, and pass its messages back
//! to [`Sketch::update`].
mod camera;
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

use camera::{Camera, ZoomKey};

use crate::color::Rgba;
use crate::edit::{self, Command, Editor, Order, Pointer, Tool};
use crate::geometry::{self, Affine, Bounds};
use crate::render::{self, Drawing, Item, Segment};
use crate::scene::Scene;

pub use crate::fonts::EXCALIFONT;

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
    drawings: HashMap<String, Rendered>,
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
    /// Decoded images by file id.
    images: HashMap<String, iced::widget::image::Handle>,
    /// Canvas size at the last draw, for placing inserted images.
    viewport: std::cell::Cell<iced::Size>,
    /// A colour being typed in the style panel, until the next click.
    color_draft: Option<(ui::ColorField, String)>,
}

/// An element as last rendered.
#[derive(Debug)]
struct Rendered {
    revision: (i64, i64),
    /// `None` for types not drawn yet.
    drawing: Option<Drawing>,
    /// Box to cull against: the element's bounds plus [`CULL_MARGIN`].
    extent: Bounds,
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
    ZoomKey(ZoomKey),
    /// A change from the style panel.
    Style(edit::StyleChange),
    /// Text typed into a colour field of the style panel.
    ColorText(ui::ColorField, String),
    /// An edit in the text overlay.
    Text(text_editor::Action),
    FinishText,
    Copy,
    Cut,
    /// Paste centred on a scene point.
    Paste(geometry::Point),
    Pasted(geometry::Point, Option<String>),
    /// A clipboard image: width, height, RGBA pixels.
    PastedImage(geometry::Point, u32, u32, std::sync::Arc<Vec<u8>>),
}

/// Gap between the content and the canvas edge on open, like Excalidraw's
/// SVG export padding.
const PADDING: f64 = 10.0;

/// Scene units added around an element's box before culling it: covers the
/// rough wobble, stroke width and arrowheads, which reach past the box.
const CULL_MARGIN: f64 = 50.0;

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
            images: HashMap::new(),
            viewport: std::cell::Cell::new(iced::Size::new(800.0, 600.0)),
            color_draft: None,
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
        if matches!(
            message.0,
            Input::Pointer(Pointer::Down, ..) | Input::Command(_) | Input::Style(_)
        ) {
            self.color_draft = None;
        }
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
                self.camera.zoom_about(self.camera.zoom * factor, cursor);
                self.editor.set_zoom(self.camera.zoom);
                self.clear_caches();
                return Task::none();
            }
            Input::ZoomKey(key) => {
                let size = self.viewport.get();
                let viewport = [f64::from(size.width), f64::from(size.height)];
                let selection = key != ZoomKey::FitAll && self.editor.selection().next().is_some();
                let live = self
                    .editor
                    .scene()
                    .elements
                    .iter()
                    .filter(|e| !e.base.is_deleted);
                let targets: Vec<&crate::scene::Element> = if selection {
                    self.editor.selection().collect()
                } else {
                    live.collect()
                };
                let bounds =
                    (!targets.is_empty()).then(|| edit::common_bounds(targets.into_iter()));
                self.camera.apply(key, viewport, bounds);
                self.editor.set_zoom(self.camera.zoom);
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
            Input::ColorText(field, text) => {
                // Applied as soon as it is a whole hex colour.
                let css = format!("#{}", text.trim().trim_start_matches('#'));
                if Rgba::parse(&css).is_some() {
                    self.editor.apply_style(field.change(css));
                }
                self.color_draft = Some((field, text));
                Task::none()
            }
            Input::FinishText => {
                self.editor.finish_text();
                Task::none()
            }
            Input::Copy => write_clipboard(self.editor.copy()),
            Input::Cut => write_clipboard(self.editor.cut()),
            Input::Paste(at) => {
                // An image on the clipboard wins over text, like Excalidraw.
                return iced::clipboard::read_image().then(move |image| match image {
                    Ok(image) => Task::done(Message(Input::PastedImage(
                        at,
                        image.size.width,
                        image.size.height,
                        std::sync::Arc::new(image.rgba.to_vec()),
                    ))),
                    Err(_) => iced::clipboard::read_text().map(move |text| {
                        Message(Input::Pasted(at, text.ok().map(|t| (*t).clone())))
                    }),
                });
            }
            Input::PastedImage(at, width, height, rgba) => {
                if let Some(png) = encode_png(width, height, rgba.to_vec()) {
                    let url = format!("data:image/png;base64,{}", crate::base64::encode(&png));
                    self.editor.insert_image(
                        url,
                        "image/png",
                        [f64::from(width), f64::from(height)],
                        at,
                    );
                }
                Task::none()
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

    /// Supplies the picture for an image file the scene does not embed (for
    /// hosts that store images elsewhere, such as Keeprs). `bytes` is an
    /// encoded image (PNG, JPEG, GIF, WebP); undecodable bytes keep the
    /// placeholder.
    pub fn set_image(&mut self, file_id: &str, bytes: &[u8]) {
        if let Some(handle) = decode_image(bytes) {
            self.images.insert(file_id.to_owned(), handle);
            self.clear_caches();
        }
    }

    /// Inserts an encoded image (PNG, JPEG, GIF, WebP) at the middle of the
    /// view, embedded in the scene as a data URL, like Excalidraw's image
    /// tool.
    ///
    /// # Errors
    ///
    /// Returns a message when the bytes are not an image this crate reads.
    pub fn insert_image(&mut self, bytes: &[u8]) -> Result<(), String> {
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let format = reader.format().ok_or("not a recognised image format")?;
        let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
        let mime = format.to_mime_type();
        let url = format!("data:{mime};base64,{}", crate::base64::encode(bytes));
        let viewport = self.viewport.get();
        let at = self.camera.scene_point(
            iced::Rectangle::with_size(viewport),
            iced::Point::new(viewport.width / 2.0, viewport.height / 2.0),
        );
        self.editor
            .insert_image(url, mime, [f64::from(width), f64::from(height)], at);
        self.refresh();
        Ok(())
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
            let rendered = match previous.remove(id) {
                Some(cached) if cached.revision == revision => cached,
                _ => {
                    static_changed |= !active.contains(id);
                    let [x1, y1, x2, y2] = geometry::element_bounds(element);
                    Rendered {
                        revision,
                        drawing: render::render_element(element, background),
                        extent: [
                            x1 - CULL_MARGIN,
                            y1 - CULL_MARGIN,
                            x2 + CULL_MARGIN,
                            y2 + CULL_MARGIN,
                        ],
                    }
                }
            };
            if rendered.drawing.is_some() {
                order.push(id.clone());
            }
            self.drawings.insert(id.clone(), rendered);
        }
        for element in scene.elements.iter().filter(|e| !e.base.is_deleted) {
            let Some(id) = element
                .file_id()
                .filter(|id| !self.images.contains_key(*id))
            else {
                continue;
            };
            let handle = scene
                .file_data_url(id)
                .and_then(crate::base64::decode_data_url)
                .and_then(|(_, bytes)| decode_image(&bytes));
            if let Some(handle) = handle {
                self.images.insert(id.to_owned(), handle);
                static_changed = true;
            }
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
            .filter_map(|id| self.drawings[id].drawing.as_ref())
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
        // Only what is in view, like Excalidraw's `getVisibleCanvasElements`.
        let [ox, oy] = self.camera.origin;
        let size = frame.size();
        let (x2, y2) = (
            ox + f64::from(size.width) / self.camera.zoom,
            oy + f64::from(size.height) / self.camera.zoom,
        );
        let visible = |[a, b, c, d]: Bounds| a <= x2 && b <= y2 && c >= ox && d >= oy;
        // The element being typed is shown by the text overlay instead.
        let shown = ids.iter().filter(|id| self.editing.as_ref() != Some(*id));
        let drawings = shown
            .map(|id| &self.drawings[id])
            .filter(|rendered| visible(rendered.extent))
            .filter_map(|rendered| rendered.drawing.as_ref());
        for drawing in drawings {
            let transform = drawing.transform.then(view);
            for item in &drawing.items {
                match item {
                    Item::Image {
                        file_id,
                        size,
                        opacity,
                    } => {
                        self.draw_image(frame, file_id, *size, *opacity, transform);
                    }
                    _ => paint::draw_item(frame, item, transform, &|c| self.paint(c)),
                }
            }
        }
    }

    /// Draws a decoded image, or a grey placeholder while it is missing.
    /// Images keep their colors in dark mode, like Excalidraw's.
    fn draw_image(
        &self,
        frame: &mut Frame,
        file_id: &str,
        size: [f64; 2],
        opacity: f32,
        transform: Affine,
    ) {
        let Some(handle) = self.images.get(file_id) else {
            let [w, h] = size;
            let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|p| {
                let [x, y] = transform.apply(p);
                iced::Point::new(x as f32, y as f32)
            });
            let outline = canvas::Path::new(|path| {
                path.move_to(corners[0]);
                corners[1..].iter().for_each(|c| path.line_to(*c));
                path.close();
            });
            let grey = Color {
                a: 0.15 * opacity,
                ..Color::BLACK
            };
            frame.fill(&outline, grey);
            return;
        };
        // iced rotates the image about its centre, so place the unrotated box
        // around the transformed centre.
        let scale = transform.scale_factor();
        let [cx, cy] = transform.apply([size[0] / 2.0, size[1] / 2.0]);
        let (w, h) = (size[0] * scale, size[1] * scale);
        let bounds = iced::Rectangle {
            x: (cx - w / 2.0) as f32,
            y: (cy - h / 2.0) as f32,
            width: w as f32,
            height: h as f32,
        };
        let image = canvas::Image::new(handle.clone())
            .rotation(iced::Radians(transform.rotation() as f32))
            .opacity(opacity);
        frame.draw_image(bounds, image);
    }
}

/// Decodes an image to RGBA up front: iced draws RGBA handles in the frame
/// they appear, while encoded ones load on a worker and pop in later.
// ponytail: decodes on the UI thread when a scene loads; move to a Task if
// large photos stall opening a scene.
fn decode_image(bytes: &[u8]) -> Option<iced::widget::image::Handle> {
    let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(iced::widget::image::Handle::from_rgba(
        width,
        height,
        rgba.into_raw(),
    ))
}

/// Encodes RGBA pixels as PNG.
fn encode_png(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
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
            // `,` and `.` for layouts where Shift does not make `<` and `>`.
            ("<" | ",", true) if shift => Some(Command::SmallerFont),
            (">" | ".", true) if shift => Some(Command::LargerFont),
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
                Item::Image { size, .. } => vec![[0.0, 0.0], *size],
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
    use iced::keyboard::{Key, Modifiers, key::Named};

    use super::{Command, Tool, shortcut};

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
        assert_eq!(
            shortcut(&key("<"), ctrl | Modifiers::SHIFT),
            Some(Command::SmallerFont)
        );
        assert_eq!(
            shortcut(&key("."), ctrl | Modifiers::SHIFT),
            Some(Command::LargerFont)
        );
    }
}
