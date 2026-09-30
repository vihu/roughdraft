//! Tool bar and style panel around the canvas, laid out like Excalidraw's:
//! tools across the top, style options down the left. Values follow
//! REFERENCE-001 section 14.
use iced::widget::{
    Column, button, column, container, opaque, pick_list, row, slider, space, text, text_input,
    themer,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use super::camera::ZoomKey;
use super::{Appearance, Input, Message, Sketch};
use crate::color::Rgba;
use crate::edit::{Command, Style, StyleChange, Tool};
use crate::scene::{Arrowhead, FillStyle, Kind, StrokeStyle, TextAlign};

/// A colour the style panel edits as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColorField {
    Stroke,
    Background,
}

impl ColorField {
    /// The style change that sets this colour to `css`.
    pub(super) fn change(self, css: String) -> StyleChange {
        match self {
            Self::Stroke => StyleChange::StrokeColor(css),
            Self::Background => StyleChange::BackgroundColor(css),
        }
    }
}

/// Stroke quick picks.
const STROKES: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];

/// Background quick picks.
const BACKGROUNDS: [&str; 5] = ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/// Tools in Excalidraw's order, with their shortcut key.
const TOOLS: [(Tool, &str, &str); 8] = [
    (Tool::Hand, "Hand", "H"),
    (Tool::Selection, "Select", "1"),
    (Tool::Rectangle, "Rect", "2"),
    (Tool::Diamond, "Diamond", "3"),
    (Tool::Ellipse, "Ellipse", "4"),
    (Tool::Arrow, "Arrow", "5"),
    (Tool::Line, "Line", "6"),
    (Tool::Text, "Text", "8"),
];

/// Arrowhead choices for the pick lists.
#[derive(Clone, Debug, PartialEq)]
struct Head(Option<Arrowhead>);

impl std::fmt::Display for Head {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match &self.0 {
            None => "None",
            Some(Arrowhead::Arrow) => "Arrow",
            Some(Arrowhead::Triangle) => "Triangle",
            Some(Arrowhead::TriangleOutline) => "Triangle (outline)",
            Some(Arrowhead::Circle | Arrowhead::Dot) => "Circle",
            Some(Arrowhead::CircleOutline) => "Circle (outline)",
            Some(Arrowhead::Diamond) => "Diamond",
            Some(Arrowhead::DiamondOutline) => "Diamond (outline)",
            Some(Arrowhead::Bar) => "Bar",
            Some(Arrowhead::CrowfootOne) => "One",
            Some(Arrowhead::CrowfootMany) => "Many",
            Some(Arrowhead::CrowfootOneOrMany) => "One or many",
            Some(Arrowhead::Other(name)) => name,
        };
        f.write_str(name)
    }
}

fn heads() -> Vec<Head> {
    let all = [
        None,
        Some(Arrowhead::Arrow),
        Some(Arrowhead::Triangle),
        Some(Arrowhead::TriangleOutline),
        Some(Arrowhead::Circle),
        Some(Arrowhead::CircleOutline),
        Some(Arrowhead::Diamond),
        Some(Arrowhead::DiamondOutline),
        Some(Arrowhead::Bar),
        Some(Arrowhead::CrowfootOne),
        Some(Arrowhead::CrowfootMany),
        Some(Arrowhead::CrowfootOneOrMany),
    ];
    all.into_iter().map(Head).collect()
}

