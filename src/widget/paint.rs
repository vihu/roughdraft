//! Draw commands to iced canvas geometry.
//!
//! Geometry is mapped to screen space here, before it reaches iced, so stroke
//! widths and dashes scale with zoom the same way on the wgpu and tiny-skia
//! backends (they disagree on how frame transforms affect strokes).
use iced::widget::canvas::{
    self, Fill, Frame, LineCap, LineDash, LineJoin, Path, Stroke, Style, fill,
};
use iced::widget::text::{Alignment, LineHeight, Shaping};
use iced::{Color, Font, Pixels, Point, Vector};

use crate::color::Rgba;
use crate::geometry::Affine;
use crate::render::{Align, FillRule, Item, Segment, TextBlock};

pub(super) fn draw_item(
    frame: &mut Frame,
    item: &Item,
    transform: Affine,
    paint: &impl Fn(Rgba) -> Color,
) {
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
        // Drawn by `Sketch::draw_ids`, which holds the decoded images.
        Item::Image { .. } => {}
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
pub(super) fn font(family: u32) -> Font {
    match family {
        3 => Font::MONOSPACE,
        2 | 6 | 9 => Font::DEFAULT,
        _ => Font::new("Excalifont"),
    }
}

pub(super) fn to_path(segments: &[Segment], transform: Affine) -> Path {
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
