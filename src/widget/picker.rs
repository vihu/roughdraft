//! The color picker that the ring swatch after a color's quick picks opens:
//! Excalidraw's palette in all its shades, one click each, and a hue ring
//! around a saturation and brightness square for any other color.
use std::f32::consts::TAU;

use iced::widget::canvas::{self, Frame, Path, Stroke, gradient};
use iced::widget::{Canvas, button, column, container, row};
use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
};

use super::ui::{ColorField, panel_style};
use super::{Input, Message, Sketch};
use crate::color::Rgba;

/// Excalidraw's `COLOR_PALETTE` hues in its order, each at the open-color
/// shades 0, 2, 4, 6 and 8 it uses (bronze from Radix, as Excalidraw does).
const PALETTE: [[&str; 5]; 12] = [
    ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"],
    ["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"],
    ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"],
    ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"],
    ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"],
    ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"],
    ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"],
    ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"],
    ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"],
    ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"],
    ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"],
    ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"],
];

/// Selection color, as the quick-pick swatches use it.
const ACCENT: Color = Color::from_rgb8(0x69, 0x65, 0xdb);

/// Palette swatch size in pixels.
const CELL: f32 = 16.0;

/// The ring canvas: size, ring radii and the square's side, in pixels.
const SIZE: f32 = 196.0;
const RING: (f32, f32) = (76.0, 96.0);
const SQUARE: f32 = 98.0;

/// Hue slices drawn around the ring (2 degrees each).
const SLICES: usize = 180;

/// The picker while open: which color it edits, and that color as hue
/// (degrees), saturation and value.
#[derive(Clone, Copy, Debug)]
pub(super) struct Picker {
    pub(super) field: ColorField,
    pub(super) hsv: [f32; 3],
}

/// Input from the picker.
#[derive(Clone, Copy, Debug)]
pub(super) enum Pick {
    /// Opens the picker for a color, or closes it when open for that one.
    Toggle(ColorField),
    /// The ring or square moved to this color.
    Hsv([f32; 3]),
    /// The ring or square was let go.
    Release,
}

impl Picker {
    /// A picker for `field`, starting at `css` (hue `keep` when `css` has
    /// none, like grey or transparent).
    pub(super) fn new(field: ColorField, css: &str, keep: f32) -> Self {
        Self {
            field,
            hsv: hsv_of(css, keep),
        }
    }
}

// Private API
impl Sketch {
    /// Handles input from the picker: opening and closing it, and colors
    /// dragged on the ring and square (one undo step per drag).
    pub(super) fn pick(&mut self, pick: Pick) {
        match pick {
            Pick::Toggle(field) => {
                self.picker = match self.picker {
                    Some(open) if open.field == field => None,
                    _ => Some(Picker::new(field, &self.field_color(field), 0.0)),
                };
            }
            Pick::Hsv(hsv) => {
                if let Some(picker) = &mut self.picker {
                    picker.hsv = hsv;
                    let change = picker.field.change(hex_of(hsv));
                    self.editor.preview_style(change);
                }
            }
            Pick::Release => self.editor.commit_style(),
        }
    }

    /// Moves the open picker's knobs to its color after a change from
    /// elsewhere (a swatch, the hex field).
    pub(super) fn follow_picker(&mut self) {
        if let Some(Picker { field, hsv }) = self.picker {
            let css = self.field_color(field);
            self.picker = Some(Picker::new(field, &css, hsv[0]));
        }
    }

    /// The color `field` shows: the selection's, or the next element's.
    fn field_color(&self, field: ColorField) -> String {
        let style = self.editor.current_style();
        match field {
            ColorField::Stroke => style.stroke_color,
            ColorField::Background => style.background_color,
        }
    }