impl Sketch {
    /// Tools across the top, with the tool lock first.
    pub(super) fn toolbar(&self) -> Element<'_, Message> {
        let current = self.editor.tool();
        let lock = choice(
            "Lock",
            self.editor.is_tool_locked(),
            Message(Input::Command(Command::ToggleLock)),
        );
        let tools = TOOLS.iter().map(|(tool, name, key)| {
            let label =
                column![text(*name).size(13), text(*key).size(9)].align_x(Alignment::Center);
            button(label)
                .padding([4, 8])
                .style(if *tool == current {
                    button::primary
                } else {
                    button::text
                })
                .on_press(Message(Input::Command(Command::Tool(*tool))))
                .into()
        });
        let bar = row(std::iter::once(lock).chain(tools))
            .spacing(2)
            .align_y(Alignment::Center);
        let bar = container(bar).padding(4).style(panel_style);
        container(opaque(self.themed(bar)))
            .center_x(Length::Fill)
            .padding(12)
            .into()
    }

    /// Zoom and history buttons in the bottom-left corner, like Excalidraw's
    /// footer: zoom out, the zoom level (click for 100%), zoom in; undo,
    /// redo.
    pub(super) fn footer(&self) -> Element<'_, Message> {
        let flat = |label: String, message: Message| {
            button(text(label).size(13))
                .padding([3, 10])
                .style(button::text)
                .on_press(message)
        };
        let zoom = |key: ZoomKey| Message(Input::ZoomKey(key));
        let command = |command: Command| Message(Input::Command(command));
        let percent = format!("{:.0}%", self.camera.get().zoom * 100.0);
        let zoom_group = row![
            flat("-".into(), zoom(ZoomKey::Out)),
            flat(percent, zoom(ZoomKey::Reset)),
            flat("+".into(), zoom(ZoomKey::In)),
        ]
        .align_y(Alignment::Center);
        let history = row![
            flat("Undo".into(), command(Command::Undo)),
            flat("Redo".into(), command(Command::Redo)),
        ];
        let bar = row![
            container(zoom_group).padding(2).style(panel_style),
            container(history).padding(2).style(panel_style),
        ]
        .spacing(8);
        container(opaque(self.themed(bar)))
            .align_bottom(Length::Fill)
            .padding(12)
            .into()
    }

    /// Draws `content` in the light or dark iced theme that matches the
    /// canvas, whatever the host's theme is, like Excalidraw's UI.
    fn themed<'a>(&self, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        let theme = match self.appearance {
            Appearance::Light => Theme::Light,
            Appearance::Dark => Theme::Dark,
        };
        themer(Some(theme), content)
            .text_color(|theme| theme.palette().background.base.text)
            .into()
    }

    /// Style options for what is selected, or for the tool about to draw.
    /// `None` when neither applies.
    pub(super) fn style_panel(&self) -> Option<Element<'_, Message>> {
        let tool = self.editor.tool();
        let selected: Vec<_> = self.editor.selection().collect();
        let drawing = !matches!(tool, Tool::Selection | Tool::Hand);
        if selected.is_empty() && !drawing {
            return None;
        }
        let has = |test: fn(&Kind) -> bool| selected.iter().any(|e| test(&e.kind));
        let text_only =
            !selected.is_empty() && selected.iter().all(|e| matches!(e.kind, Kind::Text(_)));
        let arrows = tool == Tool::Arrow || has(|k| matches!(k, Kind::Arrow(_)));
        let texts = tool == Tool::Text
            || has(|k| matches!(k, Kind::Text(_)))
            || selected.iter().any(|e| {
                e.base
                    .bound_elements
                    .iter()
                    .flatten()
                    .any(|b| b.kind == "text")
            });
        let shapes = !text_only && tool != Tool::Text;
        let style = self.editor.current_style();

        let mut sections: Vec<Element<'_, Message>> = vec![section(
            "Stroke",
            self.colors(&STROKES, &style.stroke_color, ColorField::Stroke),
        )];
        if shapes {
            sections.push(section(
                "Background",
                self.colors(
                    &BACKGROUNDS,
                    &style.background_color,
                    ColorField::Background,
                ),
            ));
            sections.push(section(
                "Fill",
                choices(&[
                    (
                        "Hachure",
                        style.fill_style == FillStyle::Hachure,
                        StyleChange::FillStyle(FillStyle::Hachure),
                    ),
                    (
                        "Cross",
                        style.fill_style == FillStyle::CrossHatch,
                        StyleChange::FillStyle(FillStyle::CrossHatch),
                    ),
                    (
                        "Solid",
                        style.fill_style == FillStyle::Solid,
                        StyleChange::FillStyle(FillStyle::Solid),
                    ),
                ]),
            ));
            sections.push(section(
                "Stroke width",
                choices(&[
                    (
                        "Thin",
                        style.stroke_width == 1.0,
                        StyleChange::StrokeWidth(1.0),
                    ),
                    (
                        "Bold",
                        style.stroke_width == 2.0,
                        StyleChange::StrokeWidth(2.0),
                    ),
                    (
                        "Extra",
                        style.stroke_width == 4.0,
                        StyleChange::StrokeWidth(4.0),
                    ),
                ]),
            ));
            sections.push(section(
                "Stroke style",
                choices(&[
                    (
                        "Solid",
                        style.stroke_style == StrokeStyle::Solid,
                        StyleChange::StrokeStyle(StrokeStyle::Solid),
                    ),
                    (
                        "Dashed",
                        style.stroke_style == StrokeStyle::Dashed,
                        StyleChange::StrokeStyle(StrokeStyle::Dashed),
                    ),
                    (
                        "Dotted",
                        style.stroke_style == StrokeStyle::Dotted,
                        StyleChange::StrokeStyle(StrokeStyle::Dotted),
                    ),
                ]),
            ));
            sections.push(section(
                "Sloppiness",
                choices(&[
                    (
                        "Architect",
                        style.roughness == 0.0,
                        StyleChange::Roughness(0.0),
                    ),
                    (
                        "Artist",
                        style.roughness == 1.0,
                        StyleChange::Roughness(1.0),
                    ),
                    (
                        "Cartoonist",
                        style.roughness == 2.0,
                        StyleChange::Roughness(2.0),
                    ),
                ]),
            ));
            sections.push(section(
                "Edges",
                choices(&[
                    ("Sharp", !style.round_edges, StyleChange::RoundEdges(false)),
                    ("Round", style.round_edges, StyleChange::RoundEdges(true)),
                ]),
            ));
        }
        if arrows {
            sections.push(section("Arrowheads", arrowheads(&style)));
        }
        if texts {
            let size = |label, value: f64| {
                (
                    label,
                    style.font_size == value,
                    StyleChange::FontSize(value),
                )
            };
            sections.push(section(
                "Font size",
                choices(&[
                    size("S", 16.0),
                    size("M", 20.0),
                    size("L", 28.0),
                    size("XL", 36.0),
                ]),
            ));
            sections.push(section(
                "Font",
                choices(&[
                    ("Hand", style.font_family == 5, StyleChange::FontFamily(5)),
                    ("Normal", style.font_family == 6, StyleChange::FontFamily(6)),
                    ("Code", style.font_family == 8, StyleChange::FontFamily(8)),
                ]),
            ));
            sections.push(section(
                "Align",
                choices(&[
                    (
                        "Left",
                        style.text_align == TextAlign::Left,
                        StyleChange::TextAlign(TextAlign::Left),
                    ),
                    (
                        "Center",
                        style.text_align == TextAlign::Center,
                        StyleChange::TextAlign(TextAlign::Center),
                    ),
                    (
                        "Right",
                        style.text_align == TextAlign::Right,
                        StyleChange::TextAlign(TextAlign::Right),
                    ),
                ]),
            ));
        }
        let opacity = slider(0.0..=100.0, style.opacity, |value| {
            Message(Input::Style(StyleChange::Opacity(value)))
        })
        .step(10.0)
        .width(Length::Fixed(180.0));
        sections.push(section("Opacity", opacity.into()));

        let panel = container(Column::with_children(sections).spacing(10))
            .padding(12)
            .style(panel_style);
        Some(
            container(opaque(self.themed(panel)))
                .padding(iced::Padding {
                    top: 76.0,
                    left: 12.0,
                    right: 0.0,
                    bottom: 0.0,
                })
                .into(),
        )
    }
}

