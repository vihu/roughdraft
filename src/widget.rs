//! iced canvas that draws a scene, with pan, zoom and dark mode.
//!
//! Geometry is mapped to screen space here, before it reaches iced, so stroke
//! widths and dashes scale with zoom the same way on the wgpu and tiny-skia
//! backends (they disagree on how frame transforms affect strokes).
use iced::keyboard;
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::{
    self, Canvas, Event, Fill, Frame, Geometry, LineCap, LineDash, LineJoin, Path, Stroke, Style,
    fill,
};
use iced::widget::text::{Alignment, LineHeight, Shaping};
use iced::{Color, Element, Font, Length, Pixels, Point, Rectangle, Renderer, Theme, Vector};

use crate::color::Rgba;
use crate::geometry::Affine;
use crate::render::{self, Align, Drawing, FillRule, Item, Segment, TextBlock};
use crate::scene::Scene;

/// Excalifont, Excalidraw's default hand-drawn font (OFL-1.1).
///
/// Register it with `iced::application(..).fonts([EXCALIFONT])` so text renders
/// in it.
pub const EXCALIFONT: &[u8] = include_bytes!("../assets/fonts/Excalifont/Excalifont-Regular.ttf");

/// Canvas color scheme, like Excalidraw's theme toggle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// Colors as stored.
    #[default]
    Light,
    /// Excalidraw's dark filter applied to every color.
    Dark,
}

/// Read-only view of a scene: drag or scroll to pan, Ctrl+scroll to zoom.
#[derive(Debug)]
pub struct Viewer {
    drawings: Vec<Drawing>,
    background: Rgba,
    appearance: Appearance,
    content_origin: [f64; 2],
    cache: canvas::Cache,
}

/// Pan and zoom state of a [`Viewer`].
#[derive(Debug)]
pub struct Camera {
    /// Scene point at the canvas' top-left corner; `None` until first moved.
    origin: Option<[f64; 2]>,
    zoom: f64,
    drag_from: Option<Point>,
    modifiers: keyboard::Modifiers,
}

impl Viewer {
    /// Renders `scene` once; the viewer keeps no reference to it.
    pub fn new(scene: &Scene) -> Self {
        let drawings = render::render(scene);
        Self {
            content_origin: content_origin(&drawings),
            drawings,
            background: Rgba::parse(scene.background_color()).unwrap_or(Rgba::WHITE),
            appearance: Appearance::Light,
            cache: canvas::Cache::new(),
        }
    }

    /// Returns the current color scheme.
    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    /// Switches the color scheme.
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
        self.cache.clear();
    }

    /// Sets the scene point shown at the canvas' top-left corner on open.
    ///
    /// Defaults to the content's top-left minus a small padding.
    pub fn set_origin(&mut self, origin: [f64; 2]) {
        self.content_origin = origin;
        self.cache.clear();
    }

    /// Returns the canvas widget, filling the available space.
    pub fn view<Message: 'static>(&self) -> Element<'_, Message> {
        Canvas::new(self)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            origin: None,
            zoom: 1.0,
            drag_from: None,
            modifiers: keyboard::Modifiers::default(),
        }
    }
}

/// Excalidraw's zoom limits.
const ZOOM: std::ops::RangeInclusive<f64> = 0.1..=30.0;

/// Pixels per scrolled line.
const LINE_HEIGHT: f64 = 50.0;

/// Gap between the content and the canvas edge on open, like Excalidraw's
/// SVG export padding.
const PADDING: f64 = 10.0;

impl<Message> canvas::Program<Message> for Viewer {
    type State = Camera;

    fn update(
        &self,
        camera: &mut Camera,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let origin = camera.origin.unwrap_or(self.content_origin);
        let moved = match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                camera.modifiers = *modifiers;
                return None;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let at = cursor.position_in(bounds)?;
                let (dx, dy) = match *delta {
                    ScrollDelta::Lines { x, y } => {
                        (f64::from(x) * LINE_HEIGHT, f64::from(y) * LINE_HEIGHT)
                    }
                    ScrollDelta::Pixels { x, y } => (f64::from(x), f64::from(y)),
                };
                if camera.modifiers.command() {
                    let zoom = (camera.zoom * (dy / 500.0).exp()).clamp(*ZOOM.start(), *ZOOM.end());
                    let at = [f64::from(at.x), f64::from(at.y)];
                    let origin = zoom_at(origin, camera.zoom, zoom, at);
                    camera.zoom = zoom;
                    origin
                } else {
                    let (dx, dy) = if camera.modifiers.shift() {
                        (dy, dx)
                    } else {
                        (dx, dy)
                    };
                    [origin[0] - dx / camera.zoom, origin[1] - dy / camera.zoom]
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left | mouse::Button::Middle,
            )) => {
                camera.drag_from = Some(cursor.position_in(bounds)?);
                return Some(canvas::Action::capture());
            }
            Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left | mouse::Button::Middle,
            )) => {
                camera.drag_from.take()?;
                return Some(canvas::Action::capture());
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let from = camera.drag_from.replace(*position)?;
                let delta = *position - from;
                [
                    origin[0] - f64::from(delta.x) / camera.zoom,
                    origin[1] - f64::from(delta.y) / camera.zoom,
                ]
            }
            _ => return None,
        };
        camera.origin = Some(moved);
        self.cache.clear();
        Some(canvas::Action::request_redraw().and_capture())
    }

    fn draw(
        &self,
        camera: &Camera,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let scene = self.cache.draw(renderer, bounds.size(), |frame| {
            let dark = self.appearance == Appearance::Dark;
            let paint = |color: Rgba| {
                let Rgba { r, g, b, a } = if dark { color.to_dark() } else { color };
                Color { r, g, b, a }
            };
            frame.fill_rectangle(Point::ORIGIN, frame.size(), paint(self.background));

            let origin = camera.origin.unwrap_or(self.content_origin);
            let view = Affine::translate([-origin[0], -origin[1]]).then(Affine::scale(camera.zoom));
            for drawing in &self.drawings {
                let transform = drawing.transform.then(view);
                for item in &drawing.items {
                    draw_item(frame, item, transform, &paint);
                }
            }
        });
        vec![scene]
    }

    fn mouse_interaction(
        &self,
        camera: &Camera,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match (camera.drag_from, cursor.is_over(bounds)) {
            (Some(_), _) => mouse::Interaction::Grabbing,
            (None, true) => mouse::Interaction::Grab,
            (None, false) => mouse::Interaction::default(),
        }
    }
}

