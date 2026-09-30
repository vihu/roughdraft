//! Tool bar and style panel icons as canvas outlines in a 20 px box, after
//! Excalidraw's, so no SVG renderer is needed.
use iced::widget::canvas::{self, Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};

use super::Message;
use crate::edit::Tool;
use crate::scene::{FillStyle, StrokeStyle, TextAlign};

/// Icon box in pixels.
const SIZE: f32 = 20.0;

/// What an icon shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Glyph {
    Tool(Tool),
    Lock {
        locked: bool,
    },
    Fill(FillStyle),
    /// Stroke width, 1 to 4.
    Width(u8),
    Dash(StrokeStyle),
    /// Sloppiness, 0 to 2.
    Rough(u8),
    Edges {
        round: bool,
    },
    Align(TextAlign),
    /// The main menu button: three lines.
    Menu,
    /// Sharp or curved arrow type.
    ArrowType {
        round: bool,
    },
}

/// An icon drawn in `color`.
pub(super) fn icon<'a>(glyph: Glyph, color: Color) -> Element<'a, Message> {
    canvas::Canvas::new(Icon { glyph, color })
        .width(Length::Fixed(SIZE))
        .height(Length::Fixed(SIZE))
        .into()
}

struct Icon {
    glyph: Glyph,
    color: Color,
}