impl Sketch {
    /// Quick-pick swatches with a hex field under them for any other colour.
    fn colors<'a>(
        &self,
        picks: &[&'static str],
        current: &str,
        field: ColorField,
    ) -> Element<'a, Message> {
        let typed = match &self.color_draft {
            Some((draft, text)) if *draft == field => text.clone(),
            _ => current.to_owned(),
        };
        let hex = text_input("#hex or name", typed)
            .on_input(move |text| Message(Input::ColorText(field, text)))
            .size(12)
            .padding([2, 6])
            .width(Length::Fixed(90.0));
        column![swatches(picks, current, move |css| field.change(css)), hex]
            .spacing(4)
            .into()
    }
}

fn section<'a>(title: &'a str, content: Element<'a, Message>) -> Element<'a, Message> {
    column![text(title).size(11), content].spacing(4).into()
}

fn choice<'a>(label: &'a str, active: bool, message: Message) -> Element<'a, Message> {
    button(text(label).size(12))
        .padding([3, 8])
        .style(if active {
            button::primary
        } else {
            button::secondary
        })
        .on_press(message)
        .into()
}

fn choices<'a>(options: &[(&'a str, bool, StyleChange)]) -> Element<'a, Message> {
    row(options.iter().map(|(label, active, change)| {
        choice(label, *active, Message(Input::Style(change.clone())))
    }))
    .spacing(4)
    .into()
}

fn swatches<'a>(
    colors: &[&'static str],
    current: &str,
    change: impl Fn(String) -> StyleChange,
) -> Element<'a, Message> {
    row(colors.iter().map(|css| {
        let Rgba { r, g, b, a } = Rgba::parse(css).unwrap_or(Rgba::WHITE);
        let fill = if a == 0.0 {
            Color::WHITE
        } else {
            Color { r, g, b, a }
        };
        let active = css.eq_ignore_ascii_case(current);
        let mark = if a == 0.0 {
            text("∅").size(12)
        } else {
            text("")
        };
        button(container(mark).center(Length::Fixed(22.0)))
            .padding(0)
            .style(move |_: &Theme, _| button::Style {
                background: Some(Background::Color(fill)),
                border: Border {
                    color: if active {
                        Color::from_rgb8(0x69, 0x65, 0xdb)
                    } else {
                        Color::from_rgb8(0xce, 0xd4, 0xda)
                    },
                    width: if active { 2.0 } else { 1.0 },
                    radius: 4.0.into(),
                },
                ..button::Style::default()
            })
            .on_press(Message(Input::Style(change((*css).to_owned()))))
            .into()
    }))
    .spacing(4)
    .into()
}

fn arrowheads(style: &Style) -> Element<'static, Message> {
    let start = pick_list(
        Some(Head(style.start_arrowhead.clone())),
        heads(),
        Head::to_string,
    )
    .on_select(|head: Head| Message(Input::Style(StyleChange::StartArrowhead(head.0))))
    .text_size(12);
    let end = pick_list(
        Some(Head(style.end_arrowhead.clone())),
        heads(),
        Head::to_string,
    )
    .on_select(|head: Head| Message(Input::Style(StyleChange::EndArrowhead(head.0))))
    .text_size(12);
    row![start, end, space::horizontal()].spacing(4).into()
}

fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: iced::Shadow {
            color: Color {
                a: 0.08,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}