    /// The ring swatch after the quick picks: the current color inside a
    /// hue ring. Opens the picker.
    pub(super) fn ring_swatch<'a>(&self, field: ColorField, current: &str) -> Element<'a, Message> {
        let open = self.picker.is_some_and(|p| p.field == field);
        let face = Canvas::new(RingSwatch {
            color: swatch_color(current),
        })
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0));
        // 22 pixels like the quick picks, with room for the open ring.
        button(face)
            .padding(2)
            .style(move |_: &Theme, _| button::Style {
                border: Border {
                    color: if open { ACCENT } else { Color::TRANSPARENT },
                    width: 2.0,
                    radius: 11.0.into(),
                },
                ..button::Style::default()
            })
            .on_press(Message(Input::Pick(Pick::Toggle(field))))
            .into()
    }

    /// The open picker: the palette, then the ring and square.
    pub(super) fn picker_view<'a>(&'a self, picker: Picker, current: &str) -> Element<'a, Message> {
        let field = picker.field;
        let rows = (0..5).map(|shade| {
            row(PALETTE.iter().map(|family| {
                let css = family[shade];
                let active = css.eq_ignore_ascii_case(current);
                button(container("").center(Length::Fixed(CELL)))
                    .padding(0)
                    .style(move |_: &Theme, _| button::Style {
                        background: Some(Background::Color(swatch_color(css))),
                        border: Border {
                            color: if active { ACCENT } else { Color::TRANSPARENT },
                            width: 2.0,
                            radius: 3.0.into(),
                        },
                        ..button::Style::default()
                    })
                    .on_press(Message(Input::Style(field.change(css.to_owned()))))
                    .into()
            }))
            .spacing(2)
            .into()
        });
        let ring = Canvas::new(Ring {
            hsv: picker.hsv,
            transparent: Rgba::parse(current).is_none_or(|c| c.a == 0.0),
            cache: &self.ring,
        })
        .width(Length::Fixed(SIZE))
        .height(Length::Fixed(SIZE));
        let content = column![column(rows).spacing(2), ring]
            .spacing(12)
            .align_x(iced::Alignment::Center);
        container(content).padding(12).style(panel_style).into()
    }
}

/// The ring and square, which report colors as they are dragged.
struct Ring<'a> {
    hsv: [f32; 3],
    /// The color is transparent: no square knob, and turning the ring
    /// starts from a visible color.
    transparent: bool,
    /// The hue ring, which never changes.
    cache: &'a canvas::Cache,
}

/// What a press on the ring canvas grabbed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Ring,
    Square,
}

