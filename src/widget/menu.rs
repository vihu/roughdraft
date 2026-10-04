//! The main menu in the top-left corner, like Excalidraw's: the actions
//! the host offers (open, save, export, ...), the grid and the dark/light
//! switch, and a line of text from the host such as its version.
use iced::widget::{button, column, container, opaque, rule, text};
use iced::{Element, Length};

use super::icons::{Glyph, icon};
use super::ui::panel_style;
use super::{Appearance, Input, Message, Sketch};
use crate::edit::Command;

/// An action the main menu asks the host to carry out: the menu cannot
/// know where the host keeps its files. See [`Message::request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Open a scene.
    Open,
    /// Save the scene where it came from.
    Save,
    /// Save the scene somewhere new.
    SaveAs,
    /// Export the scene as an SVG image (`crate::svg::export`).
    ExportSvg,
    /// Insert an image file.
    InsertImage,
}

impl Request {
    /// The menu's label for it.
    fn label(self) -> &'static str {
        match self {
            Request::Open => "Open",
            Request::Save => "Save",
            Request::SaveAs => "Save as...",
            Request::ExportSvg => "Export SVG",
            Request::InsertImage => "Insert image",
        }
    }
}

/// Width of the open menu.
const MENU_WIDTH: f32 = 200.0;

// Public API
impl Message {
    /// Returns the host action this message asks for, when it comes from a
    /// main menu item. Handle it, then pass the message on to
    /// [`Sketch::update`] as usual (which closes the menu).
    pub fn request(&self) -> Option<Request> {
        match self.0 {
            Input::Request(request) => Some(request),
            _ => None,
        }
    }
}

// Public API
impl Sketch {
    /// Sets the host actions the main menu offers, in this order. None by
    /// default; the grid and dark/light switches are always there.
    pub fn set_menu(&mut self, items: Vec<Request>) {
        self.menu = items;
    }

    /// Sets a line of text shown under the main menu's items, such as the
    /// host's name and version. None by default.
    pub fn set_menu_footer(&mut self, footer: impl Into<String>) {
        self.menu_footer = Some(footer.into());
    }
}

// Private API
impl Sketch {
    /// The menu button, and the menu under it while open.
    pub(super) fn menu(&self) -> Element<'_, Message> {
        let ink = self.theme().palette().background.base.text;
        let toggle = button(icon(Glyph::Menu, ink))
            .padding(6)
            .style(button::text)
            .on_press(Message(Input::ToggleMenu));
        let mut content = column![container(toggle).padding(2).style(panel_style)].spacing(6);
        if self.menu_open {
            let item = |label: &'static str, message: Message| {
                button(text(label).size(14))
                    .width(Length::Fill)
                    .padding([6, 10])
                    .style(button::text)
                    .on_press(message)
            };
            let theme = match self.appearance {
                Appearance::Light => "Dark mode",
                Appearance::Dark => "Light mode",
            };
            let mut items =
                column(self.menu.iter().map(|&request| {
                    item(request.label(), Message(Input::Request(request))).into()
                }));
            if !self.menu.is_empty() {
                items = items.push(rule::horizontal(1));
            }
            // Excalidraw has it in the canvas context menu (Ctrl+').
            let grid = if self.editor.scene().grid().is_some() {
                "Hide grid"
            } else {
                "Show grid"
            };
            items = items.push(item(grid, Message(Input::Command(Command::ToggleGrid))));
            items = items.push(item(theme, Message(Input::ToggleAppearance)));
            if let Some(footer) = &self.menu_footer {
                items = items
                    .push(rule::horizontal(1))
                    .push(container(text(footer).size(12).style(text::secondary)).padding([6, 10]));
            }
            content = content.push(
                container(items)
                    .width(Length::Fixed(MENU_WIDTH))
                    .padding(4)
                    .style(panel_style),
            );
        }
        container(opaque(self.themed(content))).padding(12).into()
    }
}