impl canvas::Program<Message> for Icon {
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
        let stroke = Stroke::default()
            .with_color(self.color)
            .with_width(1.5)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let polyline = |points: &[(f32, f32)], closed: bool| {
            Path::new(|path| {
                path.move_to(Point::new(points[0].0, points[0].1));
                for &(x, y) in &points[1..] {
                    path.line_to(Point::new(x, y));
                }
                if closed {
                    path.close();
                }
            })
        };
        let rounded = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            Path::rounded_rectangle(Point::new(x, y), Size::new(w, h), r.into())
        };
        let paths: Vec<Path> = match self.glyph.clone() {
            Glyph::Tool(Tool::Selection) => vec![polyline(
                &[
                    (5.0, 3.0),
                    (5.0, 16.0),
                    (8.5, 12.5),
                    (11.0, 17.5),
                    (13.0, 16.5),
                    (10.5, 11.5),
                    (15.0, 11.0),
                ],
                true,
            )],
            Glyph::Tool(Tool::Hand) => vec![
                polyline(&[(7.0, 11.0), (7.0, 5.0)], false),
                polyline(&[(9.5, 10.0), (9.5, 3.5)], false),
                polyline(&[(12.0, 10.0), (12.0, 4.5)], false),
                polyline(&[(14.5, 11.0), (14.5, 6.5)], false),
                polyline(
                    &[
                        (7.0, 11.0),
                        (4.5, 9.0),
                        (4.0, 11.5),
                        (7.5, 16.5),
                        (13.0, 17.5),
                        (14.5, 15.0),
                        (14.5, 11.0),
                    ],
                    false,
                ),
            ],
            Glyph::Tool(Tool::Rectangle) => vec![rounded(3.0, 5.0, 14.0, 10.0, 2.0)],
            Glyph::Tool(Tool::Diamond) => vec![polyline(
                &[(10.0, 3.0), (17.0, 10.0), (10.0, 17.0), (3.0, 10.0)],
                true,
            )],
            Glyph::Tool(Tool::Ellipse) => vec![Path::circle(Point::new(10.0, 10.0), 7.0)],
            Glyph::Tool(Tool::Arrow) => vec![
                polyline(&[(4.0, 16.0), (16.0, 4.0)], false),
                polyline(&[(10.0, 4.0), (16.0, 4.0), (16.0, 10.0)], false),
            ],
            Glyph::Tool(Tool::Line) => vec![polyline(&[(3.5, 10.0), (16.5, 10.0)], false)],
            Glyph::Tool(Tool::Eraser) => vec![
                polyline(
                    &[
                        (8.0, 17.0),
                        (3.5, 12.5),
                        (11.5, 4.5),
                        (17.0, 10.0),
                        (10.0, 17.0),
                    ],
                    true,
                ),
                polyline(&[(7.0, 9.0), (12.5, 14.5)], false),
                polyline(&[(10.0, 17.0), (17.0, 17.0)], false),
            ],
            // Excalidraw's frame: a box whose sides run past the corners.
            Glyph::Tool(Tool::Frame) => vec![
                polyline(&[(3.0, 6.5), (17.0, 6.5)], false),
                polyline(&[(3.0, 13.5), (17.0, 13.5)], false),
                polyline(&[(6.5, 3.0), (6.5, 17.0)], false),
                polyline(&[(13.5, 3.0), (13.5, 17.0)], false),
            ],
            Glyph::Tool(Tool::Text) => vec![
                polyline(&[(5.0, 16.0), (10.0, 4.0), (15.0, 16.0)], false),
                polyline(&[(7.0, 12.0), (13.0, 12.0)], false),
            ],
            Glyph::Fill(fill) => {
                let (lo, hi) = (3.5, 16.5);
                let square = rounded(lo, lo, hi - lo, hi - lo, 2.0);
                if fill == FillStyle::Solid {
                    frame.fill(&square, self.color);
                }
                let mut paths = vec![square];
                // Diagonals clipped to the square: x + y = c, and x - y = c
                // for cross-hatch.
                let rising = |c: f32| {
                    let (a, b) = if c <= lo + hi {
                        ((lo, c - lo), (c - lo, lo))
                    } else {
                        ((c - hi, hi), (hi, c - hi))
                    };
                    polyline(&[a, b], false)
                };
                let falling = |c: f32| {
                    let (a, b) = if c >= 0.0 {
                        ((lo + c, lo), (hi, hi - c))
                    } else {
                        ((lo, lo - c), (hi + c, hi))
                    };
                    polyline(&[a, b], false)
                };
                if matches!(fill, FillStyle::Hachure | FillStyle::CrossHatch) {
                    paths.extend([12.0, 16.5, 20.0, 24.5, 29.0].map(rising));
                }
                if fill == FillStyle::CrossHatch {
                    paths.extend([-8.5, -4.0, 0.0, 4.0, 8.5].map(falling));
                }
                paths
            }
            Glyph::Width(width) => {
                let line = polyline(&[(3.5, 10.0), (16.5, 10.0)], false);
                frame.stroke(&line, stroke.with_width(f32::from(width)));
                Vec::new()
            }
            Glyph::Dash(style) => {
                let dash: &[f32] = match style {
                    StrokeStyle::Dashed => &[4.0, 3.0],
                    StrokeStyle::Dotted => &[0.5, 3.0],
                    _ => &[],
                };
                let line = polyline(&[(3.5, 10.0), (16.5, 10.0)], false);
                let dashed = Stroke {
                    line_dash: canvas::LineDash {
                        segments: dash,
                        offset: 0,
                    },
                    ..stroke.with_width(2.0)
                };
                frame.stroke(&line, dashed);
                Vec::new()
            }
            Glyph::Rough(level) => {
                // A line that wobbles more with each level.
                let amplitude = f32::from(level) * 1.6;
                let points: Vec<(f32, f32)> = (0..=12)
                    .map(|i| {
                        let x = 3.5 + 13.0 * i as f32 / 12.0;
                        (x, 10.0 + amplitude * (i as f32 * 1.3).sin())
                    })
                    .collect();
                vec![polyline(&points, false)]
            }
            Glyph::Edges { round } => vec![if round {
                Path::new(|path| {
                    path.move_to(Point::new(4.0, 16.0));
                    path.line_to(Point::new(4.0, 10.0));
                    path.arc_to(Point::new(4.0, 4.0), Point::new(10.0, 4.0), 6.0);
                    path.line_to(Point::new(16.0, 4.0));
                })
            } else {
                polyline(&[(4.0, 16.0), (4.0, 4.0), (16.0, 4.0)], false)
            }],
            Glyph::ArrowType { round } => vec![if round {
                Path::new(|path| {
                    path.move_to(Point::new(4.0, 16.0));
                    path.quadratic_curve_to(Point::new(5.0, 5.0), Point::new(16.0, 4.0));
                })
            } else {
                polyline(&[(4.0, 16.0), (9.0, 6.0), (16.0, 4.0)], false)
            }],
            Glyph::Menu => [5.0, 10.0, 15.0]
                .into_iter()
                .map(|y| polyline(&[(4.0, y), (16.0, y)], false))
                .collect(),
            Glyph::Align(align) => [(5.0, 12.0), (10.0, 8.0), (15.0, 12.0)]
                .into_iter()
                .map(|(y, length): (f32, f32)| {
                    let x = match align {
                        TextAlign::Center => 10.0 - length / 2.0,
                        TextAlign::Right => 16.0 - length,
                        _ => 4.0,
                    };
                    polyline(&[(x, y), (x + length, y)], false)
                })
                .collect(),
            Glyph::Lock { locked } => {
                // The shackle closes into the body when locked.
                let right = if locked { 9.0 } else { 6.5 };
                let shackle = Path::new(|path| {
                    path.move_to(Point::new(7.0, 9.0));
                    path.line_to(Point::new(7.0, 6.5));
                    path.arc(canvas::path::Arc {
                        center: Point::new(10.0, 6.5),
                        radius: 3.0,
                        start_angle: iced::Radians(std::f32::consts::PI),
                        end_angle: iced::Radians(2.0 * std::f32::consts::PI),
                    });
                    path.line_to(Point::new(13.0, right));
                });
                vec![rounded(5.0, 9.0, 10.0, 8.0, 1.5), shackle]
            }
        };
        for path in paths {
            frame.stroke(&path, stroke);
        }
        vec![frame.into_geometry()]
    }
}