impl canvas::Program<Message> for Ring<'_> {
    type State = Option<Part>;

    fn update(
        &self,
        state: &mut Option<Part>,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let publish =
            |pick| Some(canvas::Action::publish(Message(Input::Pick(pick))).and_capture());
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                let part = part_at(at)?;
                *state = Some(part);
                publish(Pick::Hsv(self.pick(part, at)))
            }
            // Followed outside the canvas too, until the button is let go.
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let part = (*state)?;
                let at = Point::new(position.x - bounds.x, position.y - bounds.y);
                publish(Pick::Hsv(self.pick(part, at)))
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.take()?;
                publish(Pick::Release)
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Option<Part>,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let c = SIZE / 2.0;
        let ring = self.cache.draw(renderer, bounds.size(), |frame| {
            // Slices overlap a little so no seams show between them.
            let step = TAU / SLICES as f32;
            for i in 0..SLICES {
                let (a0, a1) = (i as f32 * step - 0.004, (i + 1) as f32 * step + 0.004);
                let at = |a: f32, r: f32| Point::new(c + r * a.sin(), c - r * a.cos());
                let slice = Path::new(|p| {
                    p.move_to(at(a0, RING.0));
                    p.line_to(at(a0, RING.1));
                    p.line_to(at(a1, RING.1));
                    p.line_to(at(a1, RING.0));
                    p.close();
                });
                let hue = (i as f32 + 0.5) * 360.0 / SLICES as f32;
                frame.fill(&slice, color_of([hue, 1.0, 1.0]));
            }
        });
        let mut frame = Frame::new(renderer, bounds.size());
        let [hue, s, v] = self.hsv;
        // Saturation from left to right, value from bottom to top.
        let corner = Point::new(c - SQUARE / 2.0, c - SQUARE / 2.0);
        let side = Size::new(SQUARE, SQUARE);
        frame.fill_rectangle(corner, side, color_of([hue, 1.0, 1.0]));
        let white = gradient::Linear::new(corner, Point::new(corner.x + SQUARE, corner.y))
            .add_stop(0.0, Color::WHITE)
            .add_stop(
                1.0,
                Color {
                    a: 0.0,
                    ..Color::WHITE
                },
            );
        frame.fill_rectangle(corner, side, white);
        let black = gradient::Linear::new(corner, Point::new(corner.x, corner.y + SQUARE))
            .add_stop(
                0.0,
                Color {
                    a: 0.0,
                    ..Color::BLACK
                },
            )
            .add_stop(1.0, Color::BLACK);
        frame.fill_rectangle(corner, side, black);
        let knob = |frame: &mut Frame, at: Point, fill: Color| {
            let dot = Path::circle(at, 7.0);
            frame.fill(&dot, fill);
            frame.stroke(
                &dot,
                Stroke::default().with_color(Color::WHITE).with_width(2.5),
            );
            let rim = Stroke::default().with_color(Color {
                a: 0.35,
                ..Color::BLACK
            });
            frame.stroke(&Path::circle(at, 8.5), rim.with_width(1.0));
        };
        let mid = (RING.0 + RING.1) / 2.0;
        let a = hue.to_radians();
        knob(
            &mut frame,
            Point::new(c + mid * a.sin(), c - mid * a.cos()),
            color_of([hue, 1.0, 1.0]),
        );
        if !self.transparent {
            let at = Point::new(corner.x + s * SQUARE, corner.y + (1.0 - v) * SQUARE);
            knob(&mut frame, at, color_of(self.hsv));
        }
        vec![ring, frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Option<Part>,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over = cursor.position_in(bounds).and_then(part_at);
        if state.is_some() || over.is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

impl Ring<'_> {
    /// The color for the pointer at `at` on `part`.
    fn pick(&self, part: Part, at: Point) -> [f32; 3] {
        let c = SIZE / 2.0;
        let [hue, s, v] = self.hsv;
        match part {
            Part::Ring => {
                let hue = (at.x - c).atan2(c - at.y).to_degrees().rem_euclid(360.0);
                // A transparent color has none to turn: start from a strong one.
                if self.transparent {
                    [hue, 0.8, 0.8]
                } else {
                    [hue, s, v]
                }
            }
            Part::Square => {
                let corner = c - SQUARE / 2.0;
                let s = ((at.x - corner) / SQUARE).clamp(0.0, 1.0);
                let v = (1.0 - (at.y - corner) / SQUARE).clamp(0.0, 1.0);
                [hue, s, v]
            }
        }
    }
}

/// The part of the ring canvas under `at`, with a little slack.
fn part_at(at: Point) -> Option<Part> {
    let c = SIZE / 2.0;
    let slack = 4.0;
    let distance = (at.x - c).hypot(at.y - c);
    let half = SQUARE / 2.0 + slack;
    if (RING.0 - slack..=RING.1 + slack).contains(&distance) {
        Some(Part::Ring)
    } else if (at.x - c).abs() <= half && (at.y - c).abs() <= half {
        Some(Part::Square)
    } else {
        None
    }
}

/// The ring swatch's face: a small hue ring around the current color.
struct RingSwatch {
    color: Color,
}

impl canvas::Program<Message> for RingSwatch {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = bounds.width / 2.0;
        let (inner, outer) = (c - 4.5, c);
        let slices = 36;
        let step = TAU / slices as f32;
        let at = |a: f32, r: f32| Point::new(c + r * a.sin(), c - r * a.cos());
        for i in 0..slices {
            let (a0, a1) = (i as f32 * step - 0.02, (i + 1) as f32 * step + 0.02);
            let slice = Path::new(|p| {
                p.move_to(at(a0, inner));
                p.line_to(at(a0, outer));
                p.line_to(at(a1, outer));
                p.line_to(at(a1, inner));
                p.close();
            });
            let hue = i as f32 * 360.0 / slices as f32;
            frame.fill(&slice, color_of([hue, 0.85, 1.0]));
        }
        let middle = Path::circle(Point::new(c, c), inner - 1.0);
        frame.fill(&middle, self.color);
        frame.stroke(
            &middle,
            Stroke::default().with_color(Color {
                a: 0.18,
                ..Color::BLACK
            }),
        );
        vec![frame.into_geometry()]
    }
}

