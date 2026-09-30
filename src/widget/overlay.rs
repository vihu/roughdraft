//! Selection borders, transform handles and the box-select rectangle.
use iced::Color;
use iced::widget::canvas::{Frame, LineDash, Path, Stroke, Style};

use super::{Appearance, SELECTION_PADDING, Sketch, paint};
use crate::color::Rgba;
use crate::edit;
use crate::geometry::{self, Affine};
use crate::render::Segment;

impl Sketch {
    /// Corner handles as white squares, the rotation knob as a circle, and
    /// line endpoints as circles, all at a fixed screen size.
    fn draw_handles(&self, frame: &mut Frame, handles: &edit::Handles, view: Affine, color: Color) {
        let white = self.paint(Rgba::WHITE);
        let stroke = Stroke {
            style: Style::Solid(color),
            width: 1.0,
            ..Stroke::default()
        };
        let half = edit::HANDLE_SIZE / 2.0;
        let rotation = Affine::rotate_about(handles.angle, [0.0, 0.0]);
        for (handle, center) in &handles.handles {
            let [cx, cy] = view.apply(*center);
            let path = if *handle == edit::Handle::Rotation {
                Path::circle(iced::Point::new(cx as f32, cy as f32), half as f32)
            } else {
                let corners = [[-half, -half], [half, -half], [half, half], [-half, half]];
                polygon(&corners, rotation.then(Affine::translate([cx, cy])))
            };
            frame.fill(&path, white);
            frame.stroke(&path, stroke);
        }
        let point_stroke = self.paint(Rgba::rgb(
            0x5e as f32 / 255.0,
            0x5a as f32 / 255.0,
            0xd8 as f32 / 255.0,
        ));
        for point in &handles.points {
            let [x, y] = view.apply(*point);
            let circle = Path::circle(
                iced::Point::new(x as f32, y as f32),
                edit::POINT_RADIUS as f32,
            );
            frame.fill(&circle, Color { a: 0.9, ..white });
            frame.stroke(
                &circle,
                Stroke {
                    style: Style::Solid(point_stroke),
                    ..stroke
                },
            );
        }
    }

    pub(super) fn draw_overlay(&self, frame: &mut Frame, zoom: f64, view: Affine) {
        let (selection, dark_selection) = (
            Rgba::rgb(0.412, 0.396, 0.859),
            Rgba::rgb(0.208, 0.188, 0.769),
        );
        let color = self.paint(match self.appearance {
            Appearance::Light => selection,
            Appearance::Dark => dark_selection,
        });
        let line = |dash: &'static [f32]| Stroke {
            style: Style::Solid(color),
            width: 1.0,
            line_dash: LineDash {
                segments: dash,
                offset: 0,
            },
            ..Stroke::default()
        };
        let pad = SELECTION_PADDING / zoom;

        // No selection chrome while typing, like Excalidraw.
        let typing = self.editor.editing().is_some();
        let selected: Vec<_> = self.editor.selection().filter(|_| !typing).collect();
        let handles = self.editor.handles();
        // A lone 2-point line shows only its endpoint handles, no border.
        let bordered = !handles.as_ref().is_some_and(|h| !h.points.is_empty());
        // Selected groups get one dashed box instead of per-element borders.
        for (_, [x1, y1, x2, y2]) in self.editor.selected_groups() {
            let corners = [
                [x1 - pad, y1 - pad],
                [x2 + pad, y1 - pad],
                [x2 + pad, y2 + pad],
                [x1 - pad, y2 + pad],
            ];
            let group_line = Stroke {
                style: Style::Solid(self.paint(Rgba::BLACK)),
                width: 1.0,
                line_dash: LineDash {
                    segments: &[4.0, 4.0],
                    offset: 0,
                },
                ..Stroke::default()
            };
            frame.stroke(&polygon(&corners, view), group_line);
        }
        let own_border =
            |e: &&&crate::scene::Element| bordered && !self.editor.in_selected_group(&e.base.id);
        for element in selected.iter().filter(own_border) {
            let [x1, y1, x2, y2] = geometry::local_bounds(element);
            let corners = [
                [x1 - pad, y1 - pad],
                [x2 + pad, y1 - pad],
                [x2 + pad, y2 + pad],
                [x1 - pad, y2 + pad],
            ];
            let transform = geometry::element_transform(element).then(view);
            frame.stroke(&polygon(&corners, transform), line(&[]));
        }
        if selected.len() > 1 {
            let [x1, y1, x2, y2] = edit::common_bounds(selected.into_iter());
            let corners = [
                [x1 - pad, y1 - pad],
                [x2 + pad, y1 - pad],
                [x2 + pad, y2 + pad],
                [x1 - pad, y2 + pad],
            ];
            frame.stroke(&polygon(&corners, view), line(&[2.0, 2.0]));
        }
        if let Some(handles) = &handles {
            self.draw_handles(frame, handles, view, color);
        }
        if let Some([x1, y1, x2, y2]) = self.editor.marquee() {
            let marquee = polygon(&[[x1, y1], [x2, y1], [x2, y2], [x1, y2]], view);
            frame.fill(
                &marquee,
                self.paint(Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 200.0 / 255.0,
                    a: 0.04,
                }),
            );
            frame.stroke(&marquee, line(&[]));
        }
    }
}

fn polygon(corners: &[geometry::Point], transform: Affine) -> Path {
    let segments: Vec<Segment> = corners
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                Segment::MoveTo(*p)
            } else {
                Segment::LineTo(*p)
            }
        })
        .chain(std::iter::once(Segment::LineTo(corners[0])))
        .collect();
    paint::to_path(&segments, transform)
}
