//! Text editing overlay, like Excalidraw's textarea over the canvas, and text
//! measured with iced's text engine (cosmic-text) so sizes match what is
//! drawn.
use std::borrow::Cow;

use iced::advanced::graphics::text::{cosmic_text, font_system, to_attributes};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard::{Key, key::Named};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::text_editor::{self, Binding, KeyPress, Motion, Status};
use iced::widget::{operation, pin};
use iced::{
    Background, Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Task, Theme,
    Vector,
};

use super::{Input, Message, Sketch, paint};
use crate::color::Rgba;
use crate::edit::Measure;
use crate::hit::container_id;
use crate::scene::Kind;

/// Widget id of the text overlay, for focusing it.
const TEXT_EDITOR: &str = "roughdraft-text";

/// Gap between a container's edge and its label (`BOUND_TEXT_PADDING`).
const LABEL_PADDING: f64 = 5.0;

/// Loads the bundled fonts into iced's font system once, so text draws and
/// measures in Excalidraw's fonts without the host registering them.
pub(super) fn load_fonts() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let mut system = font_system().write().expect("font system lock");
        for font in crate::fonts::ALL {
            system.load_font(Cow::Borrowed(font));
        }
    });
}

/// Returns a [`Measure`] with the bundled fonts, the one the canvas draws
/// with: for building scenes outside a window (`roughdraft build`).
pub fn font_measure() -> Box<dyn Measure> {
    load_fonts();
    Box::new(CosmicMeasure)
}

/// [`Measure`] backed by iced's text engine, with shaping.
#[derive(Debug)]
pub(super) struct CosmicMeasure;

impl Measure for CosmicMeasure {
    fn line_width(&self, line: &str, font_family: u32, font_size: f64) -> f64 {
        let mut system = font_system().write().expect("font system lock");
        let raw = system.raw();
        let size = font_size as f32;
        let buffer = cosmic_text::Buffer::new(raw, cosmic_text::Metrics::new(size, size));
        // The attributes the renderer draws with: the family alone would
        // ask for weight 400 and miss Nunito, whose cut is 500.
        let attrs = to_attributes(paint::font(font_family));
        let mut buffer = buffer;
        let mut buffer = buffer.borrow_with(raw);
        buffer.set_size(None, None);
        buffer.set_text(line, &attrs, cosmic_text::Shaping::Advanced, None);
        buffer.shape_until_scroll(false);
        f64::from(
            buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0, f32::max),
        )
    }
}

impl Sketch {
    /// The text editor over the element being typed, if any.
    pub(super) fn text_overlay(&self) -> Option<Element<'_, Message>> {
        let element = self.editor.editing()?;
        let Kind::Text(text) = &element.kind else {
            return None;
        };
        let zoom = self.camera.get().zoom;
        let size = text.font_size * zoom;
        let container = container_id(element)
            .and_then(|id| self.scene().elements.iter().find(|e| e.base.id == id));
        // Labels wrap inside their container; free text grows to the right.
        let (x, width, wrapping) = match container {
            Some(container) if !matches!(container.kind, Kind::Arrow(_)) => (
                container.base.x + LABEL_PADDING,
                (container.base.width - 2.0 * LABEL_PADDING) * zoom,
                Wrapping::Word,
            ),
            _ => (
                element.base.x,
                element.base.width * zoom + size,
                Wrapping::None,
            ),
        };
        let width = width.max(size);
        // Every line and one more, for the line Enter adds; at least the
        // canvas, as `pin` gave a box on screen (labels wrap in cosmic-text
        // here, measured lines may differ).
        let height = (element.base.height * zoom + size * text.line_height)
            .max(f64::from(self.viewport.get().height));
        let [x, y] = self.camera.get().view().apply([x, element.base.y]);
        let color = self.paint(Rgba::parse(&element.base.stroke_color).unwrap_or(Rgba::BLACK));
        let selection = self.paint(Rgba {
            a: 0.3,
            ..Rgba::rgb(0.412, 0.396, 0.859)
        });
        let editor = iced::widget::text_editor(&self.content)
            .id(TEXT_EDITOR)
            .on_action(|action| Message(Input::Text(action)))
            .key_binding(|press: KeyPress| match press.key.as_ref() {
                Key::Named(Named::Escape) => Some(Binding::Custom(Message(Input::FinishText))),
                Key::Named(Named::Enter) if press.modifiers.command() => {
                    Some(Binding::Custom(Message(Input::FinishText)))
                }
                _ => Binding::from_key_press(press),
            })
            .font(paint::font(text.font_family))
            .size(size as f32)
            .line_height(LineHeight::Absolute(Pixels(
                (size * text.line_height) as f32,
            )))
            .padding(0)
            .wrapping(wrapping)
            .width(width as f32)
            .style(move |_theme: &Theme, _status: Status| text_editor::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                placeholder: color,
                value: color,
                selection,
            });
        let room = Room {
            content: editor.into(),
            size: Size::new(width as f32, height as f32),
        };
        Some(pin(room).position(Point::new(x as f32, y as f32)).into())
    }

    /// Loads the element's text into the overlay when editing starts on a
    /// new element, puts the cursor at the end, and focuses it.
    pub(super) fn sync_text_overlay(&mut self) -> Task<Message> {
        let editing = self
            .editor
            .editing()
            .map(|e| (e.base.id.clone(), e.original_text().to_owned()));
        match editing {
            Some((id, text)) if self.editing.as_ref() != Some(&id) => {
                self.editing = Some(id);
                self.content = text_editor::Content::with_text(&text);
                self.content
                    .perform(text_editor::Action::Move(Motion::DocumentEnd));
                operation::focus(TEXT_EDITOR)
            }
            Some(_) => Task::none(),
            None => {
                self.editing = None;
                Task::none()
            }
        }
    }
}

/// Lays the text editor out at the size of its text wherever it sits.
/// `pin` alone gives it only the window left past its position: a box
/// scrolled past the bottom edge got no height, cosmic-text laid out no line
/// for the cursor, and iced's editor panicked on the next redraw ("Line
/// layout should be cached"). Finite, as PageDown moves by the height.
struct Room<'a> {
    content: Element<'a, Message>,
    size: Size,
}

impl Widget<Message, Theme, iced::Renderer> for Room<'_> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn diff(&mut self, tree: &mut Tree) {
        self.content.as_widget_mut().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &iced::Renderer, _limits: &layout::Limits) {
        let limits = layout::Limits::new(Size::ZERO, self.size);
        self.content.as_widget_mut().layout(tree, renderer, &limits);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget_mut()
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

impl<'a> From<Room<'a>> for Element<'a, Message> {
    fn from(room: Room<'a>) -> Self {
        Element::new(room)
    }
}
