//! iced component that edits a scene: canvas, pan and zoom, selection overlay.
//!
//! Embed it the usual iced way: keep a [`Sketch`] in your state, show
//! [`Sketch::view`] mapped to your message type, and pass its messages back
//! to [`Sketch::update`].
mod camera;
mod grid;
mod icons;
mod keys;
mod layers;
mod menu;
mod overlay;
mod paint;
mod picker;
mod picture;
mod png;
mod program;
mod text;
mod ui;

use std::collections::HashMap;

use iced::widget::canvas::{self, Canvas};
use iced::widget::{stack, text_editor};
use iced::{Color, Element, Length, Task};

use camera::{Camera, ZoomKey};
use layers::content_origin;
use picture::{Picture, encode_png};

use crate::color::Rgba;
use crate::edit::{self, Command, Editor, Pointer};
use crate::geometry::{self, Bounds};
use crate::render::Drawing;
use crate::scene::{Grid, Scene};

pub use self::menu::Request;
pub use self::png::{Png, PngError, render_png};
pub use self::text::font_measure;
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
    /// The grid drawn on the `below` layer, if on.
    grid: Option<Grid>,
    below: canvas::Cache,
    above: canvas::Cache,
    background: Rgba,
    appearance: Appearance,
    camera: std::cell::Cell<Camera>,
    /// Centre the content at the first draw, once the view size is known
    /// (Excalidraw's `scrollToContent`), unless the host placed the camera.
    unplaced: std::cell::Cell<bool>,
    /// What the text overlay edits, and the id of the element it belongs to.
    content: text_editor::Content,
    editing: Option<String>,
    /// Decoded pictures: each file whole, and cut or mirrored as elements
    /// show it.
    images: HashMap<Picture, iced::widget::image::Handle>,
    /// Files that did not decode (an SVG, a broken data URL), so they are
    /// not decoded again on every update; `set_image` can still supply one.
    undecodable: std::collections::HashSet<String>,
    /// Canvas size at the last draw, for placing inserted images.
    viewport: std::cell::Cell<iced::Size>,
    /// A colour being typed in the style panel, until the next click.
    color_draft: Option<(ui::ColorField, String)>,
    /// The color picker, while open.
    picker: Option<picker::Picker>,
    /// The picker's hue ring, which never changes.
    ring: canvas::Cache,
    /// Host actions the main menu offers, and whether it is open.
    menu: Vec<Request>,
    menu_open: bool,
}

