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

/// One style property, set from the style panel.
#[derive(Clone, Debug, PartialEq)]
pub enum StyleChange {
    /// Outline color.
    StrokeColor(String),
    /// Fill color, or `transparent`.
    BackgroundColor(String),
    /// Fill pattern.
    FillStyle(FillStyle),
    /// Outline width.
    StrokeWidth(f64),
    /// Outline dash pattern.
    StrokeStyle(StrokeStyle),
    /// Sloppiness; also draws a new seed, like Excalidraw.
    Roughness(f64),
    /// Opacity from 0 to 100.
    Opacity(f64),
    /// Round (true) or sharp edges.
    RoundEdges(bool),
    /// Arrowhead at an arrow's first point.
    StartArrowhead(Option<Arrowhead>),
    /// Arrowhead at an arrow's last point.
    EndArrowhead(Option<Arrowhead>),
    /// Font size.
    FontSize(f64),
    /// Excalidraw font id.
    FontFamily(u32),
    /// Horizontal text alignment.
    TextAlign(TextAlign),
}

impl Style {
    /// Applies a change to the style for new elements.
    pub(crate) fn apply(&mut self, change: &StyleChange) {
        match change.clone() {
            StyleChange::StrokeColor(color) => self.stroke_color = color,
            StyleChange::BackgroundColor(color) => self.background_color = color,
            StyleChange::FillStyle(fill) => self.fill_style = fill,
            StyleChange::StrokeWidth(width) => self.stroke_width = width,
            StyleChange::StrokeStyle(style) => self.stroke_style = style,
            StyleChange::Roughness(roughness) => self.roughness = roughness,
            StyleChange::Opacity(opacity) => self.opacity = opacity,
            StyleChange::RoundEdges(round) => self.round_edges = round,
            StyleChange::StartArrowhead(head) => self.start_arrowhead = head,
            StyleChange::EndArrowhead(head) => self.end_arrowhead = head,
            StyleChange::FontSize(size) => self.font_size = size,
            StyleChange::FontFamily(family) => self.font_family = family,
            StyleChange::TextAlign(align) => self.text_align = align,
        }
    }

    /// Returns this style with the first selected element's values, so the
    /// panel shows what is selected.
    pub(crate) fn of(&self, element: &crate::scene::Element) -> Self {
        use crate::scene::Kind;
        let base = &element.base;
        let mut style = Self {
            stroke_color: base.stroke_color.clone(),
            background_color: base.background_color.clone(),
            fill_style: base.fill_style.clone(),
            stroke_width: base.stroke_width,
            stroke_style: base.stroke_style.clone(),
            roughness: base.roughness,
            opacity: base.opacity,
            round_edges: base.roundness.is_some(),
            ..self.clone()
        };
        match &element.kind {
            Kind::Arrow(line) => {
                style.round_arrows = base.roundness.is_some();
                style.start_arrowhead = line.start_arrowhead.clone();
                style.end_arrowhead = line.end_arrowhead.clone();
            }
            Kind::Text(text) => {
                style.font_size = text.font_size;
                style.font_family = text.font_family;
                style.text_align = text.text_align.clone();
            }
            _ => {}
        }
        style
    }
}
