//! Style for new elements, like Excalidraw's `currentItem*` app state
//! (REFERENCE-001 section 1).
use crate::geometry::Point;
use crate::scene::{Arrowhead, Base, FillStyle, StrokeStyle, TextAlign};

/// Style applied to newly created elements.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Outline color.
    pub stroke_color: String,
    /// Fill color, or `transparent`.
    pub background_color: String,
    /// Fill pattern.
    pub fill_style: FillStyle,
    /// Outline width: 1, 2 or 4 in Excalidraw's UI.
    pub stroke_width: f64,
    /// Outline dash pattern.
    pub stroke_style: StrokeStyle,
    /// Sloppiness: 0, 1 or 2.
    pub roughness: f64,
    /// Opacity from 0 to 100.
    pub opacity: f64,
    /// Round edges: adaptive radius for rectangles, proportional for other
    /// shapes and lines.
    pub round_edges: bool,
    /// Curved arrows (Excalidraw's "round" arrow type).
    pub round_arrows: bool,
    /// Arrowhead at an arrow's first point.
    pub start_arrowhead: Option<Arrowhead>,
    /// Arrowhead at an arrow's last point.
    pub end_arrowhead: Option<Arrowhead>,
    /// Excalidraw font id.
    pub font_family: u32,
    /// Font size.
    pub font_size: f64,
    /// Horizontal text alignment.
    pub text_align: TextAlign,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill_style: FillStyle::Solid,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roughness: 1.0,
            opacity: 100.0,
            round_edges: true,
            round_arrows: true,
            start_arrowhead: None,
            end_arrowhead: Some(Arrowhead::Arrow),
            font_family: 5,
            font_size: 20.0,
            text_align: TextAlign::Left,
        }
    }
}

impl Style {
    /// Returns base fields for a new 0x0 element at `at`, with a fresh id and
    /// seed. Callers set roundness per element type.
    pub(crate) fn base(&self, [x, y]: Point) -> Base {
        Base {
            id: crate::random::id(),
            x,
            y,
            width: 0.0,
            height: 0.0,
            angle: 0.0,
            stroke_color: self.stroke_color.clone(),
            background_color: self.background_color.clone(),
            fill_style: self.fill_style.clone(),
            stroke_width: self.stroke_width,
            stroke_style: self.stroke_style.clone(),
            roundness: None,
            roughness: self.roughness,
            opacity: self.opacity,
            seed: crate::random::integer(),
            is_deleted: false,
            bound_elements: None,
        }
    }
}