/// Returns the origin that keeps the scene point under `cursor` (canvas
/// pixels) fixed while zooming from `from` to `to`.
fn zoom_at(origin: [f64; 2], from: f64, to: f64, cursor: [f64; 2]) -> [f64; 2] {
    let anchor = [origin[0] + cursor[0] / from, origin[1] + cursor[1] / from];
    [anchor[0] - cursor[0] / to, anchor[1] - cursor[1] / to]
}

fn draw_item(frame: &mut Frame, item: &Item, transform: Affine, paint: &impl Fn(Rgba) -> Color) {
    let scale = transform.scale_factor();
    match item {
        Item::Stroke { color, .. } | Item::Fill { color, .. } if color.a == 0.0 => {}
        Item::Stroke {
            path,
            color,
            width,
            dash,
        } => {
            let dash: Vec<f32> = dash.iter().flatten().map(|d| (d * scale) as f32).collect();
            let stroke = Stroke {
                style: Style::Solid(paint(*color)),
                width: (width * scale) as f32,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                line_dash: LineDash {
                    segments: &dash,
                    offset: 0,
                },
            };
            // SVG and canvas restart the dash pattern at every subpath (rough
            // draws each side as one); iced's dasher carries it across.
            for subpath in path.chunk_by(|_, next| !matches!(next, Segment::MoveTo(_))) {
                frame.stroke(&to_path(subpath, transform), stroke);
            }
        }
        Item::Fill { path, color, rule } => {
            let rule = match rule {
                FillRule::NonZero => fill::Rule::NonZero,
                FillRule::EvenOdd => fill::Rule::EvenOdd,
            };
            let style = Style::Solid(paint(*color));
            frame.fill(&to_path(path, transform), Fill { style, rule });
        }
        Item::Text(block) => draw_text(frame, block, transform, paint),
    }
}

/// Draws each line with its top at `i * line_height`. iced centers the font's
/// ascent and descent in the line box, which is how Excalidraw places its
/// baseline (`getVerticalOffset`), so `block.baseline` is implied.
fn draw_text(
    frame: &mut Frame,
    block: &TextBlock,
    transform: Affine,
    paint: &impl Fn(Rgba) -> Color,
) {
    let scale = transform.scale_factor();
    let rotation = transform.rotation();
    let align_x = match block.align {
        Align::Start => Alignment::Left,
        Align::Middle => Alignment::Center,
        Align::End => Alignment::Right,
    };
    for (i, line) in block.lines.iter().enumerate() {
        let [x, y] = transform.apply([block.x, i as f64 * block.line_height]);
        frame.with_save(|frame| {
            frame.translate(Vector::new(x as f32, y as f32));
            if rotation != 0.0 {
                // iced draws rotated text as vector paths.
                frame.rotate(rotation as f32);
            }
            frame.fill_text(canvas::Text {
                content: line.clone(),
                color: paint(block.color),
                size: Pixels((block.font_size * scale) as f32),
                line_height: LineHeight::Absolute(Pixels((block.line_height * scale) as f32)),
                font: font(block.font_family),
                align_x,
                shaping: Shaping::Advanced,
                ..canvas::Text::default()
            });
        });
    }
}

/// Maps an Excalidraw font id to a font iced can load.
// ponytail: only Excalifont is bundled; add Virgil, Cascadia, Nunito, ... when a fixture needs them
fn font(family: u32) -> Font {
    match family {
        3 => Font::MONOSPACE,
        2 | 6 | 9 => Font::DEFAULT,
        _ => Font::new("Excalifont"),
    }
}

fn to_path(segments: &[Segment], transform: Affine) -> Path {
    let point = |p| {
        let [x, y] = transform.apply(p);
        Point::new(x as f32, y as f32)
    };
    Path::new(|builder| {
        for segment in segments {
            match *segment {
                Segment::MoveTo(p) => builder.move_to(point(p)),
                Segment::LineTo(p) => builder.line_to(point(p)),
                Segment::CubicTo(c1, c2, p) => {
                    builder.bezier_curve_to(point(c1), point(c2), point(p))
                }
            }
        }
    })
}

/// Top-left of everything drawn, minus [`PADDING`], so a scene opens at 100%
/// zoom aligned like Excalidraw's SVG export.
// ponytail: uses the rough outline, off by the wobble (~2px) from Excalidraw's element bounds
fn content_origin(drawings: &[Drawing]) -> [f64; 2] {
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