/// A swatch's fill: the color as stored, white for transparent or unknown.
fn swatch_color(css: &str) -> Color {
    match Rgba::parse(css) {
        Some(Rgba { r, g, b, a }) if a > 0.0 => Color { r, g, b, a },
        _ => Color::WHITE,
    }
}

/// `css` as hue, saturation and value; `keep` is the hue when it has none.
fn hsv_of(css: &str, keep: f32) -> [f32; 3] {
    let Some(Rgba { r, g, b, .. }) = Rgba::parse(css).filter(|c| c.a > 0.0) else {
        return [keep, 0.0, 1.0];
    };
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let hue = if delta == 0.0 {
        keep
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    [hue, saturation, max]
}

/// The color at hue (degrees), saturation and value.
fn color_of([hue, s, v]: [f32; 3]) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (hue.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::from_rgb(r + m, g + m, b + m)
}

/// The color at `hsv` as `#rrggbb`, how Excalidraw stores picked colors.
fn hex_of(hsv: [f32; 3]) -> String {
    let Color { r, g, b, .. } = color_of(hsv);
    let byte = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

#[cfg(test)]
mod tests {
    use super::{Pick, hex_of, hsv_of};
    use crate::edit::{Command, Tool};
    use crate::scene::Scene;
    use crate::widget::ui::ColorField;
    use crate::widget::{Input, Message, Sketch};

    #[test]
    fn a_drag_on_the_ring_recolors_the_selection_in_one_undo_step() {
        let mut sketch = Sketch::new(Scene::default());
        let mut send = |input| {
            let _ = sketch.update(Message(input));
        };
        send(Input::Command(Command::Tool(Tool::Rectangle)));
        let none = crate::edit::Modifiers::default();
        for (pointer, at) in [
            (crate::edit::Pointer::Down, [0.0, 0.0]),
            (crate::edit::Pointer::Move, [80.0, 60.0]),
            (crate::edit::Pointer::Up, [80.0, 60.0]),
        ] {
            send(Input::Pointer(pointer, at, none));
        }
        send(Input::Pick(Pick::Toggle(ColorField::Stroke)));
        for hue in [10.0, 20.0, 30.0] {
            send(Input::Pick(Pick::Hsv([hue, 1.0, 1.0])));
        }
        send(Input::Pick(Pick::Release));
        let stroke = |sketch: &Sketch| sketch.scene().elements[0].base.stroke_color.clone();
        assert_eq!(stroke(&sketch), hex_of([30.0, 1.0, 1.0]));
        assert!(sketch.picker.is_some(), "stays open after a drag");
        let _ = sketch.update(Message(Input::Command(Command::Undo)));
        assert_eq!(stroke(&sketch), "#1e1e1e");
        assert!(sketch.picker.is_none(), "a command closes it");
    }

    #[test]
    fn hex_survives_the_trip_through_hsv() {
        for css in [
            "#1971c2", "#e03131", "#ffec99", "#343a40", "#ffffff", "#000000",
        ] {
            assert_eq!(hex_of(hsv_of(css, 0.0)), css);
        }
        // Grey has no hue: the one it had is kept.
        assert_eq!(hsv_of("#868e96", 0.0)[0].round(), 210.0);
        assert_eq!(hsv_of("#808080", 42.0), [42.0, 0.0, 128.0 / 255.0]);
        assert_eq!(hsv_of("transparent", 42.0), [42.0, 0.0, 1.0]);
    }
}