/// An element as last rendered.
#[derive(Debug)]
struct Rendered {
    revision: (i64, i64),
    /// `None` for types not drawn yet, and while `stale`.
    drawing: Option<Drawing>,
    /// Changed by the current gesture: rendered when the canvas draws
    /// (`draw_ids`), once per frame instead of on every pointer event.
    stale: bool,
    /// A frame's title, drawn before its outline.
    label: Option<Drawing>,
    /// The frame this element is clipped to, if any (`frameId`).
    frame: Option<String>,
    /// Box to cull against: the element's bounds plus a margin for the
    /// wobble and arrowheads (`layers::CULL_MARGIN`).
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
    /// A change from the style panel that is still being dragged (the
    /// opacity slider); [`Input::StyleCommit`] ends it.
    StylePreview(edit::StyleChange),
    /// The end of a dragged style change: one undo step.
    StyleCommit,
    /// Input from the color picker.
    Pick(picker::Pick),
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
    /// The main menu button.
    ToggleMenu,
    /// A host action from the main menu; see [`Message::request`].
    Request(Request),
    /// The main menu's dark/light switch.
    ToggleAppearance,
}

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
            grid: None,
            below: canvas::Cache::new(),
            above: canvas::Cache::new(),
            background,
            appearance: Appearance::Light,
            camera: std::cell::Cell::new(Camera {
                origin: [0.0, 0.0],
                zoom: 1.0,
            }),
            unplaced: std::cell::Cell::new(true),
            content: text_editor::Content::new(),
            editing: None,
            images: HashMap::new(),
            undecodable: std::collections::HashSet::new(),
            viewport: std::cell::Cell::new(iced::Size::new(800.0, 600.0)),
            color_draft: None,
            picker: None,
            ring: canvas::Cache::new(),
            menu: Vec::new(),
            menu_open: false,
        };
        sketch.refresh();
        let origin = content_origin(sketch.drawings());
        sketch.camera.set(Camera { origin, zoom: 1.0 });
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
            Input::Pointer(Pointer::Down, ..)
                | Input::Command(_)
                | Input::Style(_)
                | Input::StylePreview(_)
                | Input::Pick(_)
        ) {
            self.color_draft = None;
        }
        // A press on the canvas, a command or the menu closes the picker.
        if matches!(
            message.0,
            Input::Pointer(Pointer::Down, ..)
                | Input::DoubleClick(_)
                | Input::Command(_)
                | Input::ToggleMenu
        ) {
            self.picker = None;
        }
        // Anything but the menu button closes the menu.
        if !matches!(message.0, Input::ToggleMenu) {
            self.menu_open = false;
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
                let mut camera = self.camera.get();
                let [x, y] = camera.origin;
                camera.origin = [x - dx / camera.zoom, y - dy / camera.zoom];
                self.camera.set(camera);
                self.clear_caches();
                return Task::none();
            }
            Input::Zoom { factor, cursor } => {
                let mut camera = self.camera.get();
                camera.zoom_about(camera.zoom * factor, cursor);
                self.camera.set(camera);
                self.editor.set_zoom(camera.zoom);
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
                let mut camera = self.camera.get();
                camera.apply(key, viewport, bounds);
                self.camera.set(camera);
                self.editor.set_zoom(camera.zoom);
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
                self.follow_picker();
                Task::none()
            }
            Input::StylePreview(change) => {
                self.editor.preview_style(change);
                Task::none()
            }
            Input::StyleCommit => {
                self.editor.commit_style();
                Task::none()
            }
            Input::Pick(pick) => {
                self.pick(pick);
                Task::none()
            }
            Input::ColorText(field, text) => {
                // Applied as soon as it is a whole colour: hex with or without
                // `#`, else any CSS colour as typed (Excalidraw's `getColor`).
                let typed = text.trim();
                let hex = format!("#{}", typed.trim_start_matches('#'));
                let css = [hex, typed.to_owned()]
                    .into_iter()
                    .find(|css| Rgba::parse(css).is_some());
                if let Some(css) = css {
                    self.editor.apply_style(field.change(css));
                    self.follow_picker();
                }
                self.color_draft = Some((field, text));
                Task::none()
            }
            Input::ToggleMenu => {
                self.menu_open = !self.menu_open;
                Task::none()
            }
            // The host carries it out (`Message::request`).
            Input::Request(_) => Task::none(),
            Input::ToggleAppearance => {
                self.set_appearance(match self.appearance {
                    Appearance::Light => Appearance::Dark,
                    Appearance::Dark => Appearance::Light,
                });
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
                    let size = self.fit_image([f64::from(width), f64::from(height)]);
                    self.editor.insert_image(url, "image/png", size, at);
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

    /// Returns the canvas zoom (1 is 100%).
    pub fn zoom(&self) -> f64 {
        self.camera.get().zoom
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

    /// Sets the scene point shown at the canvas' top-left corner.
    ///
    /// By default the first draw centres the content in the view.
    pub fn set_origin(&mut self, origin: [f64; 2]) {
        let zoom = self.camera.get().zoom;
        self.camera.set(Camera { origin, zoom });
        self.unplaced.set(false);
        self.clear_caches();
    }

    /// Returns the editor: canvas with the tool bar and style panel over it.
    pub fn view(&self) -> Element<'_, Message> {
        let mut layers = vec![self.canvas(), self.toolbar(), self.footer()];
        layers.extend(self.style_panel());
        // Last, so the open menu lies over the style panel.
        layers.push(self.menu());
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
}

fn write_clipboard(json: Option<String>) -> Task<Message> {
    json.map_or_else(Task::none, |json| iced::clipboard::write(json).discard())
}
